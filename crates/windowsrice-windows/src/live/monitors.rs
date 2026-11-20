//! Monitor enumeration with work areas and effective DPI.

use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;
use windows::core::BOOL;

use crate::display::PixelRect;
use crate::util::from_wide;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorInfo {
    pub handle: HMONITOR,
    pub device_name: String,
    /// Stable identity (monitor device path), when Windows reports one.
    pub stable_id: Option<String>,
    pub friendly_name: Option<String>,
    pub rect: PixelRect,
    pub work: PixelRect,
    pub dpi: u32,
    pub primary: bool,
}

fn px(r: RECT) -> PixelRect {
    PixelRect {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

pub fn monitor_info(handle: HMONITOR) -> Option<MonitorInfo> {
    let mut mi = MONITORINFOEXW::default();
    mi.monitorInfo.cbSize = u32::try_from(size_of::<MONITORINFOEXW>()).unwrap_or(0);
    // SAFETY: `mi` is a MONITORINFOEXW with cbSize set, which GetMonitorInfoW
    // accepts through a MONITORINFO pointer.
    let ok = unsafe { GetMonitorInfoW(handle, (&raw mut mi).cast::<MONITORINFO>()) };
    if !ok.as_bool() {
        return None;
    }
    let (mut dx, mut dy) = (96u32, 96u32);
    // SAFETY: plain out-parameters.
    let _ = unsafe { GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &raw mut dx, &raw mut dy) };
    Some(MonitorInfo {
        handle,
        device_name: from_wide(&mi.szDevice),
        stable_id: None,
        friendly_name: None,
        rect: px(mi.monitorInfo.rcMonitor),
        work: px(mi.monitorInfo.rcWork),
        dpi: dx,
        primary: mi.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
    })
}

unsafe extern "system" fn collect(h: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the `&mut Vec<HMONITOR>` passed by `monitors`, valid
    // for the duration of EnumDisplayMonitors.
    let list = unsafe { &mut *(data.0 as *mut Vec<HMONITOR>) };
    list.push(h);
    BOOL(1)
}

/// All monitors in enumeration order.
pub fn monitors() -> Vec<MonitorInfo> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    // SAFETY: the callback only appends to `handles`, which outlives the call.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM((&raw mut handles) as isize),
        );
    }
    let ids = crate::display::monitor_identities().unwrap_or_default();
    handles
        .into_iter()
        .filter_map(monitor_info)
        .map(|mut m| {
            if let Some((_, path, name)) = ids.iter().find(|(gdi, _, _)| *gdi == m.device_name) {
                m.stable_id.clone_from(path);
                m.friendly_name.clone_from(name);
            }
            m
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real Windows: at least one monitor with a sane work area.
    #[test]
    fn enumerates_monitors() {
        let m = monitors();
        assert!(!m.is_empty());
        assert_eq!(m.iter().filter(|m| m.primary).count(), 1);
        for mon in &m {
            assert!(mon.dpi >= 96);
            assert!(mon.work.left >= mon.rect.left && mon.work.right <= mon.rect.right);
            // The stable id is the monitor's device path from DisplayConfig.
            // Windows leaves it empty when the monitor provides no
            // identification (observed with the monitor in standby: Windows
            // then also reports a 1920x1080 preferred mode); per-monitor
            // settings fall back to the device name. When present, it must
            // be a device interface path.
            eprintln!(
                "{}: stable_id={:?} name={:?}",
                mon.device_name, mon.stable_id, mon.friendly_name
            );
            if let Some(p) = mon.stable_id.as_deref() {
                assert!(p.starts_with(r"\\?\"), "{p:?}");
            }
        }
    }
}
