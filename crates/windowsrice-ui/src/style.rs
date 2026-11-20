//! Panel style derived from the applied theme. There is no separate theme
//! system: every value comes from the Phase 3 design tokens, except when
//! Windows high-contrast mode is on, where Windows' own system colours win.

use windowsrice_theme::Color;
use windowsrice_theme::engine::ResolvedTheme;
use windowsrice_theme::schema::{BarPosition, MotionLevel};

/// Windows high-contrast system colours (GetSysColor), supplied by the
/// platform layer when high-contrast mode is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighContrastColors {
    pub window: Color,
    pub window_text: Color,
    pub highlight: Color,
    pub highlight_text: Color,
    pub gray_text: Color,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PanelStyle {
    pub theme_id: String,
    pub theme_name: String,
    /// Opaque surface colour (`surface_base`).
    pub surface: Color,
    /// Opaque popup and menu surface (`surface_overlay`).
    pub overlay: Color,
    /// Boundaries of interactive controls (`border_control`, 3:1).
    pub border: Color,
    /// Text drawn on the accent colour.
    pub text_on_accent: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_disabled: Color,
    pub accent: Color,
    pub focus_ring: Color,
    /// Hover highlight drawn over the surface.
    pub hover: Color,
    /// Background of an expanded or pressed control.
    pub pressed: Color,
    pub success: Color,
    pub warning: Color,
    /// UI font family and its fallback chain.
    pub family: String,
    pub fallback: Vec<String>,
    pub icon_family: &'static str,
    pub icon_fallback: &'static str,
    pub size_body: f32,
    pub size_icon: f32,
    pub weight_regular: u16,
    pub weight_emphasis: u16,
    /// Panel height and spacing in DIPs.
    pub height: f32,
    pub margin: f32,
    pub padding_x: f32,
    pub gap: f32,
    pub gap_small: f32,
    pub radius_control: f32,
    pub floating: bool,
    pub position: BarPosition,
    /// Fade duration for showing the panel; 0 disables the fade.
    pub fade_ms: u16,
    pub high_contrast: bool,
}

/// Fluent icon font shipped with Windows 11.
pub const ICON_FAMILY: &str = "Segoe Fluent Icons";
/// Windows 10 icon font, used if Segoe Fluent Icons is missing.
pub const ICON_FALLBACK: &str = "Segoe MDL2 Assets";

/// Maximum weight allowed anywhere in windowsRICE.
pub const MAX_WEIGHT: u16 = 600;

impl PanelStyle {
    /// `system_animations` is Windows' "Animation effects" setting.
    pub fn from_theme(
        t: &ResolvedTheme,
        high_contrast: Option<&HighContrastColors>,
        system_animations: bool,
    ) -> Self {
        let p = &t.palette;
        let th = &t.theme;
        let bar = &th.components.bar;
        let ty = &th.typography;
        let fade_ms = match th.motion.level {
            MotionLevel::Off => 0,
            _ if !system_animations => 0,
            // Reduced motion still allows short fades.
            MotionLevel::Reduced | MotionLevel::Full => th.motion.duration_fast_ms,
        };
        let mut s = PanelStyle {
            theme_id: t.id.clone(),
            theme_name: t.name.clone(),
            surface: Color {
                a: 255,
                ..p.surface_base
            },
            overlay: Color {
                a: 255,
                ..p.surface_overlay
            },
            border: p.border_control,
            text_on_accent: p.text_on_accent,
            text_primary: p.text_primary,
            text_secondary: p.text_secondary,
            text_disabled: p.text_disabled,
            accent: p.accent,
            focus_ring: p.focus_ring,
            hover: Color {
                a: 0x1F,
                ..p.text_primary
            },
            pressed: p.selection,
            success: p.success,
            warning: p.warning,
            family: ty.family.clone(),
            fallback: ty.fallback.clone(),
            icon_family: ICON_FAMILY,
            icon_fallback: ICON_FALLBACK,
            size_body: f32::from(ty.sizes.body),
            size_icon: f32::from(ty.sizes.body) + 2.0,
            weight_regular: ty.weight_regular.min(MAX_WEIGHT),
            weight_emphasis: ty.weight_emphasis.min(MAX_WEIGHT),
            height: f32::from(bar.height),
            margin: f32::from(bar.margin),
            padding_x: f32::from(bar.padding_x),
            gap: f32::from(th.spacing.md),
            gap_small: f32::from(th.spacing.xs),
            radius_control: f32::from(th.radius.control),
            floating: bar.floating,
            position: bar.position,
            fade_ms,
            high_contrast: false,
        };
        if let Some(hc) = high_contrast {
            s.high_contrast = true;
            s.surface = hc.window;
            s.overlay = hc.window;
            s.border = hc.window_text;
            s.text_on_accent = hc.highlight_text;
            s.text_primary = hc.window_text;
            s.text_secondary = hc.window_text;
            s.text_disabled = hc.gray_text;
            s.accent = hc.highlight;
            s.focus_ring = hc.window_text;
            s.hover = hc.highlight;
            s.pressed = hc.highlight;
            s.success = hc.window_text;
            s.warning = hc.window_text;
        }
        s
    }
}

/// Applies the bar's Design Studio overrides to a resolved theme, so every
/// downstream rule (style, legibility plan, validation) sees the colours
/// and typography actually drawn. Invalid colours are ignored (the
/// configuration validator rejects them before they get here).
pub fn with_bar_style(t: &ResolvedTheme, s: &windowsrice_core::bar::BarStyle) -> ResolvedTheme {
    use windowsrice_core::bar::parse_hex_color;
    let mut r = t.clone();
    let col = |h: &Option<String>| {
        h.as_deref()
            .and_then(parse_hex_color)
            .map(|(r, g, b)| Color::rgb(r, g, b))
    };
    let p = &mut r.palette;
    if let Some(bg) = col(&s.background) {
        p.surface_base = bg;
        p.surface_raised = bg;
        p.background = bg;
    }
    if let Some(a) = col(&s.accent) {
        p.accent = a;
        p.accent_hover = a;
        p.accent_pressed = a;
        p.focus_ring = a;
    }
    if let Some(tx) = col(&s.text) {
        p.text_primary = tx;
        // Secondary text: the chosen colour at 78 % over the surface.
        p.text_secondary = Color { a: 199, ..tx }.over(p.surface_base);
    }
    if let Some(c) = col(&s.text_secondary) {
        p.text_secondary = c;
    }
    if let Some(c) = col(&s.focus_color) {
        p.focus_ring = c;
    }
    if let Some(c) = col(&s.border_color) {
        p.border_control = c;
    }
    // Readability (Phase 12): the text roles must stay readable on every
    // opaque surface they can sit on, whatever mix of theme, mode and manual
    // colours produced them (for example a light theme under a dark
    // gradient kept from an earlier style). A failing role is replaced by
    // the nearest readable colour; the configuration is not changed.
    let surfaces = text_surfaces(p.surface_base, s);
    p.text_primary = readable(p.text_primary, &surfaces, 4.5);
    p.text_secondary = readable(p.text_secondary, &surfaces, 4.5);
    if let Some(o) = s.opacity {
        r.theme.materials.surface_opacity = o;
    }
    let ty = &mut r.theme.typography;
    if let Some(f) = &s.font_family {
        ty.family.clone_from(f);
    }
    if let Some(sz) = s.font_size {
        ty.sizes.body = sz;
    }
    if let Some(w) = s.font_weight {
        ty.weight_regular = w;
        ty.weight_emphasis = ty.weight_emphasis.max(w);
    }
    if let Some(x) = s.padding_x {
        r.theme.components.bar.padding_x = x;
    }
    r.palette = p.clone();
    if let Some(pal) = match r.mode {
        windowsrice_theme::Mode::Dark => r.theme.palette.dark.as_mut(),
        windowsrice_theme::Mode::Light => r.theme.palette.light.as_mut(),
    } {
        *pal = r.palette.clone();
    }
    r
}

/// Backdrop kind and surface opacity of the bar for `bar.material`. The
/// running bar and the Design Studio preview both use this.
/// `Backdrop::None` means opaque; anything else means translucent, which
/// the bar draws itself with per-pixel alpha (no system backdrop).
pub fn bar_surface(
    t: &ResolvedTheme,
    bar: &windowsrice_core::bar::BarStyle,
) -> (windowsrice_theme::schema::Backdrop, f32) {
    use windowsrice_core::bar::Material;
    use windowsrice_theme::schema::Backdrop;
    let m = &t.theme.materials;
    match bar.material {
        Material::Theme => (m.backdrop, m.surface_opacity),
        Material::Solid => (Backdrop::None, 1.0),
        Material::Translucent => (
            Backdrop::Acrylic,
            if m.surface_opacity < 1.0 {
                m.surface_opacity
            } else {
                0.80
            },
        ),
    }
}

/// Resolves the theme of a selection the way the running bar does (mode
/// from the preference and the system, then `[bar.style]` overrides).
/// Falls back to the catalog's default theme when the id is unknown.
pub fn resolve_bar_theme(
    cat: &windowsrice_theme::catalog::ThemeCatalog,
    sel: &windowsrice_core::config::ThemeSelection,
    bar: &windowsrice_core::bar::BarStyle,
    system_mode: Option<windowsrice_theme::Mode>,
) -> Option<ResolvedTheme> {
    let e = cat.get(&sel.active).or_else(|| cat.default_entry())?;
    let (mode, _) = windowsrice_theme::engine::resolve_mode(&e.theme, sel.mode, system_mode);
    let palette = e.theme.palette.get(mode)?.clone();
    let r = ResolvedTheme {
        id: e.id().to_owned(),
        name: e.theme.meta.name.clone(),
        mode,
        version: e.version.clone(),
        source: e.source.clone(),
        palette,
        theme: e.theme.clone(),
    };
    Some(with_bar_style(&r, bar))
}

/// Every text colour of the bar against every opaque surface it can sit on
/// (the background, each gradient colour and the module background), as
/// (description, ratio). The Studio refuses ratios below 4.5:1.
pub fn design_contrast(
    t: &ResolvedTheme,
    s: &windowsrice_core::bar::BarStyle,
) -> Vec<(String, f64)> {
    use windowsrice_core::bar::parse_hex_color;
    use windowsrice_theme::color::contrast_ratio;
    let col = |h: &str| parse_hex_color(h).map(|(r, g, b)| Color::rgb(r, g, b));
    let p = &t.palette;
    let opaque = |c: Color| Color { a: 255, ..c };
    // A gradient of two or more colours replaces the background.
    let stops: Vec<Color> = s
        .gradient
        .iter()
        .flat_map(|g| g.colors.iter().filter_map(|c| col(c)))
        .collect();
    let mut surfaces = if stops.len() >= 2 {
        stops
            .iter()
            .enumerate()
            .map(|(i, c)| (format!("gradient colour {}", i + 1), *c))
            .collect()
    } else {
        vec![("background".to_owned(), opaque(p.surface_base))]
    };
    if s.module_shape != windowsrice_core::bar::ModuleShape::None
        && let Some(c) = s.module_background.as_deref().and_then(col)
    {
        surfaces.push(("module background".to_owned(), c));
    }
    let mut texts = vec![
        ("text".to_owned(), p.text_primary),
        ("secondary text".to_owned(), p.text_secondary),
    ];
    // Clock and value colours are drawn corrected for readability, so the
    // drawn colour is what is measured.
    let visible: Vec<Color> = surfaces.iter().map(|(_, c)| *c).collect();
    for (name, v) in [("clock", &s.clock_color), ("values", &s.value_color)] {
        if let Some(c) = v.as_deref().and_then(col) {
            texts.push((name.to_owned(), readable(c, &visible, 4.5)));
        }
    }
    let mut out = Vec::new();
    for (sn, sc) in &surfaces {
        for (tn, tc) in &texts {
            out.push((format!("{tn} on {sn}"), contrast_ratio(tc.over(*sc), *sc)));
        }
    }
    out
}

/// Contrast of each module's own colours (Phase 9): its accent on its tile
/// (or on the bar surface), and the bar's text on its tile.
pub fn module_contrast(
    t: &ResolvedTheme,
    modules: &[windowsrice_core::bar::ModuleConfig],
) -> Vec<(String, f64)> {
    use windowsrice_core::bar::parse_hex_color;
    use windowsrice_theme::color::contrast_ratio;
    let col = |h: &Option<String>| {
        h.as_deref()
            .and_then(parse_hex_color)
            .map(|(r, g, b)| Color::rgb(r, g, b))
    };
    let surface = Color {
        a: 255,
        ..t.palette.surface_base
    };
    let mut out = Vec::new();
    for m in modules.iter().filter(|m| m.enabled) {
        let tile = col(&m.background);
        let under = tile.unwrap_or(surface);
        if let Some(c) = col(&m.color) {
            out.push((format!("module {} colour", m.id), contrast_ratio(c, under)));
        }
        if tile.is_some() && m.color.is_none() {
            out.push((
                format!("module {} text on its background", m.id),
                contrast_ratio(t.palette.text_primary, under),
            ));
        }
    }
    out
}

/// Contrast of primary and secondary text on the opaque surface: below
/// WCAG AA 4.5:1 the configuration is refused.
pub fn opaque_text_contrast(t: &ResolvedTheme) -> f64 {
    use windowsrice_theme::color::contrast_ratio;
    let p = &t.palette;
    let surface = Color {
        a: 255,
        ..p.surface_base
    };
    contrast_ratio(p.text_primary, surface).min(contrast_ratio(p.text_secondary, surface))
}

/// The opaque surfaces text can be drawn on: the bar background (or, when
/// a gradient of two or more colours replaces it, every gradient colour)
/// and, when modules have tiles, the module background.
pub fn text_surfaces(base: Color, s: &windowsrice_core::bar::BarStyle) -> Vec<Color> {
    use windowsrice_core::bar::parse_hex_color;
    let col = |h: &str| parse_hex_color(h).map(|(r, g, b)| Color::rgb(r, g, b));
    let stops: Vec<Color> = s
        .gradient
        .iter()
        .flat_map(|g| g.colors.iter().filter_map(|c| col(c)))
        .collect();
    let mut out = if stops.len() >= 2 {
        stops
    } else {
        vec![Color { a: 255, ..base }]
    };
    if s.module_shape != windowsrice_core::bar::ModuleShape::None
        && let Some(c) = s.module_background.as_deref().and_then(col)
    {
        out.push(c);
    }
    out
}

/// Lowest contrast of `c` (composited over each surface) on `surfaces`.
pub fn min_contrast(c: Color, surfaces: &[Color]) -> f64 {
    use windowsrice_theme::color::contrast_ratio;
    surfaces
        .iter()
        .map(|s| contrast_ratio(c.over(*s), *s))
        .fold(f64::INFINITY, f64::min)
}

/// `c` if it reaches `need` contrast on every surface; otherwise the
/// closest colour that does, found by moving `c` towards white or towards
/// black (whichever needs the smaller change, so the hue is kept where
/// possible). When no mix reaches `need` (surfaces both very light and very
/// dark), the mix with the best worst-case contrast is used.
pub fn readable(c: Color, surfaces: &[Color], need: f64) -> Color {
    let c = Color {
        a: 255,
        ..c.over(surfaces.first().copied().unwrap_or(c))
    };
    if surfaces.is_empty() || min_contrast(c, surfaces) + 1e-9 >= need {
        return c;
    }
    let mix = |to: Color, k: f32| {
        let m = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * k).round() as u8;
        Color::rgb(m(c.r, to.r), m(c.g, to.g), m(c.b, to.b))
    };
    let mut best = c;
    let mut best_score = min_contrast(c, surfaces);
    for step in 1..=20 {
        let k = step as f32 / 20.0;
        for to in [Color::rgb(255, 255, 255), Color::rgb(0, 0, 0)] {
            let cand = mix(to, k);
            let score = min_contrast(cand, surfaces);
            if score + 1e-9 >= need {
                return cand;
            }
            if score > best_score {
                best = cand;
                best_score = score;
            }
        }
    }
    best
}

/// The readability corrections `with_bar_style` makes for this design, as
/// (role, chosen colour, corrected colour). The Design Studio shows them so
/// a correction is never silent.
pub fn readability_fixes(
    t: &ResolvedTheme,
    s: &windowsrice_core::bar::BarStyle,
) -> Vec<(&'static str, Color, Color)> {
    // The colours before correction (the override, else the theme's role),
    // compared with what is drawn.
    let raw = |h: &Option<String>, fallback: Color| {
        h.as_deref()
            .and_then(windowsrice_core::bar::parse_hex_color)
            .map_or(fallback, |(r, g, b)| Color::rgb(r, g, b))
    };
    let fixed = with_bar_style(t, s);
    let surfaces = text_surfaces(fixed.palette.surface_base, s);
    let mut out = Vec::new();
    let primary = raw(&s.text, t.palette.text_primary);
    if min_contrast(primary, &surfaces) + 1e-9 < 4.5 {
        out.push(("text", primary, fixed.palette.text_primary));
    }
    for (role, v, need) in [
        ("secondary text", &s.text_secondary, 4.5),
        ("clock", &s.clock_color, 4.5),
        ("values", &s.value_color, 4.5),
        ("icons", &s.icon_color, 3.0),
    ] {
        if let Some(c) = v
            .as_deref()
            .and_then(windowsrice_core::bar::parse_hex_color)
        {
            let c = Color::rgb(c.0, c.1, c.2);
            let r = readable(c, &surfaces, need);
            if r != c {
                out.push((role, c, r));
            }
        }
    }
    out
}
