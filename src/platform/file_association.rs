//! Opening encrypted backups by double-click.
//!
//! Every vault folder contains `Open with AeternaVault.avault`. With the
//! association (per user, `HKCU\Software\Classes`, no administrator rights),
//! double-clicking it starts `aeternavault open "<file>"`, which asks for
//! the passphrase and shows the backups — without changing any settings.

// Windows only; the other platforms get the no-op fallbacks below.
#![cfg_attr(not(windows), allow(dead_code))]

use std::io;

const EXTENSION: &str = ".avault";
const PROG_ID: &str = "AeternaVault.EncryptedBackups";

pub fn supported() -> bool {
    cfg!(windows)
}

fn command() -> io::Result<(String, String)> {
    let exe = super::gui_executable()?;
    Ok((
        format!("\"{}\" open \"%1\"", exe.display()),
        format!("\"{}\",0", exe.display()),
    ))
}

pub fn is_registered() -> bool {
    #[cfg(windows)]
    {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        let Ok((expected, _)) = command() else {
            return false;
        };
        let classes = RegKey::predef(HKEY_CURRENT_USER);
        let points_here = classes
            .open_subkey(format!(r"Software\Classes\{PROG_ID}\shell\open\command"))
            .and_then(|k| k.get_value::<String, _>(""))
            .is_ok_and(|v| v.eq_ignore_ascii_case(&expected));
        let extension = classes
            .open_subkey(format!(r"Software\Classes\{EXTENSION}"))
            .and_then(|k| k.get_value::<String, _>(""))
            .is_ok_and(|v| v == PROG_ID);
        points_here && extension
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub fn set_registered(enabled: bool) -> io::Result<()> {
    if !super::system_changes_allowed() {
        return Err(io::Error::other(
            "not changed in demo mode (AETERNAVAULT_PROFILE_ROOT is set)",
        ));
    }
    #[cfg(windows)]
    {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if enabled {
            let (open, icon) = command()?;
            let (ext, _) = hkcu.create_subkey(format!(r"Software\Classes\{EXTENSION}"))?;
            ext.set_value("", &PROG_ID)?;
            let (prog, _) = hkcu.create_subkey(format!(r"Software\Classes\{PROG_ID}"))?;
            prog.set_value("", &"AeternaVault encrypted backups")?;
            let (default_icon, _) = prog.create_subkey("DefaultIcon")?;
            default_icon.set_value("", &icon)?;
            let (open_command, _) = prog.create_subkey(r"shell\open\command")?;
            open_command.set_value("", &open)?;
            tracing::info!("encrypted backups open by double-click");
        } else {
            for key in [
                format!(r"Software\Classes\{PROG_ID}"),
                format!(r"Software\Classes\{EXTENSION}"),
            ] {
                match hkcu.delete_subkey_all(&key) {
                    Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err),
                    _ => {}
                }
            }
            tracing::info!("double-click association for encrypted backups removed");
        }
        notify_shell();
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}

#[cfg(windows)]
fn notify_shell() {
    use windows_sys::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
    // SAFETY: documented way to tell Explorer that file associations changed.
    unsafe {
        SHChangeNotify(
            SHCNE_ASSOCCHANGED as i32,
            SHCNF_IDLIST,
            std::ptr::null(),
            std::ptr::null(),
        );
    }
}
