# Changelog

All notable changes to AeternaVault are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] - 2026-09-14

The first public foundation. Usable for everyday folder backups; please keep a
second copy of important data while the project is young.

### Added

- Graphical interface in English and German, switchable at any time, with dark
  and light appearance (or following Windows).
- Backup of selected folders to a destination folder, full or incremental.
  Every backup is a complete, browsable folder; unchanged files are hard-linked
  where the file system supports it.
- SHA-256 checksum for every stored file, verified during restore.
- Strictly read-only preview (dry run) before backup and restore, with filters
  (new, changed, unchanged, no longer present, skipped), search, and export as
  text or CSV.
- Restore to the original locations or into another folder, with a choice of
  what happens to existing files. Nothing is ever deleted at the target.
- Sensible defaults on first start: personal folders, a destination on a second
  drive if available, and detected application profiles (Firefox, Thunderbird).
- Suggestions for application data worth keeping and a read-only overview of
  installed applications.
- Skips links, junctions and OneDrive online-only files; excludes with glob
  patterns; the destination is never backed up into itself.
- Hand-editable TOML configuration, portable mode, daily rotating log files and
  an activity view.
- Command line for unattended runs: `backup`, `snapshots`, `restore`, `paths`.
- GitHub Actions for CI and for publishing `AeternaVault.exe` with SHA-256
  checksums on version tags.

[Unreleased]: https://github.com/baba537/AeternaVault/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/baba537/AeternaVault/releases/tag/v0.1.0
