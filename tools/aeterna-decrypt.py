#!/usr/bin/env python3
"""Independent decryptor for AeternaVault encrypted backups.

This script does not use any AeternaVault code. It implements the format
described in docs/ENCRYPTION.md, so encrypted backups stay readable even
without AeternaVault — and the documentation can be checked against it.

Requirements (Python 3.9+):

    pip install cryptography argon2-cffi

Usage:

    python aeterna-decrypt.py "E:\\AeternaVault" list
    python aeterna-decrypt.py "E:\\AeternaVault" extract latest "D:\\Restored"
    python aeterna-decrypt.py "E:\\AeternaVault" extract "2026-09-14 20-00" "D:\\Restored"

The passphrase or recovery key is read from the environment variable
AETERNAVAULT_PASSPHRASE or asked for. The plain part of a partly encrypted
backup is a normal folder next to "AeternaVault Encrypted" and needs no tool.
"""

import getpass
import hashlib
import hmac
import json
import os
import struct
import sys
from pathlib import Path

try:
    from argon2.low_level import Type, hash_secret_raw
    from cryptography.hazmat.primitives.ciphers.aead import AESGCM, ChaCha20Poly1305
except ImportError:  # pragma: no cover - explained to the user
    sys.exit("Please install the requirements first:  pip install cryptography argon2-cffi")

VAULT_DIR = "AeternaVault Encrypted"
CHUNK = 1024 * 1024
TAG = 16
XCHACHA = "argon2id+xchacha20poly1305-stream-1mib"
AESGCM_NAME = "argon2id+aes256gcm-stream-1mib"


# --- XChaCha20-Poly1305 = HChaCha20 subkey + ChaCha20-Poly1305 -----------------

def _rotl(v, n):
    return ((v << n) & 0xFFFFFFFF) | (v >> (32 - n))


def _quarter(s, a, b, c, d):
    s[a] = (s[a] + s[b]) & 0xFFFFFFFF
    s[d] = _rotl(s[d] ^ s[a], 16)
    s[c] = (s[c] + s[d]) & 0xFFFFFFFF
    s[b] = _rotl(s[b] ^ s[c], 12)
    s[a] = (s[a] + s[b]) & 0xFFFFFFFF
    s[d] = _rotl(s[d] ^ s[a], 8)
    s[c] = (s[c] + s[d]) & 0xFFFFFFFF
    s[b] = _rotl(s[b] ^ s[c], 7)


def hchacha20(key: bytes, nonce16: bytes) -> bytes:
    """HChaCha20 (draft-irtf-cfrg-xchacha): 32-byte key, 16-byte nonce -> subkey."""
    state = list(struct.unpack("<4I", b"expand 32-byte k"))
    state += list(struct.unpack("<8I", key))
    state += list(struct.unpack("<4I", nonce16))
    for _ in range(10):
        _quarter(state, 0, 4, 8, 12)
        _quarter(state, 1, 5, 9, 13)
        _quarter(state, 2, 6, 10, 14)
        _quarter(state, 3, 7, 11, 15)
        _quarter(state, 0, 5, 10, 15)
        _quarter(state, 1, 6, 11, 12)
        _quarter(state, 2, 7, 8, 13)
        _quarter(state, 3, 4, 9, 14)
    return struct.pack("<4I", *state[0:4]) + struct.pack("<4I", *state[12:16])


def xchacha_decrypt(key: bytes, nonce24: bytes, data: bytes) -> bytes:
    subkey = hchacha20(key, nonce24[:16])
    return ChaCha20Poly1305(subkey).decrypt(b"\x00" * 4 + nonce24[16:], data, None)


# --- Keys ------------------------------------------------------------------------

def hmac_sha256(key: bytes, data: bytes) -> bytes:
    return hmac.new(key, data, hashlib.sha256).digest()


def normalize(kind: str, secret: str) -> bytes:
    if kind == "recovery-key":
        secret = "".join(c.upper() for c in secret if c.isascii() and c.isalnum())
    return secret.encode("utf-8")


def unlock(vault: dict, secret: str) -> bytes:
    """Returns the 32-byte vault key, trying every key slot."""
    for slot in vault["slots"]:
        kdf = slot["kdf"]
        kek = hash_secret_raw(
            normalize(slot["kind"], secret),
            bytes.fromhex(slot["salt"]),
            time_cost=kdf["iterations"],
            memory_cost=kdf["memory_kib"],
            parallelism=kdf["parallelism"],
            hash_len=32,
            type=Type.ID,
            version=19,
        )
        try:
            master = xchacha_decrypt(kek, bytes.fromhex(slot["nonce"]), bytes.fromhex(slot["wrapped_key"]))
        except Exception:
            continue
        if hmac_sha256(master, b"AeternaVault v1 key check")[:8].hex() == vault["key_check"]:
            return master
    sys.exit("The passphrase or recovery key is not correct.")


# --- Encrypted streams (.avs, .avb) ---------------------------------------------

def decrypt_stream(data_key: bytes, reader, writer) -> str:
    """Decrypts one stream; returns the SHA-256 of the plaintext (hex)."""
    magic = reader.read(4)
    if magic == b"AVB1":
        prefix = reader.read(19)

        def open_chunk(counter, last, sealed):
            nonce = prefix + struct.pack(">I", counter) + bytes([last])
            return xchacha_decrypt(data_key, nonce, sealed)
    elif magic == b"AVG1":
        salt = reader.read(32)
        stream_key = hmac_sha256(data_key, b"AeternaVault v1 AES-GCM stream key" + salt)
        aead = AESGCM(stream_key)

        def open_chunk(counter, last, sealed):
            nonce = b"\x00" * 7 + struct.pack(">I", counter) + bytes([last])
            return aead.decrypt(nonce, sealed, None)
    else:
        raise ValueError("not an AeternaVault encrypted stream")

    digest = hashlib.sha256()
    current = reader.read(CHUNK + TAG)
    counter = 0
    while True:
        following = reader.read(CHUNK + TAG) if len(current) == CHUNK + TAG else b""
        last = 1 if not following else 0
        plain = open_chunk(counter, last, current)
        digest.update(plain)
        writer.write(plain)
        if last:
            return digest.hexdigest()
        current = following
        counter += 1


class _Buffer:
    def __init__(self):
        self.parts = []

    def write(self, data):
        self.parts.append(data)

    def value(self):
        return b"".join(self.parts)


def read_snapshot(data_key: bytes, path: Path) -> dict:
    out = _Buffer()
    with open(path, "rb") as reader:
        decrypt_stream(data_key, reader, out)
    return json.loads(out.value())


def safe_join(root: Path, *parts: str) -> Path:
    target = root
    for part in parts:
        for piece in part.replace("\\", "/").split("/"):
            if piece in ("", "."):
                continue
            if piece == ".." or ":" in piece:
                raise ValueError(f"unsafe path in the index: {part}")
            target = target / piece
    return target


# --- Commands --------------------------------------------------------------------

def main() -> None:
    if len(sys.argv) < 3 or sys.argv[2] not in ("list", "extract"):
        sys.exit(__doc__)
    destination = Path(sys.argv[1])
    vault_dir = destination / VAULT_DIR
    vault = json.loads((vault_dir / "vault.json").read_text(encoding="utf-8"))
    if vault["cipher"] not in (XCHACHA, AESGCM_NAME):
        sys.exit(f"Unknown cipher {vault['cipher']}; this script may be too old.")
    secret = os.environ.get("AETERNAVAULT_PASSPHRASE") or getpass.getpass("Passphrase or recovery key: ")
    master = unlock(vault, secret)
    data_key = hmac_sha256(master, b"AeternaVault v1 data key")

    snapshots = []
    for path in sorted((vault_dir / "snapshots").glob("*.avs")):
        snapshots.append((path.stem, read_snapshot(data_key, path)))
    snapshots.sort(key=lambda s: s[1]["header"]["started_at"], reverse=True)

    if sys.argv[2] == "list":
        for name, snapshot in snapshots:
            header = snapshot["header"]
            print(f"{name:<32} {header['computer']:<16} {header['status']:<24} "
                  f"{header['stats']['files']:>8} files")
        return

    if len(sys.argv) != 5:
        sys.exit(__doc__)
    wanted, target = sys.argv[3], Path(sys.argv[4])
    chosen = next((s for s in snapshots if wanted in ("latest", s[0])), None)
    if chosen is None:
        sys.exit(f"No encrypted backup named {wanted}.")
    name, snapshot = chosen
    failed = 0
    for entry in snapshot["index"]["files"]:
        blob = entry["blob"]
        source = vault_dir / "blobs" / blob[:2] / f"{blob}.avb"
        output = safe_join(target, entry["source"], entry["path"])
        output.parent.mkdir(parents=True, exist_ok=True)
        partial = output.with_name(output.name + ".partial")
        try:
            with open(source, "rb") as reader, open(partial, "wb") as writer:
                digest = decrypt_stream(data_key, reader, writer)
            if digest != entry["sha256"].lower():
                raise ValueError("checksum mismatch")
            partial.replace(output)
            seconds = entry["modified"] / 1_000_000_000
            os.utime(output, (seconds, seconds))
        except Exception as error:  # report and continue with the next file
            failed += 1
            partial.unlink(missing_ok=True)
            print(f"could not restore {entry['source']}/{entry['path']}: {error}", file=sys.stderr)
    total = len(snapshot["index"]["files"])
    print(f"{name}: {total - failed} of {total} files restored to {target}")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
