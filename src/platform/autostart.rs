//! "Start with Windows": an entry in the current user's autostart list
//! (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`), which starts
//! AeternaVault quietly in the notification area. No administrator rights are
//! needed.
//!
//! The entry itself stays registered, so AeternaVault is always listed under
//! Task Manager → Startup apps and Settings → Apps → Startup. Switching it on or
//! off works exactly like the switch there: Windows keeps that choice in
//! `...\Explorer\StartupApproved\Run` as a 12-byte value whose first byte is
//! even (enabled, usually 02) or odd (disabled, usually 03, followed by the
//! time it was switched off).

// Windows only; the other platforms get the no-op fallbacks below.
#![cfg_attr(not(windows), allow(dead_code))]

use std::io;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const VALUE: &str = "AeternaVault";
pub const ARGUMENT: &str = "--background";

fn command_line() -> io::Result<String> {
    let exe = super::gui_executable()?;
    Ok(format!("\"{}\" {ARGUMENT}", exe.display()))
}

/// The value Windows writes for an enabled or disabled startup entry.
fn approval_value(enabled: bool) -> Vec<u8> {
    let mut value = vec![0u8; 12];
    if enabled {
        value[0] = 2;
    } else {
        value[0] = 3;
        let filetime = filetime_now();
        value[4..].copy_from_slice(&filetime.to_le_bytes());
    }
    value
}

/// 100-nanosecond intervals since 1601-01-01, like Windows' FILETIME.
fn filetime_now() -> u64 {
    const UNIX_TO_1601_SECS: u64 = 11_644_473_600;
    let since_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    (since_unix.as_secs() + UNIX_TO_1601_SECS) * 10_000_000
        + u64::from(since_unix.subsec_nanos() / 100)
}

/// `Some(true/false)` if Windows has a recorded choice, `None` otherwise
/// (which Windows treats as enabled).
fn approved_bytes(bytes: &[u8]) -> Option<bool> {
    bytes.first().map(|first| first & 1 == 0)
}

#[cfg(windows)]
mod registry {
    use super::*;
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, REG_BINARY};

    pub fn run_value() -> Option<String> {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(RUN_KEY)
            .and_then(|key| key.get_value::<String, _>(VALUE))
            .ok()
    }

    pub fn approved() -> Option<bool> {
        let raw = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(APPROVED_KEY)
            .and_then(|key| key.get_raw_value(VALUE))
            .ok()?;
        approved_bytes(&raw.bytes)
    }

    pub fn write_run(command: &str) -> io::Result<()> {
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY)?;
        key.set_value(VALUE, &command)
    }

    pub fn remove_values() -> io::Result<()> {
        use winreg::enums::KEY_SET_VALUE;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        for path in [RUN_KEY, APPROVED_KEY] {
            match hkcu.open_subkey_with_flags(path, KEY_SET_VALUE) {
                Ok(key) => match key.delete_value(VALUE) {
                    Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err),
                    _ => {}
                },
                Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err),
                Err(_) => {}
            }
        }
        Ok(())
    }

    pub fn write_approved(enabled: bool) -> io::Result<()> {
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(APPROVED_KEY)?;
        key.set_raw_value(
            VALUE,
            &winreg::RegValue {
                bytes: approval_value(enabled).into(),
                vtype: REG_BINARY,
            },
        )
    }
}

/// Whether AeternaVault starts with Windows: the entry points to this
/// executable and is not switched off.
pub fn is_enabled() -> bool {
    #[cfg(windows)]
    {
        let Ok(expected) = command_line() else {
            return false;
        };
        registry::run_value().is_some_and(|v| v.trim().eq_ignore_ascii_case(&expected))
            && registry::approved() != Some(false)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Makes sure the startup entry exists and points to this executable. A new
/// entry starts switched off; an existing one keeps the user's choice.
pub fn ensure_registered() -> io::Result<()> {
    if !super::system_changes_allowed() {
        return Ok(());
    }
    #[cfg(windows)]
    {
        let command = command_line()?;
        let existing = registry::run_value();
        if existing.as_deref().map(str::trim) == Some(command.as_str()) {
            return Ok(());
        }
        if existing.is_none() && registry::approved().is_none() {
            registry::write_approved(false)?;
        }
        registry::write_run(&command)?;
        tracing::info!("startup entry registered (switched on: {})", is_enabled());
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}

/// Removes the startup entry completely (used when uninstalling).
pub fn unregister() -> io::Result<()> {
    if !super::system_changes_allowed() {
        return Ok(());
    }
    #[cfg(windows)]
    {
        registry::remove_values()
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}

pub fn set_enabled(enabled: bool) -> io::Result<()> {
    if !super::system_changes_allowed() {
        return Err(io::Error::other(
            "not changed in demo mode (AETERNAVAULT_PROFILE_ROOT is set)",
        ));
    }
    #[cfg(windows)]
    {
        let command = command_line()?;
        if registry::run_value().as_deref().map(str::trim) != Some(command.as_str()) {
            registry::write_run(&command)?;
        }
        registry::write_approved(enabled)?;
        if enabled {
            tracing::info!("AeternaVault starts with Windows");
        } else {
            tracing::info!("AeternaVault no longer starts with Windows (entry kept, switched off)");
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err(io::Error::other("not supported on this platform"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_values_match_windows() {
        let on = approval_value(true);
        assert_eq!(on, [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(approved_bytes(&on), Some(true));
        let off = approval_value(false);
        assert_eq!(off.len(), 12);
        assert_eq!(off[0], 3);
        assert_eq!(approved_bytes(&off), Some(false));
        // Windows also uses 06/07 on some systems.
        assert_eq!(approved_bytes(&[6]), Some(true));
        assert_eq!(approved_bytes(&[7]), Some(false));
        assert_eq!(approved_bytes(&[]), None);
    }
}
