//! windowsRICE design system.
//!
//! Platform-independent: this crate defines design tokens, parses and
//! validates TOML themes, and ships the built-in presets. It has no UI
//! framework or Windows dependency. Theme files are treated as untrusted
//! input: size-limited, strictly typed, unknown keys rejected, and never
//! executed.

#![forbid(unsafe_code)]

pub mod color;
pub mod contrast;
mod error;
pub mod presets;
pub mod schema;
pub mod validate;

pub use color::Color;
pub use error::ThemeError;
pub use schema::{Mode, Palette, Theme};
pub use validate::{Issue, Severity, ValidationReport, validate};

/// Maximum accepted size of a theme TOML file.
pub const MAX_THEME_FILE_BYTES: usize = 256 * 1024;

/// A theme that parsed and passed validation. `report` holds any warnings.
#[derive(Debug, Clone)]
pub struct ValidatedTheme {
    pub theme: Theme,
    pub report: ValidationReport,
}

/// Parses and validates a theme from TOML text.
pub fn parse_theme(text: &str) -> Result<ValidatedTheme, ThemeError> {
    if text.len() > MAX_THEME_FILE_BYTES {
        return Err(ThemeError::TooLarge {
            size: text.len(),
            max: MAX_THEME_FILE_BYTES,
        });
    }
    let theme: Theme = toml::from_str(text).map_err(|e| ThemeError::Parse(e.to_string()))?;
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
