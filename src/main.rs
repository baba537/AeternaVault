//! `aeternavault`: the window.
//!
//! ```text
//! aeternavault                  open the window
//! aeternavault --background     start in the notification area (Windows autostart)
//! aeternavault open <path>      browse the backups at <path> without changing settings
//! aeternavault add <folder>     add a folder to the folders to back up
//! aeternavault --quit           close running windows (used by the installer)
//! ```
//!
//! Everything else is done with `aeternavault-cli`.

// Release builds are GUI-subsystem executables so no console window flashes up.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::process::ExitCode;

use aeterna_vault::{config, gui, logging, paths, platform};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "aeternavault",
    version,
    about = "AeternaVault",
    after_help = "Use aeternavault-cli for the command line."
)]
struct Args {
    /// Start without a window, in the notification area.
    #[arg(long)]
    background: bool,
    /// Close every running AeternaVault window of this user, then exit.
    #[arg(long, conflicts_with = "background")]
    quit: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Browse the backups at a folder (or its "Open with AeternaVault.avault"
    /// file) without changing any settings.
    Open { path: PathBuf },
    /// Add a folder to the folders to back up and show the window.
    Add { folder: PathBuf },
}

fn main() -> ExitCode {
    let args = Args::parse();
    if args.quit {
        return if platform::instance::quit_all(std::time::Duration::from_secs(30)) {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    let paths = paths::AppPaths::resolve();
    let log = logging::init(&paths, false);
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        config = %paths.config_file.display(),
        background = args.background,
        "AeternaVault window starting"
    );
    let loaded = config::load_or_create(&paths);

    match args.command {
        Some(Command::Open { path }) => open_window(
            paths,
            loaded,
            log,
            gui::StartOptions {
                hidden: false,
                instance_key: String::new(),
                viewer: Some(path),
            },
        ),
        Some(Command::Add { folder }) => {
            // Handed to the window, which may already be running.
            if let Err(err) = platform::context_menu::pending::push(&paths.config_file, &folder) {
                tracing::error!("folder could not be handed to the window: {err}");
                return ExitCode::FAILURE;
            }
            run_window(paths, loaded, log, false)
        }
        None => run_window(paths, loaded, log, args.background),
    }
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
