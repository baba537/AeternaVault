//! Portable paths: `{APPDATA}\Mozilla\Firefox` instead of
//! `C:\Users\anna\AppData\Roaming\Mozilla\Firefox`.
//!
//! Backups record the portable form next to the absolute path. When restoring
//! on another computer (or for another user name), the tokens are resolved
//! again, so application settings land in the right place.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Known locations, most specific first (so `{APPDATA}` wins over `{USERPROFILE}`).
const TOKENS: &[&str] = &[
    "LOCALLOW",
    "APPDATA",
    "LOCALAPPDATA",
    "SAVEDGAMES",
    "DOCUMENTS",
    "PICTURES",
    "MUSIC",
    "VIDEOS",
    "DESKTOP",
    "DOWNLOADS",
    "USERPROFILE",
    "PROGRAMFILESX86",
    "PROGRAMFILES",
];

/// The concrete values of all tokens on one computer.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct KnownPaths {
    pub values: Vec<(String, PathBuf)>,
}

/// Test and demo mode: `AETERNAVAULT_PROFILE_ROOT` points all user folders to
/// a fake profile and hides the real registry and program list, so the app
/// can be tried out (or photographed) without touching personal data.
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
            let values = vec![
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
                ("PROGRAMFILESX86", at(&["Program Files (x86)"])),
                ("PROGRAMFILES", at(&["Program Files"])),
            ];
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

        for token in TOKENS {
            let path: Option<PathBuf> = match *token {
                "APPDATA" => base.as_ref().map(|b| b.config_dir().to_path_buf()),
                "LOCALAPPDATA" => base.as_ref().map(|b| b.data_local_dir().to_path_buf()),
                "LOCALLOW" => base
                    .as_ref()
                    .map(|b| b.home_dir().join("AppData").join("LocalLow")),
                "SAVEDGAMES" => base.as_ref().map(|b| b.home_dir().join("Saved Games")),
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
                "USERPROFILE" => base.as_ref().map(|b| b.home_dir().to_path_buf()),
                "PROGRAMFILESX86" => env_path("ProgramFiles(x86)"),
                "PROGRAMFILES" => env_path("ProgramFiles"),
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
            base.join(tail.replace('/', "\\"))
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
                        format!("{{{token}}}\\{}", rest.display())
                    };
                    best = Some((depth, portable));
                }
            }
        }
        best.map(|(_, p)| p)
    }

    /// Rewrites a text value that contains a path of the old user profile so it
    /// points into the current profile (used for registry values).
    pub fn remap_text(&self, text: &str, from: &KnownPaths) -> Option<String> {
        let old = from.get("USERPROFILE")?.to_string_lossy().into_owned();
        let new = self.get("USERPROFILE")?.to_string_lossy().into_owned();
        if old.eq_ignore_ascii_case(&new) || old.len() < 4 {
            return None;
        }
        let lower = text.to_lowercase();
        let needle = old.to_lowercase();
        // Byte offsets are shared between `text` and `lower`; only safe if
        // lower-casing did not change any lengths.
        if lower.len() != text.len() || needle.len() != old.len() || !lower.contains(&needle) {
            return None;
        }
        let mut result = String::with_capacity(text.len());
        let mut index = 0;
        while let Some(found) = lower[index..].find(&needle) {
            result.push_str(&text[index..index + found]);
            result.push_str(&new);
            index += found + needle.len();
        }
        result.push_str(&text[index..]);
        Some(result)
    }
}

fn strip_prefix_ignore_case(path: &Path, base: &Path) -> Option<PathBuf> {
    let mut path_components = path.components();
    for base_component in base.components() {
        let component = path_components.next()?;
        if !component
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(&base_component.as_os_str().to_string_lossy())
        {
            return None;
        }
    }
    Some(path_components.as_path().to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn remaps_profile_paths_in_text() {
        let anna = sample(r"C:\Users\anna");
        let ben = sample(r"C:\Users\ben");
        assert_eq!(
            ben.remap_text(r"C:\USERS\ANNA\AppData\x.ttf", &anna)
                .unwrap(),
            r"C:\Users\ben\AppData\x.ttf"
        );
        assert_eq!(ben.remap_text("no path", &anna), None);
    }
}
