# Roadmap

Ideas for the next steps, roughly ordered by value for everyday users. Each item
is optional, and the order is open for discussion.

## Next

- [ ] **Volume Shadow Copy (VSS).** Consistent copies of files in use (Outlook
      PST, browser databases). Needs administrator rights; implement
      `platform::vss::FileReader` with `IVssBackupComponents`. Probably as a small
      helper started elevated only for that step.
- [ ] **Scheduled checks of older backups.** Jobs can check the backup they
      just made; checking all backups e.g. once a month is still open.
- [ ] **Restore single files directly** to their original place from the browse
      view (today: copy to a folder, or restore whole folders).
- [ ] **Quiet notifications** (Windows toasts) when an automatic backup could not
      run for a while.
- [ ] **Recovery sheet.** A printable page with the recovery key as text and QR
      code, plus instructions.

## Later

- [ ] **More application knowledge.** Microsoft Store apps (`PackageManager`),
      Wi-Fi profiles (`netsh wlan export`, needs administrator rights for keys),
      default app associations (`dism /Export-DefaultAppAssociations`), printer
      settings. Community-maintained catalog updates.
- [ ] **Owner filter.** Optionally back up only files owned by the current user.
- [ ] **Compression.** Optional zstd per file (plain: `.zst` suffix; encrypted:
      before encryption), or a pack format for many small files.
- [ ] **Encrypted pack files.** Combine small blobs into larger packs to reduce
      the number of files in cloud folders.
- [ ] **Network destinations.** Credentials via Windows Credential Manager,
      reconnect and retry.
- [ ] **Preserve more metadata.** Attributes, creation time, ACLs, alternate data
      streams — each optional.
- [ ] **Faster scans.** Parallel walking and the NTFS USN journal.
- [ ] **Compact index.** SQLite or compressed JSON for millions of files.
- [ ] **Re-encrypting a vault with a new vault key** (full key rotation). Today the
      passphrase and the recovery key can be replaced without re-encrypting; a
      new vault key requires copying all data into a new vault.
- [ ] **Export to a standard format** such as `age` or 7-Zip AES for sharing.

### Considered and not planned for now

- **Several destinations at once** (local, USB, NAS, cloud). Today all jobs use
  the one destination; a realistic next step is an optional destination per job. Cloud APIs (S3, OneDrive API) would add accounts and credentials; syncing a
  local cloud folder already works.
- **Databases and virtual machines** (MySQL, PostgreSQL, MongoDB, Hyper-V,
  VirtualBox, VMware, Docker images). Consistent copies need the database's own
  dump tool or a snapshot of the running machine; copying the files while they
  are in use gives broken backups. Recommended: let the tool write a dump into a
  folder and back up that folder. SQLite files that are not in use are fine.
- **Compliance labels** (GDPR, HIPAA, ISO 27001). These certify organisations and
  their processes, not a tool. AeternaVault supports them with encryption,
  local-only data, retention rules and the activity history.
- **Game saves of Epic and GOG.** Saves live in per-game folders; most are in
  *Saved Games* or *Documents\My Games*, which the catalog already covers, or in
  the launchers' cloud sync.

## Trust and distribution

- [ ] External security review of the encryption format and implementation.
- [ ] Code signing certificate (the release workflow already has an optional step).
- [ ] A second maintainer with release rights.
- [ ] Reproducible builds (pinned toolchain is in place; bit-identical output is
      not yet verified).
- [ ] `winget` manifest and/or an MSI / MSIX installer.
- [ ] ARM64 Windows build.
- [ ] More languages (French, Italian, Spanish …) — see CONTRIBUTING.md.

## Done in 0.4.0

- [x] Calmer window: one-time settings in Settings, Backup jobs tab
- [x] Activity history across sessions
- [x] Retention forecast
- [x] Passphrase rating by length, kinds of characters and patterns; no minimum
- [x] Start with Windows via StartupApproved; double-click on by default
- [x] Explorer menu "Back up with AeternaVault"; check a backup after a job
- [x] Catalog: Outlook PST, cloud/container/WSL configuration
- [x] Command-line documentation

## Done in 0.3.0

- [x] Backups view: check, browse, open files, copy out, move, delete
- [x] Retention rules with preview
- [x] Choice of cipher (XChaCha20-Poly1305 / AES-256-GCM) and Argon2id strength
- [x] Encrypt only selected folders and files
- [x] Test and replace the recovery key; save it as a file
- [x] Open encrypted backups without restoring, by double-click or `open`
- [x] Independent Python decryptor, byte-level format documentation
- [x] Automatic backups run by AeternaVault (several schedules, tray, start with Windows)
- [x] Build provenance attestation and SBOM for releases

## Done in 0.2.0

- [x] Application settings catalog (folders and HKCU registry), portable paths
- [x] Choose sub-folders and files of a source
- [x] Encrypted backups with passphrase and recovery key
- [x] Automatic backups via the Task Scheduler (replaced in 0.3)
- [x] Flatter backup folder structure
- [x] Restore selection, program list and winget export, open-application warnings
- [x] Interface tests, accessible labels

## Done in 0.1.0

- [x] Folder backups, full and incremental, browsable, with checksums
- [x] Read-only preview with filters and export
- [x] Restore to original locations or another folder, with verification
- [x] English and German, dark and light
- [x] CLI, portable mode, logs, CI and release workflow
