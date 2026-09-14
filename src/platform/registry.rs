//! Backing up and restoring application settings stored in the registry.
//!
//! Only `HKEY_CURRENT_USER` is supported on purpose: those settings belong to
//! the user, can be written without administrator rights, and cannot damage
//! the system as a whole.
//!
//! Exports are stored twice: as JSON inside the backup index (exact bytes,
//! used for restore and comparison) and as a standard `.reg` file that can be
//! opened with Regedit even without AeternaVault.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::known_paths::KnownPaths;

pub const HKCU_PREFIX: &str = "HKCU\\";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryValue {
    pub name: String,
    /// Win32 value type (REG_SZ = 1, REG_DWORD = 4, …).
    pub kind: u32,
    /// Raw bytes as lower-case hex.
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryKey {
    /// Path relative to the exported root key ("" for the root itself).
    pub path: String,
    pub values: Vec<RegistryValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryExport {
    /// Full key, e.g. `HKCU\Software\7-Zip`.
    pub root: String,
    pub keys: Vec<RegistryKey>,
}

impl RegistryExport {
    pub fn value_count(&self) -> usize {
        self.keys.iter().map(|k| k.values.len()).sum()
    }

    /// Stable fingerprint to compare two exports.
    pub fn fingerprint(&self) -> String {
        let json = serde_json::to_vec(self).unwrap_or_default();
        crate::engine::format_sha256(&Sha256::digest(&json))
    }

    /// Standard `REGEDIT5` text (UTF-16 LE with BOM when written by the caller).
    pub fn to_reg_text(&self) -> String {
        let mut out = String::from("Windows Registry Editor Version 5.00\r\n");
        let root = self.root.replacen(HKCU_PREFIX, "HKEY_CURRENT_USER\\", 1);
        for key in &self.keys {
            out.push_str("\r\n[");
            out.push_str(&root);
            if !key.path.is_empty() {
                out.push('\\');
                out.push_str(&key.path);
            }
            out.push_str("]\r\n");
            for value in &key.values {
                let name = if value.name.is_empty() {
                    "@".to_string()
                } else {
                    format!("\"{}\"", escape_reg(&value.name))
                };
                let bytes = hex_decode(&value.data).unwrap_or_default();
                let data = match value.kind {
                    1 => format!("\"{}\"", escape_reg(&utf16_to_string(&bytes))),
                    4 if bytes.len() == 4 => format!(
                        "dword:{:08x}",
                        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
                    ),
                    3 => format!("hex:{}", hex_list(&bytes)),
                    kind => format!("hex({kind:x}):{}", hex_list(&bytes)),
                };
                out.push_str(&format!("{name}={data}\r\n"));
            }
        }
        out
    }
}

fn escape_reg(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn hex_list(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn utf16_to_string(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

fn string_to_utf16(text: &str, multi: bool) -> Vec<u8> {
    let mut out: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    out.extend_from_slice(&[0, 0]);
    if multi {
        out.extend_from_slice(&[0, 0]);
    }
    out
}

pub fn hex_encode(bytes: &[u8]) -> String {
    crate::engine::format_sha256(bytes)
}

pub fn hex_decode(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

/// Validates and splits `HKCU\Software\X` into `Software\X`.
pub fn hkcu_subkey(full: &str) -> Option<&str> {
    let lower = full.to_ascii_lowercase();
    let rest = if lower.starts_with("hkcu\\") {
        &full[5..]
    } else if lower.starts_with("hkey_current_user\\") {
        &full[18..]
    } else {
        return None;
    };
    let rest = rest.trim_matches('\\');
    (!rest.is_empty() && !rest.split('\\').any(|p| p.is_empty() || p == "..")).then_some(rest)
}

/// Rewrites user-profile paths inside string values for another computer.
pub fn remap_export(export: &RegistryExport, from: &KnownPaths, to: &KnownPaths) -> RegistryExport {
    let mut result = export.clone();
    for key in &mut result.keys {
        for value in &mut key.values {
            if !matches!(value.kind, 1 | 2 | 7) {
                continue;
            }
            let Some(bytes) = hex_decode(&value.data) else {
                continue;
            };
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            let text = String::from_utf16_lossy(&units);
            let trimmed = text.trim_end_matches('\0');
            let multi = value.kind == 7;
            if let Some(remapped) = to.remap_text(trimmed, from) {
                let bytes = if multi {
                    let mut out: Vec<u8> = remapped
                        .encode_utf16()
                        .flat_map(|u| u.to_le_bytes())
                        .collect();
                    out.extend_from_slice(&[0, 0, 0, 0]);
                    out
                } else {
                    string_to_utf16(&remapped, false)
                };
                value.data = hex_encode(&bytes);
            }
        }
    }
    result
}

#[cfg(windows)]
mod imp {
    use std::borrow::Cow;
    use std::io;

    use winreg::enums::*;
    use winreg::{RegKey, RegValue};

    use super::*;

    fn kind_to_type(kind: u32) -> RegType {
        match kind {
            1 => REG_SZ,
            2 => REG_EXPAND_SZ,
            3 => REG_BINARY,
            4 => REG_DWORD,
            5 => REG_DWORD_BIG_ENDIAN,
            6 => REG_LINK,
            7 => REG_MULTI_SZ,
            8 => REG_RESOURCE_LIST,
            9 => REG_FULL_RESOURCE_DESCRIPTOR,
            10 => REG_RESOURCE_REQUIREMENTS_LIST,
            11 => REG_QWORD,
            _ => REG_NONE,
        }
    }

    pub fn key_exists(full_key: &str) -> bool {
        if super::super::known_paths::profile_override().is_some() {
            return false;
        }
        hkcu_subkey(full_key).is_some_and(|subkey| {
            RegKey::predef(HKEY_CURRENT_USER)
                .open_subkey_with_flags(subkey, KEY_READ)
                .is_ok()
        })
    }

    /// Reads a key and all subkeys. Returns `Ok(None)` if the key does not exist.
    pub fn export(full_key: &str) -> io::Result<Option<RegistryExport>> {
        let subkey = hkcu_subkey(full_key).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "only HKCU keys are supported")
        })?;
        let root = match RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(subkey, KEY_READ)
        {
            Ok(key) => key,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };
        let mut keys = Vec::new();
        collect(&root, String::new(), &mut keys, 0)?;
        Ok(Some(RegistryExport {
            root: format!("{HKCU_PREFIX}{subkey}"),
            keys,
        }))
    }

    fn collect(
        key: &RegKey,
        path: String,
        out: &mut Vec<RegistryKey>,
        depth: usize,
    ) -> io::Result<()> {
        if depth > 32 {
            return Ok(());
        }
        let mut values: Vec<RegistryValue> = key
            .enum_values()
            .flatten()
            .map(|(name, value)| RegistryValue {
                name,
                kind: value.vtype as u32,
                data: hex_encode(&value.bytes),
            })
            .collect();
        values.sort_by_key(|v| v.name.to_lowercase());
        out.push(RegistryKey {
            path: path.clone(),
            values,
        });

        let mut children: Vec<String> = key.enum_keys().flatten().collect();
        children.sort_by_key(|c| c.to_lowercase());
        for child in children {
            if let Ok(sub) = key.open_subkey_with_flags(&child, KEY_READ) {
                let child_path = if path.is_empty() {
                    child
                } else {
                    format!("{path}\\{child}")
                };
                collect(&sub, child_path, out, depth + 1)?;
            }
        }
        Ok(())
    }

    /// Writes all keys and values. Existing values with the same name are
    /// replaced; other existing values are left untouched.
    pub fn import(export: &RegistryExport) -> io::Result<usize> {
        let subkey = hkcu_subkey(&export.root).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "only HKCU keys are supported")
        })?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let mut written = 0;
        for key in &export.keys {
            if key.path.split('\\').any(|p| p == "..") {
                continue;
            }
            let full = if key.path.is_empty() {
                subkey.to_string()
            } else {
                format!("{subkey}\\{}", key.path)
            };
            let (target, _) = hkcu.create_subkey(&full)?;
            for value in &key.values {
                let bytes = hex_decode(&value.data).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid value data")
                })?;
                target.set_raw_value(
                    &value.name,
                    &RegValue {
                        bytes: Cow::Owned(bytes),
                        vtype: kind_to_type(value.kind),
                    },
                )?;
                written += 1;
            }
        }
        Ok(written)
    }

    #[cfg(test)]
    pub fn delete_test_key(subkey: &str) {
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(subkey);
    }
}

#[cfg(windows)]
pub use imp::{export, import, key_exists};

#[cfg(not(windows))]
pub fn key_exists(_full_key: &str) -> bool {
    false
}

#[cfg(not(windows))]
pub fn export(_full_key: &str) -> std::io::Result<Option<RegistryExport>> {
    Ok(None)
}

#[cfg(not(windows))]
pub fn import(_export: &RegistryExport) -> std::io::Result<usize> {
    Err(std::io::Error::other(
        "the registry is only available on Windows",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_hkcu_keys() {
        assert_eq!(hkcu_subkey(r"HKCU\Software\7-Zip"), Some(r"Software\7-Zip"));
        assert_eq!(
            hkcu_subkey(r"HKEY_CURRENT_USER\Software\X\"),
            Some(r"Software\X")
        );
        assert_eq!(hkcu_subkey(r"HKLM\Software\X"), None);
        assert_eq!(hkcu_subkey(r"HKCU\Software\..\X"), None);
    }

    #[test]
    fn reg_text_format() {
        let export = RegistryExport {
            root: r"HKCU\Software\Test".into(),
            keys: vec![RegistryKey {
                path: String::new(),
                values: vec![
                    RegistryValue {
                        name: "Path".into(),
                        kind: 1,
                        data: hex_encode(&string_to_utf16(r"C:\a", false)),
                    },
                    RegistryValue {
                        name: "Count".into(),
                        kind: 4,
                        data: hex_encode(&7u32.to_le_bytes()),
                    },
                ],
            }],
        };
        let text = export.to_reg_text();
        assert!(text.contains(r"[HKEY_CURRENT_USER\Software\Test]"));
        assert!(text.contains(r#""Path"="C:\\a""#));
        assert!(text.contains(r#""Count"=dword:00000007"#));
    }

    #[cfg(windows)]
    #[test]
    fn export_and_import_roundtrip() {
        let unique = format!(r"Software\AeternaVaultTest-{}", std::process::id());
        let full = format!(r"HKCU\{unique}");
        let export = RegistryExport {
            root: full.clone(),
            keys: vec![
                RegistryKey {
                    path: String::new(),
                    values: vec![RegistryValue {
                        name: "Greeting".into(),
                        kind: 1,
                        data: hex_encode(&string_to_utf16("hello", false)),
                    }],
                },
                RegistryKey {
                    path: "Nested".into(),
                    values: vec![RegistryValue {
                        name: "Number".into(),
                        kind: 4,
                        data: hex_encode(&42u32.to_le_bytes()),
                    }],
                },
            ],
        };
        let written = import(&export).unwrap();
        let read_back = super::export(&full).unwrap().unwrap();
        imp::delete_test_key(&unique);
        assert_eq!(written, 2);
        assert_eq!(read_back, export);
        assert!(super::export(&full).unwrap().is_none());
    }
}
