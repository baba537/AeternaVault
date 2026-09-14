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

- a restore writing outside the selected target folder
- the preview (dry run) changing anything on disk
- a backup silently missing files or storing damaged content
- unsafe handling of paths from `files.json` or `config.toml`

## Current limitations

- Backups are **not encrypted** yet. Keep the backup destination as private as
  the original data.
- Release executables are **not code-signed** unless a signing certificate has
  been configured; Windows SmartScreen may warn. Compare the SHA-256 checksum
  from `SHA256SUMS.txt` before running a downloaded file.
