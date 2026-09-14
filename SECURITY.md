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

## Scope

Examples of issues that are in scope:

- a restore writing outside the selected target folder or outside `HKEY_CURRENT_USER`
- the preview (dry run) changing anything on disk or in the registry
- a backup silently missing files or storing damaged content
- unsafe handling of paths from `files.json`, `.avs` indexes, `apps.toml` or `config.toml`
- weaknesses in the encryption format or its implementation
  (see [docs/ENCRYPTION.md](docs/ENCRYPTION.md)), including leaks of passphrases
  or keys into logs or files

## Current limitations

- Plain (unencrypted) backups are as readable as the original data. Turn on
  encryption for cloud folders and sensitive data.
- The encryption has not had an external audit yet.
- A vault key remembered for automatic backups is protected by Windows DPAPI:
  software running as your Windows user can use it.
- Release executables are **not code-signed** unless a signing certificate has
  been configured; Windows SmartScreen may warn. Compare the SHA-256 checksum
  from `SHA256SUMS.txt` before running a downloaded file.
