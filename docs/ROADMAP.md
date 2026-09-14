# Roadmap

Ideas for the next steps, roughly ordered by value for everyday users. Each item
is optional, and the order is open for discussion.

## Next

- [ ] **Scheduled backups.** A settings page that creates a Windows Task
      Scheduler entry running `AeternaVault.exe backup` (daily / weekly / at
      logon), plus a quiet notification when it finishes.
- [ ] **Keeping and removing old backups.** Retention rules such as "all of the
      last 7 days, one per week for 3 months, one per month forever", with a
      preview of what would be removed. Must respect `blob` references into
      older snapshots.
- [ ] **Check a backup.** Re-read every stored file and compare SHA-256, report
      damaged or missing files.
- [ ] **Browse and restore single files.** A tree view of a snapshot with
      search, and "restore this file / folder".
- [ ] **Volume Shadow Copy (VSS).** Consistent copies of files in use (Outlook
      PST, browser databases). Needs administrator rights; implement
      `platform::vss::FileReader` with `IVssBackupComponents`.

## Later

- [ ] **Registry and application settings.** Export selected HKCU keys
      (`reg export` format) per application and re-import on restore.
- [ ] **Installed applications list for restore.** Save the list of installed
      programs (desktop and Microsoft Store via `PackageManager`) with every
      backup, as a checklist after reinstalling Windows.
- [ ] **Default app associations.** Store the output of
      `dism /Online /Export-DefaultAppAssociations` for reference.
- [ ] **Owner filter.** Optionally back up only files owned by the current user
      (`GetNamedSecurityInfoW` / NTFS owner SID).
- [ ] **Compression.** Optional zstd per file, keeping the browsable layout
      (`.zst` suffix) or a pack format for many small files.
- [ ] **Encryption.** Optional, password-based (e.g. `age` with scrypt), with a
      very clear warning that a lost password means lost data.
- [ ] **Network destinations.** Better handling of NAS / SMB shares: credentials
      via Windows Credential Manager, reconnect and retry.
- [ ] **Preserve more metadata.** Attributes (read-only, hidden), creation time,
      ACLs, alternate data streams — each optional.
- [ ] **Faster scans.** Parallel walking (`jwalk`) and NTFS USN journal for
      change detection on large folders.
- [ ] **Compact index.** SQLite or compressed JSON for backups with millions of files.
- [ ] **Tray icon** with last backup status.

## Distribution

- [ ] Code signing certificate (the release workflow already has an optional step).
- [ ] `winget` manifest and/or an MSI / MSIX installer (e.g. `cargo-wix`).
- [ ] ARM64 Windows build.
- [ ] More languages (French, Italian, Spanish …) — see CONTRIBUTING.md.

## Done in 0.1.0

- [x] Folder backups, full and incremental, browsable, with checksums
- [x] Read-only preview with filters and export
- [x] Restore to original locations or another folder, with verification
- [x] English and German, dark and light
- [x] Application data suggestions, installed programs overview
- [x] CLI, portable mode, logs, CI and release workflow
