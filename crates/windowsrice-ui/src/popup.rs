//! The windowsRICE settings popup opened from the bar's menu module.
//!
//! Two columns: themes and bar settings on the left, modules on the right
//! (show/hide, refresh interval, order). Every control is an element of the
//! accessibility tree with a role (RadioButton, CheckBox, Button, Text), a
//! full accessible name and a state, and every element maps to a
//! [`PopupAction`]. Element ids are derived from stable keys (theme id,
//! module id, control), so focus stays on the same control after a change
//! such as moving a module.

use std::collections::HashMap;

use windowsrice_theme::Color;

use crate::draw::{DrawCmd, DrawList, FontRole, TextStyle};
use crate::geom::{Rect, Size};
use crate::legibility::SurfacePlan;
use crate::module::{ModuleConfig, ModuleKind, module_name};
use crate::panel::Scene;
use crate::strings::{Lang, Strings};
use crate::style::PanelStyle;
use crate::text::TextMeasure;
use crate::tree::{Element, ElementId, Role, UiTree};

/// Interval steps offered by the − and + buttons (milliseconds).
pub const INTERVAL_STEPS: [u32; 9] = [
    500, 1_000, 2_000, 5_000, 10_000, 30_000, 60_000, 300_000, 600_000,
];

pub const POPUP_WIDTH: f32 = 680.0;
const PAD: f32 = 14.0;
const ROW: f32 = 30.0;
const HEADER: f32 = 30.0;
const COL_GAP: f32 = 20.0;
const LEFT_COL: f32 = 260.0;
const BTN: f32 = 26.0;
/// Width of the interval label ("on change", "10 min").
const INTERVAL_W: f32 = 76.0;

#[derive(Debug, Clone, PartialEq)]
pub enum ModuleStatus {
    Ok,
    Unavailable(String),
    Absent,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PopupAction {
    Close,
    SetTheme(String),
    ToggleModule(String),
    MoveUp(String),
    MoveDown(String),
    IntervalShorter(String),
    IntervalLonger(String),
    HeightDown,
    HeightUp,
    ToggleAutoHide,
    ToggleReserve,
    ToggleAllMonitors,
    OpenStudio,
    Exit,
    /// A control of a module panel (Phase 13).
    Panel(crate::panels::PanelAction),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PopupModel {
    /// `(id, display name)` of every available theme.
    pub themes: Vec<(String, String)>,
    pub active_theme: String,
    pub modules: Vec<ModuleConfig>,
    pub status: HashMap<String, ModuleStatus>,
    /// Effective bar height in DIPs.
    pub height: u16,
    pub auto_hide: bool,
    pub reserve: bool,
    pub all_monitors: bool,
    /// Result of the last change ("Saved", an error, …).
    pub message: Option<String>,
    pub focus: Option<ElementId>,
    pub keyboard_active: bool,
    pub hover: Option<ElementId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PopupScene {
    pub scene: Scene,
    pub actions: Vec<(ElementId, PopupAction)>,
}

impl PopupScene {
    pub fn action(&self, id: ElementId) -> Option<&PopupAction> {
        self.actions.iter().find(|(e, _)| *e == id).map(|(_, a)| a)
    }

    /// Element performing `action`, if present.
    pub fn element(&self, action: &PopupAction) -> Option<ElementId> {
        self.actions
            .iter()
            .find(|(_, a)| a == action)
            .map(|(e, _)| *e)
    }
}

pub mod ids {
    use crate::tree::ElementId;
    pub const ROOT: ElementId = ElementId(1);
    pub const GROUP_THEME: ElementId = ElementId(2);
    pub const GROUP_BAR: ElementId = ElementId(3);
    pub const GROUP_MODULES: ElementId = ElementId(4);
    pub const TITLE: ElementId = ElementId(5);
    pub const CLOSE: ElementId = ElementId(6);
    pub const MESSAGE: ElementId = ElementId(7);
    pub const EXIT: ElementId = ElementId(8);
    pub const STUDIO: ElementId = ElementId(9);
}

fn key_id(key: &str) -> ElementId {
    let mut h: u32 = 0x811C_9DC5;
    for b in key.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    ElementId(0x100 | (h & 0x7FFF_FFFF))
}

pub fn interval_label(ms: Option<u32>, lang: Lang) -> String {
    match ms {
        None => match lang {
            Lang::En => "on change".into(),
            Lang::Ko => "변경 시".into(),
        },
        Some(ms) if ms < 1000 => format!("{:.1} s", f64::from(ms) / 1000.0),
        Some(ms) if ms < 60_000 => format!("{} s", ms / 1000),
        Some(ms) => match lang {
            Lang::En => format!("{} min", ms / 60_000),
            Lang::Ko => format!("{}분", ms / 60_000),
        },
    }
}

/// Next shorter / longer interval step (clamped to the steps).
pub fn step_interval(current: u32, longer: bool) -> u32 {
    if longer {
        INTERVAL_STEPS
            .iter()
            .copied()
            .find(|&s| s > current)
            .unwrap_or(current)
    } else {
        INTERVAL_STEPS
            .iter()
            .rev()
            .copied()
            .find(|&s| s < current)
            .unwrap_or(current)
    }
}

struct Builder<'a> {
    tree: UiTree,
    draw: DrawList,
    actions: Vec<(ElementId, PopupAction)>,
    style: &'a PanelStyle,
    model: &'a PopupModel,
    measure: &'a dyn TextMeasure,
    scale: f32,
    plan_backplates: bool,
}

impl Builder<'_> {
    fn ui(&self, weight: u16) -> TextStyle {
        TextStyle {
            font: FontRole::Ui,
            size: self.style.size_body,
            weight,
        }
    }

    fn icon(&self) -> TextStyle {
        TextStyle {
            font: FontRole::Icon,
            size: self.style.size_body,
            weight: 400,
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

    fn text(&mut self, rect: Rect, s: &str, weight: u16, color: Color, owner: ElementId) {
        self.draw.push(DrawCmd::Text {
            rect,
            text: s.to_owned(),
            style: self.ui(weight),
            color,
            trim: true,
            owner: Some(owner),
        });
    }

    fn interactive_bg(&mut self, id: ElementId, rect: Rect) {
        if self.model.hover == Some(id) {
            self.draw.push(DrawCmd::Fill {
                rect,
                radius: self.style.radius_control,
                color: self.style.hover,
                owner: Some(id),
            });
        }
    }

    fn backplate(&mut self, id: ElementId, rect: Rect) {
        if self.plan_backplates {
            self.draw.push(DrawCmd::Fill {
                rect,
                radius: self.style.radius_control,
                color: self.style.overlay,
                owner: Some(id),
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn button(
        &mut self,
        parent: ElementId,
        key: &str,
        rect: Rect,
        glyph: &str,
        name: String,
        enabled: bool,
        action: PopupAction,
    ) {
        let rect = rect.snap(self.scale);
        let id = self.add(
            parent,
            Element::new(
                key_id(key),
                crate::tree::intern(key),
                Role::Button,
                name,
                rect,
            )
            .focusable(enabled)
            .enabled(enabled)
            .invokable(),
        );
        self.backplate(id, rect);
        self.interactive_bg(id, rect);
        let color = if enabled {
            self.style.text_primary
        } else {
            self.style.text_disabled
        };
        self.draw.push(DrawCmd::Text {
            rect: Rect::new(rect.x, rect.y, rect.w, rect.h),
            text: glyph.to_owned(),
            style: self.icon(),
            color,
            trim: false,
            owner: Some(id),
        });
        // Centre the glyph: the renderer draws text left-aligned, so offset
        // by half the free space.
        if let Some(DrawCmd::Text {
            rect: r,
            text,
            style,
            ..
        }) = self.draw.cmds.last_mut()
        {
            let gw = self.measure.measure(text, style).w;
            r.x += ((r.w - gw) / 2.0).max(0.0);
            r.w = gw.min(r.w);
        }
        self.actions.push((id, action));
    }

    #[allow(clippy::too_many_arguments)]
    fn check(
        &mut self,
        parent: ElementId,
        key: &str,
        rect: Rect,
        label: &str,
        name: String,
        on: bool,
        enabled: bool,
        help: Option<String>,
        action: PopupAction,
    ) {
        let rect = rect.snap(self.scale);
        let mut e = Element::new(
            key_id(key),
            crate::tree::intern(key),
            Role::CheckBox,
            name,
            rect,
        )
        .focusable(enabled)
        .enabled(enabled)
        .toggled(on);
        if let Some(h) = help {
            e = e.help(h);
        }
        let id = self.add(parent, e);
        self.backplate(id, rect);
        self.interactive_bg(id, rect);
        let b = 16.0;
        let boxr = Rect::new(rect.x + 6.0, rect.center_y() - b / 2.0, b, b).snap(self.scale);
        if on {
            let fill = if enabled {
                self.style.accent
            } else {
                self.style.text_disabled
            };
            self.draw.push(DrawCmd::Fill {
                rect: boxr,
                radius: 4.0,
                color: fill,
                owner: Some(id),
            });
            self.draw.push(DrawCmd::Text {
                rect: Rect::new(boxr.x + 2.0, boxr.y, b - 2.0, b),
                text: "\u{E73E}".into(),
                style: TextStyle {
                    font: FontRole::Icon,
                    size: 11.0,
                    weight: 400,
                },
                color: self.style.text_on_accent,
                trim: false,
                owner: Some(id),
            });
        } else {
            self.draw.push(DrawCmd::Stroke {
                rect: boxr.inflate(-0.5),
                radius: 4.0,
                width: 1.0,
                color: if enabled {
                    self.style.border
                } else {
                    self.style.text_disabled
                },
                owner: Some(id),
            });
        }
        let color = if enabled {
            self.style.text_primary
        } else {
            self.style.text_disabled
        };
        let tr = Rect::new(
            boxr.right() + 8.0,
            rect.y,
            (rect.right() - boxr.right() - 12.0).max(1.0),
            rect.h,
        );
        self.text(tr, label, self.style.weight_regular, color, id);
        self.actions.push((id, action));
    }

    fn radio(
        &mut self,
        parent: ElementId,
        key: &str,
        rect: Rect,
        name: &str,
        selected: bool,
        action: PopupAction,
    ) {
        let rect = rect.snap(self.scale);
        let id = self.add(
            parent,
            Element::new(
                key_id(key),
                crate::tree::intern(key),
                Role::RadioButton,
                name,
                rect,
            )
            .focusable(true)
            .selected(selected),
        );
        self.backplate(id, rect);
        self.interactive_bg(id, rect);
        let d = 16.0;
        let c = Rect::new(rect.x + 6.0, rect.center_y() - d / 2.0, d, d).snap(self.scale);
        self.draw.push(DrawCmd::Stroke {
            rect: c.inflate(-0.5),
            radius: d / 2.0,
            width: 1.0,
            color: if selected {
                self.style.accent
            } else {
                self.style.border
            },
            owner: Some(id),
        });
        if selected {
            self.draw.push(DrawCmd::Fill {
                rect: c.inflate(-4.0),
                radius: (d - 8.0) / 2.0,
                color: self.style.accent,
                owner: Some(id),
            });
        }
        let tr = Rect::new(
            c.right() + 8.0,
            rect.y,
            rect.right() - c.right() - 12.0,
            rect.h,
        );
        let weight = if selected {
            self.style.weight_emphasis
        } else {
            self.style.weight_regular
        };
        self.text(tr, name, weight, self.style.text_primary, id);
        self.actions.push((id, action));
    }

    fn header(&mut self, group: ElementId, rect: Rect, label: &str) {
        let color = self.style.text_secondary;
        self.text(rect, label, self.style.weight_emphasis, color, group);
    }
}

fn s<'a>(lang: Lang, en: &'a str, ko: &'a str) -> &'a str {
    match lang {
        Lang::En => en,
        Lang::Ko => ko,
    }
}

pub fn build_popup(
    model: &PopupModel,
    style: &PanelStyle,
    plan: &SurfacePlan,
    measure: &dyn TextMeasure,
    scale: f32,
    strings: &Strings,
) -> PopupScene {
    let lang = strings.lang;
    let right_col = POPUP_WIDTH - 2.0 * PAD - LEFT_COL - COL_GAP;
    let lx = PAD;
    let rxc = PAD + LEFT_COL + COL_GAP;
    let top = PAD + HEADER + 6.0;
    // Many themes are laid out in two columns so the window fits small
    // screens (1366×768 at 100%).
    let theme_cols = if model.themes.len() > 8 { 2 } else { 1 };
    let theme_lines = model.themes.len().div_ceil(theme_cols) as f32;
    let left_rows = 1.0 + theme_lines + 1.0 + 4.0;
    let left_h = left_rows * ROW + 8.0;
    let right_h = (1.0 + model.modules.len() as f32) * ROW;
    let body = left_h.max(right_h);
    let h = (top + body + 8.0 + ROW + PAD).ceil();
    let size = Size { w: POPUP_WIDTH, h };
    let full = Rect::new(0.0, 0.0, POPUP_WIDTH, h);

    let tree = UiTree::new(Element::new(
        ids::ROOT,
        "settings",
        Role::Window,
        s(lang, "windowsRICE settings", "windowsRICE 설정"),
        full,
    ));
    let mut b = Builder {
        tree,
        draw: DrawList::default(),
        actions: Vec::new(),
        style,
        model,
        measure,
        scale,
        plan_backplates: plan.backplates,
    };
    b.draw.push(DrawCmd::Fill {
        rect: full,
        radius: 0.0,
        color: Color {
            a: (plan.surface_opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            ..style.overlay
        },
        owner: Some(ids::ROOT),
    });

    // ---- title and close
    let title_r = Rect::new(PAD, PAD, POPUP_WIDTH - 2.0 * PAD - BTN - 8.0, HEADER).snap(scale);
    b.add(
        ids::ROOT,
        Element::new(
            ids::TITLE,
            "title",
            Role::Text,
            s(lang, "windowsRICE settings", "windowsRICE 설정"),
            title_r,
        ),
    );
    b.text(
        title_r,
        s(lang, "windowsRICE settings", "windowsRICE 설정"),
        style.weight_emphasis,
        style.text_primary,
        ids::TITLE,
    );
    let close_r = Rect::new(
        POPUP_WIDTH - PAD - BTN,
        PAD + (HEADER - BTN) / 2.0,
        BTN,
        BTN,
    );
    {
        let rect = close_r.snap(scale);
        b.add(
            ids::ROOT,
            Element::new(
                ids::CLOSE,
                "close",
                Role::Button,
                s(lang, "Close settings", "설정 닫기"),
                rect,
            )
            .focusable(true)
            .invokable(),
        );
        b.interactive_bg(ids::CLOSE, rect);
        b.draw.push(DrawCmd::Text {
            rect: Rect::new(rect.x + 7.0, rect.y, 14.0, rect.h),
            text: "\u{E8BB}".into(),
            style: b.icon(),
            color: style.text_primary,
            trim: false,
            owner: Some(ids::CLOSE),
        });
        b.actions.push((ids::CLOSE, PopupAction::Close));
    }

    // ---- left column: theme
    let mut y = top;
    let theme_rows = 1.0 + theme_lines;
    b.add(
        ids::ROOT,
        Element::new(
            ids::GROUP_THEME,
            "group.theme",
            Role::Group,
            s(lang, "Theme", "테마"),
            Rect::new(lx, y, LEFT_COL, theme_rows * ROW).snap(scale),
        ),
    );
    b.header(
        ids::GROUP_THEME,
        Rect::new(lx, y, LEFT_COL, ROW),
        s(lang, "Theme", "테마"),
    );
    y += ROW;
    let col_w = (LEFT_COL - 6.0 * (theme_cols as f32 - 1.0)) / theme_cols as f32;
    for (i, (id, name)) in model.themes.iter().enumerate() {
        let selected = *id == model.active_theme;
        // The group is named "Theme"; the shared product prefix is left out
        // of the visible label when space is split into columns.
        let label = if theme_cols > 1 {
            name.strip_prefix("windowsRICE ").unwrap_or(name)
        } else {
            name
        };
        b.radio(
            ids::GROUP_THEME,
            &format!("theme:{id}"),
            Rect::new(
                lx + (i % theme_cols) as f32 * (col_w + 6.0),
                y + (i / theme_cols) as f32 * ROW,
                col_w,
                ROW - 2.0,
            ),
            label,
            selected,
            PopupAction::SetTheme(id.clone()),
        );
    }
    y += theme_lines * ROW;

    // ---- left column: bar
    y += 8.0;
    b.add(
        ids::ROOT,
        Element::new(
            ids::GROUP_BAR,
            "group.bar",
            Role::Group,
            s(lang, "Bar", "바"),
            Rect::new(lx, y, LEFT_COL, 5.0 * ROW).snap(scale),
        ),
    );
    b.header(
        ids::GROUP_BAR,
        Rect::new(lx, y, LEFT_COL, ROW),
        s(lang, "Bar", "바"),
    );
    y += ROW;
    let height_label = match lang {
        Lang::En => format!("Height {}", model.height),
        Lang::Ko => format!("높이 {}", model.height),
    };
    let hl = Rect::new(lx, y, LEFT_COL - 2.0 * BTN - 8.0, ROW - 2.0).snap(scale);
    let hid = b.add(
        ids::GROUP_BAR,
        Element::new(
            key_id("bar:height"),
            "bar.height",
            Role::Text,
            height_label.clone(),
            hl,
        ),
    );
    b.text(
        Rect::new(hl.x + 6.0, hl.y, hl.w - 6.0, hl.h),
        &height_label,
        style.weight_regular,
        style.text_primary,
        hid,
    );
    let (hmin, hmax) = windowsrice_core::bar::BAR_HEIGHT_RANGE;
    b.button(
        ids::GROUP_BAR,
        "bar:height-",
        Rect::new(lx + LEFT_COL - 2.0 * BTN - 4.0, y + 1.0, BTN, BTN),
        "\u{E738}",
        match lang {
            Lang::En => format!("Decrease bar height (now {})", model.height),
            Lang::Ko => format!("바 높이 줄이기 (현재 {})", model.height),
        },
        model.height > hmin,
        PopupAction::HeightDown,
    );
    b.button(
        ids::GROUP_BAR,
        "bar:height+",
        Rect::new(lx + LEFT_COL - BTN, y + 1.0, BTN, BTN),
        "\u{E710}",
        match lang {
            Lang::En => format!("Increase bar height (now {})", model.height),
            Lang::Ko => format!("바 높이 늘리기 (현재 {})", model.height),
        },
        model.height < hmax,
        PopupAction::HeightUp,
    );
    y += ROW;
    for (key, en, ko, on, action) in [
        (
            "bar:autohide",
            "Hide automatically",
            "자동으로 숨기기",
            model.auto_hide,
            PopupAction::ToggleAutoHide,
        ),
        (
            "bar:reserve",
            "Reserve screen space",
            "화면 공간 확보",
            model.reserve,
            PopupAction::ToggleReserve,
        ),
        (
            "bar:monitors",
            "Show on all monitors",
            "모든 모니터에 표시",
            model.all_monitors,
            PopupAction::ToggleAllMonitors,
        ),
    ] {
        let label = s(lang, en, ko);
        b.check(
            ids::GROUP_BAR,
            key,
            Rect::new(lx, y, LEFT_COL, ROW - 2.0),
            label,
            label.to_owned(),
            on,
            true,
            None,
            action,
        );
        y += ROW;
    }

    // ---- right column: modules
    let mut y = top;
    b.add(
        ids::ROOT,
        Element::new(
            ids::GROUP_MODULES,
            "group.modules",
            Role::Group,
            s(lang, "Modules", "모듈"),
            Rect::new(rxc, y, right_col, (1.0 + model.modules.len() as f32) * ROW).snap(scale),
        ),
    );
    b.header(
        ids::GROUP_MODULES,
        Rect::new(rxc, y, right_col, ROW),
        s(lang, "Modules", "모듈"),
    );
    y += ROW;
    let n = model.modules.len();
    // interval label, gap, −/+ (each followed by a gap), ↑, gap, ↓
    let controls_w = INTERVAL_W + 2.0 + 2.0 * (BTN + 2.0) + BTN + 2.0 + BTN;
    for (i, m) in model.modules.iter().enumerate() {
        let name = module_name(m, lang);
        let status = model.status.get(&m.id).cloned().unwrap_or(ModuleStatus::Ok);
        let (suffix, help) = match (&status, lang) {
            (ModuleStatus::Ok, _) => (String::new(), None),
            (ModuleStatus::Unavailable(why), Lang::En) => {
                (" — unavailable".to_owned(), Some(why.clone()))
            }
            (ModuleStatus::Unavailable(why), Lang::Ko) => {
                (" — 사용할 수 없음".to_owned(), Some(why.clone()))
            }
            (ModuleStatus::Absent, Lang::En) => (" — not on this computer".to_owned(), None),
            (ModuleStatus::Absent, Lang::Ko) => (" — 이 컴퓨터에 없음".to_owned(), None),
        };
        let is_menu = m.kind == ModuleKind::Menu;
        let help = if is_menu {
            Some(
                s(
                    lang,
                    "The menu cannot be hidden; it opens these settings.",
                    "메뉴는 숨길 수 없습니다. 이 설정을 엽니다.",
                )
                .to_owned(),
            )
        } else {
            help
        };
        let label = format!("{name}{suffix}");
        let show_name = match lang {
            Lang::En => format!("Show {label}"),
            Lang::Ko => format!("{label} 표시"),
        };
        b.check(
            ids::GROUP_MODULES,
            &format!("module:{}:show", m.id),
            Rect::new(rxc, y, right_col - controls_w - 4.0, ROW - 2.0),
            &label,
            show_name,
            m.enabled,
            !is_menu,
            help,
            PopupAction::ToggleModule(m.id.clone()),
        );
        let mut cx = rxc + right_col - controls_w;
        let interval = m.interval_ms();
        let il = interval_label(interval, lang);
        let ir = Rect::new(cx, y, INTERVAL_W, ROW - 2.0).snap(scale);
        let iname = match lang {
            Lang::En => format!("{name} refresh interval: {il}"),
            Lang::Ko => format!("{name} 새로 고침 간격: {il}"),
        };
        let ikey = format!("module:{}:interval", m.id);
        let iid = b.add(
            ids::GROUP_MODULES,
            Element::new(
                key_id(&ikey),
                crate::tree::intern(&ikey),
                Role::Text,
                iname,
                ir,
            ),
        );
        let iw = measure.measure(&il, &b.ui(style.weight_regular)).w;
        b.text(
            Rect::new(ir.right() - iw - 4.0, ir.y, iw.min(ir.w), ir.h),
            &il,
            style.weight_regular,
            style.text_secondary,
            iid,
        );
        cx += INTERVAL_W + 2.0;
        if let Some(ms) = interval {
            b.button(
                ids::GROUP_MODULES,
                &format!("module:{}:shorter", m.id),
                Rect::new(cx, y + 1.0, BTN, BTN),
                "\u{E738}",
                match lang {
                    Lang::En => format!("Refresh {name} more often (now {il})"),
                    Lang::Ko => format!("{name} 더 자주 새로 고침 (현재 {il})"),
                },
                step_interval(ms, false) != ms,
                PopupAction::IntervalShorter(m.id.clone()),
            );
            b.button(
                ids::GROUP_MODULES,
                &format!("module:{}:longer", m.id),
                Rect::new(cx + BTN + 2.0, y + 1.0, BTN, BTN),
                "\u{E710}",
                match lang {
                    Lang::En => format!("Refresh {name} less often (now {il})"),
                    Lang::Ko => format!("{name} 덜 자주 새로 고침 (현재 {il})"),
                },
                step_interval(ms, true) != ms,
                PopupAction::IntervalLonger(m.id.clone()),
            );
        }
        cx += 2.0 * (BTN + 2.0);
        b.button(
            ids::GROUP_MODULES,
            &format!("module:{}:up", m.id),
            Rect::new(cx, y + 1.0, BTN, BTN),
            "\u{E70E}",
            match lang {
                Lang::En => format!("Move {name} up"),
                Lang::Ko => format!("{name} 위로 이동"),
            },
            i > 0,
            PopupAction::MoveUp(m.id.clone()),
        );
        b.button(
            ids::GROUP_MODULES,
            &format!("module:{}:down", m.id),
            Rect::new(cx + BTN + 2.0, y + 1.0, BTN, BTN),
            "\u{E70D}",
            match lang {
                Lang::En => format!("Move {name} down"),
                Lang::Ko => format!("{name} 아래로 이동"),
            },
            i + 1 < n,
            PopupAction::MoveDown(m.id.clone()),
        );
        y += ROW;
    }

    // ---- footer
    let fy = h - PAD - ROW;
    let exit_w = 150.0;
    let msg = model.message.clone().unwrap_or_default();
    let studio_w = 200.0;
    let mr = Rect::new(
        PAD,
        fy,
        POPUP_WIDTH - 2.0 * PAD - exit_w - studio_w - 16.0,
        ROW,
    )
    .snap(scale);
    let sr = Rect::new(
        POPUP_WIDTH - PAD - exit_w - 8.0 - studio_w,
        fy,
        studio_w,
        ROW,
    )
    .snap(scale);
    let studio_label = s(lang, "Open Design Studio", "디자인 스튜디오 열기");
    b.add(
        ids::ROOT,
        Element::new(ids::STUDIO, "open-studio", Role::Button, studio_label, sr)
            .focusable(true)
            .invokable(),
    );
    b.backplate(ids::STUDIO, sr);
    b.draw.push(DrawCmd::Fill {
        rect: sr,
        radius: style.radius_control,
        color: style.accent,
        owner: Some(ids::STUDIO),
    });
    b.interactive_bg(ids::STUDIO, sr);
    let sw = measure
        .measure(studio_label, &b.ui(style.weight_emphasis))
        .w;
    b.text(
        Rect::new(sr.x + (sr.w - sw) / 2.0, sr.y, sw, sr.h),
        studio_label,
        style.weight_emphasis,
        style.text_on_accent,
        ids::STUDIO,
    );
    b.actions.push((ids::STUDIO, PopupAction::OpenStudio));
    if !msg.is_empty() {
        b.add(
            ids::ROOT,
            Element::new(ids::MESSAGE, "message", Role::Text, msg.clone(), mr).focusable(true),
        );
        b.text(
            mr,
            &msg,
            style.weight_regular,
            style.text_secondary,
            ids::MESSAGE,
        );
    }
    let er = Rect::new(POPUP_WIDTH - PAD - exit_w, fy, exit_w, ROW).snap(scale);
    let exit_label = s(lang, "Exit windowsRICE", "windowsRICE 종료");
    b.add(
        ids::ROOT,
        Element::new(ids::EXIT, "exit", Role::Button, exit_label, er)
            .focusable(true)
            .invokable(),
    );
    b.backplate(ids::EXIT, er);
    b.interactive_bg(ids::EXIT, er);
    b.draw.push(DrawCmd::Stroke {
        rect: er.inflate(-0.5),
        radius: style.radius_control,
        width: 1.0,
        color: style.border,
        owner: Some(ids::EXIT),
    });
    let ew = measure.measure(exit_label, &b.ui(style.weight_regular)).w;
    b.text(
        Rect::new(er.x + (er.w - ew) / 2.0, er.y, ew, er.h),
        exit_label,
        style.weight_regular,
        style.text_primary,
        ids::EXIT,
    );
    b.actions.push((ids::EXIT, PopupAction::Exit));

    // ---- focus ring
    if model.keyboard_active
        && let Some(f) = model.focus
        && let Some(e) = b.tree.get(f)
        && e.is_focus_target()
    {
        let ring = e.bounds.inflate(1.0);
        b.draw.push(DrawCmd::Stroke {
            rect: ring,
            radius: style.radius_control + 1.0,
            width: 2.0,
            color: style.focus_ring,
            owner: Some(f),
        });
    }

    PopupScene {
        scene: Scene {
            tree: b.tree,
            draw: b.draw,
            size,
            scale,
        },
        actions: b.actions,
    }
}

/// Applies a module action to a module list (pure; the caller saves).
pub fn apply_module_action(modules: &mut [ModuleConfig], action: &PopupAction) -> bool {
    let find = |id: &str, m: &[ModuleConfig]| m.iter().position(|x| x.id == id);
    match action {
        PopupAction::ToggleModule(id) => {
            if let Some(i) = find(id, modules)
                && modules[i].kind != ModuleKind::Menu
            {
                modules[i].enabled = !modules[i].enabled;
                return true;
            }
            false
        }
        PopupAction::MoveUp(id) => match find(id, modules) {
            Some(i) if i > 0 => {
                modules.swap(i, i - 1);
                true
            }
            _ => false,
        },
        PopupAction::MoveDown(id) => match find(id, modules) {
            Some(i) if i + 1 < modules.len() => {
                modules.swap(i, i + 1);
                true
            }
            _ => false,
        },
        PopupAction::IntervalShorter(id) | PopupAction::IntervalLonger(id) => {
            let longer = matches!(action, PopupAction::IntervalLonger(_));
            let Some(i) = find(id, modules) else {
                return false;
            };
            let Some(cur) = modules[i].interval_ms() else {
                return false;
            };
            let next = step_interval(cur, longer);
            if next == cur {
                return false;
            }
            modules[i].interval_ms = if Some(next) == modules[i].kind.default_interval_ms() {
                None
            } else {
                Some(next)
            };
            true
        }
        _ => false,
    }
}
