//! Application settings catalog and installed-program discovery.
//!
//! The catalog (`apps.toml`, embedded at build time) describes where
//! applications keep their settings: folders (with portable path tokens) and
//! HKCU registry keys. Users can extend or override it with an `apps.toml`
//! next to their `config.toml`.
//!
//! Installed desktop programs are read from the `Uninstall` registry keys
//! (the same data "Apps & features" shows). A list is saved with every backup
//! to help reinstalling on a new computer.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::known_paths::KnownPaths;
use crate::i18n::Lang;

const BUILTIN: &str = include_str!("apps.toml");
pub const USER_CATALOG_FILE: &str = "apps.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Browsers,
    Email,
    Communication,
    Office,
    Development,
    Media,
    Gaming,
    Security,
    Utilities,
    Windows,
}

impl Category {
    pub const ALL: [Category; 10] = [
        Category::Browsers,
        Category::Email,
        Category::Communication,
        Category::Office,
        Category::Development,
        Category::Media,
        Category::Gaming,
        Category::Security,
        Category::Utilities,
        Category::Windows,
    ];
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppFolder {
    /// Portable path, e.g. `{APPDATA}\Mozilla\Firefox`.
    pub path: String,
    /// Sub-folder inside the application's backup folder (when an app has several).
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    /// If not empty, only these relative paths are taken.
    #[serde(default)]
    pub only: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub name_de: Option<String>,
    pub category: Category,
    #[serde(default = "default_true")]
    pub default: bool,
    #[serde(default)]
    pub processes: Vec<String>,
    #[serde(default)]
    pub note_en: Option<String>,
    #[serde(default)]
    pub note_de: Option<String>,
    #[serde(default, rename = "folder")]
    pub folders: Vec<AppFolder>,
    #[serde(default)]
    pub registry: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl AppDef {
    pub fn display_name(&self, lang: Lang) -> &str {
        match (lang, &self.name_de) {
            (Lang::De, Some(name)) => name,
            _ => &self.name,
        }
    }

    pub fn note(&self, lang: Lang) -> Option<&str> {
        match lang {
            Lang::De => self.note_de.as_deref().or(self.note_en.as_deref()),
            Lang::En => self.note_en.as_deref(),
        }
    }

    /// Folders that exist on this computer, with their resolved absolute path.
    pub fn present_folders(&self, known: &KnownPaths) -> Vec<(&AppFolder, PathBuf)> {
        self.folders
            .iter()
            .filter_map(|folder| {
                let root = known.resolve(&folder.path)?;
                let present = if folder.only.is_empty() {
                    root.is_dir()
                } else {
                    folder
                        .only
                        .iter()
                        .any(|p| root.join(p.replace('/', "\\")).exists())
                };
                present.then_some((folder, root))
            })
            .collect()
    }

    pub fn present_registry_keys(&self) -> Vec<&str> {
        self.registry
            .iter()
            .map(String::as_str)
            .filter(|key| super::registry::key_exists(key))
            .collect()
    }

    pub fn is_detected(&self, known: &KnownPaths) -> bool {
        !self.present_folders(known).is_empty() || !self.present_registry_keys().is_empty()
    }

    pub fn is_running(&self, running: &std::collections::HashSet<String>) -> bool {
        self.processes
            .iter()
            .any(|p| running.contains(&p.to_lowercase()))
    }
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub apps: Vec<AppDef>,
}

#[derive(Deserialize)]
struct CatalogFile {
    #[serde(default, rename = "app")]
    apps: Vec<AppDef>,
}

impl Catalog {
    pub fn builtin() -> Self {
        let file: CatalogFile = toml::from_str(BUILTIN).expect("the built-in catalog is valid");
        Self { apps: file.apps }
    }

    /// Built-in catalog merged with the user's `apps.toml`, if present.
    pub fn load(config_dir: &Path) -> Self {
        let mut catalog = Self::builtin();
        let user_file = config_dir.join(USER_CATALOG_FILE);
        match std::fs::read_to_string(&user_file) {
            Ok(text) => match toml::from_str::<CatalogFile>(&text) {
                Ok(user) => {
                    for app in user.apps {
                        match catalog.apps.iter_mut().find(|a| a.id == app.id) {
                            Some(existing) => *existing = app,
                            None => catalog.apps.push(app),
                        }
                    }
                }
                Err(err) => tracing::warn!("{} could not be read: {err}", user_file.display()),
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => tracing::warn!("{} could not be read: {err}", user_file.display()),
        }
        catalog
    }

    pub fn get(&self, id: &str) -> Option<&AppDef> {
        self.apps.iter().find(|a| a.id == id)
    }
}

#[derive(Debug, Clone)]
pub struct InstalledApp {
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub install_location: Option<PathBuf>,
}

#[cfg(windows)]
pub fn installed_apps() -> Vec<InstalledApp> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};

    const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
    const UNINSTALL_WOW: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

    if super::known_paths::profile_override().is_some() {
        return demo_programs();
    }

    let hives = [
        (HKEY_LOCAL_MACHINE, UNINSTALL),
        (HKEY_LOCAL_MACHINE, UNINSTALL_WOW),
        (HKEY_CURRENT_USER, UNINSTALL),
    ];

    let mut apps = Vec::new();
    for (hive, path) in hives {
        let Ok(root) = RegKey::predef(hive).open_subkey_with_flags(path, KEY_READ) else {
            continue;
        };
        for key_name in root.enum_keys().flatten() {
            let Ok(key) = root.open_subkey_with_flags(&key_name, KEY_READ) else {
                continue;
            };
            let Ok(name) = key.get_value::<String, _>("DisplayName") else {
                continue;
            };
            // Skip components and updates that "Apps & features" hides as well.
            let system_component = key.get_value::<u32, _>("SystemComponent").unwrap_or(0) == 1;
            let is_update = key.get_value::<String, _>("ParentKeyName").is_ok();
            if name.trim().is_empty() || system_component || is_update {
                continue;
            }
            apps.push(InstalledApp {
                name: name.trim().to_string(),
                version: key.get_value("DisplayVersion").unwrap_or_default(),
                publisher: key.get_value("Publisher").unwrap_or_default(),
                install_location: key
                    .get_value::<String, _>("InstallLocation")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .map(PathBuf::from),
            });
        }
    }

    apps.sort_by_key(|a| a.name.to_lowercase());
    apps.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name) && a.version == b.version);
    apps
}

#[cfg(not(windows))]
pub fn installed_apps() -> Vec<InstalledApp> {
    Vec::new()
}

/// A neutral example list for the demo mode (see `known_paths::profile_override`).
#[cfg_attr(not(windows), allow(dead_code))]
fn demo_programs() -> Vec<InstalledApp> {
    [
        ("7-Zip 24.08 (x64)", "24.08", "Igor Pavlov"),
        ("Firefox", "143.0", "Mozilla"),
        ("LibreOffice 25.8", "25.8.1", "The Document Foundation"),
        ("Thunderbird", "143.0", "Mozilla"),
        ("VLC media player", "3.0.21", "VideoLAN"),
    ]
    .into_iter()
    .map(|(name, version, publisher)| InstalledApp {
        name: name.into(),
        version: version.into(),
        publisher: publisher.into(),
        install_location: None,
    })
    .collect()
}

/// Plain-text list of installed programs for the backup.
pub fn program_list_text(apps: &[InstalledApp]) -> String {
    let mut out = String::from(
        "Installed programs\r\n\
         ==================\r\n\
         Saved by AeternaVault as a checklist for reinstalling.\r\n\
         If 'winget-packages.json' is next to this file, most programs can be\r\n\
         reinstalled at once with:  winget import -i winget-packages.json\r\n\r\n",
    );
    for app in apps {
        out.push_str(&app.name);
        if !app.version.is_empty() {
            out.push_str(&format!("  ({})", app.version));
        }
        if !app.publisher.is_empty() {
            out.push_str(&format!("  — {}", app.publisher));
        }
        out.push_str("\r\n");
    }
    out
}

/// Runs `winget export` into `target`. Returns `false` if winget is not
/// available or fails; this is optional and never blocks a backup.
pub fn winget_export(target: &Path) -> bool {
    if super::known_paths::profile_override().is_some() {
        return false;
    }
    let mut command = std::process::Command::new("winget");
    command
        .args([
            "export",
            "--source",
            "winget",
            "--disable-interactivity",
            "-o",
        ])
        .arg(target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let Ok(mut child) = command.spawn() else {
        return false;
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success() && target.is_file(),
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            _ => {
                let _ = child.kill();
                return false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn builtin_catalog_is_consistent() {
        let catalog = Catalog::builtin();
        assert!(catalog.apps.len() >= 50);
        let mut ids = HashSet::new();
        let known = KnownPaths {
            values: KnownPaths::current().values,
        };
        for app in &catalog.apps {
            assert!(ids.insert(app.id.clone()), "duplicate id {}", app.id);
            assert!(
                !app.folders.is_empty() || !app.registry.is_empty(),
                "{} has nothing to back up",
                app.id
            );
            for folder in &app.folders {
                assert!(
                    folder.path.starts_with('{'),
                    "{}: path must use a token",
                    app.id
                );
                assert!(
                    known.resolve(&folder.path).is_some() || folder.path.contains("PROGRAMFILES")
                );
                for only in &folder.only {
                    assert!(
                        crate::engine::safe_relative_path(only).is_some(),
                        "{}: {only}",
                        app.id
                    );
                }
            }
            for key in &app.registry {
                assert!(
                    super::super::registry::hkcu_subkey(key).is_some(),
                    "{}: {key}",
                    app.id
                );
            }
        }
    }
}
