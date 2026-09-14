//! AeternaVault — Your data, kept for eternity.
//!
//! Entry point. Without arguments the graphical interface starts; with a
//! subcommand (`backup`, `restore`, `snapshots`, `config`) the app runs
//! unattended, which is handy for the Windows Task Scheduler.

// Release builds are GUI-subsystem executables so no console window flashes up.
// Debug builds keep the console for convenient logging during development.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;
mod config;
mod engine;
mod error;
mod gui;
mod i18n;
mod logging;
mod paths;
mod platform;
mod state;

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    // A GUI-subsystem process has no console. When started from a terminal
    // with arguments, attach to the parent console so output and `--help` are visible.
    if std::env::args_os().len() > 1 {
        platform::attach_parent_console();
    }

    let args = cli::Args::parse();
    let paths = paths::AppPaths::resolve();
    let log = logging::init(&paths, args.command.is_some());

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        config = %paths.config_file.display(),
        portable = paths.portable,
        "AeternaVault starting"
    );

    let loaded = config::load_or_create(&paths);

    match args.command {
        Some(command) => cli::run(command, &paths, loaded),
        None => match gui::run(paths, loaded, log.buffer) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                tracing::error!("the window could not be opened: {err:#}");
                platform::show_fatal_error(&format!(
                    "AeternaVault could not open its window.\n\n{err:#}"
                ));
                ExitCode::FAILURE
            }
        },
    }
}
