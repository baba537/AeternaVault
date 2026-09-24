//! Where AeternaVault keeps its own files (configuration and logs).
//!
//! Resolution order:
//! 1. `AETERNAVAULT_HOME` environment variable — everything lives in that folder.
//! 2. Portable mode — an `AeternaVault.toml` next to the executable.
//! 3. Standard per-user folders:
//!    * Windows: `%APPDATA%\AeternaVault` for settings and
//!      `%LOCALAPPDATA%\AeternaVault\logs` for logs (logs are machine-specific
//!      and should not roam with the profile).
//!    * Linux: `~/.config/aeternavault` and `~/.local/share/aeternavault/logs`.
//!
//! These folders are outside the program folder, so updating or reinstalling
//! AeternaVault keeps all settings.

use std::path::PathBuf;

pub const APP_DIR_NAME: &str = if cfg!(windows) {
    "AeternaVault"
} else {
    "aeternavault"
};
pub const PORTABLE_CONFIG_NAME: &str = "AeternaVault.toml";

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_file: PathBuf,
    pub log_dir: PathBuf,
    pub portable: bool,
}

impl AppPaths {
    pub fn resolve() -> Self {
        if let Some(home) = std::env::var_os("AETERNAVAULT_HOME").filter(|v| !v.is_empty()) {
            let home = PathBuf::from(home);
            return Self {
                config_file: home.join("config.toml"),
                log_dir: home.join("logs"),
                portable: false,
            };
        }

        if let Some(exe_dir) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from))
        {
            let portable = exe_dir.join(PORTABLE_CONFIG_NAME);
            if portable.is_file() {
                return Self {
                    config_file: portable,
                    log_dir: exe_dir.join("logs"),
                    portable: true,
                };
            }
        }

        // `directories` wraps SHGetKnownFolderPath, which also honours folder
        // redirection (e.g. roaming profiles) instead of guessing from env vars.
        let base = directories::BaseDirs::new();
        let config_root = base
            .as_ref()
            .map(|b| b.config_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let local_root = base
            .as_ref()
            .map(|b| b.data_local_dir().to_path_buf())
            .unwrap_or_else(|| config_root.clone());

        Self {
            config_file: config_root.join(APP_DIR_NAME).join("config.toml"),
            log_dir: local_root.join(APP_DIR_NAME).join("logs"),
            portable: false,
        }
    }
}
