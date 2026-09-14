//! Operating-system specifics, kept behind small functions so the engine and
//! GUI stay portable. Windows is the primary target; other platforms get
//! simple fallbacks so the code base can grow in that direction later.

pub mod apps;
pub mod known_paths;
pub mod registry;
pub mod scheduler;
pub mod vss;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[cfg(windows)]
mod windows;

/// Marks a folder as hidden. Failures are ignored: hiding is cosmetic.
pub fn set_hidden(path: &Path) {
    #[cfg(windows)]
    if let Err(err) = windows::set_hidden(path) {
        tracing::debug!("could not hide {}: {err}", path.display());
    }
    #[cfg(not(windows))]
    let _ = path;
}

/// Lower CPU and I/O priority for unattended runs.
pub fn enter_background_mode() {
    #[cfg(windows)]
    windows::enter_background_mode();
}

/// Lower-case executable names of running processes.
pub fn running_processes() -> HashSet<String> {
    #[cfg(windows)]
    {
        windows::running_processes()
    }
    #[cfg(not(windows))]
    {
        HashSet::new()
    }
}

/// Protect secret bytes for the current user (Windows DPAPI).
pub fn protect_for_user(data: &[u8]) -> std::io::Result<Vec<u8>> {
    #[cfg(windows)]
    {
        windows::dpapi_protect(data)
    }
    #[cfg(not(windows))]
    {
        let _ = data;
        Err(std::io::Error::other("not supported on this platform"))
    }
}

pub fn unprotect_for_user(data: &[u8]) -> std::io::Result<Vec<u8>> {
    #[cfg(windows)]
    {
        windows::dpapi_unprotect(data)
    }
    #[cfg(not(windows))]
    {
        let _ = data;
        Err(std::io::Error::other("not supported on this platform"))
    }
}

/// Attach to the console of the parent process (for command-line use of a
/// GUI-subsystem executable). Does nothing if there is no parent console.
pub fn attach_parent_console() {
    #[cfg(windows)]
    windows::attach_parent_console();
}

/// Show a last-resort error message when the window cannot be created.
pub fn show_fatal_error(message: &str) {
    let _ = rfd::MessageDialog::new()
        .set_title("AeternaVault")
        .set_description(message)
        .set_level(rfd::MessageLevel::Error)
        .show();
}

/// A reasonable default destination: a folder on the first additional fixed
/// drive (e.g. `D:\AeternaVault\Backups`), otherwise inside the user profile.
pub fn default_destination() -> PathBuf {
    #[cfg(windows)]
    if let Some(root) = windows::first_secondary_fixed_drive() {
        return root.join("AeternaVault").join("Backups");
    }

    directories::UserDirs::new()
        .map(|d| d.home_dir().join("AeternaVault Backups"))
        .unwrap_or_else(|| PathBuf::from("AeternaVault Backups"))
}

/// Free space on the volume that contains `path`, if it can be determined.
pub fn free_space(path: &Path) -> Option<u64> {
    #[cfg(windows)]
    {
        windows::free_space(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}

pub fn computer_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "computer".to_string())
}

pub fn user_name() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default()
}

/// Open a folder (or the folder containing a file) in the system file manager.
pub fn open_in_file_manager(path: &Path) {
    let target = if path.is_file() {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    #[cfg(windows)]
    let result = std::process::Command::new("explorer").arg(target).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(target).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(target).spawn();
    if let Err(err) = result {
        tracing::warn!("could not open {}: {err}", target.display());
    }
}

/// Open a text file in the default editor (Notepad on Windows).
pub fn open_in_editor(path: &Path) {
    #[cfg(windows)]
    let result = std::process::Command::new("notepad.exe").arg(path).spawn();
    #[cfg(not(windows))]
    let result = std::process::Command::new("xdg-open").arg(path).spawn();
    if let Err(err) = result {
        tracing::warn!("could not open {}: {err}", path.display());
    }
}
