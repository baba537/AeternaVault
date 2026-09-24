//! The backup engine. It knows nothing about the GUI.
//!
//! Every operation runs in two strictly separated steps:
//!
//! 1. **Plan** ([`plan`], [`scan`], [`sources`], [`selection`], [`snapshots`],
//!    [`manifest`]): read-only. Walks the sources or a snapshot and produces a
//!    [`plan::BackupPlan`] or [`plan::RestorePlan`]. These modules never call a
//!    writing function; a unit test below enforces that.
//! 2. **Execute** ([`backup`], [`restore`]): performs a plan. File contents
//!    are written through `fsops`.
//!
//! Managing backups: [`verify`] (read-only), [`retention`] (read-only choice
//! of old backups), [`manage`] (delete, move, clean up) and [`layout`]
//! (moving backups of older versions to the current folder layout).
//!
//! Encryption lives in [`crypto`] (primitives) and [`vault`] (on-disk vault).

pub mod backup;
pub mod crypto;
mod fsops;
pub mod layout;
pub mod manage;
pub mod manifest;
pub mod passphrase;
pub mod plan;
pub mod restore;
pub mod retention;
pub mod scan;
pub mod selection;
pub mod snapshots;
pub mod sources;
#[cfg(test)]
mod tests;
pub mod vault;
pub mod verify;

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Shared cancellation flag, checked between files and between copy chunks.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Scanning,
    Copying,
    Restoring,
    Finishing,
}

#[derive(Debug, Clone, Default)]
pub struct Progress {
    pub phase: Phase,
    pub current: String,
    pub files_done: u64,
    pub files_total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

impl Progress {
    /// Share of the work done, if the total is known.
    pub fn fraction(&self) -> Option<f32> {
        if self.bytes_total > 0 {
            Some((self.bytes_done as f64 / self.bytes_total as f64).clamp(0.0, 1.0) as f32)
        } else if self.files_total > 0 {
            Some((self.files_done as f64 / self.files_total as f64).clamp(0.0, 1.0) as f32)
        } else {
            None
        }
    }
}

/// Rate-limits progress callbacks so millions of small files do not flood the UI.
pub struct Reporter<'a> {
    sink: &'a mut dyn FnMut(&Progress),
    last: Option<Instant>,
}

impl<'a> Reporter<'a> {
    const INTERVAL: Duration = Duration::from_millis(60);

    pub fn new(sink: &'a mut dyn FnMut(&Progress)) -> Self {
        Self { sink, last: None }
    }

    pub fn maybe(&mut self, progress: &Progress) {
        if self.last.is_none_or(|t| t.elapsed() >= Self::INTERVAL) {
            self.now(progress);
        }
    }

    pub fn now(&mut self, progress: &Progress) {
        self.last = Some(Instant::now());
        (self.sink)(progress);
    }
}

/// Compares paths component by component; case-insensitive on Windows.
pub fn paths_equal(a: &Path, b: &Path) -> bool {
    normalized_components(a) == normalized_components(b)
}

/// `true` if `path` equals `base` or lies inside it (case-insensitive on Windows).
pub fn path_is_within(path: &Path, base: &Path) -> bool {
    let p = normalized_components(path);
    let b = normalized_components(base);
    !b.is_empty() && p.len() >= b.len() && p[..b.len()] == b[..]
}

fn normalized_components(path: &Path) -> Vec<String> {
    path.components()
        .filter(|c| !matches!(c, Component::CurDir))
        .map(|c| {
            let text = c.as_os_str().to_string_lossy();
            if cfg!(windows) {
                text.to_lowercase()
            } else {
                text.into_owned()
            }
        })
        .collect()
}

/// Stored timestamps are nanoseconds relative to the Unix epoch (may be negative).
pub fn to_unix_nanos(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_nanos()).unwrap_or(i64::MAX),
        Err(e) => -i64::try_from(e.duration().as_nanos()).unwrap_or(i64::MAX),
    }
}

pub fn from_unix_nanos(nanos: i64) -> SystemTime {
    if nanos >= 0 {
        UNIX_EPOCH + Duration::from_nanos(nanos as u64)
    } else {
        UNIX_EPOCH - Duration::from_nanos(nanos.unsigned_abs())
    }
}

/// Relative paths are stored with `/` so the index is portable and readable.
pub fn rel_to_string(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Turn a stored relative path back into a path, rejecting anything that could
/// escape the target folder (`..`, drive letters, absolute paths). A modified
/// or damaged index must never make a restore write outside its target.
pub fn safe_relative_path(stored: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for part in stored.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        // A drive letter is rejected on every system, so an index is judged
        // the same way on Windows and Linux.
        let bytes = part.as_bytes();
        if out.as_os_str().is_empty()
            && bytes.len() == 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
        {
            return None;
        }
        let candidate = Path::new(part);
        let mut components = candidate.components();
        match (components.next(), components.next()) {
            (Some(Component::Normal(name)), None) => out.push(name),
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

pub fn format_sha256(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn relative_paths_cannot_escape() {
        assert_eq!(
            safe_relative_path("a/b.txt"),
            Some(PathBuf::from("a").join("b.txt"))
        );
        assert_eq!(safe_relative_path("../evil.txt"), None);
        assert_eq!(safe_relative_path("a/../../evil.txt"), None);
        assert_eq!(safe_relative_path("C:/Windows/x"), None);
        assert_eq!(safe_relative_path(""), None);
    }

    #[cfg(windows)]
    #[test]
    fn case_insensitive_paths() {
        assert!(paths_equal(
            Path::new(r"C:\Users\A"),
            Path::new(r"c:\users\a")
        ));
        assert!(path_is_within(
            Path::new(r"D:\Backups\x"),
            Path::new(r"d:\backups")
        ));
        assert!(!path_is_within(
            Path::new(r"D:\BackupsOld"),
            Path::new(r"D:\Backups")
        ));
    }

    #[test]
    fn nanos_roundtrip() {
        let now = SystemTime::now();
        assert_eq!(from_unix_nanos(to_unix_nanos(now)), now);
    }

    /// Planning must be strictly read-only. The planning modules may not
    /// even mention writing file-system APIs; all writes live in `fsops`,
    /// `backup` and `restore`.
    #[test]
    fn planning_modules_are_read_only() {
        let sources = [
            ("plan.rs", include_str!("plan.rs")),
            ("scan.rs", include_str!("scan.rs")),
            ("snapshots.rs", include_str!("snapshots.rs")),
            ("manifest.rs", include_str!("manifest.rs")),
            ("sources.rs", include_str!("sources.rs")),
            ("selection.rs", include_str!("selection.rs")),
            ("verify.rs", include_str!("verify.rs")),
            ("retention.rs", include_str!("retention.rs")),
        ];
        let forbidden = [
            "fs::write",
            "fs::copy",
            "fs::rename",
            "fs::remove",
            "create_dir",
            "OpenOptions",
            "File::create",
            "hard_link",
            "set_permissions",
            "set_times",
            "set_modified",
            "fsops",
            "Command::new",
            "remember_key",
            "vault::create",
        ];
        for (file, source) in sources {
            for word in forbidden {
                assert!(
                    !source.contains(word),
                    "{file} must stay read-only but mentions `{word}`"
                );
            }
        }
    }
}
