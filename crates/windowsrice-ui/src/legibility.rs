//! Surface legibility policy for translucent themes.
//!
//! A theme's palette contrast is measured on opaque colours. Over a live
//! Acrylic or Mica backdrop the colour behind text depends on the wallpaper
//! and on the windows underneath, so palette contrast alone does not
//! guarantee readable text. This module decides, at render time and without
//! changing any setting, how a surface is drawn:
//!
//! - **Reduced transparency** (Windows "Transparency effects" off, an
//!   explicit override, or high-contrast mode) and an **unavailable
//!   backdrop** both give a solid, opaque surface.
//! - Otherwise the surface is drawn at the theme's `surface_opacity`, or at
//!   a user-supplied override clamped to 0.30–1.00.
//! - If the pessimistic worst case (pure black or pure white directly behind
//!   the surface) would put text below WCAG AA 4.5:1, **solid text
//!   backplates** are drawn behind every text element (in `Auto` mode). A
//!   solid backplate restores the opaque palette contrast regardless of the
//!   backdrop, so it does not depend on how DWM blends the material.
//! - With backplates turned off on a surface that fails the worst case, the
//!   plan says so (`meets_aa_worst_case == false`) and the panel reports a
//!   warning instead of claiming accessible contrast.

use windowsrice_theme::Palette;
use windowsrice_theme::color::contrast_ratio;
use windowsrice_theme::contrast::AA_TEXT;
use windowsrice_theme::schema::Backdrop;

/// Lowest surface opacity a user override may request.
pub const MIN_OPACITY: f32 = 0.30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackplateMode {
    /// Draw backplates only when the worst case fails AA.
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceRequest {
    /// Material requested by the theme.
    pub backdrop: Backdrop,
    /// `materials.surface_opacity` from the theme.
    pub theme_opacity: f32,
    /// Runtime override (command line or future settings), not persisted.
    pub opacity_override: Option<f32>,
    pub backplates: BackplateMode,
    /// Windows "Transparency effects" is off, or reduced transparency was
    /// requested explicitly.
    pub reduced_transparency: bool,
    /// False when DWM refused the backdrop attribute.
    pub backdrop_available: bool,
    pub high_contrast: bool,
}

/// Why the surface is drawn the way it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceReason {
    Translucent,
    ThemeOpaque,
    ReducedTransparency,
    BackdropUnavailable,
    HighContrast,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfacePlan {
    /// Material to request from DWM.
    pub backdrop: Backdrop,
    /// Alpha of the surface fill.
    pub surface_opacity: f32,
    /// Draw a solid surface-coloured backplate behind each text element.
    pub backplates: bool,
    pub reason: SurfaceReason,
    /// Worst-case contrast of primary and secondary text over the surface
    /// without backplates (pure black or white behind it).
    pub worst_case_without_backplates: f64,
    /// Worst-case contrast of text as actually drawn.
    pub worst_case_text_contrast: f64,
    pub meets_aa_worst_case: bool,
    /// Lowest surface opacity that meets AA in the worst case without
    /// backplates (diagnostic; 1.0 if only an opaque surface does).
    pub min_opacity_for_aa: f32,
}

/// Worst-case contrast of `text_primary` and `text_secondary` on
/// `surface_base` drawn at `opacity` over pure black and pure white.
pub fn worst_case_contrast(p: &Palette, opacity: f32) -> f64 {
    let mut worst = f64::INFINITY;
    for backdrop in [
        windowsrice_theme::Color::BLACK,
        windowsrice_theme::Color::WHITE,
    ] {
        let bg = p
            .surface_base
            .with_opacity_over(f64::from(opacity), backdrop);
        for text in [p.text_primary, p.text_secondary] {
            worst = worst.min(contrast_ratio(text, bg));
        }
    }
    worst
}

/// Smallest opacity (to 1/100) whose worst case meets AA.
pub fn min_opacity_for_aa(p: &Palette) -> f32 {
    (30..=100)
        .map(|v| v as f32 / 100.0)
        .find(|&o| worst_case_contrast(p, o) + 1e-9 >= AA_TEXT)
        .unwrap_or(1.0)
}

pub fn plan(req: &SurfaceRequest, palette: &Palette) -> SurfacePlan {
    let solid_reason = if req.high_contrast {
        Some(SurfaceReason::HighContrast)
    } else if req.reduced_transparency {
        Some(SurfaceReason::ReducedTransparency)
    } else if req.backdrop == Backdrop::None {
        Some(SurfaceReason::ThemeOpaque)
    } else if !req.backdrop_available {
        Some(SurfaceReason::BackdropUnavailable)
    } else {
        None
    };
    let min_opacity = min_opacity_for_aa(palette);
    if let Some(reason) = solid_reason {
        let c = worst_case_contrast(palette, 1.0);
        return SurfacePlan {
            backdrop: Backdrop::None,
            surface_opacity: 1.0,
            backplates: false,
            reason,
            worst_case_without_backplates: c,
            worst_case_text_contrast: c,
            meets_aa_worst_case: c + 1e-9 >= AA_TEXT,
            min_opacity_for_aa: min_opacity,
        };
    }
    let opacity = req
        .opacity_override
        .unwrap_or(req.theme_opacity)
        .clamp(MIN_OPACITY, 1.0);
    let without = worst_case_contrast(palette, opacity);
    let fails = without + 1e-9 < AA_TEXT;
    let backplates = match req.backplates {
        BackplateMode::Always => true,
        BackplateMode::Never => false,
        BackplateMode::Auto => fails,
    };
    // A solid backplate shows the opaque surface colour behind text.
    let with = if backplates {
        worst_case_contrast(palette, 1.0)
    } else {
        without
    };
    SurfacePlan {
        backdrop: req.backdrop,
        surface_opacity: opacity,
        backplates,
        reason: SurfaceReason::Translucent,
        worst_case_without_backplates: without,
        worst_case_text_contrast: with,
        meets_aa_worst_case: with + 1e-9 >= AA_TEXT,
        min_opacity_for_aa: min_opacity,
    }
}
