//! sRGB color type, compositing and WCAG contrast math.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// An sRGB color with straight (non-premultiplied) alpha.
///
/// Serialized as `#RRGGBB` (opaque) or `#RRGGBBAA`. Shorthand forms and
/// named colors are rejected so theme files stay unambiguous.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid color \"{input}\": expected #RRGGBB or #RRGGBBAA")]
pub struct ColorParseError {
    pub input: String,
}

impl Color {
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn is_opaque(self) -> bool {
        self.a == 255
    }

    /// Source-over compositing of `self` onto `background`. The background's
    /// alpha is ignored (treated as opaque); the result is opaque.
    pub fn over(self, background: Color) -> Color {
        let a = f64::from(self.a) / 255.0;
        let mix = |fg: u8, bg: u8| -> u8 {
            let v = (f64::from(fg) * a + f64::from(bg) * (1.0 - a)).round();
            // Clamped to 0..=255, so the cast cannot truncate.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let out = v.clamp(0.0, 255.0) as u8;
            out
        };
        Color::rgb(
            mix(self.r, background.r),
            mix(self.g, background.g),
            mix(self.b, background.b),
        )
    }

    /// Like [`Color::over`], but with an extra opacity multiplier applied to
    /// `self` (used for material/surface opacity).
    pub fn with_opacity_over(self, opacity: f64, background: Color) -> Color {
        let scaled = (f64::from(self.a) * opacity.clamp(0.0, 1.0)).round();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let a = scaled.clamp(0.0, 255.0) as u8;
        Color { a, ..self }.over(background)
    }

    /// WCAG relative luminance of the RGB channels (alpha ignored; composite
    /// first). Uses the sRGB threshold 0.04045 (WCAG 2.2 wording).
    pub fn relative_luminance(self) -> f64 {
        let lin = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }
}

/// WCAG contrast ratio between two opaque colors, in the range 1.0–21.0.
pub fn contrast_ratio(a: Color, b: Color) -> f64 {
    let (la, lb) = (a.relative_luminance(), b.relative_luminance());
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

impl FromStr for Color {
    type Err = ColorParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let err = || ColorParseError {
            input: input.to_owned(),
        };
        let hex = input.strip_prefix('#').ok_or_else(err)?;
        if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(err());
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| err());
        Ok(Color {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            a: if hex.len() == 8 { byte(6)? } else { 255 },
        })
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.r, self.g, self.b)?;
        if self.a != 255 {
            write!(f, "{:02X}", self.a)?;
        }
        Ok(())
    }
}

impl TryFrom<String> for Color {
    type Error = ColorParseError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<Color> for String {
    fn from(value: Color) -> Self {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats() {
        assert_eq!(
            "#1B1D23".parse::<Color>().unwrap(),
            Color::rgb(0x1B, 0x1D, 0x23)
        );
        let c: Color = "#ffffff14".parse().unwrap();
        assert_eq!((c.r, c.a), (255, 0x14));
        assert_eq!(c.to_string(), "#FFFFFF14");
        assert_eq!(Color::rgb(1, 2, 3).to_string(), "#010203");
    }

    #[test]
    fn rejects_invalid_colors() {
        for bad in [
            "",
            "#",
            "#FFF",
            "#FFFF",
            "FFFFFF",
            "#GGGGGG",
            "#12345",
            "#1234567",
            "#123456789",
            "red",
            "#１２３４５６",
            " #FFFFFF",
        ] {
            assert!(bad.parse::<Color>().is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn wcag_reference_ratios() {
        assert!((contrast_ratio(Color::BLACK, Color::WHITE) - 21.0).abs() < 1e-9);
        assert!((contrast_ratio(Color::WHITE, Color::WHITE) - 1.0).abs() < 1e-9);
        // #767676 on white is the well-known 4.54:1 boundary gray.
        let gray = Color::rgb(0x76, 0x76, 0x76);
        assert!((contrast_ratio(gray, Color::WHITE) - 4.54).abs() < 0.01);
        // Symmetric.
        assert_eq!(
            contrast_ratio(gray, Color::BLACK),
            contrast_ratio(Color::BLACK, gray)
        );
    }

    #[test]
    fn compositing() {
        let half_white: Color = "#FFFFFF80".parse().unwrap();
        assert_eq!(half_white.over(Color::BLACK), Color::rgb(128, 128, 128));
        assert_eq!(
            Color::rgb(10, 20, 30).over(Color::WHITE),
            Color::rgb(10, 20, 30)
        );
        assert_eq!(
            Color::WHITE.with_opacity_over(0.0, Color::BLACK),
            Color::BLACK
        );
        assert_eq!(
            Color::WHITE.with_opacity_over(1.0, Color::BLACK),
            Color::WHITE
        );
    }
}
