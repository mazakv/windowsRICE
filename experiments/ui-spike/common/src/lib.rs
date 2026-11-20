//! Shared code for the Phase 3.5 UI spike (EXPERIMENTAL).
//!
//! Both candidates take their content, design tokens, bar geometry, DWM
//! window setup and instrumentation from this crate so that they render the
//! same thing and are measured the same way. Tokens come from the real
//! windowsRICE Default theme (dark mode) via `windowsrice-theme`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{FILETIME, HWND, POINT};
use windows::Win32::Graphics::Dwm::{
    DWMSBT_NONE, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea,
    DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint,
};
use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTimePreciseAsFileTime};
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, MDT_EFFECTIVE_DPI,
    SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SetWindowLongPtrW, SetWindowPos, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

// ------------------------------------------------------------------ args

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backdrop {
    /// DWM Acrylic behind a translucent surface (theme opacity 0.88).
    Acrylic,
    /// Opaque surface, no system backdrop (Gaming Mode / reduced transparency).
    None,
}

#[derive(Debug, Clone)]
pub struct Args {
    pub exit_after: Option<Duration>,
    pub stats_path: Option<PathBuf>,
    /// Forces a UI scale (1.0, 1.25, 1.5, 2.0) instead of the monitor DPI.
    pub scale_override: Option<f32>,
    pub backdrop: Backdrop,
    /// Programmatically changes the selected workspace every 2 s and records
    /// state-change-to-frame latency and animation frame counts.
    pub interaction_test: bool,
    /// Slint only: renderer name ("femtovg" default, or "software").
    pub renderer: Option<String>,
}

impl Args {
    pub fn parse() -> Self {
        let mut a = Args {
            exit_after: None,
            stats_path: None,
            scale_override: None,
            backdrop: Backdrop::Acrylic,
            interaction_test: false,
            renderer: None,
        };
        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--exit-after" => {
                    a.exit_after = it
                        .next()
                        .and_then(|v| v.parse().ok())
                        .map(Duration::from_secs)
                }
                "--stats" => a.stats_path = it.next().map(PathBuf::from),
                "--scale" => a.scale_override = it.next().and_then(|v| v.parse().ok()),
                "--backdrop" => {
                    a.backdrop = match it.next().as_deref() {
                        Some("none") => Backdrop::None,
                        _ => Backdrop::Acrylic,
                    }
                }
                "--interaction-test" => a.interaction_test = true,
                "--renderer" => a.renderer = it.next(),
                other => eprintln!("ignoring unknown argument {other}"),
            }
        }
        a
    }
}

// ---------------------------------------------------------------- tokens

/// Straight-alpha RGBA in 0..=1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    fn from_theme(c: windowsrice_theme::Color) -> Self {
        Rgba {
            r: f32::from(c.r) / 255.0,
            g: f32::from(c.g) / 255.0,
            b: f32::from(c.b) / 255.0,
            a: f32::from(c.a) / 255.0,
        }
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Rgba { a, ..self }
    }

    pub fn lerp(self, other: Rgba, t: f32) -> Rgba {
        let m = |x: f32, y: f32| x + (y - x) * t;
        Rgba {
            r: m(self.r, other.r),
            g: m(self.g, other.g),
            b: m(self.b, other.b),
            a: m(self.a, other.a),
        }
    }

    /// `0xAARRGGBB` for Slint's `Color::from_argb_encoded`.
    pub fn argb_u32(self) -> u32 {
        // Clamped to 0..=255 before the cast.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
        (q(self.a) << 24) | (q(self.r) << 16) | (q(self.g) << 8) | q(self.b)
    }
}

/// Design tokens used by both prototypes (logical pixels at 100% scale).
#[derive(Debug, Clone)]
pub struct Tokens {
    pub surface: Rgba,
    pub surface_opacity: f32,
    pub pill_idle: Rgba,
    pub text_primary: Rgba,
    pub text_secondary: Rgba,
    pub accent: Rgba,
    pub text_on_accent: Rgba,
    pub focus_ring: Rgba,
    pub font_family: String,
    pub icon_family: &'static str,
    pub body_px: f32,
    pub weight_regular: u16,
    pub weight_emphasis: u16,
    pub bar_height: f32,
    pub bar_margin: f32,
    pub padding_x: f32,
    pub radius_panel: f32,
    pub radius_control: f32,
    pub gap: f32,
    pub pill_height: f32,
    pub pill_width: f32,
    pub pill_gap: f32,
    pub anim_ms: f32,
}

impl Tokens {
    pub fn windowsrice_default_dark() -> Self {
        let t = windowsrice_theme::presets::default_theme().theme;
        let p = t.palette.dark.expect("default theme has a dark palette");
        Tokens {
            surface: Rgba::from_theme(p.surface_base),
            surface_opacity: t.materials.surface_opacity,
            pill_idle: Rgba::from_theme(p.surface_overlay),
            text_primary: Rgba::from_theme(p.text_primary),
            text_secondary: Rgba::from_theme(p.text_secondary),
            accent: Rgba::from_theme(p.accent),
            text_on_accent: Rgba::from_theme(p.text_on_accent),
            focus_ring: Rgba::from_theme(p.focus_ring),
            font_family: t.typography.family.clone(),
            icon_family: "Segoe Fluent Icons",
            body_px: f32::from(t.typography.sizes.body),
            weight_regular: t.typography.weight_regular,
            weight_emphasis: t.typography.weight_emphasis,
            bar_height: f32::from(t.components.bar.height),
            bar_margin: f32::from(t.components.bar.margin),
            padding_x: f32::from(t.components.bar.padding_x),
            radius_panel: f32::from(t.radius.panel),
            radius_control: f32::from(t.radius.control),
            gap: f32::from(t.spacing.md),
            pill_height: 22.0,
            pill_width: 24.0,
            pill_gap: 4.0,
            anim_ms: f32::from(t.motion.duration_normal_ms),
        }
    }
}

// --------------------------------------------------------------- content

pub const WORKSPACES: [&str; 4] = ["1", "2", "3", "4"];
/// Mixed Korean/English active-window title.
pub const WINDOW_TITLE: &str = "문서 — 파일 탐색기";
/// Segoe Fluent Icons: Wi-Fi, Volume, Settings.
pub const STATUS_ICONS: [&str; 3] = ["\u{E701}", "\u{E767}", "\u{E713}"];
/// Input-method indicator (Korean).
pub const INPUT_INDICATOR: &str = "한";

const WEEKDAYS_KO: [&str; 7] = [
    "일요일",
    "월요일",
    "화요일",
    "수요일",
    "목요일",
    "금요일",
    "토요일",
];

/// Returns ("HH:MM", "M월 D일 요일") for the current local time.
pub fn clock_text() -> (String, String) {
    // SAFETY: GetLocalTime has no preconditions.
    let t = unsafe { GetLocalTime() };
    (
        format!("{:02}:{:02}", t.wHour, t.wMinute),
        format!(
            "{}월 {}일 {}",
            t.wMonth,
            t.wDay,
            WEEKDAYS_KO[usize::from(t.wDayOfWeek % 7)]
        ),
    )
}

/// Time until the next minute boundary (plus 20 ms), so the clock redraws
/// exactly once per minute.
pub fn until_next_minute() -> Duration {
    // SAFETY: GetLocalTime has no preconditions.
    let t = unsafe { GetLocalTime() };
    let ms_into_minute = u64::from(t.wSecond) * 1000 + u64::from(t.wMilliseconds);
    Duration::from_millis(60_000 - ms_into_minute.min(59_999) + 20)
}

// -------------------------------------------------------------- geometry

pub fn enable_per_monitor_dpi() {
    // SAFETY: documented call; harmless if awareness is already set.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}

/// Bar rectangle in physical pixels on the primary monitor, plus the scale.
#[derive(Debug, Clone, Copy)]
pub struct BarGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
}

pub fn bar_geometry(tokens: &Tokens, scale_override: Option<f32>) -> BarGeometry {
    // SAFETY: plain monitor queries on the primary monitor.
    let (rc, dpi) = unsafe {
        let mon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO {
            cbSize: u32::try_from(size_of::<MONITORINFO>()).unwrap_or(0),
            ..Default::default()
        };
        let _ = GetMonitorInfoW(mon, &raw mut mi);
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &raw mut dx, &raw mut dy);
        (mi.rcMonitor, dx)
    };
    #[allow(clippy::cast_precision_loss)]
    let scale = scale_override.unwrap_or(dpi as f32 / 96.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let px = |v: f32| (v * scale).round() as i32;
    let margin = px(tokens.bar_margin);
    BarGeometry {
        x: rc.left + margin,
        y: rc.top + margin,
        width: u32::try_from(rc.right - rc.left - 2 * margin).unwrap_or(800),
        height: u32::try_from(px(tokens.bar_height)).unwrap_or(34),
        scale,
    }
}

// ------------------------------------------------------------------- DWM

/// Applies the same window treatment in both prototypes: tool window (no
/// taskbar button), topmost, dark frame, Windows 11 rounded corners and the
/// requested system backdrop. Only this process's own window is modified.
/// Returns a short log of which calls succeeded.
pub fn apply_window_effects(hwnd: HWND, backdrop: Backdrop) -> Vec<String> {
    let mut log = Vec::new();
    // SAFETY: `hwnd` is a window owned by this process; attributes are
    // passed as correctly sized values.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let ex = (ex | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
        let r = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        log.push(format!("topmost+toolwindow: {}", r.is_ok()));

        let dark: i32 = 1;
        let r = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&raw const dark).cast(),
            4,
        );
        log.push(format!("dark mode: {}", r.is_ok()));
        let corner = DWMWCP_ROUND;
        let r = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&raw const corner).cast(),
            4,
        );
        log.push(format!("round corners: {}", r.is_ok()));

        let kind = match backdrop {
            Backdrop::Acrylic => {
                let m = MARGINS {
                    cxLeftWidth: -1,
                    cxRightWidth: -1,
                    cyTopHeight: -1,
                    cyBottomHeight: -1,
                };
                let r = DwmExtendFrameIntoClientArea(hwnd, &raw const m);
                log.push(format!("extend frame: {}", r.is_ok()));
                DWMSBT_TRANSIENTWINDOW
            }
            Backdrop::None => DWMSBT_NONE,
        };
        let r = DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, (&raw const kind).cast(), 4);
        log.push(format!("system backdrop {kind:?}: {}", r.is_ok()));
    }
    log
}

// ----------------------------------------------------------------- stats

fn filetime_ms(ft: FILETIME) -> f64 {
    let v = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
    #[allow(clippy::cast_precision_loss)]
    let ms = v as f64 / 10_000.0;
    ms
}

/// Milliseconds since this process was created (kernel creation time).
pub fn ms_since_process_creation() -> f64 {
    // SAFETY: querying times of the current process into valid out-params.
    unsafe {
        let (mut c, mut e, mut k, mut u) = Default::default();
        let _ = GetProcessTimes(
            GetCurrentProcess(),
            &raw mut c,
            &raw mut e,
            &raw mut k,
            &raw mut u,
        );
        filetime_ms(GetSystemTimePreciseAsFileTime()) - filetime_ms(c)
    }
}

#[derive(Debug, Clone)]
pub struct Interaction {
    pub latency_ms: f64,
    pub anim_frames: u32,
    pub anim_ms: f64,
}

/// Instrumentation recorded identically by both candidates.
#[derive(Debug)]
pub struct Stats {
    pub candidate: &'static str,
    pub renderer: String,
    pub started: Instant,
    pub first_frame_ms_since_process_creation: Option<f64>,
    pub frame_times_s: Vec<f64>,
    pub interactions: Vec<Interaction>,
    pub window_log: Vec<String>,
    pub scale: f32,
    pub backdrop: Backdrop,
}

impl Stats {
    pub fn new(
        candidate: &'static str,
        renderer: impl Into<String>,
        scale: f32,
        backdrop: Backdrop,
    ) -> Self {
        Stats {
            candidate,
            renderer: renderer.into(),
            started: Instant::now(),
            first_frame_ms_since_process_creation: None,
            frame_times_s: Vec::new(),
            interactions: Vec::new(),
            window_log: Vec::new(),
            scale,
            backdrop,
        }
    }

    pub fn record_frame(&mut self) {
        if self.first_frame_ms_since_process_creation.is_none() {
            self.first_frame_ms_since_process_creation = Some(ms_since_process_creation());
        }
        if self.frame_times_s.len() < 200_000 {
            self.frame_times_s
                .push(self.started.elapsed().as_secs_f64());
        }
    }

    pub fn write(&self, path: &std::path::Path) {
        let interactions: Vec<serde_json::Value> = self
            .interactions
            .iter()
            .map(|i| serde_json::json!({"latency_ms": i.latency_ms, "anim_frames": i.anim_frames, "anim_ms": i.anim_ms}))
            .collect();
        let v = serde_json::json!({
            "candidate": self.candidate,
            "renderer": self.renderer,
            "scale": self.scale,
            "backdrop": format!("{:?}", self.backdrop),
            "run_seconds": self.started.elapsed().as_secs_f64(),
            "first_frame_ms_since_process_creation": self.first_frame_ms_since_process_creation,
            "frames_total": self.frame_times_s.len(),
            "frame_times_s": self.frame_times_s,
            "interactions": interactions,
            "window_log": self.window_log,
        });
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, serde_json::to_string_pretty(&v).unwrap_or_default());
    }
}

/// Cubic-Bézier easing used by both candidates for the selection animation
/// (theme easing [0.2, 0.0, 0.0, 1.0]); solved numerically for x.
pub fn ease(t: f32) -> f32 {
    let (x1, y1, x2, y2) = (0.2_f32, 0.0_f32, 0.0_f32, 1.0_f32);
    let bez = |a: f32, b: f32, s: f32| {
        let u = 1.0 - s;
        3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
    };
    let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if bez(x1, x2, mid) < t {
            lo = mid
        } else {
            hi = mid
        }
    }
    bez(y1, y2, (lo + hi) / 2.0)
}
