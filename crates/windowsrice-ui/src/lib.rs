//! windowsRICE UI model.
//!
//! Platform-independent: this crate turns panel data and theme tokens into a
//! [`panel::Scene`], which holds both the accessibility element tree
//! ([`tree::UiTree`]) and the draw list ([`draw::DrawList`]) produced from the
//! same layout pass. The renderer paints the draw list and the UI Automation
//! provider exposes the tree, so what is drawn and what assistive
//! technology sees cannot drift apart. It has no Windows API dependency, so
//! layout, focus, keyboard navigation and legibility rules are unit-tested
//! without a display.

#![forbid(unsafe_code)]

pub mod bar;
pub mod draw;
pub mod format;
pub mod geom;
pub mod keyboard;
pub mod legibility;
pub mod module;
pub mod panel;
pub mod placement;
pub mod popup;
pub mod strings;
pub mod studio;
pub mod style;
pub mod text;
pub mod tree;

pub use geom::{PhysRect, Point, Rect, Size};
pub use tree::{ElementId, Role, UiTree};
