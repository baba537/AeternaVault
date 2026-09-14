//! Preview / dry-run: computes what a backup or restore *would* do.
//!
//! This module is strictly read-only (enforced by a test in `engine/mod.rs`).
//! The resulting plans are shown to the user, can be exported, and are only
//! executed after explicit confirmation.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::manifest::{self, FileEntry, SnapshotHeader};
use super::scan::{self, Excludes, ScanOptions, SkipReason};
use super::snapshots::{self, SnapshotInfo};
use super::{
    CancelToken, Phase, Progress, Reporter, path_is_within, paths_equal, safe_relative_path,
    to_unix_nanos,
};
use crate::config::{BackupMode, Config, ConflictPolicy};
use crate::error::{EngineError, EngineResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemKind {
    /// Backup: not in the previous backup. Restore: target file does not exist.
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
    /// Relative path (`/`-separated) or, for some skipped entries, a full path.
    pub rel: String,
    pub size: u64,
    pub modified: i64,
    pub note: Option<Note>,
    pub blob: Option<BlobRef>,
}

#[derive(Debug, Clone)]
pub struct PlannedSource {
    pub name: String,
    /// Backup: the source folder. Restore: the folder files are written to.
    pub root: PathBuf,
    /// Folder name below `data\` in the snapshot.
    pub key: String,
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
        let root = &self.sources()[item.source].root;
        match safe_relative_path(&item.rel) {
            Some(rel) => root.join(rel),
            None => PathBuf::from(&item.rel),
        }
    }
}

// ---------------------------------------------------------------------------
// Backup
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BackupPlan {
    pub mode: BackupMode,
    pub destination: PathBuf,
    pub computer: String,
    pub computer_dir: PathBuf,
    pub base: Option<SnapshotHeader>,
    pub hardlink_unchanged: bool,
    pub sources: Vec<PlannedSource>,
    pub items: Vec<PlanItem>,
    pub summary: Summary,
    pub excluded: u64,
    pub warnings: Vec<String>,
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
        match item.kind {
            ItemKind::New | ItemKind::Changed => true,
            ItemKind::Unchanged => self.mode == BackupMode::Full || item.blob.is_none(),
            ItemKind::Removed | ItemKind::Skipped => false,
        }
    }

    /// Files that end up in the new snapshot.
    pub fn stored_files(&self) -> impl Iterator<Item = &PlanItem> {
        self.items.iter().filter(|i| {
            matches!(
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

pub fn plan_backup(
    config: &Config,
    computer: &str,
    cancel: &CancelToken,
    on_progress: &mut dyn FnMut(&Progress),
) -> EngineResult<BackupPlan> {
    let enabled: Vec<_> = config
        .enabled_sources()
        .filter(|s| !s.path.as_os_str().is_empty())
        .collect();
    if enabled.is_empty() {
        return Err(EngineError::NoSources);
    }
    let destination = config.destination.clone();
    if destination.as_os_str().is_empty() {
        return Err(EngineError::NoDestination);
    }
    if !snapshots::destination_reachable(&destination) {
        return Err(EngineError::DestinationUnavailable(destination));
    }
    for source in &enabled {
        if path_is_within(&source.path, &destination) {
            return Err(EngineError::SourceInsideDestination {
                source_path: source.path.clone(),
                destination,
            });
        }
    }

    let mut warnings = Vec::new();
    let computer_dir = destination.join(computer);

    // Load the previous backup to compare against.
    let base_snapshot = snapshots::latest_usable(&destination, computer);
    let (base, base_index) = match &base_snapshot {
        Some(info) => match manifest::read_index(&info.dir) {
            Ok(index) => (info.header.clone(), Some(index)),
            Err(err) => {
                warnings.push(format!(
                    "previous backup could not be read, all files count as new: {err}"
                ));
                (None, None)
            }
        },
        None => (None, None),
    };

    // Lookup: lowercase source path -> lowercase relative path -> entry.
    let mut base_lookup: HashMap<String, HashMap<String, &FileEntry>> = HashMap::new();
    if let (Some(header), Some(index)) = (&base, &base_index) {
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
    }

    let mut plan = BackupPlan {
        mode: config.mode,
        destination: destination.clone(),
        computer: computer.to_string(),
        computer_dir: computer_dir.clone(),
        base: base.clone(),
        hardlink_unchanged: config.advanced.hardlink_unchanged,
        sources: Vec::new(),
        items: Vec::new(),
        summary: Summary::default(),
        excluded: 0,
        warnings,
    };

    let mut used_keys = HashSet::new();
    let mut progress = Progress::default();
    let mut reporter = Reporter::new(on_progress);

    for source in enabled {
        if plan
            .sources
            .iter()
            .any(|s| paths_equal(&s.root, &source.path))
        {
            continue;
        }
        let source_index = plan.sources.len();
        plan.sources.push(PlannedSource {
            name: source.display_name(),
            root: source.path.clone(),
            key: unique_key(&source.display_name(), &mut used_keys),
        });

        let excludes = Excludes::new(config.exclude.iter().chain(source.exclude.iter()));
        let options = ScanOptions {
            excludes: &excludes,
            skip_online_only: config.advanced.skip_online_only_files,
            destination: &destination,
        };
        progress.current = source.path.display().to_string();
        reporter.now(&progress);
        let scanned =
            scan::scan_source(&source.path, &options, cancel, &mut progress, &mut reporter)?;
        plan.excluded += scanned.excluded;

        let previous = base_lookup.get(&path_key(&source.path));
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
            let item = PlanItem {
                kind,
                source: source_index,
                rel: file.rel,
                size: file.size,
                modified: file.modified,
                note: None,
                blob,
            };
            plan.summary.add(&item);
            plan.items.push(item);
        }

        for skipped in scanned.skipped {
            let rel = skipped
                .path
                .strip_prefix(&source.path)
                .map(super::rel_to_string)
                .unwrap_or_else(|_| skipped.path.display().to_string());
            let item = PlanItem {
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
            };
            plan.summary.add(&item);
            plan.items.push(item);
        }

        if let Some(previous) = previous {
            let mut removed: Vec<_> = previous
                .iter()
                .filter(|(lower, _)| !seen.contains(*lower))
                .map(|(_, e)| *e)
                .collect();
            removed.sort_by(|a, b| a.path.cmp(&b.path));
            for entry in removed {
                let item = PlanItem {
                    kind: ItemKind::Removed,
                    source: source_index,
                    rel: entry.path.clone(),
                    size: entry.size,
                    modified: entry.modified,
                    note: None,
                    blob: None,
                };
                plan.summary.add(&item);
                plan.items.push(item);
            }
        }
    }

    progress.phase = Phase::Finishing;
    reporter.now(&progress);
    Ok(plan)
}

fn path_key(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect::<Vec<_>>()
        .join("\\")
}

/// Folder-safe, unique name for a source inside the snapshot.
fn unique_key(name: &str, used: &mut HashSet<String>) -> String {
    let mut base: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | ' ' | '.' | '+') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim()
        .trim_end_matches('.')
        .to_string();
    if base.is_empty() {
        base = "source".to_string();
    }
    let mut key = base.clone();
    let mut n = 2;
    while !used.insert(key.to_lowercase()) {
        key = format!("{base} ({n})");
        n += 1;
    }
    key
}

// ---------------------------------------------------------------------------
// Restore
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreTarget {
    /// Back to where the files came from.
    Original,
    /// Into `<folder>\<source name>\...`.
    Folder(PathBuf),
}

#[derive(Debug, Clone)]
pub struct RestoreOptions {
    pub target: RestoreTarget,
    pub conflict: ConflictPolicy,
    pub verify: bool,
}

#[derive(Debug, Clone)]
pub struct RestorePlan {
    pub snapshot: SnapshotInfo,
    pub options: RestoreOptions,
    pub sources: Vec<PlannedSource>,
    pub items: Vec<PlanItem>,
    pub summary: Summary,
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

pub fn plan_restore(
    snapshot: &SnapshotInfo,
    options: RestoreOptions,
    cancel: &CancelToken,
    on_progress: &mut dyn FnMut(&Progress),
) -> EngineResult<RestorePlan> {
    let header = snapshot
        .header
        .clone()
        .ok_or_else(|| EngineError::SnapshotNotFound(snapshot.qualified_id()))?;
    let index = manifest::read_index(&snapshot.dir)?;

    let mut used_keys = HashSet::new();
    let sources: Vec<PlannedSource> = header
        .sources
        .iter()
        .map(|record| {
            let root = match &options.target {
                RestoreTarget::Original => record.path.clone(),
                RestoreTarget::Folder(folder) => {
                    folder.join(unique_key(&record.name, &mut used_keys))
                }
            };
            PlannedSource {
                name: record.name.clone(),
                root,
                key: record.key.clone(),
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
        };

        let (Some(rel), Some(blob_rel)) = (
            safe_relative_path(&entry.path),
            safe_relative_path(&entry.blob),
        ) else {
            item.kind = ItemKind::Skipped;
            item.note = Some(Note::UnsafePath);
            plan.summary.add(&item);
            plan.items.push(item);
            continue;
        };

        let target = sources[source].root.join(&rel);
        if progress.files_done % 64 == 0 {
            progress.current = target.display().to_string();
            reporter.maybe(&progress);
        }

        if !snapshot.computer_dir.join(blob_rel).is_file() {
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
                        match plan.options.conflict {
                            ConflictPolicy::ReplaceChanged => item.kind = ItemKind::Changed,
                            ConflictPolicy::KeepExisting => {
                                item.kind = ItemKind::Skipped;
                                item.note = Some(Note::KeepExisting);
                            }
                            ConflictPolicy::KeepNewer if existing_modified > entry.modified => {
                                item.kind = ItemKind::Skipped;
                                item.note = Some(Note::TargetNewer);
                            }
                            ConflictPolicy::KeepNewer => item.kind = ItemKind::Changed,
                        }
                    }
                }
            }
        }

        plan.summary.add(&item);
        plan.items.push(item);
    }

    progress.phase = Phase::Finishing;
    reporter.now(&progress);
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_safe() {
        let mut used = HashSet::new();
        assert_eq!(unique_key("Documents", &mut used), "Documents");
        assert_eq!(unique_key("documents", &mut used), "documents (2)");
        assert_eq!(unique_key("A:B/C", &mut used), "A_B_C");
        assert_eq!(unique_key("..", &mut used), "source");
    }
}
