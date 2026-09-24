# Roadmap

Open items, roughly by value. None is promised.

## Next

- **Volume Shadow Copy** on Windows for files that are in use (mail archives, browser databases). Needs administrator rights; the engine already reads through `platform::vss::FileReader`.
- **Scheduled checks** of older backups, e.g. once a month.
- **Restore single files** to their original place from the browse view.
- **Notifications** when automatic backups could not run for a while.
- **Recovery sheet**: a printable page with the recovery key and instructions.
- **Signed releases** (code signing on Windows, a signed apt repository on Linux).

## Later

- Optional destination per job, network destinations with stored credentials.
- Compression (zstd) and pack files for many small encrypted files.
- More metadata: attributes, creation time, ACLs, extended attributes.
- Faster scans (parallel walking, NTFS USN journal) and a compact index for millions of files.
- Full key rotation of an encrypted vault.
- Packages for more Linux distributions (Flatpak, RPM) and ARM64 builds.

## Not planned

- **Cloud APIs** (S3, OneDrive API). Backing up into a folder that a sync client uploads already works; encrypted backups upload only new content.
- **Databases and virtual machines.** Consistent copies need the database's own dump tool or a snapshot of the running machine. Let the tool write a dump into a folder and back up that folder.
- **Registry and sign-ins** of applications. Only data folders are backed up; settings stored in the registry and sign-in tokens are left out on purpose.
