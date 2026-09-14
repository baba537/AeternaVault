# Roadmap

Ideas for the next steps, roughly ordered by value for everyday users. Each item
is optional, and the order is open for discussion.

## Next

- [ ] **Keeping and removing old backups.** Retention rules such as "all of the
      last 7 days, one per week for 3 months, one per month forever", with a
      preview of what would be removed. Especially useful with automatic backups.
      Must respect `blob` references into older backups and shared encrypted blobs.
- [ ] **Check a backup.** Re-read every stored file (or decrypt every blob) and
      compare SHA-256, report damaged or missing files; optionally on a schedule.
- [ ] **Browse and restore single files.** A tree view of a backup with search,
      and "restore this file / folder" — also for encrypted backups.
- [ ] **Volume Shadow Copy (VSS).** Consistent copies of files in use (Outlook
      PST, browser databases). Needs administrator rights; implement
      `platform::vss::FileReader` with `IVssBackupComponents`.
- [ ] **Notifications.** A quiet Windows toast when an automatic backup could not
      run (e.g. passphrase not remembered for a long time).

## Later

- [ ] **More application knowledge.** Microsoft Store apps (`PackageManager`),
      Wi-Fi profiles (`netsh wlan export`, needs administrator rights for keys),
      default app associations (`dism /Export-DefaultAppAssociations`), printer
      settings. Community-maintained catalog updates.
- [ ] **Owner filter.** Optionally back up only files owned by the current user
      (`GetNamedSecurityInfoW` / NTFS owner SID).
- [ ] **Compression.** Optional zstd per file (plain: `.zst` suffix; encrypted:
      before encryption), or a pack format for many small files.
- [ ] **Encrypted pack files.** Combine small blobs into larger packs to reduce
      the number of files in cloud folders.
- [ ] **Network destinations.** Better handling of NAS / SMB shares: credentials
      via Windows Credential Manager, reconnect and retry.
- [ ] **Preserve more metadata.** Attributes (read-only, hidden), creation time,
      ACLs, alternate data streams — each optional.
- [ ] **Faster scans.** Parallel walking (`jwalk`) and NTFS USN journal for
      change detection on large folders.
- [ ] **Compact index.** SQLite or compressed JSON for backups with millions of files.
- [ ] **Tray icon** with last backup status (optional, for people who prefer it).

## Distribution

- [ ] Code signing certificate (the release workflow already has an optional step).
- [ ] `winget` manifest and/or an MSI / MSIX installer (e.g. `cargo-wix`).
- [ ] ARM64 Windows build.
- [ ] More languages (French, Italian, Spanish …) — see CONTRIBUTING.md.
- [ ] External security review of the encryption format.

## Done in 0.2.0

- [x] Application settings catalog (folders and HKCU registry), portable paths
- [x] Choose sub-folders and files of a source
- [x] Encrypted backups with passphrase and recovery key
- [x] Automatic backups via the Task Scheduler
- [x] Flatter backup folder structure
- [x] Restore selection, program list and winget export, open-application warnings
- [x] Interface tests, accessible labels

## Done in 0.1.0

- [x] Folder backups, full and incremental, browsable, with checksums
- [x] Read-only preview with filters and export
- [x] Restore to original locations or another folder, with verification
- [x] English and German, dark and light
- [x] CLI, portable mode, logs, CI and release workflow
