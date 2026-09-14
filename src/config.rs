//! User configuration, stored as a hand-editable TOML file.
//!
//! TOML was chosen over JSON because it allows comments and is forgiving to
//! edit in Notepad. Every field has a default (`#[serde(default)]`), so a
//! partial or older file still loads.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths::AppPaths;
use crate::platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LanguageSetting {
    #[default]
    Auto,
    En,
    De,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupMode {
    /// Copy every file again.
    Full,
    /// Copy only new and changed files; unchanged files are linked or referenced.
    #[default]
    Incremental,
}

/// What a restore does when a file already exists at the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConflictPolicy {
    /// Replace files that differ from the backup.
    #[default]
    ReplaceChanged,
    /// Never touch files that already exist.
    KeepExisting,
    /// Replace only if the existing file is older than the backed-up one.
    KeepNewer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Source {
    pub name: String,
    pub path: PathBuf,
    pub enabled: bool,
    /// Additional exclude patterns for this source only.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
    /// Individually chosen sub-folders and files (relative, `/`-separated).
    /// The nearest entry wins; `"."` stands for the whole folder.
    /// See [`crate::engine::selection`].
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub include_paths: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exclude_paths: Vec<String>,
}

impl Default for Source {
    fn default() -> Self {
        Self {
            name: String::new(),
            path: PathBuf::new(),
            enabled: true,
            exclude: Vec::new(),
            include_paths: Vec::new(),
            exclude_paths: Vec::new(),
        }
    }
}

impl Source {
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>, enabled: bool) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            enabled,
            ..Self::default()
        }
    }

    pub fn selection(&self) -> crate::engine::selection::Selection<'_> {
        crate::engine::selection::Selection::new(&self.include_paths, &self.exclude_paths)
    }

    pub fn is_partial(&self) -> bool {
        !self.include_paths.is_empty() || !self.exclude_paths.is_empty()
    }

    /// Display name, falling back to the folder name.
    pub fn display_name(&self) -> String {
        if !self.name.trim().is_empty() {
            return self.name.clone();
        }
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Advanced {
    /// Skip OneDrive/cloud files that are not stored locally (reading them
    /// would trigger a download).
    pub skip_online_only_files: bool,
    /// In incremental mode, hard-link unchanged files from the previous backup
    /// so every backup folder is complete and browsable in Explorer.
    pub hardlink_unchanged: bool,
    /// Show a short confirmation before a backup or restore starts.
    pub confirm_before_start: bool,
    /// Compare SHA-256 checksums while restoring.
    pub verify_on_restore: bool,
    pub restore_conflict: ConflictPolicy,
    /// Save a list of installed programs (and a `winget` export if available)
    /// with every backup, to make reinstalling on a new computer easier.
    pub save_program_list: bool,
}

impl Default for Advanced {
    fn default() -> Self {
        Self {
            skip_online_only_files: true,
            hardlink_unchanged: true,
            confirm_before_start: true,
            verify_on_restore: true,
            restore_conflict: ConflictPolicy::default(),
            save_program_list: true,
        }
    }
}

/// An application from the catalog chosen for backup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppChoice {
    pub id: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Frequency {
    #[default]
    Daily,
    Weekly,
    /// Every few hours.
    Hourly,
    /// A few minutes after signing in to Windows.
    AtLogon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    #[default]
    Sunday,
}

impl Weekday {
    pub const ALL: [Weekday; 7] = [
        Weekday::Monday,
        Weekday::Tuesday,
        Weekday::Wednesday,
        Weekday::Thursday,
        Weekday::Friday,
        Weekday::Saturday,
        Weekday::Sunday,
    ];
}

/// Automatic backups (Windows Task Scheduler).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Schedule {
    pub enabled: bool,
    pub frequency: Frequency,
    /// "HH:MM", local time.
    pub time: String,
    pub weekday: Weekday,
    pub every_hours: u8,
    /// Run as soon as possible if the computer was off at the planned time.
    pub catch_up: bool,
    pub only_on_ac_power: bool,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            enabled: false,
            frequency: Frequency::Daily,
            time: "20:00".into(),
            weekday: Weekday::Sunday,
            every_hours: 4,
            catch_up: true,
            only_on_ac_power: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Encryption {
    /// New backups are written into the encrypted vault at the destination.
    pub enabled: bool,
}

/// Optional font overrides. Empty means: use the built-in choice
/// (Georgia / Segoe UI from the Windows font folder, egui defaults otherwise).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fonts {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub language: LanguageSetting,
    pub appearance: Appearance,
    pub destination: PathBuf,
    pub mode: BackupMode,
    /// Exclude patterns (glob syntax) applied to file and folder names and to
    /// paths relative to the source, case-insensitive.
    pub exclude: Vec<String>,
    pub encryption: Encryption,
    pub schedule: Schedule,
    pub advanced: Advanced,
    pub fonts: Fonts,
    #[serde(rename = "source")]
    pub sources: Vec<Source>,
    /// Applications whose settings are backed up (see the catalog).
    #[serde(rename = "application")]
    pub apps: Vec<AppChoice>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            language: LanguageSetting::Auto,
            appearance: Appearance::System,
            destination: PathBuf::new(),
            mode: BackupMode::Incremental,
            exclude: default_excludes(),
            encryption: Encryption::default(),
            schedule: Schedule::default(),
            advanced: Advanced::default(),
            fonts: Fonts::default(),
            sources: Vec::new(),
            apps: Vec::new(),
        }
    }
}

pub fn default_excludes() -> Vec<String> {
    [
        "Thumbs.db",
        "desktop.ini",
        "~$*",
        "*.tmp",
        "*.aeterna-partial",
        "$RECYCLE.BIN",
        "System Volume Information",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

impl Config {
    /// Sensible defaults for a first start: the user's personal folders and a
    /// destination on a second drive if one exists.
    pub fn first_run() -> Self {
        let mut config = Self {
            destination: platform::default_destination(),
            ..Self::default()
        };

        // Source names are written once, in the language of the system.
        let lang = crate::i18n::Lang::resolve(LanguageSetting::Auto);
        let de = lang == crate::i18n::Lang::De;

        // `UserDirs` resolves Known Folders, so redirected folders (for example
        // Documents moved to OneDrive) are found correctly.
        if let Some(dirs) = directories::UserDirs::new() {
            let candidates: [(&str, &str, Option<&Path>, bool); 6] = [
                ("Desktop", "Desktop", dirs.desktop_dir(), true),
                ("Documents", "Dokumente", dirs.document_dir(), true),
                ("Pictures", "Bilder", dirs.picture_dir(), true),
                ("Music", "Musik", dirs.audio_dir(), false),
                ("Videos", "Videos", dirs.video_dir(), false),
                ("Downloads", "Downloads", dirs.download_dir(), false),
            ];
            for (en, de_name, path, enabled) in candidates {
                if let Some(path) = path.filter(|p| p.is_dir()) {
                    let name = if de { de_name } else { en };
                    config.sources.push(Source::new(name, path, enabled));
                }
            }
        }

        // Pre-select the settings of applications found on this computer.
        let catalog = platform::apps::Catalog::builtin();
        let known = platform::known_paths::KnownPaths::current();
        for app in catalog
            .apps
            .iter()
            .filter(|a| a.default && a.is_detected(&known))
        {
            config.apps.push(AppChoice {
                id: app.id.clone(),
                enabled: true,
            });
        }

        config
    }

    pub fn app_enabled(&self, id: &str) -> bool {
        self.apps.iter().any(|a| a.id == id && a.enabled)
    }

    pub fn set_app_enabled(&mut self, id: &str, enabled: bool) {
        match self.apps.iter_mut().find(|a| a.id == id) {
            Some(choice) => choice.enabled = enabled,
            None => self.apps.push(AppChoice {
                id: id.to_string(),
                enabled,
            }),
        }
    }

    pub fn enabled_sources(&self) -> impl Iterator<Item = &Source> {
        self.sources.iter().filter(|s| s.enabled)
    }

    pub fn has_source_path(&self, path: &Path) -> bool {
        self.sources
            .iter()
            .any(|s| crate::engine::paths_equal(&s.path, path))
    }

    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        let body = toml::to_string_pretty(self)?;
        Ok(format!("{CONFIG_HEADER}\n{body}"))
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let text = self.to_toml().map_err(std::io::Error::other)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Write to a temporary file first so a crash never leaves a half-written config.
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }
}

const CONFIG_HEADER: &str = "\
# AeternaVault configuration
#
# This file can be edited by hand. Changes are picked up when AeternaVault
# starts, or via Settings -> Reload configuration.
#
# language    = \"auto\" | \"en\" | \"de\"
# appearance  = \"system\" | \"dark\" | \"light\"
# mode        = \"incremental\" | \"full\"
# exclude     = glob patterns, matched case-insensitively against names and relative paths
#
# Each [[source]] block is one folder to back up:
#   [[source]]
#   name = \"Documents\"
#   path = 'C:\\Users\\you\\Documents'
#   enabled = true
#   exclude = [\"*.bak\"]          # optional patterns
#   exclude_paths = [\"Old stuff\"] # optional: sub-folders or files left out
#   include_paths = []             # optional: with exclude_paths = [\".\"], only these
#
# Each [[application]] block selects the settings of one application from the
# catalog (ids are listed in the Applications view):
#   [[application]]
#   id = \"firefox\"
#   enabled = true
";

/// Outcome of loading the configuration, used to show a quiet notice in the UI.
#[derive(Debug, Clone)]
pub enum ConfigNotice {
    Created,
    Invalid { kept_copy: PathBuf, message: String },
    NotSaved { message: String },
}

pub struct Loaded {
    pub config: Config,
    pub notice: Option<ConfigNotice>,
}

pub fn load_or_create(paths: &AppPaths) -> Loaded {
    let path = &paths.config_file;
    match std::fs::read_to_string(path) {
        Ok(text) => match toml::from_str::<Config>(&text) {
            Ok(config) => Loaded {
                config,
                notice: None,
            },
            Err(err) => {
                // Never overwrite a file the user may have edited: keep a copy.
                let kept_copy = path.with_extension("invalid.toml");
                let _ = std::fs::copy(path, &kept_copy);
                tracing::warn!("configuration could not be parsed, using defaults: {err}");
                Loaded {
                    config: Config::first_run(),
                    notice: Some(ConfigNotice::Invalid {
                        kept_copy,
                        message: err.message().to_string(),
                    }),
                }
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let config = Config::first_run();
            let notice = match config.save(path) {
                Ok(()) => ConfigNotice::Created,
                Err(err) => ConfigNotice::NotSaved {
                    message: err.to_string(),
                },
            };
            tracing::info!("created configuration at {}", path.display());
            Loaded {
                config,
                notice: Some(notice),
            }
        }
        Err(err) => {
            tracing::warn!("configuration could not be read: {err}");
            Loaded {
                config: Config::first_run(),
                notice: Some(ConfigNotice::NotSaved {
                    message: err.to_string(),
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_toml() {
        let config = Config {
            destination: PathBuf::from(r"D:\AeternaVault\Backups"),
            sources: vec![Source::new("Documents", r"C:\Users\a\Documents", true)],
            ..Config::default()
        };
        let text = config.to_toml().unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn partial_file_uses_defaults() {
        let parsed: Config = toml::from_str("language = \"de\"").unwrap();
        assert_eq!(parsed.language, LanguageSetting::De);
        assert_eq!(parsed.mode, BackupMode::Incremental);
        assert!(parsed.advanced.verify_on_restore);
    }
}
