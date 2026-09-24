//! Backup jobs in the background on Linux: a systemd user timer that runs
//! `aeternavault-cli jobs run-due` every few minutes. The command runs the
//! jobs that are due (with the same rules as the window) and exits.
//!
//! Files: `~/.config/systemd/user/aeternavault.service` and `.timer`. Nothing
//! needs administrator rights; the timer runs while the user is logged in
//! (or always, with `loginctl enable-linger`).

use std::io;
use std::path::{Path, PathBuf};

pub use super::cli_executable;

pub const UNIT: &str = "aeternavault";

pub fn supported() -> bool {
    cfg!(target_os = "linux")
}

fn unit_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.config_dir().join("systemd").join("user"))
}

pub fn is_installed() -> bool {
    unit_dir().is_some_and(|dir| dir.join(format!("{UNIT}.timer")).is_file())
}

fn systemctl(args: &[&str]) -> io::Result<()> {
    let status = std::process::Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "systemctl --user {} failed ({status})",
            args.join(" ")
        )))
    }
}

/// Writes and starts the timer. `every_minutes` is how often due jobs are checked.
pub fn install(cli: &Path, every_minutes: u32) -> io::Result<PathBuf> {
    if !super::system_changes_allowed() {
        return Err(io::Error::other(
            "not changed in demo mode (AETERNAVAULT_PROFILE_ROOT is set)",
        ));
    }
    let dir = unit_dir().ok_or_else(|| io::Error::other("no home folder"))?;
    std::fs::create_dir_all(&dir)?;
    let service = format!(
        "[Unit]\nDescription=AeternaVault backup jobs\n\n[Service]\nType=oneshot\nExecStart=\"{}\" jobs run-due\nNice=10\nIOSchedulingClass=idle\n",
        cli.display()
    );
    let timer = format!(
        "[Unit]\nDescription=Run due AeternaVault backup jobs\n\n[Timer]\nOnBootSec=3min\nOnUnitActiveSec={}min\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n",
        every_minutes.max(1)
    );
    std::fs::write(dir.join(format!("{UNIT}.service")), service)?;
    std::fs::write(dir.join(format!("{UNIT}.timer")), timer)?;
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", "--now", &format!("{UNIT}.timer")])?;
    tracing::info!("systemd timer for backup jobs installed");
    Ok(dir)
}

pub fn remove() -> io::Result<()> {
    if !super::system_changes_allowed() {
        return Err(io::Error::other(
            "not changed in demo mode (AETERNAVAULT_PROFILE_ROOT is set)",
        ));
    }
    let Some(dir) = unit_dir() else {
        return Ok(());
    };
    let _ = systemctl(&["disable", "--now", &format!("{UNIT}.timer")]);
    for ext in ["timer", "service"] {
        match std::fs::remove_file(dir.join(format!("{UNIT}.{ext}"))) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err),
            _ => {}
        }
    }
    let _ = systemctl(&["daemon-reload"]);
    tracing::info!("systemd timer for backup jobs removed");
    Ok(())
}
