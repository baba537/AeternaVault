//! Catalog of applications whose data is worth keeping.
//!
//! Each entry names the folders an application keeps its user data in, per
//! operating system, with portable tokens (`{APPDATA}`, `{HOME}` …). An
//! application counts as found only if one of these folders exists. Choosing
//! it adds the folder to the list of folders to back up, so exactly what is
//! kept stays visible and can be adjusted like any other folder.
//!
//! The built-in catalog (`apps.toml`) can be extended or overridden with an
//! `apps.toml` next to the configuration file.

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
    Documents,
    Development,
    Gaming,
    Media,
}

impl Category {
    pub const ALL: [Category; 6] = [
        Category::Browsers,
        Category::Email,
        Category::Documents,
        Category::Development,
        Category::Gaming,
        Category::Media,
    ];
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub name_de: Option<String>,
    pub category: Category,
    #[serde(default)]
    pub note_en: Option<String>,
    #[serde(default)]
    pub note_de: Option<String>,
    /// Portable folder paths on Windows.
    #[serde(default)]
    pub windows: Vec<String>,
    /// Portable folder paths on Linux.
    #[serde(default)]
    pub linux: Vec<String>,
    /// Exclude patterns added to the folder (caches, lock files).
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Suggest encrypting it (keys, mail).
    #[serde(default)]
    pub sensitive: bool,
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

    /// The portable paths for this operating system.
    pub fn paths(&self) -> &[String] {
        if cfg!(windows) {
            &self.windows
        } else {
            &self.linux
        }
    }

    /// Folders of this application that exist on this computer.
    pub fn present_folders(&self, known: &KnownPaths) -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = Vec::new();
        for portable in self.paths() {
            if let Some(path) = known.resolve(portable)
                && path.is_dir()
                && !found.iter().any(|f| crate::engine::paths_equal(f, &path))
            {
                found.push(path);
            }
        }
        found
    }

    pub fn is_detected(&self, known: &KnownPaths) -> bool {
        !self.present_folders(known).is_empty()
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

    /// Applications found on this computer, with their folders.
    pub fn detected(&self, known: &KnownPaths) -> Vec<(&AppDef, Vec<PathBuf>)> {
        self.apps
            .iter()
            .filter_map(|app| {
                let folders = app.present_folders(known);
                (!folders.is_empty()).then_some((app, folders))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_is_valid_and_has_paths_for_both_systems() {
        let catalog = Catalog::builtin();
        assert!(catalog.apps.len() >= 10);
        let mut ids = std::collections::HashSet::new();
        for app in &catalog.apps {
            assert!(ids.insert(app.id.clone()), "duplicate id {}", app.id);
            assert!(
                !app.windows.is_empty() || !app.linux.is_empty(),
                "{} has no paths",
                app.id
            );
            for path in app.windows.iter().chain(&app.linux) {
                assert!(path.starts_with('{'), "{}: {path} is not portable", app.id);
            }
        }
    }

    #[test]
    fn detects_existing_folders_only() {
        let tmp = tempfile::tempdir().unwrap();
        let known = KnownPaths {
            values: vec![
                ("APPDATA".into(), tmp.path().join("Roaming")),
                ("HOME".into(), tmp.path().to_path_buf()),
            ],
        };
        let app = AppDef {
            id: "x".into(),
            name: "X".into(),
            name_de: None,
            category: Category::Media,
            note_en: None,
            note_de: None,
            windows: vec!["{APPDATA}\\X".into(), "{APPDATA}\\Missing".into()],
            linux: vec!["{HOME}/.x".into(), "{HOME}/.missing".into()],
            exclude: Vec::new(),
            sensitive: false,
        };
        assert!(!app.is_detected(&known));
        std::fs::create_dir_all(tmp.path().join("Roaming").join("X")).unwrap();
        std::fs::create_dir_all(tmp.path().join(".x")).unwrap();
        assert_eq!(app.present_folders(&known).len(), 1);
    }
}
