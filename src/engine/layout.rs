//! Moves encrypted backups of AeternaVault 0.3 and 0.4 into the current layout.
//!
//! Old: `<destination>\AeternaVault Encrypted\{vault.json, blobs\, snapshots\<id>.avs, README.txt}`.
//! New: `<destination>\.aeternavault\{vault.json, blobs\}` and
//! `<destination>\<id>\.aeternavault\encrypted.avs` (see `manifest`).
//!
//! Only files are moved (renamed on the same drive); nothing is re-encrypted,
//! and blob ids stay valid. The migration is idempotent: an interrupted run is
//! completed by the next one.

use std::fs;
use std::io;
use std::path::Path;

use super::manifest::{ENCRYPTED_FILE, META_DIR};
use super::vault::{self, BLOB_DIR, VAULT_FILE};
use crate::platform;

pub const OLD_VAULT_DIR: &str = "AeternaVault Encrypted";
const OLD_SNAPSHOT_DIR: &str = "snapshots";

/// Moves an old encrypted vault into the current layout. Returns the number of
/// backups moved (0 if there was nothing to do).
pub fn migrate(destination: &Path) -> io::Result<usize> {
    let old = destination.join(OLD_VAULT_DIR);
    if !old.is_dir() {
        return Ok(0);
    }
    let new = vault::vault_dir(destination);
    fs::create_dir_all(&new)?;
    platform::set_hidden(&new);

    let old_header = old.join(VAULT_FILE);
    if old_header.is_file() {
        if new.join(VAULT_FILE).is_file() {
            return Err(io::Error::other(format!(
                "{} holds two encrypted vaults; please move one of them elsewhere",
                destination.display()
            )));
        }
        fs::rename(&old_header, new.join(VAULT_FILE))?;
    }

    let old_blobs = old.join(BLOB_DIR);
    if old_blobs.is_dir() {
        let new_blobs = new.join(BLOB_DIR);
        if new_blobs.exists() {
            // Merge shard by shard (an earlier run may have been interrupted).
            for shard in fs::read_dir(&old_blobs)?.flatten() {
                let target = new_blobs.join(shard.file_name());
                fs::create_dir_all(&target)?;
                for blob in fs::read_dir(shard.path())?.flatten() {
                    let to = target.join(blob.file_name());
                    if to.exists() {
                        fs::remove_file(blob.path())?;
                    } else {
                        fs::rename(blob.path(), to)?;
                    }
                }
                let _ = fs::remove_dir(shard.path());
            }
            let _ = fs::remove_dir(&old_blobs);
        } else {
            fs::rename(&old_blobs, new_blobs)?;
        }
    }

    let mut moved = 0;
    let snapshots = old.join(OLD_SNAPSHOT_DIR);
    if let Ok(entries) = fs::read_dir(&snapshots) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("avs") {
                continue;
            }
            let Some(id) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
                continue;
            };
            let dir = destination.join(&id);
            let meta = dir.join(META_DIR);
            fs::create_dir_all(&meta)?;
            platform::set_hidden(&meta);
            fs::rename(&path, meta.join(ENCRYPTED_FILE))?;
            vault::write_open_file(&dir);
            moved += 1;
        }
        let _ = fs::remove_dir(&snapshots);
    }

    for name in ["README.txt", vault::OPEN_FILE, "vault.json.tmp"] {
        let _ = fs::remove_file(old.join(name));
    }
    match fs::remove_dir(&old) {
        Ok(()) => {}
        Err(err) => tracing::warn!(
            "{} was left behind because it is not empty: {err}",
            old.display()
        ),
    }
    tracing::info!(
        "moved {moved} encrypted backups of AeternaVault 0.4 to the current layout in {}",
        destination.display()
    );
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_encrypted_layout_is_moved() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path();
        let old = dest.join(OLD_VAULT_DIR);
        fs::create_dir_all(old.join("blobs").join("ab")).unwrap();
        fs::create_dir_all(old.join("snapshots")).unwrap();
        fs::write(old.join("vault.json"), "{}").unwrap();
        fs::write(old.join("blobs").join("ab").join("abcd.avb"), "x").unwrap();
        fs::write(old.join("snapshots").join("2026-09-14 20-00.avs"), "y").unwrap();
        fs::write(old.join("README.txt"), "z").unwrap();

        assert_eq!(migrate(dest).unwrap(), 1);
        assert!(!old.exists());
        assert!(vault::vault_dir(dest).join("vault.json").is_file());
        assert!(vault::blob_path(dest, "abcd").is_file());
        assert!(vault::snapshot_path(dest, "2026-09-14 20-00").is_file());
        assert!(
            dest.join("2026-09-14 20-00")
                .join(vault::OPEN_FILE)
                .is_file()
        );
        assert_eq!(migrate(dest).unwrap(), 0);
    }
}
