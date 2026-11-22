//! Module detail panels, calendar and notification center (Phase 13):
//! accessible trees, real-or-unavailable values, controls with actions.

use windowsrice_core::bar::ModuleKind;
use windowsrice_core::config::{ThemeModePreference, ThemeSelection};
use windowsrice_theme::catalog::ThemeCatalog;
use windowsrice_theme::engine::{NoopSink, ThemeEngine};
use windowsrice_theme::schema::Mode;
use windowsrice_ui::panels::*;
use windowsrice_ui::strings::Lang;
use windowsrice_ui::style::PanelStyle;
use windowsrice_ui::text::ApproxMeasure;
use windowsrice_ui::tree::Role;

fn style() -> PanelStyle {
    let (engine, _) = ThemeEngine::start(
        ThemeCatalog::load(None),
        &ThemeSelection {
            active: "windowsrice-default".into(),
            mode: ThemeModePreference::Dark,
        },
        Some(Mode::Dark),
        &mut NoopSink,
    );
    PanelStyle::from_theme(engine.applied().unwrap(), None, true)
}

fn check(m: &PanelModel, lang: Lang) -> PanelScene {
    let s = build_panel(m, &style(), &ApproxMeasure, 1.5, lang);
    let t = &s.scene.tree;
    assert!(t.problems().is_empty(), "{:?}: {:#?}", m.kind, t.problems());
    assert_eq!(t.root().role, Role::Window);
    for e in t.iter().filter(|e| e.invokable || e.value.is_some()) {
        assert!(
            s.action(e.id).is_some(),
            "{} has no action",
            e.automation_id
        );
        assert!(!e.name.trim().is_empty());
    }
    assert!(
        s.action(
            t.iter()
                .find(|e| e.automation_id == "panel:close")
                .unwrap()
                .id
        ) == Some(&PanelAction::Close)
    );
    s
}

#[test]
fn cpu_detail_shows_history_and_marks_unavailable_values() {
    let mut m = PanelModel::new(PanelKind::Detail(ModuleKind::Cpu), "CPU");
    m.value = Some("45%".into());
    m.history = Some(History {
        values: vec![10.0, 50.0, 45.0],
        interval_s: 2.0,
    });
    m.rows = vec![
        ("Processor".into(), Some("Example CPU".into())),
        ("Per-core usage".into(), None),
    ];
    for lang in [Lang::En, Lang::Ko] {
        let s = check(&m, lang);
        let t = &s.scene.tree;
        let graph = t.iter().find(|e| e.automation_id == "panel:graph").unwrap();
        if lang == Lang::En {
            assert!(graph.name.contains("highest 50%"), "{}", graph.name);
            let row = t.iter().find(|e| e.automation_id == "panel:row1").unwrap();
            assert_eq!(row.name, "Per-core usage: not available");
        }
    }
}

#[test]
fn volume_panel_has_slider_and_buttons() {
    let mut m = PanelModel::new(PanelKind::Detail(ModuleKind::Volume), "Volume");
    m.volume = Some((40, false));
    let s = check(&m, Lang::En);
    let t = &s.scene.tree;
    let slider = t
        .iter()
        .find(|e| e.automation_id == "panel:volume")
        .unwrap();
    assert_eq!(slider.role, Role::Slider);
    assert_eq!(slider.value.as_deref(), Some("40%"));
    let up = t
        .iter()
        .find(|e| e.automation_id == "panel:vol-up")
        .unwrap();
    assert_eq!(s.action(up.id), Some(&PanelAction::VolumeStep(5)));
    let mute = t.iter().find(|e| e.automation_id == "panel:mute").unwrap();
    assert_eq!(mute.name, "Mute");
}

#[test]
fn calendar_lists_the_month_and_marks_today() {
    let mut m = PanelModel::new(PanelKind::Calendar, "Calendar");
    m.calendar = Some(CalendarView {
        today: (2026, 11, 20),
        shown: (2026, 11),
        selected: Some((2026, 11, 3)),
        monday_first: true,
        time: "14:05".into(),
        date: "Friday, 20 November 2026".into(),
        zones: vec![("Seoul".into(), "23:05".into())],
    });
    for lang in [Lang::En, Lang::Ko] {
        let s = check(&m, lang);
        let t = &s.scene.tree;
        let days: Vec<_> = t
            .iter()
            .filter(|e| e.automation_id.starts_with("cal:day:"))
            .collect();
        assert_eq!(days.len(), 30);
        if lang == Lang::En {
            let today = days
                .iter()
                .find(|e| e.automation_id == "cal:day:20")
                .unwrap();
            assert_eq!(today.name, "Friday, November 20, 2026, today");
            let sel = days
                .iter()
                .find(|e| e.automation_id == "cal:day:3")
                .unwrap();
            assert_eq!(sel.selected, Some(true));
            assert_eq!(s.action(sel.id), Some(&PanelAction::SelectDay(2026, 11, 3)));
            assert!(
                t.iter()
                    .any(|e| e.automation_id == "cal:month" && e.name == "November 2026")
            );
            assert!(t.iter().any(|e| e.name.contains("Seoul")));
        }
    }
}

#[test]
fn notification_center_explains_access_and_keeps_app_notices_apart() {
    let mut m = PanelModel::new(PanelKind::Notifications, "Notifications");
    m.notifications = Some(NotificationView {
        access: NotificationAccess::Unavailable(
            "Reading Windows notifications needs package identity.".into(),
        ),
        windows_state: "Do not disturb is off".into(),
        app: vec![AppNotice {
            id: 1,
            title: "Design applied".into(),
            body: "Neon".into(),
            time: "14:00".into(),
            unread: true,
        }],
    });
    let s = check(&m, Lang::En);
    let t = &s.scene.tree;
    assert!(
        t.iter()
            .any(|e| e.automation_id == "notif:access" && e.name.contains("package identity"))
    );
    assert!(
        t.iter()
            .any(|e| e.automation_id == "notif:app" && e.name.contains("1 unread"))
    );
    assert!(
        t.iter()
            .any(|e| e.name.starts_with("Unread. Design applied"))
    );
}
