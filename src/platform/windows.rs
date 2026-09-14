//! Thin wrappers around a handful of Win32 calls.
//!
//! `windows-sys` is used instead of the higher-level `windows` crate because
//! only a few plain C functions are needed; it compiles faster and adds no
//! COM machinery.

use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives,
};
use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
use windows_sys::Win32::System::WindowsProgramming::DRIVE_FIXED;

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

pub fn attach_parent_console() {
    // SAFETY: plain Win32 call without pointers; failure simply means there is
    // no parent console (e.g. started from Explorer).
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

pub fn first_secondary_fixed_drive() -> Option<PathBuf> {
    let system = std::env::var("SystemDrive")
        .unwrap_or_else(|_| "C:".to_string())
        .to_ascii_uppercase();

    // SAFETY: no arguments; returns a bit mask of available drive letters.
    let mask = unsafe { GetLogicalDrives() };
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        if format!("{letter}:") == system || letter < 'C' {
            continue;
        }
        let root = PathBuf::from(format!("{letter}:\\"));
        let wide_root = wide(&root);
        // SAFETY: `wide_root` is a valid, NUL-terminated UTF-16 string.
        let kind = unsafe { GetDriveTypeW(wide_root.as_ptr()) };
        if kind == DRIVE_FIXED {
            return Some(root);
        }
    }
    None
}

pub fn free_space(path: &Path) -> Option<u64> {
    // Walk up to an existing ancestor: the destination folder may not exist yet.
    let mut probe = path;
    while !probe.exists() {
        probe = probe.parent()?;
    }
    let wide_path = wide(probe);
    let mut available: u64 = 0;
    // SAFETY: valid NUL-terminated path; `available` outlives the call; the
    // two optional out-parameters are null.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide_path.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(available)
}
