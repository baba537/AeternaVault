//! Clean-up of the Windows Task Scheduler entry that AeternaVault 0.2 created
//! for automatic backups (`\AeternaVault\Automatic backup`).
//!
//! Since 0.3, AeternaVault runs its schedules itself (see
//! [`crate::automatic`]). The old task is removed once, when the window starts,
//! so backups do not run twice.

use std::io;
use std::process::Command;

pub const TASK_FOLDER: &str = "AeternaVault";
pub const TASK_NAME: &str = "Automatic backup";

pub fn task_path() -> String {
    format!("\\{TASK_FOLDER}\\{TASK_NAME}")
}

fn schtasks() -> Command {
    let mut command = Command::new("schtasks.exe");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

pub fn is_installed() -> bool {
    schtasks()
        .args(["/Query", "/TN"])
        .arg(task_path())
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Removes the old task; succeeds if it does not exist.
pub fn remove_legacy_task() -> io::Result<bool> {
    if !super::system_changes_allowed() || !is_installed() {
        return Ok(false);
    }
    let output = schtasks()
        .args(["/Delete", "/F", "/TN"])
        .arg(task_path())
        .output()?;
    if output.status.success() {
        tracing::info!("removed the scheduled task of AeternaVault 0.2");
        Ok(true)
    } else {
        Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}
