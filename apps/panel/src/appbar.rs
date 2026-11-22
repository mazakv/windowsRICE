//! Work-area reservation through the Windows application desktop toolbar
//! interface (`SHAppBarMessage`).
//!
//! A registered bar asks the shell for a position on the top (or, for the
//! Phase 14 bottom bar, the bottom) edge of its
//! monitor (`ABM_QUERYPOS`, which accounts for the taskbar and other
//! toolbars already there), then claims it (`ABM_SETPOS`). The shell removes
//! that strip from the monitor's work area, so maximised windows start below
//! the bar. The registration lives only as long as the window: `ABM_REMOVE`
//! releases it on exit, auto-hide and settings changes, and the shell drops
//! it if the window is destroyed. Nothing is written to the registry or to
//! taskbar settings.

use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::UI::Shell::{
    ABE_BOTTOM, ABE_TOP, ABM_ACTIVATE, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS,
    ABM_WINDOWPOSCHANGED, APPBARDATA, SHAppBarMessage,
};
use windows::Win32::UI::WindowsAndMessaging::WM_APP;
use windowsrice_ui::PhysRect;

/// Callback message the shell sends to registered bars.
pub const WM_APP_APPBAR: u32 = WM_APP + 3;
/// `ABN_POSCHANGED`: the work area or another toolbar changed.
pub const ABN_POSCHANGED: usize = 1;
/// `ABN_FULLSCREENAPP`: a fullscreen application opened (`lParam != 0`) or
/// closed.
pub const ABN_FULLSCREENAPP: usize = 2;

fn data(hwnd: HWND) -> APPBARDATA {
    APPBARDATA {
        cbSize: size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        uCallbackMessage: WM_APP_APPBAR,
        uEdge: ABE_TOP,
        ..Default::default()
    }
}

/// Claims the bottom `height` pixels of `monitor` (Phase 14 bottom bar).
/// The shell moves the request above the taskbar and other bottom toolbars.
pub fn set_pos_bottom(hwnd: HWND, monitor: PhysRect, height: i32) -> PhysRect {
    let mut d = data(hwnd);
    d.uEdge = ABE_BOTTOM;
    d.rc = RECT {
        left: monitor.left,
        top: monitor.bottom - height,
        right: monitor.right,
        bottom: monitor.bottom,
    };
    // SAFETY: position negotiation for our own registration.
    unsafe {
        SHAppBarMessage(ABM_QUERYPOS, &raw mut d);
        d.rc.top = d.rc.bottom - height;
        SHAppBarMessage(ABM_SETPOS, &raw mut d);
    }
    PhysRect {
        left: d.rc.left,
        top: d.rc.bottom - height,
        right: d.rc.right,
        bottom: d.rc.bottom,
    }
}

pub fn register(hwnd: HWND) -> bool {
    let mut d = data(hwnd);
    // SAFETY: registers our own window with the shell.
    unsafe { SHAppBarMessage(ABM_NEW, &raw mut d) != 0 }
}

pub fn remove(hwnd: HWND) {
    let mut d = data(hwnd);
    // SAFETY: removes our own registration.
    unsafe {
        SHAppBarMessage(ABM_REMOVE, &raw mut d);
    }
}

/// Claims the top `height` pixels of `monitor`. Returns the rectangle the
/// shell granted (it may start lower if another toolbar is already on top).
pub fn set_pos(hwnd: HWND, monitor: PhysRect, height: i32) -> PhysRect {
    let mut d = data(hwnd);
    d.rc = RECT {
        left: monitor.left,
        top: monitor.top,
        right: monitor.right,
        bottom: monitor.top + height,
    };
    // SAFETY: position negotiation for our own registration.
    unsafe {
        SHAppBarMessage(ABM_QUERYPOS, &raw mut d);
        d.rc.bottom = d.rc.top + height;
        SHAppBarMessage(ABM_SETPOS, &raw mut d);
    }
    PhysRect {
        left: d.rc.left,
        top: d.rc.top,
        right: d.rc.right,
        bottom: d.rc.top + height,
    }
}

/// Must be forwarded from `WM_ACTIVATE` of a registered bar.
pub fn activated(hwnd: HWND) {
    let mut d = data(hwnd);
    d.lParam = LPARAM(0);
    // SAFETY: notification about our own window.
    unsafe {
        SHAppBarMessage(ABM_ACTIVATE, &raw mut d);
    }
}

/// Must be forwarded from `WM_WINDOWPOSCHANGED` of a registered bar.
pub fn window_pos_changed(hwnd: HWND) {
    let mut d = data(hwnd);
    // SAFETY: notification about our own window.
    unsafe {
        SHAppBarMessage(ABM_WINDOWPOSCHANGED, &raw mut d);
    }
}
