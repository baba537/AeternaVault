//! Which old backups a retention policy would remove. Read-only; the removal
//! itself happens in [`super::manage`].
//!
//! Similar to common backup tools: keep the newest *n* backups, plus the
//! newest backup of each of the last days, weeks and months. Only backups of
//! the given computer are considered, and only those whose details are known —
//! a locked encrypted backup is never removed.

use std::collections::HashSet;

use chrono::{Datelike, Local};

use super::snapshots::SnapshotInfo;
use crate::config::Retention;

/// Qualified ids of the backups the policy does not keep, newest first.
pub fn to_remove(snapshots: &[SnapshotInfo], policy: &Retention, computer: &str) -> Vec<String> {
    let mut own: Vec<&SnapshotInfo> = snapshots
        .iter()
        .filter(|s| s.computer.eq_ignore_ascii_case(computer) && s.header.is_some())
        .collect();
    own.sort_by_key(|s| std::cmp::Reverse(s.header.as_ref().map(|h| h.started_at)));

    let usable: Vec<&SnapshotInfo> = own
        .iter()
        .copied()
        .filter(|s| s.is_usable_base() && !s.needs_unlock())
        .collect();
    let Some(newest) = usable.first() else {
        // Without a complete backup, nothing is removed.
        return Vec::new();
    };
    let newest_start = newest.header.as_ref().map(|h| h.started_at);

    let mut keep: HashSet<String> = HashSet::new();
    keep.insert(newest.qualified_id());
    for s in usable.iter().take(policy.keep_last as usize) {
        keep.insert(s.qualified_id());
    }
    let local = |s: &SnapshotInfo| {
        s.header
            .as_ref()
            .map(|h| h.started_at.with_timezone(&Local))
    };
    let mut keep_per = |limit: u32, bucket: &dyn Fn(chrono::DateTime<Local>) -> (i32, u32)| {
        let mut seen = HashSet::new();
        for s in &usable {
            if seen.len() >= limit as usize {
                break;
            }
            if let Some(time) = local(s)
                && seen.insert(bucket(time))
            {
                keep.insert(s.qualified_id());
            }
        }
    };
    keep_per(policy.keep_daily, &|t| (t.year(), t.ordinal()));
    keep_per(policy.keep_weekly, &|t| {
        let week = t.iso_week();
        (week.year(), week.week())
    });
    keep_per(policy.keep_monthly, &|t| (t.year(), t.month()));

    own.into_iter()
        .filter(|s| !keep.contains(&s.qualified_id()))
        .filter(|s| {
            // Unfinished or locked backups newer than the newest complete one stay:
            // they may still be running or simply need the passphrase.
            let start = s.header.as_ref().map(|h| h.started_at);
            (s.is_usable_base() && !s.needs_unlock()) || start < newest_start
        })
        .filter(|s| !s.needs_unlock())
        .map(SnapshotInfo::qualified_id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::manifest::{SnapshotHeader, SnapshotStats, SnapshotStatus};
    use crate::engine::snapshots::Location;
    use chrono::{Duration, TimeZone, Utc};
    use std::path::PathBuf;

    fn snapshot(id: usize, days_ago: i64, status: SnapshotStatus) -> SnapshotInfo {
        let started =
            Utc.with_ymd_and_hms(2026, 9, 15, 12, 0, 0).unwrap() - Duration::days(days_ago);
        SnapshotInfo {
            id: format!("s{id:03}"),
            computer: "PC".into(),
            location: Location::Plain {
                dir: PathBuf::from(format!("s{id:03}")),
                destination: PathBuf::new(),
            },
            header: Some(SnapshotHeader {
                format: 2,
                app_version: String::new(),
                id: format!("s{id:03}"),
                computer: "PC".into(),
                user: String::new(),
                mode: crate::config::BackupMode::Incremental,
                status,
                started_at: started,
                finished_at: None,
                base: None,
                encrypted: false,
                split: false,
                sources: Vec::new(),
                registry: Vec::new(),
                known_paths: Default::default(),
                stats: SnapshotStats::default(),
            }),
            companion: None,
        }
    }

    #[test]
    fn keeps_last_daily_weekly_monthly() {
        // One backup per day for 400 days, plus a cancelled one yesterday.
        let mut all: Vec<SnapshotInfo> = (0..400)
            .map(|d| snapshot(d, d as i64, SnapshotStatus::Complete))
            .collect();
        all.push(snapshot(999, 1, SnapshotStatus::Cancelled));
        let policy = Retention {
            enabled: true,
            keep_last: 3,
            keep_daily: 7,
            keep_weekly: 4,
            keep_monthly: 12,
        };
        let removed = to_remove(&all, &policy, "PC");
        let kept = all.len() - removed.len();
        // 7 daily (covers the last 3) + up to 4 weekly + 12 monthly, with overlaps.
        assert!((15..=23).contains(&kept), "kept {kept}");
        assert!(!removed.contains(&"s000".to_string()), "the newest stays");
        assert!(
            removed.contains(&"s999".to_string()),
            "old cancelled backups go"
        );
        assert!(removed.contains(&"s399".to_string()));

        // Other computers are left alone.
        assert!(to_remove(&all, &policy, "OTHER").is_empty());
    }

    #[test]
    fn nothing_is_removed_without_a_complete_backup() {
        let all = vec![
            snapshot(1, 1, SnapshotStatus::Cancelled),
            snapshot(2, 2, SnapshotStatus::Failed),
        ];
        assert!(to_remove(&all, &Retention::default(), "PC").is_empty());
    }
}
