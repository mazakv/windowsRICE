//! windowsRICE Bar (Phase 5).
//!
//! Full-width native top bars, one per configured monitor, rendered with
//! Direct2D, DirectWrite and DirectComposition, exposed to assistive
//! technology through UI Automation, styled by the theme engine and
//! configured through `config.toml` and the settings popup. It runs as a
//! normal user process: no elevation, no injection, no registry or shell
//! changes. The work-area reservation exists only while it runs.

#![windows_subsystem = "windows"]

mod appbar;
mod args;
mod controller;
mod design_io;
mod scheduler;
mod stats;
mod studio_win;
mod uia;
mod window;

use windows::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Accessibility::UiaDisconnectAllProviders;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DispatchMessageW, GetMessageW, IDC_ARROW, LoadCursorW, MA_NOACTIVATE, MSG,
    RegisterClassExW, TranslateMessage, WA_INACTIVE, WM_ACTIVATE, WM_CLOSE, WM_DISPLAYCHANGE,
    WM_DPICHANGED, WM_GETOBJECT, WM_HOTKEY, WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONUP,
    WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_SETFOCUS, WM_SETTINGCHANGE, WM_SYSCOLORCHANGE,
    WM_SYSKEYDOWN, WM_THEMECHANGED, WM_TIMER, WM_WINDOWPOSCHANGED, WNDCLASSEXW, WS_EX_TOOLWINDOW,
};
use windows::core::w;
use windowsrice_ui::ElementId;

use crate::controller::{
    CLASS_BAR, CLASS_CONTROLLER, CLASS_POPUP, Controller, Deferred, run_deferred, with_ctl,
};
use crate::uia::{UiaAction, WM_APP_UIA};

/// Sent by a second --studio launch to the running controller.
const WM_APP_OPEN_STUDIO: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 4;
/// Sent by `--quit` (installer, uninstaller) to close the running bar.
const WM_APP_QUIT: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 5;

/// Messages of the Design Studio window. Returns None to fall through to
/// the shared handling (UI Automation, default processing).
fn studio_message(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    use windows::Win32::Foundation::{POINT, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        MINMAXINFO, WM_CHAR, WM_GETMINMAXINFO, WM_LBUTTONDOWN, WM_SIZE,
    };
    let done = |r: LRESULT| {
        with_ctl(Controller::render_all);
        Some(r)
    };
    match msg {
        WM_SIZE => {
            with_ctl(Controller::studio_resize);
            done(LRESULT(0))
        }
        WM_GETMINMAXINFO => {
            if let Some(Some((w, h))) = with_ctl(|c| c.studio_min_size(hwnd)) {
                // SAFETY: Windows passes a valid MINMAXINFO for this message.
                unsafe { (*(lparam.0 as *mut MINMAXINFO)).ptMinTrackSize = POINT { x: w, y: h } };
            }
            Some(LRESULT(0))
        }
        WM_CHAR => {
            if let Some(ch) = char::from_u32(wparam.0 as u32) {
                with_ctl(|c| c.studio_char(ch));
            }
            done(LRESULT(0))
        }
        WM_KEYDOWN => {
            defer(with_ctl(|c| {
                c.studio_key(u16::try_from(wparam.0).unwrap_or(0))
            }));
            done(LRESULT(0))
        }
        WM_LBUTTONDOWN => {
            with_ctl(|c| c.studio_mouse_down(lo(lparam.0), hi(lparam.0)));
            done(LRESULT(0))
        }
        WM_LBUTTONUP => {
            defer(with_ctl(|c| c.studio_mouse_up(lo(lparam.0), hi(lparam.0))));
            done(LRESULT(0))
        }
        WM_MOUSEMOVE => {
            with_ctl(|c| c.studio_mouse_move(lo(lparam.0), hi(lparam.0)));
            done(LRESULT(0))
        }
        windows::Win32::UI::WindowsAndMessaging::WM_TIMER => {
            with_ctl(|c| c.studio_timer(wparam.0));
            done(LRESULT(0))
        }
        windows::Win32::UI::WindowsAndMessaging::WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16;
            with_ctl(|c| c.studio_wheel(delta));
            done(LRESULT(0))
        }
        WM_MOUSELEAVE => {
            with_ctl(Controller::studio_mouse_leave);
            done(LRESULT(0))
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            with_ctl(|c| c.studio_focus(msg == WM_SETFOCUS));
            done(LRESULT(0))
        }
        WM_APP_UIA => {
            let id = ElementId(u32::try_from(lparam.0).unwrap_or(0));
            if let Some(a) = UiaAction::from_code(wparam.0) {
                defer(with_ctl(|c| c.studio_uia(a, id)));
            }
            done(LRESULT(0))
        }
        WM_CLOSE => {
            defer(with_ctl(Controller::studio_close_request));
            done(LRESULT(0))
        }
        WM_DPICHANGED => {
            // SAFETY: lParam points to the suggested window rectangle.
            let r = unsafe { *(lparam.0 as *const RECT) };
            with_ctl(|c| c.studio_dpi_changed(Some(r)));
            done(LRESULT(0))
        }
        // The Studio activates normally on click.
        WM_MOUSEACTIVATE => None,
        _ => None,
    }
}

fn lo(v: isize) -> i32 {
    i32::from((v & 0xFFFF) as i16)
}

fn hi(v: isize) -> i32 {
    i32::from(((v >> 16) & 0xFFFF) as i16)
}

fn defer(d: Option<Deferred>) {
    if let Some(d) = d {
        run_deferred(d);
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if with_ctl(|c| c.is_studio(hwnd)).unwrap_or(false) {
        if msg == WM_MOUSEACTIVATE {
            // SAFETY: default processing (activate) for our own window.
            return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
        }
        if let Some(r) = studio_message(hwnd, msg, wparam, lparam) {
            return r;
        }
    }
    let handled = match msg {
        WM_APP_OPEN_STUDIO => {
            run_deferred(Deferred::OpenStudio);
            true
        }
        WM_APP_QUIT => {
            with_ctl(|c| c.stats.exit_reason = "--quit".into());
            run_deferred(Deferred::Quit);
            true
        }
        WM_GETOBJECT => {
            if let Some(Some(r)) = with_ctl(|c| c.get_object(hwnd, wparam, lparam)) {
                return r;
            }
            false
        }
        scheduler::WM_APP_READINGS => with_ctl(|c| c.on_readings(wparam.0)).is_some(),
        appbar::WM_APP_APPBAR => with_ctl(|c| c.on_appbar(hwnd, wparam.0, lparam)).is_some(),
        WM_APP_UIA => {
            let id = ElementId(u32::try_from(lparam.0).unwrap_or(0));
            if let Some(a) = UiaAction::from_code(wparam.0) {
                defer(with_ctl(|c| c.on_uia_action(hwnd, a, id)));
            }
            true
        }
        WM_TIMER => {
            if wparam.0 >= controller::TIMER_AUTOHIDE {
                with_ctl(|c| c.on_bar_timer(hwnd, wparam.0));
            } else {
                defer(with_ctl(|c| c.on_timer(wparam.0)));
            }
            true
        }
        // Alt+F4: Windows turns it into a close command only for windows with
        // a system menu, which the bar and popup do not have.
        WM_SYSKEYDOWN if wparam.0 == 0x73 => {
            if with_ctl(|c| c.is_popup(hwnd)).unwrap_or(false) {
                defer(with_ctl(|c| c.close_popup(true)));
            } else {
                with_ctl(|c| c.stats.exit_reason = "Alt+F4".into());
                run_deferred(Deferred::Quit);
            }
            true
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let vk = u16::try_from(wparam.0).unwrap_or(0);
            match with_ctl(|c| c.on_key(hwnd, vk)) {
                Some(Deferred::Nothing) | None => false,
                Some(d) => {
                    run_deferred(d);
                    true
                }
            }
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            defer(with_ctl(|c| c.on_focus(hwnd, msg == WM_SETFOCUS)));
            true
        }
        WM_ACTIVATE => {
            with_ctl(|c| c.on_bar_activate(hwnd));
            if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE {
                with_ctl(|c| c.on_popup_deactivated(hwnd));
            }
            false
        }
        WM_WINDOWPOSCHANGED => {
            with_ctl(|c| c.on_bar_pos_changed(hwnd));
            false
        }
        WM_MOUSEMOVE => with_ctl(|c| c.on_mouse_move(hwnd, lo(lparam.0), hi(lparam.0))).is_some(),
        WM_MOUSELEAVE => with_ctl(|c| c.on_mouse_leave(hwnd)).is_some(),
        WM_LBUTTONUP => {
            defer(with_ctl(|c| c.on_click(hwnd, lo(lparam.0), hi(lparam.0))));
            true
        }
        WM_MOUSEACTIVATE => {
            // Bars do not take activation from a click; the popup does.
            let is_popup = with_ctl(|c| c.is_popup(hwnd)).unwrap_or(false);
            if !is_popup {
                return LRESULT(MA_NOACTIVATE as isize);
            }
            false
        }
        WM_HOTKEY => {
            if wparam.0 == controller::HOTKEY_FOCUS as usize {
                defer(with_ctl(Controller::on_hotkey));
            }
            true
        }
        WM_DPICHANGED => with_ctl(|c| c.on_dpi_changed(hwnd, (wparam.0 & 0xFFFF) as u32)).is_some(),
        WM_DISPLAYCHANGE => with_ctl(|c| c.on_display_change("WM_DISPLAYCHANGE")).is_some(),
        WM_SETTINGCHANGE | WM_THEMECHANGED | WM_SYSCOLORCHANGE => {
            if with_ctl(|c| c.is_controller(hwnd)).unwrap_or(false) {
                with_ctl(Controller::on_settings_changed);
                // Monitor arrangement or work-area changes come this way too.
                with_ctl(|c| c.on_display_change("WM_SETTINGCHANGE"));
            }
            false
        }
        WM_CLOSE => {
            if with_ctl(|c| c.is_popup(hwnd)).unwrap_or(false) {
                defer(with_ctl(|c| c.close_popup(true)));
            } else {
                with_ctl(|c| c.stats.exit_reason = "WM_CLOSE".into());
                run_deferred(Deferred::Quit);
            }
            true
        }
        _ => false,
    };
    with_ctl(Controller::render_all);
    if handled {
        LRESULT(0)
    } else {
        // SAFETY: default processing for our own window.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }
}

fn fail(message: &str) -> std::process::ExitCode {
    eprintln!("windowsrice-panel: {message}");
    std::process::ExitCode::from(1)
}

fn main() -> std::process::ExitCode {
    stats::mark("main_entered");
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = match args::Args::parse(raw.clone().into_iter()) {
        Ok(a) => a,
        Err(e) => return fail(&e),
    };
    if args.quit {
        // Ask a running bar to exit cleanly (releasing its reserved screen
        // strip). Succeeds when no bar is running.
        // SAFETY: finds and messages the running controller window.
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW};
            if let Ok(h) = FindWindowW(CLASS_CONTROLLER, None) {
                let _ = PostMessageW(Some(h), WM_APP_QUIT, WPARAM(0), LPARAM(0));
            }
        }
        return std::process::ExitCode::SUCCESS;
    }
    // One instance per session.
    // SAFETY: named mutex in this session's namespace; released at exit.
    let _instance = unsafe { CreateMutexW(None, true, w!("Local\\windowsRICE.Panel")) };
    // SAFETY: no preconditions.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        if args.studio {
            // Ask the running bar to open Design Studio.
            // SAFETY: finds and messages the running controller window.
            unsafe {
                use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW};
                if let Ok(h) = FindWindowW(CLASS_CONTROLLER, None) {
                    let _ = PostMessageW(Some(h), WM_APP_OPEN_STUDIO, WPARAM(0), LPARAM(0));
                    return std::process::ExitCode::SUCCESS;
                }
            }
        }
        return fail("another windowsRICE bar is already running");
    }
    stats::mark("single_instance_checked");
    windowsrice_windows::display::enable_per_monitor_v2_awareness();
    // SAFETY: STA for this thread (UI Automation providers, DirectComposition).
    let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();

    // SAFETY: class registration and the hidden controller window.
    let controller_hwnd = unsafe {
        let Ok(instance) = GetModuleHandleW(None) else {
            return fail("GetModuleHandleW failed");
        };
        for class in [
            CLASS_BAR,
            CLASS_POPUP,
            CLASS_CONTROLLER,
            studio_win::CLASS_STUDIO,
        ] {
            let wc = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wndproc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                // The application icon (resource 1), when it was compiled in.
                hIcon: windows::Win32::UI::WindowsAndMessaging::LoadIconW(
                    Some(instance.into()),
                    windows::core::PCWSTR(std::ptr::without_provenance(1)),
                )
                .unwrap_or_default(),
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassExW(&raw const wc);
        }
        // A hidden top-level window: receives display, setting and hotkey
        // broadcasts that message-only windows do not get. Never shown.
        match window::create(CLASS_CONTROLLER, w!("windowsRICE"), WS_EX_TOOLWINDOW, None) {
            Ok(h) => h,
            Err(e) => return fail(&format!("controller window: {e}")),
        }
    };

    stats::mark("windows_registered");
    let stats_path = args.stats.clone();
    let open_studio = args.studio;
    match Controller::new(controller_hwnd, args, raw) {
        Ok(c) => controller::install(c),
        Err(e) => {
            if let Some(p) = stats_path {
                let _ = std::fs::write(p, serde_json::json!({ "startup_error": e }).to_string());
            }
            return fail(&e);
        }
    }
    with_ctl(Controller::render_all);
    // After the first frame: the font list for Design Studio is built on a
    // background thread that would otherwise compete with the first frame.
    studio_win::prefetch_fonts();
    if open_studio {
        run_deferred(Deferred::OpenStudio);
    }

    let mut msg = MSG::default();
    // SAFETY: standard message loop on the windows' thread.
    unsafe {
        while GetMessageW(&raw mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&raw const msg);
            DispatchMessageW(&raw const msg);
        }
    }
    controller::uninstall();
    // SAFETY: release providers still referenced by clients, then balance
    // CoInitializeEx.
    unsafe {
        let _ = UiaDisconnectAllProviders();
        if com {
            CoUninitialize();
        }
    }
    std::process::ExitCode::SUCCESS
}
