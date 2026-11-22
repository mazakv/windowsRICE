//! windowsRICE user configuration (`config.toml`).
//!
//! Loading never overwrites or deletes a file. If the file is missing or
//! invalid, the caller receives the defaults together with a [`LoadOutcome`]
//! that explains what happened, so the problem can be shown to the user.
//!
//! # Schema history
//!
//! - **v1** (Phase 1): `general.active_theme` held a theme *display name*.
//! - **v2** (Phase 3): theme selection moved to `[theme]`, which stores a
//!   stable theme *id* (`theme.active`) and a mode preference (`theme.mode`).
//! - **v3** (Phase 5): `[bar]` configures the windowsRICE Bar (modules,
//!   height, work-area reservation, auto-hide, monitors, popup). v2 files
//!   gain the default `[bar]` section; nothing else changes.
//!
//! Older files are migrated in memory when loaded ([`LoadOutcome::Migrated`]).
//! The file on disk is not rewritten until the application saves, and the
//! caller is expected to preserve the original before that first save.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::bar::BarConfig;
use crate::hotkey::Hotkey;
use crate::logging::LogLevel;
use crate::{CoreError, Result};

/// Current configuration schema version.
pub const CONFIG_SCHEMA_VERSION: u32 = 3;

/// Theme id selected on first run and used as the fallback.
pub const DEFAULT_THEME_ID: &str = "windowsrice-default";

/// Heaviest font weight allowed by the default design rules.
pub const MAX_ALLOWED_FONT_WEIGHT: u16 = 600;
const MIN_FONT_WEIGHT: u16 = 100;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub general: GeneralConfig,
    pub theme: ThemeSelection,
    pub typography: TypographyConfig,
    pub launcher: LauncherConfig,
    pub logging: LoggingConfig,
    pub bar: BarConfig,
    /// Optional bottom bar (Phase 14), configured independently of `[bar]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bottom_bar: Option<BarConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            general: GeneralConfig::default(),
            theme: ThemeSelection::default(),
            typography: TypographyConfig::default(),
            launcher: LauncherConfig::default(),
            logging: LoggingConfig::default(),
            bar: BarConfig::default(),
            bottom_bar: None,
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GeneralConfig {
    pub language: Language,
}

/// Which color mode of the active theme to use.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeModePreference {
    /// Follow the Windows app mode when known, otherwise the theme's default.
    #[default]
    Auto,
    Dark,
    Light,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ThemeSelection {
    /// Theme id (for example `windowsrice-default`), not a display name.
    pub active: String,
    pub mode: ThemeModePreference,
}

impl Default for ThemeSelection {
    fn default() -> Self {
        Self {
            active: DEFAULT_THEME_ID.to_owned(),
            mode: ThemeModePreference::Auto,
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
    /// The file uses an older schema and was migrated in memory. The file on
    /// disk is unchanged; `notes` describe every value that was moved or
    /// could not be carried over.
    Migrated {
        from_version: u32,
        notes: Vec<String>,
    },
    /// No file exists; defaults are in use. Nothing was written.
    Missing,
    /// The file exists but could not be used; defaults are in use and the
    /// file was left untouched.
    Invalid {
        error: String,
    },
}

/// Record of an in-memory schema migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
    pub from_version: u32,
    pub notes: Vec<String>,
}

fn is_valid_name(s: &str, max_len: usize) -> bool {
    let trimmed = s.trim();
    !trimmed.is_empty() && trimmed.chars().count() <= max_len && !s.chars().any(char::is_control)
}

/// Theme ids: 1–64 characters of `a–z`, `0–9` and single inner hyphens.
pub fn is_valid_theme_id(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !s.starts_with('-')
        && !s.ends_with('-')
        && !s.contains("--")
}

/// Converts a v1 theme display name to a theme id: ASCII letters and digits
/// are lowercased, every other run of characters becomes one hyphen.
/// `"windowsRICE macOS Inspired"` → `"windowsrice-macos-inspired"`.
pub fn theme_name_to_id(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn parse_error(origin: &Path, message: impl Into<String>) -> CoreError {
    CoreError::ConfigParse {
        path: origin.to_path_buf(),
        message: message.into(),
    }
}

/// v1 → v2: move `general.active_theme` (display name) to `theme.active` (id).
fn migrate_v1_to_v2(table: &mut toml::Table, origin: &Path, notes: &mut Vec<String>) -> Result<()> {
    if table.contains_key("theme") {
        return Err(parse_error(
            origin,
            "schema v1 file contains a [theme] section, which v1 does not define",
        ));
    }
    let legacy = match table.get_mut("general") {
        Some(toml::Value::Table(general)) => general.remove("active_theme"),
        Some(_) => return Err(parse_error(origin, "general must be a table")),
        None => None,
    };
    if let Some(value) = legacy {
        let toml::Value::String(name) = value else {
            return Err(parse_error(origin, "general.active_theme must be a string"));
        };
        let id = theme_name_to_id(&name);
        if is_valid_theme_id(&id) {
            notes.push(format!(
                "general.active_theme \"{name}\" was migrated to theme.active \"{id}\""
            ));
            let mut theme = toml::Table::new();
            theme.insert("active".into(), toml::Value::String(id));
            table.insert("theme".into(), toml::Value::Table(theme));
        } else {
            notes.push(format!(
                "general.active_theme \"{name}\" could not be converted to a theme id; \
                 theme.active uses the default \"{DEFAULT_THEME_ID}\" (the original value remains in the original file)"
            ));
        }
    }
    table.insert("schema_version".into(), toml::Value::Integer(2));
    Ok(())
}

/// v2 → v3: adds the `[bar]` section with its defaults. A v2 file has no
/// `[bar]`; if one is present the file is not a genuine v2 file.
fn migrate_v2_to_v3(table: &mut toml::Table, origin: &Path, notes: &mut Vec<String>) -> Result<()> {
    if table.contains_key("bar") {
        return Err(parse_error(
            origin,
            "schema v2 file contains a [bar] section, which v2 does not define",
        ));
    }
    notes.push("added the [bar] section with default settings (schema v3)".into());
    table.insert("schema_version".into(), toml::Value::Integer(3));
    Ok(())
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        let invalid = |m: String| Err(CoreError::ConfigInvalid(m));

        if self.schema_version != CONFIG_SCHEMA_VERSION {
            return invalid(format!(
                "in-memory schema_version must be {CONFIG_SCHEMA_VERSION}, found {}",
                self.schema_version
            ));
        }
        if !is_valid_theme_id(&self.theme.active) {
            return invalid(format!(
                "theme.active {:?} is not a valid theme id (a-z, 0-9 and hyphens)",
                self.theme.active
            ));
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
        let mut bar = self.bar.problems();
        if let Some(b) = &self.bottom_bar {
            bar.extend(
                b.problems()
                    .into_iter()
                    .map(|p| p.replacen("bar", "bottom_bar", 1)),
            );
        }
        if !bar.is_empty() {
            return invalid(bar.join("; "));
        }
        Ok(())
    }

    /// Parses, migrates (if needed) and validates TOML. A missing
    /// `schema_version` means v1, the only version that allowed omitting it.
    pub fn from_toml_str_migrating(text: &str, origin: &Path) -> Result<(Self, Option<Migration>)> {
        let mut table: toml::Table =
            toml::from_str(text).map_err(|e| parse_error(origin, e.to_string()))?;
        let version = match table.get("schema_version") {
            None => 1,
            Some(toml::Value::Integer(v)) => u32::try_from(*v)
                .map_err(|_| parse_error(origin, format!("schema_version {v} is out of range")))?,
            Some(_) => return Err(parse_error(origin, "schema_version must be an integer")),
        };
        let mut notes = Vec::new();
        match version {
            0 => {
                return Err(CoreError::ConfigInvalid(
                    "schema_version must be at least 1".into(),
                ));
            }
            1 => {
                migrate_v1_to_v2(&mut table, origin, &mut notes)?;
                migrate_v2_to_v3(&mut table, origin, &mut notes)?;
            }
            2 => migrate_v2_to_v3(&mut table, origin, &mut notes)?,
            CONFIG_SCHEMA_VERSION => {}
            newer => {
                return Err(CoreError::ConfigInvalid(format!(
                    "schema_version {newer} is newer than this build supports ({CONFIG_SCHEMA_VERSION})"
                )));
            }
        }
        let config: Config = toml::Value::Table(table)
            .try_into()
            .map_err(|e: toml::de::Error| parse_error(origin, e.to_string()))?;
        config.validate()?;
        let migration = (version != CONFIG_SCHEMA_VERSION).then_some(Migration {
            from_version: version,
            notes,
        });
        Ok((config, migration))
    }

    /// Parses, migrates and validates TOML, discarding the migration record.
    /// `origin` is used only in error messages.
    pub fn from_toml_str(text: &str, origin: &Path) -> Result<Self> {
        Self::from_toml_str_migrating(text, origin).map(|(c, _)| c)
    }

    pub fn to_toml_string(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| CoreError::ConfigSerialize(e.to_string()))
    }

    /// Loads `path`, falling back to defaults without modifying the file.
    pub fn load_or_default(path: &Path) -> (Self, LoadOutcome) {
        match std::fs::read_to_string(path) {
            Ok(text) => match Self::from_toml_str_migrating(&text, path) {
                Ok((c, None)) => (c, LoadOutcome::Loaded),
                Ok((c, Some(m))) => (
                    c,
                    LoadOutcome::Migrated {
                        from_version: m.from_version,
                        notes: m.notes,
                    },
                ),
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

/// `config.toml` + `.bak` → `config.toml.bak`.
pub fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
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
        assert_eq!(c.schema_version, CONFIG_SCHEMA_VERSION);
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
        assert_eq!(c.theme.active, "windowsrice-default");
        assert_eq!(c.theme.mode, ThemeModePreference::Auto);
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
        c.theme.active = "windowsrice-glass".into();
        c.theme.mode = ThemeModePreference::Light;
        let text = c.to_toml_string().unwrap();
        let (again, migration) = Config::from_toml_str_migrating(&text, origin()).unwrap();
        assert_eq!(again, c);
        assert_eq!(migration, None);
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
        assert!(Config::from_toml_str("schema_version = 0\n", origin()).is_err());
        assert!(Config::from_toml_str("schema_version = \"2\"\n", origin()).is_err());
        assert!(Config::from_toml_str("[typography]\ntext_scale = 5.0\n", origin()).is_err());
        // v2 no longer has general.active_theme.
        assert!(
            Config::from_toml_str(
                "schema_version = 2\n[general]\nactive_theme = \"x\"\n",
                origin()
            )
            .is_err()
        );
        assert!(
            Config::from_toml_str(
                "schema_version = 2\n[theme]\nactive = \"Bad Id\"\n",
                origin()
            )
            .is_err()
        );
        assert!(
            Config::from_toml_str("schema_version = 2\n[theme]\nmode = \"sepia\"\n", origin())
                .is_err()
        );
    }

    #[test]
    fn korean_values_are_accepted() {
        let c = Config::from_toml_str(
            "schema_version = 2\n[typography]\nfallback_fonts = [\"맑은 고딕\"]\n",
            origin(),
        )
        .unwrap();
        assert_eq!(c.typography.fallback_fonts, ["맑은 고딕"]);
    }

    #[test]
    fn theme_name_to_id_matches_builtin_ids() {
        for (name, id) in [
            ("windowsRICE Default", "windowsrice-default"),
            ("windowsRICE Minimal", "windowsrice-minimal"),
            ("windowsRICE HYPR", "windowsrice-hypr"),
            ("windowsRICE macOS Inspired", "windowsrice-macos-inspired"),
            ("windowsRICE Glass", "windowsrice-glass"),
            ("windowsRICE AMOLED", "windowsrice-amoled"),
            ("windowsRICE Ambient", "windowsrice-ambient"),
            ("windowsRICE Light", "windowsrice-light"),
            ("  My  Theme -- 2 ", "my-theme-2"),
            ("나의 테마", ""),
        ] {
            assert_eq!(theme_name_to_id(name), id, "{name}");
        }
    }

    #[test]
    fn migrates_v1_display_name_to_v2_theme_id() {
        let v1 = "schema_version = 1\n[general]\nlanguage = \"korean\"\nactive_theme = \"windowsRICE macOS Inspired\"\n[launcher]\nhotkey = \"Alt+Shift+K\"\n";
        let (c, m) = Config::from_toml_str_migrating(v1, origin()).unwrap();
        let m = m.expect("migration expected");
        assert_eq!(m.from_version, 1);
        assert!(
            m.notes[0].contains("windowsRICE macOS Inspired")
                && m.notes[0].contains("windowsrice-macos-inspired")
        );
        // v1 files are migrated through v2 to the current schema.
        assert_eq!(c.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(c.theme.active, "windowsrice-macos-inspired");
        // Every other v1 value is preserved.
        assert_eq!(c.general.language, Language::Korean);
        assert_eq!(c.launcher.hotkey.to_string(), "Alt+Shift+K");
    }

    #[test]
    fn migrates_v1_without_schema_version_or_theme() {
        let (c, m) =
            Config::from_toml_str_migrating("[launcher]\nhotkey = \"Ctrl+K\"\n", origin()).unwrap();
        assert_eq!(m.map(|m| m.from_version), Some(1));
        assert_eq!(c.theme, ThemeSelection::default());
        assert_eq!(c.launcher.hotkey.to_string(), "Ctrl+K");
    }

    #[test]
    fn unconvertible_v1_theme_name_falls_back_with_a_note() {
        let (c, m) =
            Config::from_toml_str_migrating("[general]\nactive_theme = \"나의 테마\"\n", origin())
                .unwrap();
        assert_eq!(c.theme.active, DEFAULT_THEME_ID);
        let notes = m.unwrap().notes;
        assert!(
            notes[0].contains("나의 테마") && notes[0].contains("could not be converted"),
            "{notes:?}"
        );
    }

    #[test]
    fn malformed_v1_files_are_rejected_not_guessed() {
        assert!(Config::from_toml_str("[general]\nactive_theme = 5\n", origin()).is_err());
        assert!(Config::from_toml_str("general = 3\n", origin()).is_err());
        assert!(
            Config::from_toml_str("schema_version = 1\n[theme]\nactive = \"x\"\n", origin())
                .is_err()
        );
    }

    #[test]
    fn load_reports_migration_and_leaves_file_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        let v1 = "[general]\nactive_theme = \"windowsRICE Glass\"\n";
        std::fs::write(&path, v1).unwrap();
        let (c, outcome) = Config::load_or_default(&path);
        assert!(matches!(
            outcome,
            LoadOutcome::Migrated {
                from_version: 1,
                ..
            }
        ));
        assert_eq!(c.theme.active, "windowsrice-glass");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), v1);
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
        let mut c = Config::default();
        c.theme.active = "Not An Id".into();
        assert!(c.save_atomic(&path).is_err());
        assert!(!path.exists());
    }
}
