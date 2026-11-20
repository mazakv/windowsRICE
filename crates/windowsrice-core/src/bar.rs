//! `[bar]` section of `config.toml` (schema v3): the windowsRICE Bar.
//!
//! Every module is an entry in `bar.modules` with a stable id, a kind, a
//! region, an enabled flag and an optional refresh interval. The order of the
//! entries is the display order within each region.

use serde::{Deserialize, Serialize};

use crate::hotkey::{Hotkey, Key, Modifiers};

/// Smallest and largest bar height in DIPs.
pub const BAR_HEIGHT_RANGE: (u16, u16) = (24, 64);
/// Refresh interval bounds in milliseconds.
pub const INTERVAL_RANGE_MS: (u32, u32) = (250, 3_600_000);
/// Most modules a configuration may list.
pub const MAX_MODULES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleKind {
    /// windowsRICE menu button (opens settings). Always present and enabled.
    Menu,
    Clock,
    Cpu,
    Gpu,
    Vram,
    Ram,
    Volume,
    Network,
    Upload,
    Download,
    ActiveWindow,
    Battery,
    Notifications,
    CustomText,
    CustomIcon,
}

impl ModuleKind {
    pub const ALL: [ModuleKind; 15] = [
        ModuleKind::Menu,
        ModuleKind::Clock,
        ModuleKind::Cpu,
        ModuleKind::Gpu,
        ModuleKind::Vram,
        ModuleKind::Ram,
        ModuleKind::Volume,
        ModuleKind::Network,
        ModuleKind::Upload,
        ModuleKind::Download,
        ModuleKind::ActiveWindow,
        ModuleKind::Battery,
        ModuleKind::Notifications,
        ModuleKind::CustomText,
        ModuleKind::CustomIcon,
    ];

    /// Kinds that may appear more than once.
    pub fn is_custom(self) -> bool {
        matches!(self, ModuleKind::CustomText | ModuleKind::CustomIcon)
    }

    /// Default refresh interval in milliseconds, or `None` for modules that
    /// are event-driven or static (clock: minute boundaries; active window:
    /// window events; menu and custom: never).
    pub fn default_interval_ms(self) -> Option<u32> {
        match self {
            ModuleKind::Cpu | ModuleKind::Gpu | ModuleKind::Upload | ModuleKind::Download => {
                Some(2_000)
            }
            ModuleKind::Vram | ModuleKind::Ram => Some(5_000),
            // Event-driven; the interval is a safety re-read.
            ModuleKind::Volume => Some(30_000),
            ModuleKind::Network => Some(30_000),
            ModuleKind::Battery => Some(30_000),
            ModuleKind::Notifications => Some(10_000),
            ModuleKind::Menu
            | ModuleKind::Clock
            | ModuleKind::ActiveWindow
            | ModuleKind::CustomText
            | ModuleKind::CustomIcon => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Region {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleConfig {
    /// Stable id: 1–32 characters of `a–z`, `0–9` and `-`.
    pub id: String,
    pub kind: ModuleKind,
    #[serde(default)]
    pub region: Region,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Overrides the kind's default refresh interval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_ms: Option<u32>,
    /// Text of a `custom_text` module (1–64 characters).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Segoe Fluent Icons code point of a `custom_icon` module, hex (`"E80F"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Accessible name of a custom module (defaults to its text).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Show the module's short label (`CPU`, `RAM`, the speed arrow, …)
    /// in front of its value. Default: shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_label: Option<bool>,
    /// Widest the module may be, in DIPs (40–800); longer text is shortened
    /// with an ellipsis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<u16>,
    /// Clock only: 12-hour time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hour12: Option<bool>,
    // ---- Phase 9: per-module appearance (all optional; the bar style
    // applies when unset).
    /// What this module shows (overrides `bar.style.content`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<ModuleContent>,
    /// Accent of this module: its icon and value (`#RRGGBB`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Tile behind this module (`#RRGGBB`); shown even when tiles are off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    /// Text size of this module, DIPs (10–20).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<u16>,
    /// Space inside this module.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density: Option<Density>,
    /// RAM and VRAM: percentage or the amount in use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_format: Option<ValueFormat>,
    /// Upload and download: bytes or bits per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_unit: Option<SpeedUnit>,
    /// Clock: show the date next to the time (default shown).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_date: Option<bool>,
}

/// How RAM and VRAM show their reading.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValueFormat {
    #[default]
    Percent,
    /// Amount in use, such as `12.3 GB`.
    Amount,
}

/// Unit of upload and download speeds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpeedUnit {
    /// Bytes per second (`KB/s`).
    #[default]
    Bytes,
    /// Bits per second (`Mb/s`), as network adapters are rated.
    Bits,
}

fn yes() -> bool {
    true
}

impl ModuleConfig {
    pub fn new(id: &str, kind: ModuleKind, region: Region, enabled: bool) -> Self {
        Self {
            id: id.to_owned(),
            kind,
            region,
            enabled,
            interval_ms: None,
            text: None,
            icon: None,
            label: None,
            show_label: None,
            max_width: None,
            hour12: None,
            content: None,
            color: None,
            background: None,
            font_size: None,
            density: None,
            value_format: None,
            speed_unit: None,
            show_date: None,
        }
    }

    /// Kinds whose value can be shown without its short label.
    pub fn has_label(kind: ModuleKind) -> bool {
        matches!(
            kind,
            ModuleKind::Cpu
                | ModuleKind::Gpu
                | ModuleKind::Vram
                | ModuleKind::Ram
                | ModuleKind::Volume
                | ModuleKind::Upload
                | ModuleKind::Download
        )
    }

    /// Effective refresh interval.
    pub fn interval_ms(&self) -> Option<u32> {
        self.kind
            .default_interval_ms()
            .map(|d| self.interval_ms.unwrap_or(d))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MonitorPlacement {
    /// One bar on every monitor.
    #[default]
    All,
    /// One bar on the primary monitor only.
    Primary,
}

/// Per-monitor setting. With `id` (the monitor device path reported by
/// Windows, stable when GDI names are renumbered) it matches that physical
/// monitor; without `id` (files written before this field existed) it
/// matches the Windows device name (`\\.\DISPLAY2`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonitorOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub device: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u16>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FullscreenPolicy {
    /// Hide the bar while a fullscreen application covers its monitor.
    #[default]
    Hide,
    /// Keep the bar visible (it stays below fullscreen windows that are
    /// topmost, and above others).
    Keep,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PopupConfig {
    /// Close the settings popup when it loses focus.
    pub close_on_focus_loss: bool,
    /// List modules whose reading is unavailable on this machine.
    pub show_unavailable_modules: bool,
}

impl Default for PopupConfig {
    fn default() -> Self {
        Self {
            close_on_focus_loss: true,
            show_unavailable_modules: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BarConfig {
    /// Height in DIPs; `None` uses the theme's `components.bar.height`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u16>,
    /// Gap between modules in DIPs (0–24).
    pub spacing: u16,
    /// Reserve the bar's area so maximised windows start below it
    /// (Windows application desktop toolbar).
    pub reserve_work_area: bool,
    pub auto_hide: bool,
    /// Delay before an auto-hidden bar collapses again (200–5000 ms).
    pub auto_hide_delay_ms: u32,
    pub fullscreen: FullscreenPolicy,
    pub monitors: MonitorPlacement,
    #[serde(rename = "monitor", skip_serializing_if = "Vec::is_empty")]
    pub monitor_overrides: Vec<MonitorOverride>,
    /// Keyboard shortcut that focuses the bar.
    pub focus_hotkey: Hotkey,
    pub popup: PopupConfig,
    /// Appearance overrides on top of the theme (Design Studio).
    pub style: BarStyle,
    /// Short fades (bar appearing, settings window, applying a design).
    /// Windows' "Animation effects" setting is respected as well.
    #[serde(skip_serializing_if = "is_true")]
    pub animations: bool,
    pub modules: Vec<ModuleConfig>,
}

fn is_true(v: &bool) -> bool {
    *v
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackplatePolicy {
    /// Solid text backplates when translucency could reduce contrast.
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerticalAlign {
    #[default]
    Center,
    Top,
    Bottom,
}

/// How the bar's surface is drawn.
///
/// The bar never activates, and Windows draws the system Acrylic or Mica
/// backdrop of such windows as a flat fallback tint. The bar therefore does
/// not use the system backdrop: translucency is drawn by the bar itself with
/// per-pixel alpha, so what is behind the bar shows through. There is no
/// blur.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Material {
    /// Translucent if the theme is translucent, otherwise solid.
    #[default]
    Theme,
    /// Always opaque.
    Solid,
    /// See-through at the chosen opacity (no blur).
    Translucent,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GradientDirection {
    #[default]
    Horizontal,
    Vertical,
    Diagonal,
}

/// A linear gradient across the bar surface: 2–4 `#RRGGBB` colours.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Gradient {
    pub direction: GradientDirection,
    pub colors: Vec<String>,
}

/// Background behind each module.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModuleShape {
    /// No module background (only text backplates when needed).
    #[default]
    None,
    Rounded,
    Pill,
    Square,
}

/// What each module shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleContent {
    /// As configured per module (icons where a module has one, labels per
    /// module setting).
    #[default]
    Default,
    /// Only icons; modules without an icon show their value.
    IconOnly,
    /// Only values, without icons or labels.
    ValueOnly,
    /// Icon, label and value.
    Full,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Compact,
    #[default]
    Normal,
    Expanded,
}

/// Icon font. Both ship with Windows 11.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IconSet {
    /// Segoe Fluent Icons (Windows 11).
    #[default]
    Fluent,
    /// Segoe MDL2 Assets (Windows 10 style).
    Classic,
}

/// Colour of module icons.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IconTint {
    #[default]
    Text,
    Secondary,
    Accent,
    /// A distinct colour per module kind.
    Colorful,
    /// `icon_color`.
    Custom,
}

/// A thin accent-coloured line along one edge of the bar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccentLine {
    #[default]
    None,
    Top,
    Bottom,
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// Bar appearance overrides.///
/// Every field is optional and falls back to the active theme. Colours are
/// `#RRGGBB`. Fields added in Phase 8 are skipped when they hold their
/// default, so files written before Phase 8 stay valid and unchanged.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BarStyle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Surface opacity 0.30–1.00.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    pub backplates: BackplatePolicy,
    /// A hairline along the bottom edge of the bar.
    pub border_bottom: bool,
    /// Thin separators between modules.
    pub separators: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    /// Text size in DIPs (11–18).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<u16>,
    /// Weight of normal text: 300–600, limited to what the family offers in
    /// the Studio (emphasis stays at most 600).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<u16>,
    /// Horizontal padding at both ends of the bar, DIPs (0–48).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding_x: Option<u16>,
    pub vertical_align: VerticalAlign,
    // ---- Phase 8 ----
    #[serde(skip_serializing_if = "is_default")]
    pub material: Material,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gradient: Option<Gradient>,
    /// Module background colour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_secondary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clock_color: Option<String>,
    /// Used when `icon_tint` is `custom`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
    /// Colour of module values (percentages, speeds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover_color: Option<String>,
    /// Background of the menu button while the menu is open.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus_color: Option<String>,
    /// Bottom border thickness in DIPs (1–4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<u8>,
    /// Separator thickness in DIPs (1–3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator_width: Option<u8>,
    #[serde(skip_serializing_if = "is_default")]
    pub module_shape: ModuleShape,
    /// Module background opacity in percent (10–100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_opacity: Option<u8>,
    /// Corner radius of rounded module backgrounds, DIPs (0–16).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_radius: Option<u8>,
    #[serde(skip_serializing_if = "is_default")]
    pub content: ModuleContent,
    #[serde(skip_serializing_if = "is_default")]
    pub density: Density,
    #[serde(skip_serializing_if = "is_default")]
    pub icon_set: IconSet,
    #[serde(skip_serializing_if = "is_default")]
    pub icon_tint: IconTint,
    /// Icon size relative to the text, DIPs (−2 to +6).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_size: Option<i8>,
    #[serde(skip_serializing_if = "is_default")]
    pub accent_line: AccentLine,
    /// Shading inside the bar's lower edge, percent (0–100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow: Option<u8>,
    /// Light line along the bar's top edge, percent (0–100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlight: Option<u8>,
    /// Strength of the hover background, percent (10–100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover_intensity: Option<u8>,
    /// Space between the left, centre and right groups, DIPs (0–200).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region_gap: Option<u16>,
    /// Clock text size, DIPs (11–22).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clock_size: Option<u16>,
}

/// Parses `#RRGGBB`.
pub fn parse_hex_color(s: &str) -> Option<(u8, u8, u8)> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

impl BarStyle {
    pub fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
        for (name, v) in [
            ("background", &self.background),
            ("accent", &self.accent),
            ("text", &self.text),
        ] {
            if let Some(c) = v
                && parse_hex_color(c).is_none()
            {
                p.push(format!("bar.style.{name} {c:?} must be #RRGGBB"));
            }
        }
        if let Some(o) = self.opacity
            && !(o.is_finite() && (0.30..=1.0).contains(&o))
        {
            p.push(format!("bar.style.opacity {o} is outside 0.30-1.00"));
        }
        if let Some(f) = &self.font_family
            && (f.trim().is_empty() || f.chars().count() > 64 || f.chars().any(char::is_control))
        {
            p.push("bar.style.font_family must be 1-64 printable characters".into());
        }
        if let Some(s) = self.font_size
            && !(11..=18).contains(&s)
        {
            p.push(format!("bar.style.font_size {s} is outside 11-18"));
        }
        if let Some(w) = self.font_weight
            && !(300..=600).contains(&w)
        {
            p.push(format!("bar.style.font_weight {w} is outside 300-600"));
        }
        if let Some(x) = self.padding_x
            && x > 48
        {
            p.push(format!("bar.style.padding_x {x} is outside 0-48"));
        }
        for (name, v) in [
            ("module_background", &self.module_background),
            ("text_secondary", &self.text_secondary),
            ("clock_color", &self.clock_color),
            ("icon_color", &self.icon_color),
            ("value_color", &self.value_color),
            ("border_color", &self.border_color),
            ("separator_color", &self.separator_color),
            ("hover_color", &self.hover_color),
            ("selected_color", &self.selected_color),
            ("focus_color", &self.focus_color),
        ] {
            if let Some(c) = v
                && parse_hex_color(c).is_none()
            {
                p.push(format!("bar.style.{name} {c:?} must be #RRGGBB"));
            }
        }
        if let Some(g) = &self.gradient {
            if !(2..=4).contains(&g.colors.len()) {
                p.push(format!(
                    "bar.style.gradient needs 2-4 colors, found {}",
                    g.colors.len()
                ));
            }
            for c in &g.colors {
                if parse_hex_color(c).is_none() {
                    p.push(format!("bar.style.gradient color {c:?} must be #RRGGBB"));
                }
            }
        }
        let range = |p: &mut Vec<String>, name: &str, v: Option<i32>, lo: i32, hi: i32| {
            if let Some(v) = v
                && !(lo..=hi).contains(&v)
            {
                p.push(format!("bar.style.{name} {v} is outside {lo}-{hi}"));
            }
        };
        range(
            &mut p,
            "border_width",
            self.border_width.map(i32::from),
            1,
            4,
        );
        range(
            &mut p,
            "separator_width",
            self.separator_width.map(i32::from),
            1,
            3,
        );
        range(
            &mut p,
            "module_opacity",
            self.module_opacity.map(i32::from),
            10,
            100,
        );
        range(
            &mut p,
            "module_radius",
            self.module_radius.map(i32::from),
            0,
            16,
        );
        range(&mut p, "icon_size", self.icon_size.map(i32::from), -2, 6);
        range(&mut p, "shadow", self.shadow.map(i32::from), 0, 100);
        range(&mut p, "highlight", self.highlight.map(i32::from), 0, 100);
        range(
            &mut p,
            "hover_intensity",
            self.hover_intensity.map(i32::from),
            10,
            100,
        );
        range(&mut p, "region_gap", self.region_gap.map(i32::from), 0, 200);
        range(&mut p, "clock_size", self.clock_size.map(i32::from), 11, 22);
        p
    }
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            height: None,
            spacing: 4,
            reserve_work_area: true,
            auto_hide: false,
            auto_hide_delay_ms: 800,
            fullscreen: FullscreenPolicy::Hide,
            monitors: MonitorPlacement::All,
            monitor_overrides: Vec::new(),
            focus_hotkey: Hotkey {
                modifiers: Modifiers {
                    ctrl: true,
                    alt: true,
                    shift: false,
                    win: false,
                },
                key: Key::Char('B'),
            },
            popup: PopupConfig::default(),
            style: BarStyle::default(),
            animations: true,
            modules: default_modules(),
        }
    }
}

/// Default module layout.
pub fn default_modules() -> Vec<ModuleConfig> {
    use ModuleKind as K;
    use Region::{Center, Left, Right};
    let mut text = ModuleConfig::new("text", K::CustomText, Right, false);
    text.text = Some("windowsRICE".into());
    let mut icon = ModuleConfig::new("icon", K::CustomIcon, Right, false);
    icon.icon = Some("E734".into());
    icon.label = Some("Favourite".into());
    vec![
        ModuleConfig::new("menu", K::Menu, Left, true),
        ModuleConfig::new("active-window", K::ActiveWindow, Left, true),
        ModuleConfig::new("clock", K::Clock, Center, true),
        text,
        icon,
        ModuleConfig::new("download", K::Download, Right, true),
        ModuleConfig::new("upload", K::Upload, Right, true),
        ModuleConfig::new("network", K::Network, Right, true),
        ModuleConfig::new("volume", K::Volume, Right, true),
        ModuleConfig::new("cpu", K::Cpu, Right, true),
        ModuleConfig::new("gpu", K::Gpu, Right, true),
        ModuleConfig::new("vram", K::Vram, Right, true),
        ModuleConfig::new("ram", K::Ram, Right, true),
        ModuleConfig::new("battery", K::Battery, Right, true),
        ModuleConfig::new("notifications", K::Notifications, Right, true),
    ]
}

fn valid_module_id(s: &str) -> bool {
    (1..=32).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

impl BarConfig {
    /// Checks every rule and returns all problems found.
    pub fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
        if let Some(h) = self.height
            && !(BAR_HEIGHT_RANGE.0..=BAR_HEIGHT_RANGE.1).contains(&h)
        {
            p.push(format!(
                "bar.height = {h} is outside {}-{}",
                BAR_HEIGHT_RANGE.0, BAR_HEIGHT_RANGE.1
            ));
        }
        if self.spacing > 24 {
            p.push(format!("bar.spacing = {} is outside 0-24", self.spacing));
        }
        if !(200..=5000).contains(&self.auto_hide_delay_ms) {
            p.push(format!(
                "bar.auto_hide_delay_ms = {} is outside 200-5000",
                self.auto_hide_delay_ms
            ));
        }
        if self.modules.len() > MAX_MODULES {
            p.push(format!("bar.modules lists more than {MAX_MODULES} modules"));
        }
        let mut seen_ids = std::collections::HashSet::new();
        let mut seen_kinds = std::collections::HashSet::new();
        for m in &self.modules {
            if !valid_module_id(&m.id) {
                p.push(format!(
                    "module id {:?} must be 1-32 characters of a-z, 0-9 and '-'",
                    m.id
                ));
            }
            if !seen_ids.insert(m.id.as_str()) {
                p.push(format!("module id {:?} is used more than once", m.id));
            }
            if !m.kind.is_custom() && !seen_kinds.insert(m.kind) {
                p.push(format!("module kind {:?} is listed more than once", m.kind));
            }
            if let Some(i) = m.interval_ms {
                if m.kind.default_interval_ms().is_none() {
                    p.push(format!("module {:?} does not take interval_ms", m.id));
                } else if !(INTERVAL_RANGE_MS.0..=INTERVAL_RANGE_MS.1).contains(&i) {
                    p.push(format!(
                        "module {:?}: interval_ms = {i} is outside {}-{}",
                        m.id, INTERVAL_RANGE_MS.0, INTERVAL_RANGE_MS.1
                    ));
                }
            }
            match m.kind {
                ModuleKind::CustomText => match &m.text {
                    Some(t)
                        if (1..=64).contains(&t.chars().count())
                            && !t.chars().any(char::is_control) => {}
                    _ => p.push(format!(
                        "module {:?}: custom_text needs text of 1-64 printable characters",
                        m.id
                    )),
                },
                ModuleKind::CustomIcon => {
                    let ok = m
                        .icon
                        .as_deref()
                        .and_then(|h| u32::from_str_radix(h, 16).ok())
                        .and_then(char::from_u32)
                        .is_some_and(|c| ('\u{E000}'..='\u{F8FF}').contains(&c));
                    if !ok {
                        p.push(format!(
                            "module {:?}: custom_icon needs icon = a hex code point in E000-F8FF",
                            m.id
                        ));
                    }
                }
                _ => {
                    if m.text.is_some() || m.icon.is_some() {
                        p.push(format!(
                            "module {:?}: text and icon apply only to custom modules",
                            m.id
                        ));
                    }
                }
            }
            if let Some(l) = &m.label
                && !(1..=64).contains(&l.chars().count())
            {
                p.push(format!("module {:?}: label must be 1-64 characters", m.id));
            }
            if m.show_label.is_some() && !ModuleConfig::has_label(m.kind) {
                p.push(format!(
                    "module {:?}: show_label does not apply to this kind",
                    m.id
                ));
            }
            if let Some(w) = m.max_width
                && !(40..=800).contains(&w)
            {
                p.push(format!(
                    "module {:?}: max_width {w} is outside 40-800",
                    m.id
                ));
            }
            if m.hour12.is_some() && m.kind != ModuleKind::Clock {
                p.push(format!(
                    "module {:?}: hour12 applies only to the clock",
                    m.id
                ));
            }
            for (name, v) in [("color", &m.color), ("background", &m.background)] {
                if let Some(c) = v
                    && parse_hex_color(c).is_none()
                {
                    p.push(format!("module {:?}: {name} {c:?} must be #RRGGBB", m.id));
                }
            }
            if let Some(s) = m.font_size
                && !(10..=20).contains(&s)
            {
                p.push(format!("module {:?}: font_size {s} is outside 10-20", m.id));
            }
            if m.value_format.is_some() && !matches!(m.kind, ModuleKind::Ram | ModuleKind::Vram) {
                p.push(format!(
                    "module {:?}: value_format applies only to RAM and VRAM",
                    m.id
                ));
            }
            if m.speed_unit.is_some()
                && !matches!(m.kind, ModuleKind::Upload | ModuleKind::Download)
            {
                p.push(format!(
                    "module {:?}: speed_unit applies only to upload and download",
                    m.id
                ));
            }
            if m.show_date.is_some() && m.kind != ModuleKind::Clock {
                p.push(format!(
                    "module {:?}: show_date applies only to the clock",
                    m.id
                ));
            }
        }
        match self.modules.iter().find(|m| m.kind == ModuleKind::Menu) {
            None => p.push("bar.modules must include the menu module (kind = \"menu\")".into()),
            Some(m) if !m.enabled => {
                p.push("the menu module cannot be disabled (it opens the settings)".into());
            }
            _ => {}
        }
        for o in &self.monitor_overrides {
            if o.id.as_ref().is_some_and(|i| i.is_empty() || i.len() > 512) {
                p.push("bar.monitor.id must be 1-512 characters".into());
            }
            if o.device.trim().is_empty() || o.device.len() > 64 {
                p.push("bar.monitor.device must be 1-64 characters".into());
            }
            if let Some(h) = o.height
                && !(BAR_HEIGHT_RANGE.0..=BAR_HEIGHT_RANGE.1).contains(&h)
            {
                p.push(format!(
                    "bar.monitor height {h} for {} is out of range",
                    o.device
                ));
            }
        }
        if self.focus_hotkey.modifiers.is_empty() {
            p.push("bar.focus_hotkey needs a modifier".into());
        }
        p.extend(self.style.problems());
        p
    }

    /// The override for a monitor when it is the only one known. See
    /// [`BarConfig::monitor_override_among`].
    pub fn monitor_override(&self, device: &str, id: Option<&str>) -> Option<&MonitorOverride> {
        self.monitor_override_among(device, id, &[id])
    }

    /// The override for a monitor, given the stable ids of all connected
    /// monitors (`None` where Windows reports no identity):
    ///
    /// 1. an override with the same stable id;
    /// 2. else an override without an id (written before ids existed, or
    ///    while the monitor was unidentified) for the same device name;
    /// 3. else, only when this monitor is unidentified (`id` is `None`, for
    ///    example a sleeping DisplayPort monitor), an override for the same
    ///    device name whose id belongs to no connected monitor, so the
    ///    setting is kept rather than lost.
    ///
    /// An identified monitor with a different id is a different monitor:
    /// it never takes another monitor's setting, even at the same device
    /// name (so a new path after a cable change starts from the defaults).
    ///
    /// An override whose id belongs to another connected monitor is never
    /// used, so two monitors are not merged.
    pub fn monitor_override_among(
        &self,
        device: &str,
        id: Option<&str>,
        connected: &[Option<&str>],
    ) -> Option<&MonitorOverride> {
        let os = &self.monitor_overrides;
        if let Some(i) = id
            && let Some(o) = os.iter().find(|o| o.id.as_deref() == Some(i))
        {
            return Some(o);
        }
        os.iter()
            .find(|o| o.id.is_none() && o.device == device)
            .or_else(|| {
                if id.is_some() {
                    return None;
                }
                os.iter().find(|o| {
                    o.device == device
                        && o.id
                            .as_deref()
                            .is_some_and(|oid| !connected.contains(&Some(oid)))
                })
            })
    }

    /// Whether a bar should appear on this monitor (only monitor known).
    pub fn monitor_enabled(&self, device: &str, id: Option<&str>, primary: bool) -> bool {
        self.monitor_enabled_among(device, id, primary, &[id])
    }

    /// Whether a bar should appear on this monitor among `connected`.
    pub fn monitor_enabled_among(
        &self,
        device: &str,
        id: Option<&str>,
        primary: bool,
        connected: &[Option<&str>],
    ) -> bool {
        if let Some(o) = self.monitor_override_among(device, id, connected) {
            return o.enabled;
        }
        match self.monitors {
            MonitorPlacement::All => true,
            MonitorPlacement::Primary => primary,
        }
    }

    pub fn height_for(&self, device: &str, id: Option<&str>) -> Option<u16> {
        self.height_for_among(device, id, &[id])
    }

    pub fn height_for_among(
        &self,
        device: &str,
        id: Option<&str>,
        connected: &[Option<&str>],
    ) -> Option<u16> {
        self.monitor_override_among(device, id, connected)
            .and_then(|o| o.height)
            .or(self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let b = BarConfig::default();
        assert!(b.problems().is_empty(), "{:?}", b.problems());
        assert_eq!(b.modules.len(), 15, "one entry per kind");
        for k in ModuleKind::ALL {
            assert!(b.modules.iter().any(|m| m.kind == k), "{k:?} missing");
        }
    }

    #[test]
    fn rules_are_enforced() {
        let mut b = BarConfig {
            height: Some(10),
            ..Default::default()
        };
        b.modules[2].interval_ms = Some(5); // clock takes no interval
        b.modules[5].interval_ms = Some(10); // too short
        b.modules.push(ModuleConfig::new(
            "cpu",
            ModuleKind::Cpu,
            Region::Left,
            true,
        ));
        b.modules[0].enabled = false;
        let p = b.problems();
        assert!(p.iter().any(|m| m.contains("bar.height")));
        assert!(p.iter().any(|m| m.contains("does not take interval_ms")));
        assert!(p.iter().any(|m| m.contains("outside 250")));
        assert!(p.iter().any(|m| m.contains("used more than once")));
        assert!(p.iter().any(|m| m.contains("listed more than once")));
        assert!(p.iter().any(|m| m.contains("cannot be disabled")));
    }

    #[test]
    fn custom_modules_validate_content() {
        let mut b = BarConfig::default();
        b.modules.push(ModuleConfig::new(
            "t2",
            ModuleKind::CustomText,
            Region::Left,
            true,
        ));
        let mut icon = ModuleConfig::new("i2", ModuleKind::CustomIcon, Region::Left, true);
        icon.icon = Some("41".into());
        b.modules.push(icon);
        let p = b.problems();
        assert!(p.iter().any(|m| m.contains("custom_text needs text")));
        assert!(p.iter().any(|m| m.contains("E000-F8FF")));
    }

    #[test]
    fn monitor_rules() {
        let mut b = BarConfig::default();
        assert!(b.monitor_enabled(r"\\.\DISPLAY2", None, false));
        b.monitors = MonitorPlacement::Primary;
        assert!(!b.monitor_enabled(r"\\.\DISPLAY2", None, false));
        assert!(b.monitor_enabled(r"\\.\DISPLAY1", None, true));
        // Legacy entry (no id): matched by device name.
        b.monitor_overrides.push(MonitorOverride {
            id: None,
            device: r"\\.\DISPLAY2".into(),
            enabled: true,
            height: Some(40),
        });
        assert!(b.monitor_enabled(r"\\.\DISPLAY2", Some("any"), false));
        assert_eq!(b.height_for(r"\\.\DISPLAY2", None), Some(40));
        assert_eq!(b.height_for(r"\\.\DISPLAY1", None), None);
        // Entry with an id follows the physical monitor, not the GDI name.
        b.monitor_overrides.push(MonitorOverride {
            id: Some("mon-a".into()),
            device: r"\\.\DISPLAY3".into(),
            enabled: false,
            height: None,
        });
        assert!(
            !b.monitor_enabled(r"\\.\DISPLAY5", Some("mon-a"), true),
            "renumbered monitor keeps its setting"
        );
        assert!(
            b.monitor_enabled(r"\\.\DISPLAY3", Some("mon-b"), true),
            "another monitor on the old name is not affected"
        );
    }
}
