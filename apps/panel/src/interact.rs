//! Interactive bar modules (Phase 13): clicking a module (or pressing Enter
//! on it) opens a panel with details, the calendar or the notification
//! center. Values come from the same providers as the bar; anything that
//! is not measured is shown as unavailable.

use std::collections::HashMap;

use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{SW_SHOWNORMAL, WS_EX_TOOLWINDOW, WS_EX_TOPMOST};
use windows::core::{HSTRING, w};
use windowsrice_core::bar::ModuleKind;
use windowsrice_render::TextSystem;
use windowsrice_theme::Mode;
use windowsrice_ui::format::{date_text, gigabytes, time_text};
use windowsrice_ui::module::{Reading, bit_rate};
use windowsrice_ui::panel::{LinkKind, NetworkState};
use windowsrice_ui::panels::{
    AppNotice, CalendarView, History, NotificationAccess, NotificationView, PANEL_WIDTH,
    PanelAction, PanelKind, PanelModel, Ring, add_months, build_panel,
};
use windowsrice_ui::popup::PopupAction;
use windowsrice_ui::strings::Lang;
use windowsrice_ui::tree::{ElementId, Role};
use windowsrice_ui::{PhysRect, Rect};
use windowsrice_windows::live::monitors;

use crate::controller::{CLASS_POPUP, Controller, Deferred, PopupWin, Surface, font_config, phys};
use crate::window;

/// Samples kept for each graph.
const HISTORY: usize = 60;
/// Most windowsRICE notices kept.
const MAX_NOTICES: usize = 20;

/// Why the notification center cannot read Windows notifications.
const NOTIFY_EN: &str = "Windows lets an app read and count notifications only when the app has package identity (an MSIX package declaring the User Notification Listener capability) and you grant it notification access. windowsRICE is installed as an unpackaged per-user app, so it cannot see Windows notifications and never guesses their number. Open Windows notifications to see them.";
const NOTIFY_KO: &str = "Windows는 패키지 ID(사용자 알림 수신기 기능을 선언한 MSIX 패키지)가 있고 사용자가 알림 접근을 허용한 앱만 알림을 읽고 셀 수 있게 합니다. windowsRICE는 패키지 없이 사용자별로 설치되므로 Windows 알림을 볼 수 없으며 그 수를 추측하지 않습니다. Windows 알림 열기로 확인하세요.";

#[derive(Debug, Default)]
pub(crate) struct Interactive {
    history: HashMap<ModuleKind, Ring>,
    calendar_shown: Option<(i32, u32)>,
    calendar_selected: Option<(i32, u32, u32)>,
    notices: Vec<AppNotice>,
    next_notice: u32,
    /// Interface octets (received, sent) when traffic was first measured.
    octets_start: Option<(u64, u64)>,
    cpu: Option<windowsrice_windows::system_info::CpuInfo>,
    gpu_name: Option<Option<String>>,
}

impl Interactive {
    pub(crate) fn unread(&self) -> usize {
        self.notices.iter().filter(|n| n.unread).count()
    }
}

fn pct(used: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        used as f32 / total as f32 * 100.0
    }
}

/// Opens an allowlisted Windows page (user-initiated; nothing else can be
/// launched from a panel).
fn open_uri(uri: &str) {
    const ALLOWED: [&str; 4] = [
        "ms-clock:",
        "ms-actioncenter:",
        "ms-settings:notifications",
        "ms-settings:sound",
    ];
    if !ALLOWED.contains(&uri) {
        return;
    }
    // SAFETY: opens a fixed shell URI with the default handler.
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(uri),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
}

/// Sets the default output's volume (0–100) or toggles mute. A direct user
/// action from the Volume panel. Requires COM on the calling thread.
fn set_volume(percent: Option<u8>, toggle_mute: bool) -> windows::core::Result<()> {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender,
    };
    use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
    // SAFETY: COM calls on interfaces we own; no out-pointers.
    unsafe {
        let e: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let dev = e.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let v: IAudioEndpointVolume = dev.Activate(CLSCTX_ALL, None)?;
        if let Some(p) = percent {
            v.SetMasterVolumeLevelScalar(f32::from(p.min(100)) / 100.0, std::ptr::null())?;
        }
        if toggle_mute {
            let m = v.GetMute()?.as_bool();
            v.SetMute(!m, std::ptr::null())?;
        }
    }
    Ok(())
}

impl Controller {
    /// Records graph history after a provider sample (by scheduler source).
    pub(crate) fn record_history(&mut self, source: usize) {
        use crate::scheduler::Source;
        let r = &self.readings;
        let mut push = |k: ModuleKind, v: Option<f32>| {
            if let Some(v) = v {
                self.ix
                    .history
                    .entry(k)
                    .or_insert_with(|| Ring::new(HISTORY))
                    .push(v);
            }
        };
        match Source::ALL.get(source) {
            Some(Source::Cpu) => push(
                ModuleKind::Cpu,
                match &r.cpu {
                    Reading::Ok(v) => Some(*v),
                    _ => None,
                },
            ),
            Some(Source::Memory) => push(
                ModuleKind::Ram,
                match &r.ram {
                    Reading::Ok(m) => Some(pct(m.used_bytes, m.total_bytes)),
                    _ => None,
                },
            ),
            Some(Source::Gpu) => {
                let g = match &r.gpu {
                    Reading::Ok(v) => Some(*v),
                    _ => None,
                };
                let v = match &r.vram {
                    Reading::Ok(m) => Some(pct(m.used_bytes, m.total_bytes)),
                    _ => None,
                };
                push(ModuleKind::Gpu, g);
                push(ModuleKind::Vram, v);
            }
            Some(Source::Traffic) if self.ix.octets_start.is_none() => {
                self.ix.octets_start = windowsrice_windows::live::traffic::interface_octets().ok();
            }
            _ => {}
        }
    }

    /// Adds one of windowsRICE's own notices (kept apart from Windows
    /// notifications) and updates the badge.
    pub(crate) fn notice(&mut self, title: &str, body: &str) {
        let time = self
            .readings
            .time
            .as_ref()
            .map(time_text)
            .unwrap_or_default();
        self.ix.next_notice += 1;
        let id = self.ix.next_notice;
        self.ix.notices.insert(
            0,
            AppNotice {
                id,
                title: title.to_owned(),
                body: body.to_owned(),
                time,
                unread: true,
            },
        );
        self.ix.notices.truncate(MAX_NOTICES);
        self.readings.app_unread = self.ix.unread();
        self.rebuild_all("notice");
    }

    /// The module a bar element shows, when it opens a panel.
    fn module_kind_at(&self, i: usize, id: ElementId) -> Option<ModuleKind> {
        let st = self.bars[i].s.host.state.borrow();
        let e = st.scene.tree.get(id)?;
        if e.role != Role::Button {
            return None;
        }
        let mid = e.automation_id.strip_prefix("module.")?;
        let kind = self.config().bar.modules.iter().find(|m| m.id == mid)?.kind;
        windowsrice_ui::module::has_panel(kind).then_some(kind)
    }

    /// A click or Enter on a bar module: opens (or closes) its panel.
    pub(crate) fn module_activated(&mut self, i: usize, id: ElementId) -> Deferred {
        let Some(kind) = self.module_kind_at(i, id) else {
            return Deferred::Nothing;
        };
        let pk = match kind {
            ModuleKind::Clock => PanelKind::Calendar,
            ModuleKind::Notifications => PanelKind::Notifications,
            ModuleKind::Upload | ModuleKind::Download => PanelKind::Detail(ModuleKind::Network),
            k => PanelKind::Detail(k),
        };
        let owner = self.bars[i].s.hwnd;
        if self
            .popup
            .as_ref()
            .is_some_and(|p| p.owner == owner && p.kind == Some(pk))
        {
            return self.close_popup(false);
        }
        self.bars[i].s.focus = Some(id);
        let anchor = self.bars[i]
            .s
            .host
            .state
            .borrow()
            .scene
            .tree
            .get(id)
            .map(|e| e.bounds);
        self.open_panel(i, pk, anchor)
    }

    fn open_panel(&mut self, i: usize, kind: PanelKind, anchor: Option<Rect>) -> Deferred {
        if self.popup.is_some() {
            self.close_popup_now();
        }
        if kind == PanelKind::Calendar {
            self.ix.calendar_shown = None;
            self.ix.calendar_selected = None;
        }
        if kind == PanelKind::Notifications {
            for n in &mut self.ix.notices {
                n.unread = false;
            }
            self.readings.app_unread = 0;
            self.rebuild_all("notices read");
        }
        let owner = self.bars[i].s.hwnd;
        let hwnd = match window::create(
            CLASS_POPUP,
            w!("windowsRICE panel"),
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            Some(owner),
        ) {
            Ok(h) => h,
            Err(e) => {
                self.stats.errors.push(format!("panel window: {e}"));
                return Deferred::Nothing;
            }
        };
        let text = match TextSystem::new(font_config(&self.style, self.strings.lang)) {
            Ok(t) => t,
            Err(e) => {
                self.stats.errors.push(format!("panel DirectWrite: {e}"));
                return Deferred::Nothing;
            }
        };
        let placeholder = self.bars[i].s.host.state.borrow().scene.clone();
        let mut s = match Surface::new(hwnd, text, "windowsRICE.Panel", placeholder) {
            Ok(s) => s,
            Err(e) => {
                self.stats.errors.push(e);
                return Deferred::Nothing;
            }
        };
        let b = &self.bars[i];
        s.scale = b.s.scale;
        s.dpi = b.s.dpi;
        let left = anchor.map_or(b.rect.left, |r| {
            r.to_physical(b.s.scale, (b.rect.left, b.rect.top)).left
        });
        let work = monitors::monitor_info(b.monitor.handle)
            .map_or(phys(&b.monitor.rect), |m| phys(&m.work));
        let width = (PANEL_WIDTH * s.scale).round() as i32;
        let x = left.min(work.right - width).max(work.left);
        let y = b.rect.bottom + (4.0 * s.scale).round() as i32;
        let dark = self.theme.applied().is_some_and(|t| t.mode == Mode::Dark);
        window::apply_dwm(hwnd, windowsrice_theme::schema::Backdrop::None, dark, true);
        self.popup = Some(PopupWin {
            s,
            owner,
            origin: (x, y),
            size: PhysRect {
                left: x,
                top: y,
                right: x + width,
                bottom: y + 1,
            },
            actions: Vec::new(),
            kind: Some(kind),
        });
        self.stats
            .popup_log
            .push(format!("{:.2}s panel {kind:?}", self.stats.now_s()));
        self.update_pause();
        self.rebuild_popup();
        self.render_all();
        Deferred::Activate(hwnd.0 as isize)
    }

    fn tx(&self, en: &str, ko: &str) -> String {
        if self.strings.lang == Lang::En {
            en.to_owned()
        } else {
            ko.to_owned()
        }
    }

    /// The model of an open panel, from current readings.
    pub(crate) fn panel_model(&mut self, kind: PanelKind) -> PanelModel {
        let lang = self.strings.lang;
        let r = self.readings.clone();
        let hist = |ix: &Interactive, k: ModuleKind, interval: f32| {
            ix.history
                .get(&k)
                .filter(|h| !h.is_empty())
                .map(|h| History {
                    values: h.values(),
                    interval_s: interval,
                })
        };
        let na = |s: &str| -> Option<String> {
            let _ = s;
            None
        };
        match kind {
            PanelKind::Detail(ModuleKind::Cpu) => {
                if self.ix.cpu.is_none() {
                    self.ix.cpu = windowsrice_windows::system_info::cpu_info().ok();
                }
                let c = self.ix.cpu.clone();
                let mut m = PanelModel::new(kind, "CPU");
                m.value = match &r.cpu {
                    Reading::Ok(v) => Some(format!("{:.0}%", v)),
                    _ => None,
                };
                m.history = hist(&self.ix, ModuleKind::Cpu, 2.0);
                m.rows = vec![
                    (
                        self.tx("Processor", "프로세서"),
                        c.as_ref().and_then(|c| c.name.clone()),
                    ),
                    (
                        self.tx("Cores / threads", "코어 / 스레드"),
                        c.as_ref()
                            .and_then(|c| match (c.physical_cores, c.logical_processors) {
                                (Some(p), Some(l)) => Some(format!("{p} / {l}")),
                                _ => None,
                            }),
                    ),
                    (
                        self.tx("Base frequency", "기본 클럭"),
                        c.as_ref()
                            .and_then(|c| c.reported_base_mhz)
                            .map(|mhz| format!("{:.2} GHz", f64::from(mhz) / 1000.0)),
                    ),
                    (
                        self.tx("Per-core usage", "코어별 사용률"),
                        na("not measured"),
                    ),
                    (
                        self.tx("Current frequency", "현재 클럭"),
                        na("not measured"),
                    ),
                ];
                m.note = Some(self.tx(
                    "Per-core usage and the current frequency are not measured by this version.",
                    "이 버전은 코어별 사용률과 현재 클럭을 측정하지 않습니다.",
                ));
                m
            }
            PanelKind::Detail(ModuleKind::Gpu) | PanelKind::Detail(ModuleKind::Vram) => {
                if self.ix.gpu_name.is_none() {
                    self.ix.gpu_name =
                        Some(windowsrice_windows::gpu::adapters().ok().and_then(|a| {
                            a.into_iter()
                                .find(|x| !x.is_software)
                                .map(|x| x.description)
                        }));
                }
                let vram = match &r.vram {
                    Reading::Ok(m) => Some(*m),
                    _ => None,
                };
                let gpu_kind = matches!(kind, PanelKind::Detail(ModuleKind::Gpu));
                let mut m = PanelModel::new(kind, if gpu_kind { "GPU" } else { "VRAM" });
                m.value = if gpu_kind {
                    match &r.gpu {
                        Reading::Ok(v) => Some(format!("{:.0}%", v)),
                        _ => None,
                    }
                } else {
                    vram.map(|v| format!("{:.0}%", pct(v.used_bytes, v.total_bytes)))
                };
                m.history = hist(
                    &self.ix,
                    if gpu_kind {
                        ModuleKind::Gpu
                    } else {
                        ModuleKind::Vram
                    },
                    2.0,
                );
                m.rows = vec![
                    (
                        self.tx("Adapter", "어댑터"),
                        self.ix.gpu_name.clone().flatten(),
                    ),
                    (
                        self.tx("Video memory used", "사용 중인 비디오 메모리"),
                        vram.map(|v| gigabytes(v.used_bytes)),
                    ),
                    (
                        self.tx("Video memory total", "전체 비디오 메모리"),
                        vram.map(|v| gigabytes(v.total_bytes)),
                    ),
                    (
                        self.tx("Video memory free", "남은 비디오 메모리"),
                        vram.map(|v| gigabytes(v.total_bytes.saturating_sub(v.used_bytes))),
                    ),
                    (self.tx("Temperature", "온도"), na("no documented API")),
                ];
                m.note = Some(self.tx(
                    "GPU temperature has no documented Windows interface, so it is not shown.",
                    "GPU 온도는 문서화된 Windows 인터페이스가 없어 표시하지 않습니다.",
                ));
                m
            }
            PanelKind::Detail(ModuleKind::Ram) => {
                let ram = match &r.ram {
                    Reading::Ok(m) => Some(*m),
                    _ => None,
                };
                let mut m = PanelModel::new(kind, self.tx("Memory", "메모리"));
                m.value = ram.map(|v| format!("{:.0}%", pct(v.used_bytes, v.total_bytes)));
                m.history = hist(&self.ix, ModuleKind::Ram, 2.0);
                m.rows = vec![
                    (
                        self.tx("Used", "사용 중"),
                        ram.map(|v| gigabytes(v.used_bytes)),
                    ),
                    (
                        self.tx("Available", "사용 가능"),
                        ram.map(|v| gigabytes(v.total_bytes.saturating_sub(v.used_bytes))),
                    ),
                    (
                        self.tx("Total", "전체"),
                        ram.map(|v| gigabytes(v.total_bytes)),
                    ),
                ];
                m
            }
            PanelKind::Detail(ModuleKind::Network) => {
                let tr = match &r.traffic {
                    Reading::Ok(t) => Some(*t),
                    _ => None,
                };
                let totals = match (
                    self.ix.octets_start,
                    windowsrice_windows::live::traffic::interface_octets().ok(),
                ) {
                    (Some((r0, s0)), Some((r1, s1))) => {
                        Some((r1.saturating_sub(r0), s1.saturating_sub(s0)))
                    }
                    _ => None,
                };
                let link = match &r.network {
                    Reading::Ok(NetworkState::Internet(k)) => Some(
                        match (k, lang) {
                            (LinkKind::Wired, Lang::En) => "Internet (wired)",
                            (LinkKind::Wireless, Lang::En) => "Internet (Wi-Fi)",
                            (LinkKind::Other, Lang::En) => "Internet",
                            (LinkKind::Wired, Lang::Ko) => "인터넷 (유선)",
                            (LinkKind::Wireless, Lang::Ko) => "인터넷 (Wi-Fi)",
                            (LinkKind::Other, Lang::Ko) => "인터넷",
                        }
                        .to_owned(),
                    ),
                    Reading::Ok(NetworkState::LocalOnly) => {
                        Some(self.tx("Local network only", "로컬 네트워크만"))
                    }
                    Reading::Ok(NetworkState::Disconnected) => {
                        Some(self.tx("Disconnected", "연결 안 됨"))
                    }
                    _ => None,
                };
                let mut m = PanelModel::new(kind, self.tx("Network", "네트워크"));
                m.value = tr.map(|t| {
                    format!(
                        "↓ {}  ↑ {}",
                        bit_rate(t.down_bytes_per_s),
                        bit_rate(t.up_bytes_per_s)
                    )
                });
                m.rows = vec![
                    (self.tx("Connection", "연결"), link),
                    (
                        self.tx("Download now", "현재 다운로드"),
                        tr.map(|t| bit_rate(t.down_bytes_per_s)),
                    ),
                    (
                        self.tx("Upload now", "현재 업로드"),
                        tr.map(|t| bit_rate(t.up_bytes_per_s)),
                    ),
                    (
                        self.tx("Received since start", "시작 후 받은 양"),
                        totals.map(|t| gigabytes(t.0)),
                    ),
                    (
                        self.tx("Sent since start", "시작 후 보낸 양"),
                        totals.map(|t| gigabytes(t.1)),
                    ),
                ];
                m.note = Some(self.tx(
                    "Totals count all network adapters since windowsRICE started measuring.",
                    "합계는 windowsRICE가 측정을 시작한 뒤 모든 네트워크 어댑터의 양입니다.",
                ));
                m
            }
            PanelKind::Detail(ModuleKind::Volume) => {
                let mut m = PanelModel::new(kind, self.tx("Volume", "볼륨"));
                if let Reading::Ok(a) = &r.audio {
                    m.value = Some(format!("{}%", a.volume_percent));
                    m.volume = Some((a.volume_percent, a.muted));
                } else {
                    m.note = Some(self.tx(
                        "No audio output device is available.",
                        "사용할 수 있는 오디오 출력 장치가 없습니다.",
                    ));
                }
                m.rows = vec![(
                    self.tx("Output device", "출력 장치"),
                    Some(self.tx("Windows default output", "Windows 기본 출력")),
                )];
                m
            }
            PanelKind::Detail(ModuleKind::Battery) => {
                let mut m = PanelModel::new(kind, self.tx("Battery", "배터리"));
                match &r.battery {
                    Reading::Ok(b) => {
                        m.value = b.percent.map(|p| format!("{p}%"));
                        m.rows = vec![
                            (
                                self.tx("Power", "전원"),
                                Some(if b.on_ac {
                                    self.tx("Plugged in", "전원 연결됨")
                                } else {
                                    self.tx("On battery", "배터리 사용 중")
                                }),
                            ),
                            (
                                self.tx("Charging", "충전"),
                                Some(if b.charging {
                                    self.tx("Yes", "예")
                                } else {
                                    self.tx("No", "아니요")
                                }),
                            ),
                        ];
                    }
                    Reading::Absent => {
                        m.note = Some(self.tx(
                            "This computer has no battery.",
                            "이 컴퓨터에는 배터리가 없습니다.",
                        ))
                    }
                    _ => {
                        m.note = Some(self.tx(
                            "The battery state is not available.",
                            "배터리 상태를 사용할 수 없습니다.",
                        ))
                    }
                }
                m
            }
            PanelKind::Calendar => {
                let mut m = PanelModel::new(kind, self.tx("Calendar", "달력"));
                if let Some(t) = r.time {
                    let today = (i32::from(t.year), u32::from(t.month), u32::from(t.day));
                    let zones = self
                        .config()
                        .bar
                        .modules
                        .iter()
                        .find(|x| x.kind == ModuleKind::Clock)
                        .and_then(|x| x.zones.clone())
                        .unwrap_or_default()
                        .iter()
                        .filter_map(|z| windowsrice_windows::live::clock::zone_time(z))
                        .map(|(n, h, mi)| (n, format!("{h:02}:{mi:02}")))
                        .collect();
                    m.calendar = Some(CalendarView {
                        today,
                        shown: self.ix.calendar_shown.unwrap_or((today.0, today.1)),
                        selected: self.ix.calendar_selected,
                        monday_first: true,
                        time: time_text(&t),
                        date: date_text(&t, lang),
                        zones,
                    });
                }
                m
            }
            PanelKind::Notifications => {
                let mut m = PanelModel::new(kind, self.tx("Notifications", "알림"));
                let state = match (&r.notifications, lang) {
                    (
                        Reading::Ok(windowsrice_ui::module::NotificationState::Accepting),
                        Lang::En,
                    ) => "Windows notifications: on".to_owned(),
                    (
                        Reading::Ok(windowsrice_ui::module::NotificationState::DoNotDisturb),
                        Lang::En,
                    ) => "Windows notifications: do not disturb".to_owned(),
                    (
                        Reading::Ok(windowsrice_ui::module::NotificationState::Suppressed),
                        Lang::En,
                    ) => "Windows notifications: paused by a fullscreen app".to_owned(),
                    (
                        Reading::Ok(windowsrice_ui::module::NotificationState::Accepting),
                        Lang::Ko,
                    ) => "Windows 알림: 켜짐".to_owned(),
                    (
                        Reading::Ok(windowsrice_ui::module::NotificationState::DoNotDisturb),
                        Lang::Ko,
                    ) => "Windows 알림: 방해 금지".to_owned(),
                    (
                        Reading::Ok(windowsrice_ui::module::NotificationState::Suppressed),
                        Lang::Ko,
                    ) => "Windows 알림: 전체 화면 앱이 일시 중지함".to_owned(),
                    _ => self.tx(
                        "Windows notification state: not available",
                        "Windows 알림 상태: 사용할 수 없음",
                    ),
                };
                m.notifications = Some(NotificationView {
                    access: NotificationAccess::Unavailable(self.tx(NOTIFY_EN, NOTIFY_KO)),
                    windows_state: state,
                    app: self.ix.notices.clone(),
                });
                m
            }
            PanelKind::Detail(other) => PanelModel::new(kind, format!("{other:?}")),
        }
    }

    /// Rebuilds an open panel (live values).
    pub(crate) fn rebuild_panel(&mut self, kind: PanelKind) {
        let mut model = self.panel_model(kind);
        let above = self.popup.as_ref().and_then(|p| {
            self.bars
                .iter()
                .find(|b| b.s.hwnd == p.owner && b.bottom)
                .map(|b| b.rect.top)
        });
        let Some(p) = self.popup.as_mut() else { return };
        model.hover = p.s.hover;
        let ps = build_panel(
            &model,
            &self.style,
            p.s.renderer.text(),
            p.s.scale,
            self.strings.lang,
        );
        p.actions = ps
            .actions
            .into_iter()
            .map(|(e, a)| (e, PopupAction::Panel(a)))
            .collect();
        let problems = ps.scene.tree.problems();
        if !problems.is_empty() {
            self.stats.a11y_problems = problems;
        }
        let h = (ps.scene.size.h * p.s.scale).round() as i32;
        // Above a bottom bar (Phase 14), below a top bar.
        if let Some(bar_top) = above {
            let top = bar_top - (4.0 * p.s.scale).round() as i32 - h;
            if top != p.size.top {
                p.size.top = top;
                p.origin.1 = top;
                p.size.bottom = top + h - 1;
            }
        }
        if h != p.size.height() {
            p.size.bottom = p.size.top + h;
            window::place(p.s.hwnd, p.size);
            let _ =
                p.s.renderer
                    .resize(p.size.width().max(1) as u32, h.max(1) as u32, p.s.dpi);
        }
        self.stats.uia_events_raised += u64::from(p.s.set_scene(ps.scene, p.origin, "panel"));
    }

    pub(crate) fn panel_action(&mut self, a: PanelAction) -> Deferred {
        let kind = self.popup.as_ref().and_then(|p| p.kind);
        let today = self
            .readings
            .time
            .map(|t| (i32::from(t.year), u32::from(t.month), u32::from(t.day)));
        match a {
            PanelAction::Close => return self.close_popup(true),
            PanelAction::PrevMonth | PanelAction::NextMonth => {
                let (y, m) = self
                    .ix
                    .calendar_shown
                    .or(today.map(|t| (t.0, t.1)))
                    .unwrap_or((2026, 1));
                self.ix.calendar_shown = Some(add_months(
                    y,
                    m,
                    if a == PanelAction::PrevMonth { -1 } else { 1 },
                ));
            }
            PanelAction::Today => {
                self.ix.calendar_shown = None;
                self.ix.calendar_selected = today;
            }
            PanelAction::SelectDay(y, m, d) => self.ix.calendar_selected = Some((y, m, d)),
            PanelAction::OpenWindowsClock => open_uri("ms-clock:"),
            PanelAction::OpenNotificationCenter => open_uri("ms-actioncenter:"),
            PanelAction::OpenNotificationSettings => open_uri("ms-settings:notifications"),
            PanelAction::VolumeStep(d) => {
                if let Reading::Ok(au) = &self.readings.audio {
                    let v = (i32::from(au.volume_percent) + i32::from(d)).clamp(0, 100) as u8;
                    if let Err(e) = set_volume(Some(v), false) {
                        self.stats.errors.push(format!("volume: {e}"));
                    }
                }
            }
            PanelAction::ToggleMute => {
                if let Err(e) = set_volume(None, true) {
                    self.stats.errors.push(format!("mute: {e}"));
                }
            }
            PanelAction::ReadAppNotices => {
                for n in &mut self.ix.notices {
                    n.unread = false;
                }
            }
            PanelAction::ClearAppNotices => self.ix.notices.clear(),
        }
        self.readings.app_unread = self.ix.unread();
        if let Some(k) = kind {
            self.rebuild_panel(k);
        }
        Deferred::Nothing
    }
}
