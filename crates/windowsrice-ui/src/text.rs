//! Text measurement interface. The renderer implements it with DirectWrite;
//! tests use [`ApproxMeasure`].

use crate::draw::{FontRole, TextStyle};
use crate::geom::Size;

pub trait TextMeasure {
    /// Size of `text` laid out on one line, in DIPs.
    fn measure(&self, text: &str, style: &TextStyle) -> Size;
}

/// Deterministic approximation for tests: Latin glyphs are 0.55 em wide,
/// Hangul and other wide characters 1 em, icon glyphs 1 em.
#[derive(Debug, Default, Clone, Copy)]
pub struct ApproxMeasure;

impl TextMeasure for ApproxMeasure {
    fn measure(&self, text: &str, style: &TextStyle) -> Size {
        let em = style.size;
        let w: f32 = text
            .chars()
            .map(|c| match style.font {
                FontRole::Icon => em,
                FontRole::Ui | FontRole::Family(_) if is_wide(c) => em,
                FontRole::Ui | FontRole::Family(_) => em * 0.55,
            })
            .sum();
        Size { w, h: em * 1.33 }
    }
}

/// Hangul, CJK and full-width forms.
pub fn is_wide(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF | 0x2E80..=0x303F | 0x3130..=0x318F | 0x3400..=0x9FFF
        | 0xA960..=0xA97F | 0xAC00..=0xD7FF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF)
}
