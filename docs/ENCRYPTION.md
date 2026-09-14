# Encrypted backups

This document describes how AeternaVault protects encrypted backups and how they
are stored, so that the format can be understood — and if necessary
re-implemented — independently of the application.

> AeternaVault has not had an external security audit yet. The design uses
> standard, well-reviewed building blocks and avoids inventing new cryptography.

## Goals

- Nobody without the passphrase or the recovery key can read file contents,
  file names, folder structure or the sizes of individual files.
- Damaged or modified data is detected; it is never restored silently.
- Unchanged files are stored once, so synchronising a vault to a cloud folder
  only uploads what is new.
- Changing the passphrase does not require re-encrypting anything.
- Automatic backups can run without typing the passphrase, without storing it.

Not goals: hiding *that* backups exist, their dates, their approximate total size
or the number of stored files.

## Building blocks

| Purpose | Algorithm | Crate |
|---|---|---|
| Passphrase → key | Argon2id (v1.3) | `argon2` |
| Encryption and authentication | XChaCha20-Poly1305 | `chacha20poly1305` |
| Key derivation, content names | HMAC-SHA-256 | `hmac`, `sha2` |
| Randomness | the operating system's secure random generator | `getrandom` |
| Keeping the key for automatic backups | Windows DPAPI (current user) | `windows-sys` |

## Keys

```text
passphrase ──Argon2id(salt₁, 64 MiB, 3 passes)──► KEK₁ ─┐
                                                        ├─ XChaCha20-Poly1305 wraps ─► vault key (256 bit, random)
recovery key ─Argon2id(salt₂, 19 MiB, 2 passes)─► KEK₂ ─┘

vault key ──HMAC-SHA-256("AeternaVault v1 data key")──► data key   (encrypts everything)
vault key ──HMAC-SHA-256("AeternaVault v1 name key")──► name key   (names blobs)
vault key ──HMAC-SHA-256("AeternaVault v1 key check")[0..8] ──► key check (public)
```

- The **vault key** is generated once from the operating system's random source.
- Each **key slot** stores the vault key encrypted with a key-encryption key
  (KEK) derived from a secret. There is one slot for the passphrase and one for
  the recovery key. Changing the passphrase replaces only its slot.
- The **recovery key** is 160 random bits, shown once as eight groups of four
  characters (Crockford Base32, e.g. `K7QM-2HXA-…`). Case, spaces and dashes are
  ignored when it is typed.
- The **key check** lets AeternaVault confirm that an unlocked key belongs to the
  vault without decrypting data.
- **Remember on this computer** stores the vault key (not the passphrase)
  encrypted with DPAPI in `%APPDATA%\AeternaVault\keys\vault-<id>.key`. Only the
  same Windows user on the same computer can decrypt it. Removing the tick deletes
  the file.

## Layout

```text
<destination>\AeternaVault Encrypted\
  vault.json                          public header (see below)
  README.txt                          explanation for people who find the folder
  snapshots\2026-09-14 20-00.avs      one encrypted index per backup
  blobs\3f\3fa9…c1.avb                encrypted file contents
```

### `vault.json`

```json
{
  "format": 1,
  "vault_id": "9c1e0f6a2b7d4e31",
  "created_at": "2026-09-14T18:00:00Z",
  "cipher": "argon2id+xchacha20poly1305-stream-1mib",
  "key_check": "5a0c93d1e2f47b86",
  "slots": [
    {
      "kind": "passphrase",
      "kdf": { "memory_kib": 65536, "iterations": 3, "parallelism": 1 },
      "salt": "<16 bytes hex>",
      "nonce": "<24 bytes hex>",
      "wrapped_key": "<32 + 16 bytes hex>"
    },
    { "kind": "recovery-key", "kdf": { "memory_kib": 19456, "iterations": 2, "parallelism": 1 }, "…": "…" }
  ]
}
```

`wrapped_key = XChaCha20-Poly1305(KEK, nonce).encrypt(vault key)`, no associated data.

### Encrypted stream format (`.avb`, `.avs`)

```text
"AVB1"                    4 bytes, magic
prefix                    19 random bytes
chunk₀ … chunkₙ           each: XChaCha20-Poly1305 ciphertext + 16-byte tag
```

- Plaintext is split into chunks of 1 MiB (the last chunk may be shorter; an
  empty file is one empty chunk).
- The 24-byte nonce of chunk *i* is `prefix ‖ i as u32 big-endian ‖ last`, where
  `last` is `1` for the final chunk and `0` otherwise (the "STREAM" construction).
- Swapping, removing, duplicating or appending chunks, and cutting a file at a
  chunk boundary, make authentication fail.

### Blobs

- A file's content is encrypted with the data key into a blob.
- The blob id is `HMAC-SHA-256(name key, SHA-256(plaintext))`, hex-encoded; the
  file is `blobs/<first two hex characters>/<id>.avb`.
- Identical content produces the same id, so it is stored once and reused by later
  backups. Without the name key, ids reveal nothing about the content.

### Snapshot index (`.avs`)

The encrypted JSON document contains:

- `header` — backup name, computer, times, status, statistics, the list of
  sources with their original and portable paths, and the known folders of the
  user (for adapting paths on restore);
- `index.files` — for every file: source, relative path, size, modification time,
  SHA-256 of the plaintext and blob id;
- `index.registry` — exported registry keys and values.

The file name of an `.avs` file (the backup's date) is not encrypted.

## Restoring

1. Unlock: try each key slot with the entered secret; compare the key check.
2. Decrypt the chosen `.avs` index.
3. For each file, decrypt its blob into `<target>.aeterna-partial`, compute
   SHA-256 of the plaintext, compare with the index, then rename into place.
   Any authentication or checksum failure leaves the target untouched and is
   reported.

## Threat model in short

| Situation | Protected? |
|---|---|
| Cloud provider or someone with the backup drive reads the files | Yes |
| Someone modifies or deletes files in the vault | Detected on restore (deleted files cannot be recovered) |
| Weak passphrase and a determined attacker | Argon2id slows guessing; a strong passphrase is still essential |
| Malware running as your Windows user | No — it can read your files directly, and the remembered key |
| Passphrase **and** recovery key lost | Data cannot be recovered by anyone |
