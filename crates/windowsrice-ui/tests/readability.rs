//! Readability across theme, mode and manual colours (Phase 12): switching
//! to a light (or dark) theme or mode must give readable text, secondary
//! text and icons on every surface, whatever colours the previous design
//! left behind. Pure unit tests.

use windowsrice_core::bar::{BarConfig, Gradient, GradientDirection, ModuleShape, default_modules};
use windowsrice_core::config::{ThemeModePreference, ThemeSelection};
use windowsrice_theme::Color;
use windowsrice_theme::catalog::ThemeCatalog;
use windowsrice_theme::color::contrast_ratio;
use windowsrice_theme::schema::Mode;
use windowsrice_ui::bar::{BarModel, Decor, build_bar};
use windowsrice_ui::draw::{DrawCmd, FontRole};
use windowsrice_ui::geom::Size;
use windowsrice_ui::legibility::{BackplateMode, SurfaceRequest, plan};
use windowsrice_ui::module::BarReadings;
use windowsrice_ui::strings::{Lang, Strings};
use windowsrice_ui::studio::Draft;
use windowsrice_ui::studio::catalog::{self, Ctx, Gallery};
use windowsrice_ui::style::{
    PanelStyle, min_contrast, readability_fixes, readable, resolve_bar_theme, text_surfaces,
};
use windowsrice_ui::text::ApproxMeasure;

fn ctx() -> Ctx<'static> {
    Ctx {
        background: Color::rgb(0x20, 0x20, 0x20),
        accent: Color::rgb(0x4C, 0xC2, 0xFF),
        text: Color::rgb(0xF0, 0xF0, 0xF0),
        fonts: &[],
    }
}

fn draft() -> Draft {
    Draft {
        theme: ThemeSelection::default(),
        bar: BarConfig::default(),
    }
}

/// Every text draw command of the real bar scene, checked against the
/// opaque fill drawn under it (or every bar surface when none is).
fn assert_scene_readable(cat: &ThemeCatalog, d: &Draft, what: &str) {
    let t = resolve_bar_theme(cat, &d.theme, &d.bar.style, Some(Mode::Dark)).unwrap();
    let surfaces = text_surfaces(t.palette.surface_base, &d.bar.style);
    let style = PanelStyle::from_theme(&t, None, true);
    let p = plan(
        &SurfaceRequest {
            backdrop: windowsrice_theme::schema::Backdrop::None,
            theme_opacity: 1.0,
            opacity_override: None,
            backplates: BackplateMode::Auto,
            reduced_transparency: false,
            backdrop_available: true,
            high_contrast: false,
        },
        &t.palette,
    );
    let scene = build_bar(
        &BarModel {
            modules: default_modules(),
            readings: BarReadings::default(),
            decor: Decor::from_style(&d.bar.style),
            ..Default::default()
        },
        &style,
        &p,
        &ApproxMeasure,
        Size {
            w: 1600.0,
            h: style.height,
        },
        1.0,
        &Strings::new(Lang::En),
        4.0,
    );
    let mut fills: Vec<(windowsrice_ui::geom::Rect, Color)> = Vec::new();
    for cmd in &scene.scene.draw.cmds {
        match cmd {
            DrawCmd::Fill { rect, color, .. } if color.a == 255 => fills.push((*rect, *color)),
            DrawCmd::Text {
                rect,
                style: ts,
                color,
                text,
                ..
            } if *color != style.text_disabled => {
                let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
                let under = fills
                    .iter()
                    .rev()
                    .find(|(r, _)| cx >= r.x && cx <= r.x + r.w && cy >= r.y && cy <= r.y + r.h)
                    .map(|(_, c)| *c);
                let need = if ts.font == FontRole::Icon { 3.0 } else { 4.5 };
                let got = match under {
                    Some(bg) => contrast_ratio(color.over(bg), bg),
                    None => min_contrast(*color, &surfaces),
                };
                assert!(
                    got + 1e-6 >= need,
                    "{what}: {text:?} contrast {got:.2} < {need} (colour {color:?}, under {under:?})"
                );
            }
            _ => {}
        }
    }
}

#[test]
fn every_style_switched_to_light_or_dark_mode_stays_readable() {
    let cat = ThemeCatalog::load(None);
    let n = catalog::items(Gallery::Style).len();
    assert!(n >= 15);
    for i in 0..n {
        for mode in [ThemeModePreference::Light, ThemeModePreference::Dark] {
            let mut d = draft();
            assert!(catalog::apply(Gallery::Style, i, &mut d, &ctx()));
            // The Studio's mode switch changes only the mode.
            d.theme.mode = mode;
            let t = resolve_bar_theme(&cat, &d.theme, &d.bar.style, Some(Mode::Dark)).unwrap();
            let surfaces = text_surfaces(t.palette.surface_base, &d.bar.style);
            for (role, c) in [
                ("text", t.palette.text_primary),
                ("secondary", t.palette.text_secondary),
            ] {
                let got = min_contrast(c, &surfaces);
                assert!(got + 1e-6 >= 4.5, "style {i} in {mode:?}: {role} {got:.2}");
            }
            assert_scene_readable(&cat, &d, &format!("style {i} in {mode:?}"));
        }
    }
}

#[test]
fn light_theme_after_a_dark_gradient_uses_readable_text_and_reports_it() {
    // The reported problem: a dark design's gradient and clock colour stay
    // when the light theme is chosen through the mode switch.
    let cat = ThemeCatalog::load(None);
    let mut d = draft();
    d.theme.active = "windowsrice-light".into();
    d.theme.mode = ThemeModePreference::Light;
    d.bar.style.gradient = Some(Gradient {
        direction: GradientDirection::Vertical,
        colors: vec!["#151B33".into(), "#0B1020".into()],
    });
    d.bar.style.clock_color = Some("#1A1C20".into());
    let t = resolve_bar_theme(&cat, &d.theme, &d.bar.style, Some(Mode::Light)).unwrap();
    let surfaces = text_surfaces(t.palette.surface_base, &d.bar.style);
    assert!(min_contrast(t.palette.text_primary, &surfaces) >= 4.5);
    let fixes = readability_fixes(&t_without_style(&cat, &d), &d.bar.style);
    assert!(
        fixes.iter().any(|(role, _, _)| *role == "text"),
        "{fixes:?}"
    );
    assert!(
        fixes.iter().any(|(role, _, _)| *role == "clock"),
        "{fixes:?}"
    );
    assert_scene_readable(&cat, &d, "light theme under a dark gradient");
}

fn t_without_style(cat: &ThemeCatalog, d: &Draft) -> windowsrice_theme::engine::ResolvedTheme {
    resolve_bar_theme(cat, &d.theme, &Default::default(), Some(Mode::Light)).unwrap()
}

#[test]
fn readable_colours_are_kept_exactly() {
    // A light theme with its own dark text: nothing changes, and a manual
    // colour that already passes is not touched.
    let cat = ThemeCatalog::load(None);
    let mut d = draft();
    d.theme.active = "windowsrice-light".into();
    d.theme.mode = ThemeModePreference::Light;
    let plain = resolve_bar_theme(&cat, &d.theme, &Default::default(), Some(Mode::Light)).unwrap();
    let t = resolve_bar_theme(&cat, &d.theme, &d.bar.style, Some(Mode::Light)).unwrap();
    assert_eq!(t.palette.text_primary, plain.palette.text_primary);
    assert!(readability_fixes(&plain, &d.bar.style).is_empty());
    let bg = [Color::rgb(0xF5, 0xF6, 0xF8)];
    let blue = Color::rgb(0x00, 0x4E, 0x9A);
    assert_eq!(readable(blue, &bg, 4.5), blue);
    // The light theme's foreground is dark (not hardcoded black).
    assert!(t.palette.text_primary != Color::rgb(0, 0, 0));
    assert!(contrast_ratio(t.palette.text_primary, t.palette.surface_base) >= 4.5);
}

#[test]
fn readable_keeps_the_hue_when_it_can() {
    let dark_bg = [Color::rgb(0x10, 0x10, 0x18)];
    let dim_red = Color::rgb(0x80, 0x10, 0x10);
    let r = readable(dim_red, &dark_bg, 4.5);
    assert!(min_contrast(r, &dark_bg) >= 4.5);
    assert!(r.r > r.g && r.r > r.b, "still reddish: {r:?}");
    // Light text on a light tile becomes dark enough.
    let light_bg = [Color::rgb(0xF0, 0xF0, 0xF0)];
    let r = readable(Color::rgb(0xE0, 0xE0, 0xE0), &light_bg, 4.5);
    assert!(min_contrast(r, &light_bg) >= 4.5);
}

#[test]
fn module_tiles_count_as_text_surfaces() {
    let s = windowsrice_core::bar::BarStyle {
        module_shape: ModuleShape::Rounded,
        module_background: Some("#FFFFFF".into()),
        ..Default::default()
    };
    let sf = text_surfaces(Color::rgb(0, 0, 0), &s);
    assert_eq!(
        sf.len(),
        1,
        "tiles are fitted when drawn, not a bar surface"
    );
    let tile = windowsrice_ui::style::tile_surface(Color::rgb(0, 0, 0), &s).unwrap();
    assert_eq!(tile, Color::rgb(255, 255, 255));
}
