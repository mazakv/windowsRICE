//! GPU adapter information via DXGI, and AMD ADLX library presence (read-only).
//!
//! Phase 1 does not load ADLX. It only reports whether the driver-installed
//! library exists and which version it is; sensor access is Phase 10.

use std::path::PathBuf;

use serde::Serialize;
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_ERROR_NOT_FOUND, IDXGIFactory1,
};
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VS_FIXEDFILEINFO, VerQueryValueW,
};
use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows::core::{HSTRING, w};

use crate::error::{WinError, api};
use crate::util::from_wide;

pub const VENDOR_AMD: u32 = 0x1002;
pub const VENDOR_NVIDIA: u32 = 0x10DE;
pub const VENDOR_INTEL: u32 = 0x8086;
pub const VENDOR_MICROSOFT: u32 = 0x1414;

#[derive(Debug, Clone, Serialize)]
pub struct GpuAdapter {
    pub description: String,
    pub vendor_id: u32,
    pub vendor: &'static str,
    pub device_id: u32,
    pub subsystem_id: u32,
    pub revision: u32,
    /// Dedicated video memory (VRAM) reported by the driver, in bytes.
    pub dedicated_video_memory_bytes: u64,
    pub dedicated_system_memory_bytes: u64,
    pub shared_system_memory_bytes: u64,
    pub is_software: bool,
}

pub fn vendor_name(vendor_id: u32) -> &'static str {
    match vendor_id {
        VENDOR_AMD => "AMD",
        VENDOR_NVIDIA => "NVIDIA",
        VENDOR_INTEL => "Intel",
        VENDOR_MICROSOFT => "Microsoft",
        _ => "unknown",
    }
}

/// Lists DXGI adapters. Creates a DXGI factory only; no device is created.
pub fn adapters() -> Result<Vec<GpuAdapter>, WinError> {
    // SAFETY: documented factory creation with no parameters.
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1() }.map_err(api("CreateDXGIFactory1"))?;
    let mut out = Vec::new();
    for index in 0u32.. {
        // SAFETY: enumerating by index; DXGI_ERROR_NOT_FOUND marks the end.
        let adapter = match unsafe { factory.EnumAdapters1(index) } {
            Ok(a) => a,
            Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(e) => return Err(WinError::from_windows("IDXGIFactory1::EnumAdapters1", &e)),
        };
        // SAFETY: `adapter` is a valid IDXGIAdapter1.
        let d = unsafe { adapter.GetDesc1() }.map_err(api("IDXGIAdapter1::GetDesc1"))?;
        out.push(GpuAdapter {
            description: from_wide(&d.Description).trim().to_owned(),
            vendor_id: d.VendorId,
            vendor: vendor_name(d.VendorId),
            device_id: d.DeviceId,
            subsystem_id: d.SubSysId,
            revision: d.Revision,
            dedicated_video_memory_bytes: d.DedicatedVideoMemory as u64,
            dedicated_system_memory_bytes: d.DedicatedSystemMemory as u64,
            shared_system_memory_bytes: d.SharedSystemMemory as u64,
            is_software: d.Flags & (DXGI_ADAPTER_FLAG_SOFTWARE.0.cast_unsigned()) != 0,
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
pub struct AdlxPresence {
    pub library_path: PathBuf,
    pub present: bool,
    /// Fixed file version; for this driver-shipped DLL it matches the driver package version.
    pub file_version: Option<String>,
    /// Fixed product version; for ADLX this is the ADLX SDK/runtime version.
    pub product_version: Option<String>,
}

/// Reports whether the AMD ADLX runtime (`amdadlx64.dll`) is installed.
/// The library is not loaded.
pub fn adlx_presence() -> Result<AdlxPresence, WinError> {
    let mut buf = [0u16; 260];
    // SAFETY: `buf` is writable; the return value is the copied length.
    let len = unsafe { GetSystemDirectoryW(Some(&mut buf)) } as usize;
    if len == 0 || len >= buf.len() {
        return Err(WinError::from_windows(
            "GetSystemDirectoryW",
            &windows::core::Error::from_thread(),
        ));
    }
    let library_path = PathBuf::from(String::from_utf16_lossy(&buf[..len])).join("amdadlx64.dll");
    let present = library_path.is_file();
    let versions = if present {
        fixed_versions(&library_path)
    } else {
        None
    };
    Ok(AdlxPresence {
        library_path,
        present,
        file_version: versions.as_ref().map(|v| v.0.clone()),
        product_version: versions.map(|v| v.1),
    })
}

fn dotted(ms: u32, ls: u32) -> String {
    format!("{}.{}.{}.{}", ms >> 16, ms & 0xFFFF, ls >> 16, ls & 0xFFFF)
}

/// Reads the fixed (file, product) version resource without executing the file.
fn fixed_versions(path: &std::path::Path) -> Option<(String, String)> {
    let path_h = HSTRING::from(path.as_os_str());
    // SAFETY: NUL-terminated path; the handle out-parameter is optional.
    let size = unsafe { GetFileVersionInfoSizeW(&path_h, None) };
    if size == 0 {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    // SAFETY: `data` is writable for `size` bytes.
    unsafe { GetFileVersionInfoW(&path_h, None, size, data.as_mut_ptr().cast()) }.ok()?;
    let mut info_ptr: *mut core::ffi::c_void = std::ptr::null_mut();
    let mut info_len: u32 = 0;
    // SAFETY: `data` holds a version resource; "\\" selects VS_FIXEDFILEINFO,
    // and the returned pointer points into `data`.
    let ok = unsafe {
        VerQueryValueW(
            data.as_ptr().cast(),
            w!("\\"),
            &raw mut info_ptr,
            &raw mut info_len,
        )
    };
    if !ok.as_bool() || info_ptr.is_null() || (info_len as usize) < size_of::<VS_FIXEDFILEINFO>() {
        return None;
    }
    // SAFETY: checked non-null and large enough; the buffer lives in `data`.
    let info = unsafe { std::ptr::read_unaligned(info_ptr.cast::<VS_FIXEDFILEINFO>()) };
    Some((
        dotted(info.dwFileVersionMS, info.dwFileVersionLS),
        dotted(info.dwProductVersionMS, info.dwProductVersionLS),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_names() {
        assert_eq!(vendor_name(0x1002), "AMD");
        assert_eq!(vendor_name(0x1414), "Microsoft");
        assert_eq!(vendor_name(0xFFFF), "unknown");
    }

    /// On-machine test: DXGI always exposes at least the software adapter.
    #[test]
    fn live_lists_adapters() {
        let list = adapters().unwrap();
        assert!(!list.is_empty());
    }

    /// On-machine test.
    #[test]
    fn live_adlx_presence_query_succeeds() {
        let p = adlx_presence().unwrap();
        assert!(p.library_path.ends_with("amdadlx64.dll"));
        if p.present {
            assert!(p.file_version.is_some());
        }
    }
}
