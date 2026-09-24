//! Turns the configured folders into the concrete list of things to back up.
//! Read-only.

use std::collections::HashSet;
use std::path::PathBuf;

use super::paths_equal;
use crate::config::Config;
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
    pub excludes: Vec<String>,
    pub include_paths: Vec<String>,
    pub exclude_paths: Vec<String>,
    pub encrypt: EncryptRule,
}

#[derive(Debug, Clone, Default)]
pub struct Sources {
    pub folders: Vec<BackupSource>,
}

impl Sources {
    pub fn is_empty(&self) -> bool {
        self.folders.is_empty()
    }

    /// Whether anything of this backup is going to be encrypted.
    pub fn may_encrypt(&self) -> bool {
        self.folders.iter().any(|f| f.encrypt.may_apply())
    }

    pub fn collect(config: &Config, known: &KnownPaths) -> Self {
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
        let mut sources = Sources::default();
        let mut used = HashSet::new();
        // Reserved for AeternaVault's own data inside a backup folder.
        used.insert(super::manifest::META_DIR.to_string());

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
                excludes: source.exclude.clone(),
                include_paths: source.include_paths.clone(),
                exclude_paths: source.exclude_paths.clone(),
                encrypt: folder_rule(source),
            });
        }
        sources
    }
}

/// A single folder name that is valid on Windows and Linux.
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
