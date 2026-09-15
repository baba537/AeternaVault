//! Automatic backups.
//!
//! AeternaVault runs its schedules itself instead of registering tasks in the
//! Windows Task Scheduler (as 0.2 did):
//!
//! * several schedules, each with its own folders, managed in one place;
//! * nothing is left behind in system tools, and nothing runs when the user
//!   has quit AeternaVault;
//! * an unlocked vault can be used even if its key is not remembered.
//!
//! When the window is closed while schedules are on, AeternaVault keeps
//! running in the notification area. "Start with Windows" adds an entry to
//! the user's autostart list (`--background`).
//!
//! * [`timing`] — when a schedule is due (pure functions)
//! * [`service`] — the background thread that runs due schedules
//! * [`run_unattended`] — one backup without any window, shared with the CLI

pub mod service;
pub mod timing;

use std::path::PathBuf;

use chrono::Utc;

use crate::config::{Config, Frequency, Schedule};
use crate::engine::backup::{self, BackupReport};
use crate::engine::crypto::VaultKey;
use crate::engine::manifest::SnapshotStatus;
use crate::engine::plan::{self, BackupInput};
use crate::engine::sources::Sources;
use crate::engine::{CancelToken, Progress, vault};
use crate::error::EngineError;
use crate::i18n::Lang;
use crate::paths::AppPaths;
use crate::platform::apps::Catalog;
use crate::platform::known_paths::KnownPaths;
use crate::platform::{self, vss::LiveFiles};
use crate::state::{self, AutomaticOutcome, AutomaticRun};

pub const PASSPHRASE_ENV: &str = "AETERNAVAULT_PASSPHRASE";

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

/// Plans and runs one backup without asking anything. `key` is an already
/// unlocked vault key (from the window); otherwise the remembered key is used.
pub fn run_unattended(
    paths: &AppPaths,
    config: &Config,
    key: Option<VaultKey>,
    label: &str,
    cancel: &CancelToken,
    on_progress: &mut dyn FnMut(&Progress),
) -> (AutomaticRun, Option<BackupReport>) {
    let config_dir = paths
        .config_file
        .parent()
        .map(PathBuf::from)
        .unwrap_or_default();
    let catalog = Catalog::load(&config_dir);
    let sources = Sources::collect(
        config,
        &catalog,
        &KnownPaths::current(),
        Lang::resolve(config.language),
    );
    let key = if config.encryption.enabled {
        key.or_else(|| unattended_key(paths, config))
    } else {
        None
    };
    let computer = platform::computer_name();
    let finished = |outcome, message: String, files, bytes| AutomaticRun {
        at: Utc::now(),
        outcome,
        message,
        files,
        bytes,
        schedule: label.to_string(),
    };

    let input = BackupInput {
        config,
        sources: &sources,
        computer: &computer,
        key: key.as_ref(),
        running_apps: Vec::new(),
    };
    let plan = match plan::plan_backup(&input, cancel, on_progress) {
        Ok(plan) => plan,
        Err(err) => {
            let outcome = outcome_for(&err);
            if outcome == AutomaticOutcome::DestinationUnavailable {
                // An unplugged external drive is expected, not an error.
                tracing::info!("automatic backup skipped: {err}");
            } else {
                tracing::warn!("automatic backup could not be planned: {err}");
            }
            return (finished(outcome, err.to_string(), 0, 0), None);
        }
    };

    match backup::run_backup(&plan, key.as_ref(), &LiveFiles, cancel, on_progress) {
        Ok(report) => {
            let outcome = if report.header.status == SnapshotStatus::Complete {
                AutomaticOutcome::Complete
            } else {
                AutomaticOutcome::CompleteWithNotes
            };
            let total = report.total_stats();
            let run = finished(
                outcome,
                report.warnings.first().cloned().unwrap_or_default(),
                total.files,
                total.bytes,
            );
            match apply_retention(
                &config.destination,
                &config.retention,
                &computer,
                key.as_ref(),
                cancel,
            ) {
                Some(Err(err)) => tracing::warn!("old backups could not be removed: {err}"),
                Some(Ok(removed)) => {
                    tracing::info!("{} old backups removed", removed.deleted.len())
                }
                None => {}
            }
            (run, Some(report))
        }
        Err(err) => {
            tracing::error!("automatic backup failed: {err}");
            (finished(outcome_for(&err), err.to_string(), 0, 0), None)
        }
    }
}

/// Removes old backups of this computer by the retention rules. `None` if the
/// rules are off or nothing is to be removed. Encrypted backups are only
/// removed with a key.
pub fn apply_retention(
    destination: &std::path::Path,
    policy: &crate::config::Retention,
    computer: &str,
    key: Option<&VaultKey>,
    cancel: &CancelToken,
) -> Option<Result<crate::engine::manage::DeleteReport, EngineError>> {
    if !policy.enabled {
        return None;
    }
    let all = crate::engine::snapshots::list(destination, key).ok()?;
    let ids: Vec<String> = crate::engine::retention::to_remove(&all, policy, computer)
        .into_iter()
        .filter(|id| {
            key.is_some()
                || all
                    .iter()
                    .find(|s| &s.qualified_id() == id)
                    .is_some_and(|s| !s.needs_key())
        })
        .collect();
    if ids.is_empty() {
        return None;
    }
    tracing::info!("removing {} old backups by the retention rules", ids.len());
    Some(crate::engine::manage::delete(
        destination,
        &ids,
        key,
        cancel,
        &mut |_| {},
    ))
}

fn outcome_for(err: &EngineError) -> AutomaticOutcome {
    match err {
        EngineError::DestinationUnavailable(_) => AutomaticOutcome::DestinationUnavailable,
        EngineError::Locked | EngineError::EncryptionNotSetUp => AutomaticOutcome::NeedsPassphrase,
        EngineError::AlreadyRunning => AutomaticOutcome::AlreadyRunning,
        _ => AutomaticOutcome::Failed,
    }
}

/// A short description such as "Every day at 20:00 · Documents, Pictures".
pub fn describe(schedule: &Schedule, config: &Config, lang: Lang) -> String {
    if !schedule.name.trim().is_empty() {
        return schedule.name.trim().to_string();
    }
    format!(
        "{} · {}",
        when_text(schedule, lang),
        scope_text(schedule, config, lang)
    )
}

pub fn when_text(schedule: &Schedule, lang: Lang) -> String {
    let t = lang.t();
    let time = schedule.time.trim();
    match schedule.frequency {
        Frequency::Daily => format!("{} {} {time}", t.freq_daily, t.at_time),
        Frequency::Weekly => format!(
            "{} {} {time}",
            t.weekdays_every[schedule.weekday as usize], t.at_time
        ),
        Frequency::Hourly => lang.every_hours_capital(schedule.every_hours),
        Frequency::AtStart => t.freq_at_start.to_string(),
    }
}

pub fn scope_text(schedule: &Schedule, config: &Config, lang: Lang) -> String {
    let t = lang.t();
    if schedule.is_everything() {
        return t.scope_everything.to_string();
    }
    let mut parts: Vec<String> = config
        .sources
        .iter()
        .filter(|s| {
            schedule.all_folders
                || schedule
                    .folders
                    .iter()
                    .any(|f| crate::engine::paths_equal(f, &s.path))
        })
        .map(|s| s.display_name())
        .collect();
    if schedule.all_folders {
        parts = vec![t.scope_all_folders.to_string()];
    }
    if schedule.applications {
        parts.push(t.apps_card_title.to_string());
    }
    if parts.is_empty() {
        t.scope_nothing.to_string()
    } else {
        lang.list_names(&parts)
    }
}
