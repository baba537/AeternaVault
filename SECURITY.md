# Security policy

AeternaVault handles personal files, so problems that could lead to data loss,
data exposure or writing outside of the chosen folders are taken seriously.

## Reporting a vulnerability

Please do **not** open a public issue. Use GitHub's private reporting instead:
*Security → Report a vulnerability* in this repository.

Helpful details:

- AeternaVault version and Windows version
- what you did, what happened, and what you expected
- whether a crafted backup folder or configuration file is involved

Reviews of the encryption format ([docs/ENCRYPTION.md](docs/ENCRYPTION.md)) are
very welcome, also as regular issues if they are not about an exploitable flaw.

## Scope

Examples of issues that are in scope:

- a restore, extract or move writing outside the selected target folder or
  outside `HKEY_CURRENT_USER`
- the preview (dry run) or the backup check changing anything on disk or in the registry
- deleting or moving a backup making another backup incomplete
- a backup silently missing files or storing damaged content
- unsafe handling of paths from `files.json`, `.avs` indexes, `apps.toml` or `config.toml`
- weaknesses in the encryption format or its implementation, including leaks of
  passphrases or keys into logs or files
- decrypted copies of opened files outliving their documented clean-up

## Verifying a download

- Compare the SHA-256 checksum with `SHA256SUMS.txt` of the release.
- Check the build provenance (signed by GitHub for this repository's release
  workflow): `gh attestation verify .\AeternaVault.exe --repo baba537/AeternaVault`
- Each release lists its dependencies in `sbom.cdx.json` (CycloneDX).

## Current limitations

- The project is maintained by one person and has **not had an external
  security audit**. It combines standard primitives (Argon2id, XChaCha20-Poly1305,
  AES-256-GCM, HMAC-SHA-256 from RustCrypto), documented byte by byte, and an
  independent Python implementation reads the format.
- Plain (unencrypted) backups are as readable as the original data. Turn on
  encryption for cloud folders and sensitive data.
- A vault key remembered for automatic backups is protected by Windows DPAPI:
  software running as your Windows user can use it. While AeternaVault runs with
  unlocked backups, the key is in its memory.
- Files opened from an encrypted backup are decrypted into
  `%TEMP%\AeternaVault-open` and removed at the next start of AeternaVault.
- The vault key cannot be rotated in place (passphrase and recovery key can).
- Release executables are **not code-signed** unless a signing certificate has
  been configured; Windows SmartScreen may warn.
