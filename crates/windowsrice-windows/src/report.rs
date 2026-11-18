//! Aggregated read-only diagnostics report.
//!
//! Each section is collected independently; a failure in one section is
//! recorded as `unavailable` and never prevents the other sections.

use serde::Serialize;
use windowsrice_core::capability::{Capability, PlatformFacts, evaluate};

use crate::display::{DisplayInfo, DpiAwarenessStatus, displays, enable_per_monitor_v2_awareness};
use crate::error::WinError;
use crate::gpu::{AdlxPresence, GpuAdapter, adapters, adlx_presence};
use crate::session::{SessionInfo, session_info};
use crate::system_info::{CpuInfo, MemoryInfo, OsInfo, cpu_info, memory_info, os_info};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", content = "data", rename_all = "snake_case")]
pub enum Section<T> {
    Available(T),
    Unavailable { error: String },
}

impl<T> Section<T> {
    pub fn as_ref(&self) -> Option<&T> {
        match self {
            Section::Available(v) => Some(v),
            Section::Unavailable { .. } => None,
        }
    }
}

impl<T> From<Result<T, WinError>> for Section<T> {
    fn from(r: Result<T, WinError>) -> Self {
        match r {
            Ok(v) => Section::Available(v),
            Err(e) => Section::Unavailable {
                error: e.to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticsReport {
    pub tool: &'static str,
    pub windowsrice_version: &'static str,
    pub generated_at_unix_ms: u64,
    /// Always true in Phase 1: collection performs no system changes.
    pub read_only: bool,
    pub dpi_awareness: DpiAwarenessStatus,
    pub os: Section<OsInfo>,
    pub cpu: Section<CpuInfo>,
    pub memory: Section<MemoryInfo>,
    pub displays: Section<Vec<DisplayInfo>>,
    pub gpus: Section<Vec<GpuAdapter>>,
    pub amd_adlx: Section<AdlxPresence>,
    pub session: SessionInfo,
    pub capabilities: Section<Vec<Capability>>,
}

/// Collects the full report. Opts this process into Per-Monitor-v2 DPI
/// awareness first so display values are physical pixels.
pub fn collect() -> DiagnosticsReport {
    let dpi_awareness = enable_per_monitor_v2_awareness();
    let os: Section<OsInfo> = os_info().into();
    let capabilities = match os.as_ref() {
        Some(os) => Section::Available(evaluate(&PlatformFacts {
            build: os.build,
            edition: os.edition,
        })),
        None => Section::Unavailable {
            error: "OS information unavailable".into(),
        },
    };
    DiagnosticsReport {
        tool: "windowsrice-diag",
        windowsrice_version: windowsrice_core::VERSION,
        generated_at_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0),
        read_only: true,
        dpi_awareness,
        cpu: cpu_info().into(),
        memory: memory_info().into(),
        displays: displays().into(),
        gpus: adapters().into(),
        amd_adlx: adlx_presence().into(),
        session: session_info(),
        capabilities,
        os,
    }
}
