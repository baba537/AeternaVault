//! The graphical interface (egui / eframe).
//!
//! egui was chosen because it is a single, self-contained dependency that
//! renders the whole window itself, so the calm custom look is fully under our
//! control. See docs/ARCHITECTURE.md for iced, Slint and Tauri as alternatives.

mod tasks;
mod theme;
mod views;
mod widgets;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, Stroke, Ui};

use crate::config::{Appearance, Config, ConfigNotice, Loaded, Source};
use crate::engine::backup::{self, BackupReport};
use crate::engine::plan::{self, BackupPlan, ItemKind, RestoreOptions, RestorePlan, RestoreTarget};
use crate::engine::restore::{self, RestoreReport};
use crate::engine::snapshots::{self, SnapshotInfo};
use crate::engine::{paths_equal, scan};
use crate::error::EngineError;
use crate::i18n::Lang;
use crate::logging::LogBuffer;
use crate::paths::AppPaths;
use crate::platform::{self, apps, vss::LiveFiles};
use tasks::{Job, Task, TaskKind, TaskOutput};
use theme::palette;
use widgets::{ButtonKind, NoticeKind};

pub fn run(paths: AppPaths, loaded: Loaded, log: LogBuffer) -> anyhow::Result<()> {
    let lang = Lang::resolve(loaded.config.language);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(lang.t().window_title)
        .with_inner_size([1000.0, 780.0])
        .with_min_inner_size([760.0, 580.0]);
    match eframe::icon_data::from_png_bytes(include_bytes!(
        "../../assets/icon/aeterna-vault-256.png"
    )) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(err) => tracing::warn!("window icon could not be loaded: {err}"),
    }

    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "AeternaVault",
        options,
        Box::new(move |cc| {
            theme::install_fonts(&cc.egui_ctx, &loaded.config.fonts);
            theme::apply(&cc.egui_ctx, loaded.config.appearance);
            Ok(Box::new(AeternaApp::new(&cc.egui_ctx, paths, loaded, log)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Backup,
    Restore,
    Apps,
    Settings,
    Activity,
}

pub enum Screen {
    Main,
    Working,
    Preview(Box<views::preview::PreviewState>),
    Done(Box<Done>),
}

// Lives boxed inside `Screen::Done`, so the size difference does not matter.
#[allow(clippy::large_enum_variant)]
pub enum Done {
    Backup(Result<BackupReport, String>),
    Restore(Result<RestoreReport, String>),
}

/// A plan waiting for the user's confirmation.
pub enum Pending {
    Backup(Box<BackupPlan>),
    Restore(Box<RestorePlan>),
}

pub struct Notice {
    pub kind: NoticeKind,
    pub text: String,
}

/// Folder path with (bytes, files), or `None` if the folder is missing.
type SizeLoad = (PathBuf, Option<(u64, u64)>);
type SnapshotLoad = (Result<Vec<SnapshotInfo>, EngineError>, Option<u64>);

#[derive(Default)]
pub struct RestoreUi {
    pub selected: Option<String>,
    pub to_folder: bool,
    pub folder: Option<PathBuf>,
}

#[derive(Default)]
pub struct AppsUi {
    /// Known application data folders, named in the language they were loaded for.
    pub profiles: Option<(Lang, Vec<apps::KnownProfile>)>,
    pub installed: Option<Vec<apps::InstalledApp>>,
    pub job: Option<Job<Vec<apps::InstalledApp>>>,
    pub search: String,
}

#[derive(Default)]
pub struct SettingsUi {
    pub exclude_text: String,
    pub invalid_patterns: Vec<String>,
}

pub struct AeternaApp {
    pub paths: AppPaths,
    pub config: Config,
    pub lang: Lang,
    pub view: View,
    pub screen: Screen,
    pub task: Option<Task>,
    pub pending: Option<Pending>,
    pub notices: Vec<Notice>,
    pub log: LogBuffer,
    pub computer: String,

    pub sizes: HashMap<PathBuf, Option<(u64, u64)>>,
    sizes_job: Option<Job<SizeLoad>>,
    pub snapshots: Option<Result<Vec<SnapshotInfo>, EngineError>>,
    snapshots_job: Option<Job<SnapshotLoad>>,
    pub destination_free: Option<u64>,

    pub restore: RestoreUi,
    pub apps: AppsUi,
    pub settings: SettingsUi,
    config_dirty: bool,
    applied_title: Option<Lang>,
}

impl AeternaApp {
    fn new(ctx: &egui::Context, paths: AppPaths, loaded: Loaded, log: LogBuffer) -> Self {
        let lang = Lang::resolve(loaded.config.language);
        let mut app = Self {
            settings: SettingsUi {
                exclude_text: loaded.config.exclude.join("\n"),
                invalid_patterns: Vec::new(),
            },
            paths,
            config: loaded.config,
            lang,
            view: View::Backup,
            screen: Screen::Main,
            task: None,
            pending: None,
            notices: Vec::new(),
            log,
            computer: platform::computer_name(),
            sizes: HashMap::new(),
            sizes_job: None,
            snapshots: None,
            snapshots_job: None,
            destination_free: None,
            restore: RestoreUi::default(),
            apps: AppsUi::default(),
            config_dirty: false,
            applied_title: Some(lang),
        };

        if let Some(notice) = loaded.notice {
            app.config_notice(notice);
        }
        app.refresh_sizes(ctx);
        app.refresh_snapshots(ctx);
        app
    }

    fn config_notice(&mut self, notice: ConfigNotice) {
        let (kind, text) = match notice {
            ConfigNotice::Created => (NoticeKind::Info, self.lang.t().config_created.to_string()),
            ConfigNotice::Invalid { kept_copy, message } => (
                NoticeKind::Warning,
                self.lang
                    .config_invalid(&message, &kept_copy.display().to_string()),
            ),
            ConfigNotice::NotSaved { message } => {
                (NoticeKind::Warning, self.lang.config_not_saved(&message))
            }
        };
        self.notify(kind, text);
    }

    pub fn notify(&mut self, kind: NoticeKind, text: impl Into<String>) {
        let text = text.into();
        self.notices.retain(|n| n.text != text);
        self.notices.push(Notice { kind, text });
        if self.notices.len() > 3 {
            self.notices.remove(0);
        }
    }

    pub fn mark_dirty(&mut self) {
        self.config_dirty = true;
    }

    pub fn is_busy(&self) -> bool {
        self.task.is_some()
    }

    // --- sources ---------------------------------------------------------------

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

    pub fn refresh_sizes(&mut self, ctx: &egui::Context) {
        if let Some(job) = self.sizes_job.take() {
            job.cancel.cancel();
        }
        let pending: Vec<PathBuf> = self
            .config
            .sources
            .iter()
            .map(|s| s.path.clone())
            .filter(|p| self.size_of(p).and_then(|s| s.as_ref()).is_none())
            .collect();
        if pending.is_empty() {
            return;
        }
        self.sizes_job = Some(Job::spawn(ctx, move |cancel, send| {
            for path in pending {
                if cancel.is_cancelled() {
                    return;
                }
                let size = if path.is_dir() {
                    scan::folder_size(&path, cancel)
                } else {
                    None
                };
                send((path, size));
            }
        }));
    }

    // --- snapshots -------------------------------------------------------------

    pub fn refresh_snapshots(&mut self, ctx: &egui::Context) {
        let destination = self.config.destination.clone();
        if destination.as_os_str().is_empty() {
            self.snapshots = Some(Err(EngineError::NoDestination));
            return;
        }
        self.snapshots_job = Some(Job::spawn(ctx, move |_, send| {
            let list = snapshots::list(&destination);
            let free = platform::free_space(&destination);
            send((list, free));
        }));
    }

    pub fn last_backup(&self) -> Option<&SnapshotInfo> {
        match &self.snapshots {
            Some(Ok(list)) => list
                .iter()
                .find(|s| s.computer.eq_ignore_ascii_case(&self.computer)),
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

    fn poll_jobs(&mut self, ctx: &egui::Context) {
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
                        self.restore.selected = list
                            .iter()
                            .find(|s| {
                                s.header.is_some()
                                    && s.computer.eq_ignore_ascii_case(&self.computer)
                            })
                            .or_else(|| list.iter().find(|s| s.header.is_some()))
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

        if let Some(job) = &self.apps.job {
            let (mut values, finished) = job.drain();
            if let Some(list) = values.pop() {
                self.apps.installed = Some(list);
            }
            if finished {
                self.apps.job = None;
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

    // --- operations ------------------------------------------------------------

    pub fn plan_backup(&mut self, ctx: &egui::Context, confirm: bool) {
        if self.is_busy() {
            return;
        }
        let config = self.config.clone();
        let computer = self.computer.clone();
        self.task = Some(Task::spawn(
            ctx,
            TaskKind::PlanBackup { confirm },
            move |cancel, progress| {
                TaskOutput::BackupPlan(plan::plan_backup(&config, &computer, cancel, progress))
            },
        ));
        self.screen = Screen::Working;
    }

    pub fn selected_snapshot(&self) -> Option<&SnapshotInfo> {
        let selected = self.restore.selected.as_ref()?;
        match &self.snapshots {
            Some(Ok(list)) => list.iter().find(|s| &s.qualified_id() == selected),
            _ => None,
        }
    }

    pub fn plan_restore(&mut self, ctx: &egui::Context, confirm: bool) {
        if self.is_busy() {
            return;
        }
        let Some(snapshot) = self.selected_snapshot().cloned() else {
            return;
        };
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
        };
        self.task = Some(Task::spawn(
            ctx,
            TaskKind::PlanRestore { confirm },
            move |cancel, progress| {
                TaskOutput::RestorePlan(plan::plan_restore(&snapshot, options, cancel, progress))
            },
        ));
        self.screen = Screen::Working;
    }

    pub fn start_backup(&mut self, ctx: &egui::Context, plan: Box<BackupPlan>) {
        if self.is_busy() {
            return;
        }
        self.task = Some(Task::spawn(
            ctx,
            TaskKind::Backup,
            move |cancel, progress| {
                TaskOutput::Backup(backup::run_backup(&plan, &LiveFiles, cancel, progress))
            },
        ));
        self.screen = Screen::Working;
    }

    pub fn start_restore(&mut self, ctx: &egui::Context, plan: Box<RestorePlan>) {
        if self.is_busy() {
            return;
        }
        self.task = Some(Task::spawn(
            ctx,
            TaskKind::Restore,
            move |cancel, progress| {
                TaskOutput::Restore(restore::run_restore(&plan, cancel, progress))
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
                    self.screen =
                        Screen::Preview(Box::new(views::preview::PreviewState::backup(plan)));
                } else if self.config.advanced.confirm_before_start {
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
                    self.screen =
                        Screen::Preview(Box::new(views::preview::PreviewState::restore(plan)));
                } else if self.config.advanced.confirm_before_start || replaces {
                    // Overwriting files always asks, regardless of the setting.
                    self.pending = Some(Pending::Restore(Box::new(plan)));
                    self.screen = Screen::Main;
                } else {
                    self.start_restore(ctx, Box::new(plan));
                }
            }
            TaskOutput::BackupPlan(Err(err)) | TaskOutput::RestorePlan(Err(err)) => {
                self.screen = Screen::Main;
                if !matches!(err, EngineError::Cancelled) {
                    tracing::warn!("planning failed: {err}");
                    self.notify(NoticeKind::Warning, self.lang.error_message(&err));
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
                self.refresh_sizes(ctx);
            }
        }
    }

    pub fn reload_config(&mut self, ctx: &egui::Context) {
        let loaded = crate::config::load_or_create(&self.paths);
        self.config = loaded.config;
        self.lang = Lang::resolve(self.config.language);
        self.settings.exclude_text = self.config.exclude.join("\n");
        theme::install_fonts(ctx, &self.config.fonts);
        theme::apply(ctx, self.config.appearance);
        self.sizes.clear();
        self.refresh_sizes(ctx);
        self.refresh_snapshots(ctx);
        match loaded.notice {
            Some(notice) => self.config_notice(notice),
            None => self.notify(NoticeKind::Success, self.lang.t().config_reloaded),
        }
    }

    fn save_if_dirty(&mut self) {
        if !self.config_dirty {
            return;
        }
        self.config_dirty = false;
        if let Err(err) = self.config.save(&self.paths.config_file) {
            tracing::error!("configuration could not be saved: {err}");
            self.notify(
                NoticeKind::Error,
                self.lang.config_not_saved(&err.to_string()),
            );
        }
    }

    // --- layout ------------------------------------------------------------------

    fn header(&mut self, ui: &mut Ui) {
        let p = *palette(ui);
        let t = self.lang.t();
        let busy = self.is_busy() || !matches!(self.screen, Screen::Main | Screen::Done(_));

        egui::Panel::top("header")
            .show_separator_line(false)
            .frame(Frame::new().fill(p.background).inner_margin(Margin {
                left: 32,
                right: 28,
                top: 18,
                bottom: 0,
            }))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    widgets::logo(ui, 48.0);
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.add_space(2.0);
                        widgets::title(ui, "AeternaVault");
                        widgets::slogan(ui, t.slogan);
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let dark = ui.visuals().dark_mode;
                        let theme_button = theme_toggle(ui, dark).on_hover_text(if dark {
                            t.switch_to_light
                        } else {
                            t.switch_to_dark
                        });
                        if theme_button.clicked() {
                            self.config.appearance = if dark {
                                Appearance::Light
                            } else {
                                Appearance::Dark
                            };
                            theme::apply(ui.ctx(), self.config.appearance);
                            self.mark_dirty();
                        }
                        if widgets::button(ui, ButtonKind::Quiet, t.switch_language, true).clicked()
                        {
                            self.config.language = match self.lang {
                                Lang::En => crate::config::LanguageSetting::De,
                                Lang::De => crate::config::LanguageSetting::En,
                            };
                            self.lang = Lang::resolve(self.config.language);
                            self.mark_dirty();
                        }
                    });
                });

                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 26.0;
                    for (view, label) in [
                        (View::Backup, t.nav_backup),
                        (View::Restore, t.nav_restore),
                        (View::Apps, t.nav_apps),
                        (View::Settings, t.nav_settings),
                        (View::Activity, t.nav_activity),
                    ] {
                        let selected = self.view == view && matches!(self.screen, Screen::Main);
                        if widgets::nav_tab(ui, label, selected, !busy).clicked() {
                            self.view = view;
                            self.screen = Screen::Main;
                            if view == View::Restore {
                                self.refresh_snapshots(ui.ctx());
                            }
                        }
                    }
                });
                let rect = ui.max_rect();
                ui.painter().line_segment(
                    [
                        egui::pos2(rect.left() - 32.0, ui.cursor().top()),
                        egui::pos2(rect.right() + 28.0, ui.cursor().top()),
                    ],
                    Stroke::new(1.0, p.border),
                );
            });
    }

    fn footer(&self, ui: &mut Ui) {
        let p = *palette(ui);
        egui::Panel::bottom("footer")
            .show_separator_line(false)
            .frame(
                Frame::new()
                    .fill(p.background)
                    .inner_margin(Margin::symmetric(32, 8)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    widgets::secondary_text(
                        ui,
                        format!("AeternaVault {}", env!("CARGO_PKG_VERSION")),
                    );
                    if self.paths.portable {
                        widgets::secondary_text(ui, "·");
                        widgets::secondary_text(ui, self.lang.t().portable_mode);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        widgets::secondary_text(ui, &self.computer);
                    });
                });
            });
    }

    fn confirmation(&mut self, ui: &mut Ui) {
        let Some(pending) = &self.pending else {
            return;
        };
        let t = self.lang.t();
        let lang = self.lang;
        let mut action = 0; // 1 = start, 2 = details, 3 = cancel

        let frame_palette = *palette(ui);
        let frame = Frame::new()
            .fill(frame_palette.panel)
            .stroke(Stroke::new(1.0, frame_palette.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(26))
            .shadow(ui.visuals().window_shadow);
        let modal = egui::Modal::new(egui::Id::new("confirm"))
            .frame(frame)
            .show(ui.ctx(), |ui| {
                let p = *palette(ui);
                ui.set_max_width(480.0);
                let (heading, body, warn) = match pending {
                    Pending::Backup(plan) => {
                        let (files, bytes) = plan.copy_totals();
                        (
                            t.confirm_backup_title,
                            lang.confirm_backup(
                                files,
                                bytes,
                                &plan.destination.display().to_string(),
                            ),
                            false,
                        )
                    }
                    Pending::Restore(plan) => (
                        t.confirm_restore_title,
                        lang.confirm_restore(
                            plan.summary.count(ItemKind::New),
                            plan.summary.count(ItemKind::Changed),
                        ),
                        plan.summary.count(ItemKind::Changed) > 0,
                    ),
                };
                ui.label(
                    egui::RichText::new(heading)
                        .family(egui::FontFamily::Name(theme::SERIF.into()))
                        .size(22.0),
                );
                ui.add_space(6.0);
                ui.add(egui::Label::new(body).wrap());
                if warn {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(t.confirm_overwrite).color(p.warning));
                }
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if widgets::button(ui, ButtonKind::Quiet, t.show_details, true).clicked() {
                        action = 2;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::button(ui, ButtonKind::Primary, t.start, true).clicked() {
                            action = 1;
                        }
                        if widgets::button(ui, ButtonKind::Secondary, t.cancel, true).clicked() {
                            action = 3;
                        }
                    });
                });
            });
        if modal.should_close() && action == 0 {
            action = 3;
        }

        let ctx = ui.ctx().clone();
        match action {
            1 => match self.pending.take() {
                Some(Pending::Backup(plan)) => self.start_backup(&ctx, plan),
                Some(Pending::Restore(plan)) => self.start_restore(&ctx, plan),
                None => {}
            },
            2 => match self.pending.take() {
                Some(Pending::Backup(plan)) => {
                    self.screen =
                        Screen::Preview(Box::new(views::preview::PreviewState::backup(*plan)));
                }
                Some(Pending::Restore(plan)) => {
                    self.screen =
                        Screen::Preview(Box::new(views::preview::PreviewState::restore(*plan)));
                }
                None => {}
            },
            3 => self.pending = None,
            _ => {}
        }
    }

    fn handle_dropped_folders(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        if dropped.is_empty() || self.is_busy() {
            return;
        }
        for path in dropped.into_iter().filter(|p| p.is_dir()) {
            self.add_folder(ctx, path);
        }
        self.view = View::Backup;
    }
}

/// Appearance switch drawn as a half-filled circle (a common, wordless symbol).
fn theme_toggle(ui: &mut Ui, dark: bool) -> egui::Response {
    let p = *palette(ui);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(32.0, 30.0), egui::Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), p.raised);
    }
    let color = if hovered { p.text } else { p.text_secondary };
    let c = rect.center();
    let r = 7.5;
    ui.painter().circle_stroke(c, r, Stroke::new(1.3, color));
    let half: Vec<egui::Pos2> = (0..=24)
        .map(|i| {
            let a = std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 24.0;
            let a = if dark { a + std::f32::consts::PI } else { a };
            c + egui::vec2(a.cos(), a.sin()) * r
        })
        .collect();
    ui.painter()
        .add(egui::Shape::convex_polygon(half, color, Stroke::NONE));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

impl eframe::App for AeternaApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_jobs(&ctx);
        self.handle_dropped_folders(&ctx);

        if self.applied_title != Some(self.lang) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                self.lang.t().window_title.to_string(),
            ));
            self.applied_title = Some(self.lang);
        }

        self.header(ui);
        self.footer(ui);

        let p = *palette(ui);
        if matches!(self.screen, Screen::Main) && matches!(self.view, View::Backup | View::Restore)
        {
            egui::Panel::bottom("actions")
                .show_separator_line(false)
                .frame(
                    Frame::new()
                        .fill(p.panel)
                        .inner_margin(Margin::symmetric(32, 14)),
                )
                .show(ui, |ui| {
                    let rect = ui.max_rect();
                    ui.painter().line_segment(
                        [
                            egui::pos2(rect.left() - 32.0, rect.top() - 14.0),
                            egui::pos2(rect.right() + 32.0, rect.top() - 14.0),
                        ],
                        Stroke::new(1.0, p.border),
                    );
                    widgets::centered_column(ui, 860.0, |ui| match self.view {
                        View::Restore => views::restore::action_bar(self, ui),
                        _ => views::backup::action_bar(self, ui),
                    });
                });
        }

        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.background).inner_margin(Margin {
                left: 32,
                right: 32,
                top: 20,
                bottom: 12,
            }))
            .show(ui, |ui| {
                // Notices sit above every screen.
                let mut dismissed = None;
                if !self.notices.is_empty() {
                    widgets::centered_column(ui, 860.0, |ui| {
                        for (i, n) in self.notices.iter().enumerate() {
                            if widgets::notice(ui, &n.kind, &n.text) {
                                dismissed = Some(i);
                            }
                        }
                        ui.add_space(6.0);
                    });
                }
                if let Some(i) = dismissed {
                    self.notices.remove(i);
                }

                match &self.screen {
                    Screen::Preview(_) => views::preview::show(self, ui),
                    Screen::Working => views::working::show_working(self, ui),
                    Screen::Done(_) => views::working::show_done(self, ui),
                    Screen::Main => {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                widgets::centered_column(ui, 860.0, |ui| match self.view {
                                    View::Backup => views::backup::show(self, ui),
                                    View::Restore => views::restore::show(self, ui),
                                    View::Apps => views::apps::show(self, ui),
                                    View::Settings => views::settings::show(self, ui),
                                    View::Activity => views::activity::show(self, ui),
                                });
                            });
                    }
                }
            });

        self.confirmation(ui);
        self.save_if_dirty();
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        let p = if visuals.dark_mode {
            theme::DARK
        } else {
            theme::LIGHT
        };
        p.background.to_normalized_gamma_f32()
    }
}
