//! Module update scheduler.
//!
//! Each data source runs on its own worker thread, so a slow query (GPU
//! performance counters, Network List Manager) never delays another source
//! or the UI thread. A source's interval is the shortest interval among the
//! enabled modules that use it; a source no enabled module uses, and every
//! source while all bars are hidden, waits on a condition variable without
//! doing any work. Audio and connectivity are event-driven (Windows
//! callbacks wake the worker); their interval is only a safety re-read.
//! A worker never overlaps itself: it collects, publishes, then waits.
//! Each source records its own failures as unavailable readings.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};
use windowsrice_core::bar::{ModuleConfig, ModuleKind};
use windowsrice_ui::module::{BarReadings, BatteryState, NotificationState, Reading, Traffic};
use windowsrice_ui::panel::{AudioState, LinkKind, MemoryState, NetworkState};
use windowsrice_windows::live::audio::AudioWatcher;
use windowsrice_windows::live::metrics::{
    CpuSampler, GpuSampler, dedicated_video_memory, memory_usage,
};
use windowsrice_windows::live::network::{
    Connectivity, ConnectivityWatcher, LinkType, connectivity, primary_link,
};
use windowsrice_windows::live::power::{self, battery, notification_state};
use windowsrice_windows::live::traffic::TrafficSampler;

pub const WM_APP_READINGS: u32 = WM_APP + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum Source {
    Cpu,
    Memory,
    Gpu,
    Traffic,
    Connectivity,
    Audio,
    Power,
    Notifications,
}

impl Source {
    pub const ALL: [Source; 8] = [
        Source::Cpu,
        Source::Memory,
        Source::Gpu,
        Source::Traffic,
        Source::Connectivity,
        Source::Audio,
        Source::Power,
        Source::Notifications,
    ];

    pub fn of(kind: ModuleKind) -> Option<Source> {
        match kind {
            ModuleKind::Cpu => Some(Source::Cpu),
            ModuleKind::Ram => Some(Source::Memory),
            ModuleKind::Gpu | ModuleKind::Vram => Some(Source::Gpu),
            ModuleKind::Upload | ModuleKind::Download => Some(Source::Traffic),
            ModuleKind::Network => Some(Source::Connectivity),
            ModuleKind::Volume => Some(Source::Audio),
            ModuleKind::Battery => Some(Source::Power),
            ModuleKind::Notifications => Some(Source::Notifications),
            _ => None,
        }
    }
}

/// Interval per source for a module list: the shortest interval of the
/// enabled modules using it, `None` when unused.
pub fn intervals(modules: &[ModuleConfig]) -> Vec<(Source, Option<Duration>)> {
    Source::ALL
        .iter()
        .map(|&s| {
            let ms = modules
                .iter()
                .filter(|m| m.enabled && Source::of(m.kind) == Some(s))
                .filter_map(ModuleConfig::interval_ms)
                .min();
            (s, ms.map(|ms| Duration::from_millis(u64::from(ms))))
        })
        .collect()
}

#[derive(Debug, Default)]
struct Ctl {
    interval: Option<Duration>,
    paused: bool,
    stop: bool,
    wake: bool,
}

type Control = Arc<(Mutex<Ctl>, Condvar)>;

/// Per-source statistics for diagnostics.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct SourceStats {
    pub samples: u64,
    pub errors: u64,
    pub event_wakeups: u64,
    pub last_cost_ms: f64,
    pub max_cost_ms: f64,
    pub last_error: Option<String>,
}

#[derive(Debug)]
struct Worker {
    source: Source,
    control: Control,
    thread: Option<JoinHandle<()>>,
}

#[derive(Debug)]
pub struct Scheduler {
    readings: Arc<Mutex<BarReadings>>,
    stats: Arc<Mutex<Vec<(Source, SourceStats)>>>,
    workers: Vec<Worker>,
}

fn post(hwnd_bits: isize, source: Source) {
    // SAFETY: posting to the controller window; harmless if it is gone.
    unsafe {
        let _ = PostMessageW(
            Some(HWND(hwnd_bits as *mut core::ffi::c_void)),
            WM_APP_READINGS,
            WPARAM(source as usize),
            LPARAM(0),
        );
    }
}

fn wake(control: &Control) {
    let (m, cv) = &**control;
    if let Ok(mut c) = m.lock() {
        c.wake = true;
        cv.notify_all();
    }
}

impl Scheduler {
    /// `gpu_counters: false` (diagnostics option) leaves GPU and VRAM
    /// unavailable without opening performance counters.
    pub fn start(hwnd: HWND, modules: &[ModuleConfig], gpu_counters: bool) -> Self {
        let readings = Arc::new(Mutex::new(BarReadings::default()));
        let stats = Arc::new(Mutex::new(
            Source::ALL
                .iter()
                .map(|&s| (s, SourceStats::default()))
                .collect(),
        ));
        let hwnd_bits = hwnd.0 as isize;
        let mut workers = Vec::new();
        for (source, interval) in intervals(modules) {
            let control: Control = Arc::new((
                Mutex::new(Ctl {
                    interval,
                    ..Default::default()
                }),
                Condvar::new(),
            ));
            let (r, st, c) = (readings.clone(), stats.clone(), control.clone());
            let thread = std::thread::Builder::new()
                .name(format!("windowsrice-{source:?}").to_lowercase())
                .spawn(move || run(source, hwnd_bits, gpu_counters, &r, &st, &c))
                .ok();
            workers.push(Worker {
                source,
                control,
                thread,
            });
        }
        Self {
            readings,
            stats,
            workers,
        }
    }

    /// Applies new module settings (intervals, enabled modules).
    pub fn reconfigure(&self, modules: &[ModuleConfig]) {
        for (source, interval) in intervals(modules) {
            if let Some(w) = self.workers.iter().find(|w| w.source == source) {
                let (m, cv) = &*w.control;
                if let Ok(mut c) = m.lock()
                    && c.interval != interval
                {
                    c.interval = interval;
                    c.wake = true;
                    cv.notify_all();
                }
            }
        }
    }

    /// Pauses every source (all bars hidden) or resumes them.
    pub fn set_paused(&self, paused: bool) {
        for w in &self.workers {
            let (m, cv) = &*w.control;
            if let Ok(mut c) = m.lock()
                && c.paused != paused
            {
                c.paused = paused;
                c.wake = !paused;
                cv.notify_all();
            }
        }
    }

    /// Copy of the current readings (the clock, active window and warnings
    /// are owned by the UI thread and filled in by the caller).
    pub fn readings(&self) -> BarReadings {
        self.readings.lock().map(|r| r.clone()).unwrap_or_default()
    }

    pub fn stats(&self) -> Vec<(Source, SourceStats)> {
        self.stats.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn stop(&mut self) {
        for w in &self.workers {
            let (m, cv) = &*w.control;
            if let Ok(mut c) = m.lock() {
                c.stop = true;
                cv.notify_all();
            }
        }
        for w in &mut self.workers {
            if let Some(t) = w.thread.take() {
                let _ = t.join();
            }
        }
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Waits until the next collection is due. Returns `(stop, by_event)`.
fn wait(control: &Control, last: Option<Instant>) -> (bool, bool) {
    let (m, cv) = &**control;
    let Ok(mut c) = m.lock() else {
        return (true, false);
    };
    loop {
        if c.stop {
            return (true, false);
        }
        if c.wake && !c.paused && c.interval.is_some() {
            c.wake = false;
            return (false, last.is_some());
        }
        match (c.paused, c.interval) {
            (false, Some(iv)) => {
                let due = last.map_or_else(Instant::now, |l| l + iv);
                let now = Instant::now();
                if now >= due {
                    return (false, false);
                }
                c = match cv.wait_timeout(c, due - now) {
                    Ok((g, _)) => g,
                    Err(_) => return (true, false),
                };
            }
            _ => {
                c.wake = false;
                c = match cv.wait(c) {
                    Ok(g) => g,
                    Err(_) => return (true, false),
                };
            }
        }
    }
}

fn err<T>(e: impl std::fmt::Display) -> Reading<T> {
    Reading::Unavailable(e.to_string())
}

fn run(
    source: Source,
    hwnd_bits: isize,
    gpu_counters: bool,
    readings: &Mutex<BarReadings>,
    stats: &Mutex<Vec<(Source, SourceStats)>>,
    control: &Control,
) {
    // SAFETY: COM for this worker thread only; uninitialised before exit.
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let event = Arc::new(AtomicBool::new(false));
    let signal: Arc<dyn Fn() + Send + Sync> = {
        let c = control.clone();
        let e = event.clone();
        Arc::new(move || {
            e.store(true, Ordering::SeqCst);
            wake(&c);
        })
    };
    // Per-source state, created on the worker thread.
    let mut cpu = (source == Source::Cpu).then(CpuSampler::new);
    if let Some(c) = cpu.as_mut() {
        let _ = c.sample();
    }
    let mut gpu = (source == Source::Gpu && gpu_counters).then(GpuSampler::new);
    let vram_total = (source == Source::Gpu)
        .then(dedicated_video_memory)
        .flatten();
    let mut traffic = (source == Source::Traffic).then(TrafficSampler::new);
    if let Some(t) = traffic.as_mut() {
        let _ = t.sample();
    }
    let mut audio = (source == Source::Audio).then(|| AudioWatcher::new(signal.clone()));
    let _net_watch =
        (source == Source::Connectivity).then(|| ConnectivityWatcher::new(signal.clone()));

    let mut last: Option<Instant> = None;
    loop {
        let (stop, _) = wait(control, last);
        if stop {
            break;
        }
        let by_event = event.swap(false, Ordering::SeqCst);
        let t = Instant::now();
        last = Some(t);
        let mut failed: Option<String> = None;
        {
            let mut note = |r: &Reading<()>| {
                if let Reading::Unavailable(e) = r {
                    failed = Some(e.clone());
                }
            };
            let Ok(mut rd) = readings.lock() else { break };
            match source {
                Source::Cpu => {
                    if let Some(c) = cpu.as_mut() {
                        match c.sample() {
                            Ok(Some(v)) => rd.cpu = Reading::Ok(v),
                            Ok(None) => {}
                            Err(e) => {
                                note(&err(&e));
                                rd.cpu = err(e);
                            }
                        }
                    }
                }
                Source::Memory => match memory_usage() {
                    Ok((used, total)) => {
                        rd.ram = Reading::Ok(MemoryState {
                            used_bytes: used,
                            total_bytes: total,
                        });
                    }
                    Err(e) => {
                        note(&err(&e));
                        rd.ram = err(e);
                    }
                },
                Source::Gpu => match gpu.as_mut() {
                    Some(Ok(g)) => match g.sample_all() {
                        Ok(s) => {
                            rd.gpu = Reading::Ok(s.utilisation);
                            rd.vram = match (s.dedicated_used, vram_total) {
                                (Some(used), Some(total)) => Reading::Ok(MemoryState {
                                    used_bytes: used.min(total),
                                    total_bytes: total,
                                }),
                                (None, _) => Reading::Unavailable(
                                    "GPU Adapter Memory counter not available".into(),
                                ),
                                (_, None) => Reading::Unavailable(
                                    "dedicated video memory size not reported".into(),
                                ),
                            };
                        }
                        Err(e) => {
                            note(&err(&e));
                            rd.gpu = err(&e);
                            rd.vram = err(e);
                        }
                    },
                    Some(Err(e)) => {
                        let msg = format!("GPU performance counters unavailable: {e}");
                        note(&err(&msg));
                        rd.gpu = err(&msg);
                        rd.vram = err(msg);
                    }
                    None => {
                        rd.gpu = Reading::Unavailable(
                            "GPU counters turned off (--no-gpu-counter)".into(),
                        );
                        rd.vram = Reading::Unavailable(
                            "GPU counters turned off (--no-gpu-counter)".into(),
                        );
                    }
                },
                Source::Traffic => {
                    if let Some(tr) = traffic.as_mut() {
                        match tr.sample() {
                            Ok(Some((up, down))) => {
                                rd.traffic = Reading::Ok(Traffic {
                                    up_bytes_per_s: up,
                                    down_bytes_per_s: down,
                                });
                            }
                            Ok(None) => {}
                            Err(e) => {
                                note(&err(&e));
                                rd.traffic = err(e);
                            }
                        }
                    }
                }
                Source::Connectivity => match connectivity() {
                    Ok(c) => {
                        rd.network = Reading::Ok(match c {
                            Connectivity::Internet => {
                                NetworkState::Internet(match primary_link() {
                                    Some(LinkType::Wired) => LinkKind::Wired,
                                    Some(LinkType::Wireless) => LinkKind::Wireless,
                                    _ => LinkKind::Other,
                                })
                            }
                            Connectivity::LocalOnly => NetworkState::LocalOnly,
                            Connectivity::Disconnected => NetworkState::Disconnected,
                        });
                    }
                    Err(e) => {
                        note(&err(&e));
                        rd.network = err(e);
                    }
                },
                Source::Audio => match audio.as_mut() {
                    Some(Ok(w)) => match w.read() {
                        Ok(Some(v)) => {
                            rd.audio = Reading::Ok(AudioState {
                                volume_percent: v.percent,
                                muted: v.muted,
                            });
                        }
                        Ok(None) => rd.audio = Reading::Absent,
                        Err(e) => {
                            note(&err(&e));
                            rd.audio = err(e);
                        }
                    },
                    Some(Err(e)) => {
                        note(&err(&*e));
                        rd.audio = err(&*e);
                    }
                    None => {}
                },
                Source::Power => match battery() {
                    Ok(None) => rd.battery = Reading::Absent,
                    Ok(Some(b)) => {
                        rd.battery = Reading::Ok(BatteryState {
                            percent: b.percent,
                            charging: b.charging,
                            on_ac: b.on_ac,
                        });
                    }
                    Err(e) => {
                        note(&err(&e));
                        rd.battery = err(e);
                    }
                },
                Source::Notifications => match notification_state() {
                    Ok(s) => {
                        rd.notifications = Reading::Ok(match s {
                            power::NotificationState::Accepting => NotificationState::Accepting,
                            power::NotificationState::QuietTime => NotificationState::DoNotDisturb,
                            power::NotificationState::Suppressed => NotificationState::Suppressed,
                        });
                    }
                    Err(e) => {
                        note(&err(&e));
                        rd.notifications = err(e);
                    }
                },
            }
        }
        let cost = t.elapsed().as_secs_f64() * 1000.0;
        if let Ok(mut st) = stats.lock()
            && let Some((_, s)) = st.iter_mut().find(|(s, _)| *s == source)
        {
            s.samples += 1;
            s.last_cost_ms = cost;
            s.max_cost_ms = s.max_cost_ms.max(cost);
            if by_event {
                s.event_wakeups += 1;
            }
            if let Some(e) = failed {
                s.errors += 1;
                s.last_error = Some(e);
            }
        }
        post(hwnd_bits, source);
    }
    drop(audio.take());
    drop(gpu.take());
    if com {
        // SAFETY: balances CoInitializeEx above.
        unsafe { CoUninitialize() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windowsrice_core::bar::default_modules;

    #[test]
    fn intervals_follow_enabled_modules() {
        let mut m = default_modules();
        let iv = intervals(&m);
        let get =
            |iv: &[(Source, Option<Duration>)], s| iv.iter().find(|(x, _)| *x == s).unwrap().1;
        assert_eq!(get(&iv, Source::Cpu), Some(Duration::from_secs(2)));
        // GPU source serves GPU (2 s) and VRAM (5 s): the shorter wins.
        assert_eq!(get(&iv, Source::Gpu), Some(Duration::from_secs(2)));
        for x in &mut m {
            if x.kind == ModuleKind::Gpu {
                x.enabled = false;
            }
        }
        assert_eq!(
            get(&intervals(&m), Source::Gpu),
            Some(Duration::from_secs(5))
        );
        for x in &mut m {
            if x.kind == ModuleKind::Cpu {
                x.enabled = false;
            }
        }
        assert_eq!(
            get(&intervals(&m), Source::Cpu),
            None,
            "unused source does no work"
        );
    }
}
