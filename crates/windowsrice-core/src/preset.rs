//! Design presets (Design Studio): a named snapshot of the theme selection
//! and the `[bar]` section, stored as a versioned TOML file.
//!
//! ```toml
//! format = "windowsrice-preset"
//! version = 2
//! name = "Evening"
//!
//! [theme]
//! active = "windowsrice-glass"
//! mode = "dark"
//!
//! [bar]
//! # any [bar] content of config.toml (schema v3)
//! ```
//!
//! Imported files are untrusted: size-limited, strictly typed (unknown keys
//! are rejected), and validated with the same rules as `config.toml`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::bar::BarConfig;
use crate::config::{ThemeSelection, is_valid_theme_id};

pub const PRESET_FORMAT: &str = "windowsrice-preset";
/// Version 2 (Phase 8) adds the visual design fields of `[bar.style]`;
/// version 1 files load unchanged.
pub const PRESET_VERSION: u32 = 2;
/// Largest accepted preset file.
pub const MAX_PRESET_BYTES: usize = 128 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub theme: ThemeSelection,
    pub bar: BarConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PresetError {
    #[error("the file is larger than {MAX_PRESET_BYTES} bytes")]
    TooLarge,
    #[error("not a windowsRICE preset: {0}")]
    Parse(String),
    #[error("not a windowsRICE preset (format is {0:?})")]
    WrongFormat(String),
    #[error("preset version {0} is newer than this build supports ({PRESET_VERSION})")]
    TooNew(u32),
    #[error("the preset is invalid: {0}")]
    Invalid(String),
    #[error("{0}")]
    Io(String),
}

impl Preset {
    pub fn new(name: &str, theme: ThemeSelection, bar: BarConfig) -> Self {
        Self {
            format: PRESET_FORMAT.into(),
            version: PRESET_VERSION,
            name: name.trim().to_owned(),
            theme,
            bar,
        }
    }

    pub fn validate(&self) -> Result<(), PresetError> {
        if self.format != PRESET_FORMAT {
            return Err(PresetError::WrongFormat(self.format.clone()));
        }
        if self.version > PRESET_VERSION {
            return Err(PresetError::TooNew(self.version));
        }
        if self.version == 0 {
            return Err(PresetError::Invalid("version must be at least 1".into()));
        }
        let n = self.name.trim().chars().count();
        if !(1..=48).contains(&n) || self.name.chars().any(char::is_control) {
            return Err(PresetError::Invalid(
                "name must be 1-48 printable characters".into(),
            ));
        }
        if !is_valid_theme_id(&self.theme.active) {
            return Err(PresetError::Invalid(format!(
                "theme {:?} is not a valid theme id",
                self.theme.active
            )));
        }
        let p = self.bar.problems();
        if !p.is_empty() {
            return Err(PresetError::Invalid(p.join("; ")));
        }
        Ok(())
    }

    pub fn from_toml(text: &str) -> Result<Self, PresetError> {
        if text.len() > MAX_PRESET_BYTES {
            return Err(PresetError::TooLarge);
        }
        // Check the header first so other TOML files get a clear message.
        let table: toml::Table =
            toml::from_str(text).map_err(|e| PresetError::Parse(e.message().to_owned()))?;
        match table.get("format").and_then(toml::Value::as_str) {
            Some(PRESET_FORMAT) => {}
            Some(other) => return Err(PresetError::WrongFormat(other.to_owned())),
            None => {
                return Err(PresetError::Parse(
                    "missing format = \"windowsrice-preset\"".into(),
                ));
            }
        }
        if let Some(v) = table.get("version").and_then(toml::Value::as_integer)
            && v > i64::from(PRESET_VERSION)
        {
            return Err(PresetError::TooNew(u32::try_from(v).unwrap_or(u32::MAX)));
        }
        let p: Preset = toml::Value::Table(table)
            .try_into()
            .map_err(|e: toml::de::Error| PresetError::Parse(e.message().to_owned()))?;
        p.validate()?;
        Ok(p)
    }

    pub fn to_toml(&self) -> Result<String, PresetError> {
        self.validate()?;
        toml::to_string_pretty(self).map_err(|e| PresetError::Invalid(e.to_string()))
    }

    pub fn load(path: &Path) -> Result<Self, PresetError> {
        let meta = std::fs::metadata(path)
            .map_err(|e| PresetError::Io(format!("{}: {e}", path.display())))?;
        if meta.len() > MAX_PRESET_BYTES as u64 {
            return Err(PresetError::TooLarge);
        }
        let text = std::fs::read_to_string(path)
            .map_err(|e| PresetError::Io(format!("{}: {e}", path.display())))?;
        Self::from_toml(&text)
    }

    /// Writes atomically (temporary file, then rename).
    pub fn save(&self, path: &Path) -> Result<(), PresetError> {
        let text = self.to_toml()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| PresetError::Io(e.to_string()))?;
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text).map_err(|e| PresetError::Io(e.to_string()))?;
        std::fs::rename(&tmp, path).map_err(|e| PresetError::Io(e.to_string()))
    }
}

/// File name for a preset name: lowercase ASCII letters and digits, other
/// runs become one hyphen; non-ASCII names get a stable hash suffix.
pub fn preset_file_name(name: &str) -> String {
    let mut slug = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-').to_owned();
    let mut h: u32 = 0x811C_9DC5;
    for b in name.trim().bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    if slug.is_empty() {
        format!("preset-{h:08x}.toml")
    } else if name.trim().is_ascii() {
        format!("{slug}.toml")
    } else {
        format!("{slug}-{h:08x}.toml")
    }
}

/// Presets in a directory, sorted by name. Unreadable or invalid files are
/// returned as errors next to their path, never silently dropped.
pub fn list_presets(dir: &Path) -> Vec<(PathBuf, Result<Preset, PresetError>)> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(PathBuf, Result<Preset, PresetError>)> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .take(200)
        .map(|p| {
            let r = Preset::load(&p);
            (p, r)
        })
        .collect();
    out.sort_by(|a, b| {
        let key = |x: &(PathBuf, Result<Preset, PresetError>)| match &x.1 {
            Ok(p) => p.name.to_lowercase(),
            Err(_) => x.0.display().to_string(),
        };
        key(a).cmp(&key(b))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Preset {
        Preset::new(
            "Evening glass",
            ThemeSelection::default(),
            BarConfig::default(),
        )
    }

    #[test]
    fn round_trip() {
        let p = sample();
        let t = p.to_toml().unwrap();
        assert!(t.starts_with("format = \"windowsrice-preset\""));
        assert_eq!(Preset::from_toml(&t).unwrap(), p);
    }

    #[test]
    fn rejects_malformed_files() {
        assert!(matches!(
            Preset::from_toml("this is not toml ="),
            Err(PresetError::Parse(_))
        ));
        assert!(matches!(
            Preset::from_toml("schema_version = 3\n"),
            Err(PresetError::Parse(_))
        ));
        assert!(matches!(
            Preset::from_toml("format = \"theme\"\n"),
            Err(PresetError::WrongFormat(_))
        ));
        let newer = sample()
            .to_toml()
            .unwrap()
            .replace("version = 2", "version = 9");
        assert!(matches!(
            Preset::from_toml(&newer),
            Err(PresetError::TooNew(9))
        ));
        let unknown = format!("{}\nsurprise = 1\n", sample().to_toml().unwrap());
        assert!(Preset::from_toml(&unknown).is_err());
        let mut bad = sample();
        bad.bar.height = Some(500);
        let text = toml::to_string_pretty(&bad).unwrap();
        assert!(matches!(
            Preset::from_toml(&text),
            Err(PresetError::Invalid(_))
        ));
        assert!(matches!(
            Preset::from_toml(&"x".repeat(MAX_PRESET_BYTES + 1)),
            Err(PresetError::TooLarge)
        ));
    }

    #[test]
    fn file_names() {
        assert_eq!(preset_file_name("Evening glass!"), "evening-glass.toml");
        let k = preset_file_name("저녁 테마");
        assert!(k.starts_with("preset-") && k.ends_with(".toml"), "{k}");
        assert_ne!(preset_file_name("저녁"), preset_file_name("아침"));
    }

    #[test]
    fn save_and_list() {
        let dir = tempfile::tempdir().unwrap();
        let p = sample();
        p.save(&dir.path().join(preset_file_name(&p.name))).unwrap();
        std::fs::write(dir.path().join("broken.toml"), "format = 1").unwrap();
        let list = list_presets(dir.path());
        assert_eq!(list.len(), 2);
        assert!(
            list.iter()
                .any(|(_, r)| r.as_ref().is_ok_and(|x| x.name == "Evening glass"))
        );
        assert!(
            list.iter().any(|(_, r)| r.is_err()),
            "invalid files are reported, not dropped"
        );
    }
}

#[cfg(test)]
mod version_tests {
    use super::*;

    #[test]
    fn version_1_presets_still_load() {
        let v1 = "format = \"windowsrice-preset\"\nversion = 1\nname = \"Old\"\n\n[theme]\nactive = \"windowsrice-glass\"\nmode = \"dark\"\n\n[bar]\nspacing = 4\n";
        let p = Preset::from_toml(v1).expect("a Phase 6 preset loads");
        assert_eq!(p.version, 1);
        assert_eq!(p.name, "Old");
        assert!(
            Preset::new("New", p.theme.clone(), p.bar.clone())
                .to_toml()
                .unwrap()
                .contains("version = 2")
        );
    }
}
