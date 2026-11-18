//! Semantic validation of a parsed theme.
//!
//! Parsing (serde) guarantees structure and types. Validation checks values:
//! identifiers, ranges, ordering, font-weight limits, opacity rules and WCAG
//! contrast. Errors make a theme unusable; warnings are reported to the user
//! but do not block the theme.

use std::fmt;

use serde::Serialize;

use crate::contrast::{palette_checks, translucent_surface_checks};
use crate::schema::{Backdrop, Mode, Palette, THEME_SCHEMA_VERSION, Theme};

/// Heaviest font weight any theme may use.
pub const MAX_FONT_WEIGHT: u16 = 600;
/// Required weight for normal text.
pub const REGULAR_WEIGHT: u16 = 400;
/// Required weight for limited emphasis.
pub const EMPHASIS_WEIGHT: u16 = 500;
/// Families known to cover Hangul on Windows.
pub const HANGUL_CAPABLE_FAMILIES: &[&str] = &["Malgun Gothic", "맑은 고딕"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Issue {
    pub severity: Severity,
    /// Dotted token path, e.g. `palette.dark.text_muted`.
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ValidationReport {
    pub issues: Vec<Issue>,
}

impl ValidationReport {
    pub fn errors(&self) -> impl Iterator<Item = &Issue> {
        self.issues.iter().filter(|i| i.severity == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Issue> {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
    }

    pub fn has_errors(&self) -> bool {
        self.errors().next().is_some()
    }

    fn error(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.issues.push(Issue {
            severity: Severity::Error,
            path: path.into(),
            message: message.into(),
        });
    }

    fn warning(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.issues.push(Issue {
            severity: Severity::Warning,
            path: path.into(),
            message: message.into(),
        });
    }

    fn range<T: PartialOrd + fmt::Display + Copy>(&mut self, path: &str, v: T, lo: T, hi: T) {
        if !(lo..=hi).contains(&v) {
            self.error(path, format!("{v} is outside {lo}–{hi}"));
        }
    }
}

impl fmt::Display for ValidationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in &self.issues {
            let tag = match i.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            };
            writeln!(f, "  {tag}: {}: {}", i.path, i.message)?;
        }
        Ok(())
    }
}

fn is_printable_name(s: &str, max_chars: usize) -> bool {
    let t = s.trim();
    !t.is_empty()
        && t.len() == s.len()
        && s.chars().count() <= max_chars
        && !s.chars().any(char::is_control)
}

fn is_valid_id(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !s.starts_with('-')
        && !s.ends_with('-')
        && !s.contains("--")
}

fn is_semver(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 6
                && p.bytes().all(|b| b.is_ascii_digit())
                && (p.len() == 1 || !p.starts_with('0'))
        })
}

fn mode_name(m: Mode) -> &'static str {
    match m {
        Mode::Dark => "dark",
        Mode::Light => "light",
    }
}

/// Validates a parsed theme.
pub fn validate(theme: &Theme) -> ValidationReport {
    let mut r = ValidationReport::default();
    validate_meta(theme, &mut r);
    for (mode, palette) in theme.palette.iter() {
        validate_palette(
            mode,
            palette,
            f64::from(theme.materials.surface_opacity),
            &mut r,
        );
    }
    validate_typography(theme, &mut r);
    validate_layout(theme, &mut r);
    validate_materials_and_motion(theme, &mut r);
    validate_components(theme, &mut r);
    r
}

fn validate_meta(t: &Theme, r: &mut ValidationReport) {
    let m = &t.meta;
    if m.schema_version != THEME_SCHEMA_VERSION {
        r.error(
            "meta.schema_version",
            format!(
                "unsupported schema version {} (this build supports {THEME_SCHEMA_VERSION})",
                m.schema_version
            ),
        );
    }
    if !is_valid_id(&m.id) {
        r.error(
            "meta.id",
            "must be 1–64 characters of a–z, 0–9 and single hyphens",
        );
    }
    if !is_printable_name(&m.name, 64) {
        r.error(
            "meta.name",
            "must be 1–64 printable characters without surrounding spaces",
        );
    }
    if m.description.chars().count() > 280 || m.description.chars().any(char::is_control) {
        r.error(
            "meta.description",
            "must be at most 280 printable characters",
        );
    }
    if !is_printable_name(&m.author, 64) {
        r.error("meta.author", "must be 1–64 printable characters");
    }
    if !is_semver(&m.version) {
        r.error("meta.version", "must be MAJOR.MINOR.PATCH");
    }
    if m.tags.len() > 16 {
        r.error("meta.tags", "at most 16 tags");
    }
    for tag in &m.tags {
        if !is_valid_id(tag) || tag.len() > 32 {
            r.error("meta.tags", format!("invalid tag {tag:?}"));
        }
    }
    if t.palette.dark.is_none() && t.palette.light.is_none() {
        r.error(
            "palette",
            "at least one of palette.dark or palette.light is required",
        );
    } else if t.palette.get(m.default_mode).is_none() {
        r.error(
            "meta.default_mode",
            format!(
                "default mode is {} but palette.{} is missing",
                mode_name(m.default_mode),
                mode_name(m.default_mode)
            ),
        );
    }
}

fn validate_palette(mode: Mode, p: &Palette, opacity: f64, r: &mut ValidationReport) {
    let base = format!("palette.{}", mode_name(mode));
    let opaque = [
        ("background", p.background),
        ("surface_base", p.surface_base),
        ("surface_raised", p.surface_raised),
        ("surface_overlay", p.surface_overlay),
        ("text_primary", p.text_primary),
        ("text_secondary", p.text_secondary),
        ("text_muted", p.text_muted),
        ("text_disabled", p.text_disabled),
        ("text_on_accent", p.text_on_accent),
        ("accent", p.accent),
        ("accent_hover", p.accent_hover),
        ("accent_pressed", p.accent_pressed),
        ("selection", p.selection),
        ("focus_ring", p.focus_ring),
        ("success", p.success),
        ("warning", p.warning),
        ("danger", p.danger),
        ("info", p.info),
    ];
    for (name, c) in opaque {
        if !c.is_opaque() {
            r.error(
                format!("{base}.{name}"),
                format!(
                    "{c} must be opaque; translucency is controlled by materials.surface_opacity"
                ),
            );
        }
    }
    for check in palette_checks(p) {
        if !check.passes {
            r.error(
                format!("{base}.{}", check.foreground),
                format!(
                    "contrast {:.2}:1 on {} is below the WCAG AA minimum of {:.1}:1",
                    check.ratio, check.background, check.required
                ),
            );
        }
    }
    for t in translucent_surface_checks(p, opacity) {
        if !t.passes {
            r.warning(
                format!("{base}.{}", t.surface),
                format!(
                    "at surface_opacity {opacity:.2} over a {} backdrop, text contrast drops to {:.2}:1; \
                     legibility depends on the wallpaper and on the renderer's tint layer",
                    t.backdrop, t.ratio
                ),
            );
        }
    }
}

fn validate_typography(t: &Theme, r: &mut ValidationReport) {
    let ty = &t.typography;
    for (path, name) in [
        ("typography.family", &ty.family),
        ("typography.mono_family", &ty.mono_family),
    ] {
        if !is_printable_name(name, 64) {
            r.error(path, "must be 1–64 printable characters");
        }
    }
    for (path, list) in [
        ("typography.fallback", &ty.fallback),
        ("typography.mono_fallback", &ty.mono_fallback),
    ] {
        if list.len() > 8 {
            r.error(path, "at most 8 fallback families");
        }
        if list.iter().any(|f| !is_printable_name(f, 64)) {
            r.error(path, "contains an invalid family name");
        }
    }
    let covers_hangul = std::iter::once(&ty.family).chain(&ty.fallback).any(|f| {
        HANGUL_CAPABLE_FAMILIES
            .iter()
            .any(|h| h.eq_ignore_ascii_case(f))
    });
    if !covers_hangul {
        r.warning(
            "typography.fallback",
            "no Hangul-capable family (e.g. Malgun Gothic); Korean text may not render",
        );
    }

    for (path, w) in [
        ("typography.weight_regular", ty.weight_regular),
        ("typography.weight_emphasis", ty.weight_emphasis),
        ("typography.weight_strong", ty.weight_strong),
    ] {
        if w > MAX_FONT_WEIGHT {
            r.error(
                path,
                format!("weight {w} exceeds the maximum of {MAX_FONT_WEIGHT}"),
            );
        } else if w < 100 {
            r.error(path, format!("weight {w} is below 100"));
        }
    }
    if ty.weight_regular != REGULAR_WEIGHT {
        r.error(
            "typography.weight_regular",
            format!("normal text must use weight {REGULAR_WEIGHT}"),
        );
    }
    if ty.weight_emphasis != EMPHASIS_WEIGHT {
        r.error(
            "typography.weight_emphasis",
            format!("limited emphasis must use weight {EMPHASIS_WEIGHT}"),
        );
    }
    if ty.weight_strong < ty.weight_emphasis {
        r.error(
            "typography.weight_strong",
            "must be at least weight_emphasis",
        );
    }
    if !ty.line_height.is_finite() || !(1.3..=1.8).contains(&ty.line_height) {
        r.error(
            "typography.line_height",
            "must be 1.3–1.8 (lower values clip Hangul glyphs)",
        );
    }
    let s = &ty.sizes;
    r.range("typography.sizes.caption", s.caption, 11, 16);
    r.range("typography.sizes.body", s.body, 12, 18);
    r.range("typography.sizes.display", s.display, 20, 48);
    let ordered = [
        s.caption,
        s.body,
        s.body_large,
        s.subtitle,
        s.title,
        s.display,
    ];
    if ordered.windows(2).any(|w| w[0] > w[1]) {
        r.error(
            "typography.sizes",
            "sizes must not decrease from caption to display",
        );
    }
}

fn validate_layout(t: &Theme, r: &mut ValidationReport) {
    let sp = &t.spacing;
    if ![2, 4, 8].contains(&sp.unit) {
        r.error("spacing.unit", "must be 2, 4 or 8");
    }
    let steps = [
        ("xs", sp.xs),
        ("sm", sp.sm),
        ("md", sp.md),
        ("lg", sp.lg),
        ("xl", sp.xl),
        ("xxl", sp.xxl),
    ];
    if steps.windows(2).any(|w| w[0].1 >= w[1].1) {
        r.error("spacing", "steps must strictly increase from xs to xxl");
    }
    for (name, v) in steps {
        if sp.unit != 0 && v % sp.unit != 0 {
            r.error(
                format!("spacing.{name}"),
                format!("{v} is not a multiple of unit {}", sp.unit),
            );
        }
    }
    r.range("spacing.xxl", sp.xxl, 8, 64);

    let rd = &t.radius;
    for (name, v) in [
        ("control", rd.control),
        ("menu", rd.menu),
        ("card", rd.card),
        ("panel", rd.panel),
        ("dock", rd.dock),
        ("window", rd.window),
    ] {
        r.range(&format!("radius.{name}"), v, 0, 32);
    }
    r.range("shadow.offset_y", t.shadow.offset_y, 0, 32);
    r.range("shadow.blur", t.shadow.blur, 0, 64);
}

fn validate_materials_and_motion(t: &Theme, r: &mut ValidationReport) {
    let m = &t.materials;
    if !m.surface_opacity.is_finite() || !(0.3..=1.0).contains(&m.surface_opacity) {
        r.error("materials.surface_opacity", "must be 0.3–1.0");
    } else {
        if m.backdrop == Backdrop::None && m.surface_opacity < 1.0 {
            r.error(
                "materials.surface_opacity",
                "must be 1.0 when backdrop is none (unblurred see-through panels are unreadable)",
            );
        }
        if m.surface_opacity < 0.6 {
            r.warning(
                "materials.surface_opacity",
                "below 0.6; text legibility depends heavily on the wallpaper",
            );
        }
    }

    let mo = &t.motion;
    if !(mo.duration_fast_ms <= mo.duration_normal_ms
        && mo.duration_normal_ms <= mo.duration_slow_ms)
    {
        r.error("motion", "durations must satisfy fast <= normal <= slow");
    }
    r.range("motion.duration_slow_ms", mo.duration_slow_ms, 0, 600);
    let [x1, y1, x2, y2] = mo.easing;
    if mo.easing.iter().any(|v| !v.is_finite())
        || !(0.0..=1.0).contains(&x1)
        || !(0.0..=1.0).contains(&x2)
    {
        r.error("motion.easing", "x control points must be within 0–1");
    }
    if !(-1.0..=2.0).contains(&y1) || !(-1.0..=2.0).contains(&y2) {
        r.error("motion.easing", "y control points must be within -1–2");
    }
}

fn validate_components(t: &Theme, r: &mut ValidationReport) {
    let c = &t.components;
    r.range("components.bar.height", c.bar.height, 24, 64);
    r.range("components.bar.margin", c.bar.margin, 0, 32);
    r.range("components.bar.padding_x", c.bar.padding_x, 0, 48);
    if !c.bar.floating && c.bar.margin != 0 {
        r.error(
            "components.bar.margin",
            "an attached bar must have margin 0",
        );
    }

    r.range("components.dock.icon_size", c.dock.icon_size, 24, 96);
    r.range("components.dock.spacing", c.dock.spacing, 0, 32);
    r.range("components.dock.padding", c.dock.padding, 0, 32);
    let scale = c.dock.magnification_scale;
    if !scale.is_finite() || !(1.0..=2.0).contains(&scale) {
        r.error("components.dock.magnification_scale", "must be 1.0–2.0");
    } else if !c.dock.magnification && (scale - 1.0).abs() > f32::EPSILON {
        r.warning(
            "components.dock.magnification_scale",
            "has no effect while magnification is off",
        );
    }

    r.range("components.launcher.width", c.launcher.width, 400, 1200);
    r.range(
        "components.launcher.max_results",
        c.launcher.max_results,
        3,
        20,
    );
    r.range(
        "components.launcher.top_offset_percent",
        c.launcher.top_offset_percent,
        0,
        60,
    );
    r.range("components.widget.padding", c.widget.padding, 0, 48);
    r.range("components.widget.gap", c.widget.gap, 0, 48);
    r.range(
        "components.window.border_width",
        c.window.border_width,
        0,
        4,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers() {
        for ok in ["windowsrice-default", "a", "theme-2"] {
            assert!(is_valid_id(ok), "{ok}");
        }
        for bad in [
            "",
            "Default",
            "-a",
            "a-",
            "a--b",
            "a_b",
            "a b",
            "테마",
            &"a".repeat(65),
        ] {
            assert!(!is_valid_id(bad), "{bad}");
        }
    }

    #[test]
    fn versions() {
        assert!(is_semver("1.0.0") && is_semver("10.20.30"));
        for bad in ["1", "1.0", "1.0.0.0", "01.0.0", "1.a.0", "", "1..0"] {
            assert!(!is_semver(bad), "{bad}");
        }
    }

    #[test]
    fn printable_names() {
        assert!(is_printable_name("windowsRICE Default", 64));
        assert!(is_printable_name("나의 테마", 64));
        assert!(!is_printable_name(" padded", 64));
        assert!(!is_printable_name("tab\tname", 64));
        assert!(!is_printable_name("", 64));
    }
}
