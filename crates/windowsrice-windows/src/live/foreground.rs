//! Foreground window and fullscreen detection (read-only).
//!
//! Only window properties are read (title, class, owning process image name,
//! rectangle, style). No process memory is read and no window is modified.

use windows::Win32::Foundation::{CloseHandle, HWND, RECT};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::{
    QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_STYLE, GetClassNameW, GetForegroundWindow, GetWindowLongW, GetWindowRect,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    IsZoomed, WS_CAPTION,
};
use windows::core::PWSTR;

use crate::display::PixelRect;
use crate::util::from_wide;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub title: String,
    pub class_name: String,
    pub pid: u32,
    /// Executable file name, e.g. `explorer.exe`.
    pub process_name: Option<String>,
}

pub fn foreground_window() -> Option<HWND> {
    // SAFETY: no preconditions.
    let h = unsafe { GetForegroundWindow() };
    (!h.is_invalid()).then_some(h)
}

/// Longest title read; longer titles are cut.
const MAX_TITLE: usize = 512;

pub fn window_info(hwnd: HWND) -> WindowInfo {
    // SAFETY: read-only window queries into local buffers; the process handle
    // is closed before returning.
    unsafe {
        let len = usize::try_from(GetWindowTextLengthW(hwnd))
            .unwrap_or(0)
            .min(MAX_TITLE);
        let mut title = vec![0u16; len + 1];
        let n = usize::try_from(GetWindowTextW(hwnd, &mut title)).unwrap_or(0);
        let mut class = [0u16; 256];
        let c = usize::try_from(GetClassNameW(hwnd, &mut class)).unwrap_or(0);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&raw mut pid));
        let process_name = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
            .ok()
            .and_then(|h| {
                let mut buf = [0u16; 1024];
                let mut size = u32::try_from(buf.len()).unwrap_or(0);
                let r = QueryFullProcessImageNameW(
                    h,
                    PROCESS_NAME_WIN32,
                    PWSTR(buf.as_mut_ptr()),
                    &raw mut size,
                );
                let _ = CloseHandle(h);
                r.ok()?;
                let path = from_wide(&buf[..size as usize]);
                path.rsplit(['\\', '/']).next().map(str::to_owned)
            });
        WindowInfo {
            title: from_wide(&title[..n]),
            class_name: from_wide(&class[..c]),
            pid,
            process_name,
        }
    }
}

/// Desktop and taskbar windows, which do not count as an active application.
pub fn is_shell_class(class_name: &str) -> bool {
    matches!(
        class_name,
        "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
    )
}

/// True when `hwnd` is a visible, non-minimised window that covers the whole
/// `monitor` without being an ordinary maximised captioned window (whose
/// frame also extends past the monitor edges).
pub fn covers_monitor(hwnd: HWND, monitor: &PixelRect) -> bool {
    // SAFETY: read-only window queries.
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return false;
        }
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &raw mut r).is_err() {
            return false;
        }
        let covers = r.left <= monitor.left
            && r.top <= monitor.top
            && r.right >= monitor.right
            && r.bottom >= monitor.bottom;
        let style = GetWindowLongW(hwnd, GWL_STYLE).cast_unsigned();
        let captioned = style & WS_CAPTION.0 == WS_CAPTION.0;
        covers && !(captioned && IsZoomed(hwnd).as_bool())
    }
}

/// Windows' own notification state indicates a fullscreen or presentation
/// application.
pub fn fullscreen_app_reported() -> bool {
    // SAFETY: no preconditions.
    match unsafe { SHQueryUserNotificationState() } {
        Ok(s) => s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE || s == QUNS_BUSY,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_classes() {
        assert!(is_shell_class("Progman"));
        assert!(is_shell_class("Shell_TrayWnd"));
        assert!(!is_shell_class("CabinetWClass"));
    }

    /// Real Windows: reading the current foreground window must not fail.
    #[test]
    fn foreground_info_is_readable() {
        if let Some(h) = foreground_window() {
            let info = window_info(h);
            assert!(info.pid > 0 || info.class_name.is_empty());
        }
        let _ = fullscreen_app_reported();
    }
}
