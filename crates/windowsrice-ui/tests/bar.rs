//! Full-width bar and settings popup: structure, layout, overflow,
//! accessibility state, keyboard behaviour and module actions. Pure unit
//! tests (no Windows APIs).

use std::collections::HashMap;

use windowsrice_core::bar::{ModuleConfig, ModuleKind, Region, default_modules};
use windowsrice_core::config::{ThemeModePreference, ThemeSelection};
use windowsrice_theme::catalog::ThemeCatalog;
use windowsrice_theme::engine::{NoopSink, ResolvedTheme, ThemeEngine};
use windowsrice_theme::schema::Mode;
use windowsrice_ui::bar::{BarModel, build_bar, ids, menu_element};
use windowsrice_ui::draw::DrawCmd;
use windowsrice_ui::format::LocalTime;
use windowsrice_ui::geom::Size;
use windowsrice_ui::keyboard::{Key, KeyOutcome, Modifiers, Orientation, handle_key_oriented};
use windowsrice_ui::legibility::{BackplateMode, SurfacePlan, SurfaceRequest, plan};
use windowsrice_ui::module::{BarReadings, BatteryState, NotificationState, Reading, Traffic};
use windowsrice_ui::panel::{AudioState, LinkKind, MemoryState, NetworkState};
use windowsrice_ui::popup::{
    ModuleStatus, PopupAction, PopupModel, apply_module_action, build_popup, step_interval,
};
use windowsrice_ui::strings::{Lang, Strings};
use windowsrice_ui::style::PanelStyle;
use windowsrice_ui::text::ApproxMeasure;
use windowsrice_ui::tree::{Pattern, Role};

fn theme(id: &str, mode: Mode) -> ResolvedTheme {
    let selection = ThemeSelection {
        active: id.to_owned(),
        mode: match mode {
            Mode::Dark => ThemeModePreference::Dark,
            Mode::Light => ThemeModePreference::Light,
        },
    };
    let (engine, _) = ThemeEngine::start(
        ThemeCatalog::load(None),
        &selection,
        Some(mode),
        &mut NoopSink,
    );
    engine.applied().unwrap().clone()
}

fn fixture(id: &str) -> (PanelStyle, SurfacePlan) {
    let t = theme(id, Mode::Dark);
    let p = plan(
        &SurfaceRequest {
            backdrop: t.theme.materials.backdrop,
            theme_opacity: t.theme.materials.surface_opacity,
            opacity_override: None,
            backplates: BackplateMode::Auto,
            reduced_transparency: false,
            backdrop_available: true,
            high_contrast: false,
        },
        &t.palette,
    );
    (PanelStyle::from_theme(&t, None, true), p)
}

fn readings() -> BarReadings {
    BarReadings {
        time: Some(LocalTime {
            year: 2026,
            month: 11,
            day: 19,
            weekday: 4,
            hour: 15,
            minute: 42,
        }),
        active_window: Some("문서 — 파일 탐색기".into()),
        cpu: Reading::Ok(12.4),
        gpu: Reading::Ok(3.0),
        vram: Reading::Ok(MemoryState {
            used_bytes: 2 << 30,
            total_bytes: 16 << 30,
        }),
        ram: Reading::Ok(MemoryState {
            used_bytes: 13 << 30,
            total_bytes: 32 << 30,
        }),
        audio: Reading::Ok(AudioState {
            volume_percent: 40,
            muted: false,
        }),
        network: Reading::Ok(NetworkState::Internet(LinkKind::Wired)),
        traffic: Reading::Ok(Traffic {
            up_bytes_per_s: 1_500.0,
            down_bytes_per_s: 2_400_000.0,
        }),
        battery: Reading::Absent,
        notifications: Reading::Ok(NotificationState::Accepting),
        warnings: Vec::new(),
    }
}

fn bar(
    width: f32,
    scale: f32,
    modules: Vec<ModuleConfig>,
    lang: Lang,
) -> windowsrice_ui::bar::BarScene {
    let (style, p) = fixture("windowsrice-default");
    build_bar(
        &BarModel {
            modules,
            readings: readings(),
            ..Default::default()
        },
        &style,
        &p,
        &ApproxMeasure,
        Size {
            w: width,
            h: style.height,
        },
        scale,
        &Strings::new(lang),
        4.0,
    )
}

#[test]
fn surface_spans_full_width_even_without_modules() {
    let only_menu: Vec<ModuleConfig> = default_modules()
        .into_iter()
        .filter(|m| m.kind == ModuleKind::Menu)
        .collect();
    for modules in [default_modules(), only_menu] {
        let b = bar(2560.0, 1.5, modules, Lang::En);
        match &b.scene.draw.cmds[0] {
            DrawCmd::Fill { rect, .. } => {
                assert_eq!((rect.x, rect.y, rect.w), (0.0, 0.0, 2560.0));
            }
            other => panic!("first command must be the surface, got {other:?}"),
        }
        assert!(
            b.scene.tree.problems().is_empty(),
            "{:#?}",
            b.scene.tree.problems()
        );
    }
}

#[test]
fn default_modules_structure_and_names() {
    let b = bar(2560.0, 1.5, default_modules(), Lang::En);
    let t = &b.scene.tree;
    assert_eq!(t.root().role, Role::ToolBar);
    assert_eq!(t.root().name, "windowsRICE bar");
    assert_eq!(
        t.children(ids::ROOT),
        vec![ids::LEFT, ids::CENTER, ids::RIGHT]
    );
    assert!(
        b.hidden.is_empty(),
        "nothing hidden at 2560 DIPs: {:?}",
        b.hidden
    );
    let menu = t.get(menu_element(&default_modules()).unwrap()).unwrap();
    assert_eq!(menu.role, Role::Button);
    assert_eq!(
        menu.patterns(),
        vec![Pattern::Invoke, Pattern::ExpandCollapse]
    );
    let names: Vec<&str> = t.iter().map(|e| e.name.as_str()).collect();
    for expected in [
        "Active window: 문서 — 파일 탐색기",
        "CPU usage 12%",
        "GPU usage 3%",
        "Video memory: 2.0 GB of 16.0 GB used (13%)",
        "Upload speed 2 KB/s",
        "Download speed 2.4 MB/s",
        "Notifications: on",
    ] {
        assert!(
            names.contains(&expected),
            "missing {expected:?} in {names:#?}"
        );
    }
    // Disabled-by-default custom modules and the absent battery are not shown.
    assert!(
        !t.iter()
            .any(|e| e.automation_id == "module.text" || e.automation_id == "module.battery")
    );
}

#[test]
fn modules_follow_configured_order_and_regions() {
    let mut mods = default_modules();
    let cpu = mods.iter().position(|m| m.id == "cpu").unwrap();
    mods[cpu].region = Region::Left;
    let b = bar(2560.0, 1.0, mods.clone(), Lang::En);
    let left: Vec<&str> = b
        .scene
        .tree
        .children(ids::LEFT)
        .iter()
        .map(|i| b.scene.tree.get(*i).unwrap().automation_id)
        .collect();
    assert_eq!(left, ["module.menu", "module.active-window", "module.cpu"]);
    // Moving a module changes positions, not element ids.
    let before: HashMap<_, _> = b.elements.iter().cloned().collect();
    mods.swap(cpu, cpu - 1);
    let after = bar(2560.0, 1.0, mods, Lang::En);
    for (id, el) in &after.elements {
        assert_eq!(before.get(id), Some(el), "{id} changed element id");
    }
}

#[test]
fn overflow_shortens_title_then_hides_modules_and_names_them() {
    let b = bar(700.0, 1.0, default_modules(), Lang::En);
    let t = &b.scene.tree;
    assert!(!b.hidden.is_empty(), "something must be hidden at 700 DIPs");
    let o = t.get(ids::OVERFLOW).expect("overflow element");
    assert!(
        o.name.starts_with(&format!(
            "{} modules hidden for lack of space",
            b.hidden.len()
        )),
        "{}",
        o.name
    );
    assert!(t.problems().is_empty(), "{:#?}", t.problems());
    // Menu and active window are never hidden.
    assert!(!b.hidden.iter().any(|h| h == "menu" || h == "active-window"));
    // No overlaps among shown modules.
    let leaves: Vec<_> = t
        .iter()
        .filter(|e| !matches!(e.role, Role::Group | Role::ToolBar))
        .map(|e| e.bounds)
        .collect();
    for (i, a) in leaves.iter().enumerate() {
        for c in &leaves[i + 1..] {
            assert!(!a.intersects(c), "{a:?} overlaps {c:?}");
        }
    }
}

#[test]
fn unavailable_readings_are_disabled_with_reasons() {
    let (style, p) = fixture("windowsrice-default");
    let mut r = readings();
    r.gpu = Reading::Unavailable("GPU performance counters are not available".into());
    r.vram = Reading::Pending;
    r.battery = Reading::Ok(BatteryState {
        percent: Some(80),
        charging: true,
        on_ac: true,
    });
    let b = build_bar(
        &BarModel {
            modules: default_modules(),
            readings: r,
            ..Default::default()
        },
        &style,
        &p,
        &ApproxMeasure,
        Size { w: 2560.0, h: 34.0 },
        1.0,
        &Strings::new(Lang::En),
        4.0,
    );
    let t = &b.scene.tree;
    let gpu = t.iter().find(|e| e.automation_id == "module.gpu").unwrap();
    assert!(!gpu.enabled && !gpu.is_focus_target());
    assert_eq!(gpu.name, "GPU usage unavailable");
    assert_eq!(
        gpu.help.as_deref(),
        Some("GPU performance counters are not available")
    );
    let vram = t.iter().find(|e| e.automation_id == "module.vram").unwrap();
    assert_eq!(vram.name, "Video memory: measuring");
    let battery = t
        .iter()
        .find(|e| e.automation_id == "module.battery")
        .unwrap();
    assert_eq!(battery.name, "Battery 80%, charging");
}

#[test]
fn every_scale_keeps_modules_on_the_pixel_grid() {
    for scale in [1.0f32, 1.25, 1.5, 2.0] {
        let w = (3840.0 / scale).floor();
        let b = bar(w, scale, default_modules(), Lang::Ko);
        assert!(b.scene.tree.problems().is_empty());
        for e in b
            .scene
            .tree
            .iter()
            .filter(|e| !matches!(e.role, Role::Group | Role::ToolBar))
        {
            for edge in [e.bounds.x, e.bounds.right()] {
                let px = edge * scale;
                assert!(
                    (px - px.round()).abs() < 1e-2,
                    "{} off grid at {scale}",
                    e.automation_id
                );
            }
        }
    }
}

fn popup_model() -> PopupModel {
    let mut status = HashMap::new();
    status.insert("battery".to_owned(), ModuleStatus::Absent);
    status.insert(
        "gpu".to_owned(),
        ModuleStatus::Unavailable("counters off".into()),
    );
    PopupModel {
        themes: vec![
            ("windowsrice-default".into(), "windowsRICE Default".into()),
            ("windowsrice-glass".into(), "windowsRICE Glass".into()),
        ],
        active_theme: "windowsrice-default".into(),
        modules: default_modules(),
        status,
        height: 34,
        auto_hide: false,
        reserve: true,
        all_monitors: true,
        message: Some("Settings saved".into()),
        ..Default::default()
    }
}

#[test]
fn popup_tree_roles_states_and_actions() {
    let (style, p) = fixture("windowsrice-default");
    let ps = build_popup(
        &popup_model(),
        &style,
        &p,
        &ApproxMeasure,
        1.5,
        &Strings::new(Lang::En),
    );
    let t = &ps.scene.tree;
    assert!(t.problems().is_empty(), "{:#?}", t.problems());
    assert_eq!(t.root().role, Role::Window);
    assert_eq!(t.root().name, "windowsRICE settings");
    let radios: Vec<_> = t.iter().filter(|e| e.role == Role::RadioButton).collect();
    assert_eq!(radios.len(), 2);
    assert_eq!(
        radios.iter().filter(|r| r.selected == Some(true)).count(),
        1
    );
    let menu_check = t
        .iter()
        .find(|e| e.automation_id == "module:menu:show")
        .unwrap();
    assert_eq!(menu_check.role, Role::CheckBox);
    assert!(!menu_check.enabled, "the menu cannot be hidden");
    let gpu = t
        .iter()
        .find(|e| e.automation_id == "module:gpu:show")
        .unwrap();
    assert_eq!(gpu.name, "Show GPU usage — unavailable");
    assert_eq!(gpu.help.as_deref(), Some("counters off"));
    assert_eq!(gpu.toggled, Some(true));
    let battery = t
        .iter()
        .find(|e| e.automation_id == "module:battery:show")
        .unwrap();
    assert_eq!(battery.name, "Show Battery — not on this computer");
    // Every interactive element has an action, and every action an element.
    for e in t
        .iter()
        .filter(|e| e.invokable || e.toggled.is_some() || e.selected.is_some())
    {
        assert!(
            ps.action(e.id).is_some(),
            "{} has no action",
            e.automation_id
        );
    }
    assert_eq!(
        ps.action(ps.element(&PopupAction::Exit).unwrap()),
        Some(&PopupAction::Exit)
    );
    // First module cannot move up, last cannot move down.
    let up = t
        .iter()
        .find(|e| e.automation_id == "module:menu:up")
        .unwrap();
    assert!(!up.enabled);
    let down = t
        .iter()
        .find(|e| e.automation_id == "module:notifications:down")
        .unwrap();
    assert!(!down.enabled);
}

#[test]
fn popup_keyboard_is_vertical_with_toggle_and_select() {
    let (style, p) = fixture("windowsrice-default");
    let ps = build_popup(
        &popup_model(),
        &style,
        &p,
        &ApproxMeasure,
        1.0,
        &Strings::new(Lang::En),
    );
    let t = &ps.scene.tree;
    let order = t.focus_order();
    let none = Modifiers::default();
    let v = Orientation::Vertical;
    assert_eq!(
        handle_key_oriented(t, Some(order[0]), Key::Down, none, v),
        KeyOutcome::Focus(order[1])
    );
    assert_eq!(
        handle_key_oriented(t, Some(order[1]), Key::Up, none, v),
        KeyOutcome::Focus(order[0])
    );
    let radio = t.iter().find(|e| e.role == Role::RadioButton).unwrap().id;
    assert_eq!(
        handle_key_oriented(t, Some(radio), Key::Space, none, v),
        KeyOutcome::Select(radio)
    );
    let check = t
        .iter()
        .find(|e| e.automation_id == "module:cpu:show")
        .unwrap()
        .id;
    assert_eq!(
        handle_key_oriented(t, Some(check), Key::Space, none, v),
        KeyOutcome::Toggle(check)
    );
    assert_eq!(
        handle_key_oriented(t, Some(check), Key::Escape, none, v),
        KeyOutcome::Dismiss
    );
}

#[test]
fn module_actions() {
    let mut m = default_modules();
    assert!(
        !apply_module_action(&mut m, &PopupAction::ToggleModule("menu".into())),
        "menu stays on"
    );
    assert!(apply_module_action(
        &mut m,
        &PopupAction::ToggleModule("cpu".into())
    ));
    assert!(!m.iter().find(|x| x.id == "cpu").unwrap().enabled);
    let i = m.iter().position(|x| x.id == "gpu").unwrap();
    assert!(apply_module_action(
        &mut m,
        &PopupAction::MoveUp("gpu".into())
    ));
    assert_eq!(m[i - 1].id, "gpu");
    assert!(!apply_module_action(
        &mut m,
        &PopupAction::MoveUp("menu".into())
    ));
    assert!(apply_module_action(
        &mut m,
        &PopupAction::IntervalShorter("cpu".into())
    ));
    assert_eq!(
        m.iter().find(|x| x.id == "cpu").unwrap().interval_ms,
        Some(1_000)
    );
    assert!(apply_module_action(
        &mut m,
        &PopupAction::IntervalLonger("cpu".into())
    ));
    assert_eq!(
        m.iter().find(|x| x.id == "cpu").unwrap().interval_ms,
        None,
        "back to the default"
    );
    assert!(
        !apply_module_action(&mut m, &PopupAction::IntervalLonger("clock".into())),
        "clock has no interval"
    );
    assert_eq!(step_interval(500, false), 500);
    assert_eq!(step_interval(600_000, true), 600_000);
}

#[test]
fn glass_popup_and_bar_use_backplates() {
    let (style, p) = fixture("windowsrice-glass");
    assert!(p.backplates);
    let ps = build_popup(
        &popup_model(),
        &style,
        &p,
        &ApproxMeasure,
        1.0,
        &Strings::new(Lang::Ko),
    );
    assert!(ps.scene.tree.problems().is_empty());
    let check = ps
        .scene
        .tree
        .iter()
        .find(|e| e.automation_id == "module:cpu:show")
        .unwrap();
    assert!(ps.scene.draw.cmds.iter().any(|c| matches!(c, DrawCmd::Fill { owner: Some(o), color, .. } if *o == check.id && *color == style.overlay)));
}

#[test]
fn popup_with_every_builtin_theme_fits_a_768_pixel_screen() {
    // 17 built-in themes (Phase 8) plus a custom one: the window must still
    // fit below a 34 DIP bar on a 1366×768 screen at 100% (734 DIPs).
    let mut m = popup_model();
    m.themes = ThemeCatalog::load(None)
        .entries()
        .iter()
        .map(|e| (e.id().to_owned(), e.theme.meta.name.clone()))
        .chain(std::iter::once((
            "custom-one".to_owned(),
            "My theme".to_owned(),
        )))
        .collect();
    let (style, p) = fixture("windowsrice-default");
    let ps = build_popup(&m, &style, &p, &ApproxMeasure, 1.0, &Strings::new(Lang::En));
    assert!(
        ps.scene.size.h <= 734.0,
        "popup is {} DIPs tall",
        ps.scene.size.h
    );
    assert!(
        ps.scene.tree.problems().is_empty(),
        "{:#?}",
        ps.scene.tree.problems()
    );
    let radios: Vec<_> = ps
        .scene
        .tree
        .iter()
        .filter(|e| e.automation_id.starts_with("theme:"))
        .collect();
    assert_eq!(radios.len(), 18);
    assert!(
        radios.iter().any(|r| r.name == "Glass"),
        "short labels in two columns"
    );
}
