//! Command-line interface for unattended use, e.g. from the Task Scheduler:
//!
//! ```text
//! AeternaVault.exe backup                 back up with the saved settings
//! AeternaVault.exe backup --dry-run       only show what would happen
//! AeternaVault.exe snapshots              list backups
//! AeternaVault.exe restore latest --to D:\Restored --dry-run
//! ```
//!
//! Exit codes: 0 = success, 1 = error, 2 = completed with notes or
//! confirmation (`--yes`) missing.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

use crate::config::{BackupMode, ConflictPolicy, Loaded};
use crate::engine::export::{self, ExportLabels};
use crate::engine::manifest::SnapshotStatus;
use crate::engine::plan::{self, ItemKind, Plan, RestoreOptions, RestoreTarget};
use crate::engine::{CancelToken, backup, restore, snapshots};
use crate::i18n::Lang;
use crate::paths::AppPaths;
use crate::platform::{self, vss::LiveFiles};

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
    /// Back up all enabled sources using the saved configuration.
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
    },
    /// List the backups found at the destination.
    Snapshots,
    /// Restore a backup: "latest", an ID such as 2026-09-14_143205, or COMPUTER/ID.
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

        Command::Snapshots => match snapshots::list(&config.destination) {
            Ok(list) if list.is_empty() => {
                println!("No backups found in {}", config.destination.display());
                ExitCode::SUCCESS
            }
            Ok(list) => {
                for s in list {
                    let (status, files, size) = match &s.header {
                        Some(h) => (
                            lang.status(Some(h.status)),
                            h.stats.files,
                            lang.bytes(h.stats.bytes),
                        ),
                        None => (lang.status(None), 0, String::new()),
                    };
                    println!(
                        "{:<40} {:<22} {:>10} files {:>10}",
                        s.qualified_id(),
                        status,
                        files,
                        size
                    );
                }
                ExitCode::SUCCESS
            }
            Err(err) => fail(&err),
        },

        Command::Backup { dry_run, full, csv } => {
            if full {
                config.mode = BackupMode::Full;
            }
            let plan = match plan::plan_backup(&config, &computer, &cancel, &mut quiet) {
                Ok(plan) => plan,
                Err(err) => return fail(&err),
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
            match backup::run_backup(&plan, &LiveFiles, &cancel, &mut quiet) {
                Ok(report) => {
                    println!(
                        "{} -> {}",
                        lang.backup_result(
                            report.header.stats.copied_files,
                            report.header.stats.linked_files + report.header.stats.referenced_files,
                            report.header.stats.bytes,
                            report.duration,
                        ),
                        report.snapshot_dir.display()
                    );
                    for warning in &report.warnings {
                        println!("  note: {warning}");
                    }
                    if report.header.status == SnapshotStatus::Complete {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::from(2)
                    }
                }
                Err(err) => fail(&err),
            }
        }

        Command::Restore {
            snapshot,
            to,
            dry_run,
            yes,
            conflict,
        } => {
            let info = match snapshots::find(&config.destination, &snapshot, &computer) {
                Ok(info) => info,
                Err(err) => return fail(&err),
            };
            let options = RestoreOptions {
                target: to
                    .map(RestoreTarget::Folder)
                    .unwrap_or(RestoreTarget::Original),
                conflict: conflict.into(),
                verify: config.advanced.verify_on_restore,
            };
            let plan = match plan::plan_restore(&info, options, &cancel, &mut quiet) {
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
            match restore::run_restore(&plan, &cancel, &mut quiet) {
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
