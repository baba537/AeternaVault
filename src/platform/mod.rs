//! Operating-system specifics, kept behind small functions so the engine and
//! GUI stay portable. Windows is the primary target; other platforms get
//! simple fallbacks so the code base can grow in that direction later.

pub mod apps;
pub mod vss;

use std::path::{Path, PathBuf};

#[cfg(windows)]
mod windows;

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
