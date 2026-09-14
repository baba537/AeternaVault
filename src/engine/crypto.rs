//! Cryptography for encrypted backups. See docs/ENCRYPTION.md for the format.
//!
//! Building blocks (RustCrypto, pure Rust, widely reviewed):
//! * **Argon2id** turns a passphrase (or recovery key) into a key-encryption key.
//! * **XChaCha20-Poly1305** encrypts and authenticates everything.
//! * **HMAC-SHA-256** derives sub-keys and content-based blob names.
//!
//! A random 256-bit *vault key* protects all data. It is stored only in
//! wrapped form (encrypted with the passphrase-derived key and, separately,
//! with the recovery-key-derived key), so the passphrase can be changed
//! without re-encrypting any backup.
//!
//! File contents use a chunked "STREAM" construction: 1 MiB chunks, each with
//! its own nonce made of a random 19-byte prefix, a 32-bit counter and a
//! final-chunk flag. Reordering, truncating or appending chunks is detected.

use std::io::{self, Read, Write};

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::XChaCha20Poly1305;
use chacha20poly1305::aead::{Aead, KeyInit};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

use super::CancelToken;

pub const CHUNK_SIZE: usize = 1024 * 1024;
const TAG_SIZE: usize = 16;
const PREFIX_SIZE: usize = 19;
pub const BLOB_MAGIC: &[u8; 4] = b"AVB1";

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("the passphrase or recovery key is not correct")]
    WrongKey,
    #[error("the encrypted data is damaged or was modified")]
    Damaged,
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("key derivation failed: {0}")]
    Kdf(String),
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).expect("the operating system random generator is available");
    bytes
}

fn cipher(key: &[u8; 32]) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(&(*key).into())
}

fn hmac(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac =
        <Hmac<Sha256> as hmac::KeyInit>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

// ---------------------------------------------------------------------------
// Key derivation and key slots
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl KdfParams {
    /// For passphrases: 64 MiB, 3 passes (about half a second on a typical PC).
    pub const PASSPHRASE: Self = Self {
        memory_kib: 64 * 1024,
        iterations: 3,
        parallelism: 1,
    };
    /// Recovery keys already carry 160 bits of randomness.
    pub const RECOVERY: Self = Self {
        memory_kib: 19 * 1024,
        iterations: 2,
        parallelism: 1,
    };
    #[cfg(test)]
    pub const TEST: Self = Self {
        memory_kib: 64,
        iterations: 1,
        parallelism: 1,
    };

    fn derive(&self, secret: &[u8], salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, CryptoError> {
        let params = Params::new(self.memory_kib, self.iterations, self.parallelism, Some(32))
            .map_err(|e| CryptoError::Kdf(e.to_string()))?;
        let mut out = Zeroizing::new([0u8; 32]);
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(secret, salt, out.as_mut())
            .map_err(|e| CryptoError::Kdf(e.to_string()))?;
        Ok(out)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SlotKind {
    Passphrase,
    RecoveryKey,
}

/// The vault key, wrapped with a key derived from a passphrase or recovery key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeySlot {
    pub kind: SlotKind,
    pub kdf: KdfParams,
    pub salt: String,
    pub nonce: String,
    pub wrapped_key: String,
}

impl KeySlot {
    pub fn create(
        kind: SlotKind,
        secret: &str,
        vault: &VaultKey,
        kdf: KdfParams,
    ) -> Result<Self, CryptoError> {
        let secret = normalize_secret(kind, secret);
        let salt: [u8; 16] = random_bytes();
        let nonce: [u8; 24] = random_bytes();
        let kek = kdf.derive(secret.as_bytes(), &salt)?;
        let wrapped = cipher(&kek)
            .encrypt(&nonce.into(), &vault.master[..])
            .map_err(|_| CryptoError::Damaged)?;
        Ok(Self {
            kind,
            kdf,
            salt: hex(&salt),
            nonce: hex(&nonce),
            wrapped_key: hex(&wrapped),
        })
    }

    pub fn open(&self, secret: &str) -> Result<VaultKey, CryptoError> {
        let secret = normalize_secret(self.kind, secret);
        let salt = unhex(&self.salt).ok_or(CryptoError::Damaged)?;
        let nonce: [u8; 24] = unhex(&self.nonce)
            .and_then(|n| n.try_into().ok())
            .ok_or(CryptoError::Damaged)?;
        let wrapped = unhex(&self.wrapped_key).ok_or(CryptoError::Damaged)?;
        let kek = self.kdf.derive(secret.as_bytes(), &salt)?;
        let mut master = cipher(&kek)
            .decrypt(&nonce.into(), wrapped.as_slice())
            .map_err(|_| CryptoError::WrongKey)?;
        let key: [u8; 32] = master
            .as_slice()
            .try_into()
            .map_err(|_| CryptoError::Damaged)?;
        master.zeroize();
        Ok(VaultKey::from_master(key))
    }
}

fn normalize_secret(kind: SlotKind, secret: &str) -> Zeroizing<String> {
    Zeroizing::new(match kind {
        // Passphrases are used exactly as typed (only Unicode-normalised by the OS input).
        SlotKind::Passphrase => secret.to_string(),
        // Recovery keys are forgiving: case, spaces and dashes do not matter.
        SlotKind::RecoveryKey => secret
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .map(|c| c.to_ascii_uppercase())
            .collect(),
    })
}

/// A new random recovery key, e.g. `K7QM-2HXA-…` (8 groups, 160 bits).
pub fn new_recovery_key() -> String {
    // Crockford base32 without easily confused characters.
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let bytes: [u8; 20] = random_bytes();
    let mut bits = 0u32;
    let mut bit_count = 0;
    let mut chars = String::new();
    for byte in bytes {
        bits = (bits << 8) | u32::from(byte);
        bit_count += 8;
        while bit_count >= 5 {
            bit_count -= 5;
            chars.push(ALPHABET[((bits >> bit_count) & 31) as usize] as char);
        }
    }
    chars
        .as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("-")
}

// ---------------------------------------------------------------------------
// Vault key and data encryption
// ---------------------------------------------------------------------------

pub struct VaultKey {
    master: Zeroizing<[u8; 32]>,
    data: Zeroizing<[u8; 32]>,
    names: Zeroizing<[u8; 32]>,
}

impl Clone for VaultKey {
    fn clone(&self) -> Self {
        Self::from_master(*self.master)
    }
}

impl std::fmt::Debug for VaultKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VaultKey(…)")
    }
}

impl VaultKey {
    pub fn generate() -> Self {
        Self::from_master(random_bytes())
    }

    pub fn from_master(master: [u8; 32]) -> Self {
        Self {
            data: Zeroizing::new(hmac(&master, b"AeternaVault v1 data key")),
            names: Zeroizing::new(hmac(&master, b"AeternaVault v1 name key")),
            master: Zeroizing::new(master),
        }
    }

    pub fn master_bytes(&self) -> &[u8; 32] {
        &self.master
    }

    /// Blob name for a file content: unlinkable to the content without the key,
    /// identical for identical content (deduplication).
    pub fn blob_id(&self, content_sha256: &[u8]) -> String {
        hex(&hmac(self.names.as_ref(), content_sha256))
    }

    /// Checks that a key belongs to a vault without decrypting any data.
    pub fn fingerprint(&self) -> String {
        hex(&hmac(self.master.as_ref(), b"AeternaVault v1 key check")[..8])
    }

    /// Encrypts a stream. Returns the SHA-256 of the plaintext.
    pub fn encrypt_stream(
        &self,
        input: &mut dyn Read,
        output: &mut dyn Write,
        cancel: &CancelToken,
        on_bytes: &mut dyn FnMut(u64),
    ) -> Result<[u8; 32], CryptoError> {
        let aead = cipher(&self.data);
        let prefix: [u8; PREFIX_SIZE] = random_bytes();
        output.write_all(BLOB_MAGIC)?;
        output.write_all(&prefix)?;

        let mut hasher = Sha256::new();
        let mut current = read_up_to(input, CHUNK_SIZE)?;
        let mut counter: u32 = 0;
        loop {
            if cancel.is_cancelled() {
                return Err(CryptoError::Cancelled);
            }
            let next = if current.len() == CHUNK_SIZE {
                read_up_to(input, CHUNK_SIZE)?
            } else {
                Vec::new()
            };
            let last = next.is_empty();
            hasher.update(&current);
            let nonce = chunk_nonce(&prefix, counter, last);
            let sealed = aead
                .encrypt(&nonce.into(), current.as_slice())
                .map_err(|_| CryptoError::Damaged)?;
            output.write_all(&sealed)?;
            on_bytes(current.len() as u64);
            current.zeroize();
            if last {
                break;
            }
            current = next;
            counter = counter.checked_add(1).ok_or(CryptoError::Damaged)?;
        }
        output.flush()?;
        Ok(hasher.finalize().into())
    }

    /// Decrypts a stream written by [`encrypt_stream`]. Returns the SHA-256 of
    /// the plaintext. Output written before an error must be discarded by the
    /// caller.
    pub fn decrypt_stream(
        &self,
        input: &mut dyn Read,
        output: &mut dyn Write,
        cancel: &CancelToken,
        on_bytes: &mut dyn FnMut(u64),
    ) -> Result<[u8; 32], CryptoError> {
        let aead = cipher(&self.data);
        let mut header = [0u8; 4 + PREFIX_SIZE];
        input
            .read_exact(&mut header)
            .map_err(|_| CryptoError::Damaged)?;
        if &header[..4] != BLOB_MAGIC {
            return Err(CryptoError::Damaged);
        }
        let prefix: [u8; PREFIX_SIZE] = header[4..].try_into().map_err(|_| CryptoError::Damaged)?;

        let sealed_size = CHUNK_SIZE + TAG_SIZE;
        let mut hasher = Sha256::new();
        let mut current = read_up_to(input, sealed_size)?;
        if current.len() < TAG_SIZE {
            return Err(CryptoError::Damaged);
        }
        let mut counter: u32 = 0;
        loop {
            if cancel.is_cancelled() {
                return Err(CryptoError::Cancelled);
            }
            let next = if current.len() == sealed_size {
                read_up_to(input, sealed_size)?
            } else {
                Vec::new()
            };
            let last = next.is_empty();
            let nonce = chunk_nonce(&prefix, counter, last);
            let mut plain = aead
                .decrypt(&nonce.into(), current.as_slice())
                .map_err(|_| CryptoError::Damaged)?;
            hasher.update(&plain);
            output.write_all(&plain)?;
            on_bytes(plain.len() as u64);
            plain.zeroize();
            if last {
                break;
            }
            if next.len() < TAG_SIZE {
                return Err(CryptoError::Damaged);
            }
            current = next;
            counter = counter.checked_add(1).ok_or(CryptoError::Damaged)?;
        }
        output.flush()?;
        Ok(hasher.finalize().into())
    }

    pub fn encrypt_bytes(&self, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len() + 64);
        self.encrypt_stream(
            &mut &data[..],
            &mut out,
            &CancelToken::default(),
            &mut |_| {},
        )
        .expect("encrypting into memory cannot fail");
        out
    }

    pub fn decrypt_bytes(&self, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let mut out = Vec::new();
        self.decrypt_stream(
            &mut &data[..],
            &mut out,
            &CancelToken::default(),
            &mut |_| {},
        )?;
        Ok(out)
    }
}

fn chunk_nonce(prefix: &[u8; PREFIX_SIZE], counter: u32, last: bool) -> [u8; 24] {
    let mut nonce = [0u8; 24];
    nonce[..PREFIX_SIZE].copy_from_slice(prefix);
    nonce[PREFIX_SIZE..PREFIX_SIZE + 4].copy_from_slice(&counter.to_be_bytes());
    nonce[23] = u8::from(last);
    nonce
}

fn read_up_to(input: &mut dyn Read, size: usize) -> io::Result<Vec<u8>> {
    let mut buffer = Vec::with_capacity(size.min(CHUNK_SIZE + TAG_SIZE));
    input.take(size as u64).read_to_end(&mut buffer)?;
    Ok(buffer)
}

pub fn hex(bytes: &[u8]) -> String {
    super::format_sha256(bytes)
}

pub fn unhex(text: &str) -> Option<Vec<u8>> {
    crate::platform::registry::hex_decode(text)
}

/// Rough passphrase strength for the UI: 0 = too weak … 4 = strong.
pub fn passphrase_strength(passphrase: &str) -> u8 {
    let length = passphrase.chars().count();
    let classes = [
        passphrase.chars().any(|c| c.is_lowercase()),
        passphrase.chars().any(|c| c.is_uppercase()),
        passphrase.chars().any(|c| c.is_ascii_digit()),
        passphrase.chars().any(|c| !c.is_alphanumeric()),
    ]
    .iter()
    .filter(|&&b| b)
    .count();
    let words = passphrase.split_whitespace().count();
    match length {
        0..=7 => 0,
        8..=11 => 1 + u8::from(classes >= 3),
        12..=19 => 2 + u8::from(classes >= 3 || words >= 3),
        _ => 3 + u8::from(classes >= 2 || words >= 4),
    }
    .min(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(size: usize) {
        let key = VaultKey::generate();
        let data: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        let sealed = key.encrypt_bytes(&data);
        assert_eq!(key.decrypt_bytes(&sealed).unwrap(), data);
    }

    #[test]
    fn roundtrips_various_sizes() {
        for size in [
            0,
            1,
            100,
            CHUNK_SIZE - 1,
            CHUNK_SIZE,
            CHUNK_SIZE + 1,
            2 * CHUNK_SIZE + 17,
        ] {
            roundtrip(size);
        }
    }

    #[test]
    fn detects_tampering_truncation_and_wrong_key() {
        let key = VaultKey::generate();
        let data = vec![7u8; CHUNK_SIZE + 5];
        let sealed = key.encrypt_bytes(&data);

        let mut flipped = sealed.clone();
        flipped[40] ^= 1;
        assert!(matches!(
            key.decrypt_bytes(&flipped),
            Err(CryptoError::Damaged)
        ));

        // Cutting off the final chunk leaves a chunk that is not marked as last.
        let truncated = &sealed[..4 + PREFIX_SIZE + CHUNK_SIZE + TAG_SIZE];
        assert!(key.decrypt_bytes(truncated).is_err());

        let other = VaultKey::generate();
        assert!(other.decrypt_bytes(&sealed).is_err());
    }

    #[test]
    fn key_slots_unlock_only_with_the_right_secret() {
        let key = VaultKey::generate();
        let slot =
            KeySlot::create(SlotKind::Passphrase, "correct horse", &key, KdfParams::TEST).unwrap();
        assert!(matches!(slot.open("wrong"), Err(CryptoError::WrongKey)));
        let opened = slot.open("correct horse").unwrap();
        assert_eq!(opened.master_bytes(), key.master_bytes());

        let recovery = new_recovery_key();
        assert_eq!(recovery.len(), 32 + 7);
        let slot =
            KeySlot::create(SlotKind::RecoveryKey, &recovery, &key, KdfParams::TEST).unwrap();
        let sloppy = recovery.to_lowercase().replace('-', " ");
        assert_eq!(
            slot.open(&sloppy).unwrap().master_bytes(),
            key.master_bytes()
        );
    }

    #[test]
    fn blob_ids_are_deterministic_per_key() {
        let a = VaultKey::generate();
        let b = VaultKey::generate();
        let digest = Sha256::digest(b"content");
        assert_eq!(a.blob_id(&digest), a.blob_id(&digest));
        assert_ne!(a.blob_id(&digest), b.blob_id(&digest));
    }

    #[test]
    fn strength_scale() {
        assert_eq!(passphrase_strength("abc"), 0);
        assert!(passphrase_strength("correct horse battery staple") >= 3);
    }
}
