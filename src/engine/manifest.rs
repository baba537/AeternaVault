//! On-disk format of a backup ("snapshot"). Read-only helpers only.
//!
//! Format 2 (AeternaVault 0.2+), plain:
//! ```text
//! <destination>\
//!   2026-09-14 20-00\              one folder per backup, newest last
//!     Documents\…                  your folders, directly browsable
//!     Desktop\…
//!     Applications\
//!       Firefox\…                  application settings
//!       7-Zip\Registry\….reg       registry settings (double-click to import)
//!       Installed programs.txt
//!     .aeternavault\               hidden: snapshot.json, files.json
//! ```
//! If a destination is shared by several computers, backups of other
//! computers get the computer name appended: `2026-09-14 20-00 (LAPTOP)`.
//!
//! Format 2, encrypted: see [`super::vault`].
//!
//! Format 1 (AeternaVault 0.1): `<destination>\<COMPUTER>\<id>\data\<source>\…`
//! with `snapshot.json` next to `data`. Still listed and restorable.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::crypto::VaultKey;
use crate::config::BackupMode;
use crate::error::{EngineError, EngineResult};
use crate::platform::known_paths::KnownPaths;
use crate::platform::registry::RegistryExport;

pub const FORMAT_VERSION: u32 = 2;
pub const HEADER_FILE: &str = "snapshot.json";
pub const INDEX_FILE: &str = "files.json";
pub const META_DIR: &str = ".aeternavault";
pub const LEGACY_DATA_DIR: &str = "data";
pub const APPS_DIR: &str = "Applications";
pub const PROGRAM_LIST_FILE: &str = "Installed programs.txt";
pub const WINGET_FILE: &str = "winget-packages.json";

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRecord {
    /// Folder inside the snapshot, `/`-separated (e.g. `Applications/Firefox`).
    pub key: String,
    pub name: String,
    /// Absolute path on the computer that made the backup.
    pub path: PathBuf,
    /// Portable form (`{APPDATA}\…`) used to restore on another computer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub portable: Option<String>,
    /// Catalog id if this folder belongs to an application.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    /// Generated content (program list) without an original location.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub extra: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryRecord {
    pub app: String,
    pub app_name: String,
    /// Full key, e.g. `HKCU\Software\7-Zip`.
    pub key: String,
    /// Human-readable `.reg` copy, relative to the snapshot folder (plain backups).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reg_file: Option<String>,
    pub values: usize,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SnapshotStats {
    pub files: u64,
    pub bytes: u64,
    pub copied_files: u64,
    pub copied_bytes: u64,
    pub linked_files: u64,
    pub referenced_files: u64,
    #[serde(default)]
    pub registry_keys: u64,
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
    #[serde(default)]
    pub encrypted: bool,
    pub sources: Vec<SourceRecord>,
    #[serde(default)]
    pub registry: Vec<RegistryRecord>,
    /// Known folders of the backed-up user, for remapping on restore.
    #[serde(default)]
    pub known_paths: KnownPaths,
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
    /// Where the content lives:
    /// * format 2 plain: relative to the destination, e.g. `2026-09-12 10-10/Documents/a.txt`
    ///   (may point into an older backup on file systems without hard links),
    /// * format 2 encrypted: the blob id,
    /// * format 1: relative to the computer folder.
    pub blob: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileIndex {
    pub format: u32,
    pub files: Vec<FileEntry>,
    #[serde(default)]
    pub registry: Vec<RegistryExport>,
    pub warnings: Vec<String>,
}

/// Content of an encrypted `.avs` snapshot file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedSnapshot {
    pub header: SnapshotHeader,
    pub index: FileIndex,
}

pub fn read_header(meta_dir: &Path) -> EngineResult<SnapshotHeader> {
    read_json(&meta_dir.join(HEADER_FILE))
}

pub fn read_index(meta_dir: &Path) -> EngineResult<FileIndex> {
    read_json(&meta_dir.join(INDEX_FILE))
}

pub fn read_encrypted(path: &Path, key: &VaultKey) -> EngineResult<EncryptedSnapshot> {
    let manifest_error = |message: String| EngineError::Manifest {
        path: path.to_path_buf(),
        message,
    };
    let sealed = std::fs::read(path).map_err(|e| manifest_error(e.to_string()))?;
    let plain = key
        .decrypt_bytes(&sealed)
        .map_err(|e| manifest_error(e.to_string()))?;
    serde_json::from_slice(&plain).map_err(|e| manifest_error(e.to_string()))
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
