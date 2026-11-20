//! windowsRICE Bar application state and event handling.
//!
//! One controller (a hidden top-level window that receives display, setting
//! and hotkey broadcasts) owns:
//!
//! - one full-width [`BarWin`] per configured monitor, each with its own
//!   renderer, DPI, accessibility host and work-area reservation;
//! - the settings [`PopupWin`] while it is open;
//! - the shared configuration, theme, readings and module scheduler.
//!
//! Frames are event-driven: a surface is drawn only when its rebuilt scene
//! differs from the previous one, and never while hidden or collapsed.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use serde_json::json;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromWindow};
use windows::Win32::UI::Accessibility::{
    HWINEVENTHOOK, SetWinEventHook, UiaReturnRawElementProvider, UnhookWinEvent,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    RegisterHotKey, SetFocus, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, UnregisterHotKey,
    VK_CONTROL, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_LEFT, VK_MENU, VK_RETURN, VK_RIGHT,
    VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_NAMECHANGE, EVENT_SYSTEM_FOREGROUND,
    GetCursorPos, GetForegroundWindow, KillTimer, OBJID_WINDOW, PostQuitMessage, SW_HIDE,
    SW_SHOWNOACTIVATE, SetForegroundWindow, SetTimer, ShowWindow, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
};
use windows::core::w;
use windowsrice_core::bar::ModuleKind;
use windowsrice_core::config::{Config, ThemeModePreference, ThemeSelection};
use windowsrice_core::hotkey::Key as HotkeyKey;
use windowsrice_core::paths::AppPaths;
use windowsrice_render::{FontConfig, RenderError, Renderer, TextSystem};
use windowsrice_theme::Mode;
use windowsrice_theme::catalog::ThemeCatalog;
use windowsrice_theme::engine::{ResolvedTheme, ThemeEngine, ThemeSink};
use windowsrice_theme::notice::Notice;
use windowsrice_theme::schema::Backdrop;
use windowsrice_theme::service::ThemeService;
use windowsrice_ui::bar::{BarModel, build_bar, menu_element};
use windowsrice_ui::format::LocalTime;
use windowsrice_ui::keyboard::{Key, KeyOutcome, Modifiers, Orientation, handle_key_oriented};
use windowsrice_ui::legibility::{BackplateMode, SurfacePlan, SurfaceReason, SurfaceRequest, plan};
use windowsrice_ui::module::{BarReadings, Reading};
use windowsrice_ui::panel::Scene;
use windowsrice_ui::placement::{bar_rect, collapsed_rect};
use windowsrice_ui::popup::{
    ModuleStatus, POPUP_WIDTH, PopupAction, PopupModel, apply_module_action, build_popup,
};
use windowsrice_ui::strings::{Lang, Strings};
use windowsrice_ui::style::PanelStyle;
use windowsrice_ui::tree::diff;
use windowsrice_ui::{ElementId, PhysRect, Point, Size};
use windowsrice_windows::live::{appearance, clock, foreground, monitors};

use crate::appbar;
use crate::args::Args;
use crate::scheduler::Scheduler;
use crate::stats::{Stats, ms_since_process_start};
use crate::uia::{self, A11yState, Host, UiaAction};
use crate::window::{self, process_counters};

pub const CLASS_BAR: windows::core::PCWSTR = w!("windowsRICE.Panel");
pub const CLASS_POPUP: windows::core::PCWSTR = w!("windowsRICE.Popup");
pub const CLASS_CONTROLLER: windows::core::PCWSTR = w!("windowsRICE.Controller");

// Controller timers.
pub const TIMER_CLOCK: usize = 1;
pub const TIMER_EXIT: usize = 2;
pub const TIMER_FULLSCREEN: usize = 4;
pub const TIMER_DEVICE_LOSS: usize = 5;
pub const TIMER_SUSPEND_HIDE: usize = 6;
pub const TIMER_SUSPEND_SHOW: usize = 7;
pub const TIMER_DEVICE_LOSS_EVERY: usize = 8;
pub const TIMER_DISPLAY: usize = 9;
// Bar timers.
pub const TIMER_AUTOHIDE: usize = 20;
pub const HOTKEY_FOCUS: i32 = 1;
const COLLAPSED_PX: i32 = 2;

thread_local! {
    static CTL: RefCell<Option<Controller>> = const { RefCell::new(None) };
}

/// Runs `f` on the controller; `None` if it does not exist or is borrowed
/// (a message re-entered the window procedure).
pub fn with_ctl<R>(f: impl FnOnce(&mut Controller) -> R) -> Option<R> {
    CTL.with(|c| c.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

pub fn install(c: Controller) {
    CTL.with(|x| *x.borrow_mut() = Some(c));
}

pub fn uninstall() {
    CTL.with(|x| drop(x.borrow_mut().take()));
}

/// Work that must run outside the controller borrow because it pumps
/// messages or re-enters the window procedure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Deferred {
    Nothing,
    /// Create the popup for the bar with this window handle.
    OpenPopup(isize),
    /// Bring a window to the foreground and give it keyboard focus.
    Activate(isize),
    /// Return activation to the window that had it.
    Dismiss(Option<isize>),
    /// Exit the application.
    Quit,
    /// Open (or bring forward) the Design Studio.
    OpenStudio,
    /// Ask the user (modal message box) and report the answer to the Studio.
    Confirm(crate::studio_win::Confirm),
    /// Native file dialogs for preset import and export.
    OpenFile,
    SaveFile(String),
    /// Windows colour dialog, starting at this `COLORREF`.
    PickColor(u32),
}

#[derive(Debug, Default)]
pub(crate) struct BarSink {
    accepted: u32,
}

impl ThemeSink for BarSink {
    fn apply(&mut self, _t: &ResolvedTheme) -> Result<(), String> {
        self.accepted += 1;
        Ok(())
    }
}

pub(crate) enum ThemeSource {
    /// Normal operation: configuration loaded from (and saved to) a file.
    Service(ThemeService),
    /// `--theme` / `--mode`: shown, never saved.
    Override { engine: ThemeEngine, config: Config },
}

impl ThemeSource {
    pub(crate) fn applied(&self) -> Option<&ResolvedTheme> {
        match self {
            ThemeSource::Service(s) => s.applied(),
            ThemeSource::Override { engine, .. } => engine.applied(),
        }
    }
    pub(crate) fn config(&self) -> &Config {
        match self {
            ThemeSource::Service(s) => s.config(),
            ThemeSource::Override { config, .. } => config,
        }
    }
    pub(crate) fn catalog(&self) -> &ThemeCatalog {
        match self {
            ThemeSource::Service(s) => s.engine().catalog(),
            ThemeSource::Override { engine, .. } => engine.catalog(),
        }
    }
    pub(crate) fn is_fallback(&self) -> bool {
        match self {
            ThemeSource::Service(s) => s.engine().is_fallback(),
            ThemeSource::Override { engine, .. } => engine.is_fallback(),
        }
    }
}

/// A rendered window: renderer, accessibility host and interaction state.
pub(crate) struct Surface {
    pub(crate) hwnd: HWND,
    pub(crate) renderer: Renderer,
    pub(crate) host: Rc<Host>,
    pub(crate) scale: f32,
    pub(crate) dpi: u32,
    pub(crate) dirty: bool,
    pub(crate) reason: &'static str,
    pub(crate) visible: bool,
    pub(crate) shown_once: bool,
    pub(crate) focus: Option<ElementId>,
    pub(crate) keyboard_active: bool,
    pub(crate) hover: Option<ElementId>,
    pub(crate) pending_input: Option<Instant>,
}

impl Surface {
    pub(crate) fn new(
        hwnd: HWND,
        text: TextSystem,
        class_name: &'static str,
        scene: Scene,
    ) -> Result<Self, String> {
        let renderer = Renderer::new(hwnd, 1, 1, 96, text).map_err(|e| format!("renderer: {e}"))?;
        Ok(Self {
            hwnd,
            renderer,
            host: Rc::new(Host {
                hwnd,
                class_name,
                state: RefCell::new(A11yState {
                    scene,
                    origin: (0, 0),
                    has_focus: false,
                    focus: None,
                    visible: false,
                }),
                pending_value: RefCell::new(None),
                calls: std::cell::Cell::new(0),
            }),
            scale: 1.0,
            dpi: 96,
            dirty: true,
            reason: "initial",
            visible: false,
            shown_once: false,
            focus: None,
            keyboard_active: false,
            hover: None,
            pending_input: None,
        })
    }

    /// Replaces the scene; raises UI Automation events for changes.
    /// Returns the number of events raised.
    pub(crate) fn set_scene(
        &mut self,
        scene: Scene,
        origin: (i32, i32),
        reason: &'static str,
    ) -> u32 {
        let changes = {
            let mut st = self.host.state.borrow_mut();
            st.origin = origin;
            st.focus = self.focus;
            st.has_focus = self.keyboard_active;
            st.visible = self.visible;
            if st.scene == scene {
                return 0;
            }
            let changes = diff(&st.scene.tree, &scene.tree);
            st.scene = scene;
            changes
        };
        self.dirty = true;
        self.reason = reason;
        uia::raise_changes(&self.host, &changes)
    }

    pub(crate) fn tree_hit(&self, x: i32, y: i32) -> Option<ElementId> {
        let p = Point {
            x: x as f32 / self.scale,
            y: y as f32 / self.scale,
        };
        self.host.state.borrow().scene.tree.hit_test(p)
    }

    pub(crate) fn release_uia(&self) {
        // SAFETY: tells UI Automation this window's providers are gone.
        unsafe {
            UiaReturnRawElementProvider(self.hwnd, WPARAM(0), LPARAM(0), None);
        }
    }
}

pub(crate) struct BarWin {
    pub(crate) s: Surface,
    pub(crate) device: String,
    pub(crate) monitor: monitors::MonitorInfo,
    /// Full (expanded) window rectangle.
    pub(crate) rect: PhysRect,
    pub(crate) appbar: bool,
    pub(crate) collapsed: bool,
    /// Hidden for a fullscreen application.
    pub(crate) yielded: bool,
    pub(crate) hidden_modules: Vec<String>,
}

pub(crate) struct PopupWin {
    pub(crate) s: Surface,
    pub(crate) owner: HWND,
    pub(crate) origin: (i32, i32),
    pub(crate) size: PhysRect,
    pub(crate) actions: Vec<(ElementId, windowsrice_ui::popup::PopupAction)>,
}

pub(crate) struct Controller {
    pub(crate) hwnd: HWND,
    pub(crate) args: Args,
    pub(crate) strings: Strings,
    pub(crate) theme: ThemeSource,
    pub(crate) sink: BarSink,
    pub(crate) notices: Vec<String>,
    pub(crate) style: PanelStyle,
    pub(crate) plan: SurfacePlan,
    pub(crate) backdrop_ok: bool,
    pub(crate) readings: BarReadings,
    pub(crate) scheduler: Scheduler,
    pub(crate) bars: Vec<BarWin>,
    pub(crate) popup: Option<PopupWin>,
    pub(crate) popup_message: Option<String>,
    pub(crate) stats: Stats,
    pub(crate) hooks: Vec<HWINEVENTHOOK>,
    pub(crate) name_hook: Option<HWINEVENTHOOK>,
    pub(crate) fg_window: Option<isize>,
    pub(crate) prev_foreground: Option<isize>,
    pub(crate) hotkey: bool,
    pub(crate) monitor_signature: Vec<(String, [i32; 4], u32, bool)>,
    pub(crate) suspended: bool,
    pub(crate) studio: Option<crate::studio_win::StudioWin>,
}

pub(crate) fn system_mode(args: &Args) -> Option<Mode> {
    args.mode.or_else(|| {
        appearance::apps_use_light_theme().map(|l| if l { Mode::Light } else { Mode::Dark })
    })
}

pub(crate) fn font_config(style: &PanelStyle, lang: Lang) -> FontConfig {
    FontConfig {
        family: style.family.clone(),
        fallback: style.fallback.clone(),
        icon_family: style.icon_family.to_owned(),
        icon_fallback: style.icon_fallback.to_owned(),
        locale: lang.locale().to_owned(),
    }
}

fn phys(r: &windowsrice_windows::display::PixelRect) -> PhysRect {
    PhysRect {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

fn vk_of(key: HotkeyKey) -> u32 {
    match key {
        HotkeyKey::Char(c) => u32::from(c),
        HotkeyKey::Function(n) => 0x70 + u32::from(n) - 1,
        HotkeyKey::Space => 0x20,
        HotkeyKey::Enter => 0x0D,
        HotkeyKey::Tab => 0x09,
        HotkeyKey::Escape => 0x1B,
    }
}

pub(crate) fn map_key(vk: u16) -> Option<Key> {
    Some(match vk {
        v if v == VK_TAB.0 => Key::Tab,
        v if v == VK_LEFT.0 => Key::Left,
        v if v == VK_RIGHT.0 => Key::Right,
        v if v == VK_UP.0 => Key::Up,
        v if v == VK_DOWN.0 => Key::Down,
        v if v == VK_HOME.0 => Key::Home,
        v if v == VK_END.0 => Key::End,
        v if v == VK_RETURN.0 => Key::Enter,
        v if v == VK_SPACE.0 => Key::Space,
        v if v == VK_ESCAPE.0 => Key::Escape,
        _ => return None,
    })
}

pub(crate) fn modifiers() -> Modifiers {
    // SAFETY: GetKeyState has no preconditions.
    let down = |k: u16| unsafe { GetKeyState(i32::from(k)) } < 0;
    Modifiers {
        shift: down(VK_SHIFT.0),
        ctrl: down(VK_CONTROL.0),
        alt: down(VK_MENU.0),
    }
}

pub(crate) fn notice_text(n: &Notice) -> String {
    format!("{n:?}")
}

impl Controller {
    pub fn new(hwnd: HWND, args: Args, raw: Vec<String>) -> Result<Self, String> {
        let mut stats = Stats {
            version: env!("CARGO_PKG_VERSION"),
            args: raw,
            started: Some(Instant::now()),
            resources_start: process_counters(),
            ..Default::default()
        };
        let mode = system_mode(&args);
        let mut sink = BarSink::default();
        let paths = AppPaths::from_env().ok();
        let config_path = args
            .config
            .clone()
            .or_else(|| paths.as_ref().map(AppPaths::config_file));
        // With --config (tests, portable use) themes live next to that file,
        // so nothing in %APPDATA% is touched.
        let user_themes = args
            .config
            .as_ref()
            .and_then(|c| c.parent())
            .map(|d| d.join("themes"))
            .or_else(|| paths.as_ref().map(AppPaths::user_themes_dir));
        stats.config_path = config_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let service = config_path
            .as_ref()
            .map(|p| ThemeService::start_with(p, user_themes.as_deref(), mode, &mut sink));
        crate::stats::mark("config_loaded");
        let mut notices: Vec<String> = service
            .as_ref()
            .map(|s| s.notices().iter().map(notice_text).collect())
            .unwrap_or_default();
        let theme = match service {
            Some(s) if args.theme.is_none() && args.mode.is_none() => ThemeSource::Service(s),
            other => {
                let config = other
                    .as_ref()
                    .map(|s| s.config().clone())
                    .unwrap_or_default();
                let selection = ThemeSelection {
                    active: args
                        .theme
                        .clone()
                        .unwrap_or_else(|| config.theme.active.clone()),
                    mode: match args.mode {
                        Some(Mode::Dark) => ThemeModePreference::Dark,
                        Some(Mode::Light) => ThemeModePreference::Light,
                        None => config.theme.mode,
                    },
                };
                let (engine, n) = ThemeEngine::start(
                    ThemeCatalog::load(user_themes.as_deref()),
                    &selection,
                    mode,
                    &mut sink,
                );
                notices.extend(n.iter().map(notice_text));
                ThemeSource::Override { engine, config }
            }
        };
        let resolved = theme
            .applied()
            .cloned()
            .ok_or("no theme could be applied")?;
        let config = theme.config().clone();
        let lang = match args.lang.as_deref() {
            Some("en") => Lang::En,
            Some("ko") => Lang::Ko,
            _ => Lang::resolve(config.general.language, appearance::ui_language_is_korean()),
        };
        let style = PanelStyle::from_theme(
            &resolved,
            window::high_contrast().as_ref(),
            window::animations_enabled() && config.bar.animations,
        );
        let plan_ = plan(
            &request(
                &resolved,
                &args,
                true,
                style.high_contrast,
                &config.bar.style,
            ),
            &resolved.palette,
        );
        crate::stats::mark("theme_resolved");
        let scheduler = Scheduler::start(hwnd, &config.bar.modules, !args.no_gpu_counter);
        crate::stats::mark("providers_started");
        let mut c = Controller {
            hwnd,
            args,
            strings: Strings::new(lang),
            theme,
            sink,
            notices,
            style,
            plan: plan_,
            backdrop_ok: true,
            readings: BarReadings::default(),
            scheduler,
            bars: Vec::new(),
            popup: None,
            popup_message: None,
            stats,
            hooks: Vec::new(),
            name_hook: None,
            fg_window: None,
            prev_foreground: None,
            hotkey: false,
            monitor_signature: Vec::new(),
            suspended: false,
            studio: None,
        };
        c.refresh_clock_only();
        c.install_hooks();
        c.register_hotkey();
        c.sync_bars("startup");
        if c.bars.is_empty() {
            return Err("no monitor is configured for a bar".into());
        }
        crate::stats::mark("bars_synced");
        c.on_foreground(foreground::foreground_window());
        c.start_timers();
        crate::stats::mark("controller_ready");
        Ok(c)
    }

    pub(crate) fn config(&self) -> &Config {
        self.theme.config()
    }

    fn start_timers(&self) {
        let set = |id: usize, d: Duration| {
            // SAFETY: timers on our own window.
            unsafe {
                SetTimer(
                    Some(self.hwnd),
                    id,
                    u32::try_from(d.as_millis()).unwrap_or(u32::MAX),
                    None,
                )
            };
        };
        if let Some(d) = self.args.exit_after {
            set(TIMER_EXIT, d);
        }
        if let Some(d) = self.args.simulate_device_loss_after {
            set(TIMER_DEVICE_LOSS, d);
        }
        if let Some(d) = self.args.device_loss_every {
            set(TIMER_DEVICE_LOSS_EVERY, d);
        }
        if let Some((hide, show)) = self.args.suspend_test {
            set(TIMER_SUSPEND_HIDE, hide);
            set(TIMER_SUSPEND_SHOW, show);
        }
    }

    fn register_hotkey(&mut self) {
        if self.args.no_hotkey {
            return;
        }
        let hk = self.config().bar.focus_hotkey;
        let mut m = MOD_NOREPEAT;
        if hk.modifiers.ctrl {
            m |= MOD_CONTROL;
        }
        if hk.modifiers.alt {
            m |= MOD_ALT;
        }
        if hk.modifiers.shift {
            m |= MOD_SHIFT;
        }
        if hk.modifiers.win {
            m |= MOD_WIN;
        }
        // SAFETY: hotkey for our own window; fails rather than overriding.
        self.hotkey = unsafe {
            RegisterHotKey(
                Some(self.hwnd),
                HOTKEY_FOCUS,
                HOT_KEY_MODIFIERS(m.0),
                vk_of(hk.key),
            )
        }
        .is_ok();
        self.stats.hotkey_registered = self.hotkey;
        if !self.hotkey {
            self.notices
                .push(format!("{hk} is already registered by another application"));
        }
    }

    // ------------------------------------------------------------ theme

    pub(crate) fn restyle(&mut self) {
        let Some(resolved) = self
            .theme
            .applied()
            .map(|t| windowsrice_ui::style::with_bar_style(t, &self.config().bar.style))
        else {
            return;
        };
        let style = PanelStyle::from_theme(
            &resolved,
            window::high_contrast().as_ref(),
            window::animations_enabled() && self.config().bar.animations,
        );
        let fonts_changed =
            style.family != self.style.family || style.fallback != self.style.fallback;
        self.style = style;
        let wanted = plan(
            &request(
                &resolved,
                &self.args,
                true,
                self.style.high_contrast,
                &self.config().bar.style,
            ),
            &resolved.palette,
        );
        let dark = resolved.mode == Mode::Dark;
        // The bar never activates, and Windows draws the system backdrop of
        // such windows as a flat fallback tint that hides the theme's colour
        // (Phase 7 finding). Bars therefore use no system backdrop and draw
        // translucency themselves with per-pixel alpha.
        for b in &self.bars {
            window::apply_dwm(b.s.hwnd, Backdrop::None, dark, false);
        }
        let _ = &wanted;
        let ok = true;
        self.backdrop_ok = ok;
        self.plan = plan(
            &request(
                &resolved,
                &self.args,
                ok,
                self.style.high_contrast,
                &self.config().bar.style,
            ),
            &resolved.palette,
        );
        for b in &mut self.bars {
            if fonts_changed
                && let Ok(t) = TextSystem::new(font_config(&self.style, self.strings.lang))
            {
                b.s.renderer.set_text(t);
            }
        }
        if let Some(p) = &mut self.popup {
            window::apply_dwm(p.s.hwnd, self.plan.backdrop, dark, true);
            if fonts_changed
                && let Ok(t) = TextSystem::new(font_config(&self.style, self.strings.lang))
            {
                p.s.renderer.set_text(t);
            }
        }
        self.stats.theme = json!({
            "id": resolved.id, "name": resolved.name, "mode": format!("{:?}", resolved.mode),
            "fallback": self.theme.is_fallback(), "sink_accepted": self.sink.accepted, "notices": self.notices,
        });
        self.stats.surface = json!({
            "backdrop": format!("{:?}", self.plan.backdrop), "reason": format!("{:?}", self.plan.reason),
            "surface_opacity": self.plan.surface_opacity, "backplates": self.plan.backplates,
            "worst_case_text_contrast": self.plan.worst_case_text_contrast, "meets_aa_worst_case": self.plan.meets_aa_worst_case,
            "high_contrast": self.style.high_contrast, "fade_ms": self.style.fade_ms,
        });
    }

    pub(crate) fn warnings(&self) -> Vec<String> {
        let en = self.strings.lang == Lang::En;
        let mut w = Vec::new();
        if self.theme.is_fallback() {
            w.push(
                if en {
                    "selected theme unavailable, showing windowsRICE Default"
                } else {
                    "선택한 테마를 사용할 수 없어 windowsRICE Default를 표시합니다"
                }
                .to_owned(),
            );
        }
        if !self.plan.meets_aa_worst_case {
            w.push(if en { "text contrast over the translucent surface may be below WCAG AA (text backplates are off)" } else { "반투명 표면 위 텍스트 대비가 WCAG AA보다 낮을 수 있습니다 (텍스트 배경판 꺼짐)" }.to_owned());
        }
        if self.plan.reason == SurfaceReason::BackdropUnavailable {
            w.push(
                if en {
                    "the system backdrop is unavailable; a solid surface is used"
                } else {
                    "시스템 배경 효과를 사용할 수 없어 불투명 표면을 사용합니다"
                }
                .to_owned(),
            );
        }
        if !self.args.no_hotkey && !self.hotkey {
            let hk = self.config().bar.focus_hotkey;
            w.push(if en {
                format!("{hk} is used by another application")
            } else {
                format!("{hk}을(를) 다른 응용 프로그램이 사용 중입니다")
            });
        }
        w
    }

    // ------------------------------------------------------------ monitors and bars

    /// Creates, updates and removes bars to match monitors and settings.
    pub fn sync_bars(&mut self, why: &str) {
        // Startup milestones are recorded until the first frame.
        let startup = self.stats.ms_process_to_first_frame.is_none();
        let mut all = monitors::enumerate();
        if startup {
            crate::stats::mark("monitors_enumerated");
        }
        monitors::identify(&mut all);
        if startup {
            crate::stats::mark("monitor_identity_resolved");
        }
        let bar_cfg = self.config().bar.clone();
        let connected: Vec<Option<&str>> = all.iter().map(|m| m.stable_id.as_deref()).collect();
        let desired: Vec<monitors::MonitorInfo> = all
            .iter()
            .enumerate()
            .filter(|(i, m)| match self.args.monitor {
                Some(only) => *i == only,
                None => bar_cfg.monitor_enabled_among(
                    &m.device_name,
                    m.stable_id.as_deref(),
                    m.primary,
                    &connected,
                ),
            })
            .map(|(_, m)| m.clone())
            .collect();
        self.monitor_signature = all
            .iter()
            .map(|m| {
                (
                    m.device_name.clone(),
                    [m.rect.left, m.rect.top, m.rect.right, m.rect.bottom],
                    m.dpi,
                    m.primary,
                )
            })
            .collect();
        // Remove bars whose monitor is gone or no longer configured.
        let mut i = 0;
        while i < self.bars.len() {
            if desired.iter().any(|m| m.device_name == self.bars[i].device) {
                i += 1;
            } else {
                let b = self.bars.remove(i);
                self.destroy_bar(b, why);
            }
        }
        for m in desired {
            match self.bars.iter().position(|b| b.device == m.device_name) {
                Some(i) => {
                    self.bars[i].monitor = m;
                    self.layout_bar(i, why);
                }
                None => {
                    if let Err(e) = self.create_bar(m, why) {
                        self.stats.errors.push(e);
                    }
                }
            }
        }
        self.stats.monitor = json!(
            self.monitor_signature
                .iter()
                .map(|(d, r, dpi, p)| json!({"device": d, "rect": r, "dpi": dpi, "primary": p}))
                .collect::<Vec<_>>()
        );
    }

    fn create_bar(&mut self, m: monitors::MonitorInfo, why: &str) -> Result<(), String> {
        let hwnd = window::create(
            CLASS_BAR,
            w!("windowsRICE bar"),
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            None,
        )
        .map_err(|e| format!("bar window: {e}"))?;
        let startup = self.stats.ms_process_to_first_frame.is_none();
        if startup {
            crate::stats::mark("bar_window_created");
        }
        let text = TextSystem::new(font_config(&self.style, self.strings.lang))
            .map_err(|e| format!("DirectWrite: {e}"))?;
        if startup {
            crate::stats::mark("text_system_ready");
        }
        let placeholder = Scene {
            tree: windowsrice_ui::UiTree::new(windowsrice_ui::tree::Element::new(
                ElementId(1),
                "bar",
                windowsrice_ui::Role::ToolBar,
                "windowsRICE bar",
                windowsrice_ui::Rect::new(0.0, 0.0, 1.0, 1.0),
            )),
            draw: Default::default(),
            size: Size { w: 1.0, h: 1.0 },
            scale: 1.0,
        };
        let s = Surface::new(hwnd, text, "windowsRICE.Bar", placeholder)?;
        if startup {
            crate::stats::mark("graphics_device_ready");
        }
        let dark = self.theme.applied().is_some_and(|t| t.mode == Mode::Dark);
        window::apply_dwm(hwnd, Backdrop::None, dark, false);
        self.bars.push(BarWin {
            s,
            device: m.device_name.clone(),
            monitor: m,
            rect: PhysRect::default(),
            appbar: false,
            collapsed: false,
            yielded: false,
            hidden_modules: Vec::new(),
        });
        let i = self.bars.len() - 1;
        self.stats.display_events.push(format!(
            "{:.2}s {why}: bar created on {}",
            self.stats.now_s(),
            self.bars[i].device
        ));
        self.layout_bar(i, why);
        if startup {
            crate::stats::mark("initial_layout_built");
        }
        Ok(())
    }

    fn destroy_bar(&mut self, b: BarWin, why: &str) {
        if self.popup.as_ref().is_some_and(|p| p.owner == b.s.hwnd) {
            self.close_popup_now();
        }
        if b.appbar {
            appbar::remove(b.s.hwnd);
        }
        b.s.release_uia();
        let hwnd = b.s.hwnd;
        self.stats
            .bars_destroyed
            .push(format!("{:.2}s {why}: {}", self.stats.now_s(), b.device));
        drop(b);
        // SAFETY: destroying our own bar window.
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }

    fn bar_height(&self, i: usize) -> f32 {
        let m = &self.bars[i].monitor;
        let connected: Vec<Option<&str>> = self
            .bars
            .iter()
            .map(|b| b.monitor.stable_id.as_deref())
            .collect();
        self.config()
            .bar
            .height_for_among(&m.device_name, m.stable_id.as_deref(), &connected)
            .map_or(self.style.height, f32::from)
    }

    /// Positions bar `i` on its monitor, negotiating the work area.
    pub(crate) fn layout_bar(&mut self, i: usize, why: &str) {
        let reserve = self.config().bar.reserve_work_area && !self.config().bar.auto_hide;
        let auto_hide = self.config().bar.auto_hide;
        let h_dip = self.bar_height(i);
        let b = &mut self.bars[i];
        let scale = self.args.scale.unwrap_or(b.monitor.dpi as f32 / 96.0);
        let dpi = (scale * 96.0).round() as u32;
        let monitor = phys(&b.monitor.rect);
        let mut rect = bar_rect(monitor, scale, h_dip);
        if reserve {
            if !b.appbar {
                b.appbar = appbar::register(b.s.hwnd);
                self.stats.appbar_events.push(format!(
                    "{:.2}s register {}: {}",
                    self.stats.now_s(),
                    b.device,
                    b.appbar
                ));
            }
            if self.stats.ms_process_to_first_frame.is_none() {
                crate::stats::mark("appbar_registered");
            }
            if b.appbar {
                let granted = appbar::set_pos(b.s.hwnd, monitor, rect.height());
                rect = granted;
                if self.stats.ms_process_to_first_frame.is_none() {
                    crate::stats::mark("appbar_positioned");
                }
            }
        } else if b.appbar {
            appbar::remove(b.s.hwnd);
            b.appbar = false;
            self.stats.appbar_events.push(format!(
                "{:.2}s removed {} ({why})",
                self.stats.now_s(),
                b.device
            ));
        }
        if !auto_hide {
            b.collapsed = false;
        }
        let changed = rect != b.rect || dpi != b.s.dpi;
        b.rect = rect;
        b.s.scale = scale;
        b.s.dpi = dpi;
        let shown = if b.collapsed {
            collapsed_rect(rect, COLLAPSED_PX)
        } else {
            rect
        };
        window::place(b.s.hwnd, shown);
        let startup = self.stats.ms_process_to_first_frame.is_none();
        if startup {
            crate::stats::mark("bar_window_placed");
        }
        if let Err(e) =
            b.s.renderer
                .resize(rect.width().max(1) as u32, rect.height().max(1) as u32, dpi)
        {
            self.stats.errors.push(format!("resize: {e}"));
        }
        if startup {
            crate::stats::mark("swap_chain_sized");
        }
        if changed {
            // SAFETY: reads the monitor's work area after the reservation.
            let work = monitors::monitor_info(b.monitor.handle)
                .map(|m| [m.work.left, m.work.top, m.work.right, m.work.bottom]);
            self.stats.bars.push(json!({
                "at_s": self.stats.now_s(), "why": why, "device": b.device, "monitor_rect": [monitor.left, monitor.top, monitor.right, monitor.bottom],
                "window_rect": [rect.left, rect.top, rect.right, rect.bottom], "dpi": dpi, "scale": scale, "scale_forced": self.args.scale.is_some(),
                "appbar": b.appbar, "work_area_after": work, "auto_hide": auto_hide,
            }));
        }
        self.rebuild_bar(i, "layout");
    }

    // ------------------------------------------------------------ scenes

    fn bar_model(&self, i: usize) -> BarModel {
        let b = &self.bars[i];
        let mut readings = self.readings.clone();
        readings.warnings = self.warnings();
        BarModel {
            modules: self.config().bar.modules.clone(),
            readings,
            focus: b.s.focus,
            keyboard_active: b.s.keyboard_active,
            hover: b.s.hover,
            popup_open: self.popup.as_ref().is_some_and(|p| p.owner == b.s.hwnd),
            decor: windowsrice_ui::bar::Decor::from_style(&self.config().bar.style),
        }
    }

    fn rebuild_bar(&mut self, i: usize, reason: &'static str) {
        if self.bars[i].s.shown_once
            && (!self.bars[i].s.visible || self.bars[i].collapsed)
            && reason != "layout"
        {
            // Hidden or collapsed: no layout work; one rebuild on expand/resume.
            return;
        }
        let model = self.bar_model(i);
        let b = &self.bars[i];
        let size = Size {
            w: b.rect.width().max(1) as f32 / b.s.scale,
            h: b.rect.height().max(1) as f32 / b.s.scale,
        };
        let bs = build_bar(
            &model,
            &self.style,
            &self.plan,
            b.s.renderer.text(),
            size,
            b.s.scale,
            &self.strings,
            f32::from(self.config().bar.spacing),
        );
        let origin = (b.rect.left, b.rect.top);
        let b = &mut self.bars[i];
        if bs.hidden != b.hidden_modules {
            b.hidden_modules = bs.hidden.clone();
            self.stats.hidden_modules = bs.hidden.clone();
        }
        let problems = bs.scene.tree.problems();
        if !problems.is_empty() {
            self.stats.a11y_problems = problems;
        }
        self.stats.uia_events_raised += u64::from(b.s.set_scene(bs.scene, origin, reason));
    }

    pub(crate) fn rebuild_all(&mut self, reason: &'static str) {
        for i in 0..self.bars.len() {
            self.rebuild_bar(i, reason);
        }
        if self.popup.is_some() {
            self.rebuild_popup();
        }
    }

    fn popup_model(&self) -> PopupModel {
        let mut status = HashMap::new();
        for m in &self.config().bar.modules {
            let st = match m.kind {
                ModuleKind::Cpu => reading_status(&self.readings.cpu),
                ModuleKind::Gpu => reading_status(&self.readings.gpu),
                ModuleKind::Vram => reading_status(&self.readings.vram),
                ModuleKind::Ram => reading_status(&self.readings.ram),
                ModuleKind::Volume => reading_status(&self.readings.audio),
                ModuleKind::Network => reading_status(&self.readings.network),
                ModuleKind::Upload | ModuleKind::Download => reading_status(&self.readings.traffic),
                ModuleKind::Battery => reading_status(&self.readings.battery),
                ModuleKind::Notifications => reading_status(&self.readings.notifications),
                _ => ModuleStatus::Ok,
            };
            status.insert(m.id.clone(), st);
        }
        let bar = &self.config().bar;
        let p = self.popup.as_ref();
        PopupModel {
            themes: self
                .theme
                .catalog()
                .entries()
                .iter()
                .map(|e| (e.id().to_owned(), e.theme.meta.name.clone()))
                .collect(),
            active_theme: self
                .theme
                .applied()
                .map(|t| t.id.clone())
                .unwrap_or_default(),
            modules: bar.modules.clone(),
            status,
            height: bar.height.unwrap_or(self.style.height as u16),
            auto_hide: bar.auto_hide,
            reserve: bar.reserve_work_area,
            all_monitors: bar.monitors == windowsrice_core::bar::MonitorPlacement::All,
            message: self.popup_message.clone(),
            focus: p.and_then(|p| p.s.focus),
            keyboard_active: p.is_some_and(|p| p.s.keyboard_active),
            hover: p.and_then(|p| p.s.hover),
        }
    }

    fn rebuild_popup(&mut self) {
        let model = self.popup_model();
        let Some(p) = self.popup.as_mut() else { return };
        let ps = build_popup(
            &model,
            &self.style,
            &self.plan,
            p.s.renderer.text(),
            p.s.scale,
            &self.strings,
        );
        p.actions = ps.actions;
        let problems = ps.scene.tree.problems();
        if !problems.is_empty() {
            self.stats.a11y_problems = problems;
        }
        let h = (ps.scene.size.h * p.s.scale).round() as i32;
        if h != p.size.height() {
            p.size.bottom = p.size.top + h;
            window::place(p.s.hwnd, p.size);
            let _ =
                p.s.renderer
                    .resize(p.size.width().max(1) as u32, h.max(1) as u32, p.s.dpi);
        }
        self.stats.uia_events_raised += u64::from(p.s.set_scene(ps.scene, p.origin, "popup"));
    }

    // ------------------------------------------------------------ rendering

    /// Draws every surface that needs a frame.
    pub fn render_all(&mut self) {
        let before_first = self.stats.ms_process_to_first_frame.is_none();
        self.render_studio();
        for i in 0..self.bars.len() {
            let first = !self.bars[i].s.shown_once;
            let visible = self.bars[i].s.visible && !self.bars[i].collapsed;
            if first && self.stats.ms_process_to_first_frame.is_none() {
                crate::stats::mark("first_draw_started");
            }
            if self.bars[i].s.dirty
                && (visible || first)
                && Self::render_surface(&mut self.bars[i].s, &mut self.stats)
                && first
            {
                self.bars[i].s.shown_once = true;
                self.show_bar(i);
                if self.stats.ms_process_to_first_frame.is_none() {
                    crate::stats::mark("first_frame_presented");
                    self.stats.ms_process_to_first_frame = ms_since_process_start();
                }
            }
        }
        if before_first && self.stats.ms_process_to_first_frame.is_some() {
            // Deferred from startup: includes the shell's fullscreen state.
            self.check_fullscreen();
        }
        if let Some(p) = &mut self.popup
            && p.s.dirty
            && Self::render_surface(&mut p.s, &mut self.stats)
            && !p.s.shown_once
        {
            p.s.shown_once = true;
            p.s.visible = true;
            // SAFETY: shows our own popup (activation is handled separately).
            unsafe {
                let _ = ShowWindow(p.s.hwnd, SW_SHOWNOACTIVATE);
            }
            let _ = p.s.renderer.fade_in(self.style.fade_ms);
        }
    }

    fn render_studio(&mut self) {
        let Some(st) = &mut self.studio else { return };
        if st.s.dirty {
            Self::render_surface(&mut st.s, &mut self.stats);
        }
    }

    /// Returns true when a frame was presented.
    pub(crate) fn render_surface(s: &mut Surface, stats: &mut Stats) -> bool {
        let result = {
            let st = s.host.state.borrow();
            s.renderer.render(&st.scene.draw)
        };
        match result {
            Ok(f) => {
                s.dirty = false;
                stats.frames.push(stats.now_s());
                stats.frame_reasons.push(s.reason);
                stats.frame_draw_ms.push(f.draw.as_secs_f64() * 1000.0);
                stats
                    .frame_present_ms
                    .push(f.present.as_secs_f64() * 1000.0);
                if let Some(t) = s.pending_input.take() {
                    stats
                        .input_latency_ms
                        .push(t.elapsed().as_secs_f64() * 1000.0);
                }
                true
            }
            Err(RenderError::DeviceLost(why)) => {
                stats
                    .errors
                    .push(format!("{:.2}s device lost: {why}", stats.now_s()));
                match s.renderer.recover() {
                    Ok(()) => {
                        stats.device_recoveries += 1;
                        stats.resources_after_recovery.push(process_counters());
                        s.dirty = true;
                        s.reason = "device-recovered";
                        Self::render_surface(s, stats)
                    }
                    Err(e) => {
                        stats.recovery_failures += 1;
                        stats.errors.push(format!("recovery failed: {e}"));
                        s.dirty = false;
                        false
                    }
                }
            }
            Err(RenderError::Windows(e)) => {
                stats
                    .errors
                    .push(format!("{:.2}s render error: {e}", stats.now_s()));
                s.dirty = false;
                false
            }
        }
    }

    fn show_bar(&mut self, i: usize) {
        let b = &mut self.bars[i];
        b.s.visible = true;
        b.s.host.state.borrow_mut().visible = true;
        // SAFETY: shows our own window without activating it.
        unsafe {
            let _ = ShowWindow(b.s.hwnd, SW_SHOWNOACTIVATE);
        }
        let shown = if b.collapsed {
            collapsed_rect(b.rect, COLLAPSED_PX)
        } else {
            b.rect
        };
        window::place(b.s.hwnd, shown);
        if self.stats.ms_process_to_window.is_none() {
            self.stats.ms_process_to_window = ms_since_process_start();
        }
        if let Err(e) = b.s.renderer.fade_in(self.style.fade_ms) {
            self.stats.errors.push(format!("fade: {e}"));
        }
    }

    fn hide_bar(&mut self, i: usize, why: &str) {
        let b = &mut self.bars[i];
        if !b.s.visible {
            return;
        }
        b.s.visible = false;
        b.s.host.state.borrow_mut().visible = false;
        // SAFETY: hides our own window.
        unsafe {
            let _ = ShowWindow(b.s.hwnd, SW_HIDE);
        }
        b.s.renderer.trim();
        self.stats.suspend_log.push(format!(
            "{:.2}s {} hidden ({why})",
            self.stats.now_s(),
            b.device
        ));
        self.update_pause();
    }

    fn resume_bar(&mut self, i: usize, why: &str) {
        if self.bars[i].s.visible || !self.bars[i].s.shown_once {
            return;
        }
        self.bars[i].s.visible = true;
        self.update_pause();
        self.rebuild_bar(i, "resume");
        self.bars[i].s.dirty = true;
        Self::render_surface(&mut self.bars[i].s, &mut self.stats);
        self.show_bar(i);
        self.stats.suspend_log.push(format!(
            "{:.2}s {} shown ({why})",
            self.stats.now_s(),
            self.bars[i].device
        ));
    }

    /// Pauses the scheduler when no bar is visible.
    pub(crate) fn update_pause(&mut self) {
        let any = self.bars.iter().any(|b| b.s.visible && !b.collapsed) || self.popup.is_some();
        self.scheduler.set_paused(!any);
    }

    // ------------------------------------------------------------ data

    fn refresh_clock_only(&mut self) {
        let t = clock::local_time();
        self.readings.time = Some(LocalTime {
            year: t.year,
            month: t.month,
            day: t.day,
            weekday: t.weekday,
            hour: t.hour,
            minute: t.minute,
        });
        // SAFETY: timer on our own window.
        unsafe {
            SetTimer(
                Some(self.hwnd),
                TIMER_CLOCK,
                u32::try_from(clock::until_next_minute(&t).as_millis()).unwrap_or(60_000),
                None,
            )
        };
    }

    pub fn on_readings(&mut self, _source: usize) {
        let fresh = self.scheduler.readings();
        let r = &mut self.readings;
        r.cpu = fresh.cpu;
        r.gpu = fresh.gpu;
        r.vram = fresh.vram;
        r.ram = fresh.ram;
        r.audio = fresh.audio;
        r.network = fresh.network;
        r.traffic = fresh.traffic;
        r.battery = fresh.battery;
        r.notifications = fresh.notifications;
        self.stats.samples += 1;
        self.rebuild_all("sample");
        if self.studio.is_some() {
            self.studio_rebuild();
        }
        self.check_fullscreen();
    }

    fn install_hooks(&mut self) {
        // SAFETY: out-of-context WinEvent hook; callbacks arrive through our
        // message loop and nothing is injected into other processes.
        unsafe {
            let h = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(win_event),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );
            if !h.is_invalid() {
                self.hooks.push(h);
            }
        }
    }

    fn rehook_foreground_process(&mut self, pid: u32) {
        if let Some(h) = self.name_hook.take() {
            // SAFETY: unhooking our own hook.
            unsafe {
                let _ = UnhookWinEvent(h);
            }
        }
        if pid == 0 || pid == std::process::id() {
            return;
        }
        // SAFETY: as in `install_hooks`, filtered to one process.
        let h = unsafe {
            SetWinEventHook(
                EVENT_OBJECT_LOCATIONCHANGE,
                EVENT_OBJECT_NAMECHANGE,
                None,
                Some(win_event),
                pid,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
        };
        if !h.is_invalid() {
            self.name_hook = Some(h);
        }
    }

    fn own_window(&self, h: HWND) -> bool {
        self.studio.as_ref().is_some_and(|s| s.s.hwnd == h)
            || h == self.hwnd
            || self.bars.iter().any(|b| b.s.hwnd == h)
            || self.popup.as_ref().is_some_and(|p| p.s.hwnd == h)
    }

    pub fn on_foreground(&mut self, hwnd: Option<HWND>) {
        let Some(h) = hwnd else { return };
        if self.own_window(h) {
            return;
        }
        let info = foreground::window_info(h);
        let startup = self.stats.ms_process_to_first_frame.is_none();
        if startup {
            crate::stats::mark("fg_window_info");
        }
        let title = if foreground::is_shell_class(&info.class_name) {
            None
        } else if info.title.trim().is_empty() {
            info.process_name.clone()
        } else {
            Some(info.title.clone())
        };
        if self.fg_window != Some(h.0 as isize) {
            self.fg_window = Some(h.0 as isize);
            self.rehook_foreground_process(info.pid);
            if startup {
                crate::stats::mark("fg_rehooked");
            }
        }
        let title = self.args.title.clone().or(title);
        if title != self.readings.active_window {
            self.readings.active_window = title;
            self.rebuild_all("foreground");
        }
        if startup {
            crate::stats::mark("fg_rebuilt");
        }
        self.check_fullscreen();
        if startup {
            crate::stats::mark("fullscreen_checked");
        }
    }

    pub fn on_object_event(&mut self, event: u32, hwnd: HWND, id_object: i32) {
        if id_object != OBJID_WINDOW.0 || Some(hwnd.0 as isize) != self.fg_window {
            return;
        }
        if event == EVENT_OBJECT_NAMECHANGE {
            self.on_foreground(Some(hwnd));
        } else if event == EVENT_OBJECT_LOCATIONCHANGE {
            // SAFETY: debounce timer on our own window.
            unsafe { SetTimer(Some(self.hwnd), TIMER_FULLSCREEN, 150, None) };
        }
    }

    /// Hides each bar whose monitor is covered by a fullscreen window (when
    /// the policy says so) and shows it again afterwards.
    pub fn check_fullscreen(&mut self) {
        // SAFETY: killing our own timer.
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_FULLSCREEN);
        }
        if self.suspended
            || self.config().bar.fullscreen == windowsrice_core::bar::FullscreenPolicy::Keep
        {
            return;
        }
        let fg = foreground::foreground_window().filter(|h| !self.own_window(*h));
        // The first shell notification-state query in a process takes
        // ~35 ms (Phase 11 startup profile). Before the first frame only the
        // geometric test runs (it catches fullscreen games); the full check
        // runs right after the first frame is presented.
        let shell_query = self.stats.ms_process_to_first_frame.is_some();
        for i in 0..self.bars.len() {
            let covered = fg.is_some_and(|h| {
                // SAFETY: read-only monitor lookup.
                let same = unsafe { MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST) }
                    == self.bars[i].monitor.handle;
                let info = foreground::window_info(h);
                same && !foreground::is_shell_class(&info.class_name)
                    && (foreground::covers_monitor(h, &self.bars[i].monitor.rect)
                        || (shell_query && foreground::fullscreen_app_reported()))
            });
            match (covered, self.bars[i].yielded) {
                (true, false) => {
                    self.bars[i].yielded = true;
                    self.hide_bar(i, "fullscreen application");
                }
                (false, true) => {
                    self.bars[i].yielded = false;
                    self.resume_bar(i, "fullscreen application closed");
                }
                _ => {}
            }
        }
    }

    // ------------------------------------------------------------ input (bars)

    fn bar_index(&self, hwnd: HWND) -> Option<usize> {
        self.bars.iter().position(|b| b.s.hwnd == hwnd)
    }

    fn set_bar_focus(&mut self, i: usize, id: ElementId) {
        if self.bars[i].s.focus == Some(id) {
            return;
        }
        self.bars[i].s.focus = Some(id);
        self.rebuild_bar(i, "focus");
        if self.bars[i].s.keyboard_active && uia::raise_focus(&self.bars[i].s.host, id) {
            self.stats.uia_focus_events += 1;
        }
    }

    pub fn on_focus(&mut self, hwnd: HWND, gained: bool) -> Deferred {
        if let Some(i) = self.bar_index(hwnd) {
            let s = &mut self.bars[i].s;
            s.keyboard_active = gained;
            if gained && s.focus.is_none() {
                s.focus = s
                    .host
                    .state
                    .borrow()
                    .scene
                    .tree
                    .focus_order()
                    .first()
                    .copied();
            }
            self.stats.focus_log.push(format!(
                "{:.2}s bar {} focus {}",
                self.stats.now_s(),
                self.bars[i].device,
                if gained { "gained" } else { "lost" }
            ));
            if gained && self.bars[i].collapsed {
                self.expand_bar(i);
            }
            self.rebuild_bar(i, "focus");
            if gained
                && let Some(f) = self.bars[i].s.focus
                && uia::raise_focus(&self.bars[i].s.host, f)
            {
                self.stats.uia_focus_events += 1;
            }
            if !gained {
                self.schedule_collapse(i);
            }
        } else if let Some(p) = self.popup.as_mut().filter(|p| p.s.hwnd == hwnd) {
            p.s.keyboard_active = gained;
            if gained && p.s.focus.is_none() {
                let st = p.s.host.state.borrow();
                let t = &st.scene.tree;
                p.s.focus = t
                    .iter()
                    .find(|e| e.selected == Some(true))
                    .map(|e| e.id)
                    .or_else(|| t.focus_order().first().copied());
            }
            self.stats.popup_log.push(format!(
                "{:.2}s popup focus {}",
                self.stats.now_s(),
                if gained { "gained" } else { "lost" }
            ));
            self.rebuild_popup();
            if gained
                && let Some(p) = &self.popup
                && let Some(f) = p.s.focus
                && uia::raise_focus(&p.s.host, f)
            {
                self.stats.uia_focus_events += 1;
            }
        }
        Deferred::Nothing
    }

    pub fn on_key(&mut self, hwnd: HWND, vk: u16) -> Deferred {
        let Some(key) = map_key(vk) else {
            return Deferred::Nothing;
        };
        let mods = modifiers();
        if let Some(i) = self.bar_index(hwnd) {
            self.bars[i].s.pending_input = Some(Instant::now());
            let outcome = {
                let st = self.bars[i].s.host.state.borrow();
                handle_key_oriented(
                    &st.scene.tree,
                    self.bars[i].s.focus,
                    key,
                    mods,
                    Orientation::Horizontal,
                )
            };
            return match outcome {
                KeyOutcome::Focus(id) => {
                    self.set_bar_focus(i, id);
                    Deferred::Nothing
                }
                KeyOutcome::Invoke(id) | KeyOutcome::Expand(id) => self.activate_bar_element(i, id),
                KeyOutcome::Dismiss => Deferred::Dismiss(self.prev_foreground),
                _ => {
                    self.bars[i].s.pending_input = None;
                    Deferred::Nothing
                }
            };
        }
        if self.popup.as_ref().is_some_and(|p| p.s.hwnd == hwnd) {
            let outcome = {
                let p = self.popup.as_ref().expect("checked");
                let st = p.s.host.state.borrow();
                handle_key_oriented(&st.scene.tree, p.s.focus, key, mods, Orientation::Vertical)
            };
            if let Some(p) = &mut self.popup {
                p.s.pending_input = Some(Instant::now());
            }
            return match outcome {
                KeyOutcome::Focus(id) => {
                    self.set_popup_focus(id);
                    Deferred::Nothing
                }
                KeyOutcome::Invoke(id)
                | KeyOutcome::Toggle(id)
                | KeyOutcome::Select(id)
                | KeyOutcome::Expand(id) => self.popup_activate(id),
                KeyOutcome::Dismiss => self.close_popup(true),
                KeyOutcome::Ignored => Deferred::Nothing,
            };
        }
        Deferred::Nothing
    }

    fn activate_bar_element(&mut self, i: usize, id: ElementId) -> Deferred {
        if Some(id) != menu_element(&self.config().bar.modules) {
            return Deferred::Nothing;
        }
        self.bars[i].s.focus = Some(id);
        if self
            .popup
            .as_ref()
            .is_some_and(|p| p.owner == self.bars[i].s.hwnd)
        {
            self.close_popup(false)
        } else {
            Deferred::OpenPopup(self.bars[i].s.hwnd.0 as isize)
        }
    }

    pub fn on_mouse_move(&mut self, hwnd: HWND, x: i32, y: i32) {
        if let Some(i) = self.bar_index(hwnd) {
            if self.bars[i].collapsed {
                self.expand_bar(i);
            }
            let hit = self.bars[i]
                .s
                .tree_hit(x, y)
                .filter(|h| Some(*h) == menu_element(&self.config().bar.modules));
            if hit != self.bars[i].s.hover {
                self.bars[i].s.pending_input = Some(Instant::now());
                self.bars[i].s.hover = hit;
                self.rebuild_bar(i, "hover");
            }
            track_leave(hwnd);
        } else if let Some(p) = self.popup.as_mut().filter(|p| p.s.hwnd == hwnd) {
            let hit =
                p.s.tree_hit(x, y)
                    .filter(|h| p.actions.iter().any(|(e, _)| e == h));
            if hit != p.s.hover {
                p.s.hover = hit;
                self.rebuild_popup();
            }
            track_leave(hwnd);
        }
    }

    pub fn on_mouse_leave(&mut self, hwnd: HWND) {
        if let Some(i) = self.bar_index(hwnd) {
            if self.bars[i].s.hover.take().is_some() {
                self.rebuild_bar(i, "hover");
            }
            self.schedule_collapse(i);
        } else if let Some(p) = self.popup.as_mut().filter(|p| p.s.hwnd == hwnd)
            && p.s.hover.take().is_some()
        {
            self.rebuild_popup();
        }
    }

    pub fn on_click(&mut self, hwnd: HWND, x: i32, y: i32) -> Deferred {
        if let Some(i) = self.bar_index(hwnd) {
            let hit = self.bars[i].s.tree_hit(x, y);
            self.bars[i].s.pending_input = Some(Instant::now());
            return match hit {
                Some(id) if Some(id) == menu_element(&self.config().bar.modules) => {
                    self.activate_bar_element(i, id)
                }
                _ => Deferred::Nothing,
            };
        }
        if let Some(p) = self.popup.as_mut().filter(|p| p.s.hwnd == hwnd) {
            p.s.pending_input = Some(Instant::now());
            let hit = p.s.tree_hit(x, y);
            if let Some(id) = hit.filter(|h| p.actions.iter().any(|(e, _)| e == h)) {
                p.s.focus = Some(id);
                return self.popup_activate(id);
            }
        }
        Deferred::Nothing
    }

    pub fn on_uia_action(&mut self, hwnd: HWND, action: UiaAction, id: ElementId) -> Deferred {
        if let Some(i) = self.bar_index(hwnd) {
            self.bars[i].s.pending_input = Some(Instant::now());
            return match action {
                UiaAction::Focus => {
                    self.set_bar_focus(i, id);
                    if self.bars[i].s.keyboard_active {
                        Deferred::Nothing
                    } else {
                        Deferred::Activate(hwnd.0 as isize)
                    }
                }
                UiaAction::Invoke | UiaAction::Expand => self.activate_bar_element(i, id),
                UiaAction::Collapse => self.close_popup(false),
                _ => Deferred::Nothing,
            };
        }
        if self.popup.as_ref().is_some_and(|p| p.s.hwnd == hwnd) {
            return match action {
                UiaAction::Focus => {
                    self.set_popup_focus(id);
                    Deferred::Nothing
                }
                _ => self.popup_activate(id),
            };
        }
        Deferred::Nothing
    }

    // ------------------------------------------------------------ auto-hide

    fn expand_bar(&mut self, i: usize) {
        let b = &mut self.bars[i];
        if !b.collapsed {
            return;
        }
        b.collapsed = false;
        window::place(b.s.hwnd, b.rect);
        self.stats.autohide_events.push(format!(
            "{:.2}s {} expanded",
            self.stats.now_s(),
            b.device
        ));
        self.update_pause();
        self.rebuild_bar(i, "expand");
        self.bars[i].s.dirty = true;
        let _ = self.bars[i].s.renderer.fade_in(self.style.fade_ms);
    }

    fn schedule_collapse(&self, i: usize) {
        if !self.config().bar.auto_hide {
            return;
        }
        // SAFETY: timer on our own bar window.
        unsafe {
            SetTimer(
                Some(self.bars[i].s.hwnd),
                TIMER_AUTOHIDE,
                self.config().bar.auto_hide_delay_ms,
                None,
            )
        };
    }

    fn collapse_bar(&mut self, i: usize) {
        // SAFETY: one-shot timer on our own window.
        unsafe {
            let _ = KillTimer(Some(self.bars[i].s.hwnd), TIMER_AUTOHIDE);
        }
        let b = &self.bars[i];
        if !self.config().bar.auto_hide
            || b.collapsed
            || b.s.keyboard_active
            || self.popup.as_ref().is_some_and(|p| p.owner == b.s.hwnd)
        {
            return;
        }
        let mut pt = POINT::default();
        // SAFETY: reads the cursor position.
        let over = unsafe { GetCursorPos(&raw mut pt) }.is_ok()
            && pt.x >= b.rect.left
            && pt.x < b.rect.right
            && pt.y >= b.rect.top
            && pt.y < b.rect.bottom;
        if over {
            self.schedule_collapse(i);
            return;
        }
        let b = &mut self.bars[i];
        b.collapsed = true;
        window::place(b.s.hwnd, collapsed_rect(b.rect, COLLAPSED_PX));
        self.stats.autohide_events.push(format!(
            "{:.2}s {} collapsed",
            self.stats.now_s(),
            b.device
        ));
        self.update_pause();
    }

    pub fn on_bar_timer(&mut self, hwnd: HWND, id: usize) {
        if id == TIMER_AUTOHIDE
            && let Some(i) = self.bar_index(hwnd)
        {
            self.collapse_bar(i);
        }
    }

    // ------------------------------------------------------------ popup

    /// Creates the settings popup below the menu of bar `owner`.
    pub fn open_popup(&mut self, owner: isize) -> Deferred {
        let Some(i) = self.bars.iter().position(|b| b.s.hwnd.0 as isize == owner) else {
            return Deferred::Nothing;
        };
        if self.popup.is_some() {
            self.close_popup_now();
        }
        let owner_hwnd = self.bars[i].s.hwnd;
        let hwnd = match window::create(
            CLASS_POPUP,
            w!("windowsRICE settings"),
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            Some(owner_hwnd),
        ) {
            Ok(h) => h,
            Err(e) => {
                self.stats.errors.push(format!("popup window: {e}"));
                return Deferred::Nothing;
            }
        };
        let text = match TextSystem::new(font_config(&self.style, self.strings.lang)) {
            Ok(t) => t,
            Err(e) => {
                self.stats.errors.push(format!("popup DirectWrite: {e}"));
                return Deferred::Nothing;
            }
        };
        let placeholder = self.bars[i].s.host.state.borrow().scene.clone();
        let mut s = match Surface::new(hwnd, text, "windowsRICE.Popup", placeholder) {
            Ok(s) => s,
            Err(e) => {
                self.stats.errors.push(e);
                return Deferred::Nothing;
            }
        };
        let b = &self.bars[i];
        s.scale = b.s.scale;
        s.dpi = b.s.dpi;
        let menu = menu_element(&self.config().bar.modules)
            .and_then(|m| b.s.host.state.borrow().scene.tree.get(m).map(|e| e.bounds));
        let left = menu.map_or(b.rect.left, |r| {
            r.to_physical(b.s.scale, (b.rect.left, b.rect.top)).left
        });
        let work = monitors::monitor_info(b.monitor.handle)
            .map_or(phys(&b.monitor.rect), |m| phys(&m.work));
        let width = (POPUP_WIDTH * s.scale).round() as i32;
        let x = left.min(work.right - width).max(work.left);
        let y = b.rect.bottom + (4.0 * s.scale).round() as i32;
        let dark = self.theme.applied().is_some_and(|t| t.mode == Mode::Dark);
        window::apply_dwm(hwnd, self.plan.backdrop, dark, true);
        self.popup = Some(PopupWin {
            s,
            owner: owner_hwnd,
            origin: (x, y),
            size: PhysRect {
                left: x,
                top: y,
                right: x + width,
                bottom: y + 1,
            },
            actions: Vec::new(),
        });
        self.popup_message = None;
        self.stats.menu_opens += 1;
        self.stats.popup_log.push(format!(
            "{:.2}s opened on {}",
            self.stats.now_s(),
            self.bars[i].device
        ));
        self.update_pause();
        self.rebuild_popup();
        self.rebuild_bar(i, "menu");
        self.render_all();
        Deferred::Activate(hwnd.0 as isize)
    }

    /// Closes the popup. With `refocus`, keyboard focus returns to the
    /// owner bar's menu button.
    pub fn close_popup(&mut self, refocus: bool) -> Deferred {
        let owner = self.popup.as_ref().map(|p| p.owner);
        let hwnd = self.close_popup_now();
        match (refocus, owner) {
            (true, Some(o)) => {
                if let Some(i) = self.bar_index(o) {
                    self.bars[i].s.focus = menu_element(&self.config().bar.modules);
                }
                Deferred::Activate(o.0 as isize)
            }
            _ => hwnd.map_or(Deferred::Nothing, |_| Deferred::Nothing),
        }
    }

    pub(crate) fn close_popup_now(&mut self) -> Option<HWND> {
        let p = self.popup.take()?;
        p.s.release_uia();
        let hwnd = p.s.hwnd;
        let owner = p.owner;
        drop(p);
        // SAFETY: destroying our own popup window.
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        self.stats
            .popup_log
            .push(format!("{:.2}s closed", self.stats.now_s()));
        if let Some(i) = self.bar_index(owner) {
            self.rebuild_bar(i, "menu");
            self.schedule_collapse(i);
        }
        self.update_pause();
        Some(hwnd)
    }

    pub fn on_popup_deactivated(&mut self, hwnd: HWND) {
        if self.popup.as_ref().is_some_and(|p| p.s.hwnd == hwnd)
            && self.config().bar.popup.close_on_focus_loss
        {
            self.stats
                .popup_log
                .push(format!("{:.2}s closing: focus lost", self.stats.now_s()));
            self.close_popup_now();
        }
    }

    fn set_popup_focus(&mut self, id: ElementId) {
        let Some(p) = self.popup.as_mut() else { return };
        if p.s.focus == Some(id) {
            return;
        }
        p.s.focus = Some(id);
        self.rebuild_popup();
        if let Some(p) = &self.popup
            && p.s.keyboard_active
            && uia::raise_focus(&p.s.host, id)
        {
            self.stats.uia_focus_events += 1;
        }
    }

    fn popup_activate(&mut self, id: ElementId) -> Deferred {
        let Some(action) = self.popup.as_ref().and_then(|p| {
            p.actions
                .iter()
                .find(|(e, _)| *e == id)
                .map(|(_, a)| a.clone())
        }) else {
            return Deferred::Nothing;
        };
        // Disabled controls do nothing.
        let enabled = self
            .popup
            .as_ref()
            .and_then(|p| {
                p.s.host
                    .state
                    .borrow()
                    .scene
                    .tree
                    .get(id)
                    .map(|e| e.enabled)
            })
            .unwrap_or(false);
        if !enabled {
            return Deferred::Nothing;
        }
        self.stats
            .popup_log
            .push(format!("{:.2}s action {action:?}", self.stats.now_s()));
        match action {
            PopupAction::Close => self.close_popup(true),
            PopupAction::OpenStudio => {
                self.close_popup_now();
                Deferred::OpenStudio
            }
            PopupAction::Exit => {
                self.stats.exit_reason = "settings: exit".into();
                Deferred::Quit
            }
            PopupAction::SetTheme(theme) => {
                self.set_theme(&theme);
                Deferred::Nothing
            }
            other => {
                self.change_bar(&other);
                Deferred::Nothing
            }
        }
    }

    fn set_theme(&mut self, id: &str) {
        let mode = system_mode(&self.args);
        let pref = self.config().theme.mode;
        let result = match &mut self.theme {
            ThemeSource::Service(s) => s
                .select(id, pref, mode, &mut self.sink)
                .map(|_| true)
                .map_err(|e| e.to_string()),
            ThemeSource::Override { engine, .. } => engine
                .activate(id, pref, mode, &mut self.sink)
                .map(|_| false)
                .map_err(|e| e.to_string()),
        };
        let en = self.strings.lang == Lang::En;
        self.popup_message = Some(match result {
            Ok(true) => {
                self.stats
                    .config_saves
                    .push(format!("{:.2}s theme {id}", self.stats.now_s()));
                if en {
                    "Theme saved".into()
                } else {
                    "테마를 저장했습니다".into()
                }
            }
            Ok(false) => {
                if en {
                    "Theme shown for this session only (--theme or --mode was given)".into()
                } else {
                    "이번 실행에만 적용됨 (--theme 또는 --mode 사용 중)".into()
                }
            }
            Err(e) => {
                self.stats.errors.push(format!("theme: {e}"));
                if en {
                    format!("Theme not changed: {e}")
                } else {
                    format!("테마를 바꾸지 못했습니다: {e}")
                }
            }
        });
        self.restyle();
        for i in 0..self.bars.len() {
            self.layout_bar(i, "theme");
        }
        self.rebuild_popup();
    }

    fn change_bar(&mut self, action: &PopupAction) {
        let mut next = self.config().bar.clone();
        let changed = match action {
            PopupAction::HeightDown | PopupAction::HeightUp => {
                let cur = next.height.unwrap_or(self.style.height as u16);
                let new = if *action == PopupAction::HeightUp {
                    cur + 2
                } else {
                    cur.saturating_sub(2)
                }
                .clamp(
                    windowsrice_core::bar::BAR_HEIGHT_RANGE.0,
                    windowsrice_core::bar::BAR_HEIGHT_RANGE.1,
                );
                next.height = Some(new);
                new != cur
            }
            PopupAction::ToggleAutoHide => {
                next.auto_hide = !next.auto_hide;
                true
            }
            PopupAction::ToggleReserve => {
                next.reserve_work_area = !next.reserve_work_area;
                true
            }
            PopupAction::ToggleAllMonitors => {
                next.monitors = match next.monitors {
                    windowsrice_core::bar::MonitorPlacement::All => {
                        windowsrice_core::bar::MonitorPlacement::Primary
                    }
                    windowsrice_core::bar::MonitorPlacement::Primary => {
                        windowsrice_core::bar::MonitorPlacement::All
                    }
                };
                true
            }
            other => apply_module_action(&mut next.modules, other),
        };
        if !changed {
            return;
        }
        let en = self.strings.lang == Lang::En;
        let saved = match &mut self.theme {
            ThemeSource::Service(s) => s.update_config(|c| c.bar = next.clone()).map(|_| true),
            ThemeSource::Override { config, .. } => {
                let mut c = config.clone();
                c.bar = next.clone();
                c.validate().map_err(|e| e.to_string()).map(|()| {
                    *config = c;
                    false
                })
            }
        };
        self.popup_message = Some(match saved {
            Ok(true) => {
                self.stats
                    .config_saves
                    .push(format!("{:.2}s {action:?}", self.stats.now_s()));
                if en {
                    "Settings saved".into()
                } else {
                    "설정을 저장했습니다".into()
                }
            }
            Ok(false) => {
                if en {
                    "Changed for this session only (--theme or --mode was given)".into()
                } else {
                    "이번 실행에만 적용됨 (--theme 또는 --mode 사용 중)".into()
                }
            }
            Err(e) => {
                self.stats.errors.push(format!("config: {e}"));
                if en {
                    format!("Not saved: {e}")
                } else {
                    format!("저장하지 못했습니다: {e}")
                }
            }
        });
        // Apply: scheduler intervals, bars on monitors, positions, modules.
        self.scheduler.reconfigure(&self.config().bar.modules);
        self.sync_bars("settings");
        self.rebuild_all("settings");
    }

    // ------------------------------------------------------------ system events

    pub fn on_display_change(&mut self, why: &'static str) {
        // Debounce bursts of display messages.
        self.stats
            .display_events
            .push(format!("{:.2}s {why}", self.stats.now_s()));
        // SAFETY: timer on our own window.
        unsafe { SetTimer(Some(self.hwnd), TIMER_DISPLAY, 300, None) };
    }

    pub fn on_dpi_changed(&mut self, hwnd: HWND, dpi: u32) {
        self.stats.dpi_events.push(format!(
            "{:.2}s WM_DPICHANGED {dpi} (forced scale: {})",
            self.stats.now_s(),
            self.args.scale.is_some()
        ));
        if let Some(i) = self.bar_index(hwnd) {
            if let Some(m) = monitors::monitor_info(self.bars[i].monitor.handle) {
                self.bars[i].monitor = m;
            }
            self.layout_bar(i, "WM_DPICHANGED");
        }
    }

    pub fn on_settings_changed(&mut self) {
        let mode = system_mode(&self.args);
        let notices = match &mut self.theme {
            ThemeSource::Service(s) => s.reapply(mode, &mut self.sink),
            ThemeSource::Override { engine, .. } => engine.apply_selection(mode, &mut self.sink),
        };
        self.notices.extend(notices.iter().map(notice_text));
        self.restyle();
        self.rebuild_all("settings");
    }

    pub fn on_appbar(&mut self, hwnd: HWND, code: usize, lparam: LPARAM) {
        let Some(i) = self.bar_index(hwnd) else {
            return;
        };
        match code {
            appbar::ABN_POSCHANGED => {
                self.stats.appbar_events.push(format!(
                    "{:.2}s ABN_POSCHANGED {}",
                    self.stats.now_s(),
                    self.bars[i].device
                ));
                self.layout_bar(i, "ABN_POSCHANGED");
            }
            appbar::ABN_FULLSCREENAPP => {
                self.stats.appbar_events.push(format!(
                    "{:.2}s ABN_FULLSCREENAPP {} {}",
                    self.stats.now_s(),
                    self.bars[i].device,
                    lparam.0 != 0
                ));
                self.check_fullscreen();
            }
            _ => {}
        }
    }

    pub fn on_timer(&mut self, id: usize) -> Deferred {
        let once = |id: usize| {
            // SAFETY: one-shot timers on our own window.
            unsafe {
                let _ = KillTimer(Some(self.hwnd), id);
            }
        };
        match id {
            TIMER_CLOCK => {
                self.refresh_clock_only();
                self.rebuild_all("clock");
            }
            TIMER_EXIT => {
                self.stats.exit_reason = "exit-after".into();
                return Deferred::Quit;
            }
            TIMER_FULLSCREEN => self.check_fullscreen(),
            TIMER_DISPLAY => {
                once(TIMER_DISPLAY);
                self.sync_bars("display change");
            }
            TIMER_DEVICE_LOSS => {
                once(TIMER_DEVICE_LOSS);
                self.simulate_device_loss();
            }
            TIMER_DEVICE_LOSS_EVERY => self.simulate_device_loss(),
            TIMER_SUSPEND_HIDE => {
                once(TIMER_SUSPEND_HIDE);
                self.suspended = true;
                for i in 0..self.bars.len() {
                    self.hide_bar(i, "suspend test");
                }
            }
            TIMER_SUSPEND_SHOW => {
                once(TIMER_SUSPEND_SHOW);
                self.suspended = false;
                for i in 0..self.bars.len() {
                    self.resume_bar(i, "suspend test");
                }
            }
            _ => {}
        }
        Deferred::Nothing
    }

    fn simulate_device_loss(&mut self) {
        for b in &mut self.bars {
            b.s.renderer.simulate_device_loss();
            b.s.dirty = true;
            b.s.reason = "simulated-device-loss";
        }
    }

    pub fn on_hotkey(&mut self) -> Deferred {
        // Focus the bar on the monitor under the cursor, else the first.
        let mut pt = POINT::default();
        // SAFETY: reads the cursor position.
        let _ = unsafe { GetCursorPos(&raw mut pt) };
        let i = self
            .bars
            .iter()
            .position(|b| {
                pt.x >= b.monitor.rect.left
                    && pt.x < b.monitor.rect.right
                    && pt.y >= b.monitor.rect.top
                    && pt.y < b.monitor.rect.bottom
            })
            .unwrap_or(0);
        match self.bars.get(i) {
            Some(b) => Deferred::Activate(b.s.hwnd.0 as isize),
            None => Deferred::Nothing,
        }
    }

    pub fn remember_foreground(&mut self) {
        // SAFETY: no preconditions.
        let fg = unsafe { GetForegroundWindow() };
        if !fg.is_invalid() && !self.own_window(fg) {
            self.prev_foreground = Some(fg.0 as isize);
        }
    }

    pub fn get_object(&self, hwnd: HWND, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        let host = self
            .bars
            .iter()
            .map(|b| &b.s)
            .chain(self.popup.as_ref().map(|p| &p.s))
            .chain(self.studio.as_ref().map(|s| &s.s))
            .find(|s| s.hwnd == hwnd)?
            .host
            .clone();
        let root = uia::simple(&host, ElementId(1));
        // SAFETY: returning our provider for our own window.
        Some(unsafe { UiaReturnRawElementProvider(hwnd, wparam, lparam, &root) })
    }

    pub fn is_popup(&self, hwnd: HWND) -> bool {
        self.popup.as_ref().is_some_and(|p| p.s.hwnd == hwnd)
    }

    pub fn is_controller(&self, hwnd: HWND) -> bool {
        hwnd == self.hwnd
    }

    pub fn on_bar_activate(&self, hwnd: HWND) {
        if self.bars.iter().any(|b| b.s.hwnd == hwnd && b.appbar) {
            appbar::activated(hwnd);
        }
    }

    pub fn on_bar_pos_changed(&self, hwnd: HWND) {
        if self.bars.iter().any(|b| b.s.hwnd == hwnd && b.appbar) {
            appbar::window_pos_changed(hwnd);
        }
    }

    /// Releases everything and records diagnostics. The caller then ends the
    /// message loop.
    pub fn shutdown(&mut self) {
        self.close_popup_now();
        if self.studio.is_some() {
            self.studio_close();
        }
        let calls: u64 = self.bars.iter().map(|b| b.s.host.calls.get()).sum();
        self.stats.uia_provider_calls = calls;
        self.stats.scheduler = json!(
            self.scheduler
                .stats()
                .into_iter()
                .map(|(s, st)| json!({"source": format!("{s:?}"), "stats": st}))
                .collect::<Vec<_>>()
        );
        self.stats.render = json!(self.bars.iter().map(|b| { let i = b.s.renderer.info(); json!({"device": b.device, "adapter": i.adapter, "feature_level": format!("0x{:X}", i.feature_level), "warp": i.warp, "init_ms": i.init.as_secs_f64() * 1000.0, "swapchain": [i.width, i.height], "dpi": i.dpi, "recoveries": i.recoveries}) }).collect::<Vec<_>>());
        if let Some(b) = self.bars.first() {
            let t = b.s.renderer.text();
            self.stats.text = json!({
                "family": self.style.family, "fallback": self.style.fallback, "icon_family": t.icon_family(), "missing_families": t.missing_families,
                "hangul_resolves_to": t.resolved_family("한글", self.style.weight_regular).ok().flatten(),
                "latin_resolves_to": t.resolved_family("Bar", self.style.weight_regular).ok().flatten(),
            });
        }
        self.stats.warnings = self.warnings();
        self.scheduler.stop();
        for h in self.hooks.drain(..).chain(self.name_hook.take()) {
            // SAFETY: unhooking our own hooks.
            unsafe {
                let _ = UnhookWinEvent(h);
            }
        }
        if self.hotkey {
            // SAFETY: unregistering our own hotkey.
            unsafe {
                let _ = UnregisterHotKey(Some(self.hwnd), HOTKEY_FOCUS);
            }
        }
        let bars: Vec<BarWin> = std::mem::take(&mut self.bars);
        for b in bars {
            self.destroy_bar(b, "shutdown");
        }
        self.stats.resources_end = process_counters();
        if self.stats.exit_reason.is_empty() {
            self.stats.exit_reason = "closed".into();
        }
        if let Some(p) = &self.args.stats {
            self.stats.startup_milestones = crate::stats::marks();
            self.stats.write(p);
        }
        // SAFETY: ends the message loop.
        unsafe { PostQuitMessage(0) };
    }
}

fn reading_status<T>(r: &Reading<T>) -> ModuleStatus {
    match r {
        Reading::Unavailable(why) => ModuleStatus::Unavailable(why.clone()),
        Reading::Absent => ModuleStatus::Absent,
        _ => ModuleStatus::Ok,
    }
}

pub(crate) fn request(
    t: &ResolvedTheme,
    args: &Args,
    backdrop_ok: bool,
    high_contrast: bool,
    bar: &windowsrice_core::bar::BarStyle,
) -> SurfaceRequest {
    let bar_backplates = bar.backplates;
    // `backdrop` here only says whether the surface is translucent: bars
    // draw translucency themselves (see `window::apply_dwm` callers).
    let (backdrop, theme_opacity) = windowsrice_ui::style::bar_surface(t, bar);
    SurfaceRequest {
        backdrop,
        theme_opacity,
        opacity_override: args.opacity,
        backplates: match (args.backplates, bar_backplates) {
            (BackplateMode::Auto, windowsrice_core::bar::BackplatePolicy::Always) => {
                BackplateMode::Always
            }
            (BackplateMode::Auto, windowsrice_core::bar::BackplatePolicy::Never) => {
                BackplateMode::Never
            }
            (m, _) => m,
        },
        reduced_transparency: args.reduced_transparency
            || !appearance::transparency_effects_enabled(),
        backdrop_available: backdrop_ok,
        high_contrast,
    }
}

pub(crate) fn track_leave(hwnd: HWND) {
    let mut t = TRACKMOUSEEVENT {
        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: hwnd,
        dwHoverTime: 0,
    };
    // SAFETY: tracking our own window.
    unsafe {
        let _ = TrackMouseEvent(&raw mut t);
    }
}

/// Out-of-context WinEvent callback, delivered through our message loop.
unsafe extern "system" fn win_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    if event == EVENT_SYSTEM_FOREGROUND {
        with_ctl(|c| c.on_foreground(Some(hwnd)));
    } else {
        with_ctl(|c| c.on_object_event(event, hwnd, id_object));
    }
    with_ctl(Controller::render_all);
}

/// Runs deferred work outside the controller borrow.
pub fn run_deferred(d: Deferred) {
    match d {
        Deferred::Nothing => {}
        Deferred::OpenPopup(owner) => {
            with_ctl(Controller::remember_foreground);
            if let Some(next) = with_ctl(|c| c.open_popup(owner)) {
                run_deferred(next);
            }
        }
        Deferred::Activate(h) => {
            with_ctl(Controller::remember_foreground);
            let hwnd = HWND(h as *mut core::ffi::c_void);
            // SAFETY: activating our own window in response to user input
            // (click, key, hotkey or an assistive-technology request).
            unsafe {
                let _ = SetForegroundWindow(hwnd);
                let _ = SetFocus(Some(hwnd));
            }
        }
        Deferred::Dismiss(prev) => {
            if let Some(p) = prev {
                // SAFETY: returning activation to the window that had it.
                unsafe {
                    let _ = SetForegroundWindow(HWND(p as *mut core::ffi::c_void));
                }
            }
        }
        Deferred::Quit => {
            with_ctl(Controller::shutdown);
        }
        Deferred::OpenStudio => {
            if let Some(next) = with_ctl(Controller::open_studio) {
                run_deferred(next);
            }
        }
        Deferred::Confirm(c) => crate::studio_win::run_confirm(c),
        Deferred::OpenFile => crate::studio_win::run_open_file(),
        Deferred::SaveFile(name) => crate::studio_win::run_save_file(&name),
        Deferred::PickColor(c) => crate::studio_win::run_pick_color(c),
    }
}
