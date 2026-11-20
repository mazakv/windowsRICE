//! Design Studio scene: accessibility, completeness of actions, galleries,
//! scrolling, drag and drop targets, text editing and draft edits. Pure
//! unit tests (approximate text measurement, built-in themes).

use windowsrice_core::bar::{BarConfig, ModuleKind, Region, default_modules};
use windowsrice_core::config::{ThemeModePreference, ThemeSelection};
use windowsrice_theme::catalog::ThemeCatalog;
use windowsrice_theme::schema::Mode;
use windowsrice_ui::draw::{DrawCmd, FontRole};
use windowsrice_ui::geom::Size;
use windowsrice_ui::legibility::{BackplateMode, SurfacePlan, SurfaceRequest, plan};
use windowsrice_ui::strings::{Lang, Strings};
use windowsrice_ui::studio::catalog::{self, Gallery};
use windowsrice_ui::studio::*;
use windowsrice_ui::style::{PanelStyle, bar_surface, resolve_bar_theme};
use windowsrice_ui::text::ApproxMeasure;
use windowsrice_ui::tree::Role;

thread_local! {
    static CATALOG: ThemeCatalog = ThemeCatalog::load(None);
}

/// Resolves a draft exactly like the Studio does for the running bar.
fn resolve(d: &Draft) -> Option<(PanelStyle, SurfacePlan)> {
    CATALOG.with(|cat| {
        let t = resolve_bar_theme(cat, &d.theme, &d.bar.style, Some(Mode::Dark))?;
        let style = PanelStyle::from_theme(&t, None, true);
        let (backdrop, theme_opacity) = bar_surface(&t, &d.bar.style);
        let p = plan(
            &SurfaceRequest {
                backdrop,
                theme_opacity,
                opacity_override: None,
                backplates: BackplateMode::Auto,
                reduced_transparency: false,
                backdrop_available: true,
                high_contrast: false,
            },
            &t.palette,
        );
        Some((style, p))
    })
}

fn fonts() -> Vec<FontEntry> {
    [
        ("Segoe UI Variable", false),
        ("Malgun Gothic", true),
        ("Cascadia Code", false),
        ("Bahnschrift", false),
        ("Georgia", false),
        ("Noto Sans KR", true),
    ]
    .iter()
    .map(|(f, h)| FontEntry {
        family: (*f).to_owned(),
        hangul: *h,
        weights: vec![300, 400, 600, 700],
        mono: f.contains("Cascadia"),
    })
    .collect()
}

fn model(section: Section) -> StudioModel {
    let draft = Draft {
        theme: ThemeSelection::default(),
        bar: BarConfig::default(),
    };
    let themes = CATALOG.with(|c| {
        c.entries()
            .iter()
            .map(|e| ThemeEntry {
                id: e.id().to_owned(),
                name: e.theme.meta.name.clone(),
                builtin: e.is_builtin(),
                has_dark: e.theme.palette.dark.is_some(),
                has_light: e.theme.palette.light.is_some(),
                swatches: [windowsrice_theme::Color::BLACK; 4],
            })
            .collect()
    });
    StudioModel {
        section,
        applied: draft.clone(),
        draft,
        can_undo: false,
        can_redo: false,
        themes,
        theme_selected: Some("windowsrice-default".into()),
        theme_edit: None,
        presets: vec![
            PresetEntry {
                file: "evening.toml".into(),
                name: Some("Evening".into()),
                error: None,
            },
            PresetEntry {
                file: "broken.toml".into(),
                name: None,
                error: Some("not a preset".into()),
            },
        ],
        preset_selected: Some(0),
        preset_name: String::new(),
        monitors: vec![MonitorEntry {
            device: r"\\.\DISPLAY1".into(),
            id: Some("mon-a".into()),
            name: "MSI MAG323UPF".into(),
            primary: true,
        }],
        fonts: fonts(),
        theme_font: "Segoe UI Variable".into(),
        selected_module: Some("cpu".into()),
        readings: Default::default(),
        preview_backdrop: PreviewBackdrop::Colour,
        status: None,
        blocking: vec![],
        warnings: vec!["Text contrast may be low over translucency".into()],
        text_edit: None,
        drag: None,
        focus: None,
        keyboard_active: true,
        hover: None,
        dark: true,
        appearance: None,
        bar_height: 34.0,
        monitor_width: 2560.0,
        theme_opacity: 0.88,
        scroll: 0.0,
        font_query: String::new(),
        color_target: ColorTarget::Background,
        recent_colors: vec!["#3A8DDE".into()],
        preview_view: PreviewView::All,
        theme_defaults: ThemeDefaults::default(),
        font_filter: FontFilter::All,
    }
}

fn scene(m: &StudioModel, size: Size, lang: Lang) -> StudioScene {
    let (style, p) = resolve(&m.draft).expect("default theme resolves");
    let ss = StudioStyle::from_palette(&CATALOG.with(|c| {
        c.get("windowsrice-default")
            .unwrap()
            .theme
            .palette
            .dark
            .clone()
            .unwrap()
    }));
    build_studio(
        m,
        &ss,
        &PreviewInputs {
            style: &style,
            plan: &p,
            resolve: &resolve,
        },
        &ApproxMeasure,
        size,
        1.5,
        &Strings::new(lang),
    )
}

const SIZES: [Size; 3] = [
    MIN_SIZE,
    Size {
        w: 1600.0,
        h: 1000.0,
    },
    Size {
        w: 2400.0,
        h: 1300.0,
    },
];

#[test]
fn every_section_is_accessible_and_every_control_works() {
    for section in Section::ALL {
        for size in SIZES {
            for lang in [Lang::En, Lang::Ko] {
                for scroll in [0.0, 300.0, 5000.0] {
                    let mut m = model(section);
                    m.scroll = scroll;
                    let s = scene(&m, size, lang);
                    let t = &s.scene.tree;
                    let ctx = format!("{section:?} {size:?} {lang:?} scroll {scroll}");
                    assert!(t.problems().is_empty(), "{ctx}: {:#?}", t.problems());
                    assert_eq!(t.root().role, Role::Window);
                    for e in t.iter().filter(|e| {
                        e.invokable
                            || e.toggled.is_some()
                            || e.selected.is_some()
                            || e.value.is_some()
                    }) {
                        assert!(
                            s.action(e.id).is_some(),
                            "{ctx}: {} ({}) has no action",
                            e.automation_id,
                            e.name
                        );
                    }
                    for (id, _) in &s.actions {
                        let e = t.get(*id).expect("action element exists");
                        assert!(
                            !e.name.trim().is_empty(),
                            "{ctx}: unnamed {}",
                            e.automation_id
                        );
                        assert!(
                            e.bounds.w >= 1.0 && e.bounds.h >= 1.0,
                            "{ctx}: {}",
                            e.automation_id
                        );
                    }
                    let navs: Vec<_> = t
                        .iter()
                        .filter(|e| e.automation_id.starts_with("nav:"))
                        .collect();
                    assert_eq!(navs.len(), 12, "{ctx}");
                    assert_eq!(
                        navs.iter().filter(|e| e.selected == Some(true)).count(),
                        1,
                        "{ctx}"
                    );
                }
            }
        }
    }
}

#[test]
fn galleries_show_real_cards_and_mark_the_selection() {
    let size = Size {
        w: 2400.0,
        h: 1300.0,
    };
    for (section, gal, n) in [
        (Section::Styles, Gallery::Style, 15),
        (Section::Layout, Gallery::Layout, 9),
        (Section::Effects, Gallery::Effect, 11),
        (Section::Icons, Gallery::Icons, 6),
    ] {
        let m = model(section);
        let s = scene(&m, size, Lang::En);
        let grid = s
            .grids
            .iter()
            .find(|g| g.prefix == format!("card:{}", gal.key()))
            .expect("grid");
        assert_eq!(grid.rects.len(), n, "{section:?}");
        let cards: Vec<_> = s
            .scene
            .tree
            .iter()
            .filter(|e| e.automation_id.starts_with(&format!("card:{}:", gal.key())))
            .collect();
        assert!(!cards.is_empty(), "{section:?}: visible cards");
        for c in &cards {
            assert_eq!(c.role, Role::ListItem);
            assert!(matches!(s.action(c.id), Some(StudioAction::Card(g, _)) if *g == gal));
        }
    }
    // A style card applied to the draft becomes the selected card.
    let mut m = model(Section::Styles);
    let fonts = m.fonts.clone();
    let ctx = catalog::Ctx {
        background: windowsrice_theme::Color::rgb(0x1B, 0x1D, 0x23),
        accent: windowsrice_theme::Color::rgb(0x5B, 0xA4, 0xE6),
        text: windowsrice_theme::Color::WHITE,
        fonts: &fonts,
    };
    catalog::apply(Gallery::Style, 5, &mut m.draft, &ctx);
    let s = scene(&m, size, Lang::En);
    let sel: Vec<_> = s
        .scene
        .tree
        .iter()
        .filter(|e| e.automation_id.starts_with("card:style:") && e.selected == Some(true))
        .map(|e| e.automation_id.to_owned())
        .collect();
    assert_eq!(sel, ["card:style:5"]);
    // Card previews are real bars: their copies (no owner) contain the clock.
    assert!(
        s.scene
            .draw
            .texts()
            .filter(|(t, _, o)| o.is_none() && t.contains(':'))
            .count()
            > 3
    );
}

#[test]
fn font_cards_use_the_real_family_and_never_fake_korean() {
    let mut m = model(Section::Typography);
    m.scroll = 400.0;
    let s = scene(
        &m,
        Size {
            w: 2400.0,
            h: 1300.0,
        },
        Lang::En,
    );
    let cards: Vec<_> = s
        .scene
        .tree
        .iter()
        .filter(|e| e.automation_id.starts_with("font:") && e.role == Role::ListItem)
        .collect();
    assert!(cards.len() >= 4, "{}", cards.len());
    for f in fonts() {
        let fam = windowsrice_ui::tree::intern(&f.family);
        let samples: Vec<&str> = s
            .scene
            .draw
            .cmds
            .iter()
            .filter_map(|c| match c {
                DrawCmd::Text { text, style, .. } if style.font == FontRole::Family(fam) => {
                    Some(text.as_str())
                }
                _ => None,
            })
            .collect();
        if !samples.contains(&"Aa Bb 123") {
            continue; // card not visible at this scroll position
        }
        let korean = samples.iter().any(|t| t.contains("가나다 한글"));
        assert_eq!(
            korean, f.hangul,
            "{}: Korean sample only in fonts with Hangul",
            f.family
        );
    }
    // Search filters by name.
    m.font_query = "malgun".into();
    m.scroll = 0.0;
    let s = scene(
        &m,
        Size {
            w: 2400.0,
            h: 2400.0,
        },
        Lang::En,
    );
    let grid = s.grids.iter().find(|g| g.prefix == "font").unwrap();
    assert_eq!(grid.rects.len(), 1);
}

#[test]
fn modules_section_has_cards_drop_slots_and_palette() {
    let mut m = model(Section::Modules);
    m.scroll = 280.0;
    let s = scene(
        &m,
        Size {
            w: 2400.0,
            h: 1900.0,
        },
        Lang::En,
    );
    let cards: Vec<_> = s
        .scene
        .tree
        .iter()
        .filter(|e| {
            e.automation_id.starts_with("card:")
                && e.role == Role::ListItem
                && !e.automation_id.starts_with("card:look")
        })
        .collect();
    assert_eq!(
        cards.len(),
        default_modules().len(),
        "{:?}",
        cards.iter().map(|c| c.automation_id).collect::<Vec<_>>()
    );
    let cpu = cards
        .iter()
        .find(|c| c.automation_id == "card:cpu")
        .unwrap();
    assert!(
        cpu.name.starts_with("CPU usage, Right section, position"),
        "{}",
        cpu.name
    );
    assert_eq!(s.columns.len(), 3);
    assert!(
        cards
            .iter()
            .all(|c| s.draggable.iter().any(|(e, _, _)| *e == c.id))
    );
    let right = default_modules()
        .iter()
        .filter(|m| m.region == Region::Right)
        .count();
    assert_eq!(
        s.drop_slots
            .iter()
            .filter(|(r, _, _)| *r == Region::Right)
            .count(),
        right + 1
    );
    let (_, col) = s.columns.iter().find(|(r, _)| *r == Region::Left).unwrap();
    let target = s.drop_target(col.x + 10.0, col.y + 40.0);
    assert_eq!(target.map(|t| t.0), Some(Region::Left));
    assert!(
        s.scene
            .tree
            .iter()
            .any(|e| e.automation_id == "add:CustomText")
    );
}

#[test]
fn scrolling_reports_its_range_and_keeps_offscreen_cards_out_of_the_tree() {
    let m = model(Section::Styles);
    let s = scene(&m, MIN_SIZE, Lang::En);
    assert!(
        s.scroll_max > 100.0,
        "15 style cards do not fit: {}",
        s.scroll_max
    );
    let grid = s.grids.iter().find(|g| g.prefix == "card:style").unwrap();
    let last = grid.rects.last().unwrap();
    assert!(last.bottom() > s.viewport.bottom());
    assert!(
        !s.scene
            .tree
            .iter()
            .any(|e| e.automation_id == "card:style:14"),
        "off-screen card left out"
    );
    let mut m2 = model(Section::Styles);
    m2.scroll = s.scroll_max;
    let s2 = scene(&m2, MIN_SIZE, Lang::En);
    assert!(
        s2.scene
            .tree
            .iter()
            .any(|e| e.automation_id == "card:style:14"),
        "scrolled into view"
    );
}

#[test]
fn sliders_are_accessible_and_snap() {
    let m = model(Section::Layout);
    let s = scene(
        &m,
        Size {
            w: 2400.0,
            h: 1900.0,
        },
        Lang::En,
    );
    let h = s
        .scene
        .tree
        .iter()
        .find(|e| e.automation_id == "slider:Height")
        .expect("height slider");
    assert_eq!(h.role, Role::Slider);
    assert_eq!(h.value.as_deref(), Some("34"));
    assert!(s.sliders.iter().any(|x| x.kind == SliderKind::Height));
    let mut d = m.draft.clone();
    assert!(SliderKind::Height.set(&mut d, 40.4));
    assert_eq!(d.bar.height, Some(40));
    assert!(SliderKind::Opacity.set(&mut d, 0.432));
    assert_eq!(d.bar.style.opacity, Some(0.45));
    assert!(SliderKind::Height.set(&mut d, 999.0));
    assert_eq!(d.bar.height, Some(64), "clamped to the allowed range");
    assert!(d.bar.problems().is_empty());
}

#[test]
fn colour_roles_and_theme_cards() {
    let mut d = Draft {
        theme: ThemeSelection::default(),
        bar: BarConfig::default(),
    };
    ColorTarget::Clock.set(&mut d.bar.style, Some("#E3A33B".into()));
    ColorTarget::Icons.set(&mut d.bar.style, Some("#36D1C4".into()));
    ColorTarget::Border.set(&mut d.bar.style, Some("#445566".into()));
    assert!(
        d.bar.style.border_bottom,
        "choosing a border colour shows the border"
    );
    assert_eq!(
        ColorTarget::Icons.get(&d.bar.style).as_deref(),
        Some("#36D1C4")
    );
    assert!(apply_theme_card(
        &mut d,
        "windowsrice-ocean",
        ThemeModePreference::Dark
    ));
    assert_eq!(d.theme.active, "windowsrice-ocean");
    assert_eq!(
        d.bar.style.clock_color, None,
        "theme colours show after choosing a theme"
    );
    assert_eq!(ColorTarget::Icons.get(&d.bar.style), None);
    assert!(d.bar.style.border_bottom, "non-colour settings stay");
    let m = model(Section::Colors);
    let s = scene(
        &m,
        Size {
            w: 2400.0,
            h: 1300.0,
        },
        Lang::Ko,
    );
    assert!(
        s.scene
            .tree
            .iter()
            .any(|e| e.automation_id == "field:color-hex")
    );
    assert!(
        s.scene
            .tree
            .iter()
            .filter(|e| e.automation_id.starts_with("role:"))
            .count()
            >= 6
    );
}

#[test]
fn text_editing_with_caret_selection_and_paste() {
    let mut e = TextEdit::new("k", "windows");
    assert_eq!(
        e.selection(),
        (0, 7),
        "editing starts with everything selected"
    );
    e.insert("가나", 64);
    assert_eq!(e.buffer, "가나");
    e.home(false);
    e.insert("[", 64);
    e.end(false);
    e.insert("]", 64);
    assert_eq!(e.buffer, "[가나]");
    e.left(false);
    e.left(true);
    assert_eq!(e.selected_text(), "나");
    assert_eq!(e.cut(), "나");
    assert_eq!(e.buffer, "[가]");
    e.backspace();
    assert_eq!(e.buffer, "[]");
    e.delete();
    assert_eq!(e.buffer, "[");
    e.select_all();
    e.insert("abc\u{7}def", 4);
    assert_eq!(
        e.buffer, "abcd",
        "control characters dropped, length limited"
    );
}

#[test]
fn draft_edits() {
    let mut d = Draft {
        theme: ThemeSelection::default(),
        bar: BarConfig::default(),
    };
    assert!(apply_draft_action(
        &mut d,
        &StudioAction::Background(Some("#121826".into())),
        0.88,
        34
    ));
    assert_eq!(d.bar.style.background.as_deref(), Some("#121826"));
    assert!(apply_draft_action(
        &mut d,
        &StudioAction::OpacityDown,
        0.88,
        34
    ));
    assert_eq!(d.bar.style.opacity, Some(0.83));
    assert!(apply_draft_action(
        &mut d,
        &StudioAction::HeightUp,
        0.88,
        34
    ));
    assert_eq!(d.bar.height, Some(36));
    assert!(
        !apply_draft_action(&mut d, &StudioAction::ToggleModule("menu".into()), 0.88, 34),
        "menu stays"
    );
    assert!(!apply_draft_action(
        &mut d,
        &StudioAction::RemoveModule("menu".into()),
        0.88,
        34
    ));
    assert!(apply_draft_action(
        &mut d,
        &StudioAction::AddModule(ModuleKind::CustomText),
        0.88,
        34
    ));
    assert!(
        d.bar.modules.iter().any(|m| m.id == "text-2"),
        "unique id for a second custom text"
    );
    assert!(
        !apply_draft_action(&mut d, &StudioAction::AddModule(ModuleKind::Cpu), 0.88, 34),
        "kinds are unique"
    );
    assert!(apply_draft_action(
        &mut d,
        &StudioAction::ToggleHour12All,
        0.88,
        34
    ));
    assert!(
        d.bar
            .modules
            .iter()
            .any(|m| m.kind == ModuleKind::Clock && m.hour12 == Some(true))
    );
    assert!(apply_draft_action(
        &mut d,
        &StudioAction::Slider(SliderKind::ClockSize, 18.0),
        0.88,
        34
    ));
    assert_eq!(d.bar.style.clock_size, Some(18));
    assert!(apply_draft_action(
        &mut d,
        &StudioAction::AllLabels(false),
        0.88,
        34
    ));
    assert!(d.bar.problems().is_empty(), "{:?}", d.bar.problems());
}

#[test]
fn drag_moves_between_regions_at_an_index() {
    let mut b = BarConfig::default();
    let i = b.modules.iter().position(|m| m.id == "ram").unwrap();
    move_module(&mut b, i, Region::Center, 0);
    let center: Vec<&str> = b
        .modules
        .iter()
        .filter(|m| m.region == Region::Center)
        .map(|m| m.id.as_str())
        .collect();
    assert_eq!(center, ["ram", "clock"]);
}

#[test]
fn monitor_toggle_records_stable_id() {
    let mut b = BarConfig::default();
    let mon = MonitorEntry {
        device: r"\\.\DISPLAY2".into(),
        id: Some("mon-b".into()),
        name: "Second".into(),
        primary: false,
    };
    toggle_monitor(&mut b, &mon);
    assert!(!b.monitor_enabled(r"\\.\DISPLAY7", Some("mon-b"), false));
    toggle_monitor(&mut b, &mon);
    assert!(b.monitor_enabled(r"\\.\DISPLAY2", Some("mon-b"), false));
}

#[test]
fn phase9_module_appearance_animations_and_font_filters() {
    use windowsrice_core::bar::{Density, ModuleContent};
    let mut d = Draft {
        theme: ThemeSelection::default(),
        bar: BarConfig::default(),
    };
    let set = |f: ModuleField| StudioAction::ModuleSet("cpu".into(), f);
    assert!(apply_draft_action(
        &mut d,
        &set(ModuleField::Color(Some("#36D1C4".into()))),
        0.88,
        34
    ));
    assert!(apply_draft_action(
        &mut d,
        &set(ModuleField::FontSize(3)),
        0.88,
        34
    ));
    assert!(apply_draft_action(
        &mut d,
        &set(ModuleField::Content(Some(ModuleContent::Full))),
        0.88,
        34
    ));
    assert!(apply_draft_action(
        &mut d,
        &set(ModuleField::Density(Some(Density::Compact))),
        0.88,
        34
    ));
    // Kind-specific options are ignored on other kinds.
    assert!(!apply_draft_action(
        &mut d,
        &set(ModuleField::ShowDate(false)),
        0.88,
        34
    ));
    let cpu = d.bar.modules.iter().find(|m| m.id == "cpu").unwrap();
    assert_eq!(cpu.font_size, Some(16));
    assert!(d.bar.problems().is_empty(), "{:?}", d.bar.problems());
    assert!(apply_draft_action(
        &mut d,
        &set(ModuleField::Reset),
        0.88,
        34
    ));
    assert_eq!(
        d.bar.modules,
        BarConfig::default().modules,
        "reset restores the bar style"
    );
    assert!(apply_draft_action(
        &mut d,
        &StudioAction::ToggleAnimations,
        0.88,
        34
    ));
    assert!(!d.bar.animations);

    // The appearance editor of the selected module: every control works.
    let mut m = model(Section::Modules);
    m.selected_module = Some("clock".into());
    m.scroll = 5000.0;
    let s = scene(
        &m,
        Size {
            w: 2400.0,
            h: 1300.0,
        },
        Lang::En,
    );
    let look: Vec<_> = s
        .scene
        .tree
        .iter()
        .filter(|e| e.automation_id.starts_with("look:"))
        .collect();
    assert!(
        look.iter().any(|e| e.automation_id == "look:date"),
        "clock option shown"
    );
    assert!(
        look.iter()
            .filter(|e| e.automation_id.starts_with("look:content:"))
            .count()
            == 5
    );
    for e in look
        .iter()
        .filter(|e| e.invokable || e.selected.is_some() || e.toggled.is_some())
    {
        assert!(
            matches!(s.action(e.id), Some(StudioAction::ModuleSet(id, _)) if id == "clock"),
            "{}",
            e.automation_id
        );
    }

    // Font filters.
    let mut m = model(Section::Typography);
    m.font_filter = FontFilter::Korean;
    let s = scene(
        &m,
        Size {
            w: 2400.0,
            h: 2400.0,
        },
        Lang::En,
    );
    assert_eq!(
        s.grids
            .iter()
            .find(|g| g.prefix == "font")
            .unwrap()
            .rects
            .len(),
        2
    );
    m.font_filter = FontFilter::Monospace;
    let s = scene(
        &m,
        Size {
            w: 2400.0,
            h: 2400.0,
        },
        Lang::En,
    );
    assert_eq!(
        s.grids
            .iter()
            .find(|g| g.prefix == "font")
            .unwrap()
            .rects
            .len(),
        1
    );
    // The animation switch lives in Advanced.
    let s = scene(
        &model(Section::Advanced),
        Size {
            w: 2400.0,
            h: 2400.0,
        },
        Lang::Ko,
    );
    assert!(
        s.scene
            .tree
            .iter()
            .any(|e| e.automation_id == "adv:animations" && e.toggled == Some(true))
    );
}

#[test]
fn monitor_toggle_while_unidentified_edits_the_matching_setting() {
    let mut b = BarConfig::default();
    let known = MonitorEntry {
        device: r"\.\DISPLAY1".into(),
        id: Some("mon-a".into()),
        name: "MSI".into(),
        primary: true,
    };
    toggle_monitor(&mut b, &known);
    assert!(!b.monitor_enabled(r"\.\DISPLAY1", Some("mon-a"), true));
    // The monitor sleeps: same device, no id. Toggling edits the same
    // setting (keeping its id) instead of adding a second one.
    let asleep = MonitorEntry {
        id: None,
        ..known.clone()
    };
    toggle_monitor(&mut b, &asleep);
    assert_eq!(b.monitor_overrides.len(), 1);
    assert_eq!(b.monitor_overrides[0].id.as_deref(), Some("mon-a"));
    assert!(
        b.monitor_enabled(r"\.\DISPLAY1", Some("mon-a"), true),
        "woken monitor sees the toggle"
    );
}
