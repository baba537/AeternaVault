//! Only one AeternaVault window per configuration.
//!
//! A second start (for example from the Start menu while AeternaVault waits in
//! the notification area) brings the running instance to the front instead of
//! opening another window that would run the same schedules.

use std::path::Path;

use sha2::{Digest, Sha256};

/// A name that is the same for every start with the same configuration file,
/// so portable copies and test setups do not block each other.
pub fn key_for(config_file: &Path) -> String {
    let normalized = config_file.to_string_lossy().to_lowercase();
    let digest = Sha256::digest(normalized.as_bytes());
    format!(
        "AeternaVault-{}",
        crate::engine::format_sha256(&digest[..8])
    )
}

/// Held for the lifetime of the first instance.
pub struct InstanceGuard {
    #[cfg(windows)]
    _handle: isize,
}

/// `None` if another instance with the same key is already running.
pub fn claim(key: &str) -> Option<InstanceGuard> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
        use windows_sys::Win32::System::Threading::CreateMutexW;
        let name: Vec<u16> = format!("Local\\{key}")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: valid NUL-terminated name; the handle is intentionally kept
        // open until the process ends.
        unsafe {
            let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            if handle.is_null() {
                // Without a mutex there is no protection, but the app still works.
                return Some(InstanceGuard { _handle: 0 });
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                windows_sys::Win32::Foundation::CloseHandle(handle);
                return None;
            }
            Some(InstanceGuard {
                _handle: handle as isize,
            })
        }
    }
    #[cfg(not(windows))]
    {
        let _ = key;
        Some(InstanceGuard {})
    }
}

/// Asks the running instance to show its window. Returns `false` if it could
/// not be reached.
pub fn activate_existing(key: &str) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            ASFW_ANY, AllowSetForegroundWindow, FindWindowW, PostMessageW,
        };
        let class: Vec<u16> = super::tray::TRAY_CLASS
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let title: Vec<u16> = key.encode_utf16().chain(std::iter::once(0)).collect();
        // The running instance may still be starting up.
        for _ in 0..20 {
            // SAFETY: valid NUL-terminated strings; posting to a found window.
            unsafe {
                let hwnd = FindWindowW(class.as_ptr(), title.as_ptr());
                if !hwnd.is_null() {
                    // Lets the other process put its window in front of this one.
                    AllowSetForegroundWindow(ASFW_ANY);
                    return PostMessageW(hwnd, super::tray::WM_APP_ACTIVATE, 0, 0) != 0;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
        false
    }
    #[cfg(not(windows))]
    {
        let _ = key;
        false
    }
}
