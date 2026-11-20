//! Appearance settings that windowsRICE follows (read-only).

use windows::Win32::Globalization::GetUserDefaultUILanguage;

use crate::registry_read::{Hive, read_dword};

const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

/// Windows app mode: `Some(true)` for light, `Some(false)` for dark.
pub fn apps_use_light_theme() -> Option<bool> {
    read_dword(Hive::CurrentUser, PERSONALIZE, "AppsUseLightTheme")
        .ok()
        .flatten()
        .map(|v| v != 0)
}

/// Settings › Personalization › Colors › Transparency effects. Absent means
/// the Windows default (on).
pub fn transparency_effects_enabled() -> bool {
    read_dword(Hive::CurrentUser, PERSONALIZE, "EnableTransparency")
        .ok()
        .flatten()
        .is_none_or(|v| v != 0)
}

/// The Windows display language is Korean.
pub fn ui_language_is_korean() -> bool {
    // SAFETY: no preconditions.
    let lang = unsafe { GetUserDefaultUILanguage() };
    lang & 0x3FF == 0x12
}
