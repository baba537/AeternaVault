//! Executes a confirmed [`BackupPlan`].

use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::{Local, Utc};

use super::fsops::{self, CopyError};
use super::manifest::{
    DATA_DIR, FORMAT_VERSION, FileEntry, FileIndex, HEADER_FILE, INDEX_FILE, SnapshotHeader,
    SnapshotStats, SnapshotStatus, SourceRecord,
};
use super::plan::{BackupPlan, ItemKind};
use super::{CancelToken, Phase, Progress, Reporter, safe_relative_path, to_unix_nanos};
use crate::config::BackupMode;
use crate::error::{EngineError, EngineResult, IoContext};
use crate::platform::{self, vss::FileReader};

#[derive(Debug, Clone)]
pub struct BackupReport {
    pub snapshot_dir: PathBuf,
    pub header: SnapshotHeader,
    pub warnings: Vec<String>,
    pub duration: Duration,
}

pub fn run_backup(
    plan: &BackupPlan,
    reader: &dyn FileReader,
    cancel: &CancelToken,
    on_progress: &mut dyn FnMut(&Progress),
) -> EngineResult<BackupReport> {
    let started = Instant::now();
    let started_at = Utc::now();
    let (files_total, bytes_total) = plan.copy_totals();

    if let Some(free) = platform::free_space(&plan.destination)
        && free < bytes_total
    {
        return Err(EngineError::NotEnoughSpace {
            needed: bytes_total,
            available: free,
        });
    }

    fsops::create_dir_all(&plan.computer_dir).at("could not create", &plan.computer_dir)?;
    let id = new_snapshot_id(&plan.computer_dir)?;
    let snapshot_dir = plan.computer_dir.join(&id);
    let data_dir = snapshot_dir.join(DATA_DIR);
    tracing::info!(
        snapshot = %snapshot_dir.display(),
        mode = ?plan.mode,
        reader = reader.describe(),
        files = files_total,
        bytes = bytes_total,
        "backup started"
    );

    let mut reporter = Reporter::new(on_progress);
    let mut progress = Progress {
        phase: Phase::Copying,
        files_total: plan.stored_files().count() as u64,
        bytes_total,
        ..Progress::default()
    };
    reporter.now(&progress);

    let mut entries = Vec::with_capacity(progress.files_total as usize);
    let mut warnings = plan.warnings.clone();
    let mut stats = SnapshotStats {
        skipped: plan.summary.count(ItemKind::Skipped),
        ..SnapshotStats::default()
    };
    let mut cancelled = false;

    for item in plan.stored_files() {
        if cancel.is_cancelled() {
            cancelled = true;
            break;
        }
        let source = &plan.sources[item.source];
        let Some(rel) = safe_relative_path(&item.rel) else {
            continue;
        };
        let src = source.root.join(&rel);
        let dst = data_dir.join(&source.key).join(&rel);
        let own_blob = format!("{id}/{DATA_DIR}/{}/{}", source.key, item.rel);
        progress.current = src.display().to_string();

        // Re-check the file: it may have changed since the preview.
        let read_path = reader.read_path(&src);
        let meta = match std::fs::metadata(&read_path) {
            Ok(meta) => meta,
            Err(err) => {
                warnings.push(format!("{}: {err}", src.display()));
                stats.failed += 1;
                progress.files_done += 1;
                continue;
            }
        };
        let size = meta.len();
        let modified = meta.modified().map(to_unix_nanos).unwrap_or(0);
        let unchanged_since_plan = size == item.size && modified == item.modified;

        // Incremental: reuse content of the previous backup for unchanged files.
        if plan.mode == BackupMode::Incremental
            && item.kind == ItemKind::Unchanged
            && unchanged_since_plan
            && let Some(base) = &item.blob
            && let Some(base_rel) = safe_relative_path(&base.blob)
        {
            let base_path = plan.computer_dir.join(base_rel);
            if base_path.is_file() {
                let blob = if plan.hardlink_unchanged && fsops::hard_link(&base_path, &dst).is_ok()
                {
                    stats.linked_files += 1;
                    own_blob
                } else {
                    // File systems without hard links (FAT32/exFAT, many NAS shares):
                    // point to the existing copy instead.
                    stats.referenced_files += 1;
                    base.blob.clone()
                };
                entries.push(FileEntry {
                    source: source.key.clone(),
                    path: item.rel.clone(),
                    size,
                    modified,
                    sha256: base.sha256.clone(),
                    blob,
                });
                stats.files += 1;
                stats.bytes += size;
                progress.files_done += 1;
                reporter.maybe(&progress);
                continue;
            }
        }

        let copy = fsops::copy_hashed(&read_path, &dst, None, cancel, &mut |n| {
            progress.bytes_done += n;
            reporter.maybe(&progress);
        });
        match copy {
            Ok(sha256) => {
                if let Err(err) = fsops::set_modified(&dst, modified) {
                    tracing::debug!(
                        "could not set modification time on {}: {err}",
                        dst.display()
                    );
                }
                entries.push(FileEntry {
                    source: source.key.clone(),
                    path: item.rel.clone(),
                    size,
                    modified,
                    sha256,
                    blob: own_blob,
                });
                stats.files += 1;
                stats.bytes += size;
                stats.copied_files += 1;
                stats.copied_bytes += size;
            }
            Err(CopyError::Cancelled) => {
                cancelled = true;
                break;
            }
            Err(err) => {
                tracing::warn!("could not back up {}: {err}", src.display());
                warnings.push(format!("{}: {err}", src.display()));
                stats.failed += 1;
            }
        }
        progress.files_done += 1;
        reporter.maybe(&progress);
    }

    progress.phase = Phase::Finishing;
    progress.current.clear();
    reporter.now(&progress);

    let status = if cancelled {
        SnapshotStatus::Cancelled
    } else if stats.failed > 0 {
        SnapshotStatus::CompleteWithWarnings
    } else {
        SnapshotStatus::Complete
    };

    let header = SnapshotHeader {
        format: FORMAT_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        id: id.clone(),
        computer: plan.computer.clone(),
        user: platform::user_name(),
        mode: plan.mode,
        status,
        started_at,
        finished_at: Some(Utc::now()),
        base: plan.base.as_ref().map(|b| b.id.clone()),
        sources: plan
            .sources
            .iter()
            .map(|s| SourceRecord {
                key: s.key.clone(),
                name: s.name.clone(),
                path: s.root.clone(),
            })
            .collect(),
        stats,
    };

    // The index is written first and the header last: a folder without a
    // header is recognisable as an unfinished backup.
    let index = FileIndex {
        format: FORMAT_VERSION,
        files: entries,
        warnings: warnings.clone(),
    };
    let index_path = snapshot_dir.join(INDEX_FILE);
    fsops::write_json_atomic(&index_path, &index).at("could not write", &index_path)?;
    let header_path = snapshot_dir.join(HEADER_FILE);
    fsops::write_json_atomic(&header_path, &header).at("could not write", &header_path)?;

    tracing::info!(
        status = ?header.status,
        files = header.stats.files,
        copied = header.stats.copied_files,
        linked = header.stats.linked_files,
        referenced = header.stats.referenced_files,
        failed = header.stats.failed,
        "backup finished"
    );

    Ok(BackupReport {
        snapshot_dir,
        header,
        warnings,
        duration: started.elapsed(),
    })
}

fn new_snapshot_id(computer_dir: &std::path::Path) -> EngineResult<String> {
    let base = Local::now().format("%Y-%m-%d_%H%M%S").to_string();
    for n in 1..1000 {
        let id = if n == 1 {
            base.clone()
        } else {
            format!("{base}-{n}")
        };
        let dir = computer_dir.join(&id);
        match fsops::create_new_dir(&dir) {
            Ok(()) => return Ok(id),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => {
                return Err(EngineError::io(
                    format!("could not create {}", dir.display()),
                    err,
                ));
            }
        }
    }
    Err(EngineError::io(
        "could not find a free snapshot name",
        std::io::Error::from(std::io::ErrorKind::AlreadyExists),
    ))
}
