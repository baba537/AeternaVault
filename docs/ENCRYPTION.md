# Encrypted backups

This document describes exactly how AeternaVault protects encrypted backups and
how they are stored, so the format can be checked and re-implemented without
AeternaVault. [`tools/aeterna-decrypt.py`](../tools/aeterna-decrypt.py) is such
an independent implementation (about 250 lines of Python using the
`cryptography` and `argon2-cffi` packages); it is tested against vaults written
by AeternaVault with both ciphers.

> **Status:** AeternaVault has not had an external security audit. The design
> only combines standard, widely reviewed primitives from the RustCrypto
> project, but the way they are combined is AeternaVault's own. Please read the
> threat model below and keep a second copy of important data.

## Goals

- Nobody without the passphrase or the recovery key can read file contents,
  file names, folder structure or the sizes of individual files.
- Damaged or modified data is detected; it is never restored silently.
- Unchanged files are stored once, so synchronising a vault to a cloud folder
  only uploads what is new.
- Changing the passphrase or the recovery key does not re-encrypt anything.
- Automatic backups can run without typing the passphrase, without storing it.
- The data stays readable without AeternaVault.

Not goals: hiding *that* backups exist, their dates, their approximate total
size or the number of stored files.

## Choices when setting up

| Choice | Options | Default |
|---|---|---|
| Cipher | XChaCha20-Poly1305 · AES-256-GCM | XChaCha20-Poly1305 |
| Passphrase key derivation | Argon2id 64 MiB / 3 passes · 256 MiB / 4 · 1 GiB / 4 (parallelism 1) | 64 MiB / 3 |
| What is encrypted | everything · only marked folders and files (optionally application settings) | everything |

Both ciphers are authenticated encryption with 256-bit keys. XChaCha20-Poly1305
is fast on every processor and has 192-bit nonces; AES-256-GCM is the more widely
standardised choice and fast on processors with AES instructions. There is no
known reason to prefer one for security.

## Building blocks

| Purpose | Algorithm | Crate |
|---|---|---|
| Passphrase → key | Argon2id, version 0x13, 32-byte output | `argon2` |
| Key wrapping | XChaCha20-Poly1305 | `chacha20poly1305` |
| Data encryption | XChaCha20-Poly1305 or AES-256-GCM | `chacha20poly1305`, `aes-gcm` |
| Sub-keys, content names | HMAC-SHA-256 | `hmac`, `sha2` |
| Randomness | the operating system's secure random generator | `getrandom` |
| Key for automatic backups | Windows DPAPI (current user) | `windows-sys` |

## Keys

```text
passphrase ──Argon2id(salt₁, kdf₁)──► KEK₁ ─┐
                                              ├─ XChaCha20-Poly1305 wraps ─► vault key (32 random bytes)
recovery key ─Argon2id(salt₂, kdf₂)─► KEK₂ ─┘

data key  = HMAC-SHA-256(key = vault key, message = "AeternaVault v1 data key")
name key  = HMAC-SHA-256(key = vault key, message = "AeternaVault v1 name key")
key check = HMAC-SHA-256(key = vault key, message = "AeternaVault v1 key check")[0..8], hex
```

All strings are ASCII without a terminating zero.

- **Passphrase** bytes are the UTF-8 encoding exactly as typed.
- **Recovery key**: 20 random bytes written as 32 Crockford Base32 characters
  (alphabet `0123456789ABCDEFGHJKMNPQRSTVWXYZ`, 5 bits per character,
  most significant bit first) in eight groups of four joined by `-`. For key
  derivation, every character that is not an ASCII letter or digit is removed and
  letters are upper-cased (so `k7qm 2hxa …` works too).
- A **key slot** is opened by deriving the KEK with the slot's salt and
  parameters, then decrypting `wrapped_key` with XChaCha20-Poly1305
  (key = KEK, nonce = the slot's 24-byte nonce, no associated data). The result
  is accepted only if its key check equals `key_check` in `vault.json`.
- **Remember on this computer** stores the 32-byte vault key (not the passphrase)
  encrypted with DPAPI in `%APPDATA%\AeternaVault\keys\vault-<vault id>.key`.
  Only the same Windows user on the same computer can decrypt it.

### Replacing keys

- *Change passphrase* replaces the passphrase slot (same Argon2id strength).
- *New recovery key* replaces the recovery-key slot; the old key stops working.
- The **vault key itself cannot be replaced** without re-encrypting every backup.
  If you believe the vault key was exposed (for example, malware ran as your
  Windows user while the key was remembered), set up encryption at a new
  destination and make a fresh backup there.

## Layout

```text
<destination>\AeternaVault Encrypted\
  vault.json                          public header (see below)
  README.txt                          explanation for people who find the folder
  Open with AeternaVault.avault       double-click target (plain text, no secrets)
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
      "salt": "<16 bytes, hex>",
      "nonce": "<24 bytes, hex>",
      "wrapped_key": "<48 bytes: 32 ciphertext + 16 tag, hex>"
    },
    { "kind": "recovery-key", "kdf": { "memory_kib": 19456, "iterations": 2, "parallelism": 1 }, "…": "…" }
  ]
}
```

`cipher` is `argon2id+xchacha20poly1305-stream-1mib` or
`argon2id+aes256gcm-stream-1mib` and names the cipher used for new streams. Hex is
lower-case.

## Encrypted streams (`.avb`, `.avs`)

Plaintext is split into chunks of exactly 1 048 576 bytes; the last chunk may be
shorter, and an empty plaintext is one empty chunk. Each chunk is sealed
separately (ciphertext followed by its 16-byte tag). A reader knows a chunk is the
last one when no further bytes follow it; the nonce includes that flag, so
truncation at a chunk boundary, reordering, duplication and appending are all
detected.

Both formats can be read with any key of the vault: the first four bytes name
the format.

### `AVB1` — XChaCha20-Poly1305

```text
offset 0   "AVB1"                           4 bytes
offset 4   prefix                           19 random bytes
offset 23  chunk₀ ‖ chunk₁ ‖ …              each: ciphertext ‖ 16-byte tag

key   = data key
nonce = prefix (19) ‖ chunk index as u32 big-endian (4) ‖ last (1: 0x01 or 0x00)   → 24 bytes
```

### `AVG1` — AES-256-GCM

```text
offset 0   "AVG1"                           4 bytes
offset 4   salt                             32 random bytes
offset 36  chunk₀ ‖ chunk₁ ‖ …              each: ciphertext ‖ 16-byte tag

key   = HMAC-SHA-256(key = data key, message = "AeternaVault v1 AES-GCM stream key" ‖ salt)
nonce = 7 zero bytes ‖ chunk index as u32 big-endian (4) ‖ last (1)               → 12 bytes
```

Every AES-GCM stream has its own key, so the short nonce never repeats under one key.

No associated data is used in either format.

## Blobs

- A file's content is one stream, stored at
  `blobs/<first two characters of id>/<id>.avb`.
- `id = hex(HMAC-SHA-256(key = name key, message = SHA-256(plaintext)))`.
- Identical content produces the same id, so it is stored once and reused by later
  backups. Without the name key, ids reveal nothing about the content.

## Snapshot index (`.avs`)

One stream per backup; its plaintext is UTF-8 JSON:

```json
{
  "header": {
    "format": 2,
    "app_version": "0.3.0",
    "id": "2026-09-14 20-00",
    "computer": "ANNA-PC",
    "user": "anna",
    "mode": "incremental",
    "status": "complete",
    "started_at": "2026-09-14T18:00:00Z",
    "finished_at": "2026-09-14T18:02:10Z",
    "base": "2026-09-13 20-00",
    "encrypted": true,
    "split": false,
    "sources": [
      { "key": "Documents", "name": "Documents", "path": "C:\\Users\\anna\\Documents", "portable": "{DOCUMENTS}" }
    ],
    "registry": [ { "app": "7zip", "app_name": "7-Zip", "key": "HKCU\\Software\\7-Zip", "values": 12, "fingerprint": "…" } ],
    "known_paths": { "values": [ ["APPDATA", "C:\\Users\\anna\\AppData\\Roaming"] ] },
    "stats": { "files": 1234, "bytes": 5678, "copied_files": 3, "copied_bytes": 10, "linked_files": 0,
               "referenced_files": 1231, "registry_keys": 1, "skipped": 0, "failed": 0 }
  },
  "index": {
    "format": 2,
    "files": [
      { "source": "Documents", "path": "Letters/2026/a.txt", "size": 13, "modified": 1757872800000000000,
        "sha256": "<hex of the plaintext>", "blob": "<blob id>" }
    ],
    "registry": [ "… exported registry keys and values …" ],
    "warnings": []
  }
}
```

- `source` is the folder inside the backup (`/`-separated, e.g.
  `Applications/Firefox`); `path` is relative to it, `/`-separated.
- `modified` is nanoseconds since 1970-01-01 UTC.
- A restore must reject paths containing `..`, drive letters or absolute paths.

The file name of an `.avs` file (the backup's date) is not encrypted.

## Partly encrypted backups

When only marked items are encrypted, one backup consists of

- a normal backup folder `<destination>\<id>\` (with `.aeternavault\snapshot.json`
  and `files.json`) holding the unmarked files, and
- `AeternaVault Encrypted\snapshots\<id>.avs` holding the marked ones,

both with `"split": true`. Each index lists only the files stored in its part.
AeternaVault lists, restores, checks, moves and deletes both parts together.

## Opening without restoring

- In AeternaVault: *Backups → Browse*, or double-click
  `Open with AeternaVault.avault` (optional association), or
  `AeternaVault.exe open <folder>`. Opening a single file decrypts it into
  `%TEMP%\AeternaVault-open\…`; those copies are removed at the next start.
- Command line on any computer: `AeternaVault.exe restore latest --destination <folder> --to <folder> --yes`
  (asks for the passphrase or reads `AETERNAVAULT_PASSPHRASE`).
- Without AeternaVault: `python tools/aeterna-decrypt.py <destination> extract latest <folder>`.

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
| Someone modifies or deletes files in the vault | Detected on restore and by *Check* (deleted data cannot be recovered) |
| Someone swaps whole `.avs` or blob files between backups of the same vault | Mostly detected by checksums; an old but authentic backup can be put back in place of a newer one |
| Weak passphrase and a determined attacker | Argon2id slows guessing; a strong passphrase is still essential |
| Malware running as your Windows user | No — it can read your files directly, and a remembered key |
| Opened files in `%TEMP%` | Decrypted copies exist until the next start of AeternaVault |
| Passphrase **and** recovery key lost | Data cannot be recovered by anyone |
