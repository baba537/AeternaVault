//! "Start with Windows": an entry in the current user's autostart list
//! (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`), which starts
//! AeternaVault quietly in the notification area. No administrator rights are
//! needed, and Windows' own "Startup apps" settings can switch it off as well.

use std::io;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "AeternaVault";
pub const ARGUMENT: &str = "--background";

fn command_line() -> io::Result<String> {
    let exe = std::env::current_exe()?;
    Ok(format!("\"{}\" {ARGUMENT}", exe.display()))
}

/// Whether the entry exists and points to this executable.
pub fn is_enabled() -> bool {
    #[cfg(windows)]
    {
        let Ok(expected) = command_line() else {
            return false;
        };
        winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
            .open_subkey(RUN_KEY)
            .and_then(|key| key.get_value::<String, _>(VALUE))
            .is_ok_and(|value| value.trim().eq_ignore_ascii_case(&expected))
    }
    #[cfg(not(windows))]
    {
        false
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
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if enabled {
            let (key, _) = hkcu.create_subkey(RUN_KEY)?;
            key.set_value(VALUE, &command_line()?)?;
            tracing::info!("AeternaVault starts with Windows");
        } else {
            match hkcu.open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE) {
                Ok(key) => match key.delete_value(VALUE) {
                    Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err),
                    _ => {}
                },
                Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err),
                Err(_) => {}
            }
            tracing::info!("AeternaVault no longer starts with Windows");
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err(io::Error::other("not supported on this platform"))
    }
}
