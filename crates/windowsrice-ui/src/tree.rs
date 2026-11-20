//! Accessibility element tree.
//!
//! Every visible, meaningful part of a surface is an [`Element`] with a role,
//! an accessible name, a state and bounds. The tree is built in the same
//! layout pass as the draw list, and the UI Automation provider reads it
//! directly, so the accessibility view always describes what is on screen.

use std::collections::HashMap;

use crate::geom::{Point, Rect};

/// Interns a dynamic automation id (module and control keys) as `&'static str`.
/// Each distinct string is leaked once; the set is bounded by the
/// configuration (at most 64 modules with a few controls each, plus themes).
pub fn intern(s: &str) -> &'static str {
    use std::sync::Mutex;
    static IDS: Mutex<Option<HashMap<String, &'static str>>> = Mutex::new(None);
    let mut g = IDS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let map = g.get_or_insert_with(HashMap::new);
    if let Some(v) = map.get(s) {
        return v;
    }
    let v: &'static str = Box::leak(s.to_owned().into_boxed_str());
    map.insert(s.to_owned(), v);
    v
}

/// Stable identifier of an element. The same logical element keeps the same
/// id across rebuilds, which UI Automation needs for runtime ids and events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ElementId(pub u32);

/// Control role, mapped to a UI Automation control type by the provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The surface itself (UIA ToolBar).
    ToolBar,
    /// A named region grouping related elements.
    Group,
    Button,
    /// Read-only information.
    Text,
    MenuItem,
    /// A selectable item, such as a workspace.
    ListItem,
    /// On/off setting (UIA CheckBox with the Toggle pattern).
    CheckBox,
    /// One choice of a group (UIA RadioButton with SelectionItem).
    RadioButton,
    /// A top-level surface such as the settings popup.
    Window,
    /// Single-line text entry (UIA Edit with the Value pattern).
    Edit,
    /// A value in a range (UIA Slider with the Value pattern; the value is
    /// text such as "88%").
    Slider,
}

/// Interaction capabilities, mapped to UI Automation control patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pattern {
    Invoke,
    ExpandCollapse,
    SelectionItem,
    Toggle,
    Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub id: ElementId,
    /// Stable, language-independent identifier (UIA AutomationId).
    pub automation_id: &'static str,
    pub role: Role,
    /// Accessible name. Required for every element except groups' children
    /// lists; see [`UiTree::problems`].
    pub name: String,
    /// Longer description (UIA HelpText).
    pub help: Option<String>,
    /// Bounds in DIPs relative to the surface.
    pub bounds: Rect,
    pub focusable: bool,
    pub enabled: bool,
    pub invokable: bool,
    /// `Some` for selectable elements.
    pub selected: Option<bool>,
    /// `Some` for elements that open a menu or popup.
    pub expanded: Option<bool>,
    /// `Some` for on/off elements (check boxes).
    pub toggled: Option<bool>,
    /// `Some` for editable text (the current text).
    pub value: Option<String>,
    parent: Option<usize>,
    children: Vec<usize>,
}

impl Element {
    pub fn new(
        id: ElementId,
        automation_id: &'static str,
        role: Role,
        name: impl Into<String>,
        bounds: Rect,
    ) -> Self {
        Self {
            id,
            automation_id,
            role,
            name: name.into(),
            help: None,
            bounds,
            focusable: false,
            enabled: true,
            invokable: false,
            selected: None,
            expanded: None,
            toggled: None,
            value: None,
            parent: None,
            children: Vec::new(),
        }
    }

    pub fn focusable(mut self, yes: bool) -> Self {
        self.focusable = yes;
        self
    }

    pub fn enabled(mut self, yes: bool) -> Self {
        self.enabled = yes;
        self
    }

    pub fn invokable(mut self) -> Self {
        self.invokable = true;
        self
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = Some(expanded);
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = Some(selected);
        self
    }

    pub fn toggled(mut self, on: bool) -> Self {
        self.toggled = Some(on);
        self
    }

    pub fn value(mut self, v: impl Into<String>) -> Self {
        self.value = Some(v.into());
        self
    }

    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Control patterns this element supports.
    pub fn patterns(&self) -> Vec<Pattern> {
        let mut p = Vec::new();
        if self.invokable {
            p.push(Pattern::Invoke);
        }
        if self.expanded.is_some() {
            p.push(Pattern::ExpandCollapse);
        }
        if self.selected.is_some() {
            p.push(Pattern::SelectionItem);
        }
        if self.toggled.is_some() {
            p.push(Pattern::Toggle);
        }
        if self.value.is_some() {
            p.push(Pattern::Value);
        }
        p
    }

    pub fn supports(&self, pattern: Pattern) -> bool {
        self.patterns().contains(&pattern)
    }

    /// Can receive keyboard focus right now.
    pub fn is_focus_target(&self) -> bool {
        self.focusable && self.enabled
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TreeError {
    #[error("duplicate element id {0:?}")]
    DuplicateId(ElementId),
    #[error("unknown parent element {0:?}")]
    UnknownParent(ElementId),
}

/// Arena-backed element tree. Index 0 is the root.
#[derive(Debug, Clone, PartialEq)]
pub struct UiTree {
    nodes: Vec<Element>,
    index: HashMap<ElementId, usize>,
}

impl UiTree {
    pub fn new(root: Element) -> Self {
        let mut index = HashMap::new();
        index.insert(root.id, 0);
        Self {
            nodes: vec![root],
            index,
        }
    }

    /// Appends `element` as the last child of `parent`.
    pub fn add(&mut self, parent: ElementId, mut element: Element) -> Result<ElementId, TreeError> {
        let id = element.id;
        if self.index.contains_key(&id) {
            return Err(TreeError::DuplicateId(id));
        }
        let p = *self
            .index
            .get(&parent)
            .ok_or(TreeError::UnknownParent(parent))?;
        let i = self.nodes.len();
        element.parent = Some(p);
        element.children.clear();
        self.nodes.push(element);
        self.nodes[p].children.push(i);
        self.index.insert(id, i);
        Ok(id)
    }

    pub fn root(&self) -> &Element {
        &self.nodes[0]
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn get(&self, id: ElementId) -> Option<&Element> {
        self.index.get(&id).map(|&i| &self.nodes[i])
    }

    pub fn get_mut(&mut self, id: ElementId) -> Option<&mut Element> {
        self.index.get(&id).map(|&i| &mut self.nodes[i])
    }

    pub fn contains(&self, id: ElementId) -> bool {
        self.index.contains_key(&id)
    }

    pub fn parent(&self, id: ElementId) -> Option<ElementId> {
        let e = self.get(id)?;
        e.parent.map(|p| self.nodes[p].id)
    }

    pub fn children(&self, id: ElementId) -> Vec<ElementId> {
        self.get(id)
            .map(|e| e.children.iter().map(|&c| self.nodes[c].id).collect())
            .unwrap_or_default()
    }

    pub fn first_child(&self, id: ElementId) -> Option<ElementId> {
        self.children(id).first().copied()
    }

    pub fn last_child(&self, id: ElementId) -> Option<ElementId> {
        self.children(id).last().copied()
    }

    fn sibling(&self, id: ElementId, offset: isize) -> Option<ElementId> {
        let parent = self.parent(id)?;
        let siblings = self.children(parent);
        let pos = siblings.iter().position(|&s| s == id)?;
        let target = pos.checked_add_signed(offset)?;
        siblings.get(target).copied()
    }

    pub fn next_sibling(&self, id: ElementId) -> Option<ElementId> {
        self.sibling(id, 1)
    }

    pub fn prev_sibling(&self, id: ElementId) -> Option<ElementId> {
        self.sibling(id, -1)
    }

    /// Detaches groups that ended up with no children (for example when
    /// virtualization left every member of a group outside the visible
    /// area). Repeats until no empty group remains. Returns how many.
    pub fn prune_empty_groups(&mut self) -> usize {
        let mut removed = 0;
        loop {
            let empty: Vec<usize> = (1..self.nodes.len())
                .filter(|&i| {
                    let n = &self.nodes[i];
                    n.role == Role::Group
                        && n.children.is_empty()
                        && self.index.get(&n.id) == Some(&i)
                })
                .collect();
            if empty.is_empty() {
                return removed;
            }
            for i in empty {
                if let Some(p) = self.nodes[i].parent {
                    self.nodes[p].children.retain(|&c| c != i);
                }
                let id = self.nodes[i].id;
                self.index.remove(&id);
                removed += 1;
            }
        }
    }

    /// Elements in depth-first, document order (root first).
    pub fn iter(&self) -> impl Iterator<Item = &Element> {
        let mut order = Vec::with_capacity(self.nodes.len());
        let mut stack = vec![0usize];
        while let Some(i) = stack.pop() {
            order.push(i);
            stack.extend(self.nodes[i].children.iter().rev());
        }
        order.into_iter().map(move |i| &self.nodes[i])
    }

    /// Keyboard focus order: focusable, enabled elements in document order.
    pub fn focus_order(&self) -> Vec<ElementId> {
        self.iter()
            .filter(|e| e.is_focus_target())
            .map(|e| e.id)
            .collect()
    }

    /// Deepest element containing `p` (DIPs, surface coordinates).
    pub fn hit_test(&self, p: Point) -> Option<ElementId> {
        if !self.root().bounds.contains(p) {
            return None;
        }
        let mut current = 0usize;
        'descend: loop {
            for &c in &self.nodes[current].children {
                if self.nodes[c].bounds.contains(p) {
                    current = c;
                    continue 'descend;
                }
            }
            return Some(self.nodes[current].id);
        }
    }

    /// Accessibility lint. An empty result means the tree is well formed:
    /// every element is named, focusable elements are actionable or
    /// informative, children lie inside their parents, and states are
    /// consistent.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut automation_ids = HashMap::new();
        for e in self.iter() {
            if e.name.trim().is_empty() {
                out.push(format!(
                    "{:?} ({}) has no accessible name",
                    e.id, e.automation_id
                ));
            }
            if let Some(prev) = automation_ids.insert(e.automation_id, e.id) {
                out.push(format!(
                    "automation id \"{}\" used by {prev:?} and {:?}",
                    e.automation_id, e.id
                ));
            }
            if e.bounds.w <= 0.0 || e.bounds.h <= 0.0 {
                out.push(format!("{:?} ({}) has empty bounds", e.id, e.automation_id));
            }
            if let Some(p) = e.parent
                && !self.nodes[p].bounds.contains_rect(&e.bounds, 0.01)
            {
                out.push(format!(
                    "{:?} ({}) extends outside its parent",
                    e.id, e.automation_id
                ));
            }
            if e.invokable && !matches!(e.role, Role::Button | Role::MenuItem) {
                out.push(format!(
                    "{:?} ({}) is invokable but is not a button or menu item",
                    e.id, e.automation_id
                ));
            }
            if e.role == Role::Group && e.children.is_empty() {
                out.push(format!(
                    "{:?} ({}) is an empty group",
                    e.id, e.automation_id
                ));
            }
        }
        out
    }
}

/// A change between two versions of a tree, used to raise UI Automation
/// property-changed events.
#[derive(Debug, Clone, PartialEq)]
pub enum TreeChange {
    Name {
        id: ElementId,
        old: String,
        new: String,
    },
    Enabled {
        id: ElementId,
        enabled: bool,
    },
    Expanded {
        id: ElementId,
        expanded: bool,
    },
    Selected {
        id: ElementId,
        selected: bool,
    },
    Toggled {
        id: ElementId,
        on: bool,
    },
    /// The set or order of elements changed.
    Structure,
}

/// Lists changes from `old` to `new`. Bounds-only changes are not reported.
pub fn diff(old: &UiTree, new: &UiTree) -> Vec<TreeChange> {
    let old_ids: Vec<ElementId> = old.iter().map(|e| e.id).collect();
    let new_ids: Vec<ElementId> = new.iter().map(|e| e.id).collect();
    if old_ids != new_ids {
        return vec![TreeChange::Structure];
    }
    let mut out = Vec::new();
    for n in new.iter() {
        let Some(o) = old.get(n.id) else { continue };
        if o.name != n.name {
            out.push(TreeChange::Name {
                id: n.id,
                old: o.name.clone(),
                new: n.name.clone(),
            });
        }
        if o.enabled != n.enabled {
            out.push(TreeChange::Enabled {
                id: n.id,
                enabled: n.enabled,
            });
        }
        if let (Some(a), Some(b)) = (o.expanded, n.expanded)
            && a != b
        {
            out.push(TreeChange::Expanded {
                id: n.id,
                expanded: b,
            });
        }
        if let (Some(a), Some(b)) = (o.selected, n.selected)
            && a != b
        {
            out.push(TreeChange::Selected {
                id: n.id,
                selected: b,
            });
        }
        if let (Some(a), Some(b)) = (o.toggled, n.toggled)
            && a != b
        {
            out.push(TreeChange::Toggled { id: n.id, on: b });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> UiTree {
        let r = |x: f32| Rect::new(x, 0.0, 10.0, 10.0);
        let mut t = UiTree::new(Element::new(
            ElementId(1),
            "root",
            Role::ToolBar,
            "Bar",
            Rect::new(0.0, 0.0, 100.0, 10.0),
        ));
        t.add(
            ElementId(1),
            Element::new(
                ElementId(2),
                "g",
                Role::Group,
                "G",
                Rect::new(0.0, 0.0, 50.0, 10.0),
            ),
        )
        .unwrap();
        t.add(
            ElementId(2),
            Element::new(ElementId(3), "a", Role::Button, "A", r(0.0))
                .focusable(true)
                .invokable(),
        )
        .unwrap();
        t.add(
            ElementId(2),
            Element::new(ElementId(4), "b", Role::Text, "B", r(20.0))
                .focusable(true)
                .enabled(false),
        )
        .unwrap();
        t.add(
            ElementId(1),
            Element::new(ElementId(5), "c", Role::ListItem, "C", r(60.0))
                .focusable(true)
                .selected(false),
        )
        .unwrap();
        t
    }

    #[test]
    fn navigation() {
        let t = sample();
        assert_eq!(t.children(ElementId(1)), vec![ElementId(2), ElementId(5)]);
        assert_eq!(t.parent(ElementId(3)), Some(ElementId(2)));
        assert_eq!(t.next_sibling(ElementId(3)), Some(ElementId(4)));
        assert_eq!(t.prev_sibling(ElementId(3)), None);
        assert_eq!(t.next_sibling(ElementId(2)), Some(ElementId(5)));
        assert_eq!(t.first_child(ElementId(2)), Some(ElementId(3)));
        assert_eq!(t.last_child(ElementId(2)), Some(ElementId(4)));
        assert_eq!(t.parent(ElementId(1)), None);
    }

    #[test]
    fn focus_order_skips_disabled() {
        assert_eq!(sample().focus_order(), vec![ElementId(3), ElementId(5)]);
    }

    #[test]
    fn hit_test_finds_deepest() {
        let t = sample();
        assert_eq!(t.hit_test(Point { x: 5.0, y: 5.0 }), Some(ElementId(3)));
        assert_eq!(t.hit_test(Point { x: 40.0, y: 5.0 }), Some(ElementId(2)));
        assert_eq!(t.hit_test(Point { x: 95.0, y: 5.0 }), Some(ElementId(1)));
        assert_eq!(t.hit_test(Point { x: 200.0, y: 5.0 }), None);
    }

    #[test]
    fn patterns_follow_state() {
        let t = sample();
        assert_eq!(
            t.get(ElementId(3)).unwrap().patterns(),
            vec![Pattern::Invoke]
        );
        assert_eq!(
            t.get(ElementId(5)).unwrap().patterns(),
            vec![Pattern::SelectionItem]
        );
    }

    #[test]
    fn duplicate_ids_rejected() {
        let mut t = sample();
        let e = Element::new(
            ElementId(3),
            "x",
            Role::Text,
            "X",
            Rect::new(0.0, 0.0, 1.0, 1.0),
        );
        assert_eq!(
            t.add(ElementId(1), e),
            Err(TreeError::DuplicateId(ElementId(3)))
        );
    }

    #[test]
    fn lint_reports_problems() {
        let mut t = sample();
        assert!(t.problems().is_empty(), "{:?}", t.problems());
        t.get_mut(ElementId(4)).unwrap().name.clear();
        t.get_mut(ElementId(5)).unwrap().bounds.x = 95.0;
        let p = t.problems();
        assert!(p.iter().any(|m| m.contains("no accessible name")));
        assert!(p.iter().any(|m| m.contains("outside its parent")));
    }

    #[test]
    fn diff_reports_state_changes() {
        let a = sample();
        let mut b = sample();
        b.get_mut(ElementId(3)).unwrap().name = "A2".into();
        b.get_mut(ElementId(4)).unwrap().enabled = true;
        b.get_mut(ElementId(5)).unwrap().selected = Some(true);
        let d = diff(&a, &b);
        assert_eq!(d.len(), 3);
        assert!(d.contains(&TreeChange::Selected {
            id: ElementId(5),
            selected: true
        }));
        let mut c = sample();
        c.add(
            ElementId(1),
            Element::new(
                ElementId(9),
                "z",
                Role::Text,
                "Z",
                Rect::new(0.0, 0.0, 1.0, 1.0),
            ),
        )
        .unwrap();
        assert_eq!(diff(&a, &c), vec![TreeChange::Structure]);
    }
}
