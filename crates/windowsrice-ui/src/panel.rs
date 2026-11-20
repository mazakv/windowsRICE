//! The 34 px top panel: data model, layout and scene construction.
//!
//! [`build`] lays the panel out once and emits, from the same rectangles,
//! the accessibility tree and the draw list. Left: windowsRICE menu button
//! and the active window. Centre: clock and date. Right: network, volume,
//! CPU, GPU, memory and windowsRICE status.

use windowsrice_theme::Color;

use crate::draw::{DrawCmd, DrawList, FontRole, TextStyle};
use crate::format::{self, LocalTime};
use crate::geom::{Rect, Size};
use crate::legibility::SurfacePlan;
use crate::strings::{NetworkPhrase, Strings};
use crate::style::PanelStyle;
use crate::text::TextMeasure;
use crate::tree::{Element, ElementId, Role, UiTree};

/// Stable element ids of the panel.
pub mod ids {
    use crate::tree::ElementId;
    pub const ROOT: ElementId = ElementId(1);
    pub const GROUP_APP: ElementId = ElementId(2);
    pub const MENU: ElementId = ElementId(3);
    pub const ACTIVE_WINDOW: ElementId = ElementId(4);
    pub const GROUP_CLOCK: ElementId = ElementId(5);
    pub const CLOCK: ElementId = ElementId(6);
    pub const GROUP_STATUS: ElementId = ElementId(7);
    pub const NETWORK: ElementId = ElementId(8);
    pub const VOLUME: ElementId = ElementId(9);
    pub const CPU: ElementId = ElementId(10);
    pub const GPU: ElementId = ElementId(11);
    pub const MEMORY: ElementId = ElementId(12);
    pub const STATUS: ElementId = ElementId(13);
}

/// Segoe Fluent Icons / Segoe MDL2 Assets code points.
pub mod glyphs {
    pub const WIFI: &str = "\u{E701}";
    pub const ETHERNET: &str = "\u{E839}";
    pub const NETWORK_OFFLINE: &str = "\u{F384}";
    pub const VOLUME: &str = "\u{E767}";
    /// Processor (Segoe Fluent Icons and Segoe MDL2 Assets).
    pub const CPU: &str = "\u{E950}";
    /// Graphics card.
    pub const GPU: &str = "\u{F211}";
    /// Memory module.
    pub const RAM: &str = "\u{E964}";
    /// Video memory (Fluent only; the classic set uses `RAM`).
    pub const VRAM: &str = "\u{EEA1}";
    pub const MUTE: &str = "\u{E74F}";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Wired,
    Wireless,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkState {
    Internet(LinkKind),
    LocalOnly,
    Disconnected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioState {
    pub volume_percent: u8,
    pub muted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryState {
    pub used_bytes: u64,
    pub total_bytes: u64,
}

/// Everything the panel displays. `None` means "unavailable"; the matching
/// element is then shown disabled and named accordingly.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PanelData {
    /// Title of the foreground window; `None` for the desktop.
    pub active_window: Option<String>,
    pub time: Option<LocalTime>,
    pub network: Option<NetworkState>,
    pub audio: Option<AudioState>,
    pub cpu_percent: Option<f32>,
    pub gpu_percent: Option<f32>,
    pub memory: Option<MemoryState>,
    /// windowsRICE warnings (theme fallback, reduced legibility, …).
    pub warnings: Vec<String>,
}

/// Data plus interaction state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PanelModel {
    pub data: PanelData,
    /// Logical focus (kept while the window is inactive).
    pub focus: Option<ElementId>,
    /// The panel window has keyboard focus; the focus ring is visible.
    pub keyboard_active: bool,
    pub hover: Option<ElementId>,
    pub menu_open: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub tree: UiTree,
    pub draw: DrawList,
    /// Surface size in DIPs.
    pub size: Size,
    pub scale: f32,
}

struct Piece {
    text: String,
    style: TextStyle,
    color: Color,
    width: f32,
}

struct Chip {
    id: ElementId,
    automation_id: &'static str,
    name: String,
    enabled: bool,
    pieces: Vec<Piece>,
    rect: Rect,
}

impl Chip {
    fn content_width(&self, inner_gap: f32) -> f32 {
        let n = self.pieces.len() as f32;
        self.pieces.iter().map(|p| p.width).sum::<f32>() + inner_gap * (n - 1.0).max(0.0)
    }
}

pub fn build(
    model: &PanelModel,
    style: &PanelStyle,
    plan: &SurfacePlan,
    measure: &dyn TextMeasure,
    size: Size,
    scale: f32,
    strings: &Strings,
) -> Scene {
    let d = &model.data;
    let (w, h) = (size.w, size.h);
    let chip_h = (h - 8.0).max(1.0);
    let chip_y = (h - chip_h) / 2.0;
    let pad = style.gap_small * 2.0;
    let inner_gap = style.gap_small;

    let ui = |weight: u16| TextStyle {
        font: FontRole::Ui,
        size: style.size_body,
        weight,
    };
    let icon = TextStyle {
        font: FontRole::Icon,
        size: style.size_icon,
        weight: 400,
    };
    let piece = |text: String, st: TextStyle, color: Color| Piece {
        width: measure.measure(&text, &st).w,
        text,
        style: st,
        color,
    };
    let dim = |enabled: bool, c: Color| if enabled { c } else { style.text_disabled };

    // ---- right: status indicators, laid out from the right edge
    let mut right: Vec<Chip> = Vec::new();
    {
        let (name, glyph, enabled) = match d.network {
            Some(NetworkState::Internet(k)) => (
                strings.network(match k {
                    LinkKind::Wired => NetworkPhrase::InternetWired,
                    LinkKind::Wireless => NetworkPhrase::InternetWireless,
                    LinkKind::Other => NetworkPhrase::Internet,
                }),
                if k == LinkKind::Wireless {
                    glyphs::WIFI
                } else {
                    glyphs::ETHERNET
                },
                true,
            ),
            Some(NetworkState::LocalOnly) => (
                strings.network(NetworkPhrase::LocalOnly),
                glyphs::NETWORK_OFFLINE,
                true,
            ),
            Some(NetworkState::Disconnected) => (
                strings.network(NetworkPhrase::Disconnected),
                glyphs::NETWORK_OFFLINE,
                true,
            ),
            None => (
                strings.network(NetworkPhrase::Unavailable),
                glyphs::NETWORK_OFFLINE,
                false,
            ),
        };
        right.push(Chip {
            id: ids::NETWORK,
            automation_id: "status.network",
            name: name.to_owned(),
            enabled,
            pieces: vec![piece(
                glyph.to_owned(),
                icon,
                dim(enabled, style.text_primary),
            )],
            rect: Rect::default(),
        });
    }
    {
        let (name, pieces, enabled) = match d.audio {
            Some(a) => (
                strings.volume(a.volume_percent, a.muted),
                vec![
                    piece(
                        if a.muted {
                            glyphs::MUTE
                        } else {
                            glyphs::VOLUME
                        }
                        .to_owned(),
                        icon,
                        style.text_primary,
                    ),
                    piece(
                        format!("{}%", a.volume_percent),
                        ui(style.weight_regular),
                        style.text_primary,
                    ),
                ],
                true,
            ),
            None => (
                strings.volume_unavailable().to_owned(),
                vec![piece(glyphs::VOLUME.to_owned(), icon, style.text_disabled)],
                false,
            ),
        };
        right.push(Chip {
            id: ids::VOLUME,
            automation_id: "status.volume",
            name,
            enabled,
            pieces,
            rect: Rect::default(),
        });
    }
    for (id, automation_id, label, value, name_ok, name_na) in [
        (
            ids::CPU,
            "status.cpu",
            "CPU",
            d.cpu_percent.map(format::percent),
            d.cpu_percent.map(|v| strings.cpu(format::percent(v))),
            strings.cpu_unavailable(),
        ),
        (
            ids::GPU,
            "status.gpu",
            "GPU",
            d.gpu_percent.map(format::percent),
            d.gpu_percent.map(|v| strings.gpu(format::percent(v))),
            strings.gpu_unavailable(),
        ),
        (
            ids::MEMORY,
            "status.memory",
            "RAM",
            d.memory.map(|m| memory_percent(&m)),
            d.memory.map(|m| {
                strings.memory(
                    &format::gigabytes(m.used_bytes),
                    &format::gigabytes(m.total_bytes),
                    memory_percent(&m),
                )
            }),
            strings.memory_unavailable(),
        ),
    ] {
        let enabled = value.is_some();
        let text = match value {
            Some(v) => format!("{label} {v}%"),
            None => format!("{label} —"),
        };
        right.push(Chip {
            id,
            automation_id,
            name: name_ok.unwrap_or_else(|| name_na.to_owned()),
            enabled,
            pieces: vec![piece(
                text,
                ui(style.weight_regular),
                dim(enabled, style.text_primary),
            )],
            rect: Rect::default(),
        });
    }
    let status_name = if d.warnings.is_empty() {
        strings.status_ok().to_owned()
    } else {
        strings.status_warnings(&d.warnings)
    };
    right.push(Chip {
        id: ids::STATUS,
        automation_id: "status.windowsrice",
        name: status_name,
        enabled: true,
        // The status mark is drawn as a dot; the piece reserves its width.
        pieces: vec![Piece {
            text: String::new(),
            style: icon,
            color: if d.warnings.is_empty() {
                style.success
            } else {
                style.warning
            },
            width: 8.0,
        }],
        rect: Rect::default(),
    });

    let mut rx = w - style.padding_x;
    for chip in right.iter_mut().rev() {
        let cw = chip.content_width(inner_gap) + 2.0 * pad;
        chip.rect = Rect::new(rx - cw, chip_y, cw, chip_h).snap(scale);
        rx = chip.rect.x - style.gap_small;
    }
    let right_start = right.first().map_or(w - style.padding_x, |c| c.rect.x);

    // ---- centre: clock
    let (time_s, date_s, clock_name, clock_enabled) = match &d.time {
        Some(t) => (
            format::time_text(t),
            format::date_text(t, strings.lang),
            format::clock_accessible(t, strings.lang),
            true,
        ),
        None => (
            "--:--".to_owned(),
            String::new(),
            strings.group_clock().to_owned(),
            false,
        ),
    };
    let mut clock = Chip {
        id: ids::CLOCK,
        automation_id: "clock",
        name: clock_name,
        enabled: clock_enabled,
        pieces: vec![piece(
            time_s,
            ui(style.weight_emphasis),
            dim(clock_enabled, style.text_primary),
        )],
        rect: Rect::default(),
    };
    if !date_s.is_empty() {
        clock.pieces.push(piece(
            date_s,
            ui(style.weight_regular),
            style.text_secondary,
        ));
    }
    let clock_w = clock.content_width(inner_gap * 1.5) + 2.0 * pad;
    let menu_rect = Rect::new(style.padding_x, chip_y, chip_h, chip_h).snap(scale);
    let left_min = menu_rect.right() + style.gap;
    let centred = (w - clock_w) / 2.0;
    let clock_x = centred.min(right_start - style.gap - clock_w).max(left_min);
    clock.rect = Rect::new(clock_x, chip_y, clock_w, chip_h).snap(scale);

    // ---- left: menu button and active window
    let title = d
        .active_window
        .clone()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| strings.desktop().to_owned());
    let app_x = menu_rect.right() + style.gap_small;
    let app_max = (clock.rect.x - style.gap - app_x).max(2.0 * pad + 1.0);
    let mut app = Chip {
        id: ids::ACTIVE_WINDOW,
        automation_id: "active-window",
        name: strings.active_window(&title),
        enabled: true,
        pieces: vec![piece(title, ui(style.weight_regular), style.text_primary)],
        rect: Rect::default(),
    };
    app.pieces[0].width = app.pieces[0].width.min(app_max - 2.0 * pad);
    app.rect = Rect::new(
        app_x,
        chip_y,
        app.content_width(inner_gap) + 2.0 * pad,
        chip_h,
    )
    .snap(scale);

    // ---- tree
    let full = Rect::new(0.0, 0.0, w, h);
    let mut tree = UiTree::new(Element::new(
        ids::ROOT,
        "panel",
        Role::ToolBar,
        strings.panel(),
        full,
    ));
    let add = |tree: &mut UiTree, parent: ElementId, e: Element| {
        tree.add(parent, e).expect("panel element ids are unique");
    };
    add(
        &mut tree,
        ids::ROOT,
        Element::new(
            ids::GROUP_APP,
            "group.app",
            Role::Group,
            strings.group_app(),
            menu_rect.union(&app.rect),
        ),
    );
    add(
        &mut tree,
        ids::GROUP_APP,
        Element::new(
            ids::MENU,
            "menu",
            Role::Button,
            strings.menu_button(),
            menu_rect,
        )
        .focusable(true)
        .invokable()
        .expanded(model.menu_open)
        .help(strings.menu_help()),
    );
    add(
        &mut tree,
        ids::GROUP_APP,
        Element::new(
            app.id,
            app.automation_id,
            Role::Text,
            app.name.clone(),
            app.rect,
        )
        .focusable(true),
    );
    add(
        &mut tree,
        ids::ROOT,
        Element::new(
            ids::GROUP_CLOCK,
            "group.clock",
            Role::Group,
            strings.group_clock(),
            clock.rect,
        ),
    );
    add(
        &mut tree,
        ids::GROUP_CLOCK,
        Element::new(
            clock.id,
            clock.automation_id,
            Role::Text,
            clock.name.clone(),
            clock.rect,
        )
        .focusable(true)
        .enabled(clock.enabled),
    );
    let status_bounds = right
        .iter()
        .map(|c| c.rect)
        .reduce(|a, b| a.union(&b))
        .unwrap_or_default();
    add(
        &mut tree,
        ids::ROOT,
        Element::new(
            ids::GROUP_STATUS,
            "group.status",
            Role::Group,
            strings.group_status(),
            status_bounds,
        ),
    );
    for c in &right {
        add(
            &mut tree,
            ids::GROUP_STATUS,
            Element::new(c.id, c.automation_id, Role::Text, c.name.clone(), c.rect)
                .focusable(true)
                .enabled(c.enabled),
        );
    }

    // ---- draw list
    let mut draw = DrawList::default();
    draw.push(DrawCmd::Fill {
        rect: full,
        radius: 0.0,
        color: Color {
            a: (plan.surface_opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            ..style.surface
        },
        owner: Some(ids::ROOT),
    });
    let radius = style.radius_control;
    let backplate = |draw: &mut DrawList, id: ElementId, rect: Rect| {
        if plan.backplates {
            draw.push(DrawCmd::Fill {
                rect,
                radius,
                color: style.surface,
                owner: Some(id),
            });
        }
    };

    // Menu button: backplate, hover/expanded state, 2×2 mark.
    backplate(&mut draw, ids::MENU, menu_rect);
    if model.menu_open {
        draw.push(DrawCmd::Fill {
            rect: menu_rect,
            radius,
            color: style.pressed,
            owner: Some(ids::MENU),
        });
    } else if model.hover == Some(ids::MENU) {
        draw.push(DrawCmd::Fill {
            rect: menu_rect,
            radius,
            color: style.hover,
            owner: Some(ids::MENU),
        });
    }
    let (cell, gap) = (5.0, 2.0);
    let mark = 2.0 * cell + gap;
    let mx = menu_rect.x + (menu_rect.w - mark) / 2.0;
    let my = menu_rect.y + (menu_rect.h - mark) / 2.0;
    for (i, j) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
        draw.push(DrawCmd::Fill {
            rect: Rect::new(mx + i * (cell + gap), my + j * (cell + gap), cell, cell).snap(scale),
            radius: 1.5,
            color: style.accent,
            owner: Some(ids::MENU),
        });
    }

    let emit_chip = |draw: &mut DrawList, chip: &Chip, gap_between: f32| {
        backplate(draw, chip.id, chip.rect);
        let mut x = chip.rect.x + pad;
        let n = chip.pieces.len();
        for (i, p) in chip.pieces.iter().enumerate() {
            if p.text.is_empty() {
                // Status dot.
                let dot = 8.0;
                draw.push(DrawCmd::Fill {
                    rect: Rect::new(x, chip.rect.center_y() - dot / 2.0, dot, dot).snap(scale),
                    radius: dot / 2.0,
                    color: p.color,
                    owner: Some(chip.id),
                });
            } else {
                draw.push(DrawCmd::Text {
                    rect: Rect::new(x, chip.rect.y, p.width, chip.rect.h),
                    text: p.text.clone(),
                    style: p.style,
                    color: p.color,
                    trim: chip.id == ids::ACTIVE_WINDOW,
                    owner: Some(chip.id),
                });
            }
            x += p.width;
            if i + 1 < n {
                x += gap_between;
            }
        }
    };
    emit_chip(&mut draw, &app, inner_gap);
    emit_chip(&mut draw, &clock, inner_gap * 1.5);
    for c in &right {
        emit_chip(&mut draw, c, inner_gap);
    }

    // Focus ring last, so nothing covers it. 2 DIPs wide, outside the chip.
    if model.keyboard_active
        && let Some(f) = model.focus
        && let Some(e) = tree.get(f)
        && e.is_focus_target()
    {
        draw.push(DrawCmd::Stroke {
            rect: e.bounds.inflate(1.0),
            radius: radius + 1.0,
            width: 2.0,
            color: style.focus_ring,
            owner: Some(f),
        });
    }

    Scene {
        tree,
        draw,
        size,
        scale,
    }
}

fn memory_percent(m: &MemoryState) -> u8 {
    if m.total_bytes == 0 {
        return 0;
    }
    format::percent((m.used_bytes as f64 / m.total_bytes as f64 * 100.0) as f32)
}
