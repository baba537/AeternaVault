# Contributing

Bug reports, translations, documentation and code are welcome.

## Build and test

Install Rust with [rustup](https://rustup.rs/). On Linux, the window needs the usual development packages:

```sh
sudo apt install pkg-config libxkbcommon-dev libwayland-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev
```

```sh
cargo run --bin aeternavault                        # window
cargo run --bin aeternavault-cli -- status          # command line
cargo test                                           # engine, CLI and interface tests
cargo build --release --no-default-features --bin aeternavault-cli   # command line only
```

Before a pull request (the same checks run in CI on Windows and Linux):

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

## Trying things safely

```powershell
$env:AETERNAVAULT_HOME = "$env:TEMP\av-dev"                # own settings, history, logs
$env:AETERNAVAULT_PROFILE_ROOT = "$env:TEMP\av-profile"     # fake user profile
```

With `AETERNAVAULT_PROFILE_ROOT` set, user folders resolve inside that folder and nothing in the system is changed (autostart, Explorer menu, file association, systemd timer). Tests run the same way.

## Rules

- **Planning never writes.** `engine/plan.rs`, `scan.rs`, `sources.rs`, `selection.rs`, `snapshots.rs`, `manifest.rs`, `verify.rs` and `retention.rs` must not write; a test checks this. Writing goes through `engine/fsops.rs`.
- **Never delete user data unasked.** Backups only add; deleting a backup never leaves another one incomplete.
- **Reading encrypted backups needs the passphrase.** The remembered key is for writing, listing and checking only.
- **Format changes** need a new format version, an update of `docs/ENCRYPTION.md` and `tools/aeterna-decrypt.py`, and existing backups must stay readable.
- **Tests do not change the system.** `platform::system_changes_allowed()` is false in tests and demo mode.
- **Texts** live in `src/i18n.rs`, in English and German: factual and short.
- **Application catalog** (`src/platform/apps.toml`): only folders with real user data, paths with tokens, caches excluded, never sign-ins or saved passwords.
- Code, comments and commit messages in English.

## Screenshots

README screenshots are rendered from demo data by an ignored test (`render_readme_screenshots` in `src/gui/tests.rs`); see the comment there.

## License

Contributions are dual-licensed under MIT and Apache 2.0, like the project.
