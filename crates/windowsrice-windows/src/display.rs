//! Display and DPI information (read-only).
//!
//! Native resolution comes from the monitor's *preferred mode* reported via
//! the Display Configuration API (normally taken from the EDID). Current mode
//! and refresh rate come from the active display path. DPI comes from
//! `GetDpiForMonitor(MDT_EFFECTIVE_DPI)`. All pixel values are physical
//! pixels; the effective (logical) resolution is derived from the DPI scale.

use serde::Serialize;
use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_PREFERRED_MODE, DISPLAYCONFIG_DEVICE_INFO_HEADER,
    DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_RATIONAL,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME,
    DISPLAYCONFIG_TARGET_PREFERRED_MODE, DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes,
    QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
};
use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplayMonitors, EnumDisplaySettingsW, GetMonitorInfoW,
    HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor,
    GetThreadDpiAwarenessContext, MDT_EFFECTIVE_DPI, SetProcessDpiAwarenessContext,
};
use windows::core::{BOOL, PCWSTR};

use crate::error::{WinError, api};
use crate::util::{from_wide, from_wide_nonempty};

const MONITORINFOF_PRIMARY: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PixelRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DisplayInfo {
    pub gdi_device_name: String,
    pub friendly_name: Option<String>,
    pub is_primary: bool,
    /// Monitor's preferred (native) mode, in physical pixels.
    pub native_resolution: Option<Resolution>,
    pub preferred_mode_refresh_hz: Option<f64>,
    /// Current desktop mode, in physical pixels.
    pub current_resolution: Option<Resolution>,
    /// Refresh rate of the active display path.
    pub current_refresh_hz: Option<f64>,
    pub bits_per_pixel: Option<u32>,
    pub rotation_degrees: Option<u32>,
    /// Monitor rectangle in virtual-screen physical pixels.
    pub desktop_rect: PixelRect,
    pub effective_dpi: Option<u32>,
    /// Windows display scaling (100, 125, 150, ...).
    pub scale_percent: Option<u32>,
    /// Logical resolution seen by DPI-unaware applications at this scale.
    pub effective_resolution: Option<Resolution>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct DpiAwarenessStatus {
    /// Whether this process is Per-Monitor-v2 DPI aware after setup.
    pub per_monitor_v2: bool,
}

/// Opts the calling process into Per-Monitor-v2 DPI awareness so that all
/// measurements are physical pixels. Affects this process only.
pub fn enable_per_monitor_v2_awareness() -> DpiAwarenessStatus {
    // SAFETY: documented call with a predefined context constant. It fails
    // harmlessly if awareness was already set (for example by a manifest).
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    // SAFETY: both arguments are valid awareness contexts.
    let per_monitor_v2 = unsafe {
        AreDpiAwarenessContextsEqual(
            GetThreadDpiAwarenessContext(),
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        )
    }
    .as_bool();
    DpiAwarenessStatus { per_monitor_v2 }
}

/// `dpi * 100 / 96`, rounded to the nearest percent.
pub fn scale_percent_for_dpi(dpi: u32) -> u32 {
    (dpi * 100 + 48) / 96
}

/// Physical resolution divided by the DPI scale, rounded.
pub fn effective_resolution(physical: Resolution, dpi: u32) -> Option<Resolution> {
    if dpi == 0 {
        return None;
    }
    let scale =
        |v: u32| u32::try_from((u64::from(v) * 96 + u64::from(dpi) / 2) / u64::from(dpi)).ok();
    Some(Resolution {
        width: scale(physical.width)?,
        height: scale(physical.height)?,
    })
}

/// Converts a rational refresh rate to Hz with three decimals.
pub fn rational_hz(numerator: u32, denominator: u32) -> Option<f64> {
    if denominator == 0 || numerator == 0 {
        return None;
    }
    Some((f64::from(numerator) / f64::from(denominator) * 1000.0).round() / 1000.0)
}

/// Maps `DISPLAYCONFIG_ROTATION` (1-4) to degrees.
pub fn rotation_degrees(rotation: i32) -> Option<u32> {
    match rotation {
        1 => Some(0),
        2 => Some(90),
        3 => Some(180),
        4 => Some(270),
        _ => None,
    }
}

#[derive(Debug, Default)]
struct PathDetails {
    friendly_name: Option<String>,
    native_resolution: Option<Resolution>,
    preferred_mode_refresh_hz: Option<f64>,
    current_refresh_hz: Option<f64>,
    rotation_degrees: Option<u32>,
}

fn header<T>(
    kind: windows::Win32::Devices::Display::DISPLAYCONFIG_DEVICE_INFO_TYPE,
    adapter: windows::Win32::Foundation::LUID,
    id: u32,
) -> DISPLAYCONFIG_DEVICE_INFO_HEADER {
    DISPLAYCONFIG_DEVICE_INFO_HEADER {
        r#type: kind,
        size: u32::try_from(size_of::<T>()).unwrap_or(0),
        adapterId: adapter,
        id,
    }
}

/// Queries active display paths, keyed by GDI source device name.
fn query_paths() -> Result<Vec<(String, PathDetails)>, WinError> {
    let (paths, _modes) = query_display_config()?;
    let mut out = Vec::with_capacity(paths.len());
    for path in &paths {
        let src = &path.sourceInfo;
        let tgt = &path.targetInfo;

        let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
            header: header::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>(
                DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                src.adapterId,
                src.id,
            ),
            ..Default::default()
        };
        // SAFETY: `source` begins with a correctly initialized header whose size matches.
        if unsafe { DisplayConfigGetDeviceInfo(&raw mut source.header) }
            != ERROR_SUCCESS.0.cast_signed()
        {
            continue;
        }
        let gdi_name = from_wide(&source.viewGdiDeviceName);

        let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
            header: header::<DISPLAYCONFIG_TARGET_DEVICE_NAME>(
                DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                tgt.adapterId,
                tgt.id,
            ),
            ..Default::default()
        };
        // SAFETY: as above.
        let friendly_name = (unsafe { DisplayConfigGetDeviceInfo(&raw mut target.header) }
            == ERROR_SUCCESS.0.cast_signed())
        .then(|| from_wide_nonempty(&target.monitorFriendlyDeviceName))
        .flatten();

        let mut preferred = DISPLAYCONFIG_TARGET_PREFERRED_MODE {
            header: header::<DISPLAYCONFIG_TARGET_PREFERRED_MODE>(
                DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_PREFERRED_MODE,
                tgt.adapterId,
                tgt.id,
            ),
            ..Default::default()
        };
        // SAFETY: as above.
        let (native_resolution, preferred_mode_refresh_hz) =
            if unsafe { DisplayConfigGetDeviceInfo(&raw mut preferred.header) }
                == ERROR_SUCCESS.0.cast_signed()
            {
                let vsync: DISPLAYCONFIG_RATIONAL =
                    preferred.targetMode.targetVideoSignalInfo.vSyncFreq;
                (
                    Some(Resolution {
                        width: preferred.width,
                        height: preferred.height,
                    }),
                    rational_hz(vsync.Numerator, vsync.Denominator),
                )
            } else {
                (None, None)
            };

        out.push((
            gdi_name,
            PathDetails {
                friendly_name,
                native_resolution,
                preferred_mode_refresh_hz,
                current_refresh_hz: rational_hz(
                    tgt.refreshRate.Numerator,
                    tgt.refreshRate.Denominator,
                ),
                rotation_degrees: rotation_degrees(tgt.rotation.0),
            },
        ));
    }
    Ok(out)
}

fn query_display_config()
-> Result<(Vec<DISPLAYCONFIG_PATH_INFO>, Vec<DISPLAYCONFIG_MODE_INFO>), WinError> {
    for _attempt in 0..3 {
        let (mut n_paths, mut n_modes) = (0u32, 0u32);
        // SAFETY: valid out-pointers.
        let st = unsafe {
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &raw mut n_paths, &raw mut n_modes)
        };
        if st != ERROR_SUCCESS {
            return Err(WinError::from_win32("GetDisplayConfigBufferSizes", st));
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); n_paths as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); n_modes as usize];
        // SAFETY: buffers sized from GetDisplayConfigBufferSizes; counts updated on return.
        let st = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &raw mut n_paths,
                paths.as_mut_ptr(),
                &raw mut n_modes,
                modes.as_mut_ptr(),
                None,
            )
        };
        if st == ERROR_INSUFFICIENT_BUFFER {
            continue; // topology changed between calls
        }
        if st != ERROR_SUCCESS {
            return Err(WinError::from_win32("QueryDisplayConfig", st));
        }
        paths.truncate(n_paths as usize);
        modes.truncate(n_modes as usize);
        return Ok((paths, modes));
    }
    Err(WinError::Unavailable(
        "display topology kept changing during query".into(),
    ))
}

unsafe extern "system" fn collect_monitor(
    hmon: HMONITOR,
    _hdc: HDC,
    _rc: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: `data` is the `&mut Vec<HMONITOR>` passed by `monitors()` and
    // outlives the synchronous EnumDisplayMonitors call.
    let list = unsafe { &mut *(data.0 as *mut Vec<HMONITOR>) };
    list.push(hmon);
    BOOL::from(true)
}

/// Enumerates all active monitors.
pub fn displays() -> Result<Vec<DisplayInfo>, WinError> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    // SAFETY: the callback only pushes into `handles`, which outlives the call.
    let ok = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect_monitor),
            LPARAM(&raw mut handles as isize),
        )
    };
    if !ok.as_bool() {
        return Err(WinError::from_windows(
            "EnumDisplayMonitors",
            &windows::core::Error::from_thread(),
        ));
    }

    let paths = match query_paths() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "display configuration API unavailable; native mode unknown");
            Vec::new()
        }
    };

    let mut out = Vec::with_capacity(handles.len());
    for hmon in handles {
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = u32::try_from(size_of::<MONITORINFOEXW>()).unwrap_or(0);
        // SAFETY: MONITORINFOEXW starts with MONITORINFO and cbSize declares the extended size.
        if !unsafe { GetMonitorInfoW(hmon, (&raw mut info).cast::<MONITORINFO>()) }.as_bool() {
            continue;
        }
        let gdi_device_name = from_wide(&info.szDevice);
        let r = info.monitorInfo.rcMonitor;

        let (mut dpi_x, mut dpi_y) = (0u32, 0u32);
        // SAFETY: valid monitor handle and out-pointers.
        let effective_dpi =
            unsafe { GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &raw mut dpi_x, &raw mut dpi_y) }
                .map_err(api("GetDpiForMonitor"))
                .inspect_err(|e| tracing::warn!(error = %e))
                .ok()
                .map(|()| dpi_x);

        let mut mode = DEVMODEW {
            dmSize: u16::try_from(size_of::<DEVMODEW>()).unwrap_or(0),
            ..Default::default()
        };
        let name_w: Vec<u16> = gdi_device_name.encode_utf16().chain(Some(0)).collect();
        // SAFETY: `name_w` is NUL-terminated and `mode.dmSize` is set.
        let have_mode = unsafe {
            EnumDisplaySettingsW(
                PCWSTR(name_w.as_ptr()),
                ENUM_CURRENT_SETTINGS,
                &raw mut mode,
            )
        }
        .as_bool();
        let current_resolution = have_mode.then_some(Resolution {
            width: mode.dmPelsWidth,
            height: mode.dmPelsHeight,
        });

        let details = paths
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(&gdi_device_name))
            .map(|(_, d)| d);

        out.push(DisplayInfo {
            friendly_name: details.and_then(|d| d.friendly_name.clone()),
            is_primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
            native_resolution: details.and_then(|d| d.native_resolution),
            preferred_mode_refresh_hz: details.and_then(|d| d.preferred_mode_refresh_hz),
            current_resolution,
            current_refresh_hz: details.and_then(|d| d.current_refresh_hz).or_else(|| {
                (have_mode && mode.dmDisplayFrequency > 1)
                    .then(|| f64::from(mode.dmDisplayFrequency))
            }),
            bits_per_pixel: have_mode.then_some(mode.dmBitsPerPel),
            rotation_degrees: details.and_then(|d| d.rotation_degrees),
            desktop_rect: PixelRect {
                left: r.left,
                top: r.top,
                right: r.right,
                bottom: r.bottom,
            },
            scale_percent: effective_dpi.map(scale_percent_for_dpi),
            effective_resolution: current_resolution
                .zip(effective_dpi)
                .and_then(|(res, dpi)| effective_resolution(res, dpi)),
            effective_dpi,
            gdi_device_name,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_percent_matches_windows_steps() {
        for (dpi, pct) in [
            (96, 100),
            (120, 125),
            (144, 150),
            (168, 175),
            (192, 200),
            (240, 250),
        ] {
            assert_eq!(scale_percent_for_dpi(dpi), pct);
        }
    }

    #[test]
    fn effective_resolution_divides_by_scale() {
        let uhd = Resolution {
            width: 3840,
            height: 2160,
        };
        assert_eq!(
            effective_resolution(uhd, 144),
            Some(Resolution {
                width: 2560,
                height: 1440
            })
        );
        assert_eq!(effective_resolution(uhd, 96), Some(uhd));
        assert_eq!(
            effective_resolution(uhd, 192),
            Some(Resolution {
                width: 1920,
                height: 1080
            })
        );
        assert_eq!(effective_resolution(uhd, 0), None);
    }

    #[test]
    fn rational_refresh_rates() {
        assert_eq!(rational_hz(160_000, 1000), Some(160.0));
        assert_eq!(rational_hz(60_000, 1001), Some(59.94));
        assert_eq!(rational_hz(1, 0), None);
        assert_eq!(rational_hz(0, 1), None);
    }

    #[test]
    fn rotations() {
        assert_eq!(rotation_degrees(1), Some(0));
        assert_eq!(rotation_degrees(4), Some(270));
        assert_eq!(rotation_degrees(0), None);
    }

    /// On-machine test: requires an interactive session with a display.
    #[test]
    fn live_enumerates_at_least_one_display() {
        let status = enable_per_monitor_v2_awareness();
        let list = displays().unwrap();
        assert!(!list.is_empty());
        assert!(list.iter().filter(|d| d.is_primary).count() == 1);
        if status.per_monitor_v2 {
            for d in &list {
                let r = d.desktop_rect;
                if let Some(cur) = d.current_resolution
                    && d.rotation_degrees == Some(0)
                {
                    // In physical pixels the desktop rect must equal the mode.
                    assert_eq!(i64::from(cur.width), i64::from(r.right) - i64::from(r.left));
                    assert_eq!(
                        i64::from(cur.height),
                        i64::from(r.bottom) - i64::from(r.top)
                    );
                }
            }
        }
    }
}
