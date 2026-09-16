# Changelog

All notable changes to AeternaVault are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.3.0] - 2026-09-16

Managing backups, a choice of how and what to encrypt, encrypted backups that
can be opened without restoring (and without AeternaVault), and automatic
backups run by AeternaVault itself.

### Added

- **Backups view.** Every backup at the destination with its size and status:
  - check it without restoring (plain files are hashed, encrypted data is decrypted);
  - browse its contents with search, open single files from a temporary copy,
    and copy chosen files or everything to a folder;
  - move it to another location (copied and checked before the original goes);
  - delete one or several (recycle bin where the drive has one).
  Newer backups that still need files of a deleted one get their own copies
  first; encrypted data no longer used by any backup is cleaned up.
- **Retention rules:** keep the newest *n* backups and the newest of each of the
  last days, weeks and months; preview and "remove now", or automatically after
  every backup. Only backups of the same computer are considered.
- **Choice of encryption method** when setting up encryption: XChaCha20-Poly1305
  (default) or AES-256-GCM, and three Argon2id strengths for the passphrase.
- **Encrypt only selected items:** mark folders and files with a lock in the
  contents tree, optionally also application settings. Such backups consist of a
  normal folder and an encrypted part with the same name and are handled as one.
- **How it is encrypted:** an overview of cipher, key derivation, what stays
  hidden, where the key is kept and how to get at the files without AeternaVault.
- **Recovery key:** save it as a text file, test it, or replace it.
- **Open encrypted backups by double-click** on `Open with AeternaVault.avault`
  (optional file association), or `AeternaVault.exe open <folder>`: asks for the
  passphrase and shows the backups without changing any settings.
- `snapshots` and `restore` accept `--destination` and ask for the passphrase, so
  backups can be restored from a copied folder on any computer.
- `tools/aeterna-decrypt.py`: an independent Python decryptor for encrypted
  backups, written only from the format documentation.
- **Several automatic backups**, each for all or only some folders.
- **Notification area icon** while automatic backups are on: open, back up now,
  quit. Closing the window keeps AeternaVault running there (can be turned off).
- Optional **start with Windows** (in the background).
- Interface size setting and a "compatibility graphics" option (OpenGL).
- Passphrase fields can show what was typed.

### Changed

- **Automatic backups no longer use the Windows Task Scheduler.** AeternaVault
  runs them itself while it is open or in the notification area; the task created
  by 0.2 is removed once and its schedule is taken over (with start with Windows).
- A newly chosen destination gets an `AeternaVault` folder inside (can be turned off).
- The window never opens larger than the screen; it prefers Direct3D 12 on the
  power-saving graphics chip and falls back to OpenGL if Direct3D fails.
- The Activity view shows only AeternaVault's own messages; libraries' warnings
  still go to the log file.
- The recovery key dialog is wider and confirms copying.
- Only one window per configuration; starting AeternaVault again brings it forward.

### Fixed

- The list of installed programs had almost no height.
- Configuration files saved with a byte order mark (older Notepad) could not be read.
- Turning automatic backups on and off quickly could install and remove the
  scheduled task several times.

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

[Unreleased]: https://github.com/baba537/AeternaVault/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/baba537/AeternaVault/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/baba537/AeternaVault/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/baba537/AeternaVault/releases/tag/v0.1.0
