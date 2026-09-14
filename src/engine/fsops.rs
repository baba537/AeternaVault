//! All writing file-system operations of the engine live here.
//!
//! Files are always written to `<name>.aeterna-partial` first and renamed
//! into place once complete, so an interrupted copy never leaves a truncated
//! file under the real name.

use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::{CancelToken, format_sha256, from_unix_nanos};

const BUFFER_SIZE: usize = 1024 * 1024;

#[derive(Debug)]
pub enum CopyError {
    Io(io::Error),
    ChecksumMismatch { expected: String, actual: String },
    Cancelled,
}

impl std::fmt::Display for CopyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CopyError::Io(e) => write!(f, "{e}"),
            CopyError::ChecksumMismatch { expected, actual } => {
                write!(f, "checksum mismatch (expected {expected}, got {actual})")
            }
            CopyError::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl From<io::Error> for CopyError {
    fn from(e: io::Error) -> Self {
        CopyError::Io(e)
    }
}

fn partial_path(dst: &Path) -> PathBuf {
    let mut name = dst
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".aeterna-partial");
    dst.with_file_name(name)
}

/// Copy `src` to `dst` while computing SHA-256. If `expected` is given, the
/// result only replaces `dst` when the checksum matches.
///
/// A streaming copy is used instead of `std::fs::copy` (CopyFileEx) because
/// the checksum is computed in the same pass — every byte is read only once.
pub fn copy_hashed(
    src: &Path,
    dst: &Path,
    expected: Option<&str>,
    cancel: &CancelToken,
    on_bytes: &mut dyn FnMut(u64),
) -> Result<String, CopyError> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    let partial = partial_path(dst);
    let result = copy_into(src, &partial, cancel, on_bytes);
    let hash = match result {
        Ok(hash) => hash,
        Err(err) => {
            let _ = fs::remove_file(&partial);
            return Err(err);
        }
    };

    if let Some(expected) = expected
        && !expected.eq_ignore_ascii_case(&hash)
    {
        let _ = fs::remove_file(&partial);
        return Err(CopyError::ChecksumMismatch {
            expected: expected.to_string(),
            actual: hash,
        });
    }

    // `rename` replaces an existing file on Windows (MoveFileExW with REPLACE_EXISTING).
    if let Err(err) = fs::rename(&partial, dst) {
        let _ = fs::remove_file(&partial);
        return Err(err.into());
    }
    Ok(hash)
}

fn copy_into(
    src: &Path,
    partial: &Path,
    cancel: &CancelToken,
    on_bytes: &mut dyn FnMut(u64),
) -> Result<String, CopyError> {
    let mut input = File::open(src)?;
    let mut output = File::create(partial)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; BUFFER_SIZE];
    loop {
        if cancel.is_cancelled() {
            return Err(CopyError::Cancelled);
        }
        let n = match input.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        };
        hasher.update(&buffer[..n]);
        output.write_all(&buffer[..n])?;
        on_bytes(n as u64);
    }
    output.flush()?;
    Ok(format_sha256(&hasher.finalize()))
}

/// Restore the original modification time, so later comparisons (and the
/// user's view in Explorer) stay meaningful.
pub fn set_modified(path: &Path, modified: i64) -> io::Result<()> {
    let file = OpenOptions::new().write(true).open(path)?;
    file.set_times(FileTimes::new().set_modified(from_unix_nanos(modified)))
}

pub fn hard_link(existing: &Path, new: &Path) -> io::Result<()> {
    if let Some(parent) = new.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::hard_link(existing, new)
}

pub fn create_dir_all(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)
}

/// Create a new, not yet existing directory.
pub fn create_new_dir(path: &Path) -> io::Result<()> {
    fs::create_dir(path)
}

pub fn write_json_atomic<T: serde::Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let tmp = partial_path(path);
    {
        let file = File::create(&tmp)?;
        let mut writer = io::BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, value).map_err(io::Error::other)?;
        writer.flush()?;
        writer.get_ref().sync_all()?;
    }
    fs::rename(&tmp, path)
}
