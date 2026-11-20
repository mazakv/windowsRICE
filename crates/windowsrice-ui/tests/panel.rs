//! Panel scene tests: accessibility tree structure and state, keyboard
//! navigation, draw-list/tree synchronisation, layout at every scale and the
//! Glass legibility policy. Pure unit tests (no Windows APIs).

use windowsrice_core::config::{ThemeModePreference, ThemeSelection};
use windowsrice_theme::catalog::ThemeCatalog;
use windowsrice_theme::engine::{NoopSink, ResolvedTheme, ThemeEngine};
use windowsrice_theme::presets::BUILTIN_PRESETS;
use windowsrice_theme::schema::{Backdrop, Mode};
use windowsrice_ui::draw::{DrawCmd, FontRole};
use windowsrice_ui::format::LocalTime;
use windowsrice_ui::geom::{Point, Size};
use windowsrice_ui::keyboard::{Key, KeyOutcome, Modifiers, handle_key};
use windowsrice_ui::legibility::{BackplateMode, SurfacePlan, SurfaceReason, SurfaceRequest, plan};
use windowsrice_ui::panel::{
    AudioState, LinkKind, MemoryState, NetworkState, PanelData, PanelModel, Scene, build, ids,
};
use windowsrice_ui::strings::{Lang, Strings};
use windowsrice_ui::style::{HighContrastColors, PanelStyle};
use windowsrice_ui::text::ApproxMeasure;
use windowsrice_ui::tree::{Pattern, Role};
use windowsrice_ui::{ElementId, Rect};

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
    let applied = engine.applied().expect("theme applied").clone();
    assert_eq!(applied.id, id, "fell back instead of applying {id}");
    applied
}

fn request(t: &ResolvedTheme) -> SurfaceRequest {
    SurfaceRequest {
        backdrop: t.theme.materials.backdrop,
        theme_opacity: t.theme.materials.surface_opacity,
        opacity_override: None,
        backplates: BackplateMode::Auto,
        reduced_transparency: false,
        backdrop_available: true,
        high_contrast: false,
    }
}

fn full_data() -> PanelData {
    PanelData {
        active_window: Some("문서 — 파일 탐색기".into()),
        time: Some(LocalTime {
            year: 2026,
            month: 11,
            day: 19,
            weekday: 4,
            hour: 15,
            minute: 42,
        }),
        network: Some(NetworkState::Internet(LinkKind::Wired)),
        audio: Some(AudioState {
            volume_percent: 40,
            muted: false,
        }),
        cpu_percent: Some(12.4),
        gpu_percent: Some(3.0),
        memory: Some(MemoryState {
            used_bytes: 13 << 30,
            total_bytes: 32 << 30,
        }),
        warnings: Vec::new(),
    }
}

struct Fixture {
    style: PanelStyle,
    plan: SurfacePlan,
}

fn fixture(id: &str, mode: Mode) -> Fixture {
    let t = theme(id, mode);
    Fixture {
        style: PanelStyle::from_theme(&t, None, true),
        plan: plan(&request(&t), &t.palette),
    }
}

fn scene_with(f: &Fixture, model: &PanelModel, width: f32, scale: f32, lang: Lang) -> Scene {
    build(
        model,
        &f.style,
        &f.plan,
        &ApproxMeasure,
        Size {
            w: width,
            h: f.style.height,
        },
        scale,
        &Strings::new(lang),
    )
}

fn default_scene(model: &PanelModel) -> Scene {
    scene_with(
        &fixture("windowsrice-default", Mode::Dark),
        model,
        2533.0,
        1.5,
        Lang::Ko,
    )
}

const FOCUS_ORDER: [ElementId; 9] = [
    ids::MENU,
    ids::ACTIVE_WINDOW,
    ids::CLOCK,
    ids::NETWORK,
    ids::VOLUME,
    ids::CPU,
    ids::GPU,
    ids::MEMORY,
    ids::STATUS,
];

#[test]
fn tree_structure_roles_and_names() {
    let s = default_scene(&PanelModel {
        data: full_data(),
        ..Default::default()
    });
    let t = &s.tree;
    assert!(t.problems().is_empty(), "{:#?}", t.problems());
    assert_eq!(t.root().role, Role::ToolBar);
    assert_eq!(t.root().name, "windowsRICE 패널");
    assert_eq!(
        t.children(ids::ROOT),
        vec![ids::GROUP_APP, ids::GROUP_CLOCK, ids::GROUP_STATUS]
    );
    for g in [ids::GROUP_APP, ids::GROUP_CLOCK, ids::GROUP_STATUS] {
        assert_eq!(t.get(g).unwrap().role, Role::Group);
    }
    assert_eq!(
        t.children(ids::GROUP_APP),
        vec![ids::MENU, ids::ACTIVE_WINDOW]
    );
    assert_eq!(
        t.children(ids::GROUP_STATUS),
        vec![
            ids::NETWORK,
            ids::VOLUME,
            ids::CPU,
            ids::GPU,
            ids::MEMORY,
            ids::STATUS
        ]
    );
    let menu = t.get(ids::MENU).unwrap();
    assert_eq!(menu.role, Role::Button);
    assert_eq!(
        menu.patterns(),
        vec![Pattern::Invoke, Pattern::ExpandCollapse]
    );
    assert_eq!(menu.expanded, Some(false));
    assert!(menu.help.is_some());
    assert_eq!(
        t.get(ids::ACTIVE_WINDOW).unwrap().name,
        "활성 창: 문서 — 파일 탐색기"
    );
    assert_eq!(
        t.get(ids::CLOCK).unwrap().name,
        "시계: 15시 42분, 2026년 11월 19일 목요일"
    );
    assert_eq!(t.get(ids::CPU).unwrap().name, "CPU 사용률 12%");
    assert_eq!(
        t.get(ids::MEMORY).unwrap().name,
        "메모리: 32.0 GB 중 13.0 GB 사용 (41%)"
    );
    assert_eq!(
        t.get(ids::NETWORK).unwrap().name,
        "네트워크: 인터넷 연결됨, 유선"
    );
    assert_eq!(t.get(ids::STATUS).unwrap().name, "windowsRICE 상태: 정상");
    assert_eq!(t.focus_order(), FOCUS_ORDER.to_vec());
}

#[test]
fn english_names() {
    let s = scene_with(
        &fixture("windowsrice-default", Mode::Dark),
        &PanelModel {
            data: full_data(),
            ..Default::default()
        },
        1920.0,
        1.0,
        Lang::En,
    );
    assert_eq!(s.tree.root().name, "windowsRICE panel");
    assert_eq!(s.tree.get(ids::VOLUME).unwrap().name, "Volume 40%");
    assert_eq!(
        s.tree.get(ids::CLOCK).unwrap().name,
        "Clock: 15:42, Thursday, November 19, 2026"
    );
}

#[test]
fn unavailable_readings_are_disabled_and_named() {
    let mut data = full_data();
    data.gpu_percent = None;
    data.audio = None;
    data.network = None;
    let s = scene_with(
        &fixture("windowsrice-default", Mode::Dark),
        &PanelModel {
            data,
            ..Default::default()
        },
        1920.0,
        1.0,
        Lang::En,
    );
    let t = &s.tree;
    for (id, name) in [
        (ids::GPU, "GPU usage unavailable"),
        (ids::VOLUME, "No audio output device"),
        (ids::NETWORK, "Network status unavailable"),
    ] {
        let e = t.get(id).unwrap();
        assert!(!e.enabled, "{name}");
        assert_eq!(e.name, name);
        assert!(!t.focus_order().contains(&id));
    }
    assert!(t.problems().is_empty(), "{:#?}", t.problems());
    let gpu_text = s
        .draw
        .texts()
        .find(|(_, _, o)| *o == Some(ids::GPU))
        .unwrap();
    assert_eq!(gpu_text.0, "GPU —");
}

#[test]
fn warnings_change_status_name() {
    let mut data = full_data();
    data.warnings = vec!["Theme fell back to windowsRICE Default".into()];
    let s = scene_with(
        &fixture("windowsrice-default", Mode::Dark),
        &PanelModel {
            data,
            ..Default::default()
        },
        1920.0,
        1.0,
        Lang::En,
    );
    assert_eq!(
        s.tree.get(ids::STATUS).unwrap().name,
        "windowsRICE status: 1 warning(s): Theme fell back to windowsRICE Default"
    );
}

#[test]
fn keyboard_navigation() {
    let s = default_scene(&PanelModel {
        data: full_data(),
        ..Default::default()
    });
    let t = &s.tree;
    let none = Modifiers::default();
    let shift = Modifiers {
        shift: true,
        ..none
    };
    // Tab walks the whole order and wraps.
    let mut focus = None;
    for expected in FOCUS_ORDER.iter().chain(std::iter::once(&ids::MENU)) {
        match handle_key(t, focus, Key::Tab, none) {
            KeyOutcome::Focus(f) => {
                assert_eq!(f, *expected);
                focus = Some(f);
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(
        handle_key(t, Some(ids::MENU), Key::Tab, shift),
        KeyOutcome::Focus(ids::STATUS)
    );
    assert_eq!(
        handle_key(t, Some(ids::CLOCK), Key::Left, none),
        KeyOutcome::Focus(ids::ACTIVE_WINDOW)
    );
    assert_eq!(
        handle_key(t, Some(ids::CLOCK), Key::Right, none),
        KeyOutcome::Focus(ids::NETWORK)
    );
    assert_eq!(
        handle_key(t, Some(ids::CPU), Key::Home, none),
        KeyOutcome::Focus(ids::MENU)
    );
    assert_eq!(
        handle_key(t, Some(ids::CPU), Key::End, none),
        KeyOutcome::Focus(ids::STATUS)
    );
    assert_eq!(
        handle_key(t, Some(ids::MENU), Key::Enter, none),
        KeyOutcome::Invoke(ids::MENU)
    );
    assert_eq!(
        handle_key(t, Some(ids::MENU), Key::Space, none),
        KeyOutcome::Invoke(ids::MENU)
    );
    assert_eq!(
        handle_key(t, Some(ids::MENU), Key::Down, none),
        KeyOutcome::Expand(ids::MENU)
    );
    assert_eq!(
        handle_key(t, Some(ids::CLOCK), Key::Enter, none),
        KeyOutcome::Ignored
    );
    assert_eq!(
        handle_key(t, Some(ids::CLOCK), Key::Escape, none),
        KeyOutcome::Dismiss
    );
    assert_eq!(
        handle_key(t, None, Key::Enter, none),
        KeyOutcome::Focus(ids::MENU)
    );
}

#[test]
fn draw_list_matches_tree() {
    for keyboard_active in [false, true] {
        let s = default_scene(&PanelModel {
            data: full_data(),
            focus: Some(ids::CPU),
            keyboard_active,
            ..Default::default()
        });
        let t = &s.tree;
        for cmd in &s.draw.cmds {
            let owner = cmd.owner().expect("every command has an owner");
            let e = t.get(owner).expect("owner exists in the tree");
            let slack = if matches!(cmd, DrawCmd::Stroke { .. }) {
                2.1
            } else {
                0.01
            };
            assert!(
                e.bounds.contains_rect(&cmd.rect(), slack),
                "{cmd:?} outside {:?}",
                e.bounds
            );
            assert!(
                t.root().bounds.contains_rect(&cmd.rect(), 0.01),
                "{cmd:?} outside panel"
            );
        }
        for id in t.focus_order() {
            assert!(
                s.draw.cmds.iter().any(|c| c.owner() == Some(id)),
                "{id:?} is focusable but draws nothing"
            );
        }
        let rings: Vec<_> = s
            .draw
            .cmds
            .iter()
            .filter(|c| matches!(c, DrawCmd::Stroke { .. }))
            .collect();
        if keyboard_active {
            assert_eq!(rings.len(), 1);
            assert_eq!(rings[0].owner(), Some(ids::CPU));
            assert!(matches!(s.draw.cmds.last(), Some(DrawCmd::Stroke { .. })));
        } else {
            assert!(rings.is_empty(), "focus ring without keyboard focus");
        }
    }
}

#[test]
fn layout_at_every_scale_and_width() {
    let f = fixture("windowsrice-default", Mode::Dark);
    for scale in [1.0f32, 1.25, 1.5, 2.0] {
        for physical_width in [1280.0f32, 1920.0, 2560.0, 3840.0] {
            let width = ((physical_width - 2.0 * (8.0 * scale).round()) / scale).floor();
            for lang in [Lang::Ko, Lang::En] {
                let s = scene_with(
                    &f,
                    &PanelModel {
                        data: full_data(),
                        ..Default::default()
                    },
                    width,
                    scale,
                    lang,
                );
                let t = &s.tree;
                assert!(
                    t.problems().is_empty(),
                    "scale {scale} width {width}: {:#?}",
                    t.problems()
                );
                let leaves: Vec<Rect> = t
                    .iter()
                    .filter(|e| !matches!(e.role, Role::Group | Role::ToolBar))
                    .map(|e| e.bounds)
                    .collect();
                for (i, a) in leaves.iter().enumerate() {
                    for b in &leaves[i + 1..] {
                        assert!(!a.intersects(b), "overlap {a:?} {b:?} at {scale} {width}");
                    }
                    for edge in [a.x, a.y, a.right(), a.bottom()] {
                        let px = edge * scale;
                        assert!(
                            (px - px.round()).abs() < 1e-2,
                            "{a:?} off the pixel grid at {scale}"
                        );
                    }
                }
                // The clock is centred unless centring would collide with the
                // status group, in which case it moves left of it.
                let clock = t.get(ids::CLOCK).unwrap().bounds;
                let status = t.get(ids::GROUP_STATUS).unwrap().bounds;
                let centred_x = (width - clock.w) / 2.0;
                if centred_x + clock.w + f.style.gap <= status.x {
                    assert!(
                        (clock.x - centred_x).abs() <= 1.0,
                        "clock not centred at {width}"
                    );
                } else {
                    assert!(
                        clock.right() + f.style.gap <= status.x + 1.0,
                        "clock hits status at {width}"
                    );
                }
            }
        }
    }
}

#[test]
fn long_titles_are_trimmed_before_the_clock() {
    let mut data = full_data();
    data.active_window = Some("가".repeat(400));
    let s = scene_with(
        &fixture("windowsrice-default", Mode::Dark),
        &PanelModel {
            data,
            ..Default::default()
        },
        1264.0,
        1.0,
        Lang::Ko,
    );
    let app = s.tree.get(ids::ACTIVE_WINDOW).unwrap().bounds;
    let clock = s.tree.get(ids::CLOCK).unwrap().bounds;
    assert!(app.right() < clock.x, "{app:?} reaches {clock:?}");
    let trimmed = s.draw.cmds.iter().any(|c| {
        matches!(c, DrawCmd::Text { trim: true, owner: Some(o), .. } if *o == ids::ACTIVE_WINDOW)
    });
    assert!(trimmed);
}

#[test]
fn menu_state_and_hover() {
    let base = PanelModel {
        data: full_data(),
        ..Default::default()
    };
    let open = default_scene(&PanelModel {
        menu_open: true,
        ..base.clone()
    });
    assert_eq!(open.tree.get(ids::MENU).unwrap().expanded, Some(true));
    let f = fixture("windowsrice-default", Mode::Dark);
    let pressed = open
        .draw
        .cmds
        .iter()
        .any(|c| matches!(c, DrawCmd::Fill { color, owner: Some(o), .. } if *o == ids::MENU && *color == f.style.pressed));
    assert!(pressed);
    let hovered = default_scene(&PanelModel {
        hover: Some(ids::MENU),
        ..base
    });
    assert!(
        hovered
            .draw
            .cmds
            .iter()
            .any(|c| matches!(c, DrawCmd::Fill { color, .. } if *color == f.style.hover))
    );
    assert_eq!(
        hovered.tree.hit_test(Point { x: 20.0, y: 17.0 }),
        Some(ids::MENU)
    );
}

#[test]
fn korean_and_english_text_reach_the_draw_list() {
    let s = default_scene(&PanelModel {
        data: full_data(),
        ..Default::default()
    });
    let texts: Vec<&str> = s.draw.texts().map(|(t, _, _)| t).collect();
    assert!(texts.contains(&"문서 — 파일 탐색기"));
    assert!(texts.contains(&"11월 19일 (목)"));
    assert!(texts.contains(&"15:42"));
    assert!(texts.contains(&"CPU 12%"));
    for (_, style, owner) in s.draw.texts() {
        assert!(style.weight <= 600);
        if owner == Some(ids::CLOCK) && style.font == FontRole::Ui && style.weight != 400 {
            assert_eq!(style.weight, 500, "time uses the emphasis weight");
        }
    }
}

#[test]
fn every_preset_builds_cleanly_in_every_mode() {
    for preset in BUILTIN_PRESETS {
        for mode in [Mode::Dark, Mode::Light] {
            let t = theme(preset.id, mode);
            if t.mode != mode {
                continue; // preset has no palette for this mode
            }
            let style = PanelStyle::from_theme(&t, None, true);
            assert!(style.weight_regular == 400 && style.weight_emphasis <= 600);
            let p = plan(&request(&t), &t.palette);
            assert!(
                p.meets_aa_worst_case,
                "{} {mode:?}: worst case {:.2}",
                preset.id, p.worst_case_text_contrast
            );
            let s = build(
                &PanelModel {
                    data: full_data(),
                    ..Default::default()
                },
                &style,
                &p,
                &ApproxMeasure,
                Size {
                    w: 2533.0,
                    h: style.height,
                },
                1.5,
                &Strings::new(Lang::Ko),
            );
            assert!(
                s.tree.problems().is_empty(),
                "{}: {:#?}",
                preset.id,
                s.tree.problems()
            );
        }
    }
}

#[test]
fn glass_gets_solid_backplates_in_auto_mode() {
    for mode in [Mode::Dark, Mode::Light] {
        let t = theme("windowsrice-glass", mode);
        let p = plan(&request(&t), &t.palette);
        assert_eq!(p.reason, SurfaceReason::Translucent);
        assert!(p.backplates, "{mode:?}");
        assert!(
            p.worst_case_without_backplates < 4.5,
            "{mode:?}: {}",
            p.worst_case_without_backplates
        );
        assert!(p.meets_aa_worst_case);
        assert!(
            (p.surface_opacity - 0.55).abs() < 1e-6,
            "theme opacity kept"
        );
        // Backplates are drawn behind text-bearing elements.
        let style = PanelStyle::from_theme(&t, None, true);
        let s = build(
            &PanelModel {
                data: full_data(),
                ..Default::default()
            },
            &style,
            &p,
            &ApproxMeasure,
            Size {
                w: 1904.0,
                h: style.height,
            },
            1.0,
            &Strings::new(Lang::En),
        );
        for id in [ids::ACTIVE_WINDOW, ids::CLOCK, ids::CPU] {
            let bounds = s.tree.get(id).unwrap().bounds;
            assert!(s.draw.cmds.iter().any(|c| matches!(c,
                DrawCmd::Fill { rect, color, owner: Some(o), .. }
                    if *o == id && *rect == bounds && *color == style.surface)));
        }
    }
}

#[test]
fn glass_without_backplates_is_not_reported_accessible() {
    let t = theme("windowsrice-glass", Mode::Dark);
    let p = plan(
        &SurfaceRequest {
            backplates: BackplateMode::Never,
            ..request(&t)
        },
        &t.palette,
    );
    assert!(!p.backplates);
    assert!(!p.meets_aa_worst_case);
    assert!(p.min_opacity_for_aa > 0.55 && p.min_opacity_for_aa <= 1.0);
}

#[test]
fn default_needs_no_backplates() {
    let t = theme("windowsrice-default", Mode::Dark);
    let p = plan(&request(&t), &t.palette);
    assert!(!p.backplates);
    assert!(p.meets_aa_worst_case);
    assert_eq!(p.backdrop, Backdrop::Acrylic);
}

#[test]
fn opaque_fallbacks() {
    let t = theme("windowsrice-glass", Mode::Dark);
    for (req, reason) in [
        (
            SurfaceRequest {
                reduced_transparency: true,
                ..request(&t)
            },
            SurfaceReason::ReducedTransparency,
        ),
        (
            SurfaceRequest {
                backdrop_available: false,
                ..request(&t)
            },
            SurfaceReason::BackdropUnavailable,
        ),
        (
            SurfaceRequest {
                high_contrast: true,
                ..request(&t)
            },
            SurfaceReason::HighContrast,
        ),
    ] {
        let p = plan(&req, &t.palette);
        assert_eq!(p.reason, reason);
        assert_eq!(p.backdrop, Backdrop::None);
        assert!((p.surface_opacity - 1.0).abs() < f32::EPSILON);
        assert!(p.meets_aa_worst_case);
    }
}

#[test]
fn opacity_override_is_clamped_and_not_persisted() {
    let t = theme("windowsrice-glass", Mode::Dark);
    let before = t.theme.materials.surface_opacity;
    let p = plan(
        &SurfaceRequest {
            opacity_override: Some(0.1),
            ..request(&t)
        },
        &t.palette,
    );
    assert!((p.surface_opacity - 0.30).abs() < 1e-6);
    let p = plan(
        &SurfaceRequest {
            opacity_override: Some(0.97),
            ..request(&t)
        },
        &t.palette,
    );
    assert!((p.surface_opacity - 0.97).abs() < 1e-6);
    assert_eq!(t.theme.materials.surface_opacity, before);
}

#[test]
fn high_contrast_uses_system_colours() {
    let t = theme("windowsrice-glass", Mode::Dark);
    let hc = HighContrastColors {
        window: windowsrice_theme::Color::BLACK,
        window_text: windowsrice_theme::Color::WHITE,
        highlight: windowsrice_theme::Color::rgb(0x1A, 0xEB, 0xFF),
        highlight_text: windowsrice_theme::Color::BLACK,
        gray_text: windowsrice_theme::Color::rgb(0x3F, 0xF2, 0x3F),
    };
    let s = PanelStyle::from_theme(&t, Some(&hc), true);
    assert!(s.high_contrast);
    assert_eq!(s.surface, hc.window);
    assert_eq!(s.text_primary, hc.window_text);
    assert_eq!(s.focus_ring, hc.window_text);
}

#[test]
fn animations_follow_system_setting() {
    let t = theme("windowsrice-default", Mode::Dark);
    assert!(PanelStyle::from_theme(&t, None, true).fade_ms > 0);
    assert_eq!(PanelStyle::from_theme(&t, None, false).fade_ms, 0);
}
