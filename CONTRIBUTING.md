# Contributing to AeternaVault

Thank you for taking the time. Contributions of every size are welcome — bug
reports, translations, documentation and code.

## Getting started

1. Install Rust with [rustup](https://rustup.rs/). On Windows the MSVC toolchain
   is recommended, which needs the *Visual Studio Build Tools* with the
   "Desktop development with C++" workload.
2. Clone the repository and run:

   ```powershell
   cargo run                 # starts the GUI (debug build, with console log)
   cargo test                # engine, encryption, registry and interface tests
   cargo run -- backup --dry-run
   ```

3. To try things without touching your real settings, point AeternaVault to a
   separate folder — and, for application settings, to a fake user profile:

   ```powershell
   $env:AETERNAVAULT_HOME = "$env:TEMP\aeterna-dev"
   $env:AETERNAVAULT_PROFILE_ROOT = "$env:TEMP\aeterna-profile"   # optional
   cargo run
   ```

   With `AETERNAVAULT_PROFILE_ROOT` set, `{APPDATA}` and the other folders
   resolve inside that folder, and the real registry and program list are hidden.

## Interface tests and screenshots

`src/gui/tests.rs` drives the real interface without a window using
[`egui_kittest`](https://crates.io/crates/egui_kittest): widgets are found by
their accessible label. Custom widgets therefore set `widget_info` — please keep
that when adding new ones (it also helps screen readers).

The README screenshots are rendered from demo data by an ignored test:

```powershell
$env:AETERNAVAULT_HOME = '<folder with a demo config.toml>'
$env:AETERNAVAULT_PROFILE_ROOT = '<fake user profile>'
$env:AETERNAVAULT_SCREENSHOTS = 'docs\screenshots'
cargo test render_readme_screenshots -- --ignored
```

## Adding applications to the catalog

Entries live in `src/platform/apps.toml` (format described at the top of the
file). Please:

- use path tokens (`{APPDATA}`, `{LOCALAPPDATA}`, …), never absolute paths;
- exclude caches, logs, crash reports and lock files;
- use `only` for single files in large folders (e.g. `.gitconfig` in `{USERPROFILE}`);
- add `processes` so AeternaVault can warn when the application is open;
- add a `note_en` / `note_de` if something is not restorable (e.g. passwords
  protected by the Windows account);
- keep registry keys under `HKCU\`.

A test checks that every entry is valid.

## Before opening a pull request

```powershell
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

The same checks run in CI.

## Guidelines

- **The preview must stay read-only.** `engine/plan.rs`, `engine/scan.rs`,
  `engine/sources.rs`, `engine/selection.rs`, `engine/snapshots.rs`,
  `engine/manifest.rs`, `engine/verify.rs` and `engine/retention.rs` must not
  write to disk or the registry. File writing goes through `engine/fsops.rs`.
  A unit test checks this.
- **Never delete user data behind the user's back.** Restores only create or
  replace files and registry values; backups only add new backup folders and
  blobs. Deleting backups happens only on request, and never leaves another
  backup incomplete.
- **Cryptography changes** need a matching update of `docs/ENCRYPTION.md` and of
  `tools/aeterna-decrypt.py`, and a new format version; existing vaults must stay
  readable. Check the script against sample vaults:

  ```powershell
  $env:AETERNAVAULT_REFERENCE_DIR = "$env:TEMP\aeterna-reference"
  cargo test write_reference_vaults -- --ignored
  $env:AETERNAVAULT_PASSPHRASE = 'reference passphrase'
  python tools\aeterna-decrypt.py "$env:TEMP\aeterna-reference\aes256gcm\Vault" extract latest out
  ```
- **Tests must not change the system.** Do not register autostart entries or file
  associations, use the recycle bin, or write outside temporary folders (the
  registry test uses a throw-away HKCU key). `platform::system_changes_allowed()`
  is false in tests and in demo mode.
- **Code and comments in English.** Keep comments short and explain *why*
  something is done, especially around Windows APIs.
- **User-facing text lives in `src/i18n.rs`.** Add every new text in English and
  German. Tone: factual, friendly, calm; no exclamation marks, no jargon.
- **Keep dependencies lean.** Explain new crates in the pull request.
- **Design** follows [docs/DESIGN.md](docs/DESIGN.md): one gold accent, serif
  headings, generous spacing, quiet motion.

## Adding a language

1. Add a variant to `Lang` and a `LanguageSetting` value in `src/config.rs`.
2. Create a new `static XX: Tr = Tr { ... }` in `src/i18n.rs` — the compiler
   lists every missing text.
3. Extend the `match` arms in `Lang`'s helper functions.

## Commit messages

Short imperative summary line (e.g. "Add retention policy for old backups"),
followed by an optional body explaining the reason for the change.

## License

By contributing you agree that your work is dual-licensed under the MIT license
and the Apache License 2.0, like the rest of the project.
