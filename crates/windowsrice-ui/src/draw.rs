//! Retained draw list produced by layout and consumed by the renderer.

use windowsrice_theme::Color;

use crate::geom::Rect;
use crate::tree::ElementId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontRole {
    /// The theme's UI family with its fallback chain (Segoe UI Variable,
    /// then Malgun Gothic for Hangul).
    Ui,
    /// Segoe Fluent Icons, falling back to Segoe MDL2 Assets.
    Icon,
    /// A specific installed family (font gallery samples, the classic icon
    /// set). Hangul still falls back to the theme's fallback families, so a
    /// sample is only shown in Korean when the family has Hangul itself.
    Family(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub font: FontRole,
    /// Font size in DIPs.
    pub size: f32,
    /// Exact weight (400, 500 or 600).
    pub weight: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DrawCmd {
    Fill {
        rect: Rect,
        radius: f32,
        color: Color,
        owner: Option<ElementId>,
    },
    Stroke {
        rect: Rect,
        radius: f32,
        width: f32,
        color: Color,
        owner: Option<ElementId>,
    },
    /// Single-line text, vertically centred in `rect`. When `trim` is set,
    /// text wider than `rect` ends with an ellipsis.
    Text {
        rect: Rect,
        text: String,
        style: TextStyle,
        color: Color,
        trim: bool,
        owner: Option<ElementId>,
    },
    /// Text wrapped at word boundaries inside `rect` (top-aligned).
    Wrapped {
        rect: Rect,
        text: String,
        style: TextStyle,
        color: Color,
        owner: Option<ElementId>,
    },
    /// Linear gradient fill. `from` and `to` are relative points (0–1)
    /// in `rect`; `stops` are (position 0–1, colour).
    Gradient {
        rect: Rect,
        radius: f32,
        from: (f32, f32),
        to: (f32, f32),
        stops: Vec<(f32, Color)>,
        owner: Option<ElementId>,
    },
    /// Clip following commands to `rect` until the matching `PopClip`.
    PushClip {
        rect: Rect,
    },
    PopClip,
    /// An image file from the design library (Phase 14), scaled to cover
    /// `rect` (cropped to keep its aspect ratio). For a video, the frame at
    /// `frame_ms` is drawn (a still poster frame).
    Image {
        rect: Rect,
        path: String,
        frame_ms: u32,
        opacity: f32,
        owner: Option<ElementId>,
    },
}

impl DrawCmd {
    pub fn rect(&self) -> Rect {
        match self {
            DrawCmd::Fill { rect, .. }
            | DrawCmd::Stroke { rect, .. }
            | DrawCmd::Text { rect, .. }
            | DrawCmd::Wrapped { rect, .. }
            | DrawCmd::Gradient { rect, .. }
            | DrawCmd::PushClip { rect }
            | DrawCmd::Image { rect, .. } => *rect,
            DrawCmd::PopClip => Rect::default(),
        }
    }

    pub fn owner(&self) -> Option<ElementId> {
        match self {
            DrawCmd::Fill { owner, .. }
            | DrawCmd::Stroke { owner, .. }
            | DrawCmd::Text { owner, .. }
            | DrawCmd::Wrapped { owner, .. }
            | DrawCmd::Gradient { owner, .. }
            | DrawCmd::Image { owner, .. } => *owner,
            DrawCmd::PushClip { .. } | DrawCmd::PopClip => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DrawList {
    pub cmds: Vec<DrawCmd>,
}

impl DrawList {
    pub fn push(&mut self, cmd: DrawCmd) {
        self.cmds.push(cmd);
    }

    pub fn texts(&self) -> impl Iterator<Item = (&str, &TextStyle, Option<ElementId>)> {
        self.cmds.iter().filter_map(|c| match c {
            DrawCmd::Text {
                text, style, owner, ..
            } => Some((text.as_str(), style, *owner)),
            _ => None,
        })
    }
}

/// Copies `cmds` scaled by `s` and moved by `(dx, dy)` (used to show a
/// scene inside another, such as the Design Studio's bar preview). Owners
/// are dropped: the copy is a picture, not part of the host's tree.
pub fn transformed(cmds: &[DrawCmd], dx: f32, dy: f32, s: f32) -> Vec<DrawCmd> {
    let tr = |r: &Rect| Rect::new(dx + r.x * s, dy + r.y * s, r.w * s, r.h * s);
    cmds.iter()
        .map(|c| match c {
            DrawCmd::Fill {
                rect,
                radius,
                color,
                ..
            } => DrawCmd::Fill {
                rect: tr(rect),
                radius: radius * s,
                color: *color,
                owner: None,
            },
            DrawCmd::Stroke {
                rect,
                radius,
                width,
                color,
                ..
            } => DrawCmd::Stroke {
                rect: tr(rect),
                radius: radius * s,
                width: (width * s).max(0.5),
                color: *color,
                owner: None,
            },
            DrawCmd::Text {
                rect,
                text,
                style,
                color,
                trim,
                ..
            } => DrawCmd::Text {
                rect: tr(rect),
                text: text.clone(),
                style: TextStyle {
                    size: style.size * s,
                    ..*style
                },
                color: *color,
                trim: *trim,
                owner: None,
            },
            DrawCmd::Wrapped {
                rect,
                text,
                style,
                color,
                ..
            } => DrawCmd::Wrapped {
                rect: tr(rect),
                text: text.clone(),
                style: TextStyle {
                    size: style.size * s,
                    ..*style
                },
                color: *color,
                owner: None,
            },
            DrawCmd::Gradient {
                rect,
                radius,
                from,
                to,
                stops,
                ..
            } => DrawCmd::Gradient {
                rect: tr(rect),
                radius: radius * s,
                from: *from,
                to: *to,
                stops: stops.clone(),
                owner: None,
            },
            DrawCmd::PushClip { rect } => DrawCmd::PushClip { rect: tr(rect) },
            DrawCmd::PopClip => DrawCmd::PopClip,
            DrawCmd::Image {
                rect,
                path,
                frame_ms,
                opacity,
                owner,
            } => DrawCmd::Image {
                rect: tr(rect),
                path: path.clone(),
                frame_ms: *frame_ms,
                opacity: *opacity,
                owner: *owner,
            },
        })
        .collect()
}
