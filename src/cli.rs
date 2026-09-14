//! Command-line interface for unattended use, e.g. from the Task Scheduler:
//!
//! ```text
//! AeternaVault.exe backup                 back up with the saved settings
//! AeternaVault.exe backup --dry-run       only show what would happen
//! AeternaVault.exe snapshots              list backups
//! AeternaVault.exe restore latest --to D:\Restored --dry-run
//! ```
//!
//! Encrypted backups use the key remembered on this computer, or the
//! passphrase from the environment variable `AETERNAVAULT_PASSPHRASE`.
//!
//! Exit codes: 0 = success, 1 = error, 2 = completed with notes or
//! confirmation (`--yes`) missing.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};

use crate::config::{BackupMode, Config, ConflictPolicy, Loaded};
use crate::engine::crypto::VaultKey;
use crate::engine::export::{self, ExportLabels};
use crate::engine::manifest::SnapshotStatus;
use crate::engine::plan::{self, BackupInput, ItemKind, Plan, RestoreOptions, RestoreTarget};
use crate::engine::sources::Sources;
use crate::engine::{CancelToken, backup, restore, snapshots, vault};
use crate::error::EngineError;
use crate::i18n::Lang;
use crate::paths::AppPaths;
use crate::platform::apps::Catalog;
use crate::platform::known_paths::KnownPaths;
use crate::platform::{self, vss::LiveFiles};
use crate::state::{self, AutomaticOutcome, AutomaticRun, State};

pub const PASSPHRASE_ENV: &str = "AETERNAVAULT_PASSPHRASE";

#[derive(Parser, Debug)]
#[command(
    name = "AeternaVault",
    version,
    about = "AeternaVault — Your data, kept for eternity.",
    after_help = "Without a command, the graphical interface starts."
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Back up all enabled folders and applications using the saved configuration.
    Backup {
        /// Only show what would be backed up; change nothing.
        #[arg(long)]
        dry_run: bool,
        /// Copy every file again instead of only new and changed ones.
        #[arg(long)]
        full: bool,
        /// Write the plan as CSV to this file.
        #[arg(long, value_name = "FILE")]
        csv: Option<PathBuf>,
        /// Run quietly with low priority and record the result (used by automatic backups).
        #[arg(long)]
        scheduled: bool,
    },
    /// List the backups found at the destination.
    Snapshots,
    /// Restore a backup: "latest" or a name as shown by `snapshots`.
    Restore {
        snapshot: String,
        /// Restore into this folder instead of the original locations.
        #[arg(long, value_name = "DIR")]
        to: Option<PathBuf>,
        /// Only show what would be restored; change nothing.
        #[arg(long)]
        dry_run: bool,
        /// Confirm that files may be written.
        #[arg(long)]
        yes: bool,
        /// What to do with files that already exist.
        #[arg(long, value_enum, default_value_t = ConflictArg::ReplaceChanged)]
        conflict: ConflictArg,
    },
    /// Show where configuration and logs are stored.
    Paths,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ConflictArg {
    ReplaceChanged,
    KeepExisting,
    KeepNewer,
}

impl From<ConflictArg> for ConflictPolicy {
    fn from(value: ConflictArg) -> Self {
        match value {
            ConflictArg::ReplaceChanged => ConflictPolicy::ReplaceChanged,
            ConflictArg::KeepExisting => ConflictPolicy::KeepExisting,
            ConflictArg::KeepNewer => ConflictPolicy::KeepNewer,
        }
    }
}

/// The vault key from the remembered key file or `AETERNAVAULT_PASSPHRASE`.
pub fn unattended_key(paths: &AppPaths, config: &Config) -> Option<VaultKey> {
    let header = vault::read_header(&config.destination).ok()?;
    if let Some(key) = vault::remembered_key(&state::key_dir(&paths.config_file), &header) {
        return Some(key);
    }
    let passphrase = std::env::var(PASSPHRASE_ENV).ok()?;
    vault::unlock(&config.destination, &passphrase)
        .ok()
        .map(|(_, key)| key)
}

pub fn run(command: Command, paths: &AppPaths, loaded: Loaded) -> ExitCode {
    // The command line always speaks English; the log file does as well.
    let lang = Lang::En;
    let mut config = loaded.config;
    let computer = platform::computer_name();
    let cancel = CancelToken::default();
    let mut quiet = |_: &crate::engine::Progress| {};

    let fail = |err: &dyn std::fmt::Display| {
        eprintln!("error: {err}");
        tracing::error!("{err}");
        ExitCode::FAILURE
    };

    match command {
        Command::Paths => {
            println!("configuration: {}", paths.config_file.display());
            println!("logs:          {}", paths.log_dir.display());
            println!("portable:      {}", paths.portable);
            ExitCode::SUCCESS
        }

        Command::Snapshots => {
            let key = unattended_key(paths, &config);
            match snapshots::list(&config.destination, key.as_ref()) {
                Ok(list) if list.is_empty() => {
                    println!("No backups found in {}", config.destination.display());
                    ExitCode::SUCCESS
                }
                Ok(list) => {
                    for s in list {
                        let (status, files, size) = match &s.header {
                            Some(h) => (
                                lang.status(Some(h.status)).to_string(),
                                h.stats.files,
                                lang.bytes(h.stats.bytes),
                            ),
                            None if s.is_locked() => {
                                ("encrypted (locked)".to_string(), 0, String::new())
                            }
                            None => (lang.status(None).to_string(), 0, String::new()),
                        };
                        println!(
                            "{:<44} {:<22} {:>10} files {:>10}",
                            s.qualified_id(),
                            status,
                            files,
                            size
                        );
                    }
                    ExitCode::SUCCESS
                }
                Err(err) => fail(&err),
            }
        }

        Command::Backup {
            dry_run,
            full,
            csv,
            scheduled,
        } => {
            if scheduled {
                platform::enter_background_mode();
            }
            if full {
                config.mode = BackupMode::Full;
            }
            let config_dir = paths
                .config_file
                .parent()
                .map(PathBuf::from)
                .unwrap_or_default();
            let catalog = Catalog::load(&config_dir);
            let sources = Sources::collect(
                &config,
                &catalog,
                &KnownPaths::current(),
                Lang::resolve(config.language),
            );
            let key = config
                .encryption
                .enabled
                .then(|| unattended_key(paths, &config))
                .flatten();

            let record = |outcome: AutomaticOutcome, message: String, files: u64, bytes: u64| {
                if scheduled {
                    let mut state = State::load(&paths.config_file);
                    state.last_automatic = Some(AutomaticRun {
                        at: Utc::now(),
                        outcome,
                        message,
                        files,
                        bytes,
                    });
                    state.save(&paths.config_file);
                }
            };

            let input = BackupInput {
                config: &config,
                sources: &sources,
                computer: &computer,
                key: key.as_ref(),
                running_apps: Vec::new(),
            };
            let plan = match plan::plan_backup(&input, &cancel, &mut quiet) {
                Ok(plan) => plan,
                Err(err) => {
                    let outcome = match err {
                        EngineError::DestinationUnavailable(_) => {
                            AutomaticOutcome::DestinationUnavailable
                        }
                        EngineError::Locked | EngineError::EncryptionNotSetUp => {
                            AutomaticOutcome::NeedsPassphrase
                        }
                        _ => AutomaticOutcome::Failed,
                    };
                    record(outcome, err.to_string(), 0, 0);
                    // An unplugged external drive is expected, not an error.
                    if scheduled && outcome == AutomaticOutcome::DestinationUnavailable {
                        tracing::info!("automatic backup skipped: {err}");
                        return ExitCode::SUCCESS;
                    }
                    return fail(&err);
                }
            };
            print_summary(&plan, lang, false);
            if let Some(path) = csv
                && let Err(err) = export::write_csv(&plan, &labels(lang, false), &path)
            {
                return fail(&err);
            }
            if dry_run {
                println!("Dry run: nothing was changed.");
                return ExitCode::SUCCESS;
            }
            match backup::run_backup(&plan, key.as_ref(), &LiveFiles, &cancel, &mut quiet) {
                Ok(report) => {
                    let stats = &report.header.stats;
                    println!(
                        "{} -> {}",
                        lang.backup_result(
                            stats.copied_files,
                            stats.linked_files + stats.referenced_files,
                            stats.bytes,
                            report.duration,
                        ),
                        report.snapshot_dir.display()
                    );
                    for warning in &report.warnings {
                        println!("  note: {warning}");
                    }
                    let complete = report.header.status == SnapshotStatus::Complete;
                    record(
                        if complete {
                            AutomaticOutcome::Complete
                        } else {
                            AutomaticOutcome::CompleteWithNotes
                        },
                        report.warnings.first().cloned().unwrap_or_default(),
                        stats.files,
                        stats.bytes,
                    );
                    if complete {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::from(2)
                    }
                }
                Err(err) => {
                    let outcome = if matches!(err, EngineError::AlreadyRunning) {
                        AutomaticOutcome::AlreadyRunning
                    } else {
                        AutomaticOutcome::Failed
                    };
                    record(outcome, err.to_string(), 0, 0);
                    fail(&err)
                }
            }
        }

        Command::Restore {
            snapshot,
            to,
            dry_run,
            yes,
            conflict,
        } => {
            let key = unattended_key(paths, &config);
            let info =
                match snapshots::find(&config.destination, &snapshot, &computer, key.as_ref()) {
                    Ok(info) => info,
                    Err(err) => return fail(&err),
                };
            let options = RestoreOptions {
                target: to
                    .map(RestoreTarget::Folder)
                    .unwrap_or(RestoreTarget::Original),
                conflict: conflict.into(),
                verify: config.advanced.verify_on_restore,
                skip: HashSet::new(),
            };
            let plan = match plan::plan_restore(
                &info,
                options,
                key.as_ref(),
                Vec::new(),
                &cancel,
                &mut quiet,
            ) {
                Ok(plan) => plan,
                Err(err) => return fail(&err),
            };
            println!("Backup {}", info.qualified_id());
            print_summary(&plan, lang, true);
            if dry_run {
                println!("Dry run: nothing was changed.");
                return ExitCode::SUCCESS;
            }
            if !yes {
                println!("Add --yes to restore these files.");
                return ExitCode::from(2);
            }
            match restore::run_restore(&plan, key.as_ref(), &cancel, &mut quiet) {
                Ok(report) => {
                    println!(
                        "{}",
                        lang.restore_result(
                            report.restored_files,
                            report.restored_bytes,
                            report.skipped,
                            report.duration
                        )
                    );
                    for warning in &report.warnings {
                        println!("  note: {warning}");
                    }
                    if report.failed == 0 {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::from(2)
                    }
                }
                Err(err) => fail(&err),
            }
        }
    }
}

fn print_summary(plan: &dyn Plan, lang: Lang, restore: bool) {
    let summary = plan.summary();
    for kind in ItemKind::ALL {
        let count = summary.count(kind);
        if count > 0 {
            println!(
                "  {:<20} {:>12}  {:>10}",
                lang.kind_label(kind, restore),
                lang.count(count),
                lang.bytes(summary.bytes(kind))
            );
        }
    }
}

fn labels(lang: Lang, restore: bool) -> ExportLabels<'static> {
    let kind: &'static dyn Fn(ItemKind) -> String = if restore {
        &|k| Lang::En.kind_label(k, true).to_string()
    } else {
        &|k| Lang::En.kind_label(k, false).to_string()
    };
    ExportLabels {
        title: "AeternaVault preview",
        kind,
        note: &|item| {
            item.note
                .as_ref()
                .map(|n| Lang::En.note(n))
                .unwrap_or_default()
        },
        columns: lang.t().csv_columns,
    }
}
