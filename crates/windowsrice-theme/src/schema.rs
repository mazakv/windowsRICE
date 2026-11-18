//! Theme schema (version 1).
//!
//! A theme is a complete set of design tokens. Every section is required;
//! there are no silent defaults, so an incomplete theme fails to parse with a
//! message naming the missing token. Unknown keys are rejected.

use serde::{Deserialize, Serialize};

use crate::color::Color;

/// Current theme schema version.
pub const THEME_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    pub meta: ThemeMeta,
    pub palette: Palettes,
    pub typography: Typography,
    pub spacing: Spacing,
    pub radius: Radius,
    pub shadow: Shadow,
    pub materials: Materials,
    pub motion: Motion,
    pub components: Components,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeMeta {
    pub schema_version: u32,
    /// Stable identifier: lowercase ASCII letters, digits and hyphens.
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    /// Theme version, `MAJOR.MINOR.PATCH`.
    pub version: String,
    pub default_mode: Mode,
    pub accent_source: AccentSource,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Dark,
    Light,
}

/// Where the accent color comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccentSource {
    /// The palette's `accent` tokens.
    Theme,
    /// Derived from the wallpaper (planned). Until extraction is implemented,
    /// and whenever an extracted color fails contrast checks, the palette's
    /// accent tokens are used.
    Wallpaper,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Palettes {
    pub dark: Option<Palette>,
    pub light: Option<Palette>,
}

impl Palettes {
    pub fn get(&self, mode: Mode) -> Option<&Palette> {
        match mode {
            Mode::Dark => self.dark.as_ref(),
            Mode::Light => self.light.as_ref(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (Mode, &Palette)> {
        [
            (Mode::Dark, self.dark.as_ref()),
            (Mode::Light, self.light.as_ref()),
        ]
        .into_iter()
        .filter_map(|(m, p)| p.map(|p| (m, p)))
    }
}

/// Color tokens for one mode.
///
/// Surface and text tokens must be opaque. `border_subtle`, `border_control`
/// and `shadow` may carry alpha; they are composited onto the surface they are
/// drawn on before contrast is measured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Palette {
    /// Desktop fallback behind all windowsRICE surfaces.
    pub background: Color,
    /// Default panel surface (bar, dock).
    pub surface_base: Color,
    /// Cards and widgets.
    pub surface_raised: Color,
    /// Menus, popups and the launcher.
    pub surface_overlay: Color,
    /// Decorative separators. Exempt from contrast requirements.
    pub border_subtle: Color,
    /// Boundaries of interactive controls. Needs 3:1 against the surface.
    pub border_control: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    /// Metadata and hints. Still informational, so it must meet 4.5:1.
    pub text_muted: Color,
    /// Disabled controls. Exempt from contrast requirements (WCAG 1.4.3).
    pub text_disabled: Color,
    pub text_on_accent: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_pressed: Color,
    /// Selected-item background; `text_primary` is drawn on it.
    pub selection: Color,
    pub focus_ring: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub info: Color,
    pub shadow: Color,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Typography {
    /// Primary UI family. Must be an installed Windows font.
    pub family: String,
    /// Fallback families in order. Should include a Hangul-capable font.
    pub fallback: Vec<String>,
    pub mono_family: String,
    pub mono_fallback: Vec<String>,
    /// Normal text. Must be 400.
    pub weight_regular: u16,
    /// Limited emphasis (selected controls, section titles). Must be 500.
    pub weight_emphasis: u16,
    /// Strongest weight the theme may use. 500–600.
    pub weight_strong: u16,
    /// Line height as a multiple of the font size.
    pub line_height: f32,
    pub sizes: TypeScale,
}

/// Font sizes in pixels at 100% display scaling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeScale {
    pub caption: u16,
    pub body: u16,
    pub body_large: u16,
    pub subtitle: u16,
    pub title: u16,
    pub display: u16,
}

/// Spacing scale in pixels at 100% scaling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spacing {
    pub unit: u16,
    pub xs: u16,
    pub sm: u16,
    pub md: u16,
    pub lg: u16,
    pub xl: u16,
    pub xxl: u16,
}

/// Corner radii in pixels at 100% scaling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Radius {
    pub control: u16,
    pub menu: u16,
    pub card: u16,
    /// Top bar (ignored when the bar is attached to the screen edge).
    pub panel: u16,
    pub dock: u16,
    /// windowsRICE-owned windows (Settings, Theme Studio).
    pub window: u16,
}

/// Single-level drop shadow. Color comes from `palette.*.shadow`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shadow {
    pub offset_y: u16,
    pub blur: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Materials {
    pub backdrop: Backdrop,
    /// Opacity of surface colors over the backdrop (0.3–1.0). Ignored in
    /// Gaming Mode and reduced-transparency mode, where surfaces are solid.
    pub surface_opacity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backdrop {
    /// Solid surfaces.
    None,
    /// DWM Mica (wallpaper-tinted, no live blur).
    Mica,
    /// DWM Acrylic (blurred backdrop).
    Acrylic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Motion {
    pub level: MotionLevel,
    pub duration_fast_ms: u16,
    pub duration_normal_ms: u16,
    pub duration_slow_ms: u16,
    /// Cubic Bézier control points `[x1, y1, x2, y2]`.
    pub easing: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MotionLevel {
    Full,
    /// Short fades only; no movement.
    Reduced,
    Off,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Components {
    pub bar: BarTokens,
    pub dock: DockTokens,
    pub launcher: LauncherTokens,
    pub widget: WidgetTokens,
    pub window: WindowTokens,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BarPosition {
    Top,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarTokens {
    pub position: BarPosition,
    pub height: u16,
    /// Floating bars are inset by `margin`; attached bars touch the edge.
    pub floating: bool,
    pub margin: u16,
    pub padding_x: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DockPosition {
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DockHide {
    Never,
    Always,
    /// Hide when a window overlaps the dock.
    Smart,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DockTokens {
    pub position: DockPosition,
    pub icon_size: u16,
    pub spacing: u16,
    pub padding: u16,
    pub auto_hide: DockHide,
    pub magnification: bool,
    /// Peak icon scale when magnification is on (1.0–2.0).
    pub magnification_scale: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LauncherTokens {
    pub width: u16,
    pub max_results: u8,
    /// Distance of the launcher's top edge from the top of the work area.
    pub top_offset_percent: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WidgetTokens {
    pub padding: u16,
    pub gap: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowCorner {
    /// Let Windows decide.
    Default,
    Round,
    RoundSmall,
    Square,
}

/// Appearance of windowsRICE-owned windows. Applied to other applications'
/// windows only where Windows documents support (see the compatibility matrix).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowTokens {
    pub border_width: u8,
    pub corner: WindowCorner,
    pub accent_border_on_focus: bool,
}
