//! What happens when the user clicks: background loaders, backup and restore
//! tasks, the encrypted vault and the automatic-backup schedule.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui;

use super::tasks::{Job, Task, TaskKind, TaskOutput};
use super::widgets::NoticeKind;
use super::{
    AeternaApp, AfterUnlock, AppStatus, Done, Pending, Screen, TreeEntry, VaultDialog, View,
};
use crate::config::Source;
use crate::engine::crypto::passphrase_strength;
use crate::engine::plan::{
    self, BackupInput, BackupPlan, ItemKind, RestoreOptions, RestorePlan, RestoreTarget,
};
use crate::engine::restore;
use crate::engine::selection::Selection;
use crate::engine::snapshots::{self, SnapshotInfo};
use crate::engine::sources::Sources;
use crate::engine::{backup, paths_equal, scan, vault};
use crate::error::EngineError;
use crate::i18n::Lang;
use crate::platform::known_paths::KnownPaths;
use crate::platform::{self, scheduler, vss::LiveFiles};
use crate::state;

pub const MIN_PASSPHRASE_CHARS: usize = 10;

impl AeternaApp {
    fn key_dir(&self) -> PathBuf {
        state::key_dir(&self.paths.config_file)
    }

    // --- sources and sizes ---------------------------------------------------------

    pub fn add_source(&mut self, ctx: &egui::Context, source: Source) {
        if self.config.has_source_path(&source.path) {
            return;
        }
        tracing::info!("source added: {}", source.path.display());
        self.config.sources.push(source);
        self.mark_dirty();
        self.refresh_sizes(ctx);
    }

    pub fn add_folder(&mut self, ctx: &egui::Context, path: PathBuf) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        self.add_source(ctx, Source::new(name, path, true));
    }

    pub fn size_of(&self, path: &Path) -> Option<&Option<(u64, u64)>> {
        self.sizes
            .iter()
            .find(|(p, _)| paths_equal(p, path))
            .map(|(_, size)| size)
    }

    /// Forget the size of a source after its selection changed.
    pub fn invalidate_size(&mut self, ctx: &egui::Context, path: &Path) {
        self.sizes.retain(|p, _| !paths_equal(p, path));
        self.refresh_sizes(ctx);
    }

    pub fn refresh_sizes(&mut self, ctx: &egui::Context) {
        if let Some(job) = self.sizes_job.take() {
            job.cancel.cancel();
        }
        let pending: Vec<(PathBuf, Vec<String>, Vec<String>)> = self
            .config
            .sources
            .iter()
            .filter(|s| self.size_of(&s.path).and_then(|s| s.as_ref()).is_none())
            .map(|s| {
                (
                    s.path.clone(),
                    s.include_paths.clone(),
                    s.exclude_paths.clone(),
                )
            })
            .collect();
        if pending.is_empty() {
            return;
        }
        self.sizes_job = Some(Job::spawn(ctx, move |cancel, send| {
            for (path, include, exclude) in pending {
                if cancel.is_cancelled() {
                    return;
                }
                let size = scan::folder_size(&path, &Selection::new(&include, &exclude), cancel);
                send((path, size));
            }
        }));
    }

    /// Entries of a folder for the "choose contents" tree (loaded once, then cached).
    pub fn tree_listing(&mut self, dir: &Path) -> Vec<TreeEntry> {
        if let Some(entries) = self.tree.listings.get(dir) {
            return entries.clone();
        }
        let mut entries: Vec<TreeEntry> = std::fs::read_dir(dir)
            .map(|read| {
                read.flatten()
                    .filter_map(|entry| {
                        let file_type = entry.file_type().ok()?;
                        if file_type.is_symlink() {
                            return None;
                        }
                        Some(TreeEntry {
                            name: entry.file_name().to_string_lossy().into_owned(),
                            is_dir: file_type.is_dir(),
                            size: if file_type.is_file() {
                                entry.metadata().map(|m| m.len()).unwrap_or(0)
                            } else {
                                0
                            },
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        self.tree
            .listings
            .insert(dir.to_path_buf(), entries.clone());
        entries
    }

    // --- applications ----------------------------------------------------------------

    pub fn refresh_app_status(&mut self, ctx: &egui::Context) {
        if let Some(job) = self.apps.status_job.take() {
            job.cancel.cancel();
        }
        let catalog = self.catalog.clone();
        self.apps.status_job = Some(Job::spawn(ctx, move |cancel, send| {
            let known = KnownPaths::current();
            let mut detected = Vec::new();
            // First a quick pass so the list appears immediately …
            for app in &catalog.apps {
                let folders = app.present_folders(&known);
                let keys = app.present_registry_keys();
                let status = AppStatus {
                    detected: !folders.is_empty() || !keys.is_empty(),
                    folders: folders.len(),
                    registry_keys: keys.len(),
                    bytes: None,
                };
                if status.detected {
                    detected.push((app.clone(), status.clone()));
                }
                send((app.id.clone(), status));
            }
            // … then the sizes, which can take a while for large profiles.
            for (app, mut status) in detected {
                if cancel.is_cancelled() {
                    return;
                }
                let mut bytes = 0;
                for (folder, root) in app.present_folders(&known) {
                    let (include, exclude) = if folder.only.is_empty() {
                        (Vec::new(), Vec::new())
                    } else {
                        (folder.only.clone(), vec![".".to_string()])
                    };
                    if let Some((b, _)) =
                        scan::folder_size(&root, &Selection::new(&include, &exclude), cancel)
                    {
                        bytes += b;
                    }
                }
                status.bytes = Some(bytes);
                send((app.id.clone(), status));
            }
        }));
    }

    pub fn app_detected(&self, id: &str) -> bool {
        self.apps.status.get(id).is_some_and(|s| s.detected)
    }

    /// Display names of chosen applications that are running right now.
    pub fn running_chosen_apps(&self, lang: Lang) -> Vec<String> {
        self.catalog
            .apps
            .iter()
            .filter(|a| self.config.app_enabled(&a.id) && a.is_running(&self.running))
            .map(|a| a.display_name(lang).to_string())
            .collect()
    }

    pub fn refresh_running_processes(&mut self, ctx: &egui::Context) {
        let relevant = matches!(self.screen, Screen::Main)
            && matches!(self.view, View::Backup | View::Apps | View::Restore);
        if !relevant {
            return;
        }
        if self
            .running_checked
            .is_none_or(|t| t.elapsed() > Duration::from_secs(4))
        {
            self.running = platform::running_processes();
            self.running_checked = Some(Instant::now());
        }
        ctx.request_repaint_after(Duration::from_secs(5));
    }

    pub fn collect_sources(&self) -> Sources {
        Sources::collect(
            &self.config,
            &self.catalog,
            &KnownPaths::current(),
            self.lang,
        )
    }

    // --- snapshots ----------------------------------------------------------------------

    pub fn refresh_snapshots(&mut self, ctx: &egui::Context) {
        let destination = self.config.destination.clone();
        if destination.as_os_str().is_empty() {
            self.snapshots = Some(Err(EngineError::NoDestination));
            return;
        }
        let key = self.vault.key.clone();
        self.snapshots_job = Some(Job::spawn(ctx, move |_, send| {
            let list = snapshots::list(&destination, key.as_ref());
            let free = platform::free_space(&destination);
            send((list, free));
        }));
    }

    pub fn last_backup(&self) -> Option<&SnapshotInfo> {
        match &self.snapshots {
            Some(Ok(list)) => list
                .iter()
                .find(|s| s.computer.eq_ignore_ascii_case(&self.computer) || s.is_locked()),
            _ => None,
        }
    }

    pub fn destination_reachable(&self) -> Option<bool> {
        match &self.snapshots {
            Some(Err(EngineError::DestinationUnavailable(_))) => Some(false),
            Some(_) => Some(true),
            None => None,
        }
    }

    pub fn selected_snapshot(&self) -> Option<&SnapshotInfo> {
        let selected = self.restore.selected.as_ref()?;
        match &self.snapshots {
            Some(Ok(list)) => list.iter().find(|s| &s.qualified_id() == selected),
            _ => None,
        }
    }

    pub fn destination_changed(&mut self, ctx: &egui::Context) {
        self.vault.key = None;
        self.refresh_vault();
        self.refresh_snapshots(ctx);
    }

    pub(super) fn poll_jobs(&mut self, ctx: &egui::Context) {
        if let Some(job) = &self.sizes_job {
            let (values, finished) = job.drain();
            for (path, size) in values {
                self.sizes.insert(path, size);
            }
            if finished {
                self.sizes_job = None;
            }
        }

        if let Some(job) = &self.snapshots_job {
            let (mut values, finished) = job.drain();
            if let Some((list, free)) = values.pop() {
                if let Ok(list) = &list {
                    let still_there = self
                        .restore
                        .selected
                        .as_ref()
                        .is_some_and(|sel| list.iter().any(|s| &s.qualified_id() == sel));
                    if !still_there {
                        self.restore.skip.clear();
                        self.restore.selected = list
                            .iter()
                            .find(|s| {
                                s.header.is_some()
                                    && s.computer.eq_ignore_ascii_case(&self.computer)
                            })
                            .or_else(|| list.first())
                            .map(|s| s.qualified_id());
                    }
                }
                self.snapshots = Some(list);
                self.destination_free = free;
            }
            if finished {
                self.snapshots_job = None;
            }
        }

        if let Some(job) = &self.apps.status_job {
            let (values, finished) = job.drain();
            for (id, status) in values {
                self.apps.status.insert(id, status);
            }
            if finished {
                self.apps.status_job = None;
            }
        }

        if let Some(job) = &self.apps.installed_job {
            let (mut values, finished) = job.drain();
            if let Some(list) = values.pop() {
                self.apps.installed = Some(list);
            }
            if finished {
                self.apps.installed_job = None;
            }
        }

        if let Some(job) = &self.schedule.job {
            let (mut values, finished) = job.drain();
            if let Some((schedule, result)) = values.pop() {
                match result {
                    Ok(()) => {
                        self.state.task_executable = schedule
                            .enabled
                            .then(|| std::env::current_exe().ok())
                            .flatten();
                        self.state.save(&self.paths.config_file);
                        self.schedule.applied = Some(schedule);
                    }
                    Err(message) => {
                        tracing::error!("schedule could not be applied: {message}");
                        self.notify(NoticeKind::Error, self.lang.schedule_error(&message));
                        if schedule.enabled {
                            self.config.schedule.enabled = false;
                            self.mark_dirty();
                        }
                        self.schedule.applied = Some(self.config.schedule.clone());
                    }
                }
            }
            if finished {
                self.schedule.job = None;
            }
        }

        if let Some(task) = &mut self.task
            && let Some(result) = task.poll()
        {
            let kind = task.kind;
            self.task = None;
            self.finish_task(ctx, kind, result);
        }
    }

    // --- backup and restore ---------------------------------------------------------------

    pub fn plan_backup(&mut self, ctx: &egui::Context, confirm: bool) {
        if self.is_busy() {
            return;
        }
        if self.config.encryption.enabled && self.vault.key.is_none() {
            self.refresh_vault();
            if self.vault.header.is_none() {
                self.notify(
                    NoticeKind::Warning,
                    self.lang.error_message(&EngineError::EncryptionNotSetUp),
                );
                return;
            }
            if self.vault.key.is_none() {
                self.open_unlock(AfterUnlock::Backup { confirm });
                return;
            }
        }
        let config = self.config.clone();
        let sources = self.collect_sources();
        let computer = self.computer.clone();
        let key = self.vault.key.clone();
        let running = self.running_chosen_apps(self.lang);
        self.task = Some(Task::spawn(
            ctx,
            TaskKind::PlanBackup { confirm },
            move |cancel, progress| {
                let input = BackupInput {
                    config: &config,
                    sources: &sources,
                    computer: &computer,
                    key: key.as_ref(),
                    running_apps: running,
                };
                TaskOutput::BackupPlan(plan::plan_backup(&input, cancel, progress))
            },
        ));
        self.screen = Screen::Working;
    }

    pub fn plan_restore(&mut self, ctx: &egui::Context, confirm: bool) {
        if self.is_busy() {
            return;
        }
        let Some(snapshot) = self.selected_snapshot().cloned() else {
            return;
        };
        if snapshot.is_locked() {
            self.open_unlock(AfterUnlock::RefreshSnapshots);
            return;
        }
        let target = if self.restore.to_folder {
            match &self.restore.folder {
                Some(folder) => RestoreTarget::Folder(folder.clone()),
                None => {
                    self.notify(NoticeKind::Warning, self.lang.t().choose_folder_first);
                    return;
                }
            }
        } else {
            RestoreTarget::Original
        };
        let options = RestoreOptions {
            target,
            conflict: self.config.advanced.restore_conflict,
            verify: self.config.advanced.verify_on_restore,
            skip: self.restore.skip.clone(),
        };
        let key = self.vault.key.clone();
        // Applications in this backup that are open right now.
        let running: Vec<String> = snapshot
            .header
            .as_ref()
            .map(|h| {
                let mut ids: Vec<&str> =
                    h.sources.iter().filter_map(|s| s.app.as_deref()).collect();
                ids.extend(h.registry.iter().map(|r| r.app.as_str()));
                ids.sort_unstable();
                ids.dedup();
                ids.into_iter()
                    .filter(|id| !self.restore.skip.contains(&format!("app:{id}")))
                    .filter_map(|id| self.catalog.get(id))
                    .filter(|a| a.is_running(&self.running))
                    .map(|a| a.display_name(self.lang).to_string())
                    .collect()
            })
            .unwrap_or_default();
        self.task = Some(Task::spawn(
            ctx,
            TaskKind::PlanRestore { confirm },
            move |cancel, progress| {
                TaskOutput::RestorePlan(plan::plan_restore(
                    &snapshot,
                    options,
                    key.as_ref(),
                    running,
                    cancel,
                    progress,
                ))
            },
        ));
        self.screen = Screen::Working;
    }

    pub fn start_backup(&mut self, ctx: &egui::Context, plan: Box<BackupPlan>) {
        if self.is_busy() {
            return;
        }
        let key = self.vault.key.clone();
        self.task = Some(Task::spawn(
            ctx,
            TaskKind::Backup,
            move |cancel, progress| {
                TaskOutput::Backup(backup::run_backup(
                    &plan,
                    key.as_ref(),
                    &LiveFiles,
                    cancel,
                    progress,
                ))
            },
        ));
        self.screen = Screen::Working;
    }

    pub fn start_restore(&mut self, ctx: &egui::Context, plan: Box<RestorePlan>) {
        if self.is_busy() {
            return;
        }
        let key = self.vault.key.clone();
        self.task = Some(Task::spawn(
            ctx,
            TaskKind::Restore,
            move |cancel, progress| {
                TaskOutput::Restore(restore::run_restore(&plan, key.as_ref(), cancel, progress))
            },
        ));
        self.screen = Screen::Working;
    }

    fn finish_task(
        &mut self,
        ctx: &egui::Context,
        kind: TaskKind,
        result: Result<TaskOutput, String>,
    ) {
        let output = match result {
            Ok(output) => output,
            Err(panic) => {
                tracing::error!("background task failed: {panic}");
                self.notify(NoticeKind::Error, self.lang.t().unexpected_problem);
                self.screen = Screen::Main;
                return;
            }
        };

        match output {
            TaskOutput::BackupPlan(Ok(plan)) => {
                let confirm = matches!(kind, TaskKind::PlanBackup { confirm: true });
                if !confirm {
                    self.screen = Screen::Preview(Box::new(
                        super::views::preview::PreviewState::backup(plan),
                    ));
                } else if self.config.advanced.confirm_before_start || !plan.running_apps.is_empty()
                {
                    self.pending = Some(Pending::Backup(Box::new(plan)));
                    self.screen = Screen::Main;
                } else {
                    self.start_backup(ctx, Box::new(plan));
                }
            }
            TaskOutput::RestorePlan(Ok(plan)) => {
                let confirm = matches!(kind, TaskKind::PlanRestore { confirm: true });
                let replaces = plan.summary.count(ItemKind::Changed) > 0;
                if !confirm {
                    self.screen = Screen::Preview(Box::new(
                        super::views::preview::PreviewState::restore(plan),
                    ));
                } else if self.config.advanced.confirm_before_start
                    || replaces
                    || !plan.running_apps.is_empty()
                {
                    // Overwriting files always asks, regardless of the setting.
                    self.pending = Some(Pending::Restore(Box::new(plan)));
                    self.screen = Screen::Main;
                } else {
                    self.start_restore(ctx, Box::new(plan));
                }
            }
            TaskOutput::BackupPlan(Err(err)) | TaskOutput::RestorePlan(Err(err)) => {
                self.screen = Screen::Main;
                match err {
                    EngineError::Cancelled => {}
                    EngineError::Locked => self.open_unlock(AfterUnlock::RefreshSnapshots),
                    err => {
                        tracing::warn!("planning failed: {err}");
                        self.notify(NoticeKind::Warning, self.lang.error_message(&err));
                    }
                }
            }
            TaskOutput::Backup(result) => {
                let result = result.map_err(|e| {
                    tracing::error!("backup failed: {e}");
                    self.lang.error_message(&e)
                });
                self.screen = Screen::Done(Box::new(Done::Backup(result)));
                self.refresh_snapshots(ctx);
            }
            TaskOutput::Restore(result) => {
                let result = result.map_err(|e| {
                    tracing::error!("restore failed: {e}");
                    self.lang.error_message(&e)
                });
                self.screen = Screen::Done(Box::new(Done::Restore(result)));
                self.sizes.clear();
                self.tree.listings.clear();
                self.refresh_sizes(ctx);
                self.refresh_app_status(ctx);
            }
        }
    }

    pub fn reload_config(&mut self, ctx: &egui::Context) {
        let loaded = crate::config::load_or_create(&self.paths);
        self.config = loaded.config;
        self.lang = Lang::resolve(self.config.language);
        self.settings.exclude_text = self.config.exclude.join("\n");
        let config_dir = self
            .paths
            .config_file
            .parent()
            .map(PathBuf::from)
            .unwrap_or_default();
        self.catalog = platform::apps::Catalog::load(&config_dir);
        super::theme::install_fonts(ctx, &self.config.fonts);
        super::theme::apply(ctx, self.config.appearance);
        self.sizes.clear();
        self.tree.listings.clear();
        self.destination_changed(ctx);
        self.refresh_sizes(ctx);
        self.refresh_app_status(ctx);
        match loaded.notice {
            Some(notice) => self.config_notice(notice),
            None => self.notify(NoticeKind::Success, self.lang.t().config_reloaded),
        }
    }

    // --- encrypted vault ---------------------------------------------------------------

    /// Reads the vault header at the destination and a remembered key, if any.
    pub fn refresh_vault(&mut self) {
        let header = vault::read_header(&self.config.destination).ok();
        let same_vault = match (&self.vault.header, &header) {
            (Some(a), Some(b)) => a.vault_id == b.vault_id,
            _ => false,
        };
        if !same_vault {
            self.vault.key = None;
        }
        if let Some(header) = &header {
            if self.vault.key.is_none() {
                self.vault.key = vault::remembered_key(&self.key_dir(), header);
            }
            self.vault.remembered = vault::is_remembered(&self.key_dir(), header);
        } else {
            self.vault.remembered = false;
        }
        self.vault.header = header;
    }

    pub fn open_unlock(&mut self, then: AfterUnlock) {
        self.vault.dialog = Some(VaultDialog::Unlock {
            secret: String::new(),
            remember: then == AfterUnlock::Remember || self.config.schedule.enabled,
            error: None,
            then,
        });
    }

    /// Turns encryption on or off from the Backup view.
    pub fn set_encryption(&mut self, enabled: bool) {
        if !enabled {
            self.config.encryption.enabled = false;
            self.mark_dirty();
            self.notify(NoticeKind::Info, self.lang.t().encryption_disabled_note);
            return;
        }
        self.refresh_vault();
        match (&self.vault.header, &self.vault.key) {
            (Some(_), Some(_)) => {
                self.config.encryption.enabled = true;
                self.mark_dirty();
            }
            (Some(_), None) => self.open_unlock(AfterUnlock::EnableEncryption),
            (None, _) => {
                self.vault.dialog = Some(VaultDialog::Create {
                    passphrase: String::new(),
                    repeat: String::new(),
                    remember: true,
                    error: None,
                });
            }
        }
    }

    /// Validates a new passphrase; returns a message for the user if unsuitable.
    pub fn passphrase_problem(&self, passphrase: &str, repeat: &str) -> Option<String> {
        let t = self.lang.t();
        if passphrase.chars().count() < MIN_PASSPHRASE_CHARS || passphrase_strength(passphrase) == 0
        {
            Some(t.passphrase_too_weak.to_string())
        } else if passphrase != repeat {
            Some(t.passphrases_differ.to_string())
        } else {
            None
        }
    }

    /// Creates the vault; returns the recovery key to show.
    pub fn create_vault(&mut self, passphrase: &str, remember: bool) -> Result<String, String> {
        let destination = self.config.destination.clone();
        if destination.as_os_str().is_empty() {
            return Err(self.lang.error_message(&EngineError::NoDestination));
        }
        let created = vault::create(&destination, passphrase)
            .map_err(|e| self.lang.error_message(&e.into()))?;
        if remember
            && let Err(err) = vault::remember_key(&self.key_dir(), &created.header, &created.key)
        {
            tracing::warn!("vault key could not be remembered: {err}");
        }
        tracing::info!("encrypted vault created at {}", destination.display());
        self.vault.header = Some(created.header);
        self.vault.key = Some(created.key);
        self.refresh_vault();
        self.config.encryption.enabled = true;
        self.mark_dirty();
        Ok(created.recovery_key)
    }

    pub fn unlock_vault(
        &mut self,
        ctx: &egui::Context,
        secret: &str,
        remember: bool,
        then: AfterUnlock,
    ) -> Result<(), String> {
        let (header, key) = vault::unlock(&self.config.destination, secret).map_err(|e| {
            if matches!(e, crate::engine::crypto::CryptoError::WrongKey) {
                self.lang.t().wrong_passphrase.to_string()
            } else {
                self.lang.error_message(&e.into())
            }
        })?;
        if remember && let Err(err) = vault::remember_key(&self.key_dir(), &header, &key) {
            tracing::warn!("vault key could not be remembered: {err}");
        }
        self.vault.header = Some(header);
        self.vault.key = Some(key);
        self.refresh_vault();
        self.refresh_snapshots(ctx);

        match then {
            AfterUnlock::Backup { confirm } => self.plan_backup(ctx, confirm),
            AfterUnlock::EnableEncryption => {
                self.config.encryption.enabled = true;
                self.mark_dirty();
            }
            AfterUnlock::ChangePassphrase => {
                self.vault.dialog = Some(VaultDialog::Change {
                    passphrase: String::new(),
                    repeat: String::new(),
                    error: None,
                });
            }
            AfterUnlock::RefreshSnapshots | AfterUnlock::Remember => {}
        }
        Ok(())
    }

    pub fn change_passphrase(&mut self, passphrase: &str) -> Result<(), String> {
        let Some(key) = &self.vault.key else {
            return Err(self.lang.error_message(&EngineError::Locked));
        };
        vault::change_passphrase(&self.config.destination, key, passphrase)
            .map_err(|e| self.lang.error_message(&e.into()))?;
        self.refresh_vault();
        tracing::info!("vault passphrase changed");
        Ok(())
    }

    pub fn set_remembered(&mut self, remember: bool) {
        let Some(header) = self.vault.header.clone() else {
            return;
        };
        if remember {
            match &self.vault.key {
                Some(key) => {
                    if let Err(err) = vault::remember_key(&self.key_dir(), &header, key) {
                        self.notify(NoticeKind::Error, err.to_string());
                    }
                }
                None => self.open_unlock(AfterUnlock::Remember),
            }
        } else if let Err(err) = vault::forget_key(&self.key_dir(), &header) {
            self.notify(NoticeKind::Error, err.to_string());
        }
        self.refresh_vault();
    }

    pub fn lock_vault(&mut self, ctx: &egui::Context) {
        self.vault.key = None;
        self.refresh_snapshots(ctx);
    }

    // --- automatic backups ---------------------------------------------------------------

    pub(super) fn check_schedule_on_start(&mut self, _ctx: &egui::Context) {
        if !self.config.schedule.enabled {
            // Nothing to do; turning it off later removes the task.
            self.schedule.applied = Some(self.config.schedule.clone());
            return;
        }
        let exe = std::env::current_exe().ok();
        let up_to_date = self.state.task_executable.is_some()
            && self.state.task_executable == exe
            && scheduler::is_installed();
        self.schedule.applied = up_to_date.then(|| self.config.schedule.clone());
    }

    /// Installs or removes the scheduled task when the settings changed.
    pub(super) fn apply_schedule(&mut self, ctx: &egui::Context) {
        if self.schedule.job.is_some()
            || self.schedule.applied.as_ref() == Some(&self.config.schedule)
        {
            return;
        }
        let schedule = self.config.schedule.clone();
        let work_dir = self
            .paths
            .config_file
            .parent()
            .map(PathBuf::from)
            .unwrap_or_default();
        self.schedule.job = Some(Job::spawn(ctx, move |_, send| {
            let result = if schedule.enabled {
                std::env::current_exe()
                    .and_then(|exe| scheduler::install(&schedule, &exe, &work_dir))
            } else {
                scheduler::remove()
            };
            send((schedule, result.map_err(|e| e.to_string())));
        }));
    }
}
