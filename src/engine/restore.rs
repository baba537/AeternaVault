//! Executes a confirmed [`RestorePlan`].
//!
//! Only files marked `New` or `Changed` in the plan are written. Nothing is
//! ever deleted at the target.

use std::time::{Duration, Instant};

use super::fsops::{self, CopyError};
use super::plan::{ItemKind, RestorePlan};
use super::{CancelToken, Phase, Progress, Reporter, safe_relative_path};
use crate::config::ConflictPolicy;
use crate::error::EngineResult;

#[derive(Debug, Clone, Default)]
pub struct RestoreReport {
    pub restored_files: u64,
    pub restored_bytes: u64,
    /// Files left untouched: already identical, or skipped by the conflict rule.
    pub skipped: u64,
    pub failed: u64,
    pub cancelled: bool,
    pub warnings: Vec<String>,
    pub duration: Duration,
}

pub fn run_restore(
    plan: &RestorePlan,
    cancel: &CancelToken,
    on_progress: &mut dyn FnMut(&Progress),
) -> EngineResult<RestoreReport> {
    let started = Instant::now();
    let (files_total, bytes_total) = plan.write_totals();
    let mut report = RestoreReport {
        skipped: plan.summary.count(ItemKind::Skipped) + plan.summary.count(ItemKind::Unchanged),
        ..RestoreReport::default()
    };
    let mut reporter = Reporter::new(on_progress);
    let mut progress = Progress {
        phase: Phase::Restoring,
        files_total,
        bytes_total,
        ..Progress::default()
    };
    reporter.now(&progress);
    tracing::info!(
        snapshot = %plan.snapshot.qualified_id(),
        files = files_total,
        bytes = bytes_total,
        "restore started"
    );

    for item in plan
        .items
        .iter()
        .filter(|i| matches!(i.kind, ItemKind::New | ItemKind::Changed))
    {
        if cancel.is_cancelled() {
            report.cancelled = true;
            break;
        }
        let (Some(rel), Some(blob)) = (safe_relative_path(&item.rel), item.blob.as_ref()) else {
            continue;
        };
        let Some(blob_rel) = safe_relative_path(&blob.blob) else {
            continue;
        };
        let target = plan.sources[item.source].root.join(rel);
        let blob_path = plan.snapshot.computer_dir.join(blob_rel);
        progress.current = target.display().to_string();

        // The target may have appeared since the preview; respect "keep existing".
        if item.kind == ItemKind::New
            && plan.options.conflict == ConflictPolicy::KeepExisting
            && target.exists()
        {
            report.skipped += 1;
            progress.files_done += 1;
            continue;
        }

        let expected = plan.options.verify.then_some(blob.sha256.as_str());
        let result = fsops::copy_hashed(&blob_path, &target, expected, cancel, &mut |n| {
            progress.bytes_done += n;
            reporter.maybe(&progress);
        });

        match result {
            Ok(_) => {
                if let Err(err) = fsops::set_modified(&target, item.modified) {
                    tracing::debug!(
                        "could not set modification time on {}: {err}",
                        target.display()
                    );
                }
                report.restored_files += 1;
                report.restored_bytes += item.size;
            }
            Err(CopyError::Cancelled) => {
                report.cancelled = true;
                break;
            }
            Err(err) => {
                tracing::warn!("could not restore {}: {err}", target.display());
                report.warnings.push(format!("{}: {err}", target.display()));
                report.failed += 1;
            }
        }
        progress.files_done += 1;
        reporter.maybe(&progress);
    }

    progress.phase = Phase::Finishing;
    reporter.now(&progress);
    report.duration = started.elapsed();
    tracing::info!(
        restored = report.restored_files,
        failed = report.failed,
        cancelled = report.cancelled,
        "restore finished"
    );
    Ok(report)
}
