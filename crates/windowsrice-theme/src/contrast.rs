//! WCAG 2.x AA contrast rules for palette token pairs.
//!
//! Text pairs require 4.5:1 (SC 1.4.3, normal-size text; windowsRICE body text
//! is 13 px, so the large-text exception is never assumed). Non-text pairs
//! (control boundaries, focus indicators, accent and status indicators)
//! require 3:1 (SC 1.4.11). Decorative separators and disabled text are
//! exempt and not checked.
//!
//! All checks use the opaque palette colors, which is exactly what is drawn
//! when transparency is off (Gaming Mode, reduced transparency, or no
//! backdrop). Translucent rendering is covered by
//! [`translucent_surface_checks`], whose results are reported as warnings
//! because the real backdrop is the user's wallpaper behind DWM blur.

use serde::Serialize;

use crate::color::{Color, contrast_ratio};
use crate::schema::Palette;

pub const AA_TEXT: f64 = 4.5;
pub const AA_NON_TEXT: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PairKind {
    Text,
    NonText,
}

impl PairKind {
    pub fn required_ratio(self) -> f64 {
        match self {
            PairKind::Text => AA_TEXT,
            PairKind::NonText => AA_NON_TEXT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContrastCheck {
    pub foreground: &'static str,
    pub background: &'static str,
    pub kind: PairKind,
    pub ratio: f64,
    pub required: f64,
    pub passes: bool,
}

type Getter = fn(&Palette) -> Color;
/// A named palette token.
type Token = (&'static str, Getter);
/// (foreground, background, requirement).
type Rule = (Token, Token, PairKind);

macro_rules! tok {
    ($name:ident) => {
        (stringify!($name), (|p: &Palette| p.$name) as Getter)
    };
}

/// Every (foreground, background, kind) pair that must meet AA.
fn rules() -> Vec<Rule> {
    use PairKind::{NonText, Text};
    let surfaces = [
        tok!(surface_base),
        tok!(surface_raised),
        tok!(surface_overlay),
    ];
    let mut r = Vec::new();

    r.push((tok!(text_primary), tok!(background), Text));
    for fg in [tok!(text_primary), tok!(text_secondary), tok!(text_muted)] {
        for bg in surfaces {
            r.push((fg, bg, Text));
        }
    }
    for bg in [tok!(accent), tok!(accent_hover), tok!(accent_pressed)] {
        r.push((tok!(text_on_accent), bg, Text));
    }
    r.push((tok!(text_primary), tok!(selection), Text));

    for fg in [tok!(accent), tok!(focus_ring), tok!(border_control)] {
        for bg in [tok!(surface_base), tok!(surface_raised)] {
            r.push((fg, bg, NonText));
        }
    }
    for fg in [tok!(success), tok!(warning), tok!(danger), tok!(info)] {
        for bg in [tok!(surface_base), tok!(surface_raised)] {
            r.push((fg, bg, NonText));
        }
    }
    r
}

/// Runs all AA contrast rules against one palette. Foreground colors with
/// alpha are composited onto the background first.
pub fn palette_checks(p: &Palette) -> Vec<ContrastCheck> {
    rules()
        .into_iter()
        .map(|((fg_name, fg), (bg_name, bg), kind)| {
            let bg_color = bg(p).over(Color::BLACK); // surfaces are opaque; this only normalizes alpha
            let fg_color = fg(p).over(bg_color);
            let ratio = contrast_ratio(fg_color, bg_color);
            let required = kind.required_ratio();
            ContrastCheck {
                foreground: fg_name,
                background: bg_name,
                kind,
                ratio,
                required,
                passes: ratio + 1e-9 >= required,
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TranslucencyCheck {
    pub surface: &'static str,
    pub backdrop: &'static str,
    pub ratio: f64,
    pub passes: bool,
}

/// Worst-case legibility of `text_primary` and `text_secondary` on each
/// surface drawn at `opacity` over a pure black and a pure white backdrop.
/// This is pessimistic: DWM Acrylic and Mica add blur, luminosity and tint
/// layers that reduce backdrop influence.
pub fn translucent_surface_checks(p: &Palette, opacity: f64) -> Vec<TranslucencyCheck> {
    let mut out = Vec::new();
    if opacity >= 1.0 {
        return out;
    }
    for (surface_name, surface) in [
        ("surface_base", p.surface_base),
        ("surface_raised", p.surface_raised),
        ("surface_overlay", p.surface_overlay),
    ] {
        for (backdrop_name, backdrop) in [("black", Color::BLACK), ("white", Color::WHITE)] {
            let effective = surface.with_opacity_over(opacity, backdrop);
            let ratio = contrast_ratio(p.text_primary, effective)
                .min(contrast_ratio(p.text_secondary, effective));
            out.push(TranslucencyCheck {
                surface: surface_name,
                backdrop: backdrop_name,
                ratio,
                passes: ratio + 1e-9 >= AA_TEXT,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_set_covers_required_pairs() {
        let r = rules();
        let has = |fg: &str, bg: &str, k: PairKind| {
            r.iter()
                .any(|((f, _), (b, _), kk)| *f == fg && *b == bg && *kk == k)
        };
        assert!(has("text_primary", "surface_base", PairKind::Text));
        assert!(has("text_muted", "surface_overlay", PairKind::Text));
        assert!(has("text_on_accent", "accent_pressed", PairKind::Text));
        assert!(has("border_control", "surface_base", PairKind::NonText));
        assert!(has("focus_ring", "surface_raised", PairKind::NonText));
        assert!(
            !r.iter()
                .any(|((f, _), _, _)| *f == "text_disabled" || *f == "border_subtle")
        );
    }
}
