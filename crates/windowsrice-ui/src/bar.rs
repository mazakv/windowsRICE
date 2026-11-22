//! The full-width windowsRICE Bar: module layout and scene construction.
//!
//! Modules sit in three regions. The left region starts at the theme's
//! horizontal padding, the right region ends at it, and the centre region is
//! centred on the bar unless that would collide with a side region. When the
//! modules do not fit, the active-window title is shortened first (down to
//! [`MIN_TITLE_WIDTH`]), then modules are hidden starting from the end of the
//! configuration (never the menu or the active window), and an overflow item
//! names what was hidden. The surface always spans the whole width, even
//! with no modules.

use windowsrice_core::bar::{
    AccentLine, BarStyle, Density, GradientDirection, IconSet, IconTint, ModuleContent,
    ModuleShape, VerticalAlign,
};
use windowsrice_theme::Color;

use crate::draw::{DrawCmd, DrawList, FontRole, TextStyle};
use crate::geom::{Rect, Size};
use crate::legibility::SurfacePlan;
use crate::module::{
    BarReadings, ModuleConfig, ModuleKind, ModuleView, Piece, Region, assign_ids, view,
};
use crate::panel::Scene;
use crate::strings::{Lang, Strings};
use crate::style::PanelStyle;
use crate::text::TextMeasure;
use crate::tree::{Element, ElementId, Role, UiTree};

/// Fixed element ids of the bar (module ids are hashed, ≥ 0x1000).
pub mod ids {
    use crate::tree::ElementId;
    pub const ROOT: ElementId = ElementId(1);
    pub const LEFT: ElementId = ElementId(2);
    pub const CENTER: ElementId = ElementId(3);
    pub const RIGHT: ElementId = ElementId(4);
    pub const OVERFLOW: ElementId = ElementId(5);
}

/// Narrowest the active-window title is shortened to before modules hide.
pub const MIN_TITLE_WIDTH: f32 = 64.0;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BarModel {
    pub modules: Vec<ModuleConfig>,
    pub readings: BarReadings,
    pub focus: Option<ElementId>,
    pub keyboard_active: bool,
    pub hover: Option<ElementId>,
    pub popup_open: bool,
    /// Bar decorations from `[bar.style]`.
    pub decor: Decor,
    /// Milliseconds on the animation clock (Phase 14); `None` draws the
    /// animations' first frame (still).
    pub anim_ms: Option<u64>,
}

/// Everything in `[bar.style]` that is not a theme palette colour: module
/// shape and content, icons, effects and extra colours. The running bar and
/// the Design Studio preview both build it with [`Decor::from_style`], so
/// they draw the same thing.
#[derive(Debug, Clone, PartialEq)]
pub struct Decor {
    pub border_bottom: bool,
    pub separators: bool,
    pub valign: VerticalAlign,
    pub border_width: f32,
    pub separator_width: f32,
    pub border_color: Option<Color>,
    pub separator_color: Option<Color>,
    pub gradient: Option<(GradientDirection, Vec<Color>)>,
    pub module_shape: ModuleShape,
    pub module_background: Option<Color>,
    /// 0.1–1.0.
    pub module_opacity: f32,
    pub module_radius: Option<f32>,
    pub content: ModuleContent,
    pub density: Density,
    pub icon_set: IconSet,
    pub icon_tint: IconTint,
    pub icon_color: Option<Color>,
    /// Added to the theme's icon size, DIPs.
    pub icon_size: f32,
    pub value_color: Option<Color>,
    pub clock_color: Option<Color>,
    pub clock_size: Option<f32>,
    pub hover_color: Option<Color>,
    pub hover_intensity: Option<f32>,
    pub selected_color: Option<Color>,
    pub accent_line: AccentLine,
    /// 0–1.
    pub shadow: f32,
    /// 0–1.
    pub highlight: f32,
    pub region_gap: Option<f32>,
    /// Enabled colour animations (Phase 14).
    pub animations: Vec<windowsrice_core::motion::ColorAnimation>,
}

impl Default for Decor {
    fn default() -> Self {
        Self::from_style(&BarStyle::default())
    }
}

/// Icon font of the classic set.
pub const CLASSIC_ICON_FAMILY: &str = "Segoe MDL2 Assets";

fn hex(h: &Option<String>) -> Option<Color> {
    h.as_deref()
        .and_then(windowsrice_core::bar::parse_hex_color)
        .map(|(r, g, b)| Color::rgb(r, g, b))
}

impl Decor {
    pub fn from_style(s: &BarStyle) -> Self {
        Self {
            border_bottom: s.border_bottom,
            separators: s.separators,
            valign: s.vertical_align,
            border_width: f32::from(s.border_width.unwrap_or(1)),
            separator_width: f32::from(s.separator_width.unwrap_or(1)),
            border_color: hex(&s.border_color),
            separator_color: hex(&s.separator_color),
            gradient: s.gradient.as_ref().map(|g| {
                (
                    g.direction,
                    g.colors
                        .iter()
                        .filter_map(|c| hex(&Some(c.clone())))
                        .collect(),
                )
            }),
            module_shape: s.module_shape,
            module_background: hex(&s.module_background),
            module_opacity: f32::from(s.module_opacity.unwrap_or(100)) / 100.0,
            module_radius: s.module_radius.map(f32::from),
            content: s.content,
            density: s.density,
            icon_set: s.icon_set,
            icon_tint: s.icon_tint,
            icon_color: hex(&s.icon_color),
            icon_size: f32::from(s.icon_size.unwrap_or(0)),
            value_color: hex(&s.value_color),
            clock_color: hex(&s.clock_color),
            clock_size: s.clock_size.map(f32::from),
            hover_color: hex(&s.hover_color),
            hover_intensity: s.hover_intensity.map(|v| f32::from(v) / 100.0),
            selected_color: hex(&s.selected_color),
            accent_line: s.accent_line,
            shadow: f32::from(s.shadow.unwrap_or(0)) / 100.0,
            highlight: f32::from(s.highlight.unwrap_or(0)) / 100.0,
            region_gap: s.region_gap.map(f32::from),
            animations: s.animations.iter().filter(|a| a.enabled).cloned().collect(),
        }
    }
}

/// `c` moved towards `toward` until it reaches `min` contrast on `bg`.
pub fn ensure_contrast(c: Color, toward: Color, bg: Color, min: f64) -> Color {
    use windowsrice_theme::color::contrast_ratio;
    let mut out = c;
    for i in 1..=10 {
        if contrast_ratio(out, bg) + 1e-9 >= min {
            break;
        }
        let k = i as f32 / 10.0;
        let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * k).round() as u8;
        out = Color::rgb(mix(c.r, toward.r), mix(c.g, toward.g), mix(c.b, toward.b));
    }
    out
}

/// Per-module icon colour for the colourful tint, readable (3:1, the
/// minimum for graphics) on `bg`.
fn colorful(kind: ModuleKind, style: &PanelStyle, bg: Color) -> Color {
    let c = match kind {
        ModuleKind::Cpu | ModuleKind::Download => Color::rgb(0x3D, 0xA5, 0xF4),
        ModuleKind::Gpu | ModuleKind::Network => Color::rgb(0x3F, 0xC1, 0x7A),
        ModuleKind::Ram | ModuleKind::Upload => Color::rgb(0xF0, 0xA0, 0x30),
        ModuleKind::Vram => Color::rgb(0xE8, 0x5D, 0x75),
        ModuleKind::Volume => Color::rgb(0xA3, 0x7B, 0xF0),
        ModuleKind::Battery => Color::rgb(0x8C, 0xC8, 0x3C),
        _ => style.accent,
    };
    ensure_contrast(c, style.text_primary, bg, 3.0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct BarScene {
    pub scene: Scene,
    /// Ids of enabled modules hidden for lack of space.
    pub hidden: Vec<String>,
    /// Element id of each shown module.
    pub elements: Vec<(String, ElementId)>,
}

/// Element id of the menu module, if configured.
pub fn menu_element(modules: &[ModuleConfig]) -> Option<ElementId> {
    let ids = assign_ids(modules);
    modules
        .iter()
        .zip(ids)
        .find(|(m, _)| m.kind == ModuleKind::Menu)
        .map(|(_, id)| id)
}

/// Per-module appearance after combining `[[bar.modules]]` overrides with
/// the bar style.
#[derive(Clone, Copy)]
struct Look {
    content: ModuleContent,
    /// Text size override, DIPs.
    font_size: Option<f32>,
    pad: f32,
    color: Option<Color>,
    background: Option<Color>,
}

struct Laid {
    look: Look,
    view: ModuleView,
    widths: Vec<f32>,
    width: f32,
    rect: Rect,
}

fn region_name(r: Region, lang: Lang) -> &'static str {
    match (r, lang) {
        (Region::Left, Lang::En) => "Left section",
        (Region::Center, Lang::En) => "Center section",
        (Region::Right, Lang::En) => "Right section",
        (Region::Left, Lang::Ko) => "왼쪽 영역",
        (Region::Center, Lang::Ko) => "가운데 영역",
        (Region::Right, Lang::Ko) => "오른쪽 영역",
    }
}

pub fn bar_name(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "windowsRICE bar",
        Lang::Ko => "windowsRICE 바",
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_bar(
    model: &BarModel,
    style: &PanelStyle,
    plan: &SurfacePlan,
    measure: &dyn TextMeasure,
    size: Size,
    scale: f32,
    strings: &Strings,
    spacing: f32,
) -> BarScene {
    let (w, h) = (size.w, size.h);
    let chip_h = (h - 8.0).max(16.0).min(h);
    let chip_y = match model.decor.valign {
        VerticalAlign::Center => (h - chip_h) / 2.0,
        VerticalAlign::Top => 2.0_f32.min((h - chip_h) / 2.0),
        VerticalAlign::Bottom => (h - chip_h - 2.0).max((h - chip_h) / 2.0),
    };
    // Colour animations (Phase 14): the animated colours replace their
    // targets for this frame; readability is enforced below as for any
    // colour.
    use windowsrice_core::motion::AnimTarget;
    let anim_t = model.anim_ms.unwrap_or(0);
    let anim_color = |target: AnimTarget| {
        model
            .decor
            .animations
            .iter()
            .find(|a| a.target == target)
            .and_then(|a| a.color_at(anim_t))
            .map(|c| {
                let (r, g, b) = c.to_u8();
                Color::rgb(r, g, b)
            })
    };
    let text_gradient = model
        .decor
        .animations
        .iter()
        .find(|a| a.target == AnimTarget::TextGradient)
        .cloned();
    let style_anim;
    let style = match (anim_color(AnimTarget::Text), anim_color(AnimTarget::Accent)) {
        (None, None) => style,
        (tx, ac) => {
            let mut s = style.clone();
            if let Some(c) = tx {
                s.text_primary = c;
            }
            if let Some(c) = ac {
                s.accent = c;
            }
            style_anim = s;
            &style_anim
        }
    };
    let decor_anim;
    let d = if model.decor.animations.is_empty() {
        &model.decor
    } else {
        let mut x = model.decor.clone();
        if let Some(c) = anim_color(AnimTarget::Icons) {
            x.icon_tint = IconTint::Custom;
            x.icon_color = Some(c);
        }
        if let Some(c) = anim_color(AnimTarget::Clock) {
            x.clock_color = Some(c);
        }
        if let Some(a) = x
            .animations
            .iter()
            .find(|a| a.target == AnimTarget::Background)
        {
            // The keyframes become a gradient that scrolls once per cycle.
            let p = a.phase(anim_t);
            let stops: Vec<Color> = (0..4)
                .filter_map(|i| a.color_at_phase((p + i as f32 / 3.0).fract()))
                .map(|c| {
                    let (r, g, b) = c.to_u8();
                    Color::rgb(r, g, b)
                })
                .collect();
            if stops.len() >= 2 {
                x.gradient = Some((GradientDirection::Horizontal, stops));
            }
        }
        decor_anim = x;
        &decor_anim
    };
    let density_factor = |x: Density| match x {
        Density::Compact => 0.5,
        Density::Normal => 1.0,
        Density::Expanded => 1.6,
    };
    let pad = style.gap_small * 2.0 * density_factor(d.density);
    let inner = style.gap_small;
    let ui = |weight: u16| TextStyle {
        font: FontRole::Ui,
        size: style.size_body,
        weight,
    };
    let icon_style = TextStyle {
        font: match d.icon_set {
            IconSet::Fluent => FontRole::Icon,
            IconSet::Classic => FontRole::Family(CLASSIC_ICON_FAMILY),
        },
        size: (style.size_icon + d.icon_size).max(8.0),
        weight: 400,
    };
    let glyph = |g: &str| -> String {
        if d.icon_set == IconSet::Classic && g == crate::panel::glyphs::VRAM {
            crate::panel::glyphs::RAM.to_owned()
        } else {
            g.to_owned()
        }
    };
    let clock_size = d.clock_size.unwrap_or(style.size_body);
    // A module's own text size scales its icons with it.
    let icon_for = |fs: Option<f32>| TextStyle {
        size: fs.map_or(icon_style.size, |f| {
            (icon_style.size * f / style.size_body).max(8.0)
        }),
        ..icon_style
    };
    let piece_style = |p: &Piece, kind: ModuleKind, fs: Option<f32>| {
        let sized = |t: TextStyle| TextStyle {
            size: fs.unwrap_or(t.size),
            ..t
        };
        match p {
            Piece::Text { emphasis: true, .. } if kind == ModuleKind::Clock => TextStyle {
                size: fs.unwrap_or(clock_size),
                ..ui(style.weight_emphasis)
            },
            Piece::Text { emphasis: true, .. } => sized(ui(style.weight_emphasis)),
            Piece::Icon(_) | Piece::ExtraIcon(_) => icon_for(fs),
            _ => sized(ui(style.weight_regular)),
        }
    };
    let piece_width = |p: &Piece, kind: ModuleKind, fs: Option<f32>| match p {
        Piece::Text { text, .. } => measure.measure(text, &piece_style(p, kind, fs)).w,
        Piece::Icon(g) | Piece::ExtraIcon(g) => measure.measure(&glyph(g), &icon_for(fs)).w,
        Piece::Dot { .. } => 8.0,
        Piece::Mark => 12.0,
    };
    let look_of = |m: &ModuleConfig| Look {
        content: m.content.unwrap_or(d.content),
        font_size: m.font_size.map(f32::from),
        pad: m
            .density
            .map_or(pad, |x| style.gap_small * 2.0 * density_factor(x)),
        color: hex(&m.color),
        background: hex(&m.background),
    };

    // ---- module content: labels on or off for every module, icons kept
    // or dropped. Modules that have only an icon, or only text, keep it.
    let configs: Vec<ModuleConfig> = model
        .modules
        .iter()
        .map(|m| {
            let mut m = m.clone();
            if ModuleConfig::has_label(m.kind) {
                match m.content.unwrap_or(d.content) {
                    ModuleContent::Full => m.show_label = Some(true),
                    ModuleContent::ValueOnly | ModuleContent::IconOnly => {
                        m.show_label = Some(false);
                    }
                    ModuleContent::Default => {}
                }
            }
            m
        })
        .collect();
    let keep = |pieces: Vec<Piece>, content: ModuleContent| -> Vec<Piece> {
        let is_icon = |p: &Piece| matches!(p, Piece::Icon(_) | Piece::ExtraIcon(_));
        let has_icon = pieces.iter().any(is_icon);
        let has_text = pieces.iter().any(|p| matches!(p, Piece::Text { .. }));
        pieces
            .into_iter()
            .filter(|p| match content {
                ModuleContent::Default => !matches!(p, Piece::ExtraIcon(_)),
                ModuleContent::Full => true,
                ModuleContent::IconOnly => !has_icon || !matches!(p, Piece::Text { .. }),
                ModuleContent::ValueOnly => {
                    !(is_icon(p) && has_text) && !matches!(p, Piece::Text { label: true, .. })
                }
            })
            .collect()
    };

    // ---- views of enabled modules, in configuration order
    let ids = assign_ids(&model.modules);
    let mut laid: Vec<Laid> = configs
        .iter()
        .zip(ids)
        .filter(|(m, _)| m.enabled)
        .filter_map(|(m, id)| view(m, id, &model.readings, strings).map(|v| (look_of(m), v)))
        .map(|(look, mut v)| {
            v.pieces = keep(std::mem::take(&mut v.pieces), look.content);
            (look, v)
        })
        .map(|(look, v)| {
            let pad = look.pad;
            let widths: Vec<f32> = v
                .pieces
                .iter()
                .map(|p| piece_width(p, v.kind, look.font_size))
                .collect();
            let gaps = inner
                * (widths.len().saturating_sub(1)) as f32
                * if v.kind == ModuleKind::Clock {
                    1.5
                } else {
                    1.0
                };
            let content: f32 = widths.iter().sum::<f32>() + gaps;
            let width = if v.kind == ModuleKind::Menu {
                (content + 2.0 * pad).max(chip_h)
            } else {
                content + 2.0 * pad
            };
            Laid {
                look,
                view: v,
                widths,
                width,
                rect: Rect::default(),
            }
        })
        .collect();

    // ---- per-module width limits: shorten the widest text piece
    for l in &mut laid {
        let Some(mw) = model
            .modules
            .iter()
            .find(|m| m.id == l.view.id)
            .and_then(|m| m.max_width)
        else {
            continue;
        };
        let excess = l.width - f32::from(mw);
        if excess <= 0.0 {
            continue;
        }
        let widest = l
            .view
            .pieces
            .iter()
            .enumerate()
            .filter(|(_, p)| matches!(p, Piece::Text { .. }))
            .max_by(|a, b| l.widths[a.0].total_cmp(&l.widths[b.0]))
            .map(|(i, _)| i);
        if let Some(i) = widest {
            let cut = excess.min((l.widths[i] - 16.0).max(0.0));
            l.widths[i] -= cut;
            l.width -= cut;
        }
    }

    // ---- overflow: shorten the title, then hide modules from the end
    let avail = (w - 2.0 * style.padding_x).max(0.0);
    let region_gap = d.region_gap.unwrap_or(style.gap);
    let total = |laid: &[Laid], overflow: f32| -> f32 {
        let mut sum = overflow;
        let mut regions = 0;
        for r in [Region::Left, Region::Center, Region::Right] {
            let ws: Vec<f32> = laid
                .iter()
                .filter(|l| l.view.region == r)
                .map(|l| l.width)
                .collect();
            if !ws.is_empty() {
                regions += 1;
                sum += ws.iter().sum::<f32>() + spacing * (ws.len() - 1) as f32;
            }
        }
        sum + region_gap * (regions.max(1) - 1) as f32
    };
    let overflow_style = ui(style.weight_regular);
    let mut hidden: Vec<String> = Vec::new();
    let mut overflow_w = 0.0;
    // Shorten the title as far as needed.
    let over = total(&laid, 0.0) - avail;
    if over > 0.0
        && let Some(t) = laid.iter_mut().find(|l| l.view.trimmable)
        && let Some(first) = t.widths.first().copied()
    {
        let new_w = (first - over).max(MIN_TITLE_WIDTH.min(first));
        t.width -= first - new_w;
        t.widths[0] = new_w;
    }
    while total(&laid, overflow_w) > avail {
        let victim = laid
            .iter()
            .rposition(|l| !matches!(l.view.kind, ModuleKind::Menu | ModuleKind::ActiveWindow));
        let Some(i) = victim else { break };
        hidden.push(laid.remove(i).view.id);
        let label = format!("+{}", hidden.len());
        overflow_w = measure.measure(&label, &overflow_style).w + 2.0 * pad + spacing;
    }

    // ---- positions
    let mut x = style.padding_x;
    for l in laid.iter_mut().filter(|l| l.view.region == Region::Left) {
        l.rect = Rect::new(x, chip_y, l.width, chip_h).snap(scale);
        x = l.rect.right() + spacing;
    }
    let left_end = if x > style.padding_x {
        x - spacing
    } else {
        style.padding_x
    };
    let mut rx = w - style.padding_x;
    let overflow_rect = if hidden.is_empty() {
        None
    } else {
        let ow = overflow_w - spacing;
        let r = Rect::new(rx - ow, chip_y, ow, chip_h).snap(scale);
        rx = r.x - spacing;
        Some(r)
    };
    for l in laid
        .iter_mut()
        .filter(|l| l.view.region == Region::Right)
        .rev()
    {
        l.rect = Rect::new(rx - l.width, chip_y, l.width, chip_h).snap(scale);
        rx = l.rect.x - spacing;
    }
    let right_start = rx + spacing;
    let center_w: f32 = {
        let ws: Vec<f32> = laid
            .iter()
            .filter(|l| l.view.region == Region::Center)
            .map(|l| l.width)
            .collect();
        if ws.is_empty() {
            0.0
        } else {
            ws.iter().sum::<f32>() + spacing * (ws.len() - 1) as f32
        }
    };
    let mut cx = ((w - center_w) / 2.0)
        .min(right_start - region_gap - center_w)
        .max(left_end + region_gap);
    for l in laid.iter_mut().filter(|l| l.view.region == Region::Center) {
        l.rect = Rect::new(cx, chip_y, l.width, chip_h).snap(scale);
        cx = l.rect.right() + spacing;
    }

    // ---- tree
    let full = Rect::new(0.0, 0.0, w, h);
    let lang = strings.lang;
    let mut tree = UiTree::new(Element::new(
        ids::ROOT,
        "bar",
        Role::ToolBar,
        bar_name(lang),
        full,
    ));
    let menu_id = menu_element(&model.modules);
    for (region, gid, aid) in [
        (Region::Left, ids::LEFT, "region.left"),
        (Region::Center, ids::CENTER, "region.center"),
        (Region::Right, ids::RIGHT, "region.right"),
    ] {
        let members: Vec<&Laid> = laid.iter().filter(|l| l.view.region == region).collect();
        let has_overflow = region == Region::Right && overflow_rect.is_some();
        if members.is_empty() && !has_overflow {
            continue;
        }
        let mut bounds = members.iter().map(|l| l.rect).reduce(|a, b| a.union(&b));
        if has_overflow && let Some(o) = overflow_rect {
            bounds = Some(bounds.map_or(o, |b| b.union(&o)));
        }
        tree.add(
            ids::ROOT,
            Element::new(
                gid,
                aid,
                Role::Group,
                region_name(region, lang),
                bounds.unwrap_or_default(),
            ),
        )
        .expect("fixed ids are unique");
        for l in members {
            let v = &l.view;
            let mut e = Element::new(
                v.element,
                crate::tree::intern(&format!("module.{}", v.id)),
                v.role,
                v.name.clone(),
                l.rect,
            )
            .focusable(v.available || v.role == Role::Button)
            .enabled(v.available);
            if let Some(hp) = &v.help {
                e = e.help(hp.clone());
            }
            if Some(v.element) == menu_id {
                e = e.invokable().expanded(model.popup_open);
            } else if v.role == Role::Button {
                e = e.invokable();
            }
            tree.add(gid, e).expect("module ids are unique");
        }
        if has_overflow && let Some(o) = overflow_rect {
            let names: Vec<String> = hidden
                .iter()
                .filter_map(|id| model.modules.iter().find(|m| &m.id == id))
                .map(|m| crate::module::module_name(m, lang))
                .collect();
            let name = match lang {
                Lang::En => format!(
                    "{} modules hidden for lack of space: {}",
                    hidden.len(),
                    names.join(", ")
                ),
                Lang::Ko => format!(
                    "공간이 부족해 숨긴 모듈 {}개: {}",
                    hidden.len(),
                    names.join(", ")
                ),
            };
            tree.add(
                gid,
                Element::new(ids::OVERFLOW, "overflow", Role::Text, name, o).focusable(true),
            )
            .expect("fixed ids are unique");
        }
    }

    // ---- draw list
    let mut draw = DrawList::default();
    let alpha = (plan.surface_opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    match &d.gradient {
        Some((dir, colors)) if colors.len() >= 2 => {
            let (from, to) = match dir {
                GradientDirection::Horizontal => ((0.0, 0.5), (1.0, 0.5)),
                GradientDirection::Vertical => ((0.5, 0.0), (0.5, 1.0)),
                GradientDirection::Diagonal => ((0.0, 0.0), (1.0, 1.0)),
            };
            let n = (colors.len() - 1) as f32;
            draw.push(DrawCmd::Gradient {
                rect: full,
                radius: 0.0,
                from,
                to,
                stops: colors
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (i as f32 / n, Color { a: alpha, ..*c }))
                    .collect(),
                owner: Some(ids::ROOT),
            });
        }
        _ => draw.push(DrawCmd::Fill {
            rect: full,
            radius: 0.0,
            color: Color {
                a: alpha,
                ..style.surface
            },
            owner: Some(ids::ROOT),
        }),
    }
    // Shading inside the lower edge and a highlight along the top edge.
    if d.shadow > 0.0 {
        let a = (d.shadow * 110.0).round() as u8;
        draw.push(DrawCmd::Gradient {
            rect: Rect::new(0.0, h * 0.45, w, h * 0.55),
            radius: 0.0,
            from: (0.5, 0.0),
            to: (0.5, 1.0),
            stops: vec![
                (
                    0.0,
                    Color {
                        a: 0,
                        ..Color::BLACK
                    },
                ),
                (1.0, Color { a, ..Color::BLACK }),
            ],
            owner: Some(ids::ROOT),
        });
    }
    if d.highlight > 0.0 {
        draw.push(DrawCmd::Fill {
            rect: Rect::new(0.0, 0.0, w, 1.0),
            radius: 0.0,
            color: Color {
                a: (d.highlight * 140.0).round() as u8,
                ..Color::WHITE
            },
            owner: Some(ids::ROOT),
        });
    }
    let radius = style.radius_control;
    // Opaque colour under the modules, for readable icon colours.
    let under = |c: Color| {
        Color { a: 255, ..c }.over(Color {
            a: 255,
            ..style.surface
        })
    };
    // The opaque bar colours behind the modules.
    let bar_bgs: Vec<Color> = match &d.gradient {
        Some((_, colors)) if colors.len() >= 2 => colors.iter().map(|c| under(*c)).collect(),
        _ => vec![under(style.surface)],
    };
    let module_fill = d.module_background.unwrap_or_else(|| {
        Color {
            a: 0x1A,
            ..style.text_primary
        }
        .over(Color {
            a: 255,
            ..style.surface
        })
    });
    let shape_radius = |r: Rect| match d.module_shape {
        ModuleShape::Pill => r.h / 2.0,
        ModuleShape::Square => 0.0,
        _ => d.module_radius.unwrap_or(radius),
    };
    let hover_fill = d.hover_color.map_or(style.hover, |c| Color {
        a: style.hover.a.max(0x33),
        ..c
    });
    let hover_fill = match d.hover_intensity {
        Some(k) => Color {
            a: (f32::from(hover_fill.a) * k * 2.0).clamp(12.0, 255.0) as u8,
            ..hover_fill
        },
        None => hover_fill,
    };
    for l in &laid {
        let v = &l.view;
        let owner = Some(v.element);
        let chip = d.module_shape != ModuleShape::None || l.look.background.is_some();
        let tile = l.look.background.unwrap_or(module_fill);
        if chip {
            draw.push(DrawCmd::Fill {
                rect: l.rect,
                radius: if d.module_shape == ModuleShape::None {
                    d.module_radius.unwrap_or(radius)
                } else {
                    shape_radius(l.rect)
                },
                color: Color {
                    a: if l.look.background.is_some() {
                        255
                    } else {
                        (d.module_opacity * 255.0).round() as u8
                    },
                    ..tile
                },
                owner,
            });
        }
        if plan.backplates && !(chip && d.module_opacity >= 1.0) {
            draw.push(DrawCmd::Fill {
                rect: l.rect,
                radius: if chip { shape_radius(l.rect) } else { radius },
                color: style.surface,
                owner,
            });
        }
        if Some(v.element) == menu_id && model.popup_open {
            draw.push(DrawCmd::Fill {
                rect: l.rect,
                radius,
                color: d.selected_color.unwrap_or(style.pressed),
                owner,
            });
        } else if model.hover == Some(v.element) && v.role == Role::Button {
            draw.push(DrawCmd::Fill {
                rect: l.rect,
                radius,
                color: hover_fill,
                owner,
            });
        }
        // What is actually under the module's text: its own background,
        // the tile composited at its opacity over the bar, or the bar (the
        // gradient stops when a gradient replaces the bar colour).
        let tile_alpha = (d.module_opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        let unders: Vec<Color> = if l.look.background.is_some() {
            vec![tile]
        } else if chip {
            bar_bgs
                .iter()
                .map(|b| {
                    Color {
                        a: tile_alpha,
                        ..module_fill
                    }
                    .over(*b)
                })
                .collect()
        } else {
            bar_bgs.clone()
        };
        let bg = unders[0];
        // Every colour stays readable on what is actually under it
        // (Phase 12): 3:1 for icons, 4.5:1 for text.
        let fit_icon = |c: Color| crate::style::readable(c, &unders, 3.0);
        let fit_text = |c: Color| crate::style::readable(c, &unders, 4.5);
        let icon_color = if !v.available {
            style.text_disabled
        } else if let Some(c) = l.look.color {
            fit_icon(c)
        } else {
            fit_icon(match d.icon_tint {
                IconTint::Text => style.text_primary,
                IconTint::Secondary => style.text_secondary,
                IconTint::Accent => ensure_contrast(style.accent, style.text_primary, bg, 3.0),
                IconTint::Colorful => colorful(v.kind, style, bg),
                IconTint::Custom => d.icon_color.unwrap_or(style.text_primary),
            })
        };
        let content_w: f32 = l.widths.iter().sum::<f32>()
            + inner
                * (l.widths.len().saturating_sub(1)) as f32
                * if v.kind == ModuleKind::Clock {
                    1.5
                } else {
                    1.0
                };
        let mut px = l.rect.x + (l.rect.w - content_w) / 2.0;
        for (p, pw) in v.pieces.iter().zip(&l.widths) {
            match p {
                Piece::Text {
                    text,
                    secondary,
                    emphasis,
                    label,
                } => {
                    let color = if !v.available {
                        style.text_disabled
                    } else if *secondary {
                        style.text_secondary
                    } else if v.kind == ModuleKind::Clock && *emphasis {
                        d.clock_color.map_or(style.text_primary, fit_text)
                    } else if !*label
                        && !matches!(
                            v.kind,
                            ModuleKind::Clock
                                | ModuleKind::ActiveWindow
                                | ModuleKind::CustomText
                                | ModuleKind::CustomIcon
                        )
                    {
                        l.look
                            .color
                            .or(d.value_color)
                            .map_or(style.text_primary, fit_text)
                    } else {
                        style.text_primary
                    };
                    // Rainbow text: the colour moves across the bar.
                    let color = match (&text_gradient, v.available) {
                        (Some(g), true) => g
                            .color_at_phase((g.phase(anim_t) + px / w.max(1.0)).fract())
                            .map_or(color, |c| {
                                let (r, gg, b) = c.to_u8();
                                Color::rgb(r, gg, b)
                            }),
                        _ => color,
                    };
                    let color = if v.available { fit_text(color) } else { color };
                    draw.push(DrawCmd::Text {
                        rect: Rect::new(px, l.rect.y, *pw, l.rect.h),
                        text: text.clone(),
                        style: piece_style(p, v.kind, l.look.font_size),
                        color,
                        trim: v.trimmable,
                        owner,
                    });
                }
                Piece::Icon(g) | Piece::ExtraIcon(g) => draw.push(DrawCmd::Text {
                    rect: Rect::new(px, l.rect.y, *pw, l.rect.h),
                    text: glyph(g),
                    style: icon_for(l.look.font_size),
                    color: icon_color,
                    trim: false,
                    owner,
                }),
                Piece::Dot { warning } => {
                    let dd = 8.0;
                    draw.push(DrawCmd::Fill {
                        rect: Rect::new(px, l.rect.center_y() - dd / 2.0, dd, dd).snap(scale),
                        radius: dd / 2.0,
                        color: if *warning {
                            style.warning
                        } else {
                            style.success
                        },
                        owner,
                    });
                }
                Piece::Mark => {
                    let (cell, gap) = (5.0, 2.0);
                    let my = l.rect.center_y() - (2.0 * cell + gap) / 2.0;
                    for (i, j) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
                        draw.push(DrawCmd::Fill {
                            rect: Rect::new(
                                px + i * (cell + gap),
                                my + j * (cell + gap),
                                cell,
                                cell,
                            )
                            .snap(scale),
                            radius: 1.5,
                            color: style.accent,
                            owner,
                        });
                    }
                }
            }
            px += pw
                + inner
                    * if v.kind == ModuleKind::Clock {
                        1.5
                    } else {
                        1.0
                    };
        }
    }
    if d.separators {
        let sw = d.separator_width;
        for pair in laid.windows(2) {
            let (a, c) = (&pair[0], &pair[1]);
            if a.view.region == c.view.region {
                let x = (a.rect.right() + c.rect.x) / 2.0;
                draw.push(DrawCmd::Fill {
                    rect: Rect::new(x - sw / 2.0, chip_y + chip_h * 0.25, sw, chip_h * 0.5)
                        .snap(scale),
                    radius: 0.0,
                    color: d.separator_color.unwrap_or(Color {
                        a: 0x40,
                        ..style.text_secondary
                    }),
                    owner: Some(ids::ROOT),
                });
            }
        }
    }
    if d.border_bottom {
        let bw = d.border_width.max(1.0 / scale);
        draw.push(DrawCmd::Fill {
            rect: Rect::new(0.0, h - bw, w, bw),
            radius: 0.0,
            color: d.border_color.unwrap_or(Color {
                a: 0x66,
                ..style.border
            }),
            owner: Some(ids::ROOT),
        });
    }
    match d.accent_line {
        AccentLine::None => {}
        AccentLine::Top | AccentLine::Bottom => {
            let y = if d.accent_line == AccentLine::Top {
                0.0
            } else {
                h - 2.0
            };
            draw.push(DrawCmd::Fill {
                rect: Rect::new(0.0, y, w, 2.0),
                radius: 0.0,
                color: style.accent,
                owner: Some(ids::ROOT),
            });
        }
    }
    if let Some(o) = overflow_rect {
        if plan.backplates {
            draw.push(DrawCmd::Fill {
                rect: o,
                radius,
                color: style.surface,
                owner: Some(ids::OVERFLOW),
            });
        }
        draw.push(DrawCmd::Text {
            rect: Rect::new(o.x + pad, o.y, o.w - 2.0 * pad, o.h),
            text: format!("+{}", hidden.len()),
            style: overflow_style,
            color: style.text_secondary,
            trim: false,
            owner: Some(ids::OVERFLOW),
        });
    }
    if model.keyboard_active
        && let Some(f) = model.focus
        && let Some(e) = tree.get(f)
        && e.is_focus_target()
    {
        // Inside the bar's height: the ring must not be clipped at the top edge.
        let ring = e.bounds.inflate(1.0);
        let ring = Rect::new(ring.x, ring.y.max(1.0), ring.w, ring.h.min(h - 2.0));
        draw.push(DrawCmd::Stroke {
            rect: ring,
            radius: radius + 1.0,
            width: 2.0,
            color: style.focus_ring,
            owner: Some(f),
        });
    }

    let elements = laid
        .iter()
        .map(|l| (l.view.id.clone(), l.view.element))
        .collect();
    BarScene {
        scene: Scene {
            tree,
            draw,
            size,
            scale,
        },
        hidden,
        elements,
    }
}
