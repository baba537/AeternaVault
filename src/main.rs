//! AeternaVault — Your data, kept for eternity.
//!
//! Entry point. Without arguments the graphical interface starts; with a
//! subcommand (`backup`, `restore`, `snapshots`, `paths`) the app runs
//! unattended from the command line.

// Release builds are GUI-subsystem executables so no console window flashes up.
// Debug builds keep the console for convenient logging during development.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod automatic;
mod cli;
mod config;
mod engine;
mod error;
mod gui;
mod history;
mod i18n;
mod logging;
mod paths;
mod platform;
mod state;

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    // A GUI-subsystem process has no console. When started from a terminal
    // with a command, attach to the parent console so output and `--help` are visible.
    let args_os: Vec<_> = std::env::args_os().collect();
    let background_only = args_os.len() == 2 && args_os[1] == platform::autostart::ARGUMENT;
    if args_os.len() > 1 && !background_only {
        platform::attach_parent_console();
    }

    let args = cli::Args::parse();
    let paths = paths::AppPaths::resolve();
    let log = logging::init(&paths, args.command.is_some());

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        config = %paths.config_file.display(),
        portable = paths.portable,
        background = args.background,
        "AeternaVault starting"
    );

    let loaded = config::load_or_create(&paths);

    let Some(command) = args.command else {
        return run_window(paths, loaded, log, args.background);
    };
    if let cli::Command::Add { folder } = &command {
        // Handed to the window, which may already be running.
        if let Err(err) = platform::context_menu::pending::push(&paths.config_file, folder) {
            tracing::error!("folder could not be handed to the window: {err}");
            return ExitCode::FAILURE;
        }
        return run_window(paths, loaded, log, false);
    }
    if let cli::Command::Open { path } = command {
        return open_window(
            paths,
            loaded,
            log,
            gui::StartOptions {
                hidden: false,
                instance_key: String::new(),
                viewer: Some(path),
            },
        );
    }
    cli::run(command, &paths, loaded)
}

fn run_window(
    paths: paths::AppPaths,
    loaded: config::Loaded,
    log: logging::Logging,
    background: bool,
) -> ExitCode {
    let key = platform::instance::key_for(&paths.config_file);
    let Some(_instance) = platform::instance::claim(&key) else {
        if !background && !platform::instance::activate_existing(&key) {
            tracing::warn!("another AeternaVault window is running but did not respond");
        }
        return ExitCode::SUCCESS;
    };
    let options = gui::StartOptions {
        hidden: background,
        instance_key: key,
        viewer: None,
    };
    open_window(paths, loaded, log, options)
}

fn open_window(
    paths: paths::AppPaths,
    loaded: config::Loaded,
    log: logging::Logging,
    options: gui::StartOptions,
) -> ExitCode {
    match gui::run(paths, loaded, log.buffer.clone(), options) {
        Ok(()) => {
            tracing::info!("window closed");
            ExitCode::SUCCESS
        }
        Err(err) => {
            tracing::error!("the window could not be opened: {err:#}");
            platform::show_fatal_error(&format!(
                "AeternaVault could not open its window.\n\n{err:#}"
            ));
            ExitCode::FAILURE
        }
    }
}
