//! Built-in presets, embedded at compile time.
//!
//! Each preset is an ordinary theme file under `themes/` and goes through the
//! same parser and validator as user themes. Tests guarantee that every
//! preset validates without errors.

use crate::{ThemeError, ValidatedTheme, parse_theme};

/// Identifier of the first-run default theme and the fallback for corrupt themes.
pub const DEFAULT_THEME_ID: &str = "windowsrice-default";

#[derive(Debug, Clone, Copy)]
pub struct BuiltinPreset {
    pub id: &'static str,
    pub file_name: &'static str,
    pub source: &'static str,
}

macro_rules! preset {
    ($id:literal, $file:literal) => {
        BuiltinPreset {
            id: $id,
            file_name: $file,
            source: include_str!(concat!("../themes/", $file)),
        }
    };
}

/// The built-in presets, in display order.
pub const BUILTIN_PRESETS: [BuiltinPreset; 17] = [
    preset!("windowsrice-default", "windowsrice-default.toml"),
    preset!("windowsrice-minimal", "windowsrice-minimal.toml"),
    preset!("windowsrice-hypr", "windowsrice-hypr.toml"),
    preset!(
        "windowsrice-macos-inspired",
        "windowsrice-macos-inspired.toml"
    ),
    preset!("windowsrice-glass", "windowsrice-glass.toml"),
    preset!("windowsrice-amoled", "windowsrice-amoled.toml"),
    preset!("windowsrice-ambient", "windowsrice-ambient.toml"),
    preset!("windowsrice-light", "windowsrice-light.toml"),
    preset!("windowsrice-midnight", "windowsrice-midnight.toml"),
    preset!("windowsrice-soft-gray", "windowsrice-soft-gray.toml"),
    preset!("windowsrice-blue-accent", "windowsrice-blue-accent.toml"),
    preset!("windowsrice-monochrome", "windowsrice-monochrome.toml"),
    preset!("windowsrice-ocean", "windowsrice-ocean.toml"),
    preset!("windowsrice-neon", "windowsrice-neon.toml"),
    preset!("windowsrice-pastel", "windowsrice-pastel.toml"),
    preset!("windowsrice-warm", "windowsrice-warm.toml"),
    preset!(
        "windowsrice-high-contrast",
        "windowsrice-high-contrast.toml"
    ),
];

/// Parses and validates one built-in preset by id.
pub fn load_builtin(id: &str) -> Option<Result<ValidatedTheme, ThemeError>> {
    BUILTIN_PRESETS
        .iter()
        .find(|p| p.id == id)
        .map(|p| parse_theme(p.source))
}

/// Parses and validates all built-in presets.
pub fn load_all_builtin() -> Vec<(BuiltinPreset, Result<ValidatedTheme, ThemeError>)> {
    BUILTIN_PRESETS
        .iter()
        .map(|p| (*p, parse_theme(p.source)))
        .collect()
}

/// The windowsRICE Default theme. This is the fallback when any other theme
/// is missing or invalid, so it must always load.
pub fn default_theme() -> ValidatedTheme {
    match load_builtin(DEFAULT_THEME_ID) {
        Some(Ok(theme)) => theme,
        // Unreachable in a tested build: the `all_eight_presets_load_without_errors`
        // and `default_has_both_modes_and_light_preset_is_light` tests guard it.
        Some(Err(e)) => panic!("built-in default theme failed validation: {e}"),
        None => panic!("built-in default theme is missing"),
    }
}
