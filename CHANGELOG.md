# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [Semantic Versioning](https://semver.org/).

## [0.5.0] - 2026-09-24

### Added

- Linux support: window and command line, `.deb` packages and tarballs; background jobs through a systemd user timer.
- `aeternavault-cli`: a separate command-line program that covers everything the window does (backups, restore, folders, applications, jobs, retention, destination, encryption, settings, history), with `--json` and exit codes for scripts.
- PowerShell module `AeternaVault` (Windows PowerShell 5.1 and PowerShell 7).
- Windows installer: Program Files, `aeternavault-cli` on `PATH`, updates keep settings, uninstall removes autostart and all registry entries.
- True black appearance for OLED screens.
- Tooltips for every setting; a list of keyboard and mouse shortcuts in the settings.
- Keyboard: Ctrl+1…7 and Ctrl+Tab switch tabs, Alt+←/→ and the mouse side buttons go back and forward, Ctrl+B backs up, Esc goes back. Middle-click scrolling.
- The backup list shows when each backup will be removed by the retention rules.
- Passphrase rating that recognises repetitions, dictionary words and keyboard patterns (zxcvbn).
- Default destination: the drive with the most free space other than the system drive.

### Changed

- All backups use one layout: a folder per date. Encrypted backups keep their index in that folder; the shared keys and contents are in a hidden `.aeternavault` folder. Encrypted backups of earlier versions are moved into this layout automatically.
- Applications: only well-known data folders (browsers, mail, notes, keys, games, OBS) are offered, and only when they exist. Choosing one adds its folders to the folders to back up, where their contents can be adjusted. Sign-ins and saved passwords are left out.
- Larger interface and dialogs; the tabs can be used from every screen.
- The window program is `aeternavault.exe`; command-line use moved to `aeternavault-cli`.

### Removed

- Preview (dry run) and its export.
- Registry settings, lists of installed programs and the winget export.
- `README.txt` and the application list files inside backups.

### Fixed

- Encrypted backups could be browsed and opened without entering the passphrase when the key was remembered. Reading encrypted names or contents now always requires the passphrase or recovery key.
- The passphrase criteria were shown only when pointing at the strength bar; the (?) button now shows them too and keeps them open when clicked.

[0.5.0]: https://github.com/baba537/AeternaVault/releases/tag/v0.5.0
