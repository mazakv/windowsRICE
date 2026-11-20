//! Colour animations (Phase 12–14): keyframed colours for the bar's text,
//! accent, icons, clock or background gradient, including rainbow text.
//!
//! An animation is declarative data: a list of keyframes (position 0–1 and
//! colour), a duration, an interpolation and a direction. It is evaluated
//! by [`ColorAnimation::color_at`]; nothing is executed. Limits keep
//! animations gentle: at most [`MAX_KEYFRAMES`] keyframes, a cycle of at
//! least [`MIN_DURATION_MS`] (no flashing: well under three changes per
//! second) and at most [`MAX_ANIMATIONS`] animations per bar.

use serde::{Deserialize, Serialize};

use crate::bar::parse_hex_color;

pub const MAX_ANIMATIONS: usize = 4;
pub const MAX_KEYFRAMES: usize = 16;
/// Shortest cycle, milliseconds.
pub const MIN_DURATION_MS: u32 = 1000;
/// Longest cycle, milliseconds.
pub const MAX_DURATION_MS: u32 = 120_000;

/// What an animation colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimTarget {
    /// Primary text of every module.
    Text,
    /// Text drawn with a moving multi-colour gradient across the bar
    /// (rainbow text): the keyframes are the gradient's colours and the
    /// gradient scrolls once per cycle.
    TextGradient,
    Accent,
    Icons,
    Clock,
    /// The bar background gradient: the keyframes are its colours and the
    /// gradient scrolls once per cycle.
    Background,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Interpolation {
    Linear,
    /// Ease in and out between keyframes.
    #[default]
    Smooth,
    /// Holds each keyframe's colour until the next one.
    Step,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimDirection {
    /// 0 → 1, then starts again at 0 (the last keyframe blends into the
    /// first, so a loop has no jump when the first and last colours match
    /// or when [`ColorAnimation::wrap`] is on).
    #[default]
    Normal,
    Reverse,
    /// 0 → 1 → 0.
    Alternate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keyframe {
    /// Position in the cycle, 0.0–1.0.
    pub at: f32,
    /// `#RRGGBB`.
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorAnimation {
    pub target: AnimTarget,
    pub keyframes: Vec<Keyframe>,
    pub duration_ms: u32,
    #[serde(default)]
    pub interpolation: Interpolation,
    #[serde(default)]
    pub direction: AnimDirection,
    /// Blend from the last keyframe back to the first at the end of a
    /// cycle (a seamless loop). Default on.
    #[serde(default = "yes")]
    pub wrap: bool,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

/// An RGB colour as floats 0–255.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb(pub f32, pub f32, pub f32);

impl Rgb {
    pub fn to_u8(self) -> (u8, u8, u8) {
        let c = |v: f32| v.round().clamp(0.0, 255.0) as u8;
        (c(self.0), c(self.1), c(self.2))
    }
}

fn rgb(hex: &str) -> Option<Rgb> {
    parse_hex_color(hex).map(|(r, g, b)| Rgb(f32::from(r), f32::from(g), f32::from(b)))
}

fn lerp(a: Rgb, b: Rgb, k: f32) -> Rgb {
    Rgb(
        a.0 + (b.0 - a.0) * k,
        a.1 + (b.1 - a.1) * k,
        a.2 + (b.2 - a.2) * k,
    )
}

impl ColorAnimation {
    /// A rainbow across the text (seven hues, 8-second cycle).
    pub fn rainbow_text() -> Self {
        let hues = [
            "#FF5A5A", "#FFA94D", "#FFE066", "#69DB7C", "#4DABF7", "#9775FA", "#F783AC",
        ];
        let n = hues.len() as f32;
        Self {
            target: AnimTarget::TextGradient,
            keyframes: hues
                .iter()
                .enumerate()
                .map(|(i, h)| Keyframe {
                    at: i as f32 / n,
                    color: (*h).to_owned(),
                })
                .collect(),
            duration_ms: 8000,
            interpolation: Interpolation::Linear,
            direction: AnimDirection::Normal,
            wrap: true,
            enabled: true,
        }
    }

    /// Every problem, as messages naming the field under `path`.
    pub fn problems(&self, path: &str) -> Vec<String> {
        let mut p = Vec::new();
        if self.keyframes.is_empty() {
            p.push(format!("{path}.keyframes needs at least one keyframe"));
        }
        if self.keyframes.len() > MAX_KEYFRAMES {
            p.push(format!(
                "{path}.keyframes has {} keyframes; at most {MAX_KEYFRAMES} are allowed",
                self.keyframes.len()
            ));
        }
        let mut last = -1.0f32;
        for (i, k) in self.keyframes.iter().enumerate() {
            if !(0.0..=1.0).contains(&k.at) || !k.at.is_finite() {
                p.push(format!(
                    "{path}.keyframes[{i}].at = {} is outside 0.0-1.0",
                    k.at
                ));
            } else if k.at < last {
                p.push(format!(
                    "{path}.keyframes[{i}].at = {} is before the previous keyframe; keyframes must be in order",
                    k.at
                ));
            }
            last = k.at;
            if parse_hex_color(&k.color).is_none() {
                p.push(format!(
                    "{path}.keyframes[{i}].color = {:?} is not a #RRGGBB colour",
                    k.color
                ));
            }
        }
        if self.duration_ms < MIN_DURATION_MS {
            p.push(format!(
                "{path}.duration_ms = {} is shorter than {MIN_DURATION_MS} (fast colour changes can trigger photosensitive reactions)",
                self.duration_ms
            ));
        }
        if self.duration_ms > MAX_DURATION_MS {
            p.push(format!(
                "{path}.duration_ms = {} is longer than {MAX_DURATION_MS}",
                self.duration_ms
            ));
        }
        p
    }

    /// Position 0–1 in the keyframes after direction is applied, for a time
    /// in milliseconds since the animation started.
    pub fn phase(&self, t_ms: u64) -> f32 {
        let d = u64::from(self.duration_ms.max(1));
        let raw = (t_ms % d) as f32 / d as f32;
        match self.direction {
            AnimDirection::Normal => raw,
            AnimDirection::Reverse => 1.0 - raw,
            AnimDirection::Alternate => {
                let cycle = (t_ms / d) % 2;
                if cycle == 0 { raw } else { 1.0 - raw }
            }
        }
    }

    /// The colour at position `p` (0–1) of the cycle. Invalid keyframe
    /// colours are skipped; `None` when no keyframe is valid.
    pub fn color_at_phase(&self, p: f32) -> Option<Rgb> {
        let ks: Vec<(f32, Rgb)> = self
            .keyframes
            .iter()
            .filter_map(|k| rgb(&k.color).map(|c| (k.at.clamp(0.0, 1.0), c)))
            .collect();
        let first = *ks.first()?;
        if ks.len() == 1 {
            return Some(first.1);
        }
        let last = *ks.last()?;
        let p = p.clamp(0.0, 1.0);
        // Segment containing p; with wrap, the segment after the last
        // keyframe blends back into the first (one cycle later).
        let (a, b, span_start, span_len) = if p < first.0 {
            if self.wrap {
                let len = 1.0 - last.0 + first.0;
                (last, first, last.0 - 1.0, len)
            } else {
                return Some(first.1);
            }
        } else if p >= last.0 {
            if self.wrap {
                let len = 1.0 - last.0 + first.0;
                (last, first, last.0, len)
            } else {
                return Some(last.1);
            }
        } else {
            let i = ks.iter().rposition(|k| k.0 <= p).unwrap_or(0);
            let (a, b) = (ks[i], ks[i + 1]);
            (a, b, a.0, b.0 - a.0)
        };
        if span_len <= f32::EPSILON {
            return Some(b.1);
        }
        let k = ((p - span_start) / span_len).clamp(0.0, 1.0);
        let k = match self.interpolation {
            Interpolation::Linear => k,
            Interpolation::Smooth => k * k * (3.0 - 2.0 * k),
            Interpolation::Step => 0.0,
        };
        Some(lerp(a.1, b.1, k))
    }

    /// The colour at `t_ms` milliseconds after the animation started.
    pub fn color_at(&self, t_ms: u64) -> Option<Rgb> {
        self.color_at_phase(self.phase(t_ms))
    }
}

/// Problems of a bar's animation list.
pub fn animation_problems(list: &[ColorAnimation], path: &str) -> Vec<String> {
    let mut p = Vec::new();
    if list.len() > MAX_ANIMATIONS {
        p.push(format!(
            "{path} has {} animations; at most {MAX_ANIMATIONS} are allowed",
            list.len()
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for (i, a) in list.iter().enumerate() {
        if !seen.insert(a.target) {
            p.push(format!("{path}[{i}] animates {:?} a second time", a.target));
        }
        p.extend(a.problems(&format!("{path}[{i}]")));
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anim(keys: &[(f32, &str)]) -> ColorAnimation {
        ColorAnimation {
            target: AnimTarget::Text,
            keyframes: keys
                .iter()
                .map(|(at, c)| Keyframe {
                    at: *at,
                    color: (*c).to_owned(),
                })
                .collect(),
            duration_ms: 4000,
            interpolation: Interpolation::Linear,
            direction: AnimDirection::Normal,
            wrap: false,
            enabled: true,
        }
    }

    #[test]
    fn interpolates_between_keyframes() {
        let a = anim(&[(0.0, "#000000"), (1.0, "#FF0000")]);
        assert_eq!(a.color_at_phase(0.0).unwrap().to_u8(), (0, 0, 0));
        assert_eq!(a.color_at_phase(0.5).unwrap().to_u8(), (128, 0, 0));
        assert_eq!(a.color_at_phase(1.0).unwrap().to_u8(), (255, 0, 0));
        assert_eq!(a.color_at(2000).unwrap().to_u8(), (128, 0, 0));
    }

    #[test]
    fn step_and_smooth() {
        let mut a = anim(&[(0.0, "#000000"), (0.5, "#FFFFFF"), (1.0, "#000000")]);
        a.interpolation = Interpolation::Step;
        assert_eq!(a.color_at_phase(0.4).unwrap().to_u8(), (0, 0, 0));
        assert_eq!(a.color_at_phase(0.6).unwrap().to_u8(), (255, 255, 255));
        a.interpolation = Interpolation::Smooth;
        // Smoothstep is symmetric: the middle of a segment is the midpoint.
        assert_eq!(a.color_at_phase(0.25).unwrap().to_u8(), (128, 128, 128));
        assert!(a.color_at_phase(0.05).unwrap().0 < 0.05 * 2.0 * 255.0 * 0.5);
    }

    #[test]
    fn wrap_makes_a_seamless_loop() {
        let mut a = anim(&[(0.0, "#FF0000"), (0.5, "#0000FF")]);
        a.wrap = true;
        // Halfway between the last keyframe (0.5) and the first one again
        // (1.0): purple, and the end of the cycle equals its start.
        assert_eq!(a.color_at_phase(0.75).unwrap().to_u8(), (128, 0, 128));
        let (end, start) = (
            a.color_at_phase(0.999).unwrap(),
            a.color_at_phase(0.0).unwrap(),
        );
        assert!(
            (end.0 - start.0).abs() < 2.0 && (end.2 - start.2).abs() < 2.0,
            "{end:?} vs {start:?}"
        );
    }

    #[test]
    fn directions() {
        let mut a = anim(&[(0.0, "#000000"), (1.0, "#FFFFFF")]);
        a.direction = AnimDirection::Reverse;
        assert!((a.phase(1000) - 0.75).abs() < 1e-6);
        a.direction = AnimDirection::Alternate;
        assert!((a.phase(1000) - 0.25).abs() < 1e-6);
        assert!((a.phase(5000) - 0.75).abs() < 1e-6);
    }

    #[test]
    fn limits_are_enforced() {
        let mut a = anim(&[(0.5, "#000000"), (0.2, "red")]);
        a.duration_ms = 200;
        let p = a.problems("bar.style.animations[0]");
        assert!(p.iter().any(|m| m.contains("in order")), "{p:?}");
        assert!(p.iter().any(|m| m.contains("#RRGGBB")), "{p:?}");
        assert!(p.iter().any(|m| m.contains("photosensitive")), "{p:?}");
        let many = vec![ColorAnimation::rainbow_text(); MAX_ANIMATIONS + 1];
        let p = animation_problems(&many, "a");
        assert!(p.iter().any(|m| m.contains("at most")), "{p:?}");
        assert!(p.iter().any(|m| m.contains("second time")), "{p:?}");
        assert!(ColorAnimation::rainbow_text().problems("x").is_empty());
    }
}
