//! Platform-independent hotkey model.
//!
//! Hotkeys are stored as human-readable strings such as `Ctrl+Alt+Space`.
//! Registration happens later through the documented Windows `RegisterHotKey`
//! API; windowsRICE does not use low-level keyboard hooks. Registration
//! conflicts are detected at registration time and reported to the user.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::CoreError;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

impl Modifiers {
    pub fn is_empty(&self) -> bool {
        !(self.ctrl || self.alt || self.shift || self.win)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// `A`–`Z` (stored uppercase) or `0`–`9`.
    Char(char),
    /// `F1`–`F24`.
    Function(u8),
    Space,
    Enter,
    Tab,
    Escape,
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Char(c) => write!(f, "{c}"),
            Key::Function(n) => write!(f, "F{n}"),
            Key::Space => f.write_str("Space"),
            Key::Enter => f.write_str("Enter"),
            Key::Tab => f.write_str("Tab"),
            Key::Escape => f.write_str("Esc"),
        }
    }
}

fn parse_key(token: &str) -> Option<Key> {
    let lower = token.to_ascii_lowercase();
    match lower.as_str() {
        "space" => return Some(Key::Space),
        "enter" | "return" => return Some(Key::Enter),
        "tab" => return Some(Key::Tab),
        "esc" | "escape" => return Some(Key::Escape),
        _ => {}
    }
    let mut chars = token.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_alphanumeric() {
            return Some(Key::Char(c.to_ascii_uppercase()));
        }
        return None;
    }
    lower
        .strip_prefix('f')
        .and_then(|rest| rest.parse::<u8>().ok())
        .filter(|n| (1..=24).contains(n))
        .map(Key::Function)
}

/// A key combination. Always has at least one modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Hotkey {
    pub modifiers: Modifiers,
    pub key: Key,
}

impl Hotkey {
    /// The initial default launcher shortcut: `Ctrl+Alt+Space`.
    pub const DEFAULT_LAUNCHER: Hotkey = Hotkey {
        modifiers: Modifiers {
            ctrl: true,
            alt: true,
            shift: false,
            win: false,
        },
        key: Key::Space,
    };
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = &self.modifiers;
        for (on, name) in [
            (m.ctrl, "Ctrl"),
            (m.alt, "Alt"),
            (m.shift, "Shift"),
            (m.win, "Win"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        write!(f, "{}", self.key)
    }
}

impl FromStr for Hotkey {
    type Err = CoreError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let err = |reason: &str| CoreError::Hotkey {
            input: input.to_owned(),
            reason: reason.to_owned(),
        };
        let tokens: Vec<&str> = input.split('+').map(str::trim).collect();
        if tokens.iter().any(|t| t.is_empty()) {
            return Err(err("empty key name"));
        }
        let (key_token, modifier_tokens) = tokens.split_last().ok_or_else(|| err("empty"))?;

        let mut modifiers = Modifiers::default();
        for token in modifier_tokens {
            let slot = match token.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => &mut modifiers.ctrl,
                "alt" => &mut modifiers.alt,
                "shift" => &mut modifiers.shift,
                "win" | "super" | "meta" => &mut modifiers.win,
                _ => return Err(err(&format!("unknown modifier \"{token}\""))),
            };
            if *slot {
                return Err(err(&format!("duplicate modifier \"{token}\"")));
            }
            *slot = true;
        }
        let key =
            parse_key(key_token).ok_or_else(|| err(&format!("unknown key \"{key_token}\"")))?;
        if modifiers.is_empty() {
            return Err(err(
                "at least one modifier (Ctrl, Alt, Shift or Win) is required",
            ));
        }
        Ok(Hotkey { modifiers, key })
    }
}

impl TryFrom<String> for Hotkey {
    type Error = CoreError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<Hotkey> for String {
    fn from(value: Hotkey) -> Self {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_launcher_is_ctrl_alt_space() {
        assert_eq!(Hotkey::DEFAULT_LAUNCHER.to_string(), "Ctrl+Alt+Space");
        assert_eq!(
            "Ctrl+Alt+Space".parse::<Hotkey>().unwrap(),
            Hotkey::DEFAULT_LAUNCHER
        );
    }

    #[test]
    fn parsing_is_case_and_whitespace_insensitive_and_canonicalizes() {
        let h: Hotkey = " alt + control + space ".parse().unwrap();
        assert_eq!(h.to_string(), "Ctrl+Alt+Space");
        let h: Hotkey = "win+shift+f12".parse().unwrap();
        assert_eq!(h.to_string(), "Shift+Win+F12");
        let h: Hotkey = "Ctrl+k".parse().unwrap();
        assert_eq!(h.key, Key::Char('K'));
    }

    #[test]
    fn rejects_invalid_hotkeys() {
        for bad in [
            "",
            "Space",
            "Ctrl+",
            "Ctrl+Ctrl+A",
            "Hyper+A",
            "Ctrl+F25",
            "Ctrl+F0",
            "Ctrl+ab",
            "Ctrl+한",
        ] {
            assert!(bad.parse::<Hotkey>().is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn round_trips_through_string() {
        for s in [
            "Ctrl+Alt+Space",
            "Alt+Shift+Tab",
            "Ctrl+Win+F1",
            "Alt+Esc",
            "Ctrl+9",
        ] {
            let h: Hotkey = s.parse().unwrap();
            assert_eq!(h.to_string().parse::<Hotkey>().unwrap(), h);
        }
    }
}
