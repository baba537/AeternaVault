//! The encrypted vault inside a backup destination.
//!
//! ```text
//! <destination>\AeternaVault Encrypted\
//!   vault.json                   public: format, vault id, wrapped keys (no secrets)
//!   README.txt                   what this folder is and how to restore it
//!   snapshots\2026-09-14 20-00.avs   encrypted backup index (files, registry, header)
//!   blobs\3f\3fa9…c1.avb             encrypted file contents, named by keyed hash
//! ```
//!
//! Identical content is stored once; unchanged files are never uploaded again,
//! which keeps cloud synchronisation small. File names, folder structure and
//! sizes of individual files stay hidden inside the encrypted index.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::crypto::{self, CryptoError, KdfParams, KeySlot, SlotKind, VaultKey};
use crate::platform;

pub const VAULT_DIR: &str = "AeternaVault Encrypted";
pub const VAULT_FILE: &str = "vault.json";
pub const SNAPSHOT_DIR: &str = "snapshots";
pub const BLOB_DIR: &str = "blobs";
pub const SNAPSHOT_EXT: &str = "avs";
pub const BLOB_EXT: &str = "avb";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultHeader {
    pub format: u32,
    pub vault_id: String,
    pub created_at: DateTime<Utc>,
    pub cipher: String,
    pub key_check: String,
    pub slots: Vec<KeySlot>,
}

pub fn vault_dir(destination: &Path) -> PathBuf {
    destination.join(VAULT_DIR)
}

pub fn exists(destination: &Path) -> bool {
    vault_dir(destination).join(VAULT_FILE).is_file()
}

pub fn read_header(destination: &Path) -> io::Result<VaultHeader> {
    let text = fs::read_to_string(vault_dir(destination).join(VAULT_FILE))?;
    serde_json::from_str(&text).map_err(io::Error::other)
}

/// Unlocks with a passphrase or a recovery key (tries every slot).
pub fn unlock(destination: &Path, secret: &str) -> Result<(VaultHeader, VaultKey), CryptoError> {
    let header = read_header(destination)?;
    let mut last_error = CryptoError::WrongKey;
    for slot in &header.slots {
        match slot.open(secret) {
            Ok(key) if key.fingerprint() == header.key_check => return Ok((header, key)),
            Ok(_) => last_error = CryptoError::Damaged,
            Err(CryptoError::WrongKey) => {}
            Err(err) => last_error = err,
        }
    }
    Err(last_error)
}

/// What the user needs to keep after creating a vault.
pub struct NewVault {
    pub header: VaultHeader,
    pub key: VaultKey,
    pub recovery_key: String,
}

/// Creates a new vault. Fails if one already exists at the destination.
pub fn create(destination: &Path, passphrase: &str) -> Result<NewVault, CryptoError> {
    create_with(
        destination,
        passphrase,
        KdfParams::PASSPHRASE,
        KdfParams::RECOVERY,
    )
}

pub(crate) fn create_with(
    destination: &Path,
    passphrase: &str,
    passphrase_kdf: KdfParams,
    recovery_kdf: KdfParams,
) -> Result<NewVault, CryptoError> {
    let dir = vault_dir(destination);
    if dir.join(VAULT_FILE).exists() {
        return Err(CryptoError::Io(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "an encrypted vault already exists at this destination",
        )));
    }
    fs::create_dir_all(dir.join(SNAPSHOT_DIR))?;
    fs::create_dir_all(dir.join(BLOB_DIR))?;

    let key = VaultKey::generate();
    let recovery_key = crypto::new_recovery_key();
    let header = VaultHeader {
        format: 1,
        vault_id: crypto::hex(&crypto::random_bytes::<8>()),
        created_at: Utc::now(),
        cipher: "argon2id+xchacha20poly1305-stream-1mib".into(),
        key_check: key.fingerprint(),
        slots: vec![
            KeySlot::create(SlotKind::Passphrase, passphrase, &key, passphrase_kdf)?,
            KeySlot::create(SlotKind::RecoveryKey, &recovery_key, &key, recovery_kdf)?,
        ],
    };
    write_header(destination, &header)?;
    fs::write(dir.join("README.txt"), README)?;
    Ok(NewVault {
        header,
        key,
        recovery_key,
    })
}

fn write_header(destination: &Path, header: &VaultHeader) -> io::Result<()> {
    let dir = vault_dir(destination);
    let tmp = dir.join("vault.json.tmp");
    fs::write(
        &tmp,
        serde_json::to_vec_pretty(header).map_err(io::Error::other)?,
    )?;
    fs::rename(tmp, dir.join(VAULT_FILE))
}

/// Replaces the passphrase slot. The recovery key and all data stay valid.
pub fn change_passphrase(
    destination: &Path,
    key: &VaultKey,
    new_passphrase: &str,
) -> Result<(), CryptoError> {
    let mut header = read_header(destination)?;
    if header.key_check != key.fingerprint() {
        return Err(CryptoError::WrongKey);
    }
    header.slots.retain(|s| s.kind != SlotKind::Passphrase);
    header.slots.insert(
        0,
        KeySlot::create(
            SlotKind::Passphrase,
            new_passphrase,
            key,
            KdfParams::PASSPHRASE,
        )?,
    );
    write_header(destination, &header)?;
    Ok(())
}

pub fn blob_path(destination: &Path, id: &str) -> PathBuf {
    let shard = id.get(..2).unwrap_or("00");
    vault_dir(destination)
        .join(BLOB_DIR)
        .join(shard)
        .join(format!("{id}.{BLOB_EXT}"))
}

pub fn snapshot_path(destination: &Path, id: &str) -> PathBuf {
    vault_dir(destination)
        .join(SNAPSHOT_DIR)
        .join(format!("{id}.{SNAPSHOT_EXT}"))
}

// ---------------------------------------------------------------------------
// Remembering the key on this computer (for automatic backups)
// ---------------------------------------------------------------------------

fn remembered_key_file(key_dir: &Path, vault_id: &str) -> PathBuf {
    key_dir.join(format!("vault-{vault_id}.key"))
}

/// Stores the vault key protected by Windows DPAPI: only this Windows user on
/// this computer can read it. The passphrase itself is never stored.
pub fn remember_key(key_dir: &Path, header: &VaultHeader, key: &VaultKey) -> io::Result<()> {
    fs::create_dir_all(key_dir)?;
    let protected = platform::protect_for_user(key.master_bytes())?;
    fs::write(remembered_key_file(key_dir, &header.vault_id), protected)
}

pub fn remembered_key(key_dir: &Path, header: &VaultHeader) -> Option<VaultKey> {
    let protected = fs::read(remembered_key_file(key_dir, &header.vault_id)).ok()?;
    let bytes = platform::unprotect_for_user(&protected).ok()?;
    let master: [u8; 32] = bytes.as_slice().try_into().ok()?;
    let key = VaultKey::from_master(master);
    (key.fingerprint() == header.key_check).then_some(key)
}

pub fn is_remembered(key_dir: &Path, header: &VaultHeader) -> bool {
    remembered_key_file(key_dir, &header.vault_id).is_file()
}

pub fn forget_key(key_dir: &Path, header: &VaultHeader) -> io::Result<()> {
    match fs::remove_file(remembered_key_file(key_dir, &header.vault_id)) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
        _ => Ok(()),
    }
}

const README: &str = "\
AeternaVault encrypted backups
==============================

This folder contains backups encrypted with AeternaVault. File names and
contents cannot be read without the passphrase or the recovery key.

To restore:
  1. Install AeternaVault (https://github.com/baba537/AeternaVault/releases).
  2. Choose the folder that contains this folder as the destination.
  3. Open Restore, select a backup and unlock it with your passphrase or
     recovery key.

Please copy or synchronise the whole folder. Do not rename or delete single
files: they are shared between backups.

The format is described in docs/ENCRYPTION.md in the AeternaVault repository.
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_unlock_and_change_passphrase() {
        let tmp = tempfile::tempdir().unwrap();
        let created = create_with(
            tmp.path(),
            "first passphrase",
            KdfParams::TEST,
            KdfParams::TEST,
        )
        .unwrap();
        assert!(exists(tmp.path()));
        assert!(matches!(
            unlock(tmp.path(), "nope"),
            Err(CryptoError::WrongKey)
        ));

        let (_, key) = unlock(tmp.path(), "first passphrase").unwrap();
        assert_eq!(key.master_bytes(), created.key.master_bytes());
        let (_, key) = unlock(tmp.path(), &created.recovery_key).unwrap();
        assert_eq!(key.master_bytes(), created.key.master_bytes());

        change_passphrase(tmp.path(), &key, "second passphrase").unwrap();
        assert!(unlock(tmp.path(), "first passphrase").is_err());
        assert!(unlock(tmp.path(), "second passphrase").is_ok());
        assert!(unlock(tmp.path(), &created.recovery_key).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn remembered_key_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let created = create_with(tmp.path(), "pass", KdfParams::TEST, KdfParams::TEST).unwrap();
        let keys = tmp.path().join("keys");
        remember_key(&keys, &created.header, &created.key).unwrap();
        let loaded = remembered_key(&keys, &created.header).unwrap();
        assert_eq!(loaded.master_bytes(), created.key.master_bytes());
        forget_key(&keys, &created.header).unwrap();
        assert!(remembered_key(&keys, &created.header).is_none());
    }
}
