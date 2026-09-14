//! Preview / dry-run: computes what a backup or restore *would* do.
//!
//! This module is strictly read-only (enforced by a test in `engine/mod.rs`).
//! The resulting plans are shown to the user, can be exported, and are only
//! executed after explicit confirmation.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::crypto::VaultKey;
use super::manifest::{self, FileEntry, SnapshotHeader};
use super::scan::{self, Excludes, ScanOptions, SkipReason};
use super::selection::Selection;
use super::snapshots::{self, SnapshotInfo};
use super::sources::{Sources, sanitize_segment};
use super::{
    CancelToken, Phase, Progress, Reporter, path_is_within, safe_relative_path, to_unix_nanos,
    vault,
};
use crate::config::{BackupMode, Config, ConflictPolicy};
use crate::error::{EngineError, EngineResult};
use crate::platform::known_paths::KnownPaths;
use crate::platform::registry::{self, RegistryExport};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemKind {
    /// Backup: not in the previous backup. Restore: target does not exist.
    New,
    /// Backup: differs from the previous backup. Restore: target will be replaced.
    Changed,
    /// Nothing to do.
    Unchanged,
    /// Backup only: was in the previous backup, no longer in the source.
    /// Nothing is deleted — older backups keep the file.
    Removed,
    /// Left out, see [`Note`].
    Skipped,
}

impl ItemKind {
    pub const ALL: [ItemKind; 5] = [
        Self::New,
        Self::Changed,
        Self::Unchanged,
        Self::Removed,
        Self::Skipped,
    ];

    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    Link,
    OnlineOnly,
    InsideDestination,
    SourceMissing,
    NoAccess(String),
    KeepExisting,
    TargetNewer,
    TargetIsFolder,
    BackupFileMissing,
    UnsafePath,
}

impl From<SkipReason> for Note {
    fn from(reason: SkipReason) -> Self {
        match reason {
            SkipReason::Link => Note::Link,
            SkipReason::OnlineOnly => Note::OnlineOnly,
            SkipReason::InsideDestination => Note::InsideDestination,
            SkipReason::SourceMissing => Note::SourceMissing,
            SkipReason::NoAccess(e) => Note::NoAccess(e),
        }
    }
}

/// Reference to existing content inside the backup.
#[derive(Debug, Clone)]
pub struct BlobRef {
    pub blob: String,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct PlanItem {
    pub kind: ItemKind,
    /// Index into the plan's `sources`.
    pub source: usize,
    /// Relative path (`/`-separated); for skipped entries possibly a full path;
    /// for registry items the full key.
    pub rel: String,
    pub size: u64,
    pub modified: i64,
    pub note: Option<Note>,
    pub blob: Option<BlobRef>,
    pub registry: bool,
}

#[derive(Debug, Clone)]
pub struct PlannedSource {
    pub name: String,
    /// Backup: the source folder. Restore: the folder files are written to.
    pub root: PathBuf,
    /// Folder inside the snapshot.
    pub key: String,
    pub portable: Option<String>,
    pub app: Option<String>,
    pub extra: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Summary {
    pub counts: [u64; 5],
    pub bytes: [u64; 5],
}

impl Summary {
    fn add(&mut self, item: &PlanItem) {
        self.counts[item.kind.index()] += 1;
        self.bytes[item.kind.index()] += item.size;
    }

    pub fn count(&self, kind: ItemKind) -> u64 {
        self.counts[kind.index()]
    }

    pub fn bytes(&self, kind: ItemKind) -> u64 {
        self.bytes[kind.index()]
    }
}

/// Common read access for the preview UI and exports.
pub trait Plan {
    fn items(&self) -> &[PlanItem];
    fn sources(&self) -> &[PlannedSource];
    fn summary(&self) -> &Summary;

    fn item_path(&self, item: &PlanItem) -> PathBuf {
        if item.registry {
            return PathBuf::from(&item.rel);
        }
        let root = &self.sources()[item.source].root;
        match safe_relative_path(&item.rel) {
            Some(rel) => root.join(rel),
            None => PathBuf::from(&item.rel),
        }
    }
}

fn push(items: &mut Vec<PlanItem>, summary: &mut Summary, item: PlanItem) {
    summary.add(&item);
    items.push(item);
}

// ---------------------------------------------------------------------------
// Backup
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PlannedRegistry {
    pub app: String,
    pub app_name: String,
    pub key: String,
}

#[derive(Debug, Clone)]
pub struct BackupPlan {
    pub mode: BackupMode,
    pub destination: PathBuf,
    pub computer: String,
    pub encrypted: bool,
    pub base: Option<SnapshotInfo>,
    pub hardlink_unchanged: bool,
    pub save_program_list: bool,
    pub sources: Vec<PlannedSource>,
    pub registry: Vec<PlannedRegistry>,
    pub items: Vec<PlanItem>,
    pub summary: Summary,
    pub excluded: u64,
    pub warnings: Vec<String>,
    /// Applications whose processes are running (display names).
    pub running_apps: Vec<String>,
}

impl Plan for BackupPlan {
    fn items(&self) -> &[PlanItem] {
        &self.items
    }
    fn sources(&self) -> &[PlannedSource] {
        &self.sources
    }
    fn summary(&self) -> &Summary {
        &self.summary
    }
}

impl BackupPlan {
    /// Whether an item's content is copied (as opposed to linked/referenced).
    pub fn copies(&self, item: &PlanItem) -> bool {
        if item.registry {
            return false;
        }
        match item.kind {
            ItemKind::New | ItemKind::Changed => true,
            ItemKind::Unchanged => self.mode == BackupMode::Full || item.blob.is_none(),
            ItemKind::Removed | ItemKind::Skipped => false,
        }
    }

    /// Files that end up in the new snapshot.
    pub fn stored_files(&self) -> impl Iterator<Item = &PlanItem> {
        self.items.iter().filter(|i| {
            !i.registry
                && matches!(
                    i.kind,
                    ItemKind::New | ItemKind::Changed | ItemKind::Unchanged
                )
        })
    }

    /// (files, bytes) that will be copied.
    pub fn copy_totals(&self) -> (u64, u64) {
        self.items
            .iter()
            .filter(|i| self.copies(i))
            .fold((0, 0), |(f, b), i| (f + 1, b + i.size))
    }
}

pub struct BackupInput<'a> {
    pub config: &'a Config,
    pub sources: &'a Sources,
    pub computer: &'a str,
    pub key: Option<&'a VaultKey>,
    pub running_apps: Vec<String>,
}

pub fn plan_backup(
    input: &BackupInput<'_>,
    cancel: &CancelToken,
    on_progress: &mut dyn FnMut(&Progress),
) -> EngineResult<BackupPlan> {
    let config = input.config;
    if input.sources.is_empty() {
        return Err(EngineError::NoSources);
    }
    let destination = config.destination.clone();
    if destination.as_os_str().is_empty() {
        return Err(EngineError::NoDestination);
    }
    if !snapshots::destination_reachable(&destination) {
        return Err(EngineError::DestinationUnavailable(destination));
    }
    for source in &input.sources.folders {
        if path_is_within(&source.root, &destination) {
            return Err(EngineError::SourceInsideDestination {
                source_path: source.root.clone(),
                destination,
            });
        }
    }

    let encrypted = config.encryption.enabled;
    if encrypted {
        if !vault::exists(&destination) {
            return Err(EngineError::EncryptionNotSetUp);
        }
        if input.key.is_none() {
            return Err(EngineError::Locked);
        }
    }

    let mut warnings = Vec::new();

    // The previous backup to compare against.
    let base = snapshots::latest_usable(&destination, input.computer, encrypted, input.key);
    let base_index = match &base {
        Some(info) => match info.load_index(input.key) {
            Ok(index) => Some(index),
            Err(err) => {
                warnings.push(format!(
                    "previous backup could not be read, all files count as new: {err}"
                ));
                None
            }
        },
        None => None,
    };
    let base = if base_index.is_some() { base } else { None };

    // Lookup: lowercase source path -> lowercase relative path -> entry.
    let mut base_lookup: HashMap<String, HashMap<String, &FileEntry>> = HashMap::new();
    let mut base_registry: HashMap<String, &RegistryExport> = HashMap::new();
    if let (Some(info), Some(index)) = (&base, &base_index)
        && let Some(header) = &info.header
    {
        let key_to_path: HashMap<&str, String> = header
            .sources
            .iter()
            .map(|s| (s.key.as_str(), path_key(&s.path)))
            .collect();
        for entry in &index.files {
            if let Some(path) = key_to_path.get(entry.source.as_str()) {
                base_lookup
                    .entry(path.clone())
                    .or_default()
                    .insert(entry.path.to_lowercase(), entry);
            }
        }
        for export in &index.registry {
            base_registry.insert(export.root.to_lowercase(), export);
        }
    }

    let mut plan = BackupPlan {
        mode: config.mode,
        destination: destination.clone(),
        computer: input.computer.to_string(),
        encrypted,
        base: base.clone(),
        hardlink_unchanged: config.advanced.hardlink_unchanged,
        save_program_list: config.advanced.save_program_list,
        sources: Vec::new(),
        registry: Vec::new(),
        items: Vec::new(),
        summary: Summary::default(),
        excluded: 0,
        warnings,
        running_apps: input.running_apps.clone(),
    };

    let mut progress = Progress::default();
    let mut reporter = Reporter::new(on_progress);

    for source in &input.sources.folders {
        let source_index = plan.sources.len();
        plan.sources.push(PlannedSource {
            name: source.name.clone(),
            root: source.root.clone(),
            key: source.key.clone(),
            portable: source.portable.clone(),
            app: source.app.clone(),
            extra: false,
        });

        let excludes = Excludes::new(config.exclude.iter().chain(source.excludes.iter()));
        let options = ScanOptions {
            excludes: &excludes,
            selection: Selection::new(&source.include_paths, &source.exclude_paths),
            skip_online_only: config.advanced.skip_online_only_files,
            destination: &destination,
        };
        progress.current = source.root.display().to_string();
        reporter.now(&progress);
        let scanned =
            scan::scan_source(&source.root, &options, cancel, &mut progress, &mut reporter)?;
        plan.excluded += scanned.excluded;

        let previous = base_lookup.get(&path_key(&source.root));
        let mut seen = HashSet::new();

        for file in scanned.files {
            let lower = file.rel.to_lowercase();
            let previous_entry = previous.and_then(|m| m.get(&lower));
            let kind = match previous_entry {
                None => ItemKind::New,
                Some(e) if e.size == file.size && e.modified == file.modified => {
                    ItemKind::Unchanged
                }
                Some(_) => ItemKind::Changed,
            };
            let blob = previous_entry
                .filter(|_| kind == ItemKind::Unchanged)
                .map(|e| BlobRef {
                    blob: e.blob.clone(),
                    sha256: e.sha256.clone(),
                });
            seen.insert(lower);
            push(
                &mut plan.items,
                &mut plan.summary,
                PlanItem {
                    kind,
                    source: source_index,
                    rel: file.rel,
                    size: file.size,
                    modified: file.modified,
                    note: None,
                    blob,
                    registry: false,
                },
            );
        }

        for skipped in scanned.skipped {
            let rel = skipped
                .path
                .strip_prefix(&source.root)
                .map(super::rel_to_string)
                .unwrap_or_default();
            push(
                &mut plan.items,
                &mut plan.summary,
                PlanItem {
                    kind: ItemKind::Skipped,
                    source: source_index,
                    rel: if rel.is_empty() {
                        skipped.path.display().to_string()
                    } else {
                        rel
                    },
                    size: 0,
                    modified: 0,
                    note: Some(skipped.reason.into()),
                    blob: None,
                    registry: false,
                },
            );
        }

        if let Some(previous) = previous {
            let mut removed: Vec<_> = previous
                .iter()
                .filter(|(lower, _)| !seen.contains(*lower))
                .map(|(_, e)| *e)
                .collect();
            removed.sort_by(|a, b| a.path.cmp(&b.path));
            for entry in removed {
                push(
                    &mut plan.items,
                    &mut plan.summary,
                    PlanItem {
                        kind: ItemKind::Removed,
                        source: source_index,
                        rel: entry.path.clone(),
                        size: entry.size,
                        modified: entry.modified,
                        note: None,
                        blob: None,
                        registry: false,
                    },
                );
            }
        }
    }

    // Registry settings of chosen applications.
    let mut app_sources: HashMap<String, usize> = HashMap::new();
    for reg in &input.sources.registry {
        if cancel.is_cancelled() {
            return Err(EngineError::Cancelled);
        }
        let export = match registry::export(&reg.key) {
            Ok(Some(export)) => export,
            Ok(None) => continue,
            Err(err) => {
                plan.warnings.push(format!("{}: {err}", reg.key));
                continue;
            }
        };
        let source = *app_sources.entry(reg.app.clone()).or_insert_with(|| {
            plan.sources.push(PlannedSource {
                name: reg.app_name.clone(),
                root: PathBuf::new(),
                key: registry_folder_key(&reg.app_name),
                portable: None,
                app: Some(reg.app.clone()),
                extra: false,
            });
            plan.sources.len() - 1
        });
        let kind = match base_registry.get(&export.root.to_lowercase()) {
            None => ItemKind::New,
            Some(previous) if previous.fingerprint() == export.fingerprint() => ItemKind::Unchanged,
            Some(_) => ItemKind::Changed,
        };
        plan.registry.push(PlannedRegistry {
            app: reg.app.clone(),
            app_name: reg.app_name.clone(),
            key: export.root.clone(),
        });
        push(
            &mut plan.items,
            &mut plan.summary,
            PlanItem {
                kind,
                source,
                rel: export.root,
                size: 0,
                modified: 0,
                note: None,
                blob: None,
                registry: true,
            },
        );
    }

    progress.phase = Phase::Finishing;
    reporter.now(&progress);
    Ok(plan)
}

/// Folder for `.reg` copies of an application's registry settings.
pub fn registry_folder_key(app_name: &str) -> String {
    format!(
        "{}/{}/Registry",
        manifest::APPS_DIR,
        sanitize_segment(app_name)
    )
}

fn path_key(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect::<Vec<_>>()
        .join("\\")
}

// ---------------------------------------------------------------------------
// Restore
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreTarget {
    /// Back to where the files came from (resolved for the current user).
    Original,
    /// Into `<folder>\<the snapshot's folder structure>`.
    Folder(PathBuf),
}

#[derive(Debug, Clone)]
pub struct RestoreOptions {
    pub target: RestoreTarget,
    pub conflict: ConflictPolicy,
    pub verify: bool,
    /// Parts to leave out: source keys, or `app:<id>` for all of an application.
    pub skip: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct RestorePlan {
    pub snapshot: SnapshotInfo,
    pub options: RestoreOptions,
    pub sources: Vec<PlannedSource>,
    pub items: Vec<PlanItem>,
    pub summary: Summary,
    /// Registry exports to write (already adapted to the current user).
    pub registry: Vec<RegistryExport>,
    /// Applications whose processes are running (display names).
    pub running_apps: Vec<String>,
}

impl Plan for RestorePlan {
    fn items(&self) -> &[PlanItem] {
        &self.items
    }
    fn sources(&self) -> &[PlannedSource] {
        &self.sources
    }
    fn summary(&self) -> &Summary {
        &self.summary
    }
}

impl RestorePlan {
    /// (files, bytes) that will be written.
    pub fn write_totals(&self) -> (u64, u64) {
        self.items
            .iter()
            .filter(|i| matches!(i.kind, ItemKind::New | ItemKind::Changed))
            .fold((0, 0), |(f, b), i| (f + 1, b + i.size))
    }
}

pub fn is_skipped(skip: &HashSet<String>, key: &str, app: Option<&str>) -> bool {
    skip.contains(key) || app.is_some_and(|id| skip.contains(&format!("app:{id}")))
}

pub fn plan_restore(
    snapshot: &SnapshotInfo,
    options: RestoreOptions,
    key: Option<&VaultKey>,
    running_apps: Vec<String>,
    cancel: &CancelToken,
    on_progress: &mut dyn FnMut(&Progress),
) -> EngineResult<RestorePlan> {
    let header: SnapshotHeader = match &snapshot.header {
        Some(header) => header.clone(),
        None if snapshot.is_locked() => return Err(EngineError::Locked),
        None => return Err(EngineError::SnapshotNotFound(snapshot.qualified_id())),
    };
    let index = snapshot.load_index(key)?;
    let here = KnownPaths::current();
    let destination = snapshot.destination();

    let sources: Vec<PlannedSource> = header
        .sources
        .iter()
        .map(|record| {
            let root = match &options.target {
                RestoreTarget::Original => record
                    .portable
                    .as_deref()
                    .and_then(|p| here.resolve(p))
                    .unwrap_or_else(|| record.path.clone()),
                RestoreTarget::Folder(folder) => record
                    .key
                    .split('/')
                    .fold(folder.clone(), |acc, part| acc.join(sanitize_segment(part))),
            };
            PlannedSource {
                name: record.name.clone(),
                root,
                key: record.key.clone(),
                portable: record.portable.clone(),
                app: record.app.clone(),
                extra: record.extra,
            }
        })
        .collect();
    let key_to_index: HashMap<&str, usize> = sources
        .iter()
        .enumerate()
        .map(|(i, s)| (s.key.as_str(), i))
        .collect();

    let mut plan = RestorePlan {
        snapshot: snapshot.clone(),
        options,
        sources: sources.clone(),
        items: Vec::with_capacity(index.files.len()),
        summary: Summary::default(),
        registry: Vec::new(),
        running_apps,
    };

    let mut progress = Progress {
        phase: Phase::Scanning,
        files_total: index.files.len() as u64,
        ..Progress::default()
    };
    let mut reporter = Reporter::new(on_progress);

    for entry in &index.files {
        if cancel.is_cancelled() {
            return Err(EngineError::Cancelled);
        }
        let Some(&source) = key_to_index.get(entry.source.as_str()) else {
            continue;
        };
        let planned = &sources[source];
        if is_skipped(&plan.options.skip, &planned.key, planned.app.as_deref())
            || (planned.extra && plan.options.target == RestoreTarget::Original)
        {
            continue;
        }
        progress.files_done += 1;

        let mut item = PlanItem {
            kind: ItemKind::New,
            source,
            rel: entry.path.clone(),
            size: entry.size,
            modified: entry.modified,
            note: None,
            blob: Some(BlobRef {
                blob: entry.blob.clone(),
                sha256: entry.sha256.clone(),
            }),
            registry: false,
        };

        let Some(rel) = safe_relative_path(&entry.path) else {
            item.kind = ItemKind::Skipped;
            item.note = Some(Note::UnsafePath);
            push(&mut plan.items, &mut plan.summary, item);
            continue;
        };
        let blob_path = match snapshot.blob_root() {
            Some(root) => safe_relative_path(&entry.blob).map(|b| root.join(b)),
            None => entry
                .blob
                .chars()
                .all(|c| c.is_ascii_hexdigit())
                .then(|| vault::blob_path(&destination, &entry.blob)),
        };
        let Some(blob_path) = blob_path else {
            item.kind = ItemKind::Skipped;
            item.note = Some(Note::UnsafePath);
            push(&mut plan.items, &mut plan.summary, item);
            continue;
        };

        let target = planned.root.join(&rel);
        if progress.files_done.is_multiple_of(64) {
            progress.current = target.display().to_string();
            reporter.maybe(&progress);
        }

        if !blob_path.is_file() {
            item.kind = ItemKind::Skipped;
            item.note = Some(Note::BackupFileMissing);
        } else {
            match std::fs::symlink_metadata(&target) {
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => item.kind = ItemKind::New,
                Err(err) => {
                    item.kind = ItemKind::Skipped;
                    item.note = Some(Note::NoAccess(err.to_string()));
                }
                Ok(meta) if meta.is_dir() => {
                    item.kind = ItemKind::Skipped;
                    item.note = Some(Note::TargetIsFolder);
                }
                Ok(meta) => {
                    let existing_modified = meta.modified().map(to_unix_nanos).unwrap_or(0);
                    if meta.len() == entry.size && existing_modified == entry.modified {
                        item.kind = ItemKind::Unchanged;
                    } else {
                        apply_conflict_policy(
                            &mut item,
                            plan.options.conflict,
                            existing_modified > entry.modified,
                        );
                    }
                }
            }
        }
        push(&mut plan.items, &mut plan.summary, item);
    }

    // Registry settings.
    let mut registry_sources: HashMap<String, usize> = HashMap::new();
    for export in &index.registry {
        let record = header
            .registry
            .iter()
            .find(|r| r.key.eq_ignore_ascii_case(&export.root));
        let (app, app_name) = record
            .map(|r| (r.app.clone(), r.app_name.clone()))
            .unwrap_or_else(|| (String::new(), "Registry".to_string()));
        let folder_key = registry_folder_key(&app_name);
        if is_skipped(
            &plan.options.skip,
            &folder_key,
            (!app.is_empty()).then_some(app.as_str()),
        ) {
            continue;
        }
        let source = *registry_sources.entry(app.clone()).or_insert_with(|| {
            let root = match &plan.options.target {
                RestoreTarget::Original => PathBuf::new(),
                RestoreTarget::Folder(folder) => folder_key
                    .split('/')
                    .fold(folder.clone(), |acc, part| acc.join(part)),
            };
            plan.sources.push(PlannedSource {
                name: app_name.clone(),
                root,
                key: folder_key.clone(),
                portable: None,
                app: (!app.is_empty()).then(|| app.clone()),
                extra: false,
            });
            plan.sources.len() - 1
        });

        let adapted = registry::remap_export(export, &header.known_paths, &here);
        let mut item = PlanItem {
            kind: ItemKind::New,
            source,
            rel: adapted.root.clone(),
            size: 0,
            modified: 0,
            note: None,
            blob: None,
            registry: true,
        };
        if plan.options.target == RestoreTarget::Original {
            match registry::export(&adapted.root) {
                Ok(None) => item.kind = ItemKind::New,
                Ok(Some(current)) if contains_all(&current, &adapted) => {
                    item.kind = ItemKind::Unchanged
                }
                Ok(Some(_)) => apply_conflict_policy(&mut item, plan.options.conflict, false),
                Err(err) => {
                    item.kind = ItemKind::Skipped;
                    item.note = Some(Note::NoAccess(err.to_string()));
                }
            }
        }
        push(&mut plan.items, &mut plan.summary, item);
        plan.registry.push(adapted);
    }

    progress.phase = Phase::Finishing;
    reporter.now(&progress);
    Ok(plan)
}

fn apply_conflict_policy(item: &mut PlanItem, policy: ConflictPolicy, existing_is_newer: bool) {
    match policy {
        ConflictPolicy::ReplaceChanged => item.kind = ItemKind::Changed,
        ConflictPolicy::KeepExisting => {
            item.kind = ItemKind::Skipped;
            item.note = Some(Note::KeepExisting);
        }
        ConflictPolicy::KeepNewer if existing_is_newer => {
            item.kind = ItemKind::Skipped;
            item.note = Some(Note::TargetNewer);
        }
        ConflictPolicy::KeepNewer => item.kind = ItemKind::Changed,
    }
}

/// Whether every value of `wanted` is already present with the same data.
fn contains_all(current: &RegistryExport, wanted: &RegistryExport) -> bool {
    wanted.keys.iter().all(|key| {
        current
            .keys
            .iter()
            .find(|k| k.path.eq_ignore_ascii_case(&key.path))
            .is_some_and(|existing| {
                key.values.iter().all(|value| {
                    existing.values.iter().any(|v| {
                        v.name.eq_ignore_ascii_case(&value.name)
                            && v.kind == value.kind
                            && v.data == value.data
                    })
                })
            })
    })
}
