//! Thin wrappers around a handful of Win32 calls.
//!
//! `windows-sys` is used instead of the higher-level `windows` crate because
//! only a few plain C functions are needed; it compiles faster and adds no
//! COM machinery.

use std::collections::HashSet;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE, LocalFree};
use windows_sys::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_HIDDEN, GetDiskFreeSpaceExW, GetDriveTypeW, GetFileAttributesW,
    GetLogicalDrives, INVALID_FILE_ATTRIBUTES, SetFileAttributesW,
};
use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, PROCESS_MODE_BACKGROUND_BEGIN, SetPriorityClass,
};
use windows_sys::Win32::System::WindowsProgramming::DRIVE_FIXED;

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

pub fn attach_parent_console() {
    // SAFETY: plain Win32 call without pointers; failure simply means there is
    // no parent console (e.g. started from Explorer or the Task Scheduler).
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

/// Marks a file or folder as hidden (used for AeternaVault's metadata folder).
pub fn set_hidden(path: &Path) -> io::Result<()> {
    let wide_path = wide(path);
    // SAFETY: valid NUL-terminated path.
    let attributes = unsafe { GetFileAttributesW(wide_path.as_ptr()) };
    if attributes == INVALID_FILE_ATTRIBUTES {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: valid NUL-terminated path; attributes come from the same file.
    let ok = unsafe { SetFileAttributesW(wide_path.as_ptr(), attributes | FILE_ATTRIBUTE_HIDDEN) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Lowers CPU and I/O priority of the whole process. Used for automatic
/// backups so they do not slow down the computer.
pub fn enter_background_mode() {
    // SAFETY: the current-process handle is always valid and needs no closing.
    unsafe {
        SetPriorityClass(GetCurrentProcess(), PROCESS_MODE_BACKGROUND_BEGIN);
    }
}

/// Lower-case executable names of all running processes (e.g. `firefox.exe`).
pub fn running_processes() -> HashSet<String> {
    let mut names = HashSet::new();
    // SAFETY: standard Toolhelp snapshot enumeration; the handle is closed below
    // and `entry.dwSize` is initialised as the API requires.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return names;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                names.insert(String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase());
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }
    names
}

/// Encrypts data for the current Windows user (DPAPI). Only the same user on
/// the same computer can decrypt it — used to remember the vault key for
/// automatic backups without storing the passphrase.
pub fn dpapi_protect(data: &[u8]) -> io::Result<Vec<u8>> {
    dpapi(data, true)
}

pub fn dpapi_unprotect(data: &[u8]) -> io::Result<Vec<u8>> {
    dpapi(data, false)
}

fn dpapi(data: &[u8], protect: bool) -> io::Result<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(data.len()).map_err(io::Error::other)?,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: `input` points to `data`, which outlives the call and is only read.
    // On success `output.pbData` is allocated by the system and freed with
    // LocalFree after copying.
    unsafe {
        let ok = if protect {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData as _);
        Ok(result)
    }
}
