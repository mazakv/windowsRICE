//! OS, CPU and memory information (read-only).

use serde::Serialize;
use windows::Wdk::System::SystemServices::RtlGetVersion;
use windows::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
use windows::Win32::System::SystemInformation::{
    GetLogicalProcessorInformation, GetNativeSystemInfo, GetPhysicallyInstalledSystemMemory,
    GetProductInfo, GlobalMemoryStatusEx, MEMORYSTATUSEX, OS_PRODUCT_TYPE, OSVERSIONINFOW,
    PROCESSOR_ARCHITECTURE_AMD64, PROCESSOR_ARCHITECTURE_ARM64, PROCESSOR_ARCHITECTURE_INTEL,
    RelationProcessorCore, SYSTEM_INFO, SYSTEM_LOGICAL_PROCESSOR_INFORMATION,
};
use windowsrice_core::capability::EditionFamily;

use crate::error::{WinError, api};
use crate::registry_read::{Hive, read_dword, read_string};

const CURRENT_VERSION_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
const CPU0_KEY: &str = r"HARDWARE\DESCRIPTION\System\CentralProcessor\0";

#[derive(Debug, Clone, Serialize)]
pub struct OsInfo {
    pub major: u32,
    pub minor: u32,
    pub build: u32,
    /// Update build revision (the part after the dot, e.g. 26200.**9974**).
    pub ubr: Option<u32>,
    /// Feature update label such as "25H2".
    pub display_version: Option<String>,
    /// Raw `GetProductInfo` product type (e.g. 0x30 = PRODUCT_PROFESSIONAL).
    pub product_type: u32,
    pub edition: EditionFamily,
    /// Registry `EditionID`, for reference only.
    pub edition_id_registry: Option<String>,
    /// Registry `ProductName`. Known to read "Windows 10 ..." on Windows 11,
    /// so it is reported for reference and never used for decisions.
    pub product_name_registry: Option<String>,
    pub architecture: String,
}

impl OsInfo {
    pub fn is_windows_11(&self) -> bool {
        self.major == 10 && self.build >= 22000
    }
}

pub fn os_info() -> Result<OsInfo, WinError> {
    let mut v = OSVERSIONINFOW {
        dwOSVersionInfoSize: u32::try_from(size_of::<OSVERSIONINFOW>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: `v` is a properly sized OSVERSIONINFOW with its size field set.
    let status = unsafe { RtlGetVersion(&raw mut v) };
    if status.is_err() {
        return Err(WinError::Api {
            api: "RtlGetVersion",
            code: status.0.cast_unsigned(),
            message: "NTSTATUS failure".into(),
        });
    }

    let mut product = OS_PRODUCT_TYPE::default();
    // SAFETY: `product` is a valid out-pointer. The return value only signals
    // whether the type is known; an unknown type leaves 0 (PRODUCT_UNDEFINED).
    let _ = unsafe { GetProductInfo(v.dwMajorVersion, v.dwMinorVersion, 0, 0, &raw mut product) };

    let reg_str = |name| {
        read_string(Hive::LocalMachine, CURRENT_VERSION_KEY, name)
            .ok()
            .flatten()
    };

    Ok(OsInfo {
        major: v.dwMajorVersion,
        minor: v.dwMinorVersion,
        build: v.dwBuildNumber,
        ubr: read_dword(Hive::LocalMachine, CURRENT_VERSION_KEY, "UBR")
            .ok()
            .flatten(),
        display_version: reg_str("DisplayVersion"),
        product_type: product.0,
        edition: EditionFamily::from_product_type(product.0),
        edition_id_registry: reg_str("EditionID"),
        product_name_registry: reg_str("ProductName"),
        architecture: native_architecture(),
    })
}

fn native_architecture() -> String {
    let mut info = SYSTEM_INFO::default();
    // SAFETY: `info` is a valid out-pointer.
    unsafe { GetNativeSystemInfo(&raw mut info) };
    // SAFETY: GetNativeSystemInfo initializes the processor-architecture view of the union.
    let arch = unsafe { info.Anonymous.Anonymous.wProcessorArchitecture };
    match arch {
        a if a == PROCESSOR_ARCHITECTURE_AMD64 => "x64".into(),
        a if a == PROCESSOR_ARCHITECTURE_ARM64 => "arm64".into(),
        a if a == PROCESSOR_ARCHITECTURE_INTEL => "x86".into(),
        a => format!("unknown ({})", a.0),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CpuInfo {
    pub name: Option<String>,
    pub physical_cores: Option<u32>,
    pub logical_processors: Option<u32>,
    /// Base frequency reported by firmware in MHz (`~MHz` registry value).
    pub reported_base_mhz: Option<u32>,
}

pub fn cpu_info() -> Result<CpuInfo, WinError> {
    let name = read_string(Hive::LocalMachine, CPU0_KEY, "ProcessorNameString")?
        .map(|s| s.trim().to_owned());
    let reported_base_mhz = read_dword(Hive::LocalMachine, CPU0_KEY, "~MHz")?;
    let (physical_cores, logical_processors) = match core_counts() {
        Ok((p, l)) => (Some(p), Some(l)),
        Err(e) => {
            tracing::warn!(error = %e, "processor topology unavailable");
            (None, None)
        }
    };
    Ok(CpuInfo {
        name,
        physical_cores,
        logical_processors,
        reported_base_mhz,
    })
}

/// Counts cores and logical processors. Limited to the calling process's
/// processor group (at most 64 logical processors); sufficient for desktop CPUs.
fn core_counts() -> Result<(u32, u32), WinError> {
    let mut len: u32 = 0;
    // SAFETY: querying the required buffer length with a null buffer.
    match unsafe { GetLogicalProcessorInformation(None, &raw mut len) } {
        Err(e) if e.code() == ERROR_INSUFFICIENT_BUFFER.to_hresult() => {}
        Err(e) => return Err(WinError::from_windows("GetLogicalProcessorInformation", &e)),
        Ok(()) => return Err(WinError::Unavailable("empty processor information".into())),
    }
    let count = len as usize / size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION>();
    let mut buf = vec![SYSTEM_LOGICAL_PROCESSOR_INFORMATION::default(); count];
    // SAFETY: `buf` holds `len` bytes of correctly typed elements.
    unsafe { GetLogicalProcessorInformation(Some(buf.as_mut_ptr()), &raw mut len) }
        .map_err(api("GetLogicalProcessorInformation"))?;
    let filled = len as usize / size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION>();
    let cores: Vec<_> = buf[..filled]
        .iter()
        .filter(|i| i.Relationship == RelationProcessorCore)
        .collect();
    let logical: u32 = cores.iter().map(|c| c.ProcessorMask.count_ones()).sum();
    Ok((u32::try_from(cores.len()).unwrap_or(u32::MAX), logical))
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryInfo {
    /// Physically installed RAM as reported by SMBIOS firmware tables.
    pub installed_bytes: Option<u64>,
    /// RAM visible to Windows (installed minus hardware/firmware reservations).
    pub visible_bytes: u64,
    /// Point-in-time sample; not a live value.
    pub available_bytes_snapshot: u64,
    /// Point-in-time sample; not a live value.
    pub load_percent_snapshot: u32,
}

pub fn memory_info() -> Result<MemoryInfo, WinError> {
    let mut status = MEMORYSTATUSEX {
        dwLength: u32::try_from(size_of::<MEMORYSTATUSEX>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: `status` is a properly sized MEMORYSTATUSEX with its length set.
    unsafe { GlobalMemoryStatusEx(&raw mut status) }.map_err(api("GlobalMemoryStatusEx"))?;

    let mut installed_kib: u64 = 0;
    // SAFETY: valid out-pointer.
    let installed_bytes = unsafe { GetPhysicallyInstalledSystemMemory(&raw mut installed_kib) }
        .ok()
        .map(|()| installed_kib * 1024);

    Ok(MemoryInfo {
        installed_bytes,
        visible_bytes: status.ullTotalPhys,
        available_bytes_snapshot: status.ullAvailPhys,
        load_percent_snapshot: status.dwMemoryLoad,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On-machine test.
    #[test]
    fn live_os_info_is_plausible() {
        let os = os_info().unwrap();
        assert_eq!(os.major, 10);
        assert!(os.build >= 10240, "build {}", os.build);
        assert!(!os.architecture.starts_with("unknown"));
    }

    /// On-machine test.
    #[test]
    fn live_cpu_and_memory_are_plausible() {
        let cpu = cpu_info().unwrap();
        let (p, l) = (cpu.physical_cores.unwrap(), cpu.logical_processors.unwrap());
        assert!(p >= 1 && l >= p);
        let mem = memory_info().unwrap();
        assert!(mem.visible_bytes > 0);
        if let Some(installed) = mem.installed_bytes {
            assert!(installed >= mem.visible_bytes);
        }
    }
}
