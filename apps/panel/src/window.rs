//! Win32 window creation and treatment of the panel's own window, plus a
//! few read-only system settings. Nothing here touches another process's
//! windows or changes a system setting.

use windows::Win32::Foundation::{COLORREF, HWND};
use windows::Win32::Graphics::Dwm::{
    DWM_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW, DWMSBT_NONE, DWMSBT_TRANSIENTWINDOW,
    DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_DONOTROUND, DWMWCP_ROUND, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_WINDOW, COLOR_WINDOWTEXT,
    GetSysColor, SYS_COLOR_INDEX,
};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows::Win32::System::Threading::{
    GR_GDIOBJECTS, GR_USEROBJECTS, GetCurrentProcess, GetGuiResources, GetProcessHandleCount,
};
use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_TOPMOST, SPI_GETCLIENTAREAANIMATION, SPI_GETHIGHCONTRAST, SWP_NOACTIVATE,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetWindowPos, SystemParametersInfoW,
};
use windows::core::BOOL;
use windowsrice_theme::Color;
use windowsrice_theme::schema::Backdrop;
use windowsrice_ui::PhysRect;
use windowsrice_ui::style::HighContrastColors;

/// Applies DWM treatment to the panel window. Returns whether the requested
/// backdrop was accepted (false means: draw a solid surface).
pub fn apply_dwm(hwnd: HWND, backdrop: Backdrop, dark: bool, round: bool) -> bool {
    // SAFETY: attributes of our own window, passed as correctly sized values.
    unsafe {
        let d: BOOL = dark.into();
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&raw const d).cast(),
            size_of::<BOOL>() as u32,
        );
        let corner = if round {
            DWMWCP_ROUND
        } else {
            DWMWCP_DONOTROUND
        };
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&raw const corner).cast(),
            size_of_val(&corner) as u32,
        );
        let (kind, extend): (DWM_SYSTEMBACKDROP_TYPE, i32) = match backdrop {
            Backdrop::Acrylic => (DWMSBT_TRANSIENTWINDOW, -1),
            Backdrop::Mica => (DWMSBT_MAINWINDOW, -1),
            Backdrop::None => (DWMSBT_NONE, 0),
        };
        let m = MARGINS {
            cxLeftWidth: extend,
            cxRightWidth: extend,
            cyTopHeight: extend,
            cyBottomHeight: extend,
        };
        let extended = DwmExtendFrameIntoClientArea(hwnd, &raw const m).is_ok();
        let set = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            (&raw const kind).cast(),
            size_of_val(&kind) as u32,
        )
        .is_ok();
        backdrop == Backdrop::None || (extended && set)
    }
}

/// Moves and sizes the panel window (topmost, never activated by this call).
pub fn place(hwnd: HWND, r: PhysRect) {
    // SAFETY: positions our own window.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            r.left,
            r.top,
            r.width(),
            r.height(),
            SWP_NOACTIVATE,
        );
    }
}

fn sys_color(i: SYS_COLOR_INDEX) -> Color {
    // SAFETY: no preconditions.
    let c: COLORREF = COLORREF(unsafe { GetSysColor(i) });
    Color::rgb(
        (c.0 & 0xFF) as u8,
        ((c.0 >> 8) & 0xFF) as u8,
        ((c.0 >> 16) & 0xFF) as u8,
    )
}

/// Windows high-contrast colours, when high-contrast mode is on. Read-only
/// query (`SPI_GET…`).
pub fn high_contrast() -> Option<HighContrastColors> {
    let mut hc = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    // SAFETY: GET query into a correctly sized struct; changes nothing.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            hc.cbSize,
            Some((&raw mut hc).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .is_ok();
    (ok && hc.dwFlags.contains(HCF_HIGHCONTRASTON)).then(|| HighContrastColors {
        window: sys_color(COLOR_WINDOW),
        window_text: sys_color(COLOR_WINDOWTEXT),
        highlight: sys_color(COLOR_HIGHLIGHT),
        highlight_text: sys_color(COLOR_HIGHLIGHTTEXT),
        gray_text: sys_color(COLOR_GRAYTEXT),
    })
}

/// Settings › Accessibility › Visual effects › Animation effects.
pub fn animations_enabled() -> bool {
    let mut on = BOOL(1);
    // SAFETY: GET query into a BOOL; changes nothing.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&raw mut on).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .is_ok();
    !ok || on.as_bool()
}

/// Resource counters of this process.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct ProcessCounters {
    pub handles: u32,
    pub gdi_objects: u32,
    pub user_objects: u32,
    pub private_bytes: usize,
    pub working_set: usize,
}

pub fn process_counters() -> ProcessCounters {
    // SAFETY: queries about the current process into local out-parameters.
    unsafe {
        let p = GetCurrentProcess();
        let mut handles = 0u32;
        let _ = GetProcessHandleCount(p, &raw mut handles);
        let mut mem = PROCESS_MEMORY_COUNTERS_EX {
            cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };
        let _ = GetProcessMemoryInfo(p, (&raw mut mem).cast(), mem.cb);
        ProcessCounters {
            handles,
            gdi_objects: GetGuiResources(p, GR_GDIOBJECTS),
            user_objects: GetGuiResources(p, GR_USEROBJECTS),
            private_bytes: mem.PrivateUsage,
            working_set: mem.WorkingSetSize,
        }
    }
}

/// Creates a hidden window of a registered class (no redirection bitmap:
/// content comes from DirectComposition).
pub fn create(
    class: windows::core::PCWSTR,
    title: windows::core::PCWSTR,
    ex_style: windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE,
    owner: Option<HWND>,
) -> windows::core::Result<HWND> {
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, WS_EX_NOREDIRECTIONBITMAP, WS_POPUP,
    };
    // SAFETY: creates a window of a class registered by this process.
    unsafe {
        let instance = GetModuleHandleW(None)?;
        CreateWindowExW(
            ex_style | WS_EX_NOREDIRECTIONBITMAP,
            class,
            title,
            WS_POPUP,
            0,
            0,
            1,
            1,
            owner,
            None,
            Some(instance.into()),
            None,
        )
    }
}
