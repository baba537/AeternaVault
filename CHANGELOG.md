# Changelog

All notable changes to AeternaVault are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.2.0] - 2026-09-14

Everything needed to move to a new computer, encryption for sensitive data and
cloud folders, and automatic backups.

### Added

- **Application settings.** A catalog of 63 applications and Windows settings
  (browsers, e-mail, office, development tools, media, game saves, password
  managers, utilities, keyboard layouts, Explorer options, fonts and more).
  Folders and HKCU registry keys are backed up; on restore they go to the right
  place for the current user, even on another computer with a different user
  name. Registry settings are also saved as `.reg` files. The catalog can be
  extended with an `apps.toml` next to the configuration.
- **Choose contents of a folder.** A tree with checkboxes to keep only some
  sub-folders and files of a source.
- **Encrypted backups (optional).** Passphrase plus recovery key, Argon2id and
  XChaCha20-Poly1305, deduplicated encrypted blobs suited to cloud folders.
  Unlock in the Restore view; change the passphrase in Settings. See
  docs/ENCRYPTION.md.
- **Automatic backups** through the Windows Task Scheduler: daily, weekly, every
  few hours or at sign-in; makes up missed backups, optional only on mains power.
  The result of the last automatic run is shown in the app.
- **Restore selection:** choose which folders and applications to restore.
- A list of installed programs and a `winget` export are saved with every backup.
- Warnings when applications whose settings are backed up or restored are open.
- Interface tests with `egui_kittest`; custom widgets expose accessible labels
  for screen readers.
- `backup --scheduled` and `AETERNAVAULT_PASSPHRASE` for unattended runs.

### Changed

- **Flatter backup folders:** `Backups\2026-09-14 20-00\Documents\…` instead of
  `Backups\COMPUTER\2026-09-14_200000\data\Documents\…`. Metadata moved into a
  hidden `.aeternavault` folder. Backups made with 0.1 remain listed and
  restorable.
- The first start names default folders in the language of the system.
- Minimum supported Rust version is now 1.88.

### Fixed

- Release notes no longer include the changelog's link definitions.

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

[Unreleased]: https://github.com/baba537/AeternaVault/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/baba537/AeternaVault/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/baba537/AeternaVault/releases/tag/v0.1.0
