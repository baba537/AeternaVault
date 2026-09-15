//! Turns the configuration (folders + chosen applications) into the concrete
//! list of things to back up. Read-only.

use std::collections::HashSet;
use std::path::PathBuf;

use super::manifest::APPS_DIR;
use super::paths_equal;
use crate::config::Config;
use crate::i18n::Lang;
use crate::platform::apps::Catalog;
use crate::platform::known_paths::KnownPaths;

/// Which files of a source are encrypted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncryptRule {
    Never,
    Always,
    /// Marked sub-folders and files (see [`super::selection::is_marked`]).
    Marked {
        marked: Vec<String>,
        unmarked: Vec<String>,
    },
}

impl EncryptRule {
    pub fn applies(&self, rel: &str) -> bool {
        match self {
            EncryptRule::Never => false,
            EncryptRule::Always => true,
            EncryptRule::Marked { marked, unmarked } => {
                super::selection::is_marked(marked, unmarked, rel)
            }
        }
    }

    pub fn may_apply(&self) -> bool {
        match self {
            EncryptRule::Never => false,
            EncryptRule::Always => true,
            EncryptRule::Marked { marked, .. } => !marked.is_empty(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BackupSource {
    /// Folder inside the snapshot, `/`-separated.
    pub key: String,
    pub name: String,
    pub root: PathBuf,
    pub portable: Option<String>,
    pub app: Option<String>,
    pub excludes: Vec<String>,
    pub include_paths: Vec<String>,
    pub exclude_paths: Vec<String>,
    pub encrypt: EncryptRule,
}

#[derive(Debug, Clone)]
pub struct RegistrySource {
    pub app: String,
    pub app_name: String,
    pub key: String,
    pub encrypted: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Sources {
    pub folders: Vec<BackupSource>,
    pub registry: Vec<RegistrySource>,
    /// Where the list of installed programs goes.
    pub program_list_encrypted: bool,
}

impl Sources {
    pub fn is_empty(&self) -> bool {
        self.folders.is_empty() && self.registry.is_empty()
    }

    /// Whether anything of this backup is going to be encrypted.
    pub fn may_encrypt(&self) -> bool {
        self.program_list_encrypted
            || self.folders.iter().any(|f| f.encrypt.may_apply())
            || self.registry.iter().any(|r| r.encrypted)
    }

    pub fn collect(config: &Config, catalog: &Catalog, known: &KnownPaths, lang: Lang) -> Self {
        let encryption = &config.encryption;
        let folder_rule = |source: &crate::config::Source| {
            if encryption.everything() {
                EncryptRule::Always
            } else if encryption.selected() && !source.encrypt_paths.is_empty() {
                EncryptRule::Marked {
                    marked: source.encrypt_paths.clone(),
                    unmarked: source.plain_paths.clone(),
                }
            } else {
                EncryptRule::Never
            }
        };
        let apps_encrypted =
            encryption.everything() || (encryption.selected() && encryption.applications);
        let mut sources = Sources {
            program_list_encrypted: encryption.everything(),
            ..Sources::default()
        };
        let mut used = HashSet::new();
        // Reserve the applications folder for application data.
        used.insert(APPS_DIR.to_lowercase());

        for source in config.enabled_sources() {
            if source.path.as_os_str().is_empty()
                || sources
                    .folders
                    .iter()
                    .any(|s| paths_equal(&s.root, &source.path))
            {
                continue;
            }
            sources.folders.push(BackupSource {
                key: unique_key(&sanitize_segment(&source.display_name()), &mut used),
                name: source.display_name(),
                root: source.path.clone(),
                portable: known.to_portable(&source.path),
                app: None,
                excludes: source.exclude.clone(),
                include_paths: source.include_paths.clone(),
                exclude_paths: source.exclude_paths.clone(),
                encrypt: folder_rule(source),
            });
        }

        for choice in config.apps.iter().filter(|c| c.enabled) {
            let Some(app) = catalog.get(&choice.id) else {
                continue;
            };
            let app_name = app.display_name(lang).to_string();
            let app_key = format!("{APPS_DIR}/{}", sanitize_segment(&app_name));
            let present = app.present_folders(known);
            let several = present.len() > 1;
            for (folder, root) in present {
                let label = folder.label.clone().or_else(|| {
                    several
                        .then(|| root.file_name().map(|n| n.to_string_lossy().into_owned()))
                        .flatten()
                });
                let key = match (&label, several) {
                    (Some(label), true) => format!("{app_key}/{}", sanitize_segment(label)),
                    _ => app_key.clone(),
                };
                let (include_paths, exclude_paths) = if folder.only.is_empty() {
                    (Vec::new(), Vec::new())
                } else {
                    (
                        folder.only.clone(),
                        vec![super::selection::ROOT.to_string()],
                    )
                };
                sources.folders.push(BackupSource {
                    key: unique_key(&key, &mut used),
                    name: match label {
                        Some(label) if several => format!("{app_name} · {label}"),
                        _ => app_name.clone(),
                    },
                    root,
                    portable: Some(folder.path.clone()),
                    app: Some(app.id.clone()),
                    excludes: folder.exclude.clone(),
                    include_paths,
                    exclude_paths,
                    encrypt: if apps_encrypted {
                        EncryptRule::Always
                    } else {
                        EncryptRule::Never
                    },
                });
            }
            for key in app.present_registry_keys() {
                sources.registry.push(RegistrySource {
                    app: app.id.clone(),
                    app_name: app_name.clone(),
                    key: key.to_string(),
                    encrypted: apps_encrypted,
                });
            }
        }
        sources
    }
}

/// A single folder name that is valid on Windows.
pub fn sanitize_segment(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect::<String>()
        .trim()
        .trim_end_matches('.')
        .to_string();
    if cleaned.is_empty() {
        "source".to_string()
    } else {
        cleaned
    }
}

/// Makes a `/`-separated key unique (case-insensitive) by appending ` (2)` …
pub fn unique_key(key: &str, used: &mut HashSet<String>) -> String {
    let mut candidate = key.to_string();
    let mut n = 2;
    while !used.insert(candidate.to_lowercase()) {
        candidate = format!("{key} ({n})");
        n += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_safe() {
        let mut used = HashSet::new();
        assert_eq!(unique_key("Documents", &mut used), "Documents");
        assert_eq!(unique_key("documents", &mut used), "documents (2)");
        assert_eq!(sanitize_segment("A:B/C"), "A_B_C");
        assert_eq!(sanitize_segment(".."), "source");
    }
}
