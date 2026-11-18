//! windowsRICE design system and theme engine.
//!
//! Platform-independent: this crate defines design tokens, parses and
//! validates TOML themes, ships the built-in presets, builds the theme
//! catalog (built-in plus user theme files), activates themes with rollback,
//! and persists the selection in `config.toml`. It has no UI framework or
//! Windows API dependency. Theme files are treated as untrusted input:
//! size-limited, strictly typed, unknown keys rejected, and never executed.

#![forbid(unsafe_code)]

pub mod catalog;
pub mod color;
pub mod contrast;
pub mod engine;
mod error;
pub mod notice;
pub mod presets;
pub mod schema;
pub mod service;
pub mod validate;
pub mod version;

pub use color::Color;
pub use error::ThemeError;
pub use schema::{Mode, Palette, THEME_SCHEMA_VERSION, Theme};
pub use validate::{Issue, Severity, ValidationReport, validate};

/// Maximum accepted size of a theme TOML file.
pub const MAX_THEME_FILE_BYTES: usize = 256 * 1024;

/// A theme that parsed and passed validation. `report` holds any warnings.
#[derive(Debug, Clone)]
pub struct ValidatedTheme {
    pub theme: Theme,
    pub report: ValidationReport,
}

/// Reads `meta.schema_version` without applying the strict schema.
fn declared_schema_version(text: &str) -> Option<u32> {
    let table: toml::Table = toml::from_str(text).ok()?;
    match table.get("meta")?.get("schema_version")? {
        toml::Value::Integer(v) => u32::try_from(*v).ok(),
        _ => None,
    }
}

/// Reads `meta.id` without applying the strict schema (for error reporting).
pub fn declared_theme_id(text: &str) -> Option<String> {
    let table: toml::Table = toml::from_str(text).ok()?;
    table.get("meta")?.get("id")?.as_str().map(str::to_owned)
}

/// Parses and validates a theme from TOML text.
pub fn parse_theme(text: &str) -> Result<ValidatedTheme, ThemeError> {
    if text.len() > MAX_THEME_FILE_BYTES {
        return Err(ThemeError::TooLarge {
            size: text.len(),
            max: MAX_THEME_FILE_BYTES,
        });
    }
    let theme: Theme = match toml::from_str(text) {
        Ok(theme) => theme,
        Err(e) => {
            // A newer schema usually adds tokens this build rejects as unknown.
            // Report that as an incompatibility rather than a confusing parse error.
            return Err(match declared_schema_version(text) {
                Some(found) if found > THEME_SCHEMA_VERSION => ThemeError::Incompatible {
                    found,
                    supported: THEME_SCHEMA_VERSION,
                    detail: e.message().to_owned(),
                },
                _ => ThemeError::Parse(e.to_string()),
            });
        }
    };
    let report = validate(&theme);
    if report.has_errors() {
        return Err(ThemeError::Invalid(report));
    }
    Ok(ValidatedTheme { theme, report })
}

impl Theme {
    /// Serializes the theme to TOML (for export and duplication).
    pub fn to_toml_string(&self) -> Result<String, ThemeError> {
        toml::to_string_pretty(self).map_err(|e| ThemeError::Serialize(e.to_string()))
    }
}
