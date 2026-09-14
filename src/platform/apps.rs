//! Application discovery (read-only).
//!
//! Two sources of information:
//! * A small catalog of well-known application data folders that are worth
//!   backing up (browser profiles, mail, editor settings). Only entries that
//!   exist on this computer are returned.
//! * Installed desktop applications from the `Uninstall` registry keys
//!   (HKLM, HKLM\WOW6432Node, HKCU). This is the same data "Apps & features"
//!   shows. Microsoft Store apps are not listed yet (see docs/ROADMAP.md).

use std::path::PathBuf;

use crate::i18n::Lang;

#[derive(Debug, Clone)]
pub struct KnownProfile {
    pub name: String,
    pub path: PathBuf,
    pub exclude: Vec<String>,
    /// Added (disabled) to the source list on first start.
    pub suggest_on_first_run: bool,
}

enum Base {
    RoamingAppData,
    LocalAppData,
}

struct CatalogEntry {
    /// English and German display name.
    name: [&'static str; 2],
    base: Base,
    relative: &'static str,
    exclude: &'static [&'static str],
    suggest_on_first_run: bool,
}

/// Extend this list to teach AeternaVault about more applications.
const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        name: ["Firefox profile", "Firefox-Profil"],
        base: Base::RoamingAppData,
        relative: r"Mozilla\Firefox",
        exclude: &[
            "Crash Reports",
            "Pending Pings",
            "datareporting",
            "parent.lock",
        ],
        suggest_on_first_run: true,
    },
    CatalogEntry {
        name: ["Thunderbird profile", "Thunderbird-Profil"],
        base: Base::RoamingAppData,
        relative: "Thunderbird",
        exclude: &["Crash Reports", "Pending Pings", "parent.lock"],
        suggest_on_first_run: true,
    },
    CatalogEntry {
        name: ["Google Chrome profile", "Google-Chrome-Profil"],
        base: Base::LocalAppData,
        relative: r"Google\Chrome\User Data",
        exclude: CHROMIUM_CACHES,
        suggest_on_first_run: false,
    },
    CatalogEntry {
        name: ["Microsoft Edge profile", "Microsoft-Edge-Profil"],
        base: Base::LocalAppData,
        relative: r"Microsoft\Edge\User Data",
        exclude: CHROMIUM_CACHES,
        suggest_on_first_run: false,
    },
    CatalogEntry {
        name: [
            "Visual Studio Code settings",
            "Visual-Studio-Code-Einstellungen",
        ],
        base: Base::RoamingAppData,
        relative: r"Code\User",
        exclude: &["workspaceStorage", "History"],
        suggest_on_first_run: false,
    },
    CatalogEntry {
        name: ["Notepad++ settings", "Notepad++-Einstellungen"],
        base: Base::RoamingAppData,
        relative: "Notepad++",
        exclude: &["backup"],
        suggest_on_first_run: false,
    },
    CatalogEntry {
        name: ["Outlook signatures", "Outlook-Signaturen"],
        base: Base::RoamingAppData,
        relative: r"Microsoft\Signatures",
        exclude: &[],
        suggest_on_first_run: false,
    },
    CatalogEntry {
        name: [
            "Windows Terminal settings",
            "Windows-Terminal-Einstellungen",
        ],
        base: Base::LocalAppData,
        relative: r"Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState",
        exclude: &[],
        suggest_on_first_run: false,
    },
];

const CHROMIUM_CACHES: &[&str] = &[
    "Cache",
    "Code Cache",
    "GPUCache",
    "DawnCache",
    "GrShaderCache",
    "ShaderCache",
    "Service Worker",
    "Crashpad",
    "*.log",
];

pub fn known_profiles(lang: Lang) -> Vec<KnownProfile> {
    let Some(base) = directories::BaseDirs::new() else {
        return Vec::new();
    };
    CATALOG
        .iter()
        .filter_map(|entry| {
            let root = match entry.base {
                Base::RoamingAppData => base.config_dir(),
                Base::LocalAppData => base.data_local_dir(),
            };
            let path = root.join(entry.relative);
            path.is_dir().then(|| KnownProfile {
                name: match lang {
                    Lang::En => entry.name[0],
                    Lang::De => entry.name[1],
                }
                .to_string(),
                path,
                exclude: entry.exclude.iter().map(|s| s.to_string()).collect(),
                suggest_on_first_run: entry.suggest_on_first_run,
            })
        })
        .collect()
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
