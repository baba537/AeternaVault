# Architecture

AeternaVault is a single Rust binary with a clear split between the **engine**
(what happens to files), the **platform** layer (Windows specifics) and the
**interface** (how people see and control it). Everything in this document
describes the current state and the reasons behind it. None of it is set in stone.

## Modules

```text
src/
├── main.rs            entry point: GUI without arguments, CLI with a subcommand
├── cli.rs             command line (clap): backup [--scheduled], snapshots, restore, paths
├── config.rs          TOML configuration with defaults for every field
├── paths.rs           where config and logs live (standard, portable, env override)
├── state.rs           result of the last automatic backup
├── i18n.rs            all user-facing texts, English and German
├── logging.rs         tracing: rotating log file + in-memory buffer for the GUI
├── error.rs           engine error type
├── engine/
│   ├── mod.rs         shared helpers, progress, cancellation, read-only guard test
│   ├── sources.rs     config + catalog → concrete folders and registry keys (read-only)
│   ├── selection.rs   chosen sub-folders/files of a source (read-only)
│   ├── scan.rs        walks sources (read-only)
│   ├── snapshots.rs   finds plain, encrypted and legacy backups (read-only)
│   ├── manifest.rs    on-disk format of a backup (read-only)
│   ├── plan.rs        preview / dry run: BackupPlan, RestorePlan (read-only)
│   ├── backup.rs      executes a BackupPlan (plain or encrypted)
│   ├── restore.rs     executes a RestorePlan (files and registry)
│   ├── fsops.rs       the only place that writes files; destination lock
│   ├── crypto.rs      Argon2id, XChaCha20-Poly1305 stream format, key slots
│   ├── vault.rs       encrypted vault on disk, remembered keys
│   ├── export.rs      preview export as text / CSV
│   └── tests.rs       end-to-end tests on temporary folders
├── platform/
│   ├── mod.rs         OS helpers with fallbacks (drives, free space, DPAPI, processes)
│   ├── windows.rs     a few Win32 calls via windows-sys
│   ├── apps.rs        application catalog loader, installed programs, winget export
│   ├── apps.toml      the built-in catalog (63 applications and Windows settings)
│   ├── known_paths.rs {APPDATA}-style portable paths, profile remapping
│   ├── registry.rs    HKCU export/import, .reg files
│   ├── scheduler.rs   Windows Task Scheduler task for automatic backups
│   └── vss.rs         FileReader interface; Volume Shadow Copy comes later
└── gui/
    ├── mod.rs         application state and frame layout
    ├── actions.rs     background loaders, tasks, vault and schedule handling
    ├── dialogs.rs     confirmation and encryption dialogs
    ├── theme.rs       palette, typography, fonts
    ├── widgets.rs     cards, buttons, toggle, section titles, logo, progress line
    ├── tasks.rs       worker threads with progress and cancellation
    ├── tests.rs       interface tests (egui_kittest) and screenshot rendering
    └── views/         backup, restore, preview, working/done, apps, settings, activity
```

## Data flow

Every operation is split into **plan** (read-only) and **execute**. The preview
is not a separate code path that imitates a backup — it *is* the first half of
the real backup.

```mermaid
flowchart LR
    C[config.toml] --> SRC[sources::collect]
    CAT[apps.toml catalog] --> SRC
    KP[known folders of this user] --> SRC
    SRC --> P[plan_backup]
    S[(Folders)] -->|read| P
    R[(HKCU registry)] -->|read| P
    D[(Previous backup index)] -->|read| P
    P --> BP[BackupPlan]
    BP --> PV[Preview · filter · export]
    BP --> CF[Confirmation]
    PV -->|Start| X[run_backup]
    CF -->|Start| X
    X -->|plain| N[(Backups\date\…)]
    X -->|encrypted| V[(AeternaVault Encrypted\…)]
```

```mermaid
flowchart LR
    SN[(Backup)] -->|read / decrypt| PR[plan_restore]
    T[(Target folders, registry)] -->|read| PR
    KP[known folders here] -->|resolve portable paths| PR
    PR --> RP[RestorePlan]
    RP --> PV[Preview / Confirmation]
    PV -->|Start| RR[run_restore]
    RR -->|write .partial, verify SHA-256, rename| T
    RR -->|import HKCU keys or write .reg| T
```

### Why the preview is trustworthy

- `plan.rs`, `scan.rs`, `sources.rs`, `selection.rs`, `snapshots.rs` and
  `manifest.rs` only read. A unit test (`planning_modules_are_read_only`) fails if
  they mention writing APIs, registry import or vault creation.
- `BackupPlan` / `RestorePlan` are plain data. The executors take a plan by
  reference and re-check each file's size and modification time before acting.
- End-to-end tests assert that planning creates no files and no folders.

### Moving between computers

Sources record both the absolute path and a **portable path** such as
`{APPDATA}\Mozilla\Firefox`. On restore to the original locations the tokens are
resolved for the current user. Registry string values containing the old
profile path (e.g. `C:\Users\anna\…`) are rewritten to the new profile.

### GUI threading

The engine is synchronous. The GUI runs every plan and execution on a worker
thread (`gui/tasks.rs`) and receives progress through a channel; each message
wakes the UI with `request_repaint`. Folder sizes, the backup list, application
detection and Task Scheduler changes are small background jobs as well.
Cancellation is a shared atomic flag checked between files and between 1 MiB
chunks. Panics in a worker are caught and shown as a calm error.

### Automatic backups

Turning the switch on creates `\AeternaVault\Automatic backup` in the Task
Scheduler (`schtasks /Create /XML`, current user, least privilege). The task runs
`AeternaVault.exe backup --scheduled`, which lowers CPU and I/O priority, uses a
remembered vault key if encryption is on, skips quietly when the destination is
not connected, and writes the outcome to `state.json`. A lock file in the
destination prevents two backups from running at once.

## Backup format

Plain (format 2):

```text
<destination>\
  2026-09-14 20-00\                one folder per backup; " (PC)" if several computers share it
    Documents\…                    source folders
    Applications\<App>\…           application folders (label sub-folders if several)
    Applications\<App>\Registry\*.reg
    Applications\Installed programs.txt, winget-packages.json
    .aeternavault\                 hidden: snapshot.json (header, written last), files.json (index)
```

- **Every backup is complete and browsable.** Incremental backups hard-link
  unchanged files from the previous backup (NTFS); on file systems without hard
  links the index points to the older copy (`blob` field).
- **Files are written as `*.aeterna-partial` and renamed** when complete.
- **Restores verify SHA-256** before a file replaces anything.
- **Paths from the index are sanitised** (`safe_relative_path`), so a damaged or
  crafted index cannot make a restore write outside its target.
- Format 1 backups (`<COMPUTER>\<id>\data\…`) from 0.1 are still listed,
  restorable and usable as incremental base.

Encrypted: see [ENCRYPTION.md](ENCRYPTION.md).

## Crates

| Area | Crate | Why | Alternatives |
|---|---|---|---|
| GUI | `eframe` / `egui` | One dependency, draws everything itself, easy to give a custom calm look, immediate mode keeps state simple | `iced` (Elm architecture), `slint` (declarative UI files), `tauri` (HTML/CSS UI, WebView2) |
| Renderer | `wgpu` (default) | Direct3D 12 with software fallback: works in VMs and remote sessions | `glow` feature: OpenGL, smaller executable |
| Dialogs | `rfd` | Native Windows file and folder pickers | `windows` crate `IFileOpenDialog` |
| Windows APIs | `windows-sys` | Raw bindings for DPAPI, Toolhelp, priority, attributes; fast compile | `windows` (safer wrappers; needed for VSS/COM later) |
| Registry | `winreg` | Idiomatic HKCU export/import with raw value types | `windows-registry` |
| Task Scheduler | `schtasks.exe` + XML | No COM code; the full task definition is readable | `windows` crate `ITaskService` |
| Encryption | `argon2`, `chacha20poly1305`, `hmac`, `getrandom`, `zeroize` | RustCrypto: pure Rust, widely reviewed, no C toolchain | `age` (file format, heavier dependency tree), `ring`/`aws-lc-rs` |
| Config | `serde` + `toml` | Comments allowed, friendly to hand-editing | `serde_json`, `ron` |
| Index | `serde_json` | Human-readable, robust | SQLite (`rusqlite`) for very large backups |
| Paths | `directories` | Known Folders incl. redirection (OneDrive) | `dirs`, `known-folders` |
| Walking | `walkdir` | Mature, does not follow links, fine-grained control | `jwalk` (parallel), `ignore` |
| Excludes | `globset` | Fast glob matching from ripgrep, case-insensitive | `glob`, `wildmatch` |
| Checksums | `sha2` | SHA-256 is verifiable with `Get-FileHash` | `blake3` |
| Errors | `thiserror`, `anyhow` | Typed engine errors, simple top-level handling | `snafu`, `miette` |
| Logging | `tracing`, `tracing-subscriber`, `tracing-appender` | Structured fields, rotating files, custom GUI layer | `log` + `simplelog` |
| CLI | `clap` (derive) | Standard, generates help and validation | `argh`, `pico-args` |
| Locale | `sys-locale` | Detects the Windows UI language | `GetUserDefaultLocaleName` |
| Icon | `winresource` (build) | Embeds icon and version info into the EXE | `embed-resource` |
| Interface tests | `egui_kittest` (dev) | Clicks the real UI headless via AccessKit; renders screenshots | manual testing |

## Configuration and paths

| What | Default location |
|---|---|
| Configuration | `%APPDATA%\AeternaVault\config.toml` |
| Own catalog entries | `%APPDATA%\AeternaVault\apps.toml` |
| Remembered vault keys | `%APPDATA%\AeternaVault\keys\` (DPAPI) |
| Automatic backup result | `%APPDATA%\AeternaVault\state.json` |
| Logs | `%LOCALAPPDATA%\AeternaVault\logs\aeterna-vault.YYYY-MM-DD.log` (14 days) |
| Portable mode | `AeternaVault.toml` next to the EXE, logs in `logs\` beside it |
| Override | environment variable `AETERNAVAULT_HOME` |
| Demo / test profile | `AETERNAVAULT_PROFILE_ROOT` points user folders to a fake profile and hides the real registry |

## Extension points

- **More applications:** add entries to `platform/apps.toml` or a user `apps.toml`.
- **Volume Shadow Copy:** implement `platform::vss::FileReader` and pass it to
  `backup::run_backup`.
- **Retention:** remove old plain backups and unreferenced encrypted blobs; the
  index already records where every file's content lives.
- **Languages:** see `CONTRIBUTING.md`.
