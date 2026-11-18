//! windowsRICE user configuration (`config.toml`).
//!
//! Loading never overwrites or deletes a file. If the file is missing or
//! invalid, the caller receives the defaults together with a [`LoadOutcome`]
//! that explains what happened, so the problem can be shown to the user.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::hotkey::Hotkey;
use crate::logging::LogLevel;
use crate::{CoreError, Result};

/// Current configuration schema version.
pub const CONFIG_SCHEMA_VERSION: u32 = 1;

/// Heaviest font weight allowed by the default design rules.
pub const MAX_ALLOWED_FONT_WEIGHT: u16 = 600;
const MIN_FONT_WEIGHT: u16 = 100;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub general: GeneralConfig,
    pub typography: TypographyConfig,
    pub launcher: LauncherConfig,
    pub logging: LoggingConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            general: GeneralConfig::default(),
            typography: TypographyConfig::default(),
            launcher: LauncherConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// Follow the Windows display language.
    #[default]
    Auto,
    English,
    Korean,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GeneralConfig {
    pub language: Language,
    pub active_theme: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            language: Language::Auto,
            active_theme: "windowsRICE Default".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TypographyConfig {
    /// Primary UI font family. Uses an installed Windows font by default.
    pub font_family: String,
    /// Fallback families in order; Malgun Gothic covers Hangul.
    pub fallback_fonts: Vec<String>,
    /// Weight for normal text.
    pub regular_weight: u16,
    /// Weight for limited emphasis (selected controls, section titles).
    pub emphasis_weight: u16,
    /// Upper bound for any weight used by the theme. Never above 600.
    pub max_weight: u16,
    /// Additional text scale on top of Windows display scaling.
    pub text_scale: f32,
}

impl Default for TypographyConfig {
    fn default() -> Self {
        Self {
            font_family: "Segoe UI Variable".to_owned(),
            fallback_fonts: vec!["Malgun Gothic".to_owned()],
            regular_weight: 400,
            emphasis_weight: 500,
            max_weight: 600,
            text_scale: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LauncherConfig {
    pub hotkey: Hotkey,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            hotkey: Hotkey::DEFAULT_LAUNCHER,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LoggingConfig {
    pub level: LogLevel,
    pub max_log_files: u16,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            max_log_files: 7,
        }
    }
}

/// What happened when loading the configuration file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadOutcome {
    Loaded,
    /// No file exists; defaults are in use. Nothing was written.
    Missing,
    /// The file exists but could not be used; defaults are in use and the
    /// file was left untouched.
    Invalid {
        error: String,
    },
}

fn is_valid_name(s: &str, max_len: usize) -> bool {
    let trimmed = s.trim();
    !trimmed.is_empty() && trimmed.chars().count() <= max_len && !s.chars().any(char::is_control)
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        let invalid = |m: String| Err(CoreError::ConfigInvalid(m));

        if self.schema_version == 0 {
            return invalid("schema_version must be at least 1".into());
        }
        if self.schema_version > CONFIG_SCHEMA_VERSION {
            return invalid(format!(
                "schema_version {} is newer than this build supports ({CONFIG_SCHEMA_VERSION})",
                self.schema_version
            ));
        }
        if !is_valid_name(&self.general.active_theme, 64) {
            return invalid("general.active_theme must be 1-64 printable characters".into());
        }

        let t = &self.typography;
        if !is_valid_name(&t.font_family, 64) {
            return invalid("typography.font_family must be 1-64 printable characters".into());
        }
        if let Some(bad) = t.fallback_fonts.iter().find(|f| !is_valid_name(f, 64)) {
            return invalid(format!(
                "typography.fallback_fonts contains an invalid name {bad:?}"
            ));
        }
        for (name, w) in [
            ("regular_weight", t.regular_weight),
            ("emphasis_weight", t.emphasis_weight),
            ("max_weight", t.max_weight),
        ] {
            if !(MIN_FONT_WEIGHT..=MAX_ALLOWED_FONT_WEIGHT).contains(&w) {
                return invalid(format!(
                    "typography.{name} = {w} is outside {MIN_FONT_WEIGHT}-{MAX_ALLOWED_FONT_WEIGHT}"
                ));
            }
        }
        if !(t.regular_weight <= t.emphasis_weight && t.emphasis_weight <= t.max_weight) {
            return invalid("typography weights must satisfy regular <= emphasis <= max".into());
        }
        if !t.text_scale.is_finite() || !(0.75..=2.0).contains(&t.text_scale) {
            return invalid(format!(
                "typography.text_scale = {} is outside 0.75-2.0",
                t.text_scale
            ));
        }
        if !(1..=100).contains(&self.logging.max_log_files) {
            return invalid("logging.max_log_files must be 1-100".into());
        }
        Ok(())
    }

    /// Parses and validates TOML. `origin` is used only in error messages.
    pub fn from_toml_str(text: &str, origin: &Path) -> Result<Self> {
        let config: Config = toml::from_str(text).map_err(|e| CoreError::ConfigParse {
            path: origin.to_path_buf(),
            message: e.to_string(),
        })?;
        config.validate()?;
        Ok(config)
    }

    pub fn to_toml_string(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| CoreError::ConfigSerialize(e.to_string()))
    }

    /// Loads `path`, falling back to defaults without modifying the file.
    pub fn load_or_default(path: &Path) -> (Self, LoadOutcome) {
        match std::fs::read_to_string(path) {
            Ok(text) => match Self::from_toml_str(&text, path) {
                Ok(c) => (c, LoadOutcome::Loaded),
                Err(e) => (
                    Self::default(),
                    LoadOutcome::Invalid {
                        error: e.to_string(),
                    },
                ),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                (Self::default(), LoadOutcome::Missing)
            }
            Err(e) => (
                Self::default(),
                LoadOutcome::Invalid {
                    error: CoreError::io(path, e).to_string(),
                },
            ),
        }
    }

    /// Validates and writes the configuration atomically. If a previous file
    /// exists it is copied to `<file>.bak` first.
    pub fn save_atomic(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let text = self.to_toml_string()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
        }
        if path.exists() {
            let backup = sibling_with_suffix(path, ".bak");
            std::fs::copy(path, &backup).map_err(|e| CoreError::io(&backup, e))?;
        }
        let tmp = sibling_with_suffix(path, ".tmp");
        {
            let mut f = std::fs::File::create(&tmp).map_err(|e| CoreError::io(&tmp, e))?;
            f.write_all(text.as_bytes())
                .and_then(|()| f.sync_all())
                .map_err(|e| CoreError::io(&tmp, e))?;
        }
        std::fs::rename(&tmp, path).map_err(|e| CoreError::io(path, e))
    }
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin() -> &'static Path {
        Path::new("test.toml")
    }

    #[test]
    fn defaults_match_design_rules() {
        let c = Config::default();
        c.validate().unwrap();
        assert_eq!(c.typography.font_family, "Segoe UI Variable");
        assert_eq!(c.typography.fallback_fonts, ["Malgun Gothic"]);
        assert_eq!(
            (
                c.typography.regular_weight,
                c.typography.emphasis_weight,
                c.typography.max_weight
            ),
            (400, 500, 600)
        );
        assert_eq!(c.launcher.hotkey.to_string(), "Ctrl+Alt+Space");
        assert_eq!(c.general.active_theme, "windowsRICE Default");
    }

    #[test]
    fn empty_file_yields_defaults() {
        assert_eq!(
            Config::from_toml_str("", origin()).unwrap(),
            Config::default()
        );
    }

    #[test]
    fn partial_file_fills_in_defaults() {
        let c = Config::from_toml_str("[launcher]\nhotkey = \"Alt+Shift+K\"\n", origin()).unwrap();
        assert_eq!(c.launcher.hotkey.to_string(), "Alt+Shift+K");
        assert_eq!(c.typography, TypographyConfig::default());
    }

    #[test]
    fn round_trips_through_toml() {
        let mut c = Config::default();
        c.general.language = Language::Korean;
        c.typography.text_scale = 1.25;
        let text = c.to_toml_string().unwrap();
        assert_eq!(Config::from_toml_str(&text, origin()).unwrap(), c);
    }

    #[test]
    fn rejects_bold_and_inverted_weights() {
        let bold = "[typography]\nmax_weight = 700\n";
        assert!(Config::from_toml_str(bold, origin()).is_err());
        let inverted = "[typography]\nregular_weight = 500\nemphasis_weight = 400\n";
        assert!(Config::from_toml_str(inverted, origin()).is_err());
    }

    #[test]
    fn rejects_unknown_fields_bad_hotkeys_and_future_schema() {
        assert!(Config::from_toml_str("unknown = 1\n", origin()).is_err());
        assert!(Config::from_toml_str("[launcher]\nhotkey = \"Space\"\n", origin()).is_err());
        assert!(Config::from_toml_str("schema_version = 99\n", origin()).is_err());
        assert!(Config::from_toml_str("[typography]\ntext_scale = 5.0\n", origin()).is_err());
        assert!(Config::from_toml_str("[general]\nactive_theme = \"  \"\n", origin()).is_err());
    }

    #[test]
    fn korean_values_are_accepted() {
        let c = Config::from_toml_str(
            "[general]\nactive_theme = \"나의 테마\"\n[typography]\nfallback_fonts = [\"맑은 고딕\"]\n",
            origin(),
        )
        .unwrap();
        assert_eq!(c.general.active_theme, "나의 테마");
    }

    #[test]
    fn load_missing_file_reports_missing_and_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        let (c, outcome) = Config::load_or_default(&path);
        assert_eq!(outcome, LoadOutcome::Missing);
        assert_eq!(c, Config::default());
        assert!(!path.exists());
    }

    #[test]
    fn load_corrupt_file_falls_back_without_touching_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "this is [not valid").unwrap();
        let (c, outcome) = Config::load_or_default(&path);
        assert!(matches!(outcome, LoadOutcome::Invalid { .. }));
        assert_eq!(c, Config::default());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "this is [not valid"
        );
    }

    #[test]
    fn save_atomic_writes_and_backs_up_previous() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("sub").join("config.toml");
        let first = Config::default();
        first.save_atomic(&path).unwrap();
        assert!(!sibling_with_suffix(&path, ".bak").exists());

        let mut second = first.clone();
        second.general.language = Language::English;
        second.save_atomic(&path).unwrap();

        let (loaded, outcome) = Config::load_or_default(&path);
        assert_eq!(outcome, LoadOutcome::Loaded);
        assert_eq!(loaded, second);
        let backup = std::fs::read_to_string(sibling_with_suffix(&path, ".bak")).unwrap();
        assert_eq!(Config::from_toml_str(&backup, origin()).unwrap(), first);
        assert!(!sibling_with_suffix(&path, ".tmp").exists());
    }

    #[test]
    fn save_refuses_invalid_config() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        let mut c = Config::default();
        c.typography.max_weight = 800;
        assert!(c.save_atomic(&path).is_err());
        assert!(!path.exists());
    }
}
