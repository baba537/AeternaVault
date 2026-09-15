//! Finding existing backups in a destination folder. Read-only.

use std::path::{Path, PathBuf};

use super::crypto::VaultKey;
use super::manifest::{self, FileIndex, HEADER_FILE, META_DIR, SnapshotHeader};
use super::vault;
use crate::error::{EngineError, EngineResult};

#[derive(Debug, Clone)]
pub enum Location {
    /// AeternaVault 0.1: `<destination>\<COMPUTER>\<id>\`.
    Legacy { dir: PathBuf, computer_dir: PathBuf },
    /// `<destination>\<id>\` with a hidden `.aeternavault` folder.
    Plain { dir: PathBuf, destination: PathBuf },
    /// `<destination>\AeternaVault Encrypted\snapshots\<id>.avs`.
    Encrypted { file: PathBuf, destination: PathBuf },
}

#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub id: String,
    pub computer: String,
    pub location: Location,
    /// `None` if the backup is unfinished, damaged, or encrypted and locked.
    pub header: Option<SnapshotHeader>,
    /// The encrypted part of a partly encrypted backup (this is the plain part).
    pub companion: Option<Box<SnapshotInfo>>,
}

impl SnapshotInfo {
    /// This backup and, if partly encrypted, its encrypted part.
    pub fn parts(&self) -> impl Iterator<Item = &SnapshotInfo> {
        std::iter::once(self).chain(self.companion.as_deref())
    }

    /// Whether the key is needed to see or restore everything.
    pub fn needs_key(&self) -> bool {
        self.parts().any(|p| p.is_encrypted())
    }

    /// Whether some part is encrypted and not unlocked.
    pub fn needs_unlock(&self) -> bool {
        self.parts().any(SnapshotInfo::is_locked)
    }

    pub fn is_split(&self) -> bool {
        self.companion.is_some()
    }

    /// Statistics of all parts together, if known.
    pub fn total_stats(&self) -> Option<super::manifest::SnapshotStats> {
        let mut total = self.header.as_ref()?.stats.clone();
        if let Some(companion) = &self.companion {
            let stats = &companion.header.as_ref()?.stats;
            total.add(stats);
        }
        Some(total)
    }
    /// Unique within a destination.
    pub fn qualified_id(&self) -> String {
        match &self.location {
            Location::Legacy { .. } => format!("{}/{}", self.computer, self.id),
            Location::Plain { .. } => self.id.clone(),
            Location::Encrypted { .. } => format!("encrypted/{}", self.id),
        }
    }

    pub fn is_encrypted(&self) -> bool {
        matches!(self.location, Location::Encrypted { .. })
    }

    pub fn is_locked(&self) -> bool {
        self.is_encrypted() && self.header.is_none()
    }

    pub fn is_usable_base(&self) -> bool {
        self.header
            .as_ref()
            .is_some_and(|h| h.status.is_usable_base())
    }

    /// Folder that plain `blob` paths are relative to.
    pub fn blob_root(&self) -> Option<&Path> {
        match &self.location {
            Location::Legacy { computer_dir, .. } => Some(computer_dir),
            Location::Plain { destination, .. } => Some(destination),
            Location::Encrypted { .. } => None,
        }
    }

    pub fn destination(&self) -> PathBuf {
        match &self.location {
            Location::Legacy { computer_dir, .. } => computer_dir
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default(),
            Location::Plain { destination, .. } | Location::Encrypted { destination, .. } => {
                destination.clone()
            }
        }
    }

    pub fn load_index(&self, key: Option<&VaultKey>) -> EngineResult<FileIndex> {
        match &self.location {
            Location::Legacy { dir, .. } => manifest::read_index(dir),
            Location::Plain { dir, .. } => manifest::read_index(&dir.join(META_DIR)),
            Location::Encrypted { file, .. } => {
                let key = key.ok_or(EngineError::Locked)?;
                Ok(manifest::read_encrypted(file, key)?.index)
            }
        }
    }

    fn sort_key(&self) -> String {
        let digits: String = self
            .id
            .chars()
            .filter(char::is_ascii_digit)
            .take(14)
            .collect();
        format!("{digits:0<14}")
    }
}

/// One stored file of a backup, for browsing.
#[derive(Debug, Clone)]
pub struct StoredFile {
    /// Folder inside the backup (e.g. `Documents`, `Applications/Firefox`).
    pub source: String,
    /// Path inside that folder, `/`-separated.
    pub path: String,
    pub size: u64,
    pub modified: i64,
    pub sha256: String,
    pub blob: String,
    pub encrypted: bool,
}

impl StoredFile {
    pub fn display_path(&self) -> String {
        format!("{}/{}", self.source, self.path)
    }
}

/// All files of a backup (every part), sorted by path.
pub fn contents(snapshot: &SnapshotInfo, key: Option<&VaultKey>) -> EngineResult<Vec<StoredFile>> {
    if snapshot.needs_unlock() {
        return Err(EngineError::Locked);
    }
    let mut files = Vec::new();
    for part in snapshot.parts() {
        let index = part.load_index(key)?;
        files.extend(index.files.into_iter().map(|f| StoredFile {
            source: f.source,
            path: f.path,
            size: f.size,
            modified: f.modified,
            sha256: f.sha256,
            blob: f.blob,
            encrypted: part.is_encrypted(),
        }));
    }
    files.sort_by_key(|f| f.display_path().to_lowercase());
    Ok(files)
}

/// `true` if the destination drive/share is reachable. The destination folder
/// itself does not need to exist yet.
pub fn destination_reachable(destination: &Path) -> bool {
    destination
        .ancestors()
        .any(|p| !p.as_os_str().is_empty() && p.exists())
}

/// Name of the folder created inside a freshly chosen destination.
pub const APP_FOLDER: &str = "AeternaVault";

/// The destination to use for a folder the user picked. With `app_folder`,
/// backups go into an `AeternaVault` sub-folder — unless the picked folder is
/// already such a folder or already contains backups.
pub fn chosen_destination(picked: &Path, app_folder: bool) -> PathBuf {
    if !app_folder || contains_backups(picked) {
        return picked.to_path_buf();
    }
    let already_named = picked
        .file_name()
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(APP_FOLDER));
    if already_named {
        picked.to_path_buf()
    } else {
        picked.join(APP_FOLDER)
    }
}

/// Whether `folder` holds AeternaVault backups of any format.
pub fn contains_backups(folder: &Path) -> bool {
    if vault::exists(folder) {
        return true;
    }
    let Ok(entries) = std::fs::read_dir(folder) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if looks_like_snapshot_id(&name) {
            return path.join(META_DIR).is_dir();
        }
        // Format 1: <COMPUTER>\<id>\snapshot.json
        path.is_dir()
            && std::fs::read_dir(&path).is_ok_and(|children| {
                children.flatten().any(|c| {
                    looks_like_snapshot_id(&c.file_name().to_string_lossy())
                        && c.path().join(HEADER_FILE).is_file()
                })
            })
    })
}

/// All snapshots below `destination`, newest first. Encrypted snapshots are
/// only described in detail when `key` unlocks them.
pub fn list(destination: &Path, key: Option<&VaultKey>) -> EngineResult<Vec<SnapshotInfo>> {
    if destination.as_os_str().is_empty() {
        return Err(EngineError::NoDestination);
    }
    if !destination_reachable(destination) {
        return Err(EngineError::DestinationUnavailable(
            destination.to_path_buf(),
        ));
    }
    let Ok(entries) = std::fs::read_dir(destination) else {
        return Ok(Vec::new());
    };

    let mut snapshots = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();

        if name == vault::VAULT_DIR {
            list_encrypted(destination, key, &mut snapshots);
        } else if looks_like_snapshot_id(&name) {
            let meta = dir.join(META_DIR);
            if !meta.is_dir() {
                continue;
            }
            let header = meta
                .join(HEADER_FILE)
                .is_file()
                .then(|| {
                    manifest::read_header(&meta)
                        .inspect_err(|e| tracing::warn!("{e}"))
                        .ok()
                })
                .flatten();
            let computer = header
                .as_ref()
                .map(|h| h.computer.clone())
                .or_else(|| computer_suffix(&name))
                .unwrap_or_default();
            snapshots.push(SnapshotInfo {
                id: name,
                computer,
                location: Location::Plain {
                    dir,
                    destination: destination.to_path_buf(),
                },
                header,
                companion: None,
            });
        } else {
            list_legacy(&dir, &name, &mut snapshots);
        }
    }

    snapshots.sort_by(|a, b| {
        b.sort_key()
            .cmp(&a.sort_key())
            .then_with(|| a.computer.cmp(&b.computer))
    });
    Ok(pair_parts(snapshots))
}

/// Attaches the encrypted part of a partly encrypted backup to its plain part.
fn pair_parts(snapshots: Vec<SnapshotInfo>) -> Vec<SnapshotInfo> {
    let mut encrypted: Vec<SnapshotInfo> = Vec::new();
    let mut others: Vec<SnapshotInfo> = Vec::new();
    for info in snapshots {
        if info.is_encrypted() {
            encrypted.push(info);
        } else {
            others.push(info);
        }
    }
    for info in &mut others {
        let split = info.header.as_ref().is_some_and(|h| h.split);
        if split
            && matches!(info.location, Location::Plain { .. })
            && let Some(position) = encrypted.iter().position(|e| e.id == info.id)
        {
            info.companion = Some(Box::new(encrypted.remove(position)));
        }
    }
    others.extend(encrypted);
    others.sort_by(|a, b| {
        b.sort_key()
            .cmp(&a.sort_key())
            .then_with(|| a.computer.cmp(&b.computer))
    });
    others
}

fn list_encrypted(destination: &Path, key: Option<&VaultKey>, out: &mut Vec<SnapshotInfo>) {
    let dir = vault::vault_dir(destination).join(vault::SNAPSHOT_DIR);
    let Ok(files) = std::fs::read_dir(&dir) else {
        return;
    };
    for file in files.flatten() {
        let path = file.path();
        if path.extension().and_then(|e| e.to_str()) != Some(vault::SNAPSHOT_EXT) {
            continue;
        }
        let Some(id) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
            continue;
        };
        let header = key.and_then(|key| {
            manifest::read_encrypted(&path, key)
                .inspect_err(|e| tracing::warn!("{e}"))
                .ok()
                .map(|s| s.header)
        });
        out.push(SnapshotInfo {
            computer: header
                .as_ref()
                .map(|h| h.computer.clone())
                .or_else(|| computer_suffix(&id))
                .unwrap_or_default(),
            id,
            location: Location::Encrypted {
                file: path,
                destination: destination.to_path_buf(),
            },
            header,
            companion: None,
        });
    }
}

fn list_legacy(computer_dir: &Path, computer: &str, out: &mut Vec<SnapshotInfo>) {
    let Ok(children) = std::fs::read_dir(computer_dir) else {
        return;
    };
    for child in children.flatten() {
        let dir = child.path();
        let id = child.file_name().to_string_lossy().into_owned();
        if !dir.is_dir() || !looks_like_snapshot_id(&id) {
            continue;
        }
        let header = if dir.join(HEADER_FILE).is_file() {
            manifest::read_header(&dir)
                .inspect_err(|e| tracing::warn!("{e}"))
                .ok()
        } else if dir.join(manifest::LEGACY_DATA_DIR).is_dir() {
            None
        } else {
            continue;
        };
        out.push(SnapshotInfo {
            id,
            computer: computer.to_string(),
            location: Location::Legacy {
                dir,
                computer_dir: computer_dir.to_path_buf(),
            },
            header,
            companion: None,
        });
    }
}

/// The newest finished snapshot of `computer` of the requested kind, used as
/// incremental base.
pub fn latest_usable(
    destination: &Path,
    computer: &str,
    encrypted: bool,
    key: Option<&VaultKey>,
) -> Option<SnapshotInfo> {
    list(destination, key)
        .ok()?
        .into_iter()
        .flat_map(|mut s| {
            let companion = s.companion.take().map(|c| *c);
            std::iter::once(s).chain(companion)
        })
        .find(|s| {
            s.computer.eq_ignore_ascii_case(computer)
                && s.is_usable_base()
                && s.is_encrypted() == encrypted
        })
}

/// Whether another computer already writes plain backups into `destination`.
pub fn has_other_computers(destination: &Path, computer: &str) -> bool {
    list(destination, None).is_ok_and(|all| {
        all.iter().any(|s| {
            !s.is_encrypted()
                && !s.computer.is_empty()
                && !s.computer.eq_ignore_ascii_case(computer)
        })
    })
}

/// Resolve `latest`, an id, or a qualified id. Plain ids prefer this computer.
pub fn find(
    destination: &Path,
    wanted: &str,
    computer: &str,
    key: Option<&VaultKey>,
) -> EngineResult<SnapshotInfo> {
    let all = list(destination, key)?;
    let found = if wanted.eq_ignore_ascii_case("latest") {
        // This computer's newest backup; otherwise the newest one at all
        // (e.g. restoring on a new computer).
        let own = all
            .iter()
            .position(|s| s.computer.eq_ignore_ascii_case(computer) && s.is_usable_base())
            .or_else(|| all.iter().position(SnapshotInfo::is_usable_base));
        own.map(|i| all[i].clone())
    } else {
        let mut matches: Vec<_> = all
            .into_iter()
            .filter(|s| s.qualified_id() == wanted || s.id == wanted)
            .collect();
        matches.sort_by_key(|s| !s.computer.eq_ignore_ascii_case(computer));
        matches.into_iter().next()
    };
    found.ok_or_else(|| EngineError::SnapshotNotFound(wanted.to_string()))
}

fn computer_suffix(name: &str) -> Option<String> {
    let start = name.rfind(" (")?;
    name.ends_with(')')
        .then(|| name[start + 2..name.len() - 1].to_string())
}

/// Snapshot names: `2026-09-14 14-32` (format 2, optional ` (PC)` / `-2`)
/// or `2026-09-14_143205` (format 1, optional `-2`).
pub fn looks_like_snapshot_id(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 16 {
        return false;
    }
    let digit = |i: usize| bytes.get(i).is_some_and(u8::is_ascii_digit);
    let date = (0..4).all(digit)
        && bytes[4] == b'-'
        && digit(5)
        && digit(6)
        && bytes[7] == b'-'
        && digit(8)
        && digit(9);
    if !date {
        return false;
    }
    let v2 =
        bytes[10] == b' ' && digit(11) && digit(12) && bytes[13] == b'-' && digit(14) && digit(15);
    let v1 = bytes.len() >= 17 && bytes[10] == b'_' && (11..17).all(digit);
    v1 || v2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_ids() {
        assert!(looks_like_snapshot_id("2026-09-14_143205"));
        assert!(looks_like_snapshot_id("2026-09-14_143205-2"));
        assert!(looks_like_snapshot_id("2026-09-14 14-32"));
        assert!(looks_like_snapshot_id("2026-09-14 14-32 (LAPTOP)"));
        assert!(!looks_like_snapshot_id("Documents"));
        assert!(!looks_like_snapshot_id("2026-09-14"));
        assert_eq!(
            computer_suffix("2026-09-14 14-32 (LAPTOP)").as_deref(),
            Some("LAPTOP")
        );
    }
}
