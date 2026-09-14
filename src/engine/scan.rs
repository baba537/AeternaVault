//! Walks a source folder and collects the files to consider. Read-only.
//!
//! Decisions made here:
//! * Symbolic links and junctions are never followed. They cause loops and
//!   duplicates (older profiles contain junctions such as `My Music` inside
//!   `Documents`). They are listed as skipped so nothing disappears silently.
//! * Cloud placeholders (OneDrive "online-only" files) are skipped by default,
//!   because reading them would download them.
//! * The destination folder is skipped if it lies inside a source.
//! * Access errors do not abort the scan; they become skipped entries.

use std::path::{Path, PathBuf};

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use walkdir::WalkDir;

use super::{CancelToken, Phase, Progress, Reporter, path_is_within, rel_to_string, to_unix_nanos};
use crate::error::{EngineError, EngineResult};

#[derive(Debug, Clone)]
pub struct ScannedFile {
    /// Relative path, `/`-separated.
    pub rel: String,
    pub size: u64,
    pub modified: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    Link,
    OnlineOnly,
    InsideDestination,
    SourceMissing,
    NoAccess(String),
}

#[derive(Debug, Clone)]
pub struct SkippedEntry {
    pub path: PathBuf,
    pub reason: SkipReason,
}

#[derive(Debug, Default)]
pub struct ScanResult {
    pub files: Vec<ScannedFile>,
    pub skipped: Vec<SkippedEntry>,
    pub excluded: u64,
}

pub struct ScanOptions<'a> {
    pub excludes: &'a Excludes,
    pub skip_online_only: bool,
    pub destination: &'a Path,
}

/// Compiled exclude patterns, matched against names and relative paths.
pub struct Excludes {
    set: GlobSet,
    pub invalid: Vec<String>,
}

impl Excludes {
    pub fn new<'a>(patterns: impl IntoIterator<Item = &'a String>) -> Self {
        let mut builder = GlobSetBuilder::new();
        let mut invalid = Vec::new();
        for pattern in patterns {
            let pattern = pattern.trim();
            if pattern.is_empty() || pattern.starts_with('#') {
                continue;
            }
            match GlobBuilder::new(&pattern.replace('\\', "/"))
                .case_insensitive(true)
                .literal_separator(false)
                .build()
            {
                Ok(glob) => {
                    builder.add(glob);
                }
                Err(err) => {
                    tracing::warn!("ignoring invalid exclude pattern `{pattern}`: {err}");
                    invalid.push(pattern.to_string());
                }
            }
        }
        let set = builder.build().unwrap_or_else(|_| GlobSet::empty());
        Self { set, invalid }
    }

    pub fn is_excluded(&self, name: &str, rel: &str) -> bool {
        !self.set.is_empty() && (self.set.is_match(name) || self.set.is_match(rel))
    }
}

// Win32 file attribute bits (see `GetFileAttributesW` documentation).
#[cfg(windows)]
const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
#[cfg(windows)]
const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
#[cfg(windows)]
const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;

fn is_online_only(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes()
            & (FILE_ATTRIBUTE_OFFLINE
                | FILE_ATTRIBUTE_RECALL_ON_OPEN
                | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS)
            != 0
    }
    #[cfg(not(windows))]
    {
        let _ = meta;
        false
    }
}

pub fn scan_source(
    root: &Path,
    options: &ScanOptions<'_>,
    cancel: &CancelToken,
    progress: &mut Progress,
    reporter: &mut Reporter<'_>,
) -> EngineResult<ScanResult> {
    let mut result = ScanResult::default();

    if !root.is_dir() {
        result.skipped.push(SkippedEntry {
            path: root.to_path_buf(),
            reason: SkipReason::SourceMissing,
        });
        return Ok(result);
    }

    let mut walker = WalkDir::new(root).follow_links(false).into_iter();
    while let Some(next) = walker.next() {
        if cancel.is_cancelled() {
            return Err(EngineError::Cancelled);
        }

        let entry = match next {
            Ok(entry) => entry,
            Err(err) => {
                let path = err
                    .path()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| root.to_path_buf());
                result.skipped.push(SkippedEntry {
                    path,
                    reason: SkipReason::NoAccess(err.to_string()),
                });
                continue;
            }
        };
        if entry.depth() == 0 {
            continue;
        }

        let path = entry.path();
        let Ok(rel_path) = path.strip_prefix(root) else {
            continue;
        };
        let rel = rel_to_string(rel_path);
        let name = entry.file_name().to_string_lossy();
        let file_type = entry.file_type();

        if file_type.is_symlink() {
            result.skipped.push(SkippedEntry {
                path: path.to_path_buf(),
                reason: SkipReason::Link,
            });
            continue;
        }

        if file_type.is_dir() {
            if path_is_within(path, options.destination) {
                result.skipped.push(SkippedEntry {
                    path: path.to_path_buf(),
                    reason: SkipReason::InsideDestination,
                });
                walker.skip_current_dir();
            } else if options.excludes.is_excluded(&name, &rel) {
                result.excluded += 1;
                walker.skip_current_dir();
            }
            continue;
        }

        if options.excludes.is_excluded(&name, &rel) {
            result.excluded += 1;
            continue;
        }

        let meta = match entry.metadata() {
            Ok(meta) => meta,
            Err(err) => {
                result.skipped.push(SkippedEntry {
                    path: path.to_path_buf(),
                    reason: SkipReason::NoAccess(err.to_string()),
                });
                continue;
            }
        };

        if options.skip_online_only && is_online_only(&meta) {
            result.skipped.push(SkippedEntry {
                path: path.to_path_buf(),
                reason: SkipReason::OnlineOnly,
            });
            continue;
        }

        let modified = meta.modified().map(to_unix_nanos).unwrap_or(0);
        result.files.push(ScannedFile {
            rel,
            size: meta.len(),
            modified,
        });

        progress.phase = Phase::Scanning;
        progress.files_done += 1;
        progress.bytes_done += meta.len();
        if progress.files_done % 64 == 0 {
            progress.current = path.display().to_string();
            reporter.maybe(progress);
        }
    }

    Ok(result)
}

/// Quick size estimate of a folder for the source list (no excludes, errors ignored).
pub fn folder_size(root: &Path, cancel: &CancelToken) -> Option<(u64, u64)> {
    let mut bytes = 0u64;
    let mut files = 0u64;
    for entry in WalkDir::new(root).follow_links(false).into_iter().flatten() {
        if cancel.is_cancelled() {
            return None;
        }
        if entry.file_type().is_file()
            && let Ok(meta) = entry.metadata()
        {
            bytes += meta.len();
            files += 1;
        }
    }
    Some((bytes, files))
}
