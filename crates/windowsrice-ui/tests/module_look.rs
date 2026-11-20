//! Phase 9 per-module appearance: every option changes what the bar draws
//! for that module only, and unavailable readings stay unavailable.

use windowsrice_core::bar::{
    BarStyle, Density, ModuleConfig, ModuleContent, SpeedUnit, ValueFormat, default_modules,
};
use windowsrice_core::config::{ThemeModePreference, ThemeSelection};
use windowsrice_theme::Color;
use windowsrice_theme::catalog::ThemeCatalog;
use windowsrice_theme::engine::{NoopSink, ThemeEngine};
use windowsrice_theme::schema::Mode;
use windowsrice_ui::bar::{BarModel, BarScene, Decor, build_bar};
use windowsrice_ui::draw::{DrawCmd, FontRole};
use windowsrice_ui::format::LocalTime;
use windowsrice_ui::geom::Size;
use windowsrice_ui::legibility::{BackplateMode, SurfaceRequest, plan};
use windowsrice_ui::module::{BarReadings, Reading, Traffic};
use windowsrice_ui::panel::MemoryState;
use windowsrice_ui::strings::{Lang, Strings};
use windowsrice_ui::style::{PanelStyle, module_contrast};
use windowsrice_ui::text::ApproxMeasure;

const GB: u64 = 1024 * 1024 * 1024;

fn readings() -> BarReadings {
    BarReadings {
        time: Some(LocalTime {
            year: 2026,
            month: 11,
            day: 20,
            weekday: 5,
            hour: 9,
            minute: 7,
        }),
        active_window: Some("Documents".into()),
        cpu: Reading::Ok(12.0),
        gpu: Reading::Ok(30.0),
        vram: Reading::Ok(MemoryState {
            used_bytes: 4 * GB,
            total_bytes: 16 * GB,
        }),
        ram: Reading::Ok(MemoryState {
            used_bytes: 12 * GB + GB / 2,
            total_bytes: 32 * GB,
        }),
        traffic: Reading::Ok(Traffic {
            up_bytes_per_s: 125_000.0,
            down_bytes_per_s: 2_500_000.0,
        }),
        ..Default::default()
    }
}

fn scene(modules: Vec<ModuleConfig>, r: BarReadings) -> BarScene {
    let sel = ThemeSelection {
        active: "windowsrice-default".into(),
        mode: ThemeModePreference::Dark,
    };
    let (engine, _) = ThemeEngine::start(
        ThemeCatalog::load(None),
        &sel,
        Some(Mode::Dark),
        &mut NoopSink,
    );
    let t = engine.applied().unwrap().clone();
    let p = plan(
        &SurfaceRequest {
            backdrop: t.theme.materials.backdrop,
            theme_opacity: 1.0,
            opacity_override: None,
            backplates: BackplateMode::Auto,
            reduced_transparency: false,
            backdrop_available: true,
            high_contrast: false,
        },
        &t.palette,
    );
    build_bar(
        &BarModel {
            modules,
            readings: r,
            decor: Decor::from_style(&BarStyle::default()),
            ..Default::default()
        },
        &PanelStyle::from_theme(&t, None, true),
        &p,
        &ApproxMeasure,
        Size { w: 3000.0, h: 34.0 },
        1.0,
        &Strings::new(Lang::En),
        4.0,
    )
}

/// Texts drawn for one module, with their colour and size.
fn texts(s: &BarScene, id: &str) -> Vec<(String, Color, f32, FontRole)> {
    let el = s
        .elements
        .iter()
        .find(|(m, _)| m == id)
        .map(|(_, e)| *e)
        .unwrap();
    s.scene
        .draw
        .cmds
        .iter()
        .filter_map(|c| match c {
            DrawCmd::Text {
                text,
                color,
                style,
                owner,
                ..
            } if *owner == Some(el) => Some((text.clone(), *color, style.size, style.font)),
            _ => None,
        })
        .collect()
}

fn with(id: &str, f: impl Fn(&mut ModuleConfig)) -> Vec<ModuleConfig> {
    let mut ms = default_modules();
    f(ms.iter_mut().find(|m| m.id == id).unwrap());
    ms
}

#[test]
fn colour_background_size_and_density_apply_to_one_module_only() {
    let base = scene(default_modules(), readings());
    let ms = with("cpu", |m| {
        m.color = Some("#36D1C4".into());
        m.background = Some("#202830".into());
        m.font_size = Some(16);
        m.density = Some(Density::Expanded);
    });
    let s = scene(ms, readings());
    let cpu = texts(&s, "cpu");
    let value = cpu.iter().find(|t| t.0 == "12%").unwrap();
    assert_eq!(
        value.1,
        Color::rgb(0x36, 0xD1, 0xC4),
        "value in the module accent"
    );
    assert_eq!(value.2, 16.0, "module text size");
    // A tile in the module background colour sits under the module.
    let el = s.elements.iter().find(|(m, _)| m == "cpu").unwrap().1;
    assert!(s.scene.draw.cmds.iter().any(|c| matches!(c,
        DrawCmd::Fill { color, owner, .. } if *owner == Some(el) && *color == Color::rgb(0x20, 0x28, 0x30))));
    // Wider than before (bigger text, more padding); other modules unchanged.
    let w = |s: &BarScene, id: &str| {
        let e = s.elements.iter().find(|(m, _)| m == id).unwrap().1;
        s.scene.tree.get(e).unwrap().bounds.w
    };
    assert!(w(&s, "cpu") > w(&base, "cpu") + 10.0);
    assert_eq!(w(&s, "ram"), w(&base, "ram"));
    assert_eq!(texts(&s, "ram"), texts(&base, "ram"));
}

#[test]
fn content_units_and_formats() {
    let ms = with("cpu", |m| m.content = Some(ModuleContent::IconOnly));
    let s = scene(ms, readings());
    let cpu = texts(&s, "cpu");
    assert_eq!(cpu.len(), 1, "icon only: {cpu:?}");
    assert_eq!(cpu[0].3, FontRole::Icon);
    // RAM as an amount, download in bits, clock without the date.
    let mut ms = default_modules();
    for m in &mut ms {
        match m.id.as_str() {
            "ram" => m.value_format = Some(ValueFormat::Amount),
            "download" => m.speed_unit = Some(SpeedUnit::Bits),
            "clock" => m.show_date = Some(false),
            _ => {}
        }
    }
    let s = scene(ms.clone(), readings());
    assert!(
        texts(&s, "ram").iter().any(|t| t.0 == "12.5 GB"),
        "{:?}",
        texts(&s, "ram")
    );
    assert!(
        texts(&s, "download").iter().any(|t| t.0 == "20.0 Mb/s"),
        "{:?}",
        texts(&s, "download")
    );
    let clock = texts(&s, "clock");
    assert_eq!(clock.len(), 1, "time only: {clock:?}");
    // Unavailable memory stays unavailable, even as an amount.
    let mut r = readings();
    r.ram = Reading::Unavailable("no counter".into());
    let s = scene(ms, r);
    assert!(
        texts(&s, "ram").iter().all(|t| !t.0.contains("GB")),
        "nothing invented: {:?}",
        texts(&s, "ram")
    );
}

#[test]
fn module_options_validate_and_round_trip() {
    let mut b = windowsrice_core::bar::BarConfig::default();
    let cpu = b.modules.iter_mut().find(|m| m.id == "cpu").unwrap();
    cpu.color = Some("#36D1C4".into());
    cpu.font_size = Some(16);
    cpu.content = Some(ModuleContent::Full);
    assert!(b.problems().is_empty(), "{:?}", b.problems());
    let mut bad = b.clone();
    let cpu = bad.modules.iter_mut().find(|m| m.id == "cpu").unwrap();
    cpu.value_format = Some(ValueFormat::Amount);
    cpu.speed_unit = Some(SpeedUnit::Bits);
    cpu.font_size = Some(40);
    cpu.background = Some("teal".into());
    let p = bad.problems().join("\n");
    for needle in ["value_format", "speed_unit", "font_size", "#RRGGBB"] {
        assert!(p.contains(needle), "{needle} missing in:\n{p}");
    }
    let c = windowsrice_core::config::Config {
        bar: b,
        ..Default::default()
    };
    let t = c.to_toml_string().unwrap();
    assert!(t.contains("color = \"#36D1C4\""));
    let back =
        windowsrice_core::config::Config::from_toml_str(&t, std::path::Path::new("config.toml"))
            .unwrap();
    assert_eq!(back.bar, c.bar);
}

#[test]
fn unreadable_module_colours_are_reported() {
    let sel = ThemeSelection {
        active: "windowsrice-default".into(),
        mode: ThemeModePreference::Dark,
    };
    let (engine, _) = ThemeEngine::start(
        ThemeCatalog::load(None),
        &sel,
        Some(Mode::Dark),
        &mut NoopSink,
    );
    let t = engine.applied().unwrap();
    let ms = with("cpu", |m| m.color = Some("#20232A".into()));
    let low: Vec<_> = module_contrast(t, &ms)
        .into_iter()
        .filter(|(_, c)| *c < 4.5)
        .collect();
    assert_eq!(low.len(), 1, "{low:?}");
    let ms = with("cpu", |m| m.color = Some("#E8EAEE".into()));
    assert!(module_contrast(t, &ms).iter().all(|(_, c)| *c >= 4.5));
}
