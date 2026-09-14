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
   cargo test                # engine and configuration tests
   cargo run -- backup --dry-run
   ```

3. To try things without touching your real settings, point AeternaVault to a
   separate folder:

   ```powershell
   $env:AETERNAVAULT_HOME = "$env:TEMP\aeterna-dev"
   cargo run
   ```

## Before opening a pull request

```powershell
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

The same checks run in CI.

## Guidelines

- **The preview must stay read-only.** `engine/plan.rs`, `engine/scan.rs`,
  `engine/snapshots.rs` and `engine/manifest.rs` must not write to disk. All
  writing goes through `engine/fsops.rs`. A unit test checks this.
- **Never delete user data.** Restores only create or replace files; backups
  only add new snapshot folders.
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
