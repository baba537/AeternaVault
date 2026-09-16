//! "Back up with AeternaVault" in the Explorer context menu of folders.
//!
//! Registered per user (`HKCU\Software\Classes\Directory\shell\AeternaVault`),
//! no administrator rights. On Windows 11 the entry is under "Show more options".
//! It runs `AeternaVault.exe add "<folder>"`, which hands the folder to the
//! running window (see [`pending`]) or starts one.

use std::io;
use std::path::{Path, PathBuf};

const KEY: &str = r"Software\Classes\Directory\shell\AeternaVault";

fn command() -> io::Result<(String, String)> {
    let exe = std::env::current_exe()?;
    Ok((
        format!("\"{}\" add \"%1\"", exe.display()),
        format!("\"{}\",0", exe.display()),
    ))
}

pub fn is_registered() -> bool {
    #[cfg(windows)]
    {
        let Ok((expected, _)) = command() else {
            return false;
        };
        winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
            .open_subkey(format!(r"{KEY}\command"))
            .and_then(|k| k.get_value::<String, _>(""))
            .is_ok_and(|v| v.eq_ignore_ascii_case(&expected))
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// `label` is the menu text in the user's language.
pub fn set_registered(enabled: bool, label: &str) -> io::Result<()> {
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
            let (run, icon) = command()?;
            let (key, _) = hkcu.create_subkey(KEY)?;
            key.set_value("MUIVerb", &label)?;
            key.set_value("Icon", &icon)?;
            let (command_key, _) = key.create_subkey("command")?;
            command_key.set_value("", &run)?;
            tracing::info!("Explorer menu entry added");
        } else {
            match hkcu.delete_subkey_all(KEY) {
                Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err),
                _ => {}
            }
            tracing::info!("Explorer menu entry removed");
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (enabled, label);
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}

/// Folders handed over by `AeternaVault.exe add`, waiting for the window.
pub mod pending {
    use super::*;

    fn file(config_file: &Path) -> PathBuf {
        config_file
            .parent()
            .map(|dir| dir.join("pending-folders.txt"))
            .unwrap_or_else(|| PathBuf::from("pending-folders.txt"))
    }

    pub fn push(config_file: &Path, folder: &Path) -> io::Result<()> {
        use std::io::Write as _;
        let path = file(config_file);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        writeln!(f, "{}", folder.display())
    }

    /// Takes all waiting folders (the file is removed).
    pub fn take(config_file: &Path) -> Vec<PathBuf> {
        let path = file(config_file);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Vec::new();
        };
        let _ = std::fs::remove_file(&path);
        text.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::pending;

    #[test]
    fn pending_folders_are_handed_over_once() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("config.toml");
        pending::push(&config, &tmp.path().join("Photos")).unwrap();
        pending::push(&config, &tmp.path().join("Letters")).unwrap();
        let taken = pending::take(&config);
        assert_eq!(taken.len(), 2);
        assert!(taken[1].ends_with("Letters"));
        assert!(pending::take(&config).is_empty());
    }
}
