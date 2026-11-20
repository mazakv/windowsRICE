//! UI Automation provider for the panel.
//!
//! The provider has no element state of its own. Every property, every
//! navigation step and every bounding rectangle is read from the same
//! [`Scene`] the renderer draws, through [`A11yState`]. A fragment root
//! provider represents the panel window (hosted on its HWND), and element
//! providers are created on demand for the tree's elements, identified by
//! their stable [`ElementId`].
//!
//! Actions requested by assistive technology (Invoke, Expand, SetFocus, …)
//! are posted to the window as `WM_APP_UIA` messages and carried out by the
//! normal input path, so keyboard, mouse and UI Automation behave alike.
//!
//! Threading: providers declare `UseComThreading` and are created on the
//! window's STA thread, so UI Automation calls arrive on that thread through
//! the message loop.

// windows-rs keeps the Win32 SDK names of UI Automation constants
// (`UIA_NamePropertyId`, …), which are matched on below.
#![allow(non_upper_case_globals)]

use std::cell::RefCell;
use std::mem::ManuallyDrop;
use std::rc::Rc;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::Ole::{SafeArrayCreateVector, SafeArrayPutElement};
use windows::Win32::System::Variant::{VARIANT, VT_BOOL, VT_BSTR, VT_I4, VariantClear};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};
use windows::core::{BSTR, Error, HRESULT, IUnknown, IUnknownImpl, Interface, Result, implement};
use windowsrice_ui::panel::{Scene, ids};
use windowsrice_ui::tree::{Element, Pattern, Role, TreeChange};
use windowsrice_ui::{ElementId, Point};

pub const WM_APP_UIA: u32 = WM_APP + 2;

const UIA_E_ELEMENTNOTAVAILABLE: HRESULT = HRESULT(0x8004_0201_u32.cast_signed());
const UIA_E_ELEMENTNOTENABLED: HRESULT = HRESULT(0x8004_0200_u32.cast_signed());
const UIA_E_INVALIDOPERATION: HRESULT = HRESULT(0x8013_1509_u32.cast_signed());

/// What the provider reads. Updated by the app after every rebuild.
#[derive(Debug)]
pub struct A11yState {
    pub scene: Scene,
    /// Window's top-left corner in physical screen pixels.
    pub origin: (i32, i32),
    /// The panel window has keyboard focus.
    pub has_focus: bool,
    pub focus: Option<ElementId>,
    pub visible: bool,
}

#[derive(Debug)]
pub struct Host {
    pub hwnd: HWND,
    /// Class name reported for the root element.
    pub class_name: &'static str,
    pub state: RefCell<A11yState>,
    /// Text set through the Value pattern, waiting for the window to take it.
    pub pending_value: RefCell<Option<(ElementId, String)>>,
    /// Number of provider calls served (overhead diagnostics).
    pub calls: std::cell::Cell<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiaAction {
    Focus,
    Invoke,
    Expand,
    Collapse,
    Select,
    Toggle,
    SetValue,
}

impl UiaAction {
    fn code(self) -> usize {
        self as usize
    }

    pub fn from_code(c: usize) -> Option<Self> {
        [
            Self::Focus,
            Self::Invoke,
            Self::Expand,
            Self::Collapse,
            Self::Select,
            Self::Toggle,
            Self::SetValue,
        ]
        .into_iter()
        .find(|a| a.code() == c)
    }
}

fn post(host: &Host, action: UiaAction, id: ElementId) -> Result<()> {
    // SAFETY: posting to our own window.
    unsafe {
        PostMessageW(
            Some(host.hwnd),
            WM_APP_UIA,
            WPARAM(action.code()),
            LPARAM(id.0 as isize),
        )
    }
}

fn with_element<R>(
    host: &Host,
    id: ElementId,
    f: impl FnOnce(&Element, &A11yState) -> R,
) -> Result<R> {
    host.calls.set(host.calls.get() + 1);
    let state = host
        .state
        .try_borrow()
        .map_err(|_| Error::from(UIA_E_ELEMENTNOTAVAILABLE))?;
    let e = state
        .scene
        .tree
        .get(id)
        .ok_or_else(|| Error::from(UIA_E_ELEMENTNOTAVAILABLE))?;
    Ok(f(e, &state))
}

// ------------------------------------------------------------- VARIANTs

fn var_i32(v: i32) -> VARIANT {
    let mut x = VARIANT::default();
    // SAFETY: writing the active union member together with its type tag.
    unsafe {
        let inner = &mut *x.Anonymous.Anonymous;
        inner.vt = VT_I4;
        inner.Anonymous.lVal = v;
    }
    x
}

fn var_bool(v: bool) -> VARIANT {
    let mut x = VARIANT::default();
    // SAFETY: as above.
    unsafe {
        let inner = &mut *x.Anonymous.Anonymous;
        inner.vt = VT_BOOL;
        inner.Anonymous.boolVal = windows::Win32::Foundation::VARIANT_BOOL(if v { -1 } else { 0 });
    }
    x
}

fn var_str(s: &str) -> VARIANT {
    let mut x = VARIANT::default();
    // SAFETY: as above; ownership of the BSTR passes to the VARIANT, which
    // the caller frees.
    unsafe {
        let inner = &mut *x.Anonymous.Anonymous;
        inner.vt = VT_BSTR;
        inner.Anonymous.bstrVal = ManuallyDrop::new(BSTR::from(s));
    }
    x
}

fn control_type(role: Role) -> UIA_CONTROLTYPE_ID {
    match role {
        Role::ToolBar => UIA_ToolBarControlTypeId,
        Role::Group => UIA_GroupControlTypeId,
        Role::Button => UIA_ButtonControlTypeId,
        Role::Text => UIA_TextControlTypeId,
        Role::MenuItem => UIA_MenuItemControlTypeId,
        Role::ListItem => UIA_ListItemControlTypeId,
        Role::CheckBox => UIA_CheckBoxControlTypeId,
        Role::RadioButton => UIA_RadioButtonControlTypeId,
        Role::Window => UIA_WindowControlTypeId,
        Role::Edit => UIA_EditControlTypeId,
        Role::Slider => UIA_SliderControlTypeId,
    }
}

fn property(host: &Host, id: ElementId, prop: UIA_PROPERTY_ID) -> Result<VARIANT> {
    with_element(host, id, |e, s| {
        let focused = s.has_focus && s.focus == Some(id);
        match prop {
            UIA_ControlTypePropertyId => var_i32(control_type(e.role).0),
            UIA_NamePropertyId => var_str(&e.name),
            UIA_AutomationIdPropertyId => var_str(e.automation_id),
            UIA_HelpTextPropertyId => e.help.as_deref().map_or_else(VARIANT::default, var_str),
            UIA_IsKeyboardFocusablePropertyId => var_bool(e.is_focus_target()),
            UIA_HasKeyboardFocusPropertyId => var_bool(focused),
            UIA_IsEnabledPropertyId => var_bool(e.enabled),
            UIA_IsControlElementPropertyId | UIA_IsContentElementPropertyId => var_bool(true),
            UIA_IsOffscreenPropertyId => var_bool(!s.visible),
            UIA_FrameworkIdPropertyId => var_str("windowsRICE"),
            UIA_ProviderDescriptionPropertyId => {
                var_str("windowsRICE panel UI Automation provider")
            }
            UIA_ClassNamePropertyId if id == ids::ROOT => var_str(host.class_name),
            UIA_IsDialogPropertyId if id == ids::ROOT && e.role == Role::Window => var_bool(true),
            _ => VARIANT::default(),
        }
    })
}

fn runtime_id(id: ElementId) -> Result<*mut SAFEARRAY> {
    // SAFETY: creates a two-element VT_I4 SAFEARRAY that UI Automation frees.
    unsafe {
        let sa = SafeArrayCreateVector(VT_I4, 0, 2);
        if sa.is_null() {
            return Err(Error::from(HRESULT(0x8007_000E_u32.cast_signed())));
        }
        for (i, v) in [UiaAppendRuntimeId as i32, id.0 as i32].iter().enumerate() {
            let index = i as i32;
            SafeArrayPutElement(sa, &raw const index, (&raw const *v).cast())?;
        }
        Ok(sa)
    }
}

fn bounds(host: &Host, id: ElementId) -> Result<UiaRect> {
    with_element(host, id, |e, s| {
        let r = e.bounds.to_physical(s.scene.scale, s.origin);
        UiaRect {
            left: f64::from(r.left),
            top: f64::from(r.top),
            width: f64::from(r.width()),
            height: f64::from(r.height()),
        }
    })
}

/// Provider for any element id (the root id yields the root provider).
pub fn fragment(host: &Rc<Host>, id: ElementId) -> IRawElementProviderFragment {
    if id == ids::ROOT {
        RootProvider { host: host.clone() }.into()
    } else {
        ElementProvider {
            host: host.clone(),
            id,
        }
        .into()
    }
}

pub fn simple(host: &Rc<Host>, id: ElementId) -> IRawElementProviderSimple {
    fragment(host, id)
        .cast()
        .expect("every fragment implements IRawElementProviderSimple")
}

fn navigate(
    host: &Rc<Host>,
    id: ElementId,
    dir: NavigateDirection,
) -> Result<IRawElementProviderFragment> {
    let target = with_element(host, id, |_, s| {
        let t = &s.scene.tree;
        match dir {
            NavigateDirection_Parent => t.parent(id),
            NavigateDirection_NextSibling => t.next_sibling(id),
            NavigateDirection_PreviousSibling => t.prev_sibling(id),
            NavigateDirection_FirstChild => t.first_child(id),
            NavigateDirection_LastChild => t.last_child(id),
            _ => None,
        }
    })?;
    match target {
        // Root: parent and siblings come from the HWND host provider.
        Some(_) if id == ids::ROOT && dir == NavigateDirection_Parent => Err(Error::empty()),
        Some(t) => Ok(fragment(host, t)),
        None => Err(Error::empty()),
    }
}

// ------------------------------------------------------------ root

#[derive(Debug)]
#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    IRawElementProviderFragmentRoot
)]
pub struct RootProvider {
    pub host: Rc<Host>,
}

impl IRawElementProviderSimple_Impl for RootProvider_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider | ProviderOptions_UseComThreading)
    }
    fn GetPatternProvider(&self, _pattern: UIA_PATTERN_ID) -> Result<IUnknown> {
        Err(Error::empty())
    }
    fn GetPropertyValue(&self, prop: UIA_PROPERTY_ID) -> Result<VARIANT> {
        property(&self.host, ids::ROOT, prop)
    }
    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        // SAFETY: our own window handle.
        unsafe { UiaHostProviderFromHwnd(self.host.hwnd) }
    }
}

impl IRawElementProviderFragment_Impl for RootProvider_Impl {
    fn Navigate(&self, dir: NavigateDirection) -> Result<IRawElementProviderFragment> {
        if dir == NavigateDirection_Parent
            || dir == NavigateDirection_NextSibling
            || dir == NavigateDirection_PreviousSibling
        {
            return Err(Error::empty());
        }
        navigate(&self.host, ids::ROOT, dir)
    }
    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        // The host provider supplies the root's runtime id.
        Ok(std::ptr::null_mut())
    }
    fn BoundingRectangle(&self) -> Result<UiaRect> {
        // The host provider supplies the window rectangle.
        Ok(UiaRect::default())
    }
    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }
    fn SetFocus(&self) -> Result<()> {
        Ok(())
    }
    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(RootProvider {
            host: self.host.clone(),
        }
        .into())
    }
}

impl IRawElementProviderFragmentRoot_Impl for RootProvider_Impl {
    fn ElementProviderFromPoint(&self, x: f64, y: f64) -> Result<IRawElementProviderFragment> {
        let hit = with_element(&self.host, ids::ROOT, |_, s| {
            let scale = f64::from(s.scene.scale);
            let p = Point {
                x: ((x - f64::from(s.origin.0)) / scale) as f32,
                y: ((y - f64::from(s.origin.1)) / scale) as f32,
            };
            s.scene.tree.hit_test(p)
        })?;
        Ok(fragment(&self.host, hit.unwrap_or(ids::ROOT)))
    }
    fn GetFocus(&self) -> Result<IRawElementProviderFragment> {
        let focus = with_element(&self.host, ids::ROOT, |_, s| {
            if s.has_focus { s.focus } else { None }
        })?;
        focus
            .map(|f| fragment(&self.host, f))
            .ok_or_else(Error::empty)
    }
}

// ------------------------------------------------------------ elements

#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    IInvokeProvider,
    IExpandCollapseProvider,
    ISelectionItemProvider,
    IToggleProvider,
    IValueProvider
)]
#[derive(Debug)]
pub struct ElementProvider {
    pub host: Rc<Host>,
    pub id: ElementId,
}

impl ElementProvider {
    fn require(&self, pattern: Pattern) -> Result<()> {
        let (supported, enabled) =
            with_element(&self.host, self.id, |e, _| (e.supports(pattern), e.enabled))?;
        if !supported {
            Err(Error::from(UIA_E_INVALIDOPERATION))
        } else if !enabled {
            Err(Error::from(UIA_E_ELEMENTNOTENABLED))
        } else {
            Ok(())
        }
    }
}

impl IRawElementProviderSimple_Impl for ElementProvider_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider | ProviderOptions_UseComThreading)
    }
    fn GetPatternProvider(&self, pattern: UIA_PATTERN_ID) -> Result<IUnknown> {
        let wanted = match pattern {
            UIA_InvokePatternId => Pattern::Invoke,
            UIA_ExpandCollapsePatternId => Pattern::ExpandCollapse,
            UIA_SelectionItemPatternId => Pattern::SelectionItem,
            UIA_TogglePatternId => Pattern::Toggle,
            UIA_ValuePatternId => Pattern::Value,
            _ => return Err(Error::empty()),
        };
        if with_element(&self.host, self.id, |e, _| e.supports(wanted))? {
            let unknown: IUnknown = self.to_interface();
            Ok(unknown)
        } else {
            Err(Error::empty())
        }
    }
    fn GetPropertyValue(&self, prop: UIA_PROPERTY_ID) -> Result<VARIANT> {
        property(&self.host, self.id, prop)
    }
    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        Err(Error::empty())
    }
}

impl IRawElementProviderFragment_Impl for ElementProvider_Impl {
    fn Navigate(&self, dir: NavigateDirection) -> Result<IRawElementProviderFragment> {
        navigate(&self.host, self.id, dir)
    }
    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        with_element(&self.host, self.id, |_, _| ())?;
        runtime_id(self.id)
    }
    fn BoundingRectangle(&self) -> Result<UiaRect> {
        bounds(&self.host, self.id)
    }
    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }
    fn SetFocus(&self) -> Result<()> {
        let ok = with_element(&self.host, self.id, |e, _| e.is_focus_target())?;
        if !ok {
            return Err(Error::from(UIA_E_INVALIDOPERATION));
        }
        post(&self.host, UiaAction::Focus, self.id)
    }
    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(RootProvider {
            host: self.host.clone(),
        }
        .into())
    }
}

impl IInvokeProvider_Impl for ElementProvider_Impl {
    fn Invoke(&self) -> Result<()> {
        self.require(Pattern::Invoke)?;
        post(&self.host, UiaAction::Invoke, self.id)
    }
}

impl IExpandCollapseProvider_Impl for ElementProvider_Impl {
    fn Expand(&self) -> Result<()> {
        self.require(Pattern::ExpandCollapse)?;
        post(&self.host, UiaAction::Expand, self.id)
    }
    fn Collapse(&self) -> Result<()> {
        self.require(Pattern::ExpandCollapse)?;
        post(&self.host, UiaAction::Collapse, self.id)
    }
    fn ExpandCollapseState(&self) -> Result<ExpandCollapseState> {
        with_element(&self.host, self.id, |e, _| match e.expanded {
            Some(true) => ExpandCollapseState_Expanded,
            Some(false) => ExpandCollapseState_Collapsed,
            None => ExpandCollapseState_LeafNode,
        })
    }
}

impl ISelectionItemProvider_Impl for ElementProvider_Impl {
    fn Select(&self) -> Result<()> {
        self.require(Pattern::SelectionItem)?;
        post(&self.host, UiaAction::Select, self.id)
    }
    fn AddToSelection(&self) -> Result<()> {
        Err(Error::from(UIA_E_INVALIDOPERATION))
    }
    fn RemoveFromSelection(&self) -> Result<()> {
        Err(Error::from(UIA_E_INVALIDOPERATION))
    }
    fn IsSelected(&self) -> Result<windows::core::BOOL> {
        with_element(&self.host, self.id, |e, _| {
            e.selected.unwrap_or(false).into()
        })
    }
    fn SelectionContainer(&self) -> Result<IRawElementProviderSimple> {
        let parent = with_element(&self.host, self.id, |_, s| s.scene.tree.parent(self.id))?;
        parent
            .map(|p| simple(&self.host, p))
            .ok_or_else(Error::empty)
    }
}

impl IToggleProvider_Impl for ElementProvider_Impl {
    fn Toggle(&self) -> Result<()> {
        self.require(Pattern::Toggle)?;
        post(&self.host, UiaAction::Toggle, self.id)
    }
    fn ToggleState(&self) -> Result<ToggleState> {
        with_element(&self.host, self.id, |e, _| match e.toggled {
            Some(true) => ToggleState_On,
            _ => ToggleState_Off,
        })
    }
}

impl IValueProvider_Impl for ElementProvider_Impl {
    fn SetValue(&self, val: &windows::core::PCWSTR) -> Result<()> {
        self.require(Pattern::Value)?;
        // SAFETY: UI Automation passes a valid NUL-terminated string.
        let s = unsafe { val.to_string() }.map_err(|_| Error::from(UIA_E_INVALIDOPERATION))?;
        *self.host.pending_value.borrow_mut() = Some((self.id, s));
        post(&self.host, UiaAction::SetValue, self.id)
    }
    fn Value(&self) -> Result<BSTR> {
        with_element(&self.host, self.id, |e, _| {
            BSTR::from(e.value.clone().unwrap_or_default())
        })
    }
    fn IsReadOnly(&self) -> Result<windows::core::BOOL> {
        with_element(&self.host, self.id, |e, _| (!e.enabled).into())
    }
}

// ------------------------------------------------------------ events

pub fn clients_listening() -> bool {
    // SAFETY: no preconditions.
    unsafe { UiaClientsAreListening().as_bool() }
}

/// Raises the focus-changed event for `id`. Returns whether it was raised.
pub fn raise_focus(host: &Rc<Host>, id: ElementId) -> bool {
    if !clients_listening() {
        return false;
    }
    // SAFETY: raising an event for our own provider.
    unsafe { UiaRaiseAutomationEvent(&simple(host, id), UIA_AutomationFocusChangedEventId) }.is_ok()
}

fn raise_property(
    host: &Rc<Host>,
    id: ElementId,
    prop: UIA_PROPERTY_ID,
    mut old: VARIANT,
    mut new: VARIANT,
) -> bool {
    // SAFETY: raising an event for our own provider; the VARIANTs are ours
    // and freed afterwards.
    unsafe {
        let ok =
            UiaRaiseAutomationPropertyChangedEvent(&simple(host, id), prop, &old, &new).is_ok();
        let _ = VariantClear(&raw mut old);
        let _ = VariantClear(&raw mut new);
        ok
    }
}

/// Raises property-changed and structure-changed events for `changes`.
/// Returns the number of events raised.
pub fn raise_changes(host: &Rc<Host>, changes: &[TreeChange]) -> u32 {
    if changes.is_empty() || !clients_listening() {
        return 0;
    }
    let mut n = 0;
    for c in changes {
        let ok = match c {
            TreeChange::Name { id, old, new } => {
                raise_property(host, *id, UIA_NamePropertyId, var_str(old), var_str(new))
            }
            TreeChange::Enabled { id, enabled } => raise_property(
                host,
                *id,
                UIA_IsEnabledPropertyId,
                var_bool(!enabled),
                var_bool(*enabled),
            ),
            TreeChange::Expanded { id, expanded } => {
                let (o, n) = if *expanded {
                    (ExpandCollapseState_Collapsed, ExpandCollapseState_Expanded)
                } else {
                    (ExpandCollapseState_Expanded, ExpandCollapseState_Collapsed)
                };
                raise_property(
                    host,
                    *id,
                    UIA_ExpandCollapseExpandCollapseStatePropertyId,
                    var_i32(o.0),
                    var_i32(n.0),
                )
            }
            TreeChange::Selected { id, selected } => raise_property(
                host,
                *id,
                UIA_SelectionItemIsSelectedPropertyId,
                var_bool(!selected),
                var_bool(*selected),
            ),
            TreeChange::Toggled { id, on } => {
                let (o, n) = if *on {
                    (ToggleState_Off, ToggleState_On)
                } else {
                    (ToggleState_On, ToggleState_Off)
                };
                raise_property(
                    host,
                    *id,
                    UIA_ToggleToggleStatePropertyId,
                    var_i32(o.0),
                    var_i32(n.0),
                )
            }
            TreeChange::Structure => {
                // SAFETY: structure event on our own root provider.
                unsafe {
                    UiaRaiseStructureChangedEvent(
                        &simple(host, ids::ROOT),
                        StructureChangeType_ChildrenInvalidated,
                        std::ptr::null_mut(),
                        0,
                    )
                }
                .is_ok()
            }
        };
        if ok {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every action posted by a provider must decode to itself; an action
    /// missing from `from_code` would be dropped silently.
    #[test]
    fn every_action_round_trips() {
        for a in [
            UiaAction::Focus,
            UiaAction::Invoke,
            UiaAction::Expand,
            UiaAction::Collapse,
            UiaAction::Select,
            UiaAction::Toggle,
            UiaAction::SetValue,
        ] {
            assert_eq!(UiaAction::from_code(a.code()), Some(a));
        }
    }
}
