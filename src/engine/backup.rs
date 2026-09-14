//! Executes a confirmed [`BackupPlan`].

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{Local, Utc};

use super::crypto::VaultKey;
use super::fsops::{self, CopyError, DestinationLock};
use super::manifest::{
    APPS_DIR, EncryptedSnapshot, FORMAT_VERSION, FileEntry, FileIndex, HEADER_FILE, INDEX_FILE,
    META_DIR, PROGRAM_LIST_FILE, RegistryRecord, SnapshotHeader, SnapshotStats, SnapshotStatus,
    SourceRecord, WINGET_FILE,
};
use super::plan::{BackupPlan, ItemKind};
use super::sources::sanitize_segment;
use super::{
    CancelToken, Phase, Progress, Reporter, safe_relative_path, snapshots, to_unix_nanos, vault,
};
use crate::config::BackupMode;
use crate::error::{EngineError, EngineResult, IoContext};
use crate::platform::known_paths::KnownPaths;
use crate::platform::{self, apps, registry, vss::FileReader};

#[derive(Debug, Clone)]
pub struct BackupReport {
    /// Folder of the new backup (plain) or the vault folder (encrypted).
    pub snapshot_dir: PathBuf,
    pub header: SnapshotHeader,
    pub warnings: Vec<String>,
    pub duration: Duration,
}

/// Where the data of one backup goes.
enum Store<'a> {
    Plain { snapshot_dir: PathBuf },
    Encrypted { key: &'a VaultKey },
}

pub fn run_backup(
    plan: &BackupPlan,
    key: Option<&VaultKey>,
    reader: &dyn FileReader,
    cancel: &CancelToken,
    on_progress: &mut dyn FnMut(&Progress),
) -> EngineResult<BackupReport> {
    let started = Instant::now();
    let started_at = Utc::now();
    let (files_total, bytes_total) = plan.copy_totals();

    let _lock = DestinationLock::acquire(&plan.destination)
        .at("could not prepare", &plan.destination)?
        .ok_or(EngineError::AlreadyRunning)?;

    if let Some(free) = platform::free_space(&plan.destination)
        && free < bytes_total
    {
        return Err(EngineError::NotEnoughSpace {
            needed: bytes_total,
            available: free,
        });
    }

    let id = new_snapshot_id(plan)?;
    let store = if plan.encrypted {
        let key = key.ok_or(EngineError::Locked)?;
        Store::Encrypted { key }
    } else {
        let snapshot_dir = plan.destination.join(&id);
        let meta = snapshot_dir.join(META_DIR);
        fsops::create_dir_all(&meta).at("could not create", &meta)?;
        platform::set_hidden(&meta);
        Store::Plain { snapshot_dir }
    };

    tracing::info!(
        snapshot = %id,
        encrypted = plan.encrypted,
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
        let reuse_base = plan.mode == BackupMode::Incremental
            && item.kind == ItemKind::Unchanged
            && unchanged_since_plan;

        let outcome: Result<(String, String), CopyError> = match &store {
            Store::Plain { snapshot_dir } => {
                let dst = snapshot_dir.join(key_path(&source.key)).join(&rel);
                let own_blob = format!("{id}/{}/{}", source.key, item.rel);
                let base_path = item
                    .blob
                    .as_ref()
                    .filter(|_| reuse_base)
                    .and_then(|b| {
                        let root = plan.base.as_ref()?.blob_root()?;
                        Some((root.join(safe_relative_path(&b.blob)?), b))
                    })
                    .filter(|(path, _)| path.is_file());
                match base_path {
                    Some((base_path, base)) => {
                        if plan.hardlink_unchanged && fsops::hard_link(&base_path, &dst).is_ok() {
                            stats.linked_files += 1;
                            Ok((own_blob, base.sha256.clone()))
                        } else {
                            // File systems without hard links (FAT32/exFAT, many NAS
                            // shares): point to the existing copy instead.
                            stats.referenced_files += 1;
                            Ok((base.blob.clone(), base.sha256.clone()))
                        }
                    }
                    None => fsops::copy_hashed(&read_path, &dst, None, cancel, &mut |n| {
                        progress.bytes_done += n;
                        reporter.maybe(&progress);
                    })
                    .map(|sha| {
                        if let Err(err) = fsops::set_modified(&dst, modified) {
                            tracing::debug!(
                                "could not set modification time on {}: {err}",
                                dst.display()
                            );
                        }
                        stats.copied_files += 1;
                        stats.copied_bytes += size;
                        (own_blob, sha)
                    }),
                }
            }
            Store::Encrypted { key } => {
                let existing = item.blob.as_ref().filter(|b| {
                    reuse_base && vault::blob_path(&plan.destination, &b.blob).is_file()
                });
                match existing {
                    Some(base) => {
                        stats.referenced_files += 1;
                        Ok((base.blob.clone(), base.sha256.clone()))
                    }
                    None => fsops::encrypt_into_vault(
                        key,
                        &plan.destination,
                        &read_path,
                        cancel,
                        &mut |n| {
                            progress.bytes_done += n;
                            reporter.maybe(&progress);
                        },
                    )
                    .map(|(blob, sha, stored_new)| {
                        if stored_new {
                            stats.copied_files += 1;
                            stats.copied_bytes += size;
                        } else {
                            stats.referenced_files += 1;
                        }
                        (blob, sha)
                    }),
                }
            }
        };

        match outcome {
            Ok((blob, sha256)) => {
                entries.push(FileEntry {
                    source: source.key.clone(),
                    path: item.rel.clone(),
                    size,
                    modified,
                    sha256,
                    blob,
                });
                stats.files += 1;
                stats.bytes += size;
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

    let mut source_records: Vec<SourceRecord> = plan
        .sources
        .iter()
        .filter(|s| !s.root.as_os_str().is_empty())
        .map(|s| SourceRecord {
            key: s.key.clone(),
            name: s.name.clone(),
            path: s.root.clone(),
            portable: s.portable.clone(),
            app: s.app.clone(),
            extra: false,
        })
        .collect();

    // Registry settings (exported again now, so they match the moment of the backup).
    let mut registry_exports = Vec::new();
    let mut registry_records = Vec::new();
    if !cancelled {
        for planned in &plan.registry {
            match registry::export(&planned.key) {
                Ok(Some(export)) => {
                    let mut reg_file = None;
                    if let Store::Plain { snapshot_dir } = &store {
                        let folder = super::plan::registry_folder_key(&planned.app_name);
                        let file_name = format!(
                            "{}.reg",
                            sanitize_segment(&export.root.replace('\\', " - "))
                        );
                        let path = snapshot_dir.join(key_path(&folder)).join(&file_name);
                        match fsops::write_utf16_file(&path, &export.to_reg_text()) {
                            Ok(()) => reg_file = Some(format!("{folder}/{file_name}")),
                            Err(err) => warnings.push(format!("{}: {err}", path.display())),
                        }
                    }
                    registry_records.push(RegistryRecord {
                        app: planned.app.clone(),
                        app_name: planned.app_name.clone(),
                        key: export.root.clone(),
                        reg_file,
                        values: export.value_count(),
                        fingerprint: export.fingerprint(),
                    });
                    stats.registry_keys += 1;
                    registry_exports.push(export);
                }
                Ok(None) => {}
                Err(err) => {
                    warnings.push(format!("{}: {err}", planned.key));
                    stats.failed += 1;
                }
            }
        }
    }

    // A list of installed programs, to make reinstalling on a new computer easier.
    if plan.save_program_list && !cancelled {
        add_program_list(
            &store,
            plan,
            &id,
            &mut source_records,
            &mut entries,
            &mut warnings,
        );
    }

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
        encrypted: plan.encrypted,
        sources: source_records,
        registry: registry_records,
        known_paths: KnownPaths::current(),
        stats,
    };
    let index = FileIndex {
        format: FORMAT_VERSION,
        files: entries,
        registry: registry_exports,
        warnings: warnings.clone(),
    };

    let snapshot_dir = match &store {
        Store::Plain { snapshot_dir } => {
            // The index is written first and the header last: a folder without a
            // header is recognisable as an unfinished backup.
            let meta = snapshot_dir.join(META_DIR);
            let index_path = meta.join(INDEX_FILE);
            fsops::write_json_atomic(&index_path, &index).at("could not write", &index_path)?;
            let header_path = meta.join(HEADER_FILE);
            fsops::write_json_atomic(&header_path, &header).at("could not write", &header_path)?;
            snapshot_dir.clone()
        }
        Store::Encrypted { key } => {
            let sealed = serde_json::to_vec(&EncryptedSnapshot {
                header: header.clone(),
                index,
            })
            .map_err(|e| {
                EngineError::io(
                    "could not encode the backup index",
                    std::io::Error::other(e),
                )
            })?;
            let path = vault::snapshot_path(&plan.destination, &id);
            fsops::write_bytes_atomic(&path, &key.encrypt_bytes(&sealed))
                .at("could not write", &path)?;
            vault::vault_dir(&plan.destination)
        }
    };

    tracing::info!(
        status = ?header.status,
        files = header.stats.files,
        copied = header.stats.copied_files,
        linked = header.stats.linked_files,
        referenced = header.stats.referenced_files,
        registry = header.stats.registry_keys,
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

fn add_program_list(
    store: &Store<'_>,
    plan: &BackupPlan,
    id: &str,
    records: &mut Vec<SourceRecord>,
    entries: &mut Vec<FileEntry>,
    warnings: &mut Vec<String>,
) {
    let programs = apps::installed_apps();
    if programs.is_empty() {
        return;
    }
    records.push(SourceRecord {
        key: APPS_DIR.to_string(),
        name: APPS_DIR.to_string(),
        path: PathBuf::new(),
        portable: None,
        app: None,
        extra: true,
    });

    let temp = tempfile_path(plan, id);
    let mut files: Vec<(&str, Vec<u8>)> = vec![(
        PROGRAM_LIST_FILE,
        apps::program_list_text(&programs).into_bytes(),
    )];
    if apps::winget_export(&temp)
        && let Ok(bytes) = std::fs::read(&temp)
    {
        files.push((WINGET_FILE, bytes));
    }
    let _ = fsops::remove_file(&temp);

    let now = to_unix_nanos(std::time::SystemTime::now());
    for (name, bytes) in files {
        let stored = match store {
            Store::Plain { snapshot_dir } => {
                let path = snapshot_dir.join(APPS_DIR).join(name);
                fsops::write_bytes_atomic(&path, &bytes)
                    .map(|()| {
                        (
                            format!("{id}/{APPS_DIR}/{name}"),
                            super::format_sha256(&sha(&bytes)),
                        )
                    })
                    .map_err(|e| e.to_string())
            }
            Store::Encrypted { key } => {
                fsops::encrypt_bytes_into_vault(key, &plan.destination, &bytes)
                    .map_err(|e| e.to_string())
            }
        };
        match stored {
            Ok((blob, sha256)) => entries.push(FileEntry {
                source: APPS_DIR.to_string(),
                path: name.to_string(),
                size: bytes.len() as u64,
                modified: now,
                sha256,
                blob,
            }),
            Err(err) => warnings.push(format!("{name}: {err}")),
        }
    }
}

fn sha(bytes: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).into()
}

fn tempfile_path(plan: &BackupPlan, id: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "aeternavault-winget-{}-{}.json",
        std::process::id(),
        sanitize_segment(&format!("{}{id}", plan.computer))
    ))
}

/// `Applications/Firefox` → `Applications\Firefox`.
fn key_path(key: &str) -> PathBuf {
    key.split('/').collect()
}

/// `2026-09-14 20-00`, with ` (COMPUTER)` if other computers use the same
/// destination and `-2`, `-3` … if the minute is already taken.
fn new_snapshot_id(plan: &BackupPlan) -> EngineResult<String> {
    let base = Local::now().format("%Y-%m-%d %H-%M").to_string();
    let suffix = if snapshots::has_other_computers(&plan.destination, &plan.computer) {
        format!(" ({})", sanitize_segment(&plan.computer))
    } else {
        String::new()
    };
    for n in 1..1000 {
        let id = if n == 1 {
            format!("{base}{suffix}")
        } else {
            format!("{base}-{n}{suffix}")
        };
        if plan.encrypted {
            if !vault::snapshot_path(&plan.destination, &id).exists() {
                return Ok(id);
            }
            continue;
        }
        let dir = plan.destination.join(&id);
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
        "could not find a free backup name",
        std::io::Error::from(std::io::ErrorKind::AlreadyExists),
    ))
}

#[allow(dead_code)]
fn _assert_path(_: &Path) {}
