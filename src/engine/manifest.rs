//! On-disk format of a backup ("snapshot"). Read-only helpers only.
//!
//! ```text
//! <destination>\
//!   <COMPUTERNAME>\
//!     2026-09-14_143205\
//!       snapshot.json   header: status, times, sources, statistics (written last)
//!       files.json      index: one entry per file incl. SHA-256
//!       data\
//!         <source key>\<original relative path>
//! ```
//!
//! Every backup folder is a plain directory tree that can be browsed and
//! copied with Explorer, even without AeternaVault. JSON keeps the index
//! human-readable; a compact format (e.g. SQLite or zstd-compressed JSON) is a
//! possible later optimisation for very large backups.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::BackupMode;
use crate::error::{EngineError, EngineResult};

pub const FORMAT_VERSION: u32 = 1;
pub const HEADER_FILE: &str = "snapshot.json";
pub const INDEX_FILE: &str = "files.json";
pub const DATA_DIR: &str = "data";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnapshotStatus {
    Complete,
    CompleteWithWarnings,
    Cancelled,
    Failed,
}

impl SnapshotStatus {
    /// Whether this snapshot may serve as the base of an incremental backup.
    pub fn is_usable_base(self) -> bool {
        matches!(self, Self::Complete | Self::CompleteWithWarnings)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRecord {
    /// Folder name below `data\` in this snapshot.
    pub key: String,
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SnapshotStats {
    pub files: u64,
    pub bytes: u64,
    pub copied_files: u64,
    pub copied_bytes: u64,
    pub linked_files: u64,
    pub referenced_files: u64,
    pub skipped: u64,
    pub failed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotHeader {
    pub format: u32,
    pub app_version: String,
    pub id: String,
    pub computer: String,
    pub user: String,
    pub mode: BackupMode,
    pub status: SnapshotStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    /// Snapshot that unchanged files were compared against.
    pub base: Option<String>,
    pub sources: Vec<SourceRecord>,
    pub stats: SnapshotStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Source key (see [`SourceRecord::key`]).
    pub source: String,
    /// Path relative to the source folder, `/`-separated.
    pub path: String,
    pub size: u64,
    /// Last-modified time in nanoseconds since the Unix epoch.
    pub modified: i64,
    pub sha256: String,
    /// Where the file content physically lives, relative to the computer
    /// folder, e.g. `2026-09-12_101010/data/Documents/letter.docx`. For
    /// incremental backups on file systems without hard links this points
    /// into an older snapshot.
    pub blob: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileIndex {
    pub format: u32,
    pub files: Vec<FileEntry>,
    pub warnings: Vec<String>,
}

pub fn read_header(snapshot_dir: &Path) -> EngineResult<SnapshotHeader> {
    read_json(&snapshot_dir.join(HEADER_FILE))
}

pub fn read_index(snapshot_dir: &Path) -> EngineResult<FileIndex> {
    read_json(&snapshot_dir.join(INDEX_FILE))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> EngineResult<T> {
    let file = std::fs::File::open(path).map_err(|e| EngineError::Manifest {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    serde_json::from_reader(std::io::BufReader::new(file)).map_err(|e| EngineError::Manifest {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}
