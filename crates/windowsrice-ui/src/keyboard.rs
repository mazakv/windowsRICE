//! Keyboard navigation over the element tree.
//!
//! - Tab / Right Arrow: next focusable element (wraps).
//! - Shift+Tab / Left Arrow: previous focusable element (wraps).
//! - Home / End: first / last focusable element.
//! - Enter / Space: invoke the focused button, or open its menu.
//! - Down Arrow or Alt+Down Arrow: open the focused element's menu.
//! - Escape: leave the panel (return focus to the previous window).

use crate::tree::{ElementId, UiTree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Tab,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Enter,
    Space,
    Escape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOutcome {
    /// Not handled.
    Ignored,
    /// Move keyboard focus.
    Focus(ElementId),
    Invoke(ElementId),
    Expand(ElementId),
    /// Flip a check box.
    Toggle(ElementId),
    /// Choose a radio button.
    Select(ElementId),
    /// Leave the panel.
    Dismiss,
}

/// How arrow keys move focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    /// Bar: Left/Right move focus, Down opens a menu.
    Horizontal,
    /// Lists such as the settings popup: Up/Down and Left/Right move focus.
    Vertical,
}

pub fn handle_key(
    tree: &UiTree,
    focus: Option<ElementId>,
    key: Key,
    mods: Modifiers,
) -> KeyOutcome {
    handle_key_oriented(tree, focus, key, mods, Orientation::Horizontal)
}

/// Like [handle_key], with the arrow-key behaviour of orientation.
/// Space flips a check box and Enter or Space chooses a radio button.
pub fn handle_key_oriented(
    tree: &UiTree,
    focus: Option<ElementId>,
    key: Key,
    mods: Modifiers,
    orientation: Orientation,
) -> KeyOutcome {
    let order = tree.focus_order();
    if order.is_empty() {
        return if key == Key::Escape {
            KeyOutcome::Dismiss
        } else {
            KeyOutcome::Ignored
        };
    }
    let pos = focus.and_then(|f| order.iter().position(|&o| o == f));
    let n = order.len();
    let step = |forward: bool| -> KeyOutcome {
        let next = match (pos, forward) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
        };
        KeyOutcome::Focus(order[next])
    };
    let current = pos.map(|i| order[i]);
    match key {
        Key::Tab => step(!mods.shift),
        Key::Right => step(true),
        Key::Left => step(false),
        Key::Home => KeyOutcome::Focus(order[0]),
        Key::End => KeyOutcome::Focus(order[n - 1]),
        Key::Escape => KeyOutcome::Dismiss,
        Key::Enter | Key::Space => {
            let Some(id) = current else {
                return step(true);
            };
            let Some(e) = tree.get(id) else {
                return KeyOutcome::Ignored;
            };
            if e.toggled.is_some() {
                KeyOutcome::Toggle(id)
            } else if e.selected.is_some() {
                KeyOutcome::Select(id)
            } else if e.invokable {
                KeyOutcome::Invoke(id)
            } else if e.expanded.is_some() {
                KeyOutcome::Expand(id)
            } else {
                KeyOutcome::Ignored
            }
        }
        Key::Down if orientation == Orientation::Vertical => step(true),
        Key::Up if orientation == Orientation::Vertical => step(false),
        Key::Down => match current.and_then(|id| tree.get(id)) {
            Some(e) if e.expanded.is_some() => KeyOutcome::Expand(e.id),
            _ => KeyOutcome::Ignored,
        },
        Key::Up => KeyOutcome::Ignored,
    }
}
