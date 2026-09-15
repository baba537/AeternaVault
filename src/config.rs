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
    /// With encryption of selected items: sub-folders and files (or `"."`)
    /// that are encrypted. The nearest entry wins, like the selection above.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub encrypt_paths: Vec<String>,
    /// Exceptions inside encrypted folders that stay unencrypted.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub plain_paths: Vec<String>,
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
            encrypt_paths: Vec::new(),
            plain_paths: Vec::new(),
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
    /// When a destination is chosen, create an `AeternaVault` folder inside it.
    pub destination_app_folder: bool,
    /// Draw the window with OpenGL instead of Direct3D 12 (for graphics drivers
    /// that show glitches). Takes effect on the next start.
    pub compatibility_graphics: bool,
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
            destination_app_folder: true,
            compatibility_graphics: false,
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
    /// A few minutes after AeternaVault starts (with Windows: after signing in).
    #[serde(alias = "at-logon")]
    AtStart,
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

/// One automatic backup. AeternaVault itself runs them while it is open or
/// waiting in the notification area (see [`crate::automatic`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Schedule {
    /// Stable identifier, used to remember when the schedule last ran.
    pub id: String,
    /// Optional name chosen by the user; a description is shown otherwise.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub enabled: bool,
    pub frequency: Frequency,
    /// "HH:MM", local time.
    pub time: String,
    pub weekday: Weekday,
    pub every_hours: u8,
    /// Run as soon as possible if the computer was off at the planned time.
    pub catch_up: bool,
    pub only_on_ac_power: bool,
    /// Back up all ticked folders; otherwise only those in `folders`.
    pub all_folders: bool,
    /// Folders (by path) this schedule backs up when `all_folders` is off.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<PathBuf>,
    /// Whether the chosen application settings are included.
    pub applications: bool,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            enabled: true,
            frequency: Frequency::Daily,
            time: "20:00".into(),
            weekday: Weekday::Sunday,
            every_hours: 4,
            catch_up: true,
            only_on_ac_power: false,
            all_folders: true,
            folders: Vec::new(),
            applications: true,
        }
    }
}

impl Schedule {
    pub fn new_id() -> String {
        crate::engine::crypto::hex(&crate::engine::crypto::random_bytes::<4>())
    }

    /// Whether this schedule covers everything that is ticked.
    pub fn is_everything(&self) -> bool {
        self.all_folders && self.applications
    }
}

/// Reads `schedule` either as the single table of AeternaVault 0.2 or as a
/// list of `[[schedule]]` tables.
fn schedules_compat<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Schedule>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        Many(Vec<Schedule>),
        One(Box<Schedule>),
    }
    Ok(match OneOrMany::deserialize(d)? {
        OneOrMany::Many(list) => list,
        // 0.2 always wrote a schedule table; a switched-off one is not worth keeping.
        OneOrMany::One(single) if single.enabled => vec![*single],
        OneOrMany::One(_) => Vec::new(),
    })
}

/// Removing old backups automatically.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Retention {
    /// Apply the rules after every backup.
    pub enabled: bool,
    /// Always keep this many of the newest backups.
    pub keep_last: u32,
    /// Keep the newest backup of each of the last … days,
    pub keep_daily: u32,
    /// … weeks,
    pub keep_weekly: u32,
    /// … and months.
    pub keep_monthly: u32,
}

impl Default for Retention {
    fn default() -> Self {
        Self {
            enabled: false,
            keep_last: 3,
            keep_daily: 7,
            keep_weekly: 4,
            keep_monthly: 12,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Background {
    /// Keep running in the notification area when the window is closed while
    /// automatic backups are switched on.
    pub keep_running: bool,
    /// Whether the user has already been told that closing keeps the app running.
    pub close_hint_shown: bool,
}

impl Default for Background {
    fn default() -> Self {
        Self {
            keep_running: true,
            close_hint_shown: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EncryptionScope {
    /// Everything goes into the encrypted vault.
    #[default]
    Everything,
    /// Only marked folders and files (and, if chosen, application settings);
    /// the rest stays a normal, browsable backup.
    Selected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Encryption {
    /// New backups are written (fully or partly) into the encrypted vault.
    pub enabled: bool,
    pub scope: EncryptionScope,
    /// With `Selected`: whether application settings are encrypted.
    pub applications: bool,
}

impl Default for Encryption {
    fn default() -> Self {
        Self {
            enabled: false,
            scope: EncryptionScope::Everything,
            applications: true,
        }
    }
}

impl Encryption {
    pub fn everything(&self) -> bool {
        self.enabled && self.scope == EncryptionScope::Everything
    }

    pub fn selected(&self) -> bool {
        self.enabled && self.scope == EncryptionScope::Selected
    }
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
    /// Size of the interface in percent, on top of the Windows display scaling.
    pub interface_scale: u16,
    pub destination: PathBuf,
    pub mode: BackupMode,
    /// Exclude patterns (glob syntax) applied to file and folder names and to
    /// paths relative to the source, case-insensitive.
    pub exclude: Vec<String>,
    pub encryption: Encryption,
    pub retention: Retention,
    pub background: Background,
    pub advanced: Advanced,
    pub fonts: Fonts,
    #[serde(
        rename = "schedule",
        deserialize_with = "schedules_compat",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub schedules: Vec<Schedule>,
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
            interface_scale: 100,
            destination: PathBuf::new(),
            mode: BackupMode::Incremental,
            exclude: default_excludes(),
            encryption: Encryption::default(),
            retention: Retention::default(),
            background: Background::default(),
            advanced: Advanced::default(),
            fonts: Fonts::default(),
            schedules: Vec::new(),
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

    /// Fills in what older or hand-edited files may lack (schedule ids).
    pub fn normalize(&mut self) {
        let mut seen = std::collections::HashSet::new();
        for schedule in &mut self.schedules {
            if schedule.id.trim().is_empty() || !seen.insert(schedule.id.clone()) {
                schedule.id = Schedule::new_id();
                seen.insert(schedule.id.clone());
            }
        }
    }

    pub fn any_schedule_enabled(&self) -> bool {
        self.schedules.iter().any(|s| s.enabled)
    }

    /// The configuration as one schedule sees it: only its folders and, if
    /// chosen, the application settings.
    pub fn for_schedule(&self, schedule: &Schedule) -> Config {
        let mut config = self.clone();
        if !schedule.all_folders {
            for source in &mut config.sources {
                source.enabled = source.enabled
                    && schedule
                        .folders
                        .iter()
                        .any(|f| crate::engine::paths_equal(f, &source.path));
            }
        }
        if !schedule.applications {
            for app in &mut config.apps {
                app.enabled = false;
            }
        }
        config
    }

    /// `interface_scale` as egui zoom factor, limited to sensible values.
    pub fn zoom_factor(&self) -> f32 {
        f32::from(self.interface_scale.clamp(75, 200)) / 100.0
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
#
# Each [[schedule]] block is one automatic backup:
#   [[schedule]]
#   frequency = \"daily\"            # daily | weekly | hourly | at-start
#   time = \"20:00\"
#   weekday = \"sunday\"             # for weekly
#   every_hours = 4                # for hourly
#   all_folders = false            # optional: only the folders listed below
#   folders = ['C:\\Users\\you\\Documents']
#   applications = true
";

/// Outcome of loading the configuration, used to show a quiet notice in the UI.
#[derive(Debug, Clone)]
pub enum ConfigNotice {
    Created,
    Invalid { kept_copy: PathBuf, message: String },
    NotSaved { message: String },
}

#[derive(Clone)]
pub struct Loaded {
    pub config: Config,
    pub notice: Option<ConfigNotice>,
}

pub fn load_or_create(paths: &AppPaths) -> Loaded {
    let path = &paths.config_file;
    match std::fs::read_to_string(path) {
        // Older Notepad versions save UTF-8 with a byte order mark.
        Ok(text) => match toml::from_str::<Config>(text.trim_start_matches('\u{feff}')) {
            Ok(mut config) => {
                config.normalize();
                Loaded {
                    config,
                    notice: None,
                }
            }
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
    fn schedules_roundtrip_and_old_single_schedule_is_read() {
        let mut config = Config {
            sources: vec![Source::new("Documents", r"C:\Users\a\Documents", true)],
            schedules: vec![
                Schedule::default(),
                Schedule {
                    frequency: Frequency::Hourly,
                    all_folders: false,
                    folders: vec![PathBuf::from(r"C:\Users\a\Documents")],
                    applications: false,
                    ..Schedule::default()
                },
            ],
            ..Config::default()
        };
        config.normalize();
        assert_ne!(config.schedules[0].id, config.schedules[1].id);
        let text = config.to_toml().unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(parsed, config);

        // AeternaVault 0.2 wrote one [schedule] table.
        let old = "[schedule]\nenabled = true\nfrequency = \"at-logon\"\ntime = \"08:00\"\n";
        let mut parsed: Config = toml::from_str(old).unwrap();
        parsed.normalize();
        assert_eq!(parsed.schedules.len(), 1);
        assert_eq!(parsed.schedules[0].frequency, Frequency::AtStart);
        assert!(!parsed.schedules[0].id.is_empty());
        let off = "[schedule]\nenabled = false\n";
        assert!(toml::from_str::<Config>(off).unwrap().schedules.is_empty());

        // A schedule for one folder leaves the other folders and the apps out.
        let mut two = config.clone();
        two.sources
            .push(Source::new("Music", r"C:\Users\a\Music", true));
        two.apps.push(AppChoice {
            id: "firefox".into(),
            enabled: true,
        });
        let narrowed = two.for_schedule(&config.schedules[1]);
        assert!(narrowed.sources[0].enabled);
        assert!(!narrowed.sources[1].enabled);
        assert!(!narrowed.apps[0].enabled);
    }

    #[test]
    fn partial_file_uses_defaults() {
        let parsed: Config = toml::from_str("language = \"de\"").unwrap();
        assert_eq!(parsed.language, LanguageSetting::De);
        assert_eq!(parsed.mode, BackupMode::Incremental);
        assert!(parsed.advanced.verify_on_restore);
    }
}
