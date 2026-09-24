//! Portable paths: `{DOCUMENTS}\Letters` instead of
//! `C:\Users\anna\Documents\Letters`.
//!
//! Backups record the portable form next to the absolute path. When restoring
//! on another computer (or for another user name, or on the other operating
//! system), the tokens are resolved again, so folders land in the right place.
//! `{HOME}` and `{USERPROFILE}` both mean the user's home folder.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Known locations, most specific first (so `{APPDATA}` wins over `{USERPROFILE}`).
const TOKENS: &[&str] = &[
    "LOCALLOW",
    "APPDATA",
    "LOCALAPPDATA",
    "CONFIG",
    "DATA",
    "SAVEDGAMES",
    "DOCUMENTS",
    "PICTURES",
    "MUSIC",
    "VIDEOS",
    "DESKTOP",
    "DOWNLOADS",
    "USERPROFILE",
    "HOME",
    "PROGRAMFILESX86",
    "PROGRAMFILES",
];

/// The concrete values of all tokens on one computer.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct KnownPaths {
    pub values: Vec<(String, PathBuf)>,
}

/// Test and demo mode: `AETERNAVAULT_PROFILE_ROOT` points all user folders to
/// a fake profile, so the app can be tried out (or photographed) without
/// touching personal data or changing any system settings.
pub fn profile_override() -> Option<PathBuf> {
    std::env::var_os("AETERNAVAULT_PROFILE_ROOT")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

impl KnownPaths {
    /// Known folders of the current user on this computer.
    pub fn current() -> Self {
        if let Some(root) = profile_override() {
            let at = |parts: &[&str]| parts.iter().fold(root.clone(), |acc, p| acc.join(p));
            let values = if cfg!(windows) {
                vec![
                    ("LOCALLOW", at(&["AppData", "LocalLow"])),
                    ("APPDATA", at(&["AppData", "Roaming"])),
                    ("LOCALAPPDATA", at(&["AppData", "Local"])),
                    ("SAVEDGAMES", at(&["Saved Games"])),
                    ("DOCUMENTS", at(&["Documents"])),
                    ("PICTURES", at(&["Pictures"])),
                    ("MUSIC", at(&["Music"])),
                    ("VIDEOS", at(&["Videos"])),
                    ("DESKTOP", at(&["Desktop"])),
                    ("DOWNLOADS", at(&["Downloads"])),
                    ("USERPROFILE", root.clone()),
                    ("HOME", root.clone()),
                    ("PROGRAMFILESX86", at(&["Program Files (x86)"])),
                    ("PROGRAMFILES", at(&["Program Files"])),
                ]
            } else {
                vec![
                    ("CONFIG", at(&[".config"])),
                    ("DATA", at(&[".local", "share"])),
                    ("DOCUMENTS", at(&["Documents"])),
                    ("PICTURES", at(&["Pictures"])),
                    ("MUSIC", at(&["Music"])),
                    ("VIDEOS", at(&["Videos"])),
                    ("DESKTOP", at(&["Desktop"])),
                    ("DOWNLOADS", at(&["Downloads"])),
                    ("HOME", root.clone()),
                    ("USERPROFILE", root.clone()),
                ]
            };
            return Self {
                values: values
                    .into_iter()
                    .map(|(t, p)| (t.to_string(), p))
                    .collect(),
            };
        }

        let mut values = Vec::new();
        let base = directories::BaseDirs::new();
        let user = directories::UserDirs::new();
        let env_path = |name: &str| std::env::var_os(name).map(PathBuf::from);
        let home = base.as_ref().map(|b| b.home_dir().to_path_buf());

        for token in TOKENS {
            let path: Option<PathBuf> = match *token {
                "APPDATA" if cfg!(windows) => base.as_ref().map(|b| b.config_dir().to_path_buf()),
                "LOCALAPPDATA" if cfg!(windows) => {
                    base.as_ref().map(|b| b.data_local_dir().to_path_buf())
                }
                "LOCALLOW" if cfg!(windows) => {
                    home.as_ref().map(|h| h.join("AppData").join("LocalLow"))
                }
                "SAVEDGAMES" if cfg!(windows) => home.as_ref().map(|h| h.join("Saved Games")),
                "PROGRAMFILESX86" if cfg!(windows) => env_path("ProgramFiles(x86)"),
                "PROGRAMFILES" if cfg!(windows) => env_path("ProgramFiles"),
                "CONFIG" if !cfg!(windows) => base.as_ref().map(|b| b.config_dir().to_path_buf()),
                "DATA" if !cfg!(windows) => base.as_ref().map(|b| b.data_dir().to_path_buf()),
                "DOCUMENTS" => user
                    .as_ref()
                    .and_then(|u| u.document_dir().map(Path::to_path_buf)),
                "PICTURES" => user
                    .as_ref()
                    .and_then(|u| u.picture_dir().map(Path::to_path_buf)),
                "MUSIC" => user
                    .as_ref()
                    .and_then(|u| u.audio_dir().map(Path::to_path_buf)),
                "VIDEOS" => user
                    .as_ref()
                    .and_then(|u| u.video_dir().map(Path::to_path_buf)),
                "DESKTOP" => user
                    .as_ref()
                    .and_then(|u| u.desktop_dir().map(Path::to_path_buf)),
                "DOWNLOADS" => user
                    .as_ref()
                    .and_then(|u| u.download_dir().map(Path::to_path_buf)),
                "USERPROFILE" | "HOME" => home.clone(),
                _ => None,
            };
            if let Some(path) = path {
                values.push((token.to_string(), path));
            }
        }
        Self { values }
    }

    pub fn get(&self, token: &str) -> Option<&Path> {
        self.values
            .iter()
            .find(|(t, _)| t.eq_ignore_ascii_case(token))
            .map(|(_, p)| p.as_path())
    }

    /// Replaces `{TOKEN}` prefixes. Returns `None` if a token is unknown.
    pub fn resolve(&self, portable: &str) -> Option<PathBuf> {
        let Some(rest) = portable.strip_prefix('{') else {
            return Some(PathBuf::from(portable));
        };
        let (token, tail) = rest.split_once('}')?;
        let base = self.get(token)?;
        let tail = tail.trim_start_matches(['\\', '/']);
        Some(if tail.is_empty() {
            base.to_path_buf()
        } else {
            base.join(tail.replace(['/', '\\'], std::path::MAIN_SEPARATOR_STR))
        })
    }

    /// The most specific portable form of an absolute path, if it lies in a
    /// known folder.
    pub fn to_portable(&self, path: &Path) -> Option<String> {
        let mut best: Option<(usize, String)> = None;
        for (token, base) in &self.values {
            if let Some(rest) = strip_prefix_ignore_case(path, base) {
                let depth = base.components().count();
                if best.as_ref().is_none_or(|(d, _)| depth > *d) {
                    let portable = if rest.as_os_str().is_empty() {
                        format!("{{{token}}}")
                    } else {
                        format!("{{{token}}}{}{}", std::path::MAIN_SEPARATOR, rest.display())
                    };
                    best = Some((depth, portable));
                }
            }
        }
        best.map(|(_, p)| p)
    }
}

fn strip_prefix_ignore_case(path: &Path, base: &Path) -> Option<PathBuf> {
    let mut path_components = path.components();
    for base_component in base.components() {
        let component = path_components.next()?;
        let (a, b) = (
            component.as_os_str().to_string_lossy(),
            base_component.as_os_str().to_string_lossy(),
        );
        let same = if cfg!(windows) {
            a.eq_ignore_ascii_case(&b)
        } else {
            a == b
        };
        if !same {
            return None;
        }
    }
    Some(path_components.as_path().to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    fn sample(profile: &str) -> KnownPaths {
        KnownPaths {
            values: vec![
                (
                    "APPDATA".into(),
                    PathBuf::from(format!(r"{profile}\AppData\Roaming")),
                ),
                ("USERPROFILE".into(), PathBuf::from(profile)),
            ],
        }
    }

    #[cfg(windows)]
    #[test]
    fn portable_roundtrip_across_users() {
        let anna = sample(r"C:\Users\anna");
        let ben = sample(r"D:\Profiles\ben");
        let portable = anna
            .to_portable(Path::new(r"C:\Users\Anna\AppData\Roaming\Mozilla\Firefox"))
            .unwrap();
        assert_eq!(portable, r"{APPDATA}\Mozilla\Firefox");
        assert_eq!(
            ben.resolve(&portable).unwrap(),
            PathBuf::from(r"D:\Profiles\ben\AppData\Roaming\Mozilla\Firefox")
        );
        assert_eq!(anna.to_portable(Path::new(r"E:\Other")), None);
    }

    #[cfg(unix)]
    #[test]
    fn portable_roundtrip_across_users() {
        let anna = KnownPaths {
            values: vec![("HOME".into(), PathBuf::from("/home/anna"))],
        };
        let ben = KnownPaths {
            values: vec![("HOME".into(), PathBuf::from("/home/ben"))],
        };
        let portable = anna
            .to_portable(Path::new("/home/anna/Documents/Letters"))
            .unwrap();
        assert_eq!(portable, "{HOME}/Documents/Letters");
        assert_eq!(
            ben.resolve(&portable).unwrap(),
            PathBuf::from("/home/ben/Documents/Letters")
        );
        // Paths written on Windows resolve as well.
        assert_eq!(
            ben.resolve(r"{HOME}\.ssh").unwrap(),
            PathBuf::from("/home/ben/.ssh")
        );
    }
}
