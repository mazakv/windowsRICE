//! Panels that open from bar modules (Phase 13): module details with a
//! recent-history graph, the calendar, and the notification center.
//!
//! Pure scene builders like the bar and the settings popup: the
//! application supplies a [`PanelModel`] with real readings (anything a
//! provider cannot measure is passed as `None` and shown as unavailable,
//! never invented) and gets back an accessible element tree, a draw list
//! and the actions of its controls.

use windowsrice_core::bar::ModuleKind;
use windowsrice_theme::Color;

use crate::draw::{DrawCmd, DrawList, FontRole, TextStyle};
use crate::geom::{Rect, Size};
use crate::panel::Scene;
use crate::strings::Lang;
use crate::style::PanelStyle;
use crate::text::TextMeasure;
use crate::tree::{Element, ElementId, Role, UiTree, intern};

pub const PANEL_WIDTH: f32 = 380.0;
const PAD: f32 = 16.0;
const BTN: f32 = 32.0;

/// Which panel is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    Detail(ModuleKind),
    Calendar,
    Notifications,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PanelAction {
    Close,
    PrevMonth,
    NextMonth,
    Today,
    SelectDay(i32, u32, u32),
    /// Opens the Windows Clock app.
    OpenWindowsClock,
    /// Volume up or down by this many percent.
    VolumeStep(i8),
    ToggleMute,
    /// Opens Windows' own notification center (Win+N).
    OpenNotificationCenter,
    /// Opens Settings > System > Notifications.
    OpenNotificationSettings,
    /// Marks windowsRICE's own notices as read.
    ReadAppNotices,
    ClearAppNotices,
}

/// Recent values for the graph, oldest first, as percentages (0–100).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct History {
    pub values: Vec<f32>,
    /// Seconds between samples.
    pub interval_s: f32,
}

/// A bounded history buffer (at most `cap` values, oldest dropped).
#[derive(Debug, Clone, PartialEq)]
pub struct Ring {
    cap: usize,
    v: std::collections::VecDeque<f32>,
}

impl Ring {
    pub fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            v: std::collections::VecDeque::with_capacity(cap.max(1)),
        }
    }
    pub fn push(&mut self, x: f32) {
        if self.v.len() == self.cap {
            self.v.pop_front();
        }
        self.v.push_back(x);
    }
    pub fn len(&self) -> usize {
        self.v.len()
    }
    pub fn is_empty(&self) -> bool {
        self.v.is_empty()
    }
    pub fn values(&self) -> Vec<f32> {
        self.v.iter().copied().collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CalendarView {
    pub today: (i32, u32, u32),
    /// Month shown (year, month 1–12).
    pub shown: (i32, u32),
    pub selected: Option<(i32, u32, u32)>,
    pub monday_first: bool,
    /// Current time and date as formatted by the clock module.
    pub time: String,
    pub date: String,
    /// Additional time zones: (name, time).
    pub zones: Vec<(String, String)>,
}

/// Access to Windows notifications.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationAccess {
    /// Not possible in this installation (the reason is shown).
    Unavailable(String),
}

/// One of windowsRICE's own notices (kept apart from system notifications).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppNotice {
    pub id: u32,
    pub title: String,
    pub body: String,
    pub time: String,
    pub unread: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NotificationView {
    pub access: NotificationAccess,
    /// Windows' own state (Do not disturb and similar), as text.
    pub windows_state: String,
    pub app: Vec<AppNotice>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PanelModel {
    pub kind: PanelKind,
    pub title: String,
    /// The main reading, such as "45%"; `None` when unavailable.
    pub value: Option<String>,
    pub history: Option<History>,
    /// (label, value); a `None` value is shown as unavailable.
    pub rows: Vec<(String, Option<String>)>,
    pub note: Option<String>,
    /// Output volume percent and mute (Volume panel).
    pub volume: Option<(u8, bool)>,
    pub calendar: Option<CalendarView>,
    pub notifications: Option<NotificationView>,
    pub hover: Option<ElementId>,
}

impl PanelModel {
    pub fn new(kind: PanelKind, title: impl Into<String>) -> Self {
        Self {
            kind,
            title: title.into(),
            value: None,
            history: None,
            rows: Vec::new(),
            note: None,
            volume: None,
            calendar: None,
            notifications: None,
            hover: None,
        }
    }
}

#[derive(Debug)]
pub struct PanelScene {
    pub scene: Scene,
    pub actions: Vec<(ElementId, PanelAction)>,
}

impl PanelScene {
    pub fn action(&self, id: ElementId) -> Option<&PanelAction> {
        self.actions.iter().find(|(e, _)| *e == id).map(|(_, a)| a)
    }
}

// ------------------------------------------------------------ calendar math

pub fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if is_leap(y) => 29,
        _ => 28,
    }
}

/// Day of the week, 0 = Monday … 6 = Sunday (proleptic Gregorian).
pub fn weekday(y: i32, m: u32, d: u32) -> u32 {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if m < 3 { y - 1 } else { y };
    // Sakamoto: 0 = Sunday.
    let s = (y + y / 4 - y / 100 + y / 400 + T[(m - 1) as usize] + d as i32).rem_euclid(7);
    ((s + 6) % 7) as u32
}

pub fn add_months(y: i32, m: u32, delta: i32) -> (i32, u32) {
    let idx = y * 12 + m as i32 - 1 + delta;
    (idx.div_euclid(12), (idx.rem_euclid(12) + 1) as u32)
}

/// Six weeks of day numbers for a month (`None` outside it).
pub fn month_grid(y: i32, m: u32, monday_first: bool) -> Vec<Option<u32>> {
    let first = weekday(y, m, 1);
    let offset = if monday_first { first } else { (first + 1) % 7 };
    let n = days_in_month(y, m);
    (0..42)
        .map(|i| {
            let d = i as i64 - i64::from(offset) + 1;
            (d >= 1 && d <= i64::from(n)).then_some(d as u32)
        })
        .collect()
}

pub fn month_name(m: u32, lang: Lang) -> &'static str {
    const EN: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    const KO: [&str; 12] = [
        "1월", "2월", "3월", "4월", "5월", "6월", "7월", "8월", "9월", "10월", "11월", "12월",
    ];
    let i = (m.clamp(1, 12) - 1) as usize;
    if lang == Lang::En { EN[i] } else { KO[i] }
}

/// Weekday names starting Monday: (short, long).
fn weekday_names(lang: Lang) -> [(&'static str, &'static str); 7] {
    if lang == Lang::En {
        [
            ("Mo", "Monday"),
            ("Tu", "Tuesday"),
            ("We", "Wednesday"),
            ("Th", "Thursday"),
            ("Fr", "Friday"),
            ("Sa", "Saturday"),
            ("Su", "Sunday"),
        ]
    } else {
        [
            ("월", "월요일"),
            ("화", "화요일"),
            ("수", "수요일"),
            ("목", "목요일"),
            ("금", "금요일"),
            ("토", "토요일"),
            ("일", "일요일"),
        ]
    }
}

/// Text of an unread badge: none for 0, the number up to 99, then "99+".
pub fn badge_text(unread: usize) -> Option<String> {
    match unread {
        0 => None,
        1..=99 => Some(unread.to_string()),
        _ => Some("99+".into()),
    }
}

// ------------------------------------------------------------ building

fn key_id(key: &str) -> ElementId {
    let mut h: u32 = 0x811C_9DC5;
    for b in key.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    ElementId(0x100 | (h & 0x7FFF_FFFF))
}

fn t(lang: Lang, en: &'static str, ko: &'static str) -> &'static str {
    if lang == Lang::En { en } else { ko }
}

struct B<'a> {
    tree: UiTree,
    draw: DrawList,
    actions: Vec<(ElementId, PanelAction)>,
    style: &'a PanelStyle,
    model: &'a PanelModel,
    measure: &'a dyn TextMeasure,
    scale: f32,
}

impl B<'_> {
    fn ts(&self, size: f32, weight: u16) -> TextStyle {
        TextStyle {
            font: FontRole::Ui,
            size,
            weight,
        }
    }
    fn add(&mut self, parent: ElementId, mut e: Element) -> ElementId {
        while self.tree.contains(e.id) {
            e.id = ElementId(e.id.0.wrapping_add(1) | 0x100);
        }
        let id = e.id;
        self.tree.add(parent, e).expect("ids made unique above");
        id
    }
    fn text(&mut self, r: Rect, s: &str, ts: TextStyle, color: Color, owner: ElementId) {
        self.draw.push(DrawCmd::Text {
            rect: r,
            text: s.to_owned(),
            style: ts,
            color,
            trim: true,
            owner: Some(owner),
        });
    }
    #[allow(clippy::too_many_arguments)]
    fn label(
        &mut self,
        parent: ElementId,
        key: &str,
        r: Rect,
        s: &str,
        size: f32,
        weight: u16,
        color: Color,
    ) {
        let r = r.snap(self.scale);
        let id = self.add(
            parent,
            Element::new(key_id(key), intern(key), Role::Text, s, r),
        );
        let ts = self.ts(size, weight);
        self.text(r, s, ts, color, id);
    }
    #[allow(clippy::too_many_arguments)]
    fn button(
        &mut self,
        parent: ElementId,
        key: &str,
        r: Rect,
        glyph: Option<&str>,
        label: &str,
        name: &str,
        selected: Option<bool>,
        action: PanelAction,
    ) -> ElementId {
        let r = r.snap(self.scale);
        let mut e = Element::new(key_id(key), intern(key), Role::Button, name, r)
            .focusable(true)
            .invokable();
        if let Some(s) = selected {
            e = e.selected(s);
        }
        let id = self.add(parent, e);
        if self.model.hover == Some(id) {
            self.draw.push(DrawCmd::Fill {
                rect: r,
                radius: self.style.radius_control,
                color: self.style.hover,
                owner: Some(id),
            });
        }
        let mut x = r.x + 8.0;
        if let Some(gl) = glyph {
            let ts = TextStyle {
                font: FontRole::Icon,
                size: self.style.size_body,
                weight: 400,
            };
            let gw = self.measure.measure(gl, &ts).w;
            let gx = if label.is_empty() {
                r.x + (r.w - gw) / 2.0
            } else {
                x
            };
            self.draw.push(DrawCmd::Text {
                rect: Rect::new(gx, r.y, gw.max(1.0), r.h),
                text: gl.to_owned(),
                style: ts,
                color: self.style.text_primary,
                trim: false,
                owner: Some(id),
            });
            x += gw + 8.0;
        }
        if !label.is_empty() {
            let ts = self.ts(self.style.size_body, 400);
            self.text(
                Rect::new(x, r.y, r.right() - x - 4.0, r.h),
                label,
                ts,
                self.style.text_primary,
                id,
            );
        }
        self.actions.push((id, action));
        id
    }
}

/// Builds a panel scene. Height follows the content.
pub fn build_panel(
    model: &PanelModel,
    style: &PanelStyle,
    measure: &dyn TextMeasure,
    scale: f32,
    lang: Lang,
) -> PanelScene {
    let w = PANEL_WIDTH;
    let root = ElementId(1);
    let mut b = B {
        tree: UiTree::new(Element::new(
            root,
            "panel",
            Role::Window,
            model.title.clone(),
            Rect::new(0.0, 0.0, w, 100.0),
        )),
        draw: DrawList::default(),
        actions: Vec::new(),
        style,
        model,
        measure,
        scale,
    };
    // Background (filled after the height is known: inserted at index 0).
    let mut y = PAD;
    b.label(
        root,
        "panel:title",
        Rect::new(PAD, y, w - 2.0 * PAD - BTN - 8.0, BTN),
        &model.title,
        style.size_body + 2.0,
        style.weight_emphasis,
        style.text_primary,
    );
    let close = match lang {
        Lang::En => format!("Close {}", model.title),
        Lang::Ko => format!("{} 닫기", model.title),
    };
    b.button(
        root,
        "panel:close",
        Rect::new(w - PAD - BTN, y, BTN, BTN),
        Some("\u{E711}"),
        "",
        &close,
        None,
        PanelAction::Close,
    );
    y += BTN + 8.0;

    if let Some(v) = &model.value {
        b.label(
            root,
            "panel:value",
            Rect::new(PAD, y, w - 2.0 * PAD, 40.0),
            v,
            28.0,
            style.weight_emphasis,
            style.text_primary,
        );
        y += 46.0;
    }

    // History graph: one column per sample.
    if let Some(h) = model.history.as_ref().filter(|h| !h.values.is_empty()) {
        let gh = 64.0;
        let r = Rect::new(PAD, y, w - 2.0 * PAD, gh).snap(scale);
        let (lo, hi) = h
            .values
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, z), v| (a.min(*v), z.max(*v)));
        let span = h.interval_s * h.values.len() as f32;
        let name = match lang {
            Lang::En => format!(
                "History, last {:.0} seconds: lowest {lo:.0}%, highest {hi:.0}%, now {:.0}%",
                span,
                h.values.last().copied().unwrap_or(0.0)
            ),
            Lang::Ko => format!(
                "최근 {:.0}초 기록: 최저 {lo:.0}%, 최고 {hi:.0}%, 현재 {:.0}%",
                span,
                h.values.last().copied().unwrap_or(0.0)
            ),
        };
        let gid = b.add(
            root,
            Element::new(key_id("panel:graph"), "panel:graph", Role::Text, name, r),
        );
        b.draw.push(DrawCmd::Fill {
            rect: r,
            radius: style.radius_control,
            color: Color {
                a: 0x22,
                ..style.text_primary
            },
            owner: Some(gid),
        });
        let n = h.values.len() as f32;
        let cw = (r.w - 8.0) / n.max(1.0);
        for (i, v) in h.values.iter().enumerate() {
            let vh = (v.clamp(0.0, 100.0) / 100.0 * (r.h - 8.0)).max(1.0);
            b.draw.push(DrawCmd::Fill {
                rect: Rect::new(
                    r.x + 4.0 + i as f32 * cw,
                    r.bottom() - 4.0 - vh,
                    (cw - 1.0).max(1.0),
                    vh,
                ),
                radius: 0.0,
                color: style.accent,
                owner: Some(gid),
            });
        }
        y += gh + 12.0;
    }

    // Rows of details.
    for (i, (k, v)) in model.rows.iter().enumerate() {
        let shown = v
            .clone()
            .unwrap_or_else(|| t(lang, "not available", "사용할 수 없음").to_owned());
        let name = format!("{k}: {shown}");
        let r = Rect::new(PAD, y, w - 2.0 * PAD, 24.0).snap(scale);
        let id = b.add(
            root,
            Element::new(
                key_id(&format!("panel:row{i}")),
                intern(&format!("panel:row{i}")),
                Role::Text,
                name,
                r,
            ),
        );
        let ts = b.ts(style.size_body, 400);
        b.text(
            Rect::new(r.x, r.y, r.w * 0.5, r.h),
            k,
            ts,
            style.text_secondary,
            id,
        );
        let col = if v.is_some() {
            style.text_primary
        } else {
            style.text_disabled
        };
        b.text(
            Rect::new(r.x + r.w * 0.5, r.y, r.w * 0.5, r.h),
            &shown,
            ts,
            col,
            id,
        );
        y += 26.0;
    }

    // Volume controls.
    if let Some((pct, muted)) = model.volume {
        y += 4.0;
        let name = match (lang, muted) {
            (Lang::En, false) => format!("Volume {pct}%"),
            (Lang::En, true) => format!("Volume {pct}%, muted"),
            (Lang::Ko, false) => format!("볼륨 {pct}%"),
            (Lang::Ko, true) => format!("볼륨 {pct}%, 음소거"),
        };
        let r = Rect::new(PAD + BTN + 8.0, y, w - 2.0 * PAD - 3.0 * BTN - 24.0, BTN).snap(scale);
        let sid = b.add(
            root,
            Element::new(
                key_id("panel:volume"),
                "panel:volume",
                Role::Slider,
                name,
                r,
            )
            .focusable(true)
            .value(format!("{pct}%")),
        );
        let track = Rect::new(r.x, r.center_y() - 3.0, r.w, 6.0);
        b.draw.push(DrawCmd::Fill {
            rect: track,
            radius: 3.0,
            color: Color {
                a: 0x40,
                ..style.text_primary
            },
            owner: Some(sid),
        });
        b.draw.push(DrawCmd::Fill {
            rect: Rect::new(
                track.x,
                track.y,
                track.w * f32::from(pct.min(100)) / 100.0,
                6.0,
            ),
            radius: 3.0,
            color: if muted {
                style.text_disabled
            } else {
                style.accent
            },
            owner: Some(sid),
        });
        b.actions.push((sid, PanelAction::VolumeStep(0)));
        b.button(
            root,
            "panel:vol-down",
            Rect::new(PAD, y, BTN, BTN),
            Some("\u{E738}"),
            "",
            t(lang, "Volume down", "볼륨 낮추기"),
            None,
            PanelAction::VolumeStep(-5),
        );
        b.button(
            root,
            "panel:vol-up",
            Rect::new(r.right() + 8.0, y, BTN, BTN),
            Some("\u{E710}"),
            "",
            t(lang, "Volume up", "볼륨 높이기"),
            None,
            PanelAction::VolumeStep(5),
        );
        let mute_name = if muted {
            t(lang, "Unmute", "음소거 해제")
        } else {
            t(lang, "Mute", "음소거")
        };
        b.button(
            root,
            "panel:mute",
            Rect::new(w - PAD - BTN, y, BTN, BTN),
            Some(if muted { "\u{E74F}" } else { "\u{E767}" }),
            "",
            mute_name,
            None,
            PanelAction::ToggleMute,
        );
        y += BTN + 12.0;
    }

    if let Some(c) = &model.calendar {
        y = calendar(&mut b, root, c, y, lang);
    }
    if let Some(n) = &model.notifications {
        y = notifications(&mut b, root, n, y, lang);
    }
    if let Some(note) = &model.note {
        let lines = (note.chars().count() as f32 / 52.0).ceil().max(1.0);
        let h = 18.0 * lines + 6.0;
        let r = Rect::new(PAD, y, w - 2.0 * PAD, h).snap(scale);
        let id = b.add(
            root,
            Element::new(
                key_id("panel:note"),
                "panel:note",
                Role::Text,
                note.clone(),
                r,
            ),
        );
        b.draw.push(DrawCmd::Wrapped {
            rect: r,
            text: note.clone(),
            style: b.ts(12.0, 400),
            color: style.text_secondary,
            owner: Some(id),
        });
        y += h + 8.0;
    }

    let h = (y + PAD - 8.0).ceil();
    let full = Rect::new(0.0, 0.0, w, h);
    b.draw.cmds.insert(
        0,
        DrawCmd::Fill {
            rect: full,
            radius: 0.0,
            color: Color {
                a: 255,
                ..style.overlay
            },
            owner: Some(root),
        },
    );
    if let Some(r) = b.tree.get_mut(root) {
        r.bounds = full;
    }
    PanelScene {
        scene: Scene {
            tree: b.tree,
            draw: b.draw,
            size: Size { w, h },
            scale,
        },
        actions: b.actions,
    }
}

fn calendar(b: &mut B<'_>, root: ElementId, c: &CalendarView, mut y: f32, lang: Lang) -> f32 {
    let w = PANEL_WIDTH;
    let style = b.style;
    b.label(
        root,
        "cal:time",
        Rect::new(PAD, y, w - 2.0 * PAD, 40.0),
        &c.time,
        28.0,
        style.weight_emphasis,
        style.text_primary,
    );
    y += 42.0;
    b.label(
        root,
        "cal:date",
        Rect::new(PAD, y, w - 2.0 * PAD, 22.0),
        &c.date,
        style.size_body,
        400,
        style.text_secondary,
    );
    y += 32.0;
    let (sy, sm) = c.shown;
    let title = match lang {
        Lang::En => format!("{} {sy}", month_name(sm, lang)),
        Lang::Ko => format!("{sy}년 {}", month_name(sm, lang)),
    };
    b.label(
        root,
        "cal:month",
        Rect::new(PAD, y, w - 2.0 * PAD - 3.0 * BTN - 16.0, BTN),
        &title,
        style.size_body + 1.0,
        style.weight_emphasis,
        style.text_primary,
    );
    b.button(
        root,
        "cal:prev",
        Rect::new(w - PAD - 3.0 * BTN - 8.0, y, BTN, BTN),
        Some("\u{E70E}"),
        "",
        t(lang, "Previous month", "이전 달"),
        None,
        PanelAction::PrevMonth,
    );
    b.button(
        root,
        "cal:today",
        Rect::new(w - PAD - 2.0 * BTN - 4.0, y, BTN, BTN),
        Some("\u{E8D1}"),
        "",
        t(lang, "Today", "오늘"),
        None,
        PanelAction::Today,
    );
    b.button(
        root,
        "cal:next",
        Rect::new(w - PAD - BTN, y, BTN, BTN),
        Some("\u{E70D}"),
        "",
        t(lang, "Next month", "다음 달"),
        None,
        PanelAction::NextMonth,
    );
    y += BTN + 6.0;
    let names = weekday_names(lang);
    let order: Vec<usize> = if c.monday_first {
        (0..7).collect()
    } else {
        std::iter::once(6).chain(0..6).collect()
    };
    let cw = (w - 2.0 * PAD) / 7.0;
    for (col, &wd) in order.iter().enumerate() {
        let r = Rect::new(PAD + col as f32 * cw, y, cw, 22.0);
        b.label(
            root,
            &format!("cal:wd{col}"),
            r,
            names[wd].0,
            12.0,
            500,
            style.text_secondary,
        );
    }
    y += 24.0;
    let grid = b.add(
        root,
        Element::new(
            key_id("cal:grid"),
            "cal:grid",
            Role::Group,
            title.clone(),
            Rect::new(PAD, y, w - 2.0 * PAD, 6.0 * 34.0),
        ),
    );
    for (i, d) in month_grid(sy, sm, c.monday_first).iter().enumerate() {
        let Some(d) = *d else { continue };
        let (row, col) = (i / 7, i % 7);
        let r = Rect::new(
            PAD + col as f32 * cw + 2.0,
            y + row as f32 * 34.0 + 1.0,
            cw - 4.0,
            32.0,
        )
        .snap(b.scale);
        let date = (sy, sm, d);
        let is_today = date == c.today;
        let is_sel = c.selected == Some(date);
        let wd = weekday(sy, sm, d) as usize;
        let name = match lang {
            Lang::En => format!(
                "{}, {} {d}, {sy}{}",
                names[wd].1,
                month_name(sm, lang),
                if is_today { ", today" } else { "" }
            ),
            Lang::Ko => format!(
                "{sy}년 {} {d}일 {}{}",
                month_name(sm, lang),
                names[wd].1,
                if is_today { ", 오늘" } else { "" }
            ),
        };
        let id = b.add(
            grid,
            Element::new(
                key_id(&format!("cal:day:{d}")),
                intern(&format!("cal:day:{d}")),
                Role::Button,
                name,
                r,
            )
            .focusable(true)
            .invokable()
            .selected(is_sel),
        );
        if is_today {
            b.draw.push(DrawCmd::Fill {
                rect: r,
                radius: r.h / 2.0,
                color: style.accent,
                owner: Some(id),
            });
        } else if is_sel {
            b.draw.push(DrawCmd::Stroke {
                rect: r,
                radius: r.h / 2.0,
                width: 2.0,
                color: style.accent,
                owner: Some(id),
            });
        } else if b.model.hover == Some(id) {
            b.draw.push(DrawCmd::Fill {
                rect: r,
                radius: r.h / 2.0,
                color: style.hover,
                owner: Some(id),
            });
        }
        let s = d.to_string();
        let ts = b.ts(13.0, if is_today { 600 } else { 400 });
        let tw = b.measure.measure(&s, &ts).w;
        let col_ = if is_today {
            style.text_on_accent
        } else {
            style.text_primary
        };
        b.text(
            Rect::new(r.x + (r.w - tw) / 2.0, r.y, tw.max(1.0), r.h),
            &s,
            ts,
            col_,
            id,
        );
        b.actions.push((id, PanelAction::SelectDay(sy, sm, d)));
    }
    y += 6.0 * 34.0 + 8.0;
    for (i, (zone, time)) in c.zones.iter().enumerate() {
        b.label(
            root,
            &format!("cal:zone{i}"),
            Rect::new(PAD, y, w - 2.0 * PAD, 22.0),
            &format!("{zone}  {time}"),
            style.size_body,
            400,
            style.text_secondary,
        );
        y += 24.0;
    }
    b.button(
        root,
        "cal:windows",
        Rect::new(PAD, y, w - 2.0 * PAD, BTN),
        Some("\u{E823}"),
        t(lang, "Open Windows Clock", "Windows 시계 열기"),
        t(lang, "Open Windows Clock", "Windows 시계 열기"),
        None,
        PanelAction::OpenWindowsClock,
    );
    y + BTN + 12.0
}

fn notifications(
    b: &mut B<'_>,
    root: ElementId,
    n: &NotificationView,
    mut y: f32,
    lang: Lang,
) -> f32 {
    let w = PANEL_WIDTH;
    let style = b.style;
    b.label(
        root,
        "notif:state",
        Rect::new(PAD, y, w - 2.0 * PAD, 22.0),
        &n.windows_state,
        style.size_body,
        400,
        style.text_secondary,
    );
    y += 28.0;
    let NotificationAccess::Unavailable(why) = &n.access;
    let r = Rect::new(PAD, y, w - 2.0 * PAD, 76.0).snap(b.scale);
    let id = b.add(
        root,
        Element::new(
            key_id("notif:access"),
            "notif:access",
            Role::Text,
            why.clone(),
            r,
        ),
    );
    b.draw.push(DrawCmd::Wrapped {
        rect: r,
        text: why.clone(),
        style: b.ts(12.0, 400),
        color: style.text_secondary,
        owner: Some(id),
    });
    y += 82.0;
    b.button(
        root,
        "notif:open",
        Rect::new(PAD, y, w - 2.0 * PAD, BTN),
        Some("\u{E7E7}"),
        t(lang, "Open Windows notifications", "Windows 알림 열기"),
        t(lang, "Open Windows notifications", "Windows 알림 열기"),
        None,
        PanelAction::OpenNotificationCenter,
    );
    y += BTN + 6.0;
    b.button(
        root,
        "notif:settings",
        Rect::new(PAD, y, w - 2.0 * PAD, BTN),
        Some("\u{E713}"),
        t(lang, "Notification settings", "알림 설정"),
        t(lang, "Notification settings", "알림 설정"),
        None,
        PanelAction::OpenNotificationSettings,
    );
    y += BTN + 12.0;
    let unread = n.app.iter().filter(|a| a.unread).count();
    let head = match lang {
        Lang::En => format!("windowsRICE notices ({unread} unread)"),
        Lang::Ko => format!("windowsRICE 알림 (읽지 않음 {unread}개)"),
    };
    b.label(
        root,
        "notif:app",
        Rect::new(PAD, y, w - 2.0 * PAD, 22.0),
        &head,
        style.size_body,
        style.weight_emphasis,
        style.text_primary,
    );
    y += 26.0;
    if n.app.is_empty() {
        b.label(
            root,
            "notif:none",
            Rect::new(PAD, y, w - 2.0 * PAD, 22.0),
            t(lang, "No notices.", "알림이 없습니다."),
            style.size_body,
            400,
            style.text_secondary,
        );
        y += 26.0;
    }
    for a in n.app.iter().take(8) {
        let name = format!(
            "{}{}: {} ({})",
            if a.unread {
                t(lang, "Unread. ", "읽지 않음. ")
            } else {
                ""
            },
            a.title,
            a.body,
            a.time
        );
        let r = Rect::new(PAD, y, w - 2.0 * PAD, 40.0).snap(b.scale);
        let id = b.add(
            root,
            Element::new(
                key_id(&format!("notif:{}", a.id)),
                intern(&format!("notif:{}", a.id)),
                Role::Text,
                name,
                r,
            ),
        );
        if a.unread {
            b.draw.push(DrawCmd::Fill {
                rect: Rect::new(r.x, r.y + 14.0, 6.0, 6.0),
                radius: 3.0,
                color: style.accent,
                owner: Some(id),
            });
        }
        let t1 = b.ts(style.size_body, 500);
        b.text(
            Rect::new(r.x + 12.0, r.y, r.w - 80.0, 20.0),
            &a.title,
            t1,
            style.text_primary,
            id,
        );
        let t2 = b.ts(12.0, 400);
        b.text(
            Rect::new(r.right() - 64.0, r.y, 64.0, 20.0),
            &a.time,
            t2,
            style.text_secondary,
            id,
        );
        b.text(
            Rect::new(r.x + 12.0, r.y + 20.0, r.w - 12.0, 18.0),
            &a.body,
            t2,
            style.text_secondary,
            id,
        );
        y += 44.0;
    }
    if !n.app.is_empty() {
        let half = (w - 2.0 * PAD - 8.0) / 2.0;
        b.button(
            root,
            "notif:read",
            Rect::new(PAD, y, half, BTN),
            Some("\u{E8FB}"),
            t(lang, "Mark read", "읽음 표시"),
            t(
                lang,
                "Mark windowsRICE notices read",
                "windowsRICE 알림을 읽음으로 표시",
            ),
            None,
            PanelAction::ReadAppNotices,
        );
        b.button(
            root,
            "notif:clear",
            Rect::new(PAD + half + 8.0, y, half, BTN),
            Some("\u{E894}"),
            t(lang, "Clear", "지우기"),
            t(lang, "Clear windowsRICE notices", "windowsRICE 알림 지우기"),
            None,
            PanelAction::ClearAppNotices,
        );
        y += BTN + 8.0;
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_math() {
        assert_eq!(weekday(2026, 11, 20), 4, "20 Nov 2026 is a Friday");
        assert_eq!(weekday(2000, 1, 1), 5, "1 Jan 2000 was a Saturday");
        assert_eq!(weekday(2024, 2, 29), 3, "29 Feb 2024 was a Thursday");
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(1900, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(add_months(2026, 12, 1), (2027, 1));
        assert_eq!(add_months(2026, 1, -1), (2025, 12));
        assert_eq!(add_months(2026, 3, -27), (2023, 12));
        // November 2026 starts on a Sunday.
        let g = month_grid(2026, 11, true);
        assert_eq!(g[6], Some(1));
        assert_eq!(g.iter().flatten().count(), 30);
        let g = month_grid(2026, 11, false);
        assert_eq!(g[0], Some(1));
    }

    #[test]
    fn badge() {
        assert_eq!(badge_text(0), None);
        assert_eq!(badge_text(1).as_deref(), Some("1"));
        assert_eq!(badge_text(12).as_deref(), Some("12"));
        assert_eq!(badge_text(99).as_deref(), Some("99"));
        assert_eq!(badge_text(100).as_deref(), Some("99+"));
    }

    #[test]
    fn ring_is_bounded() {
        let mut r = Ring::new(3);
        for i in 0..10 {
            r.push(i as f32);
        }
        assert_eq!(r.values(), vec![7.0, 8.0, 9.0]);
        assert_eq!(r.len(), 3);
    }
}
