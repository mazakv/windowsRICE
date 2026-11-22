//! Bar modules: readings, per-module content, names and stable ids.
//!
//! A module turns its configuration ([`ModuleConfig`]) and the shared
//! [`BarReadings`] into a [`ModuleView`]: the pieces to draw and the
//! accessible name, role and state. Readings are never invented: a value
//! that has not arrived yet is shown as pending, a failed or unsupported
//! reading as unavailable (disabled, named so), and hardware that does not
//! exist (no battery) hides the module from the bar.

pub use windowsrice_core::bar::{ModuleConfig, ModuleKind, Region};

use crate::format::{self, LocalTime};
use crate::panel::{AudioState, LinkKind, MemoryState, NetworkState, glyphs};
use crate::strings::{Lang, NetworkPhrase, Strings};
use crate::tree::{ElementId, Role};

/// State of one reading.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Reading<T> {
    /// Not measured yet.
    #[default]
    Pending,
    Ok(T),
    /// The query failed or is not supported; the reason is shown to the user.
    Unavailable(String),
    /// The hardware does not exist on this computer (for example a battery).
    Absent,
}

impl<T> Reading<T> {
    pub fn ok(&self) -> Option<&T> {
        match self {
            Reading::Ok(v) => Some(v),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Traffic {
    pub up_bytes_per_s: f64,
    pub down_bytes_per_s: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryState {
    pub percent: Option<u8>,
    pub charging: bool,
    pub on_ac: bool,
}

/// Windows' notification state (the user-notification state; the number of
/// notifications is not available to desktop applications).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationState {
    Accepting,
    DoNotDisturb,
    /// A fullscreen or presentation application suppresses notifications.
    Suppressed,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BarReadings {
    pub time: Option<LocalTime>,
    /// Title of the foreground window; `None` for the desktop.
    pub active_window: Option<String>,
    pub cpu: Reading<f32>,
    pub gpu: Reading<f32>,
    pub vram: Reading<MemoryState>,
    pub ram: Reading<MemoryState>,
    pub audio: Reading<AudioState>,
    pub network: Reading<NetworkState>,
    pub traffic: Reading<Traffic>,
    pub battery: Reading<BatteryState>,
    pub notifications: Reading<NotificationState>,
    /// windowsRICE warnings shown by the menu module's status.
    pub warnings: Vec<String>,
    /// Unread windowsRICE notices (the notification badge, Phase 13).
    /// Windows' own notifications are not readable by this installation.
    pub app_unread: usize,
}

/// Modules that open a detail panel when clicked (Phase 13).
pub fn has_panel(kind: ModuleKind) -> bool {
    use ModuleKind as K;
    matches!(
        kind,
        K::Cpu
            | K::Gpu
            | K::Vram
            | K::Ram
            | K::Network
            | K::Upload
            | K::Download
            | K::Volume
            | K::Battery
            | K::Clock
            | K::Notifications
    )
}

/// One drawable part of a module.
#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Text {
        text: String,
        emphasis: bool,
        secondary: bool,
        /// A label such as `CPU` (as opposed to the value).
        label: bool,
    },
    Icon(String),
    /// An icon shown only when the bar style asks for icons on every
    /// module (CPU, GPU, memory and video memory have none by default).
    ExtraIcon(String),
    /// Small status dot (menu module): `true` for warnings.
    Dot {
        warning: bool,
    },
    /// The windowsRICE mark (menu module).
    Mark,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModuleView {
    pub id: String,
    pub kind: ModuleKind,
    pub region: Region,
    pub element: ElementId,
    pub role: Role,
    /// Accessible name.
    pub name: String,
    pub help: Option<String>,
    /// False when the reading is unavailable (shown dimmed, not focusable).
    pub available: bool,
    pub pieces: Vec<Piece>,
    /// Text may be shortened with an ellipsis (active window).
    pub trimmable: bool,
}

/// Stable element id for a module id (FNV-1a), outside the range used by
/// fixed bar elements. Collisions are resolved by [`assign_ids`].
pub fn module_element_id(module_id: &str) -> ElementId {
    let mut h: u32 = 0x811C_9DC5;
    for b in module_id.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    ElementId(0x1000 | (h & 0x7FFF_FFFF))
}

/// Ids for every module, unique even if two hashes collide.
pub fn assign_ids(modules: &[ModuleConfig]) -> Vec<ElementId> {
    let mut used = std::collections::HashSet::new();
    modules
        .iter()
        .map(|m| {
            let mut id = module_element_id(&m.id);
            while !used.insert(id) {
                id = ElementId(id.0.wrapping_add(1) | 0x1000);
            }
            id
        })
        .collect()
}

fn pick(lang: Lang, en: &'static str, ko: &'static str) -> &'static str {
    match lang {
        Lang::En => en,
        Lang::Ko => ko,
    }
}

/// Display name of a module kind (settings popup).
pub fn kind_name(kind: ModuleKind, lang: Lang) -> &'static str {
    use ModuleKind as K;
    match kind {
        K::Menu => pick(lang, "windowsRICE menu", "windowsRICE 메뉴"),
        K::Clock => pick(lang, "Clock and date", "시계와 날짜"),
        K::Cpu => pick(lang, "CPU usage", "CPU 사용률"),
        K::Gpu => pick(lang, "GPU usage", "GPU 사용률"),
        K::Vram => pick(lang, "Video memory", "비디오 메모리"),
        K::Ram => pick(lang, "Memory", "메모리"),
        K::Volume => pick(lang, "Volume", "볼륨"),
        K::Network => pick(lang, "Network status", "네트워크 상태"),
        K::Upload => pick(lang, "Upload speed", "업로드 속도"),
        K::Download => pick(lang, "Download speed", "다운로드 속도"),
        K::ActiveWindow => pick(lang, "Active window", "활성 창"),
        K::Battery => pick(lang, "Battery", "배터리"),
        K::Notifications => pick(lang, "Notifications", "알림"),
        K::CustomText => pick(lang, "Custom text", "사용자 텍스트"),
        K::CustomIcon => pick(lang, "Custom icon", "사용자 아이콘"),
    }
}

/// Display name of a configured module (custom modules use their label).
pub fn module_name(m: &ModuleConfig, lang: Lang) -> String {
    match m.kind {
        ModuleKind::CustomText => m
            .label
            .clone()
            .or_else(|| m.text.clone())
            .unwrap_or_else(|| kind_name(m.kind, lang).to_owned()),
        ModuleKind::CustomIcon => m
            .label
            .clone()
            .unwrap_or_else(|| kind_name(m.kind, lang).to_owned()),
        k => kind_name(k, lang).to_owned(),
    }
}

/// Bytes per second as bits per second, "12.4 Mb/s" (decimal units, as
/// network adapters are rated).
pub fn bit_rate(bytes_per_s: f64) -> String {
    let b = (bytes_per_s * 8.0).max(0.0);
    if b >= 1e9 {
        format!("{:.1} Gb/s", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.1} Mb/s", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.0} kb/s", b / 1e3)
    } else {
        format!("{b:.0} b/s")
    }
}

/// Speed in the module's unit.
fn speed(m: &ModuleConfig, bytes_per_s: f64) -> String {
    match m.speed_unit {
        Some(windowsrice_core::bar::SpeedUnit::Bits) => bit_rate(bytes_per_s),
        _ => rate(bytes_per_s),
    }
}

/// Bytes per second as "1.2 MB/s" (decimal units, as Windows shows rates).
pub fn rate(bps: f64) -> String {
    let b = bps.max(0.0);
    if b >= 1e9 {
        format!("{:.1} GB/s", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.1} MB/s", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.0} KB/s", b / 1e3)
    } else {
        format!("{b:.0} B/s")
    }
}

fn mem_percent(m: &MemoryState) -> u8 {
    if m.total_bytes == 0 {
        0
    } else {
        format::percent((m.used_bytes as f64 / m.total_bytes as f64 * 100.0) as f32)
    }
}

fn text(t: impl Into<String>) -> Piece {
    Piece::Text {
        text: t.into(),
        emphasis: false,
        secondary: false,
        label: false,
    }
}

fn label(t: impl Into<String>) -> Piece {
    Piece::Text {
        text: t.into(),
        emphasis: false,
        secondary: false,
        label: true,
    }
}

/// Icon, optional label and value of a percentage module.
fn percent_pieces(glyph: &str, short: &str, value: String, show_label: bool) -> Vec<Piece> {
    let mut v = vec![Piece::ExtraIcon(glyph.to_owned())];
    if show_label {
        v.push(label(short));
    }
    v.push(text(value));
    v
}

fn icon(g: &str) -> Piece {
    Piece::Icon(g.to_owned())
}

fn icon_from_hex(h: Option<&str>) -> String {
    h.and_then(|h| u32::from_str_radix(h, 16).ok())
        .and_then(char::from_u32)
        .map(String::from)
        .unwrap_or_default()
}

/// Unavailable/pending naming helper.
fn not_ready<T>(r: &Reading<T>, what: &str, lang: Lang) -> (String, Option<String>) {
    match r {
        Reading::Pending => (
            match lang {
                Lang::En => format!("{what}: measuring"),
                Lang::Ko => format!("{what}: 측정 중"),
            },
            None,
        ),
        Reading::Unavailable(why) => (
            match lang {
                Lang::En => format!("{what} unavailable"),
                Lang::Ko => format!("{what}을(를) 알 수 없음"),
            },
            Some(why.clone()),
        ),
        Reading::Absent => (
            match lang {
                Lang::En => format!("{what}: not present on this computer"),
                Lang::Ko => format!("{what}: 이 컴퓨터에 없음"),
            },
            None,
        ),
        Reading::Ok(_) => (what.to_owned(), None),
    }
}

/// Builds the view of one enabled module. Returns `None` when the module
/// should not appear in the bar (hardware absent).
pub fn view(
    m: &ModuleConfig,
    element: ElementId,
    r: &BarReadings,
    strings: &Strings,
) -> Option<ModuleView> {
    use ModuleKind as K;
    let lang = strings.lang;
    let show_label = m.show_label.unwrap_or(true);
    let base =
        |name: String, pieces: Vec<Piece>, available: bool, help: Option<String>| ModuleView {
            id: m.id.clone(),
            kind: m.kind,
            region: m.region,
            element,
            role: Role::Text,
            name,
            help,
            available,
            pieces,
            trimmable: false,
        };
    let unavailable = |reading_label: &str,
                       short: &str,
                       glyph: Option<&str>,
                       reading: &dyn Fn() -> (String, Option<String>)| {
        let (name, help) = reading();
        let mut pieces = Vec::new();
        if let Some(g) = glyph {
            pieces.push(icon(g));
        }
        pieces.push(text(format!("{short} —")));
        let _ = reading_label;
        (name, pieces, help)
    };
    let mut v = match m.kind {
        K::Menu => {
            let mut v = base(
                strings.menu_button().to_owned(),
                vec![Piece::Mark],
                true,
                Some(strings.menu_help().to_owned()),
            );
            v.role = Role::Button;
            if !r.warnings.is_empty() {
                v.pieces.push(Piece::Dot { warning: true });
                v.name = format!("{} — {}", v.name, strings.status_warnings(&r.warnings));
            }
            v
        }
        K::Clock => match &r.time {
            Some(t) => base(
                format::clock_accessible(t, lang),
                vec![
                    Piece::Text {
                        text: if m.hour12 == Some(true) {
                            format::time_text_12(t, lang)
                        } else {
                            format::time_text(t)
                        },
                        emphasis: true,
                        secondary: false,
                        label: false,
                    },
                    Piece::Text {
                        text: format::date_text(t, lang),
                        emphasis: false,
                        secondary: true,
                        label: false,
                    },
                ],
                true,
                None,
            ),
            None => base(
                strings.group_clock().to_owned(),
                vec![text("--:--")],
                false,
                None,
            ),
        },
        K::ActiveWindow => {
            let title = r
                .active_window
                .clone()
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| strings.desktop().to_owned());
            let mut v = base(strings.active_window(&title), vec![text(title)], true, None);
            v.trimmable = true;
            v
        }
        K::Cpu | K::Gpu => {
            let (reading, short) = if m.kind == K::Cpu {
                (&r.cpu, "CPU")
            } else {
                (&r.gpu, "GPU")
            };
            match reading {
                Reading::Ok(p) => {
                    let p = format::percent(*p);
                    let name = if m.kind == K::Cpu {
                        strings.cpu(p)
                    } else {
                        strings.gpu(p)
                    };
                    let glyph = match m.kind {
                        K::Cpu => glyphs::CPU,
                        K::Gpu => glyphs::GPU,
                        K::Ram => glyphs::RAM,
                        _ => glyphs::VRAM,
                    };
                    let value = match (m.kind, m.value_format) {
                        (K::Ram | K::Vram, Some(windowsrice_core::bar::ValueFormat::Amount)) => {
                            r_amount(m.kind, r)
                        }
                        _ => format!("{p}%"),
                    };
                    base(
                        name,
                        percent_pieces(glyph, short, value, show_label),
                        true,
                        None,
                    )
                }
                other => {
                    let what = kind_name(m.kind, lang);
                    let (name, pieces, help) =
                        unavailable(what, short, None, &|| not_ready(other, what, lang));
                    base(name, pieces, false, help)
                }
            }
        }
        K::Ram | K::Vram => {
            let (reading, short) = if m.kind == K::Ram {
                (&r.ram, "RAM")
            } else {
                (&r.vram, "VRAM")
            };
            match reading {
                Reading::Ok(mm) => {
                    let p = mem_percent(mm);
                    let name = match (m.kind, lang) {
                        (K::Ram, _) => strings.memory(
                            &format::gigabytes(mm.used_bytes),
                            &format::gigabytes(mm.total_bytes),
                            p,
                        ),
                        (_, Lang::En) => format!(
                            "Video memory: {} of {} used ({p}%)",
                            format::gigabytes(mm.used_bytes),
                            format::gigabytes(mm.total_bytes)
                        ),
                        (_, Lang::Ko) => format!(
                            "비디오 메모리: {} 중 {} 사용 ({p}%)",
                            format::gigabytes(mm.total_bytes),
                            format::gigabytes(mm.used_bytes)
                        ),
                    };
                    let glyph = match m.kind {
                        K::Cpu => glyphs::CPU,
                        K::Gpu => glyphs::GPU,
                        K::Ram => glyphs::RAM,
                        _ => glyphs::VRAM,
                    };
                    let value = match (m.kind, m.value_format) {
                        (K::Ram | K::Vram, Some(windowsrice_core::bar::ValueFormat::Amount)) => {
                            r_amount(m.kind, r)
                        }
                        _ => format!("{p}%"),
                    };
                    base(
                        name,
                        percent_pieces(glyph, short, value, show_label),
                        true,
                        None,
                    )
                }
                other => {
                    let what = kind_name(m.kind, lang);
                    let (name, pieces, help) =
                        unavailable(what, short, None, &|| not_ready(other, what, lang));
                    base(name, pieces, false, help)
                }
            }
        }
        K::Volume => match &r.audio {
            Reading::Ok(a) => base(
                strings.volume(a.volume_percent, a.muted),
                if show_label {
                    vec![
                        icon(if a.muted {
                            glyphs::MUTE
                        } else {
                            glyphs::VOLUME
                        }),
                        text(format!("{}%", a.volume_percent)),
                    ]
                } else {
                    vec![text(format!("{}%", a.volume_percent))]
                },
                true,
                None,
            ),
            Reading::Absent => base(
                strings.volume_unavailable().to_owned(),
                vec![icon(glyphs::MUTE)],
                false,
                None,
            ),
            other => {
                let what = kind_name(m.kind, lang);
                let (name, help) = not_ready(other, what, lang);
                base(name, vec![icon(glyphs::VOLUME)], false, help)
            }
        },
        K::Network => match &r.network {
            Reading::Ok(n) => {
                let (phrase, g) = match n {
                    NetworkState::Internet(LinkKind::Wired) => {
                        (NetworkPhrase::InternetWired, glyphs::ETHERNET)
                    }
                    NetworkState::Internet(LinkKind::Wireless) => {
                        (NetworkPhrase::InternetWireless, glyphs::WIFI)
                    }
                    NetworkState::Internet(LinkKind::Other) => {
                        (NetworkPhrase::Internet, glyphs::ETHERNET)
                    }
                    NetworkState::LocalOnly => (NetworkPhrase::LocalOnly, glyphs::NETWORK_OFFLINE),
                    NetworkState::Disconnected => {
                        (NetworkPhrase::Disconnected, glyphs::NETWORK_OFFLINE)
                    }
                };
                base(
                    strings.network(phrase).to_owned(),
                    vec![icon(g)],
                    true,
                    None,
                )
            }
            other => {
                let (_, help) = not_ready(other, kind_name(m.kind, lang), lang);
                base(
                    strings.network(NetworkPhrase::Unavailable).to_owned(),
                    vec![icon(glyphs::NETWORK_OFFLINE)],
                    false,
                    help,
                )
            }
        },
        K::Upload | K::Download => {
            let up = m.kind == K::Upload;
            let glyph = if up { "\u{E74A}" } else { "\u{E74B}" };
            match &r.traffic {
                Reading::Ok(t) => {
                    let v = if up {
                        t.up_bytes_per_s
                    } else {
                        t.down_bytes_per_s
                    };
                    let sv = speed(m, v);
                    let name = match (up, lang) {
                        (true, Lang::En) => format!("Upload speed {sv}"),
                        (false, Lang::En) => format!("Download speed {sv}"),
                        (true, Lang::Ko) => format!("업로드 속도 {sv}"),
                        (false, Lang::Ko) => format!("다운로드 속도 {sv}"),
                    };
                    let pieces = if show_label {
                        vec![icon(glyph), text(sv)]
                    } else {
                        vec![text(sv)]
                    };
                    base(name, pieces, true, None)
                }
                other => {
                    let what = kind_name(m.kind, lang);
                    let (name, help) = not_ready(other, what, lang);
                    base(name, vec![icon(glyph), text("—")], false, help)
                }
            }
        }
        K::Battery => match &r.battery {
            Reading::Absent => return None,
            Reading::Ok(b) => {
                let p = b.percent.map_or_else(|| "?".to_owned(), |p| p.to_string());
                let glyph = if b.charging || b.on_ac {
                    "\u{E83E}"
                } else {
                    "\u{E850}"
                };
                let name = match (lang, b.charging) {
                    (Lang::En, true) => format!("Battery {p}%, charging"),
                    (Lang::En, false) if b.on_ac => format!("Battery {p}%, plugged in"),
                    (Lang::En, false) => format!("Battery {p}%"),
                    (Lang::Ko, true) => format!("배터리 {p}%, 충전 중"),
                    (Lang::Ko, false) if b.on_ac => format!("배터리 {p}%, 전원 연결됨"),
                    (Lang::Ko, false) => format!("배터리 {p}%"),
                };
                base(name, vec![icon(glyph), text(format!("{p}%"))], true, None)
            }
            other => {
                let what = kind_name(m.kind, lang);
                let (name, help) = not_ready(other, what, lang);
                base(name, vec![icon("\u{E850}"), text("—")], false, help)
            }
        },
        K::Notifications => match &r.notifications {
            Reading::Ok(s) => {
                let (glyph, name) = match (s, lang) {
                    (NotificationState::Accepting, Lang::En) => ("\u{EA8F}", "Notifications: on"),
                    (NotificationState::DoNotDisturb, Lang::En) => {
                        ("\u{E708}", "Notifications: do not disturb")
                    }
                    (NotificationState::Suppressed, Lang::En) => {
                        ("\u{E708}", "Notifications: paused by a fullscreen app")
                    }
                    (NotificationState::Accepting, Lang::Ko) => ("\u{EA8F}", "알림: 켜짐"),
                    (NotificationState::DoNotDisturb, Lang::Ko) => ("\u{E708}", "알림: 방해 금지"),
                    (NotificationState::Suppressed, Lang::Ko) => {
                        ("\u{E708}", "알림: 전체 화면 앱이 일시 중지함")
                    }
                };
                let mut pieces = vec![icon(glyph)];
                let mut name = name.to_owned();
                if let Some(b) = crate::panels::badge_text(r.app_unread) {
                    pieces.push(text(b));
                    name = match lang {
                        Lang::En => format!("{name}, {} unread windowsRICE notices", r.app_unread),
                        Lang::Ko => {
                            format!("{name}, 읽지 않은 windowsRICE 알림 {}개", r.app_unread)
                        }
                    };
                }
                base(name, pieces, true, None)
            }
            other => {
                let what = kind_name(m.kind, lang);
                let (name, help) = not_ready(other, what, lang);
                base(name, vec![icon("\u{EA8F}")], false, help)
            }
        },
        K::CustomText => {
            let t = m.text.clone().unwrap_or_default();
            base(
                m.label.clone().unwrap_or_else(|| t.clone()),
                vec![text(t)],
                true,
                None,
            )
        }
        K::CustomIcon => base(
            module_name(m, lang),
            vec![icon(&icon_from_hex(m.icon.as_deref()))],
            true,
            None,
        ),
    };
    if m.max_width.is_some() {
        v.trimmable = true;
    }
    // Clicking (or Enter) opens the module's detail panel.
    if has_panel(m.kind) && v.role == Role::Text {
        v.role = Role::Button;
        if v.help.is_none() {
            v.help = Some(
                match lang {
                    Lang::En => "Press Enter for details",
                    Lang::Ko => "Enter를 누르면 자세히 표시",
                }
                .to_owned(),
            );
        }
    }
    if m.kind == K::Clock && m.show_date == Some(false) {
        v.pieces.retain(|p| {
            !matches!(
                p,
                Piece::Text {
                    secondary: true,
                    ..
                }
            )
        });
    }
    Some(v)
}

/// Memory in use as an amount ("12.3 GB"), for RAM or VRAM; "—" when the
/// reading is not available (nothing is invented).
fn r_amount(kind: ModuleKind, r: &BarReadings) -> String {
    let reading = if kind == ModuleKind::Ram {
        &r.ram
    } else {
        &r.vram
    };
    match reading {
        Reading::Ok(mm) => format::gigabytes(mm.used_bytes),
        _ => "—".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windowsrice_core::bar::default_modules;

    #[test]
    fn ids_are_stable_and_unique() {
        let mods = default_modules();
        let a = assign_ids(&mods);
        let b = assign_ids(&mods);
        assert_eq!(a, b);
        let set: std::collections::HashSet<_> = a.iter().collect();
        assert_eq!(set.len(), a.len());
        assert!(a.iter().all(|i| i.0 >= 0x1000));
    }

    #[test]
    fn readings_are_never_invented() {
        let s = Strings::new(Lang::En);
        let r = BarReadings::default();
        for m in default_modules() {
            if let Some(v) = view(&m, ElementId(0x1000), &r, &s) {
                let dynamic = !matches!(
                    m.kind,
                    ModuleKind::Menu
                        | ModuleKind::ActiveWindow
                        | ModuleKind::CustomText
                        | ModuleKind::CustomIcon
                );
                if dynamic {
                    assert!(!v.available, "{:?} available without a reading", m.kind);
                    assert!(!v.name.is_empty());
                }
            }
        }
    }

    #[test]
    fn absent_battery_hides_module() {
        let s = Strings::new(Lang::En);
        let r = BarReadings {
            battery: Reading::Absent,
            ..Default::default()
        };
        let m = ModuleConfig::new("battery", ModuleKind::Battery, Region::Right, true);
        assert!(view(&m, ElementId(0x1000), &r, &s).is_none());
    }

    #[test]
    fn unavailable_reason_is_kept() {
        let s = Strings::new(Lang::En);
        let r = BarReadings {
            gpu: Reading::Unavailable("performance counters disabled".into()),
            ..Default::default()
        };
        let m = ModuleConfig::new("gpu", ModuleKind::Gpu, Region::Right, true);
        let v = view(&m, ElementId(0x1000), &r, &s).unwrap();
        assert!(!v.available);
        assert_eq!(v.name, "GPU usage unavailable");
        assert_eq!(v.help.as_deref(), Some("performance counters disabled"));
    }

    #[test]
    fn labels_and_clock_format_follow_options() {
        let s = Strings::new(Lang::En);
        let r = BarReadings {
            cpu: Reading::Ok(12.0),
            time: Some(LocalTime {
                year: 2026,
                month: 11,
                day: 19,
                weekday: 4,
                hour: 15,
                minute: 4,
            }),
            ..Default::default()
        };
        let mut cpu = ModuleConfig::new("cpu", ModuleKind::Cpu, Region::Right, true);
        cpu.show_label = Some(false);
        let v = view(&cpu, ElementId(0x1000), &r, &s).unwrap();
        // The icon is always offered; the bar draws it only when the style
        // asks for icons on every module.
        assert_eq!(
            v.pieces,
            vec![Piece::ExtraIcon(glyphs::CPU.to_owned()), text("12%")]
        );
        cpu.show_label = None;
        let v = view(&cpu, ElementId(0x1000), &r, &s).unwrap();
        assert_eq!(
            v.pieces,
            vec![
                Piece::ExtraIcon(glyphs::CPU.to_owned()),
                label("CPU"),
                text("12%")
            ]
        );
        assert_eq!(
            v.name, "CPU usage 12%",
            "the accessible name keeps the label"
        );
        let mut clock = ModuleConfig::new("clock", ModuleKind::Clock, Region::Center, true);
        clock.hour12 = Some(true);
        let v = view(&clock, ElementId(0x1001), &r, &s).unwrap();
        assert!(matches!(&v.pieces[0], Piece::Text { text, .. } if text == "3:04 PM"));
        assert_eq!(
            format::time_text_12(r.time.as_ref().unwrap(), Lang::Ko),
            "오후 3:04"
        );
    }

    #[test]
    fn rates() {
        assert_eq!(rate(512.0), "512 B/s");
        assert_eq!(rate(12_345.0), "12 KB/s");
        assert_eq!(rate(1_260_000.0), "1.3 MB/s");
    }
}
