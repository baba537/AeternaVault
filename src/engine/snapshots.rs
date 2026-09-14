//! Finding existing backups in a destination folder. Read-only.

use std::path::{Path, PathBuf};

use super::manifest::{self, HEADER_FILE, SnapshotHeader};
use crate::error::{EngineError, EngineResult};

#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub id: String,
    pub computer: String,
    pub dir: PathBuf,
    pub computer_dir: PathBuf,
    /// `None` if the backup never finished (no `snapshot.json`) or the header is damaged.
    pub header: Option<SnapshotHeader>,
}

impl SnapshotInfo {
    /// Unique across computers: `COMPUTER/2026-09-14_143205`.
    pub fn qualified_id(&self) -> String {
        format!("{}/{}", self.computer, self.id)
    }

    pub fn is_usable_base(&self) -> bool {
        self.header
            .as_ref()
            .is_some_and(|h| h.status.is_usable_base())
    }
}

/// `true` if the destination drive/share is reachable. The destination folder
/// itself does not need to exist yet.
pub fn destination_reachable(destination: &Path) -> bool {
    destination
        .ancestors()
        .any(|p| !p.as_os_str().is_empty() && p.exists())
}

/// All snapshots below `destination`, newest first.
pub fn list(destination: &Path) -> EngineResult<Vec<SnapshotInfo>> {
    if destination.as_os_str().is_empty() {
        return Err(EngineError::NoDestination);
    }
    if !destination_reachable(destination) {
        return Err(EngineError::DestinationUnavailable(
            destination.to_path_buf(),
        ));
    }
    let Ok(computers) = std::fs::read_dir(destination) else {
        return Ok(Vec::new());
    };

    let mut snapshots = Vec::new();
    for computer_entry in computers.flatten() {
        let computer_dir = computer_entry.path();
        if !computer_dir.is_dir() {
            continue;
        }
        let Ok(children) = std::fs::read_dir(&computer_dir) else {
            continue;
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
            } else {
                None
            };
            snapshots.push(SnapshotInfo {
                id,
                computer: computer_entry.file_name().to_string_lossy().into_owned(),
                dir,
                computer_dir: computer_dir.clone(),
                header,
            });
        }
    }

    // IDs are timestamps, so sorting them sorts chronologically.
    snapshots.sort_by(|a, b| b.id.cmp(&a.id).then_with(|| a.computer.cmp(&b.computer)));
    Ok(snapshots)
}

/// The newest finished snapshot of `computer`, used as incremental base.
pub fn latest_usable(destination: &Path, computer: &str) -> Option<SnapshotInfo> {
    list(destination)
        .ok()?
        .into_iter()
        .find(|s| s.computer.eq_ignore_ascii_case(computer) && s.is_usable_base())
}

/// Resolve `latest`, `ID` or `COMPUTER/ID`. Plain IDs prefer this computer.
pub fn find(destination: &Path, wanted: &str, computer: &str) -> EngineResult<SnapshotInfo> {
    let all = list(destination)?;
    let found = if wanted.eq_ignore_ascii_case("latest") {
        all.into_iter()
            .find(|s| s.computer.eq_ignore_ascii_case(computer) && s.is_usable_base())
    } else if let Some((pc, id)) = wanted.split_once(['/', '\\']) {
        all.into_iter()
            .find(|s| s.computer.eq_ignore_ascii_case(pc) && s.id == id)
    } else {
        let mut matches: Vec<_> = all.into_iter().filter(|s| s.id == wanted).collect();
        matches.sort_by_key(|s| !s.computer.eq_ignore_ascii_case(computer));
        matches.into_iter().next()
    };
    found.ok_or_else(|| EngineError::SnapshotNotFound(wanted.to_string()))
}

/// Snapshot folder names have the shape `YYYY-MM-DD_HHMMSS` with an optional `-N` suffix.
pub fn looks_like_snapshot_id(name: &str) -> bool {
    let base = name.split('-').take(3).collect::<Vec<_>>().join("-");
    let bytes = base.as_bytes();
    bytes.len() == 17
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'_'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| matches!(i, 4 | 7 | 10) || b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::looks_like_snapshot_id;

    #[test]
    fn snapshot_ids() {
        assert!(looks_like_snapshot_id("2026-09-14_143205"));
        assert!(looks_like_snapshot_id("2026-09-14_143205-2"));
        assert!(!looks_like_snapshot_id("Documents"));
        assert!(!looks_like_snapshot_id("2026-09-14"));
    }
}
