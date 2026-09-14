//! Small persistent state next to the configuration: the outcome of the last
//! automatic backup, so the window can report it on the next start.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const STATE_FILE: &str = "state.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AutomaticOutcome {
    Complete,
    CompleteWithNotes,
    /// The destination (e.g. an external drive) was not connected.
    DestinationUnavailable,
    /// Encrypted backups are on, but the key is not remembered on this computer.
    NeedsPassphrase,
    AlreadyRunning,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomaticRun {
    pub at: DateTime<Utc>,
    pub outcome: AutomaticOutcome,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub files: u64,
    #[serde(default)]
    pub bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub last_automatic: Option<AutomaticRun>,
    /// When the user last saw the result in the window.
    pub acknowledged_at: Option<DateTime<Utc>>,
    /// Executable path the scheduled task was created for.
    pub task_executable: Option<PathBuf>,
}

impl State {
    pub fn path(config_file: &Path) -> PathBuf {
        config_file
            .parent()
            .map(|p| p.join(STATE_FILE))
            .unwrap_or_else(|| PathBuf::from(STATE_FILE))
    }

    pub fn load(config_file: &Path) -> Self {
        std::fs::read_to_string(Self::path(config_file))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, config_file: &Path) {
        let path = Self::path(config_file);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_vec_pretty(self) {
            Ok(bytes) => {
                if let Err(err) = std::fs::write(&path, bytes) {
                    tracing::warn!("state could not be saved: {err}");
                }
            }
            Err(err) => tracing::warn!("state could not be encoded: {err}"),
        }
    }

    /// An automatic backup result the user has not seen yet.
    pub fn unseen_automatic(&self) -> Option<&AutomaticRun> {
        let run = self.last_automatic.as_ref()?;
        match self.acknowledged_at {
            Some(seen) if seen >= run.at => None,
            _ => Some(run),
        }
    }
}

/// Folder for remembered vault keys (protected with DPAPI).
pub fn key_dir(config_file: &Path) -> PathBuf {
    config_file
        .parent()
        .map(|p| p.join("keys"))
        .unwrap_or_else(|| PathBuf::from("keys"))
}
