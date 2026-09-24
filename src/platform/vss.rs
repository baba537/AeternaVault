//! Consistent reading of files that are in use (Volume Shadow Copy Service).
//!
//! Status: interface only. The backup engine already reads every file through
//! a [`FileReader`], so a VSS implementation can be added without touching
//! the engine.
//!
//! A future `ShadowCopyReader` would:
//! 1. Require administrator rights (VSS requestors need `SeBackupPrivilege`).
//! 2. Use `IVssBackupComponents` via the `windows` crate
//!    (`CreateVssBackupComponents`, `StartSnapshotSet`, `AddToSnapshotSet`,
//!    `PrepareForBackup`, `DoSnapshotSet`).
//! 3. Map `C:\Users\...` to `\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopyN\Users\...`.
//! 4. Release the snapshot set when dropped.
//!
//! Alternatives: shelling out to `vssadmin`/`wmic` (fragile, deprecated) or
//! `diskshadow` (Server editions only).

use std::path::{Path, PathBuf};

/// Translates an original path into the path that should actually be read.
pub trait FileReader: Send + Sync {
    fn read_path(&self, original: &Path) -> PathBuf;

    /// Human-readable description for the log.
    fn describe(&self) -> &'static str;
}

/// Reads files directly from their live location. Files locked exclusively by
/// another program are reported as warnings and skipped.
pub struct LiveFiles;

impl FileReader for LiveFiles {
    fn read_path(&self, original: &Path) -> PathBuf {
        original.to_path_buf()
    }

    fn describe(&self) -> &'static str {
        "live files (no shadow copy)"
    }
}
