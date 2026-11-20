//! Diagnostics collected during a run and written as JSON with `--stats`.

use std::time::Instant;

use serde::Serialize;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

use crate::window::ProcessCounters;

#[derive(Debug, Default, Serialize)]
pub struct Stats {
    pub version: &'static str,
    pub args: Vec<String>,
    pub render: serde_json::Value,
    pub text: serde_json::Value,
    pub monitor: serde_json::Value,
    pub theme: serde_json::Value,
    pub surface: serde_json::Value,
    pub warnings: Vec<String>,
    pub ms_process_to_window: Option<f64>,
    pub ms_process_to_first_frame: Option<f64>,
    /// Startup milestones: name and milliseconds since process creation.
    pub startup_milestones: Vec<(&'static str, f64)>,
    /// Seconds since start for every presented frame.
    pub frames: Vec<f64>,
    pub frame_reasons: Vec<&'static str>,
    pub frame_draw_ms: Vec<f64>,
    pub frame_present_ms: Vec<f64>,
    /// Input event (key, click or UI Automation action) to presented frame.
    pub input_latency_ms: Vec<f64>,
    pub uia_provider_calls: u64,
    pub uia_events_raised: u64,
    pub uia_focus_events: u64,
    pub a11y_problems: Vec<String>,
    pub menu_opens: u32,
    pub hotkey_registered: bool,
    pub focus_log: Vec<String>,
    pub suspend_log: Vec<String>,
    pub frames_while_hidden: u32,
    pub samples: u64,
    pub samples_while_hidden: u32,
    pub sample_cost_ms: Vec<f64>,
    pub device_recoveries: u32,
    pub recovery_failures: u32,
    pub resources_start: ProcessCounters,
    pub resources_end: ProcessCounters,
    /// Counters after each simulated device loss (cleanup stress test).
    pub resources_after_recovery: Vec<ProcessCounters>,
    pub dpi_events: Vec<String>,
    /// One entry per bar created: device, rectangles, DPI, AppBar state.
    pub bars: Vec<serde_json::Value>,
    pub bars_destroyed: Vec<String>,
    pub appbar_events: Vec<String>,
    pub autohide_events: Vec<String>,
    pub popup_log: Vec<String>,
    /// Time to build each Design Studio scene, microseconds.
    pub studio_build_us: Vec<u64>,
    pub config_saves: Vec<String>,
    pub config_path: String,
    pub scheduler: serde_json::Value,
    pub hidden_modules: Vec<String>,
    pub display_events: Vec<String>,
    pub errors: Vec<String>,
    pub exit_reason: String,
    #[serde(skip)]
    pub started: Option<Instant>,
}

impl Stats {
    pub fn now_s(&self) -> f64 {
        self.started.map_or(0.0, |s| s.elapsed().as_secs_f64())
    }

    pub fn write(&self, path: &std::path::Path) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}

fn ft(t: FILETIME) -> u64 {
    (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)
}

/// Milliseconds since this process was created (precise clock, backed by
/// QueryPerformanceCounter; the coarse system clock ticks every ~15.6 ms).
pub fn ms_since_process_start() -> Option<f64> {
    let (mut c, mut e, mut k, mut u) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: queries about the current process into locals.
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &raw mut c,
            &raw mut e,
            &raw mut k,
            &raw mut u,
        )
        .ok()?;
        let now = GetSystemTimePreciseAsFileTime();
        Some(ft(now).saturating_sub(ft(c)) as f64 / 10_000.0)
    }
}

static MARKS: std::sync::Mutex<Vec<(&'static str, f64)>> = std::sync::Mutex::new(Vec::new());

/// Records a startup milestone (cheap: one clock read and a push). The
/// list is copied into the `--stats` output after the first frame.
pub fn mark(name: &'static str) {
    if let (Some(ms), Ok(mut m)) = (ms_since_process_start(), MARKS.lock()) {
        m.push((name, ms));
    }
}

/// The milestones recorded so far.
pub fn marks() -> Vec<(&'static str, f64)> {
    MARKS.lock().map(|m| m.clone()).unwrap_or_default()
}
