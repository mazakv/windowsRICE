//! Processor, memory and GPU utilisation.

use windows::Win32::Foundation::{ERROR_SUCCESS, FILETIME};
use windows::Win32::System::Performance::{
    PDH_CSTATUS_INVALID_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE,
    PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA, PdhAddEnglishCounterW, PdhCloseQuery,
    PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;
use windows::core::w;

use crate::error::{WinError, api};

fn ft(t: FILETIME) -> u64 {
    (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)
}

/// Whole-system processor utilisation from successive `GetSystemTimes` calls.
#[derive(Debug, Default)]
pub struct CpuSampler {
    prev: Option<(u64, u64)>,
}

impl CpuSampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Percentage busy since the previous call. The first call primes the
    /// sampler and returns `None`.
    pub fn sample(&mut self) -> Result<Option<f32>, WinError> {
        let (mut idle, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        // SAFETY: valid out-pointers to local FILETIMEs.
        unsafe {
            GetSystemTimes(
                Some(&raw mut idle),
                Some(&raw mut kernel),
                Some(&raw mut user),
            )
        }
        .map_err(api("GetSystemTimes"))?;
        // Kernel time includes idle time.
        let total = ft(kernel) + ft(user);
        let idle = ft(idle);
        let out = self.prev.and_then(|(pt, pi)| {
            let dt = total.checked_sub(pt)?;
            let di = idle.checked_sub(pi)?;
            (dt > 0).then(|| (100.0 * (1.0 - di as f64 / dt as f64)).clamp(0.0, 100.0) as f32)
        });
        self.prev = Some((total, idle));
        Ok(out)
    }
}

/// Physical memory in use and installed (visible to Windows), in bytes.
pub fn memory_usage() -> Result<(u64, u64), WinError> {
    let mut m = MEMORYSTATUSEX {
        dwLength: u32::try_from(size_of::<MEMORYSTATUSEX>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: `m` is a correctly sized MEMORYSTATUSEX.
    unsafe { GlobalMemoryStatusEx(&raw mut m) }.map_err(api("GlobalMemoryStatusEx"))?;
    Ok((
        m.ullTotalPhys.saturating_sub(m.ullAvailPhys),
        m.ullTotalPhys,
    ))
}

/// GPU utilisation as Task Manager reports it: for each engine of each
/// adapter, the utilisation of all processes is summed; the busiest engine
/// is the result. Uses the `GPU Engine` performance counters (read-only).
/// The same query also reads `GPU Adapter Memory` (dedicated memory in use
/// per adapter) for VRAM usage.
#[derive(Debug)]
pub struct GpuSampler {
    query: PDH_HQUERY,
    engine: PDH_HCOUNTER,
    adapter_memory: Option<PDH_HCOUNTER>,
    buf: Vec<u64>,
}

/// One GPU reading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuSample {
    /// Busiest engine, percent.
    pub utilisation: f32,
    /// Dedicated memory in use on the busiest adapter, bytes.
    pub dedicated_used: Option<u64>,
}

fn pdh(api_name: &'static str, status: u32) -> Result<(), WinError> {
    if status == ERROR_SUCCESS.0 {
        Ok(())
    } else {
        Err(WinError::Api {
            api: api_name,
            code: status,
            message: "PDH call failed".into(),
        })
    }
}

impl GpuSampler {
    pub fn new() -> Result<Self, WinError> {
        let mut query = PDH_HQUERY::default();
        let mut engine = PDH_HCOUNTER::default();
        let mut memory = PDH_HCOUNTER::default();
        // SAFETY: out-pointers to locals; the query is closed in Drop (the
        // value is moved into `Self` before any further fallible call).
        unsafe {
            pdh("PdhOpenQueryW", PdhOpenQueryW(None, 0, &raw mut query))?;
            let mut s = Self {
                query,
                engine,
                adapter_memory: None,
                buf: Vec::new(),
            };
            pdh(
                "PdhAddEnglishCounterW",
                PdhAddEnglishCounterW(
                    s.query,
                    w!("\\GPU Engine(*)\\Utilization Percentage"),
                    0,
                    &raw mut engine,
                ),
            )?;
            s.engine = engine;
            if PdhAddEnglishCounterW(
                s.query,
                w!("\\GPU Adapter Memory(*)\\Dedicated Usage"),
                0,
                &raw mut memory,
            ) == ERROR_SUCCESS.0
            {
                s.adapter_memory = Some(memory);
            }
            // Prime: rate counters need two samples.
            pdh("PdhCollectQueryData", PdhCollectQueryData(s.query))?;
            Ok(s)
        }
    }

    /// Instance names and values of one wildcard counter after a collect.
    fn values(&mut self, counter: PDH_HCOUNTER) -> Result<Vec<(String, f64)>, WinError> {
        // SAFETY: the counter belongs to our query; the buffer is sized as PDH
        // requests and kept 8-byte aligned (Vec<u64>).
        unsafe {
            let mut count = 0u32;
            let mut status = PDH_MORE_DATA;
            for _ in 0..4 {
                let ptr = if self.buf.is_empty() {
                    None
                } else {
                    Some(self.buf.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>())
                };
                let mut size_bytes = u32::try_from(self.buf.len() * 8).unwrap_or(u32::MAX);
                status = PdhGetFormattedCounterArrayW(
                    counter,
                    PDH_FMT_DOUBLE,
                    &raw mut size_bytes,
                    &raw mut count,
                    ptr,
                );
                if status == PDH_MORE_DATA {
                    self.buf.resize((size_bytes as usize).div_ceil(8) + 64, 0);
                    continue;
                }
                break;
            }
            pdh("PdhGetFormattedCounterArrayW", status)?;
            let items = std::slice::from_raw_parts(
                self.buf.as_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>(),
                count as usize,
            );
            Ok(items
                .iter()
                .filter(|i| {
                    let c = i.FmtValue.CStatus;
                    c == PDH_CSTATUS_VALID_DATA || c == PDH_CSTATUS_INVALID_DATA
                })
                .map(|i| {
                    (
                        i.szName.to_string().unwrap_or_default(),
                        i.FmtValue.Anonymous.doubleValue,
                    )
                })
                .filter(|(_, v)| v.is_finite())
                .collect())
        }
    }

    /// Busiest engine's utilisation since the previous call, in percent.
    pub fn sample(&mut self) -> Result<f32, WinError> {
        self.sample_all().map(|s| s.utilisation)
    }

    /// Utilisation and VRAM in one collection.
    pub fn sample_all(&mut self) -> Result<GpuSample, WinError> {
        // SAFETY: collecting our own query.
        pdh("PdhCollectQueryData", unsafe {
            PdhCollectQueryData(self.query)
        })?;
        let mut per_engine: std::collections::HashMap<String, f64> =
            std::collections::HashMap::new();
        for (name, value) in self.values(self.engine)? {
            *per_engine.entry(engine_key(&name).to_owned()).or_default() += value;
        }
        let busiest = per_engine.values().copied().fold(0.0f64, f64::max);
        let dedicated_used = match self.adapter_memory {
            Some(c) => self
                .values(c)?
                .into_iter()
                .map(|(_, v)| v)
                .fold(None, |a: Option<f64>, v| Some(a.map_or(v, |a| a.max(v))))
                .map(|v| v.max(0.0) as u64),
            None => None,
        };
        Ok(GpuSample {
            utilisation: busiest.clamp(0.0, 100.0) as f32,
            dedicated_used,
        })
    }
}

impl Drop for GpuSampler {
    fn drop(&mut self) {
        // SAFETY: closing the query handle opened in `new`.
        unsafe {
            PdhCloseQuery(self.query);
        }
    }
}

/// Dedicated memory of the largest hardware adapter (DXGI), bytes.
pub fn dedicated_video_memory() -> Option<u64> {
    crate::gpu::adapters()
        .ok()?
        .into_iter()
        .filter(|a| !a.is_software)
        .map(|a| a.dedicated_video_memory_bytes)
        .max()
        .filter(|&b| b > 0)
}
/// Strips the process part of a `GPU Engine` instance name:
/// `pid_1234_luid_0x0_0xABC_phys_0_eng_3_engtype_3D` → `luid_…_engtype_3D`.
pub fn engine_key(instance: &str) -> &str {
    instance.find("luid_").map_or(instance, |i| &instance[i..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_key_drops_pid() {
        assert_eq!(
            engine_key("pid_42_luid_0x00000000_0x0000D3C1_phys_0_eng_0_engtype_3D"),
            "luid_0x00000000_0x0000D3C1_phys_0_eng_0_engtype_3D"
        );
        assert_eq!(engine_key("other"), "other");
    }

    /// Real Windows: reads this machine's counters (read-only).
    #[test]
    fn samplers_return_plausible_values() {
        let mut cpu = CpuSampler::new();
        assert_eq!(cpu.sample().unwrap(), None);
        std::thread::sleep(std::time::Duration::from_millis(200));
        let v = cpu.sample().unwrap().expect("second sample");
        assert!((0.0..=100.0).contains(&v));

        let (used, total) = memory_usage().unwrap();
        assert!(total > 0 && used <= total);

        if let Ok(mut gpu) = GpuSampler::new() {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let g = gpu.sample().unwrap();
            assert!((0.0..=100.0).contains(&g));
        }
    }
}
