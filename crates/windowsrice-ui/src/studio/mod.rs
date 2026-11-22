//! windowsRICE Design Studio: scene construction.
//!
//! The Studio is a native window built from the same parts as the bar: one
//! layout pass produces the accessibility tree, the draw list and a map from
//! interactive elements to [`StudioAction`]s. Its live preview is not a
//! separate implementation: it is [`crate::bar::build_bar`] run on the draft
//! settings with the draft's theme, drawn scaled (whole bar) and at actual
//! size (both ends and the middle).

pub mod catalog;
mod gallery;

use std::collections::HashMap;

use windowsrice_core::bar::{
    BackplatePolicy, BarConfig, BarStyle, FullscreenPolicy, IconTint, ModuleConfig, ModuleKind,
    MonitorPlacement, Region, VerticalAlign, parse_hex_color,
};
use windowsrice_core::config::{ThemeModePreference, ThemeSelection};
use windowsrice_theme::Color;

use crate::bar::{BarModel, Decor, build_bar};
use crate::draw::{DrawCmd, DrawList, FontRole, TextStyle, transformed};
use crate::geom::{Rect, Size};
use crate::legibility::SurfacePlan;
use crate::module::{BarReadings, kind_name, module_name};
use crate::panel::Scene;
use crate::strings::{Lang, Strings};
use crate::style::PanelStyle;
use crate::text::TextMeasure;
use crate::tree::{Element, ElementId, Role, UiTree, intern};

/// Smallest Studio client area (Phase 12: fits a 1024×768 screen and the
/// work area of a 1366×768 laptop at 100 %; the section area scrolls).
pub const MIN_SIZE: Size = Size {
    w: 1000.0,
    h: 680.0,
};
/// The Studio keeps the bar to this many modules so every card fits.
pub const STUDIO_MAX_MODULES: usize = 20;

const HEADER: f32 = 60.0;
const SIDEBAR: f32 = 212.0;
const PROPS: f32 = 320.0;
const PAD: f32 = 20.0;
const ROW: f32 = 34.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    Home,
    Styles,
    Themes,
    Colors,
    Background,
    Typography,
    Icons,
    Modules,
    Layout,
    Effects,
    Presets,
    /// Import RICE Design Code or a .ricepack (Phase 12).
    Import,
    /// Colour animations and their timeline (Phase 14).
    Motion,
    /// Background image or video (Phase 14).
    Media,
    Advanced,
}

impl Section {
    /// Every page reachable from the navigation. `Home` (before Phase 12)
    /// is no longer listed: Templates is the starting page.
    pub const ALL: [Section; 14] = [
        Section::Styles,
        Section::Themes,
        Section::Colors,
        Section::Background,
        Section::Typography,
        Section::Icons,
        Section::Modules,
        Section::Layout,
        Section::Effects,
        Section::Presets,
        Section::Import,
        Section::Motion,
        Section::Media,
        Section::Advanced,
    ];
    pub fn name(self, lang: Lang) -> &'static str {
        let (en, ko) = match self {
            Section::Home => ("Home", "홈"),
            Section::Styles => ("Templates", "템플릿"),
            Section::Themes => ("Themes", "테마"),
            Section::Colors => ("Colours", "색"),
            Section::Background => ("Background", "배경"),
            Section::Typography => ("Typography", "글꼴"),
            Section::Icons => ("Icons", "아이콘"),
            Section::Modules => ("Modules", "모듈"),
            Section::Layout => ("Layout", "레이아웃"),
            Section::Effects => ("Effects", "효과"),
            Section::Presets => ("My designs", "내 디자인"),
            Section::Import => ("Import design code", "디자인 코드 가져오기"),
            Section::Motion => ("Animation", "애니메이션"),
            Section::Media => ("Image and video", "이미지와 동영상"),
            Section::Advanced => ("Advanced", "고급"),
        };
        if lang == Lang::En { en } else { ko }
    }
    pub fn glyph(self) -> &'static str {
        match self {
            Section::Home => "\u{E80F}",
            Section::Styles => "\u{E790}",
            Section::Themes => "\u{E771}",
            Section::Colors => "\u{E2B1}",
            Section::Background => "\u{EB9F}",
            Section::Typography => "\u{E8D2}",
            Section::Icons => "\u{ED58}",
            Section::Modules => "\u{E8A9}",
            Section::Layout => "\u{E80A}",
            Section::Effects => "\u{E706}",
            Section::Presets => "\u{E74E}",
            Section::Import => "\u{E8B5}",
            Section::Motion => "\u{E768}",
            Section::Media => "\u{E8B9}",
            Section::Advanced => "\u{E713}",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Section::Home => "home",
            Section::Styles => "styles",
            Section::Themes => "themes",
            Section::Colors => "colors",
            Section::Background => "background",
            Section::Typography => "typography",
            Section::Icons => "icons",
            Section::Modules => "modules",
            Section::Layout => "layout",
            Section::Effects => "effects",
            Section::Presets => "presets",
            Section::Import => "import",
            Section::Motion => "motion",
            Section::Media => "media",
            Section::Advanced => "advanced",
        }
    }
    /// One-line description (Home cards).
    pub fn hint(self, lang: Lang) -> &'static str {
        let (en, ko) = match self {
            Section::Home => ("Overview", "개요"),
            Section::Styles => (
                "Complete looks in one click",
                "클릭 한 번으로 완성된 디자인",
            ),
            Section::Themes => ("Built-in and custom themes", "기본 테마와 사용자 테마"),
            Section::Colors => ("Palettes and every colour role", "팔레트와 모든 색 역할"),
            Section::Background => (
                "Solid, translucent and gradients",
                "단색, 반투명, 그라데이션",
            ),
            Section::Typography => (
                "Every installed font, as it looks",
                "설치된 모든 글꼴을 실제 모양으로",
            ),
            Section::Icons => ("Icon shapes, colours and size", "아이콘 모양, 색, 크기"),
            Section::Modules => ("What each module shows and where", "모듈 표시 내용과 위치"),
            Section::Layout => ("Arrangement, height and spacing", "배치, 높이, 간격"),
            Section::Effects => ("Borders, depth and highlights", "테두리, 입체감, 강조"),
            Section::Presets => (
                "Save and share complete designs",
                "완성된 디자인 저장과 공유",
            ),
            Section::Motion => ("Animated colours and gradients", "움직이는 색과 그라데이션"),
            Section::Media => (
                "Background picture or video frame",
                "배경 그림 또는 동영상 프레임",
            ),
            Section::Import => (
                "Check and import shared designs",
                "공유된 디자인 검사 및 가져오기",
            ),
            Section::Advanced => ("Screen space, monitors, reset", "화면 공간, 모니터, 재설정"),
        };
        if lang == Lang::En { en } else { ko }
    }
}

/// The six main areas of the Design Studio (Phase 12). The sidebar shows
/// the groups; the selected group lists its pages underneath.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    Templates,
    BarLayout,
    Appearance,
    Motion,
    MyDesigns,
    Advanced,
}

impl Group {
    pub const ALL: [Group; 6] = [
        Group::Templates,
        Group::BarLayout,
        Group::Appearance,
        Group::Motion,
        Group::MyDesigns,
        Group::Advanced,
    ];
    /// The pages of the group, first page first.
    pub fn sections(self) -> &'static [Section] {
        match self {
            Group::Templates => &[Section::Styles, Section::Themes],
            Group::BarLayout => &[Section::Modules, Section::Layout],
            Group::Appearance => &[
                Section::Colors,
                Section::Background,
                Section::Typography,
                Section::Icons,
            ],
            Group::Motion => &[Section::Effects, Section::Motion, Section::Media],
            Group::MyDesigns => &[Section::Presets, Section::Import],
            Group::Advanced => &[Section::Advanced],
        }
    }
    pub fn of(section: Section) -> Group {
        Group::ALL
            .into_iter()
            .find(|g| g.sections().contains(&section))
            .unwrap_or(Group::Templates)
    }
    pub fn name(self, lang: Lang) -> &'static str {
        let (en, ko) = match self {
            Group::Templates => ("Templates", "템플릿"),
            Group::BarLayout => ("Bar layout", "바 레이아웃"),
            Group::Appearance => ("Appearance", "모양"),
            Group::Motion => ("Motion & media", "모션 및 미디어"),
            Group::MyDesigns => ("My designs", "내 디자인"),
            Group::Advanced => ("Advanced", "고급"),
        };
        if lang == Lang::En { en } else { ko }
    }
    pub fn glyph(self) -> &'static str {
        match self {
            Group::Templates => "\u{E790}",
            Group::BarLayout => "\u{E8A9}",
            Group::Appearance => "\u{E2B1}",
            Group::Motion => "\u{E706}",
            Group::MyDesigns => "\u{E74E}",
            Group::Advanced => "\u{E713}",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Group::Templates => "templates",
            Group::BarLayout => "bar-layout",
            Group::Appearance => "appearance",
            Group::Motion => "motion",
            Group::MyDesigns => "my-designs",
            Group::Advanced => "advanced",
        }
    }
}

/// The part of the bar shown in the live preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PreviewView {
    /// Whole bar scaled plus three actual-size close-ups.
    #[default]
    All,
    /// The whole bar in two halves, larger.
    Whole,
    Left,
    Center,
    Right,
    /// The selected module (or the clock), enlarged.
    Module,
}

/// A colour the Colours section edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ColorTarget {
    #[default]
    Background,
    ModuleBackground,
    Accent,
    Text,
    SecondaryText,
    Clock,
    Icons,
    Values,
    Border,
    Separator,
    Hover,
    Selected,
    Focus,
    /// One colour of the background gradient (0–3).
    GradientStop(u8),
}

impl ColorTarget {
    pub const ROLES: [ColorTarget; 13] = [
        ColorTarget::Background,
        ColorTarget::ModuleBackground,
        ColorTarget::Accent,
        ColorTarget::Text,
        ColorTarget::SecondaryText,
        ColorTarget::Clock,
        ColorTarget::Icons,
        ColorTarget::Values,
        ColorTarget::Border,
        ColorTarget::Separator,
        ColorTarget::Hover,
        ColorTarget::Selected,
        ColorTarget::Focus,
    ];
    pub fn name(self, lang: Lang) -> String {
        let (en, ko) = match self {
            ColorTarget::Background => ("Background", "배경"),
            ColorTarget::ModuleBackground => ("Module background", "모듈 배경"),
            ColorTarget::Accent => ("Accent", "강조색"),
            ColorTarget::Text => ("Text", "글자"),
            ColorTarget::SecondaryText => ("Secondary text", "보조 글자"),
            ColorTarget::Clock => ("Clock", "시계"),
            ColorTarget::Icons => ("Icons", "아이콘"),
            ColorTarget::Values => ("Values", "값"),
            ColorTarget::Border => ("Border", "테두리"),
            ColorTarget::Separator => ("Separators", "구분선"),
            ColorTarget::Hover => ("Hover", "호버"),
            ColorTarget::Selected => ("Open menu", "열린 메뉴"),
            ColorTarget::Focus => ("Focus ring", "포커스 링"),
            ColorTarget::GradientStop(i) => {
                return match lang {
                    Lang::En => format!("Gradient colour {}", i + 1),
                    Lang::Ko => format!("그라데이션 색 {}", i + 1),
                };
            }
        };
        (if lang == Lang::En { en } else { ko }).to_owned()
    }
    /// Where the colour appears.
    pub fn hint(self, lang: Lang) -> &'static str {
        let (en, ko) = match self {
            ColorTarget::Background => ("The bar surface", "바 표면"),
            ColorTarget::ModuleBackground => (
                "Tiles behind modules (when tiles are on)",
                "모듈 뒤 타일 (타일을 켠 경우)",
            ),
            ColorTarget::Accent => (
                "Menu mark, accent lines and accent icons",
                "메뉴 표시, 강조선, 강조 아이콘",
            ),
            ColorTarget::Text => ("Labels, titles and values", "레이블, 제목, 값"),
            ColorTarget::SecondaryText => ("The date and other quiet text", "날짜 등 보조 글자"),
            ColorTarget::Clock => ("The time", "시간"),
            ColorTarget::Icons => ("Module icons", "모듈 아이콘"),
            ColorTarget::Values => ("Percentages and speeds", "퍼센트와 속도"),
            ColorTarget::Border => ("The bottom border", "아래 테두리"),
            ColorTarget::Separator => ("Lines between modules", "모듈 사이 선"),
            ColorTarget::Hover => ("Background under the pointer", "포인터 아래 배경"),
            ColorTarget::Selected => (
                "Menu button while the menu is open",
                "메뉴가 열린 동안의 메뉴 버튼",
            ),
            ColorTarget::Focus => ("Keyboard focus ring", "키보드 포커스 링"),
            ColorTarget::GradientStop(_) => {
                ("Part of the background gradient", "배경 그라데이션의 일부")
            }
        };
        if lang == Lang::En { en } else { ko }
    }
    pub fn key(self) -> String {
        match self {
            ColorTarget::GradientStop(i) => format!("grad{i}"),
            other => format!("{other:?}").to_lowercase(),
        }
    }
    /// The `[bar.style]` value (`None`: the theme's colour).
    pub fn get(self, s: &BarStyle) -> Option<String> {
        match self {
            ColorTarget::Background => s.background.clone(),
            ColorTarget::ModuleBackground => s.module_background.clone(),
            ColorTarget::Accent => s.accent.clone(),
            ColorTarget::Text => s.text.clone(),
            ColorTarget::SecondaryText => s.text_secondary.clone(),
            ColorTarget::Clock => s.clock_color.clone(),
            ColorTarget::Icons => (s.icon_tint == IconTint::Custom)
                .then(|| s.icon_color.clone())
                .flatten(),
            ColorTarget::Values => s.value_color.clone(),
            ColorTarget::Border => s.border_color.clone(),
            ColorTarget::Separator => s.separator_color.clone(),
            ColorTarget::Hover => s.hover_color.clone(),
            ColorTarget::Selected => s.selected_color.clone(),
            ColorTarget::Focus => s.focus_color.clone(),
            ColorTarget::GradientStop(i) => s
                .gradient
                .as_ref()
                .and_then(|g| g.colors.get(usize::from(i)).cloned()),
        }
    }
    /// Sets the colour; `None` returns it to the theme's colour.
    pub fn set(self, s: &mut BarStyle, v: Option<String>) {
        match self {
            ColorTarget::Background => s.background = v,
            ColorTarget::ModuleBackground => s.module_background = v,
            ColorTarget::Accent => s.accent = v,
            ColorTarget::Text => s.text = v,
            ColorTarget::SecondaryText => s.text_secondary = v,
            ColorTarget::Clock => s.clock_color = v,
            ColorTarget::Icons => {
                s.icon_tint = if v.is_some() {
                    IconTint::Custom
                } else {
                    IconTint::Text
                };
                s.icon_color = v;
            }
            ColorTarget::Values => s.value_color = v,
            ColorTarget::Border => {
                if v.is_some() {
                    s.border_bottom = true;
                }
                s.border_color = v;
            }
            ColorTarget::Separator => {
                if v.is_some() {
                    s.separators = true;
                }
                s.separator_color = v;
            }
            ColorTarget::Hover => s.hover_color = v,
            ColorTarget::Selected => s.selected_color = v,
            ColorTarget::Focus => s.focus_color = v,
            ColorTarget::GradientStop(i) => {
                if let (Some(g), Some(v)) = (s.gradient.as_mut(), v)
                    && let Some(slot) = g.colors.get_mut(usize::from(i))
                {
                    *slot = v;
                }
            }
        }
    }
}

/// A numeric setting edited with a slider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SliderKind {
    Opacity,
    ModuleOpacity,
    Height,
    Spacing,
    Padding,
    RegionGap,
    FontSize,
    ClockSize,
    IconSize,
    BorderWidth,
    SeparatorWidth,
    Radius,
    Shadow,
    Highlight,
    Hover,
}

/// Values the theme supplies when a setting is not overridden.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeDefaults {
    pub opacity: f32,
    pub height: f32,
    pub font_size: f32,
    pub padding: f32,
    pub gap: f32,
    pub radius: f32,
}

impl Default for ThemeDefaults {
    fn default() -> Self {
        Self {
            opacity: 0.88,
            height: 34.0,
            font_size: 13.0,
            padding: 12.0,
            gap: 16.0,
            radius: 6.0,
        }
    }
}

impl SliderKind {
    /// (minimum, maximum, step).
    pub fn range(self) -> (f32, f32, f32) {
        match self {
            SliderKind::Opacity => (0.30, 1.0, 0.05),
            SliderKind::ModuleOpacity => (10.0, 100.0, 5.0),
            SliderKind::Height => (
                f32::from(windowsrice_core::bar::BAR_HEIGHT_RANGE.0),
                f32::from(windowsrice_core::bar::BAR_HEIGHT_RANGE.1),
                1.0,
            ),
            SliderKind::Spacing => (0.0, 24.0, 1.0),
            SliderKind::Padding => (0.0, 48.0, 2.0),
            SliderKind::RegionGap => (0.0, 200.0, 4.0),
            SliderKind::FontSize => (11.0, 18.0, 1.0),
            SliderKind::ClockSize => (11.0, 22.0, 1.0),
            SliderKind::IconSize => (-2.0, 6.0, 1.0),
            SliderKind::BorderWidth => (1.0, 4.0, 1.0),
            SliderKind::SeparatorWidth => (1.0, 3.0, 1.0),
            SliderKind::Radius => (0.0, 16.0, 1.0),
            SliderKind::Shadow | SliderKind::Highlight => (0.0, 100.0, 5.0),
            SliderKind::Hover => (10.0, 100.0, 5.0),
        }
    }
    pub fn name(self, lang: Lang) -> &'static str {
        let (en, ko) = match self {
            SliderKind::Opacity => ("Opacity", "불투명도"),
            SliderKind::ModuleOpacity => ("Module background opacity", "모듈 배경 불투명도"),
            SliderKind::Height => ("Bar height", "바 높이"),
            SliderKind::Spacing => ("Space between modules", "모듈 간격"),
            SliderKind::Padding => ("Padding at the ends", "양 끝 여백"),
            SliderKind::RegionGap => ("Space between groups", "그룹 간격"),
            SliderKind::FontSize => ("Text size", "글자 크기"),
            SliderKind::ClockSize => ("Clock size", "시계 크기"),
            SliderKind::IconSize => ("Icon size", "아이콘 크기"),
            SliderKind::BorderWidth => ("Border thickness", "테두리 두께"),
            SliderKind::SeparatorWidth => ("Separator width", "구분선 두께"),
            SliderKind::Radius => ("Corner radius", "모서리 반경"),
            SliderKind::Shadow => ("Shadow strength", "그림자 세기"),
            SliderKind::Highlight => ("Highlight intensity", "하이라이트 세기"),
            SliderKind::Hover => ("Hover intensity", "호버 세기"),
        };
        if lang == Lang::En { en } else { ko }
    }
    /// Current value, falling back to the theme's.
    pub fn get(self, d: &Draft, td: &ThemeDefaults) -> f32 {
        let s = &d.bar.style;
        match self {
            SliderKind::Opacity => s.opacity.unwrap_or(td.opacity),
            SliderKind::ModuleOpacity => f32::from(s.module_opacity.unwrap_or(100)),
            SliderKind::Height => d.bar.height.map_or(td.height, f32::from),
            SliderKind::Spacing => f32::from(d.bar.spacing),
            SliderKind::Padding => s.padding_x.map_or(td.padding, f32::from),
            SliderKind::RegionGap => s.region_gap.map_or(td.gap, f32::from),
            SliderKind::FontSize => s.font_size.map_or(td.font_size, f32::from),
            SliderKind::ClockSize => s
                .clock_size
                .map_or_else(|| s.font_size.map_or(td.font_size, f32::from), f32::from),
            SliderKind::IconSize => f32::from(s.icon_size.unwrap_or(0)),
            SliderKind::BorderWidth => f32::from(s.border_width.unwrap_or(1)),
            SliderKind::SeparatorWidth => f32::from(s.separator_width.unwrap_or(1)),
            SliderKind::Radius => s.module_radius.map_or(td.radius, f32::from),
            SliderKind::Shadow => f32::from(s.shadow.unwrap_or(0)),
            SliderKind::Highlight => f32::from(s.highlight.unwrap_or(0)),
            SliderKind::Hover => f32::from(s.hover_intensity.unwrap_or(50)),
        }
    }
    /// Sets a value (snapped to the step and clamped). Returns `true` if the
    /// draft changed.
    pub fn set(self, d: &mut Draft, v: f32) -> bool {
        let (lo, hi, step) = self.range();
        let v = ((v.clamp(lo, hi) - lo) / step).round() * step + lo;
        let v = v.clamp(lo, hi);
        let before = d.clone();
        let s = &mut d.bar.style;
        let u8v = || v.round() as u8;
        match self {
            SliderKind::Opacity => s.opacity = Some((v * 100.0).round() / 100.0),
            SliderKind::ModuleOpacity => s.module_opacity = Some(u8v()),
            SliderKind::Height => d.bar.height = Some(v.round() as u16),
            SliderKind::Spacing => d.bar.spacing = v.round() as u16,
            SliderKind::Padding => s.padding_x = Some(v.round() as u16),
            SliderKind::RegionGap => s.region_gap = Some(v.round() as u16),
            SliderKind::FontSize => s.font_size = Some(v.round() as u16),
            SliderKind::ClockSize => s.clock_size = Some(v.round() as u16),
            SliderKind::IconSize => s.icon_size = Some(v.round() as i8),
            SliderKind::BorderWidth => {
                s.border_width = Some(u8v());
                s.border_bottom = true;
            }
            SliderKind::SeparatorWidth => {
                s.separator_width = Some(u8v());
                s.separators = true;
            }
            SliderKind::Radius => s.module_radius = Some(u8v()),
            SliderKind::Shadow => s.shadow = Some(u8v()),
            SliderKind::Highlight => s.highlight = Some(u8v()),
            SliderKind::Hover => s.hover_intensity = Some(u8v()),
        }
        *d != before
    }
    /// Value as shown to people and screen readers.
    pub fn text(self, v: f32) -> String {
        match self {
            SliderKind::Opacity => format!("{}%", (v * 100.0).round()),
            SliderKind::ModuleOpacity
            | SliderKind::Shadow
            | SliderKind::Highlight
            | SliderKind::Hover => format!("{}%", v.round()),
            SliderKind::IconSize => format!("{:+}", v.round() as i32),
            _ => format!("{}", v.round()),
        }
    }
}

/// A gallery laid out as a grid: card rectangles by index, for keyboard
/// navigation and scrolling (rectangles may lie outside the visible area).
#[derive(Debug, Clone, PartialEq)]
pub struct GridInfo {
    pub prefix: String,
    pub cols: usize,
    pub rects: Vec<Rect>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SliderInfo {
    pub element: ElementId,
    pub track: Rect,
    pub kind: SliderKind,
}

/// Clears colour overrides so a newly chosen theme shows its own colours
/// (fonts, layout and effects stay). Undo restores them.
pub fn apply_theme_card(d: &mut Draft, id: &str, mode: ThemeModePreference) -> bool {
    let before = d.clone();
    d.theme.active = id.to_owned();
    d.theme.mode = mode;
    let s = &mut d.bar.style;
    for t in ColorTarget::ROLES {
        if t != ColorTarget::Icons {
            t.set(s, None);
        }
    }
    s.icon_color = None;
    if s.icon_tint == IconTint::Custom {
        s.icon_tint = IconTint::Text;
    }
    s.gradient = None;
    *d != before
}
/// State of the Import design code page (Phase 12).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportView {
    /// The design code being checked.
    pub text: String,
    /// Where it came from ("Pasted code", a file name).
    pub source: String,
    /// The code came from a `.ricepack` (its assets were checked).
    pub package: bool,
    pub analyzed: bool,
    pub diagnostics: Vec<windowsrice_core::design_code::Diagnostic>,
    /// (check, status) in display order.
    pub checks: Vec<(String, ImportCheck)>,
    pub can_apply: bool,
    pub selected: Option<usize>,
    /// Name of the analysed design.
    pub name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportCheck {
    Passed,
    Failed,
    NotRun,
    /// The check could not be performed here (for example no antivirus).
    Unavailable,
}

impl ImportView {
    /// The line (1-based) of the selected problem, if it has one.
    pub fn selected_line(&self) -> Option<u32> {
        self.selected
            .and_then(|i| self.diagnostics.get(i))
            .and_then(|d| d.line)
    }
}

/// What the Studio edits. Applying writes it to `config.toml`.
///
/// `bar` is the bar being edited: the top bar, or the bottom bar when
/// `editing_bottom` is set (Phase 14). The other bar waits in `other`, so
/// every Studio page edits whichever bar is selected.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Draft {
    pub theme: ThemeSelection,
    pub bar: BarConfig,
    pub other: Option<BarConfig>,
    pub editing_bottom: bool,
}

impl Draft {
    pub fn new(theme: ThemeSelection, top: BarConfig, bottom: Option<BarConfig>) -> Self {
        Self {
            theme,
            bar: top,
            other: bottom,
            editing_bottom: false,
        }
    }
    /// The top bar.
    pub fn top(&self) -> &BarConfig {
        match (&self.other, self.editing_bottom) {
            (Some(t), true) => t,
            _ => &self.bar,
        }
    }
    /// The bottom bar, if one is configured.
    pub fn bottom(&self) -> Option<&BarConfig> {
        if self.editing_bottom {
            Some(&self.bar)
        } else {
            self.other.as_ref()
        }
    }
    /// Swaps which bar `bar` holds. A bottom bar that does not exist yet is
    /// created from [`BarConfig::default_bottom`], turned off.
    pub fn swap_bars(&mut self) {
        let other = self.other.take().unwrap_or_else(|| BarConfig {
            enabled: false,
            ..BarConfig::default_bottom()
        });
        self.other = Some(std::mem::replace(&mut self.bar, other));
        self.editing_bottom = !self.editing_bottom;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThemeEntry {
    pub id: String,
    pub name: String,
    pub builtin: bool,
    pub has_dark: bool,
    pub has_light: bool,
    /// Background, surface, accent and text of the default mode.
    pub swatches: [Color; 4],
}

/// Editable copy of a custom theme's main colours.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeEdit {
    pub id: String,
    pub name: String,
    pub dark: bool,
    pub background: String,
    pub accent: String,
    pub text: String,
    pub backdrop: windowsrice_theme::schema::Backdrop,
    pub opacity: f32,
    /// Validation result of the edited theme (errors block saving).
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresetEntry {
    pub file: String,
    pub name: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonitorEntry {
    pub device: String,
    pub id: Option<String>,
    pub name: String,
    pub primary: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FontEntry {
    pub family: String,
    /// The family has Hangul glyphs; otherwise Hangul comes from the
    /// fallback (Malgun Gothic).
    pub hangul: bool,
    /// Weights of the upright faces, ascending.
    pub weights: Vec<u16>,
    /// Fixed-width family.
    pub mono: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewBackdrop {
    #[default]
    Dark,
    Light,
    Colour,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Success,
    Warning,
    Error,
}

/// Text being typed into a field. `caret` and `anchor` are character
/// indexes; the selection is between them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextEdit {
    pub key: String,
    pub buffer: String,
    pub caret: usize,
    pub anchor: usize,
}

impl TextEdit {
    /// Starts editing `value` with everything selected.
    pub fn new(key: &str, value: &str) -> Self {
        let n = value.chars().count();
        Self {
            key: key.to_owned(),
            buffer: value.to_owned(),
            caret: n,
            anchor: 0,
        }
    }
    fn len(&self) -> usize {
        self.buffer.chars().count()
    }
    fn byte(&self, ci: usize) -> usize {
        self.buffer
            .char_indices()
            .nth(ci)
            .map_or(self.buffer.len(), |(b, _)| b)
    }
    /// Selected character range.
    pub fn selection(&self) -> (usize, usize) {
        (self.caret.min(self.anchor), self.caret.max(self.anchor))
    }
    pub fn selected_text(&self) -> String {
        let (a, b) = self.selection();
        self.buffer[self.byte(a)..self.byte(b)].to_owned()
    }
    fn delete_selection(&mut self) -> bool {
        let (a, b) = self.selection();
        if a == b {
            return false;
        }
        let (ba, bb) = (self.byte(a), self.byte(b));
        self.buffer.replace_range(ba..bb, "");
        self.caret = a;
        self.anchor = a;
        true
    }
    /// Inserts text at the caret, replacing the selection. Control
    /// characters are dropped and the field keeps at most `max` characters.
    pub fn insert(&mut self, s: &str, max: usize) {
        self.delete_selection();
        let room = max.saturating_sub(self.len());
        let add: String = s.chars().filter(|c| !c.is_control()).take(room).collect();
        let at = self.byte(self.caret);
        self.buffer.insert_str(at, &add);
        self.caret += add.chars().count();
        self.anchor = self.caret;
    }
    pub fn backspace(&mut self) {
        if !self.delete_selection() && self.caret > 0 {
            let (a, b) = (self.byte(self.caret - 1), self.byte(self.caret));
            self.buffer.replace_range(a..b, "");
            self.caret -= 1;
            self.anchor = self.caret;
        }
    }
    pub fn delete(&mut self) {
        if !self.delete_selection() && self.caret < self.len() {
            let (a, b) = (self.byte(self.caret), self.byte(self.caret + 1));
            self.buffer.replace_range(a..b, "");
        }
    }
    /// Moves the caret; with `extend`, the selection follows.
    pub fn move_to(&mut self, to: usize, extend: bool) {
        self.caret = to.min(self.len());
        if !extend {
            self.anchor = self.caret;
        }
    }
    pub fn left(&mut self, extend: bool) {
        let (a, b) = self.selection();
        if !extend && a != b {
            self.move_to(a, false);
        } else {
            self.move_to(self.caret.saturating_sub(1), extend);
        }
    }
    pub fn right(&mut self, extend: bool) {
        let (a, b) = self.selection();
        if !extend && a != b {
            self.move_to(b, false);
        } else {
            self.move_to(self.caret + 1, extend);
        }
    }
    pub fn home(&mut self, extend: bool) {
        self.move_to(0, extend);
    }
    pub fn end(&mut self, extend: bool) {
        self.move_to(self.len(), extend);
    }
    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.caret = self.len();
    }
    /// Removes and returns the selection (Cut).
    pub fn cut(&mut self) -> String {
        let s = self.selected_text();
        self.delete_selection();
        s
    }
}

/// A module card being dragged and where it would land.
#[derive(Debug, Clone, PartialEq)]
pub struct Drag {
    /// Module id, or `None` when dragging a new module from the palette.
    pub module: Option<String>,
    pub kind: ModuleKind,
    pub x: f32,
    pub y: f32,
    pub target: Option<(Region, usize)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StudioModel {
    pub section: Section,
    pub draft: Draft,
    pub applied: Draft,
    pub can_undo: bool,
    pub can_redo: bool,
    pub themes: Vec<ThemeEntry>,
    pub theme_selected: Option<String>,
    pub theme_edit: Option<ThemeEdit>,
    pub presets: Vec<PresetEntry>,
    pub preset_selected: Option<usize>,
    /// Name typed for saving or renaming a preset.
    pub preset_name: String,
    pub monitors: Vec<MonitorEntry>,
    pub fonts: Vec<FontEntry>,
    pub theme_font: String,
    pub selected_module: Option<String>,
    pub readings: BarReadings,
    pub preview_backdrop: PreviewBackdrop,
    pub status: Option<(String, Severity)>,
    /// Problems that block Apply (shown in red).
    pub blocking: Vec<String>,
    /// Advisory warnings (contrast over translucency, unavailable modules).
    pub warnings: Vec<String>,
    pub text_edit: Option<TextEdit>,
    pub drag: Option<Drag>,
    pub focus: Option<ElementId>,
    pub keyboard_active: bool,
    pub hover: Option<ElementId>,
    pub dark: bool,
    /// Studio appearance preference: `None` follows Windows.
    pub appearance: Option<bool>,
    /// Effective bar height (DIPs) and monitor width (DIPs) for the preview.
    pub bar_height: f32,
    pub monitor_width: f32,
    pub theme_opacity: f32,
    // ---- Phase 8
    /// Vertical scroll of the section area, DIPs.
    pub scroll: f32,
    /// Font gallery search text.
    pub font_query: String,
    pub color_target: ColorTarget,
    /// Most recently chosen colours, newest first.
    pub recent_colors: Vec<String>,
    pub preview_view: PreviewView,
    pub theme_defaults: ThemeDefaults,
    // ---- Phase 9
    pub font_filter: FontFilter,
    /// Import design code page (Phase 12).
    pub import: ImportView,
    /// Animation page (Phase 14): the animation edited, its selected
    /// keyframe and the preview position in the cycle (0–100 %).
    pub motion_target: windowsrice_core::motion::AnimTarget,
    pub motion_key: Option<usize>,
    pub motion_scrub: u8,
    /// Background file of the draft and what its header says (Phase 14):
    /// (library path, video frame), and (is video, width, height, length ms).
    pub background_file: Option<(String, u32)>,
    pub media_info: Option<(bool, u32, u32, u64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StudioAction {
    Nav(Section),
    Undo,
    Redo,
    Revert,
    Apply,
    ResetAll,
    Mode(ThemeModePreference),
    Background(Option<String>),
    Accent(Option<String>),
    TextColour(Option<String>),
    OpacityDown,
    OpacityUp,
    OpacityFromTheme,
    Backplates(BackplatePolicy),
    ToggleBorder,
    ToggleSeparators,
    PreviewBackdrop(PreviewBackdrop),
    SelectModule(String),
    ToggleModule(String),
    ModuleEarlier(String),
    ModuleLater(String),
    ModuleRegion(String, Region),
    RemoveModule(String),
    AddModule(ModuleKind),
    IntervalShorter(String),
    IntervalLonger(String),
    ToggleShowLabel(String),
    MaxWidthDown(String),
    MaxWidthUp(String),
    MaxWidthNone(String),
    ToggleHour12(String),
    CustomIcon(String, &'static str),
    /// Give keyboard focus to a text field (the key identifies it).
    EditField(String),
    SelectTheme(String),
    UseTheme(String),
    DuplicateTheme(String),
    DeleteTheme(String),
    SaveTheme,
    ThemeDark(bool),
    ThemeBackdrop(windowsrice_theme::schema::Backdrop),
    ThemeOpacityDown,
    ThemeOpacityUp,
    ThemeColour(&'static str, String),
    Font(Option<String>),
    FontSizeDown,
    FontSizeUp,
    FontWeight(Option<u16>),
    AllLabels(bool),
    HeightDown,
    HeightUp,
    PaddingDown,
    PaddingUp,
    SpacingDown,
    SpacingUp,
    VAlign(VerticalAlign),
    SelectPreset(usize),
    SavePreset,
    LoadPreset,
    DuplicatePreset,
    RenamePreset,
    DeletePreset,
    ImportPreset,
    ExportPreset,
    /// Import design code page (Phase 12).
    ImportPaste,
    ImportOpen,
    ImportAnalyze,
    ImportSelect(usize),
    ImportPreview,
    ImportSave,
    ImportClear,
    /// Scan the opened package with Microsoft Defender, when available.
    ImportScan,
    /// Copy the current design as RICE Design Code.
    CopyDesignCode,
    /// Export the current design as a .ricepack.
    ExportPackage,
    /// Start a design from scratch (Phase 12).
    CreateCustom,
    /// Edit the bottom bar (`true`) or the top bar (Phase 14).
    EditBottomBar(bool),
    /// Animation page (Phase 14). Selection and preview:
    MotionTarget(windowsrice_core::motion::AnimTarget),
    MotionKey(usize),
    MotionScrub(i8),
    /// Draft edits of the animation of a target:
    MotionToggle(windowsrice_core::motion::AnimTarget),
    MotionPreset(windowsrice_core::motion::AnimTarget, u8),
    MotionAddKey(windowsrice_core::motion::AnimTarget),
    MotionRemoveKey(windowsrice_core::motion::AnimTarget, usize),
    MotionMoveKey(windowsrice_core::motion::AnimTarget, usize, i8),
    MotionInterp(
        windowsrice_core::motion::AnimTarget,
        windowsrice_core::motion::Interpolation,
    ),
    MotionDirection(
        windowsrice_core::motion::AnimTarget,
        windowsrice_core::motion::AnimDirection,
    ),
    MotionWrap(windowsrice_core::motion::AnimTarget),
    MotionSpeed(windowsrice_core::motion::AnimTarget, i32),
    /// Media page (Phase 14): choose a file, remove it, move the video
    /// poster frame by a number of milliseconds (within the video length).
    MediaChoose,
    MediaRemove,
    MediaPoster(i32, u32),
    /// Show or hide the bottom bar.
    ToggleBottomBar,
    ToggleReserve,
    ToggleAutoHide,
    AutoHideDelayDown,
    AutoHideDelayUp,
    Fullscreen(FullscreenPolicy),
    Monitors(MonitorPlacement),
    ToggleMonitor(usize),
    Appearance(Option<bool>),
    // ---- Phase 8
    /// A gallery card.
    Card(catalog::Gallery, usize),
    /// A theme card: theme and mode (colour overrides are cleared).
    ThemeCard(String, ThemeModePreference),
    /// Choose which colour the colour editor edits.
    ColorTarget(ColorTarget),
    /// Set the edited colour (`None`: the theme's colour).
    SetColor(Option<String>),
    /// Add `delta` to one channel (0 red, 1 green, 2 blue) of the edited colour.
    ColorChannel(u8, i16),
    /// Open the Windows colour dialog for the edited colour.
    PickColor,
    /// Go to Colours with a role chosen.
    GoColor(ColorTarget),
    Slider(SliderKind, f32),
    PreviewView(PreviewView),
    GradientDirection(windowsrice_core::bar::GradientDirection),
    Material(windowsrice_core::bar::Material),
    ToggleHour12All,
    /// Per-module appearance (Phase 9).
    ModuleSet(String, ModuleField),
    /// Turn animations on or off.
    ToggleAnimations,
    /// Font gallery filter.
    FontFilter(FontFilter),
    /// Create a custom theme from the current design.
    NewTheme,
}

/// Inputs for the live preview.
#[derive(Clone)]
pub struct PreviewInputs<'a> {
    pub style: &'a PanelStyle,
    pub plan: &'a SurfacePlan,
    /// Resolves any draft the way the running bar would (for gallery card
    /// previews). `None` when its theme is not available.
    pub resolve: &'a dyn Fn(&Draft) -> Option<(PanelStyle, SurfacePlan)>,
}

impl std::fmt::Debug for PreviewInputs<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreviewInputs")
            .field("style", &self.style.theme_id)
            .finish_non_exhaustive()
    }
}

/// Colours of the Studio itself (from windowsRICE Default, dark or light).
#[derive(Debug, Clone, PartialEq)]
pub struct StudioStyle {
    pub background: Color,
    pub panel: Color,
    pub card: Color,
    pub text: Color,
    pub text2: Color,
    pub disabled: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub border: Color,
    pub control: Color,
    pub focus: Color,
    pub selection: Color,
    pub hover: Color,
    pub warning: Color,
    pub danger: Color,
    pub success: Color,
    pub family_size: f32,
}

impl StudioStyle {
    /// Windows high-contrast colours: text, backgrounds and selection come
    /// from the system so the Studio follows the chosen contrast theme.
    pub fn high_contrast(hc: &crate::style::HighContrastColors) -> Self {
        Self {
            background: hc.window,
            panel: hc.window,
            card: hc.window,
            text: hc.window_text,
            text2: hc.window_text,
            disabled: hc.gray_text,
            accent: hc.highlight,
            on_accent: hc.highlight_text,
            border: hc.window_text,
            control: hc.window_text,
            focus: hc.highlight,
            selection: hc.highlight,
            hover: Color {
                a: 0x40,
                ..hc.highlight
            },
            warning: hc.window_text,
            danger: hc.window_text,
            success: hc.window_text,
            family_size: 13.0,
        }
    }

    pub fn from_palette(p: &windowsrice_theme::Palette) -> Self {
        Self {
            background: p.background,
            panel: p.surface_base,
            card: p.surface_raised,
            text: p.text_primary,
            text2: p.text_secondary,
            disabled: p.text_disabled,
            accent: p.accent,
            on_accent: p.text_on_accent,
            border: p.border_subtle,
            control: p.border_control,
            focus: p.focus_ring,
            selection: p.selection,
            hover: Color {
                a: 0x18,
                ..p.text_primary
            },
            warning: p.warning,
            danger: p.danger,
            success: p.success,
            family_size: 13.0,
        }
    }
}

/// Swatches offered for colours (name, hex).
pub const SWATCHES: [(&str, &str); 10] = [
    ("Graphite", "#1B1D22"),
    ("Midnight", "#121826"),
    ("Slate", "#2A3140"),
    ("Forest", "#16241D"),
    ("Plum", "#251A2E"),
    ("Paper", "#F4F5F7"),
    ("Sky", "#3A8DDE"),
    ("Teal", "#1FA39A"),
    ("Amber", "#E3A33B"),
    ("Rose", "#D9577A"),
];
/// Icons offered for custom icon modules (code point, name).
pub const ICON_CHOICES: [(&str, &str); 6] = [
    ("E734", "Star"),
    ("EB51", "Heart"),
    ("E7FC", "Game"),
    ("E8D6", "Music"),
    ("E774", "Globe"),
    ("E715", "Mail"),
];
pub const MAX_WIDTH_STEPS: [u16; 9] = [60, 80, 100, 120, 160, 200, 260, 320, 400];

#[derive(Debug, Clone, PartialEq)]
pub struct StudioScene {
    pub scene: Scene,
    pub actions: Vec<(ElementId, StudioAction)>,
    /// Elements that start a drag: element → (module id or none, kind).
    pub draggable: Vec<(ElementId, Option<String>, ModuleKind)>,
    /// Drop slots: (region, index, rect where the indicator is drawn).
    pub drop_slots: Vec<(Region, usize, Rect)>,
    /// Region columns for hit-testing drops.
    pub columns: Vec<(Region, Rect)>,
    /// Text field keys and their element ids.
    pub fields: Vec<(String, ElementId)>,
    // ---- Phase 8
    /// Largest useful scroll of the section area.
    pub scroll_max: f32,
    /// The visible section area.
    pub viewport: Rect,
    pub grids: Vec<GridInfo>,
    pub sliders: Vec<SliderInfo>,
}

impl StudioScene {
    pub fn action(&self, id: ElementId) -> Option<&StudioAction> {
        self.actions.iter().find(|(e, _)| *e == id).map(|(_, a)| a)
    }
    pub fn element(&self, a: &StudioAction) -> Option<ElementId> {
        self.actions.iter().find(|(_, x)| x == a).map(|(e, _)| *e)
    }
    /// Where a drop at `(x, y)` would land.
    pub fn drop_target(&self, x: f32, y: f32) -> Option<(Region, usize)> {
        let (region, col) = self
            .columns
            .iter()
            .find(|(_, r)| x >= r.x && x < r.right() && y >= r.y - 20.0 && y < r.bottom() + 20.0)?;
        let slots: Vec<&(Region, usize, Rect)> = self
            .drop_slots
            .iter()
            .filter(|(r, _, _)| r == region)
            .collect();
        let _ = col;
        slots
            .iter()
            .min_by(|a, b| {
                (a.2.center_y() - y)
                    .abs()
                    .total_cmp(&(b.2.center_y() - y).abs())
            })
            .map(|s| (s.0, s.1))
    }
}

fn key_id(key: &str) -> ElementId {
    let mut h: u32 = 0x811C_9DC5;
    for b in key.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    ElementId(0x100 | (h & 0x7FFF_FFFF))
}

fn hex_color(h: &str) -> Option<Color> {
    parse_hex_color(h).map(|(r, g, b)| Color::rgb(r, g, b))
}

fn t<'a>(lang: Lang, en: &'a str, ko: &'a str) -> &'a str {
    if lang == Lang::En { en } else { ko }
}

struct Ui<'a> {
    tree: UiTree,
    draw: DrawList,
    actions: Vec<(ElementId, StudioAction)>,
    draggable: Vec<(ElementId, Option<String>, ModuleKind)>,
    drop_slots: Vec<(Region, usize, Rect)>,
    columns: Vec<(Region, Rect)>,
    fields: Vec<(String, ElementId)>,
    s: &'a StudioStyle,
    m: &'a StudioModel,
    measure: &'a dyn TextMeasure,
    scale: f32,
    lang: Lang,
    // ---- Phase 8
    p: &'a PreviewInputs<'a>,
    strings: &'a Strings,
    /// Visible part of the scrolled section area (elements outside are left
    /// out of the tree and not drawn).
    clip: Option<Rect>,
    /// Lowest point reached by content in the scrolled area.
    content_bottom: f32,
    grids: Vec<GridInfo>,
    sliders: Vec<SliderInfo>,
}

/// Id returned for elements outside the visible area (never in the tree).
const HIDDEN: ElementId = ElementId(0);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Primary,
    Secondary,
    Danger,
    Quiet,
}

impl Ui<'_> {
    fn ts(&self, size: f32, weight: u16) -> TextStyle {
        TextStyle {
            font: FontRole::Ui,
            size,
            weight,
        }
    }
    fn icon(&self, size: f32) -> TextStyle {
        TextStyle {
            font: FontRole::Icon,
            size,
            weight: 400,
        }
    }
    fn width(&self, text: &str, st: &TextStyle) -> f32 {
        self.measure.measure(text, st).w
    }
    fn add(&mut self, parent: ElementId, mut e: Element) -> ElementId {
        if let Some(v) = self.clip {
            self.content_bottom = self.content_bottom.max(e.bounds.bottom());
            match e.bounds.intersect(&v) {
                Some(b) if b.w >= 1.0 && b.h >= 1.0 => e.bounds = b,
                _ => return HIDDEN,
            }
        }
        if !self.tree.contains(parent) {
            return HIDDEN;
        }
        while self.tree.contains(e.id) {
            e.id = ElementId(e.id.0.wrapping_add(1) | 0x100);
        }
        let id = e.id;
        self.tree.add(parent, e).expect("ids made unique above");
        id
    }
    /// Whether `r` can be seen (always, outside the scrolled area).
    fn visible(&self, r: Rect) -> bool {
        self.clip.is_none_or(|v| v.intersects(&r))
    }
    fn fill(&mut self, r: Rect, radius: f32, c: Color, owner: ElementId) {
        if owner == HIDDEN || !self.visible(r) {
            return;
        }
        self.draw.push(DrawCmd::Fill {
            rect: r,
            radius,
            color: c,
            owner: Some(owner),
        });
    }
    fn stroke(&mut self, r: Rect, radius: f32, w: f32, c: Color, owner: ElementId) {
        if owner == HIDDEN || !self.visible(r) {
            return;
        }
        self.draw.push(DrawCmd::Stroke {
            rect: r,
            radius,
            width: w,
            color: c,
            owner: Some(owner),
        });
    }
    fn text(&mut self, r: Rect, s: &str, st: TextStyle, c: Color, owner: ElementId) {
        if owner == HIDDEN || !self.visible(r) {
            return;
        }
        self.draw.push(DrawCmd::Text {
            rect: r,
            text: s.to_owned(),
            style: st,
            color: c,
            trim: true,
            owner: Some(owner),
        });
    }
    #[allow(clippy::too_many_arguments)]
    fn label(
        &mut self,
        parent: ElementId,
        key: &str,
        r: Rect,
        s: &str,
        size: f32,
        weight: u16,
        c: Color,
    ) -> ElementId {
        let r = r.snap(self.scale);
        let id = self.add(
            parent,
            Element::new(key_id(key), intern(key), Role::Text, s, r),
        );
        let st = self.ts(size, weight);
        // Wrap long text when the box has room for at least two lines.
        if id == HIDDEN {
            return id;
        }
        if self.width(s, &st) > r.w && r.h >= size * 2.6 {
            self.draw.push(DrawCmd::Wrapped {
                rect: r,
                text: s.to_owned(),
                style: st,
                color: c,
                owner: Some(id),
            });
        } else {
            self.text(r, s, st, c, id);
        }
        id
    }
    /// Draws the hover/focus decoration of an interactive element.
    fn decorate(&mut self, id: ElementId, r: Rect, radius: f32) {
        if self.m.hover == Some(id) {
            self.fill(r, radius, self.s.hover, id);
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn button(
        &mut self,
        parent: ElementId,
        key: &str,
        r: Rect,
        glyph: Option<&str>,
        label: &str,
        name: &str,
        kind: Kind,
        enabled: bool,
        a: StudioAction,
    ) -> ElementId {
        let r = r.snap(self.scale);
        let id = self.add(
            parent,
            Element::new(key_id(key), intern(key), Role::Button, name, r)
                .focusable(enabled)
                .enabled(enabled)
                .invokable(),
        );
        let (bg, fg) = match (kind, enabled) {
            (_, false) => (Some(self.s.card), self.s.disabled),
            (Kind::Primary, _) => (Some(self.s.accent), self.s.on_accent),
            (Kind::Danger, _) => (Some(self.s.card), self.s.danger),
            (Kind::Secondary, _) => (Some(self.s.card), self.s.text),
            (Kind::Quiet, _) => (None, self.s.text),
        };
        if let Some(bg) = bg {
            self.fill(r, 6.0, bg, id);
        }
        if kind != Kind::Quiet && kind != Kind::Primary {
            self.stroke(r.inflate(-0.5), 6.0, 1.0, self.s.border, id);
        }
        self.decorate(id, r, 6.0);
        let lst = self.ts(13.0, 400);
        let ist = self.icon(14.0);
        let gw = glyph.map_or(0.0, |g| self.width(g, &ist));
        let lw = if label.is_empty() {
            0.0
        } else {
            self.width(label, &lst)
        };
        let gap = if gw > 0.0 && lw > 0.0 { 8.0 } else { 0.0 };
        let total = (gw + gap + lw).min(r.w - 12.0);
        let mut x = r.x + (r.w - total) / 2.0;
        if let Some(g) = glyph {
            self.text(Rect::new(x, r.y, gw, r.h), g, ist, fg, id);
            x += gw + gap;
        }
        if lw > 0.0 {
            self.text(
                Rect::new(x, r.y, (r.right() - 6.0 - x).max(1.0), r.h),
                label,
                lst,
                fg,
                id,
            );
        }
        self.actions.push((id, a));
        id
    }
    #[allow(clippy::too_many_arguments)]
    fn toggle(
        &mut self,
        parent: ElementId,
        key: &str,
        r: Rect,
        label: &str,
        on: bool,
        enabled: bool,
        help: Option<&str>,
        a: StudioAction,
    ) -> ElementId {
        let r = r.snap(self.scale);
        let mut e = Element::new(key_id(key), intern(key), Role::CheckBox, label, r)
            .focusable(enabled)
            .enabled(enabled)
            .toggled(on);
        if let Some(h) = help {
            e = e.help(h);
        }
        let id = self.add(parent, e);
        self.decorate(id, r, 6.0);
        // Switch: 36×18 track with a knob.
        let track = Rect::new(r.right() - 44.0, r.center_y() - 9.0, 36.0, 18.0).snap(self.scale);
        let (tc, kc) = match (on, enabled) {
            (_, false) => (self.s.card, self.s.disabled),
            (true, _) => (self.s.accent, self.s.on_accent),
            (false, _) => (self.s.card, self.s.text2),
        };
        self.fill(track, 9.0, tc, id);
        if !on {
            self.stroke(track.inflate(-0.5), 9.0, 1.0, self.s.control, id);
        }
        let kx = if on {
            track.right() - 16.0
        } else {
            track.x + 2.0
        };
        self.fill(
            Rect::new(kx, track.y + 2.0, 14.0, 14.0).snap(self.scale),
            7.0,
            kc,
            id,
        );
        let c = if enabled {
            self.s.text
        } else {
            self.s.disabled
        };
        self.text(
            Rect::new(r.x + 10.0, r.y, (r.w - 64.0).max(1.0), r.h),
            label,
            self.ts(13.0, 400),
            c,
            id,
        );
        self.actions.push((id, a));
        id
    }
    fn radio(
        &mut self,
        parent: ElementId,
        key: &str,
        r: Rect,
        label: &str,
        selected: bool,
        a: StudioAction,
    ) -> ElementId {
        let r = r.snap(self.scale);
        let id = self.add(
            parent,
            Element::new(key_id(key), intern(key), Role::RadioButton, label, r)
                .focusable(true)
                .selected(selected),
        );
        if selected {
            self.fill(r, 6.0, self.s.selection, id);
        }
        self.decorate(id, r, 6.0);
        let c = Rect::new(r.x + 10.0, r.center_y() - 8.0, 16.0, 16.0).snap(self.scale);
        self.stroke(
            c.inflate(-0.5),
            8.0,
            1.0,
            if selected {
                self.s.accent
            } else {
                self.s.control
            },
            id,
        );
        if selected {
            self.fill(c.inflate(-4.0), 4.0, self.s.accent, id);
        }
        self.text(
            Rect::new(
                c.right() + 10.0,
                r.y,
                (r.right() - c.right() - 16.0).max(1.0),
                r.h,
            ),
            label,
            self.ts(13.0, if selected { 500 } else { 400 }),
            self.s.text,
            id,
        );
        self.actions.push((id, a));
        id
    }
    /// A labelled value with − and + buttons.
    #[allow(clippy::too_many_arguments)]
    fn stepper(
        &mut self,
        parent: ElementId,
        key: &str,
        r: Rect,
        label: &str,
        value: &str,
        dec: Option<StudioAction>,
        inc: Option<StudioAction>,
    ) {
        let lid = self.label(
            parent,
            &format!("{key}:label"),
            Rect::new(r.x + 10.0, r.y, r.w - 160.0, r.h),
            &format!("{label}: {value}"),
            13.0,
            400,
            self.s.text,
        );
        let _ = lid;
        let bx = r.right() - 96.0;
        let (dname, iname) = match self.lang {
            Lang::En => (
                format!("Decrease {label} (now {value})"),
                format!("Increase {label} (now {value})"),
            ),
            Lang::Ko => (
                format!("{label} 줄이기 (현재 {value})"),
                format!("{label} 늘리기 (현재 {value})"),
            ),
        };
        let de = dec.is_some();
        let ie = inc.is_some();
        self.button(
            parent,
            &format!("{key}:dec"),
            Rect::new(bx, r.y + 3.0, 44.0, r.h - 6.0),
            Some("\u{E738}"),
            "",
            &dname,
            Kind::Secondary,
            de,
            dec.unwrap_or(StudioAction::Undo),
        );
        self.button(
            parent,
            &format!("{key}:inc"),
            Rect::new(bx + 50.0, r.y + 3.0, 44.0, r.h - 6.0),
            Some("\u{E710}"),
            "",
            &iname,
            Kind::Secondary,
            ie,
            inc.unwrap_or(StudioAction::Undo),
        );
    }
    #[allow(clippy::too_many_arguments)]
    fn swatch(
        &mut self,
        parent: ElementId,
        key: &str,
        r: Rect,
        colour: Option<Color>,
        name: &str,
        selected: bool,
        a: StudioAction,
    ) {
        let r = r.snap(self.scale);
        let id = self.add(
            parent,
            Element::new(key_id(key), intern(key), Role::RadioButton, name, r)
                .focusable(true)
                .selected(selected),
        );
        match colour {
            Some(c) => self.fill(r.inflate(-3.0), 6.0, c, id),
            None => {
                self.fill(r.inflate(-3.0), 6.0, self.s.card, id);
                self.text(
                    Rect::new(r.x + 7.0, r.y, r.w - 8.0, r.h),
                    "\u{E790}",
                    self.icon(13.0),
                    self.s.text2,
                    id,
                );
            }
        }
        self.stroke(r.inflate(-3.5), 6.0, 1.0, self.s.control, id);
        if selected {
            self.stroke(r.inflate(-0.5), 8.0, 2.0, self.s.accent, id);
        }
        self.actions.push((id, a));
    }
    /// Single-line text field. Typing starts after the field receives focus
    /// (click, Enter or the UI Automation Value pattern).
    fn field(
        &mut self,
        parent: ElementId,
        key: &str,
        r: Rect,
        label: &str,
        value: &str,
        placeholder: &str,
    ) -> ElementId {
        let r = r.snap(self.scale);
        let editing = self.m.text_edit.as_ref().filter(|e| e.key == key);
        let shown = editing.map_or(value, |e| e.buffer.as_str());
        let id = self.add(
            parent,
            Element::new(key_id(key), intern(key), Role::Edit, label, r)
                .focusable(true)
                .value(shown)
                .help(placeholder),
        );
        self.fill(r, 6.0, self.s.background, id);
        self.stroke(
            r.inflate(-0.5),
            6.0,
            if editing.is_some() { 2.0 } else { 1.0 },
            if editing.is_some() {
                self.s.accent
            } else {
                self.s.control
            },
            id,
        );
        let (txt, col) = if shown.is_empty() {
            (placeholder, self.s.disabled)
        } else {
            (shown, self.s.text)
        };
        let st = self.ts(13.0, 400);
        self.text(
            Rect::new(r.x + 10.0, r.y, r.w - 20.0, r.h),
            txt,
            st,
            col,
            id,
        );
        if let Some(te) = editing {
            let x_at = |u: &Self, ci: usize| {
                let prefix: String = shown.chars().take(ci).collect();
                (r.x + 10.0 + u.width(&prefix, &st)).min(r.right() - 8.0)
            };
            let (a, b) = te.selection();
            if a != b {
                let (xa, xb) = (x_at(self, a), x_at(self, b));
                self.fill(
                    Rect::new(xa, r.y + 6.0, xb - xa, r.h - 12.0),
                    2.0,
                    Color {
                        a: 0x66,
                        ..self.s.accent
                    },
                    id,
                );
                // Redraw the text over the selection.
                self.text(
                    Rect::new(r.x + 10.0, r.y, r.w - 20.0, r.h),
                    shown,
                    st,
                    self.s.text,
                    id,
                );
            }
            let cx = x_at(self, te.caret);
            self.fill(
                Rect::new(cx + 0.5, r.y + 7.0, 1.5, r.h - 14.0),
                0.0,
                self.s.text,
                id,
            );
        }
        self.fields.push((key.to_owned(), id));
        self.actions
            .push((id, StudioAction::EditField(key.to_owned())));
        id
    }
    fn group(&mut self, parent: ElementId, key: &str, name: &str, r: Rect) -> ElementId {
        self.add(
            parent,
            Element::new(
                key_id(key),
                intern(key),
                Role::Group,
                name,
                r.snap(self.scale),
            ),
        )
    }
    fn card(&mut self, r: Rect, owner: ElementId) {
        self.fill(r, 10.0, self.s.panel, owner);
        self.stroke(r.inflate(-0.5), 10.0, 1.0, self.s.border, owner);
    }
    fn heading(&mut self, parent: ElementId, key: &str, r: Rect, s: &str) {
        self.label(parent, key, r, s, 15.0, 600, self.s.text);
    }
}

fn interval_text(ms: Option<u32>, lang: Lang) -> String {
    crate::popup::interval_label(ms, lang)
}

/// Builds the Studio scene for a window of `size` DIPs.
pub fn build_studio(
    model: &StudioModel,
    s: &StudioStyle,
    preview: &PreviewInputs<'_>,
    measure: &dyn TextMeasure,
    size: Size,
    scale: f32,
    strings: &Strings,
) -> StudioScene {
    let scene = build_once(model, s, preview, measure, size, scale, strings);
    if model.scroll > scene.scroll_max + 0.5 || model.scroll < 0.0 {
        // Scrolled past the content (for example after the window grew):
        // build again at the nearest valid position.
        let mut m = model.clone();
        m.scroll = model.scroll.clamp(0.0, scene.scroll_max);
        return build_once(&m, s, preview, measure, size, scale, strings);
    }
    scene
}

#[allow(clippy::too_many_arguments)]
fn build_once(
    model: &StudioModel,
    s: &StudioStyle,
    preview: &PreviewInputs<'_>,
    measure: &dyn TextMeasure,
    size: Size,
    scale: f32,
    strings: &Strings,
) -> StudioScene {
    let lang = strings.lang;
    let (w, h) = (size.w.max(MIN_SIZE.w), size.h.max(MIN_SIZE.h));
    let full = Rect::new(0.0, 0.0, w, h);
    let tree = UiTree::new(Element::new(
        ElementId(1),
        "studio",
        Role::Window,
        t(
            lang,
            "windowsRICE Design Studio",
            "windowsRICE 디자인 스튜디오",
        ),
        full,
    ));
    let mut u = Ui {
        tree,
        draw: DrawList::default(),
        actions: Vec::new(),
        draggable: Vec::new(),
        drop_slots: Vec::new(),
        columns: Vec::new(),
        fields: Vec::new(),
        s,
        m: model,
        measure,
        scale,
        lang,
        p: preview,
        strings,
        clip: None,
        content_bottom: 0.0,
        grids: Vec::new(),
        sliders: Vec::new(),
    };
    let root = ElementId(1);
    u.fill(full, 0.0, s.background, root);

    // ---------------------------------------------------------------- header
    let hdr = u.group(
        root,
        "header",
        t(lang, "Toolbar", "도구 모음"),
        Rect::new(0.0, 0.0, w, HEADER),
    );
    u.label(
        hdr,
        "title",
        Rect::new(PAD + 2.0, 0.0, 300.0, HEADER),
        t(lang, "Design Studio", "디자인 스튜디오"),
        20.0,
        600,
        s.text,
    );
    let dirty = model.draft != model.applied;
    let (msg, sev) = model.status.clone().unwrap_or_else(|| {
        if !model.blocking.is_empty() {
            (model.blocking[0].clone(), Severity::Error)
        } else if dirty {
            (
                t(
                    lang,
                    "Changes not applied yet",
                    "아직 적용하지 않은 변경 사항이 있습니다",
                )
                .to_owned(),
                Severity::Info,
            )
        } else {
            (
                t(
                    lang,
                    "All changes applied",
                    "모든 변경 사항이 적용되었습니다",
                )
                .to_owned(),
                Severity::Success,
            )
        }
    });
    let mc = match sev {
        Severity::Info => s.text2,
        Severity::Success => s.success,
        Severity::Warning => s.warning,
        Severity::Error => s.danger,
    };
    let status = u.add(
        hdr,
        Element::new(
            key_id("status"),
            "status",
            Role::Text,
            msg.clone(),
            Rect::new(250.0, 12.0, (w - 250.0 - 480.0).max(80.0), 36.0).snap(scale),
        )
        .focusable(true),
    );
    u.fill(
        Rect::new(250.0, HEADER / 2.0 - 4.0, 8.0, 8.0),
        4.0,
        mc,
        status,
    );
    u.text(
        Rect::new(266.0, 12.0, (w - 266.0 - 480.0).max(60.0), 36.0),
        &msg,
        u.ts(13.0, 400),
        mc,
        status,
    );
    let mut bx = w - PAD;
    let apply_ok = dirty && model.blocking.is_empty();
    for (key, glyph, label, name, kind, enabled, a) in [
        (
            "apply",
            "\u{E73E}",
            t(lang, "Apply", "적용"),
            t(lang, "Apply changes to the bar", "바에 변경 사항 적용"),
            Kind::Primary,
            apply_ok,
            StudioAction::Apply,
        ),
        (
            "revert",
            "\u{E7A7}",
            t(lang, "Revert", "되돌리기"),
            t(
                lang,
                "Discard changes not applied yet",
                "적용하지 않은 변경 사항 버리기",
            ),
            Kind::Secondary,
            dirty,
            StudioAction::Revert,
        ),
        (
            "redo",
            "\u{E7A6}",
            "",
            t(lang, "Redo", "다시 실행"),
            Kind::Secondary,
            model.can_redo,
            StudioAction::Redo,
        ),
        (
            "undo",
            "\u{E7A7}",
            "",
            t(lang, "Undo", "실행 취소"),
            Kind::Secondary,
            model.can_undo,
            StudioAction::Undo,
        ),
    ] {
        let bw = if label.is_empty() { 44.0 } else { 108.0 };
        bx -= bw;
        u.button(
            hdr,
            key,
            Rect::new(bx, 12.0, bw, 36.0),
            Some(glyph),
            label,
            name,
            kind,
            enabled,
            a,
        );
        bx -= 8.0;
    }
    u.fill(Rect::new(0.0, HEADER - 1.0, w, 1.0), 0.0, s.border, hdr);

    // ---------------------------------------------------------------- sidebar
    let nav = u.group(
        root,
        "nav",
        t(lang, "Sections", "섹션"),
        Rect::new(0.0, HEADER, SIDEBAR, h - HEADER),
    );
    u.fill(
        Rect::new(0.0, HEADER, SIDEBAR, h - HEADER),
        0.0,
        s.panel,
        nav,
    );
    // Six groups (Phase 12); the selected group lists its pages underneath
    // (automation ids "group:<key>" and "nav:<page>").
    let mut y = HEADER + 16.0;
    let current = Group::of(model.section);
    for g in Group::ALL {
        let r = Rect::new(12.0, y, SIDEBAR - 24.0, 40.0).snap(scale);
        let open = g == current;
        let single = g.sections().len() == 1;
        let selected = open && single;
        let key = format!("group:{}", g.key());
        let id = u.add(
            nav,
            Element::new(key_id(&key), intern(&key), Role::ListItem, g.name(lang), r)
                .focusable(true)
                .selected(selected),
        );
        if selected {
            u.fill(r, 8.0, s.selection, id);
            u.fill(Rect::new(r.x, r.y + 10.0, 3.0, 20.0), 1.5, s.accent, id);
        }
        u.decorate(id, r, 8.0);
        u.text(
            Rect::new(r.x + 16.0, r.y, 20.0, r.h),
            g.glyph(),
            u.icon(16.0),
            if open { s.accent } else { s.text2 },
            id,
        );
        u.text(
            Rect::new(r.x + 46.0, r.y, r.w - 52.0, r.h),
            g.name(lang),
            u.ts(14.0, if open { 600 } else { 400 }),
            s.text,
            id,
        );
        let target = if open { model.section } else { g.sections()[0] };
        u.actions.push((id, StudioAction::Nav(target)));
        y += 44.0;
        if !open || single {
            continue;
        }
        for &sec in g.sections() {
            let r = Rect::new(36.0, y, SIDEBAR - 48.0, 34.0).snap(scale);
            let selected = sec == model.section;
            let key = format!("nav:{}", sec.key());
            let id = u.add(
                nav,
                Element::new(
                    key_id(&key),
                    intern(&key),
                    Role::ListItem,
                    sec.name(lang),
                    r,
                )
                .focusable(true)
                .selected(selected),
            );
            if selected {
                u.fill(r, 8.0, s.selection, id);
                u.fill(Rect::new(r.x, r.y + 8.0, 3.0, 18.0), 1.5, s.accent, id);
            }
            u.decorate(id, r, 8.0);
            u.text(
                Rect::new(r.x + 14.0, r.y, r.w - 18.0, r.h),
                sec.name(lang),
                u.ts(13.0, if selected { 600 } else { 400 }),
                if selected { s.text } else { s.text2 },
                id,
            );
            u.actions.push((id, StudioAction::Nav(sec)));
            y += 36.0;
        }
        y += 4.0;
    }

    // ---------------------------------------------------------------- preview
    let cx = SIDEBAR + PAD;
    let cw = w - SIDEBAR - 2.0 * PAD;
    let ph = 196.0;
    let pv = u.group(
        root,
        "preview",
        t(lang, "Live preview", "실시간 미리 보기"),
        Rect::new(cx, HEADER + PAD, cw, ph),
    );
    u.card(Rect::new(cx, HEADER + PAD, cw, ph), pv);
    u.label(
        pv,
        "preview:title",
        Rect::new(cx + 16.0, HEADER + PAD + 8.0, 300.0, 28.0),
        t(lang, "Live preview", "실시간 미리 보기"),
        14.0,
        600,
        s.text,
    );
    // View tabs (left) and backdrop choice (right).
    let mut tx = cx + 130.0;
    for (view, en, ko, en_name, ko_name) in [
        (
            PreviewView::All,
            "All",
            "전체",
            "Whole bar and three close-ups",
            "바 전체와 세 부분 확대",
        ),
        (
            PreviewView::Whole,
            "Whole",
            "바 전체",
            "Whole bar, larger, in two halves",
            "바 전체를 두 줄로 크게",
        ),
        (
            PreviewView::Left,
            "Left",
            "왼쪽",
            "Left end at actual size",
            "왼쪽 끝 실제 크기",
        ),
        (
            PreviewView::Center,
            "Centre",
            "가운데",
            "Middle at actual size",
            "가운데 실제 크기",
        ),
        (
            PreviewView::Right,
            "Right",
            "오른쪽",
            "Right end at actual size",
            "오른쪽 끝 실제 크기",
        ),
        (
            PreviewView::Module,
            "Module",
            "모듈",
            "Selected module, enlarged",
            "선택한 모듈 확대",
        ),
    ] {
        let key = format!("preview:view:{en}");
        let r = Rect::new(tx, HEADER + PAD + 8.0, 72.0, 28.0).snap(scale);
        let sel = model.preview_view == view;
        let id = u.add(
            pv,
            Element::new(
                key_id(&key),
                intern(&key),
                Role::RadioButton,
                t(lang, en_name, ko_name),
                r,
            )
            .focusable(true)
            .selected(sel),
        );
        u.fill(r, 14.0, if sel { s.selection } else { s.card }, id);
        u.decorate(id, r, 14.0);
        u.text(
            Rect::new(r.x + 10.0, r.y, r.w - 14.0, r.h),
            t(lang, en, ko),
            u.ts(12.0, if sel { 500 } else { 400 }),
            s.text,
            id,
        );
        u.actions.push((id, StudioAction::PreviewView(view)));
        tx += 76.0;
    }
    let mut kx = cx + cw - 16.0;
    for (bd, en, ko) in [
        (PreviewBackdrop::Colour, "Colourful", "다채로움"),
        (PreviewBackdrop::Light, "Light", "밝음"),
        (PreviewBackdrop::Dark, "Dark", "어두움"),
    ] {
        kx -= 96.0;
        let name = match lang {
            Lang::En => format!("Preview over a {} backdrop", en.to_lowercase()),
            Lang::Ko => format!("{ko} 배경 위에서 미리 보기"),
        };
        let key = format!("preview:backdrop:{en}");
        let r = Rect::new(kx, HEADER + PAD + 8.0, 92.0, 28.0).snap(scale);
        let sel = model.preview_backdrop == bd;
        let id = u.add(
            pv,
            Element::new(key_id(&key), intern(&key), Role::RadioButton, name, r)
                .focusable(true)
                .selected(sel),
        );
        u.fill(r, 14.0, if sel { s.selection } else { s.card }, id);
        u.decorate(id, r, 14.0);
        u.text(
            Rect::new(r.x + 10.0, r.y, r.w - 14.0, r.h),
            t(lang, en, ko),
            u.ts(12.0, if sel { 500 } else { 400 }),
            s.text,
            id,
        );
        u.actions.push((id, StudioAction::PreviewBackdrop(bd)));
    }
    // The bar, laid out at the monitor's real width.
    let bar_w = model.monitor_width.max(800.0);
    let bar_h = model.bar_height;
    let bs = build_bar(
        &BarModel {
            modules: model.draft.bar.modules.clone(),
            readings: model.readings.clone(),
            decor: Decor::from_style(&model.draft.bar.style),
            background_file: model.background_file.clone(),
            anim_ms: (model.section == Section::Motion).then(|| {
                let d = model
                    .draft
                    .bar
                    .style
                    .animations
                    .iter()
                    .find(|a| a.target == model.motion_target)
                    .map_or(1000, |a| a.duration_ms);
                u64::from(d) * u64::from(model.motion_scrub) / 100
            }),
            ..Default::default()
        },
        preview.style,
        preview.plan,
        measure,
        Size { w: bar_w, h: bar_h },
        1.0,
        strings,
        f32::from(model.draft.bar.spacing),
    );
    let inner_w = cw - 32.0;
    let area = Rect::new(cx + 16.0, HEADER + PAD + 44.0, inner_w, ph - 52.0);
    let whole = preview_views(&mut u, pv, area, &bs, bar_w, bar_h);
    let shown: Vec<String> = bs
        .elements
        .iter()
        .filter_map(|(id, _)| model.draft.bar.modules.iter().find(|m| &m.id == id))
        .map(|m| module_name(m, lang))
        .collect();
    let summary = match lang {
        Lang::En => format!(
            "Preview shows {} modules: {}{}",
            shown.len(),
            shown.join(", "),
            if bs.hidden.is_empty() {
                String::new()
            } else {
                format!(". {} hidden for lack of space", bs.hidden.len())
            }
        ),
        Lang::Ko => format!(
            "미리 보기의 모듈 {}개: {}{}",
            shown.len(),
            shown.join(", "),
            if bs.hidden.is_empty() {
                String::new()
            } else {
                format!(". 공간 부족으로 {}개 숨김", bs.hidden.len())
            }
        ),
    };
    u.add(
        pv,
        Element::new(
            key_id("preview:summary"),
            "preview:summary",
            Role::Text,
            summary,
            whole.snap(scale),
        )
        .focusable(true),
    );

    // ---------------------------------------------------------------- body
    let by = HEADER + PAD + ph + PAD;
    let bh = h - by - PAD;
    let mw = cw - PROPS - PAD;
    let main = Rect::new(cx, by, mw, bh);
    let props = Rect::new(cx + mw + PAD, by, PROPS, bh);
    let body = u.group(root, "section", model.section.name(lang), main);
    u.card(main, body);
    let pg = u.group(root, "properties", t(lang, "Properties", "속성"), props);
    u.card(props, pg);
    let inner = Rect::new(main.x + 18.0, main.y + 14.0, main.w - 36.0, main.h - 28.0);
    let pinner = Rect::new(
        props.x + 16.0,
        props.y + 14.0,
        props.w - 32.0,
        props.h - 28.0,
    );
    // The section area scrolls: content is laid out from `top` and clipped
    // to `inner`; elements outside are left out of the tree.
    let top = inner.y - model.scroll;
    u.draw.push(DrawCmd::PushClip { rect: inner });
    u.clip = Some(inner);
    u.content_bottom = top;
    let content = Rect::new(inner.x, top, inner.w, inner.h);
    match model.section {
        Section::Home => gallery::home(&mut u, body, content),
        Section::Styles => gallery::styles(&mut u, body, content),
        Section::Themes => gallery::themes(&mut u, body, content),
        Section::Colors => gallery::colors(&mut u, body, content),
        Section::Background => gallery::background(&mut u, body, content),
        Section::Typography => gallery::typography(&mut u, body, content),
        Section::Icons => gallery::icons(&mut u, body, content),
        Section::Modules => gallery::modules(&mut u, body, content),
        Section::Layout => gallery::layout(&mut u, body, content),
        Section::Effects => gallery::effects(&mut u, body, content),
        Section::Presets => presets(&mut u, body, content),
        Section::Import => import_page(&mut u, body, content),
        Section::Motion => motion_page(&mut u, body, content),
        Section::Media => media_page(&mut u, body, content),
        Section::Advanced => gallery::advanced(&mut u, body, content),
    }
    u.clip = None;
    u.draw.push(DrawCmd::PopClip);
    let scroll_max = (u.content_bottom + 12.0 - top - inner.h).max(0.0);
    if scroll_max > 0.0 {
        // Scroll position indicator.
        let frac = (inner.h / (inner.h + scroll_max)).clamp(0.08, 1.0);
        let th = inner.h * frac;
        let ty = inner.y + (inner.h - th) * (model.scroll / scroll_max).clamp(0.0, 1.0);
        u.fill(
            Rect::new(main.right() - 8.0, ty, 4.0, th),
            2.0,
            Color { a: 0x66, ..s.text2 },
            body,
        );
    }
    gallery::properties(&mut u, pg, pinner);

    // Drag ghost and drop indicator.
    if let Some(d) = &model.drag {
        if let Some((region, index)) = d.target
            && let Some((_, _, r)) = u
                .drop_slots
                .iter()
                .find(|(rg, i, _)| *rg == region && *i == index)
                .cloned()
        {
            u.fill(
                Rect::new(r.x, r.center_y() - 1.5, r.w, 3.0),
                1.5,
                s.accent,
                root,
            );
        }
        let name = d
            .module
            .as_ref()
            .and_then(|id| model.draft.bar.modules.iter().find(|m| &m.id == id))
            .map_or_else(
                || kind_name(d.kind, lang).to_owned(),
                |m| module_name(m, lang),
            );
        let g = Rect::new(d.x - 90.0, d.y - 16.0, 180.0, 32.0);
        u.fill(g, 8.0, Color { a: 0xE6, ..s.card }, root);
        u.stroke(g.inflate(-0.5), 8.0, 1.5, s.accent, root);
        u.text(
            Rect::new(g.x + 12.0, g.y, g.w - 20.0, g.h),
            &name,
            u.ts(13.0, 500),
            s.text,
            root,
        );
    }

    // Focus ring last.
    if model.keyboard_active
        && let Some(f) = model.focus
        && let Some(e) = u.tree.get(f)
        && e.is_focus_target()
    {
        let ring = e.bounds.inflate(2.0);
        u.draw.push(DrawCmd::Stroke {
            rect: ring,
            radius: 8.0,
            width: 2.0,
            color: s.focus,
            owner: Some(f),
        });
    }

    // Groups whose members all lie outside the visible area are left out.
    u.tree.prune_empty_groups();
    // Leave out entries of elements that were not visible.
    let actions: Vec<_> = u
        .actions
        .into_iter()
        .filter(|(e, _)| u.tree.contains(*e))
        .collect();
    let draggable: Vec<_> = u
        .draggable
        .into_iter()
        .filter(|(e, _, _)| u.tree.contains(*e))
        .collect();
    let fields: Vec<_> = u
        .fields
        .into_iter()
        .filter(|(_, e)| u.tree.contains(*e))
        .collect();
    let sliders: Vec<_> = u
        .sliders
        .into_iter()
        .filter(|x| u.tree.contains(x.element))
        .collect();
    StudioScene {
        scene: Scene {
            tree: u.tree,
            draw: u.draw,
            size: Size { w, h },
            scale,
        },
        actions,
        draggable,
        drop_slots: u.drop_slots,
        columns: u.columns,
        fields,
        scroll_max,
        viewport: inner,
        grids: u.grids,
        sliders,
    }
}

/// Draws the chosen preview view of the real bar layout `bs` into `area`.
/// Returns the rectangle of the main picture (for the accessible summary).
fn preview_views(
    u: &mut Ui,
    pv: ElementId,
    area: Rect,
    bs: &crate::bar::BarScene,
    bar_w: f32,
    bar_h: f32,
) -> Rect {
    let lang = u.lang;
    let s = u.s;
    let draw_part = |u: &mut Ui, r: Rect, src_x: f32, src_y: f32, f: f32| {
        u.draw.push(DrawCmd::PushClip { rect: r });
        gallery::paint_backdrop(u, r, pv);
        u.draw.cmds.extend(transformed(
            &bs.scene.draw.cmds,
            r.x - src_x * f,
            r.y - src_y * f,
            f,
        ));
        u.draw.push(DrawCmd::PopClip);
    };
    let caption = |u: &mut Ui, key: &str, r: Rect, en: &str, ko: &str| {
        u.label(
            pv,
            key,
            Rect::new(r.x, r.bottom() + 2.0, r.w, 18.0),
            t(lang, en, ko),
            11.0,
            400,
            s.text2,
        );
    };
    match u.m.preview_view {
        PreviewView::All => {
            let f = area.w / bar_w;
            let whole = Rect::new(area.x, area.y, area.w, bar_h * f + 16.0);
            draw_part(u, whole, 0.0, -8.0 / f, f);
            caption(
                u,
                "preview:whole",
                whole,
                "Whole bar (scaled)",
                "바 전체 (축소)",
            );
            let crop_y = whole.bottom() + 24.0;
            let crop_w = (area.w - 16.0) / 3.0;
            let crop_h = bar_h.min(area.bottom() - crop_y - 20.0).max(8.0);
            for (i, (en, ko)) in [
                ("Left end, actual size", "왼쪽 끝, 실제 크기"),
                ("Middle, actual size", "가운데, 실제 크기"),
                ("Right end, actual size", "오른쪽 끝, 실제 크기"),
            ]
            .iter()
            .enumerate()
            {
                let r = Rect::new(area.x + i as f32 * (crop_w + 8.0), crop_y, crop_w, crop_h);
                let src_x = match i {
                    0 => 0.0,
                    1 => (bar_w - crop_w) / 2.0,
                    _ => bar_w - crop_w,
                };
                if i == 2 {
                    // Start the right close-up at the first module that fits whole,
                    // so no module is cut in half at its left edge.
                    let first = bs
                        .elements
                        .iter()
                        .filter_map(|(_, e)| bs.scene.tree.get(*e).map(|el| el.bounds.x))
                        .filter(|x| *x >= src_x)
                        .fold(f32::MAX, f32::min);
                    let cut = if first < bar_w {
                        (first - src_x - 4.0).max(0.0)
                    } else {
                        0.0
                    };
                    draw_part(
                        u,
                        Rect::new(r.x + cut, r.y, r.w - cut, r.h),
                        src_x + cut,
                        (bar_h - crop_h) / 2.0,
                        1.0,
                    );
                } else {
                    draw_part(u, r, src_x, (bar_h - crop_h) / 2.0, 1.0);
                }
                caption(u, &format!("preview:crop{i}"), r, en, ko);
            }
            whole
        }
        PreviewView::Whole => {
            let half = bar_w / 2.0;
            let f = (area.w / half).min((area.h - 40.0) / (2.0 * bar_h));
            let ph = bar_h * f;
            let top = Rect::new(area.x, area.y, half * f, ph);
            let bottom = Rect::new(area.x, area.y + ph + 18.0, half * f, ph);
            draw_part(u, top, 0.0, 0.0, f);
            draw_part(u, bottom, half, 0.0, f);
            caption(u, "preview:half1", top, "Left half", "왼쪽 절반");
            caption(u, "preview:half2", bottom, "Right half", "오른쪽 절반");
            top.union(&bottom)
        }
        PreviewView::Left | PreviewView::Center | PreviewView::Right => {
            let f = ((area.h - 24.0) / bar_h).clamp(1.0, 2.0);
            let w = area.w / f;
            let src_x = match u.m.preview_view {
                PreviewView::Left => 0.0,
                PreviewView::Center => (bar_w - w) / 2.0,
                _ => bar_w - w,
            };
            let r = Rect::new(area.x, area.y, area.w, bar_h * f);
            draw_part(u, r, src_x, 0.0, f);
            caption(
                u,
                "preview:part",
                r,
                &format!("{}% of actual size", (f * 100.0).round()),
                &format!("실제 크기의 {}%", (f * 100.0).round()),
            );
            r
        }
        PreviewView::Module => {
            let want =
                u.m.selected_module
                    .clone()
                    .unwrap_or_else(|| "clock".to_owned());
            let el = bs
                .elements
                .iter()
                .find(|(id, _)| *id == want)
                .or_else(|| bs.elements.first())
                .map(|(_, e)| *e);
            let src = el
                .and_then(|e| bs.scene.tree.get(e))
                .map_or(Rect::new(0.0, 0.0, 200.0, bar_h), |e| e.bounds);
            let f = ((area.h - 24.0) / bar_h).clamp(1.0, 3.0);
            let w = area.w / f;
            let src_x = (src.x + src.w / 2.0 - w / 2.0).clamp(0.0, (bar_w - w).max(0.0));
            let r = Rect::new(area.x, area.y, area.w, bar_h * f);
            draw_part(u, r, src_x, 0.0, f);
            caption(
                u,
                "preview:module",
                r,
                &format!("Module {want}, {}%", (f * 100.0).round()),
                &format!("모듈 {want}, {}%", (f * 100.0).round()),
            );
            r
        }
    }
}

// ------------------------------------------------------------------ sections

fn section_title(u: &mut Ui, g: ElementId, r: Rect, title: &str, hint: &str) -> f32 {
    u.label(
        g,
        "section:title",
        Rect::new(r.x, r.y, r.w, 28.0),
        title,
        17.0,
        600,
        u.s.text,
    );
    u.label(
        g,
        "section:hint",
        Rect::new(r.x, r.y + 28.0, r.w, 22.0),
        hint,
        12.0,
        400,
        u.s.text2,
    );
    r.y + 60.0
}

fn modules(u: &mut Ui, g: ElementId, r: Rect) {
    let lang = u.lang;
    let m = u.m;
    let mut y = section_title(
        u,
        g,
        r,
        t(lang, "Modules", "모듈"),
        t(
            lang,
            "Drag cards between sections, or select a card and use the buttons on the right. Add modules from the row below.",
            "카드를 끌어 옮기거나, 카드를 선택하고 오른쪽 단추를 사용하세요. 아래 줄에서 모듈을 추가합니다.",
        ),
    );
    // Palette: kinds not present, plus custom modules.
    let present: Vec<ModuleKind> = m.draft.bar.modules.iter().map(|x| x.kind).collect();
    let full = m.draft.bar.modules.len() >= STUDIO_MAX_MODULES;
    let mut px = r.x;
    let avail: Vec<ModuleKind> = ModuleKind::ALL
        .iter()
        .copied()
        .filter(|k| k.is_custom() || !present.contains(k))
        .collect();
    let pal = u.group(
        g,
        "modules:palette",
        t(lang, "Available modules", "추가할 수 있는 모듈"),
        Rect::new(r.x, y, r.w, 32.0),
    );
    for k in avail {
        let name = kind_name(k, lang);
        let bw = u.width(name, &u.ts(12.0, 400)) + 44.0;
        if px + bw > r.right() {
            break;
        }
        let label = match lang {
            Lang::En => format!("Add {name}"),
            Lang::Ko => format!("{name} 추가"),
        };
        let key = format!("add:{k:?}");
        let id = u.button(
            pal,
            &key,
            Rect::new(px, y, bw, 30.0),
            Some("\u{E710}"),
            name,
            &label,
            Kind::Secondary,
            !full,
            StudioAction::AddModule(k),
        );
        if !full {
            u.draggable.push((id, None, k));
        }
        px += bw + 6.0;
    }
    y += 42.0;
    if full {
        u.label(
            g,
            "modules:full",
            Rect::new(r.x, y - 8.0, r.w, 18.0),
            t(
                lang,
                "The bar holds up to 20 modules in Design Studio. Remove one to add another.",
                "디자인 스튜디오에서는 바에 모듈을 20개까지 둘 수 있습니다.",
            ),
            11.0,
            400,
            u.s.warning,
        );
        y += 12.0;
    }
    let col_w = (r.w - 2.0 * 12.0) / 3.0;
    let col_h = r.bottom() - y;
    for (ci, (region, en, ko)) in [
        (Region::Left, "Left", "왼쪽"),
        (Region::Center, "Center", "가운데"),
        (Region::Right, "Right", "오른쪽"),
    ]
    .iter()
    .enumerate()
    {
        let cr = Rect::new(r.x + ci as f32 * (col_w + 12.0), y, col_w, col_h);
        let gname = match lang {
            Lang::En => format!("{en} section"),
            Lang::Ko => format!("{ko} 영역"),
        };
        let cg = u.group(g, &format!("col:{ci}"), &gname, cr);
        u.fill(cr, 8.0, u.s.background, cg);
        u.columns.push((*region, cr));
        u.label(
            cg,
            &format!("col:{ci}:title"),
            Rect::new(cr.x + 12.0, cr.y + 4.0, cr.w - 24.0, 26.0),
            &gname,
            13.0,
            600,
            u.s.text2,
        );
        let items: Vec<&ModuleConfig> = m
            .draft
            .bar
            .modules
            .iter()
            .filter(|x| x.region == *region)
            .collect();
        let top = cr.y + 34.0;
        let avail_h = cr.bottom() - top - 6.0;
        // One sub-column, or two when the cards would not fit.
        let sub = if items.len() as f32 * 24.0 > avail_h {
            2
        } else {
            1
        };
        let per_col = items.len().div_ceil(sub).max(1);
        let card_h = (avail_h / per_col as f32 - 3.0).clamp(20.0, 32.0);
        let sub_w = (cr.w - 16.0 - 6.0 * (sub - 1) as f32) / sub as f32;
        u.drop_slots
            .push((*region, 0, Rect::new(cr.x + 8.0, top - 2.0, sub_w, 2.0)));
        if items.is_empty() {
            u.label(
                cg,
                &format!("col:{ci}:empty"),
                Rect::new(cr.x + 12.0, top, cr.w - 24.0, 26.0),
                t(
                    lang,
                    "Empty. Drop a module here.",
                    "비어 있음. 여기에 모듈을 놓으세요.",
                ),
                12.0,
                400,
                u.s.disabled,
            );
        }
        for (i, mc) in items.iter().enumerate() {
            let (sc, row) = (i / per_col, i % per_col);
            let card = Rect::new(
                cr.x + 8.0 + sc as f32 * (sub_w + 6.0),
                top + row as f32 * (card_h + 3.0),
                sub_w,
                card_h,
            )
            .snap(u.scale);
            let name = module_name(mc, lang);
            let selected = m.selected_module.as_deref() == Some(mc.id.as_str());
            let state = if mc.enabled {
                t(lang, "shown", "표시")
            } else {
                t(lang, "hidden", "숨김")
            };
            let aname = match lang {
                Lang::En => format!(
                    "{name}, {en} section, position {} of {}, {state}",
                    i + 1,
                    items.len()
                ),
                Lang::Ko => format!(
                    "{name}, {ko} 영역, {}번째/{}개, {state}",
                    i + 1,
                    items.len()
                ),
            };
            let key = format!("card:{}", mc.id);
            let id = u.add(
                cg,
                Element::new(key_id(&key), intern(&key), Role::ListItem, aname, card)
                    .focusable(true)
                    .selected(selected),
            );
            let dragging = m
                .drag
                .as_ref()
                .is_some_and(|d| d.module.as_deref() == Some(mc.id.as_str()));
            u.fill(
                card,
                6.0,
                if selected { u.s.selection } else { u.s.card },
                id,
            );
            u.stroke(
                card.inflate(-0.5),
                6.0,
                1.0,
                if selected { u.s.accent } else { u.s.border },
                id,
            );
            u.decorate(id, card, 6.0);
            let tc = if !mc.enabled || dragging {
                u.s.disabled
            } else {
                u.s.text
            };
            u.text(
                Rect::new(card.x + 8.0, card.y, 16.0, card.h),
                "\u{E76F}",
                u.icon(12.0),
                u.s.text2,
                id,
            );
            u.text(
                Rect::new(card.x + 28.0, card.y, card.w - 60.0, card.h),
                &name,
                u.ts(13.0, 400),
                tc,
                id,
            );
            if !mc.enabled {
                u.text(
                    Rect::new(card.right() - 28.0, card.y, 22.0, card.h),
                    "\u{ED1A}",
                    u.icon(13.0),
                    u.s.disabled,
                    id,
                );
            }
            u.actions
                .push((id, StudioAction::SelectModule(mc.id.clone())));
            u.draggable.push((id, Some(mc.id.clone()), mc.kind));
            u.drop_slots.push((
                *region,
                i + 1,
                Rect::new(card.x, card.bottom() + 0.5, card.w, 2.0),
            ));
        }
    }
}

/// Buttons shown on Templates and My designs: start from scratch, import.
fn creator_row(u: &mut Ui, g: ElementId, r: Rect, y: f32, with_export: bool) -> f32 {
    let lang = u.lang;
    let mut items: Vec<(&str, &str, &str, StudioAction, Kind)> = vec![
        (
            "creator:new",
            "\u{E710}",
            t(lang, "Create custom design", "새 디자인 만들기"),
            StudioAction::CreateCustom,
            Kind::Primary,
        ),
        (
            "creator:import",
            "\u{E8B5}",
            t(lang, "Import design code", "디자인 코드 가져오기"),
            StudioAction::Nav(Section::Import),
            Kind::Secondary,
        ),
    ];
    if with_export {
        items.push((
            "creator:copy",
            "\u{E8C8}",
            t(lang, "Copy design code", "디자인 코드 복사"),
            StudioAction::CopyDesignCode,
            Kind::Secondary,
        ));
        items.push((
            "creator:export",
            "\u{EDE1}",
            t(lang, "Export .ricepack…", ".ricepack 내보내기…"),
            StudioAction::ExportPackage,
            Kind::Secondary,
        ));
    }
    let n = items.len() as f32;
    let bw = ((r.w - 8.0 * (n - 1.0)) / n).min(230.0);
    for (i, (key, glyph, label, a, kind)) in items.into_iter().enumerate() {
        u.button(
            g,
            key,
            Rect::new(r.x + i as f32 * (bw + 8.0), y, bw, 34.0),
            Some(glyph),
            label,
            label,
            kind,
            true,
            a,
        );
    }
    y + 46.0
}

fn import_page(u: &mut Ui, g: ElementId, r: Rect) {
    use windowsrice_core::design_code::Severity as Sev;
    let lang = u.lang;
    let m = u.m;
    let iv = &m.import;
    let mut y = section_title(
        u,
        g,
        r,
        t(lang, "Import design code", "디자인 코드 가져오기"),
        t(
            lang,
            "Paste RICE Design Code or open a .ricepack, then Analyze. Design code is data only: it is checked and never run. Checks cannot prove that a file is free of malware; windowsRICE accepts only settings, PNG and JPEG images and MP4 videos.",
            "RICE 디자인 코드를 붙여 넣거나 .ricepack을 연 뒤 분석하세요. 디자인 코드는 데이터일 뿐이며 검사만 하고 실행하지 않습니다. 검사로 파일에 악성 코드가 없음을 증명할 수는 없으며, windowsRICE는 설정, PNG/JPEG 이미지, MP4 동영상만 받습니다.",
        ),
    );
    let has_text = !iv.text.trim().is_empty();
    let row: [(&str, &str, &str, bool, StudioAction, Kind); 5] = [
        (
            "import:paste",
            "\u{E77F}",
            t(lang, "Paste code", "코드 붙여넣기"),
            true,
            StudioAction::ImportPaste,
            Kind::Secondary,
        ),
        (
            "import:open",
            "\u{E8E5}",
            t(lang, "Open file…", "파일 열기…"),
            true,
            StudioAction::ImportOpen,
            Kind::Secondary,
        ),
        (
            "import:analyze",
            "\u{E721}",
            t(lang, "Analyze", "분석"),
            has_text,
            StudioAction::ImportAnalyze,
            Kind::Primary,
        ),
        (
            "import:scan",
            "\u{E83D}",
            t(lang, "Defender scan", "Defender 검사"),
            iv.package,
            StudioAction::ImportScan,
            Kind::Secondary,
        ),
        (
            "import:clear",
            "\u{E894}",
            t(lang, "Clear", "지우기"),
            has_text,
            StudioAction::ImportClear,
            Kind::Secondary,
        ),
    ];
    let bw = ((r.w - 4.0 * 8.0) / 5.0).min(170.0);
    for (i, (key, glyph, label, enabled, a, kind)) in row.into_iter().enumerate() {
        u.button(
            g,
            key,
            Rect::new(r.x + i as f32 * (bw + 8.0), y, bw, 34.0),
            Some(glyph),
            label,
            label,
            kind,
            enabled,
            a,
        );
    }
    y += 46.0;
    let lines: Vec<&str> = iv.text.lines().collect();
    let source = if has_text {
        match lang {
            Lang::En => format!("{} · {} lines", iv.source, lines.len()),
            Lang::Ko => format!("{} · {}줄", iv.source, lines.len()),
        }
    } else {
        t(
            lang,
            "Nothing to check yet.",
            "아직 검사할 내용이 없습니다.",
        )
        .to_owned()
    };
    u.label(
        g,
        "import:source",
        Rect::new(r.x, y, r.w, 22.0),
        &source,
        12.0,
        400,
        u.s.text2,
    );
    y += 28.0;

    // Results: one line per check, then the problems.
    if iv.analyzed {
        let summary = match (lang, iv.can_apply) {
            (Lang::En, true) => format!(
                "Ready: no errors{}. Preview it, then save or apply.",
                match iv
                    .diagnostics
                    .iter()
                    .filter(|d| d.severity == Sev::Warning)
                    .count()
                {
                    0 => String::new(),
                    n => format!(", {n} warning(s)"),
                }
            ),
            (Lang::En, false) => format!(
                "{} error(s) must be fixed before this design can be used.",
                iv.diagnostics
                    .iter()
                    .filter(|d| d.severity == Sev::Error)
                    .count()
            ),
            (Lang::Ko, true) => {
                "준비됨: 오류가 없습니다. 미리 본 뒤 저장하거나 적용하세요.".to_owned()
            }
            (Lang::Ko, false) => format!(
                "이 디자인을 사용하려면 오류 {}개를 고쳐야 합니다.",
                iv.diagnostics
                    .iter()
                    .filter(|d| d.severity == Sev::Error)
                    .count()
            ),
        };
        let col = if iv.can_apply {
            u.s.success
        } else {
            u.s.danger
        };
        u.label(
            g,
            "import:summary",
            Rect::new(r.x, y, r.w, 24.0),
            &summary,
            13.0,
            600,
            col,
        );
        y += 30.0;
        let cw = (r.w - 8.0) / 2.0;
        for (i, (name, st)) in iv.checks.iter().enumerate() {
            let (word, col) = match st {
                ImportCheck::Passed => (t(lang, "passed", "통과"), u.s.success),
                ImportCheck::Failed => (t(lang, "failed", "실패"), u.s.danger),
                ImportCheck::NotRun => (t(lang, "not checked", "검사 안 함"), u.s.text2),
                ImportCheck::Unavailable => (t(lang, "unavailable", "사용할 수 없음"), u.s.warning),
            };
            let x = r.x + (i % 2) as f32 * (cw + 8.0);
            let label = format!("{name}: {word}");
            u.label(
                g,
                &format!("import:check{i}"),
                Rect::new(x, y, cw, 22.0),
                &label,
                12.0,
                400,
                col,
            );
            if i % 2 == 1 || i + 1 == iv.checks.len() {
                y += 24.0;
            }
        }
        y += 6.0;
        for (i, d) in iv.diagnostics.iter().enumerate().take(40) {
            let sev = match (d.severity, lang) {
                (Sev::Error, Lang::En) => "Error",
                (Sev::Warning, Lang::En) => "Warning",
                (Sev::Info, Lang::En) => "Note",
                (Sev::Error, Lang::Ko) => "오류",
                (Sev::Warning, Lang::Ko) => "경고",
                (Sev::Info, Lang::Ko) => "참고",
            };
            let at = match (d.line, d.column, lang) {
                (Some(l), Some(c), Lang::En) => format!(" · line {l}, column {c}"),
                (Some(l), Some(c), Lang::Ko) => format!(" · {l}행 {c}열"),
                _ => String::new(),
            };
            let place = if d.path.is_empty() {
                String::new()
            } else {
                format!(" · {}", d.path)
            };
            let fix = d
                .suggestion
                .as_deref()
                .map_or(String::new(), |s| match lang {
                    Lang::En => format!(" Fix: {s}"),
                    Lang::Ko => format!(" 해결: {s}"),
                });
            let label = format!(
                "{sev}{at}{place}: {}.{fix}",
                d.message.trim_end_matches('.')
            );
            u.radio(
                g,
                &format!("import:diag:{i}"),
                Rect::new(r.x, y, r.w, 44.0),
                &label,
                iv.selected == Some(i),
                StudioAction::ImportSelect(i),
            );
            y += 48.0;
        }
    }

    // Line editor for the selected problem.
    if let Some(line) = iv.selected_line() {
        let label = match lang {
            Lang::En => format!("Line {line} (edit, press Enter, then Analyze)"),
            Lang::Ko => format!("{line}행 (수정 후 Enter, 그다음 분석)"),
        };
        let current = lines.get(line as usize - 1).copied().unwrap_or("");
        u.field(
            g,
            "field:import-line",
            Rect::new(r.x, y, r.w, 32.0),
            &label,
            current,
            "",
        );
        y += 42.0;
    }

    // The code, with line numbers; the selected problem's line is marked.
    if has_text {
        let mark = iv.selected_line().unwrap_or(0) as usize;
        let first = mark.saturating_sub(8).max(1);
        let shown = 24usize;
        let lh = 18.0;
        let box_h = (lines.len().saturating_sub(first - 1).min(shown)) as f32 * lh + 12.0;
        let code = u.group(
            g,
            "import:code",
            t(lang, "Design code", "디자인 코드"),
            Rect::new(r.x, y, r.w, box_h),
        );
        u.fill(Rect::new(r.x, y, r.w, box_h), 8.0, u.s.panel, code);
        let mut ly = y + 6.0;
        for (n, text) in lines.iter().enumerate().skip(first - 1).take(shown) {
            let no = n + 1;
            if no == mark {
                u.fill(
                    Rect::new(r.x + 2.0, ly, r.w - 4.0, lh),
                    4.0,
                    Color {
                        a: 0x40,
                        ..u.s.danger
                    },
                    code,
                );
            }
            let s = format!("{no:>4}  {}", text.chars().take(160).collect::<String>());
            u.text(
                Rect::new(r.x + 8.0, ly, r.w - 16.0, lh),
                &s,
                u.ts(12.0, 400),
                u.s.text,
                code,
            );
            ly += lh;
        }
        y += box_h + 12.0;
    }

    // Use the design.
    let half = ((r.w - 8.0) / 2.0).min(260.0);
    u.button(
        g,
        "import:preview",
        Rect::new(r.x, y, half, 36.0),
        Some("\u{E7B3}"),
        t(lang, "Preview in Studio", "스튜디오에서 미리 보기"),
        t(
            lang,
            "Load the design into the draft (Undo returns; Apply uses it)",
            "디자인을 초안으로 불러오기 (실행 취소 가능, 적용해야 사용)",
        ),
        Kind::Primary,
        iv.can_apply,
        StudioAction::ImportPreview,
    );
    u.button(
        g,
        "import:save",
        Rect::new(r.x + half + 8.0, y, half, 36.0),
        Some("\u{E74E}"),
        t(lang, "Save to My designs", "내 디자인에 저장"),
        t(
            lang,
            "Save the design to My designs",
            "디자인을 내 디자인에 저장",
        ),
        Kind::Secondary,
        iv.can_apply,
        StudioAction::ImportSave,
    );
    u.content_bottom = u.content_bottom.max(y + 48.0);
}

fn presets(u: &mut Ui, g: ElementId, r: Rect) {
    let lang = u.lang;
    let m = u.m;
    let mut y = section_title(
        u,
        g,
        r,
        t(lang, "Presets", "프리셋"),
        t(
            lang,
            "A preset stores the theme and every bar setting. Loading one changes the draft; apply it to use it.",
            "프리셋은 테마와 모든 바 설정을 저장합니다. 불러오면 초안이 바뀌며, 적용해야 사용됩니다.",
        ),
    );
    y = creator_row(u, g, r, y, true);
    if m.presets.is_empty() {
        u.label(
            g,
            "pre:empty",
            Rect::new(r.x, y, r.w, 26.0),
            t(
                lang,
                "No presets yet. Type a name in the property panel and choose Save current design.",
                "아직 프리셋이 없습니다. 오른쪽에서 이름을 입력하고 현재 디자인 저장을 누르세요.",
            ),
            13.0,
            400,
            u.s.text2,
        );
        y += 32.0;
    }
    for (i, p) in m.presets.iter().enumerate() {
        if y + 36.0 > r.bottom() {
            let more = match lang {
                Lang::En => format!("{} more presets are not shown.", m.presets.len() - i),
                Lang::Ko => format!("프리셋 {}개가 더 있습니다.", m.presets.len() - i),
            };
            u.label(
                g,
                "pre:more",
                Rect::new(r.x, y, r.w, 22.0),
                &more,
                12.0,
                400,
                u.s.text2,
            );
            break;
        }
        let label = match (&p.name, &p.error) {
            (Some(n), _) => n.clone(),
            (None, Some(e)) => match lang {
                Lang::En => format!("{} — cannot be used: {e}", p.file),
                Lang::Ko => format!("{} — 사용할 수 없음: {e}", p.file),
            },
            _ => p.file.clone(),
        };
        u.radio(
            g,
            &format!("pre:{i}"),
            Rect::new(r.x, y, r.w, 32.0),
            &label,
            m.preset_selected == Some(i),
            StudioAction::SelectPreset(i),
        );
        y += 36.0;
    }
}

fn advanced(u: &mut Ui, g: ElementId, r: Rect) {
    let lang = u.lang;
    let m = u.m;
    let b = &m.draft.bar;
    let mut y = section_title(
        u,
        g,
        r,
        t(lang, "Advanced", "고급"),
        t(
            lang,
            "Screen space, hiding, monitors and the Studio's own appearance.",
            "화면 공간, 숨기기, 모니터, 스튜디오 모양입니다.",
        ),
    );
    let half = (r.w - 16.0) / 2.0;
    u.toggle(
        g,
        "adv:reserve",
        Rect::new(r.x, y, half, ROW),
        t(lang, "Reserve screen space", "화면 공간 확보"),
        b.reserve_work_area,
        true,
        Some(t(
            lang,
            "Maximised windows start below the bar.",
            "최대화한 창이 바 아래에서 시작합니다.",
        )),
        StudioAction::ToggleReserve,
    );
    u.toggle(
        g,
        "adv:autohide",
        Rect::new(r.x + half + 16.0, y, half, ROW),
        t(lang, "Hide automatically", "자동으로 숨기기"),
        b.auto_hide,
        true,
        None,
        StudioAction::ToggleAutoHide,
    );
    y += ROW + 8.0;
    let delay = format!("{:.1} s", f64::from(b.auto_hide_delay_ms) / 1000.0);
    u.stepper(
        g,
        "adv:delay",
        Rect::new(r.x, y, half, ROW),
        t(lang, "Hide after", "숨기기 지연"),
        &delay,
        (b.auto_hide_delay_ms > 200).then_some(StudioAction::AutoHideDelayDown),
        (b.auto_hide_delay_ms < 5000).then_some(StudioAction::AutoHideDelayUp),
    );
    y += ROW + 12.0;
    u.label(
        g,
        "adv:fs",
        Rect::new(r.x, y, 166.0, ROW),
        t(lang, "Fullscreen apps", "전체 화면 앱"),
        13.0,
        500,
        u.s.text,
    );
    let ow = (r.w - 170.0) / 2.0;
    u.radio(
        g,
        "adv:fs:hide",
        Rect::new(r.x + 170.0, y, ow - 6.0, ROW),
        t(lang, "Hide the bar", "바 숨기기"),
        b.fullscreen == FullscreenPolicy::Hide,
        StudioAction::Fullscreen(FullscreenPolicy::Hide),
    );
    u.radio(
        g,
        "adv:fs:keep",
        Rect::new(r.x + 170.0 + ow, y, ow - 6.0, ROW),
        t(lang, "Keep it", "유지"),
        b.fullscreen == FullscreenPolicy::Keep,
        StudioAction::Fullscreen(FullscreenPolicy::Keep),
    );
    y += ROW + 8.0;
    u.label(
        g,
        "adv:mon",
        Rect::new(r.x, y, 166.0, ROW),
        t(lang, "Monitors", "모니터"),
        13.0,
        500,
        u.s.text,
    );
    u.radio(
        g,
        "adv:mon:all",
        Rect::new(r.x + 170.0, y, ow - 6.0, ROW),
        t(lang, "Every monitor", "모든 모니터"),
        b.monitors == MonitorPlacement::All,
        StudioAction::Monitors(MonitorPlacement::All),
    );
    u.radio(
        g,
        "adv:mon:primary",
        Rect::new(r.x + 170.0 + ow, y, ow - 6.0, ROW),
        t(lang, "Primary only", "주 모니터만"),
        b.monitors == MonitorPlacement::Primary,
        StudioAction::Monitors(MonitorPlacement::Primary),
    );
    y += ROW + 6.0;
    for (i, mon) in m.monitors.iter().enumerate() {
        if y + ROW > r.bottom() - ROW - 12.0 {
            break;
        }
        let on = b.monitor_enabled(&mon.device, mon.id.as_deref(), mon.primary);
        let label = format!(
            "{}{}",
            mon.name,
            if mon.primary {
                t(lang, " (primary)", " (주 모니터)")
            } else {
                ""
            }
        );
        u.toggle(
            g,
            &format!("adv:mon:{i}"),
            Rect::new(r.x + 170.0, y, r.w - 170.0, ROW),
            &label,
            on,
            true,
            None,
            StudioAction::ToggleMonitor(i),
        );
        y += ROW + 4.0;
    }
    y += 8.0;
    u.label(
        g,
        "adv:look",
        Rect::new(r.x, y, 166.0, ROW),
        t(lang, "Studio appearance", "스튜디오 모양"),
        13.0,
        500,
        u.s.text,
    );
    for (i, (v, en, ko)) in [
        (None, "Follow Windows", "Windows 설정"),
        (Some(true), "Dark", "어둡게"),
        (Some(false), "Light", "밝게"),
    ]
    .iter()
    .enumerate()
    {
        let ow = (r.w - 170.0) / 3.0;
        u.radio(
            g,
            &format!("adv:look:{i}"),
            Rect::new(r.x + 170.0 + i as f32 * ow, y, ow - 6.0, ROW),
            t(lang, en, ko),
            m.appearance == *v,
            StudioAction::Appearance(*v),
        );
    }
    y += ROW + 14.0;
    if y + 38.0 <= r.bottom() {
        u.button(
            g,
            "adv:reset",
            Rect::new(r.x, y, 260.0, 38.0),
            Some("\u{E72C}"),
            t(
                lang,
                "Reset everything to defaults",
                "모두 기본값으로 재설정",
            ),
            t(
                lang,
                "Reset the theme and every bar setting to the defaults (asks first)",
                "테마와 모든 바 설정을 기본값으로 재설정 (먼저 확인)",
            ),
            Kind::Danger,
            true,
            StudioAction::ResetAll,
        );
    }
}

// ------------------------------------------------------------------ properties

fn legacy_properties(u: &mut Ui, g: ElementId, r: Rect) {
    let lang = u.lang;
    let m = u.m;
    let mut y = r.y;
    let w = r.w;
    match m.section {
        Section::Modules => {
            let Some(mc) = m
                .selected_module
                .as_ref()
                .and_then(|id| m.draft.bar.modules.iter().find(|x| &x.id == id))
            else {
                u.label(
                    g,
                    "props:empty",
                    Rect::new(r.x, y, w, 60.0),
                    t(
                        lang,
                        "Select a module card to edit it.",
                        "편집할 모듈 카드를 선택하세요.",
                    ),
                    13.0,
                    400,
                    u.s.text2,
                );
                return;
            };
            let name = module_name(mc, lang);
            u.heading(g, "props:title", Rect::new(r.x, y, w, 28.0), &name);
            y += 34.0;
            let is_menu = mc.kind == ModuleKind::Menu;
            u.toggle(
                g,
                "props:show",
                Rect::new(r.x, y, w, ROW),
                t(lang, "Shown on the bar", "바에 표시"),
                mc.enabled,
                !is_menu,
                is_menu.then_some(t(
                    lang,
                    "The menu opens the settings and cannot be hidden.",
                    "메뉴는 설정을 열기 때문에 숨길 수 없습니다.",
                )),
                StudioAction::ToggleModule(mc.id.clone()),
            );
            y += ROW + 6.0;
            let third = (w - 8.0) / 3.0;
            for (i, (rg, en, ko)) in [
                (Region::Left, "Left", "왼쪽"),
                (Region::Center, "Center", "가운데"),
                (Region::Right, "Right", "오른쪽"),
            ]
            .iter()
            .enumerate()
            {
                let label = match lang {
                    Lang::En => format!("{en} section"),
                    Lang::Ko => format!("{ko} 영역"),
                };
                u.radio(
                    g,
                    &format!("props:region:{i}"),
                    Rect::new(r.x + i as f32 * (third + 4.0), y, third, ROW),
                    &label,
                    mc.region == *rg,
                    StudioAction::ModuleRegion(mc.id.clone(), *rg),
                );
            }
            y += ROW + 6.0;
            let half = (w - 8.0) / 2.0;
            let same: Vec<&ModuleConfig> = m
                .draft
                .bar
                .modules
                .iter()
                .filter(|x| x.region == mc.region)
                .collect();
            let pos = same.iter().position(|x| x.id == mc.id).unwrap_or(0);
            u.button(
                g,
                "props:earlier",
                Rect::new(r.x, y, half, 34.0),
                Some("\u{E70E}"),
                t(lang, "Earlier", "앞으로"),
                &match lang {
                    Lang::En => format!("Move {name} earlier"),
                    Lang::Ko => format!("{name} 앞으로 이동"),
                },
                Kind::Secondary,
                pos > 0,
                StudioAction::ModuleEarlier(mc.id.clone()),
            );
            u.button(
                g,
                "props:later",
                Rect::new(r.x + half + 8.0, y, half, 34.0),
                Some("\u{E70D}"),
                t(lang, "Later", "뒤로"),
                &match lang {
                    Lang::En => format!("Move {name} later"),
                    Lang::Ko => format!("{name} 뒤로 이동"),
                },
                Kind::Secondary,
                pos + 1 < same.len(),
                StudioAction::ModuleLater(mc.id.clone()),
            );
            y += 42.0;
            if let Some(ms) = mc.interval_ms() {
                let it = interval_text(Some(ms), lang);
                u.stepper(
                    g,
                    "props:interval",
                    Rect::new(r.x, y, w, ROW),
                    t(lang, "Refresh", "새로 고침"),
                    &it,
                    (crate::popup::step_interval(ms, false) != ms)
                        .then(|| StudioAction::IntervalShorter(mc.id.clone())),
                    (crate::popup::step_interval(ms, true) != ms)
                        .then(|| StudioAction::IntervalLonger(mc.id.clone())),
                );
                y += ROW + 6.0;
            }
            if ModuleConfig::has_label(mc.kind) {
                u.toggle(
                    g,
                    "props:label",
                    Rect::new(r.x, y, w, ROW),
                    t(lang, "Show its short label", "짧은 이름 표시"),
                    mc.show_label != Some(false),
                    true,
                    None,
                    StudioAction::ToggleShowLabel(mc.id.clone()),
                );
                y += ROW + 6.0;
            }
            if mc.kind == ModuleKind::Clock {
                u.toggle(
                    g,
                    "props:hour12",
                    Rect::new(r.x, y, w, ROW),
                    t(lang, "12-hour time", "12시간제"),
                    mc.hour12 == Some(true),
                    true,
                    None,
                    StudioAction::ToggleHour12(mc.id.clone()),
                );
                y += ROW + 6.0;
            }
            let mwv = mc
                .max_width
                .map_or_else(|| t(lang, "none", "없음").to_owned(), |v| v.to_string());
            let cur = mc.max_width.unwrap_or(0);
            u.stepper(
                g,
                "props:maxw",
                Rect::new(r.x, y, w, ROW),
                t(lang, "Max width", "최대 너비"),
                &mwv,
                (cur > MAX_WIDTH_STEPS[0]).then(|| StudioAction::MaxWidthDown(mc.id.clone())),
                (cur < MAX_WIDTH_STEPS[MAX_WIDTH_STEPS.len() - 1])
                    .then(|| StudioAction::MaxWidthUp(mc.id.clone())),
            );
            y += ROW + 4.0;
            if mc.max_width.is_some() {
                u.button(
                    g,
                    "props:maxw-none",
                    Rect::new(r.x, y, w, 30.0),
                    None,
                    t(lang, "Remove the width limit", "너비 제한 없애기"),
                    t(lang, "Remove the width limit", "너비 제한 없애기"),
                    Kind::Quiet,
                    true,
                    StudioAction::MaxWidthNone(mc.id.clone()),
                );
                y += 34.0;
            }
            if mc.kind == ModuleKind::CustomText {
                u.field(
                    g,
                    "field:module-text",
                    Rect::new(r.x, y, w, 32.0),
                    t(lang, "Text", "텍스트"),
                    mc.text.as_deref().unwrap_or(""),
                    t(lang, "Text to show", "표시할 텍스트"),
                );
                y += 38.0;
            }
            if mc.kind == ModuleKind::CustomIcon {
                for (i, (code, n)) in ICON_CHOICES.iter().enumerate() {
                    let glyph = u32::from_str_radix(code, 16)
                        .ok()
                        .and_then(char::from_u32)
                        .map(String::from)
                        .unwrap_or_default();
                    let key = format!("props:icon:{i}");
                    let rr = Rect::new(r.x + i as f32 * 40.0, y, 36.0, 36.0).snap(u.scale);
                    let sel = mc.icon.as_deref() == Some(*code);
                    let name = match lang {
                        Lang::En => format!("Icon {n}"),
                        Lang::Ko => format!("아이콘 {n}"),
                    };
                    let id = u.add(
                        g,
                        Element::new(key_id(&key), intern(&key), Role::RadioButton, name, rr)
                            .focusable(true)
                            .selected(sel),
                    );
                    u.fill(rr, 6.0, if sel { u.s.selection } else { u.s.card }, id);
                    u.decorate(id, rr, 6.0);
                    u.text(
                        Rect::new(rr.x + 10.0, rr.y, 20.0, rr.h),
                        &glyph,
                        u.icon(15.0),
                        u.s.text,
                        id,
                    );
                    u.actions
                        .push((id, StudioAction::CustomIcon(mc.id.clone(), code)));
                }
                y += 44.0;
            }
            if mc.kind.is_custom() {
                u.field(
                    g,
                    "field:module-label",
                    Rect::new(r.x, y, w, 32.0),
                    t(
                        lang,
                        "Name read by screen readers",
                        "화면 낭독기가 읽는 이름",
                    ),
                    mc.label.as_deref().unwrap_or(""),
                    t(lang, "Accessible name", "접근성 이름"),
                );
                y += 38.0;
            }
            if !is_menu && y + 36.0 <= r.bottom() {
                u.button(
                    g,
                    "props:remove",
                    Rect::new(r.x, y, w, 36.0),
                    Some("\u{E74D}"),
                    t(lang, "Remove from the bar", "바에서 제거"),
                    &match lang {
                        Lang::En => format!("Remove {name} from the bar"),
                        Lang::Ko => format!("바에서 {name} 제거"),
                    },
                    Kind::Danger,
                    true,
                    StudioAction::RemoveModule(mc.id.clone()),
                );
            }
        }
        Section::Themes => {
            let Some(e) = m
                .theme_selected
                .as_ref()
                .and_then(|id| m.themes.iter().find(|x| &x.id == id))
            else {
                u.label(
                    g,
                    "props:empty",
                    Rect::new(r.x, y, w, 60.0),
                    t(lang, "Select a theme.", "테마를 선택하세요."),
                    13.0,
                    400,
                    u.s.text2,
                );
                return;
            };
            u.heading(g, "props:title", Rect::new(r.x, y, w, 28.0), &e.name);
            y += 34.0;
            let active = m.draft.theme.active == e.id;
            u.button(
                g,
                "props:use",
                Rect::new(r.x, y, (w - 8.0) / 2.0, 36.0),
                Some("\u{E73E}"),
                if active {
                    t(lang, "In use", "사용 중")
                } else {
                    t(lang, "Use this theme", "이 테마 사용")
                },
                t(lang, "Use this theme for the bar", "바에 이 테마 사용"),
                Kind::Primary,
                !active,
                StudioAction::UseTheme(e.id.clone()),
            );
            u.button(
                g,
                "props:dup",
                Rect::new(r.x + (w + 8.0) / 2.0, y, (w - 8.0) / 2.0, 36.0),
                Some("\u{E8C8}"),
                t(lang, "Duplicate", "복제"),
                &match lang {
                    Lang::En => format!("Duplicate {}", e.name),
                    Lang::Ko => format!("{} 복제", e.name),
                },
                Kind::Secondary,
                true,
                StudioAction::DuplicateTheme(e.id.clone()),
            );
            y += 42.0;
            if e.builtin {
                u.label(g, "props:builtin", Rect::new(r.x, y, w, 40.0), t(lang, "Built-in themes cannot be changed. Duplicate one to make an editable copy.", "기본 테마는 바꿀 수 없습니다. 복제하면 편집할 수 있습니다."), 12.0, 400, u.s.text2);
                return;
            }
            let Some(ed) = m.theme_edit.as_ref().filter(|x| x.id == e.id) else {
                return;
            };
            u.field(
                g,
                "field:theme-name",
                Rect::new(r.x, y, w, 32.0),
                t(lang, "Theme name", "테마 이름"),
                &ed.name,
                t(lang, "Name", "이름"),
            );
            y += 38.0;
            // Save and Delete right under the name, so they are always
            // reachable (small windows cut off the bottom of this panel).
            {
                let half = (w - 8.0) / 2.0;
                u.button(
                    g,
                    "props:tsave",
                    Rect::new(r.x, y, half, 34.0),
                    Some("\u{E74E}"),
                    t(lang, "Save theme", "테마 저장"),
                    t(lang, "Save the theme file", "테마 파일 저장"),
                    Kind::Primary,
                    ed.dirty && ed.errors.is_empty(),
                    StudioAction::SaveTheme,
                );
                u.button(
                    g,
                    "props:tdelete",
                    Rect::new(r.x + half + 8.0, y, half, 34.0),
                    Some("\u{E74D}"),
                    t(lang, "Delete", "삭제"),
                    &match lang {
                        Lang::En => format!("Delete {} (asks first)", e.name),
                        Lang::Ko => format!("{} 삭제 (먼저 확인)", e.name),
                    },
                    Kind::Danger,
                    !active,
                    StudioAction::DeleteTheme(e.id.clone()),
                );
            }
            y += 42.0;
            let half = (w - 8.0) / 2.0;
            u.radio(
                g,
                "props:tdark",
                Rect::new(r.x, y, half, 30.0),
                t(lang, "Dark", "어두움"),
                ed.dark,
                StudioAction::ThemeDark(true),
            );
            u.radio(
                g,
                "props:tlight",
                Rect::new(r.x + half + 8.0, y, half, 30.0),
                t(lang, "Light", "밝음"),
                !ed.dark,
                StudioAction::ThemeDark(false),
            );
            y += 34.0;
            // Three colours side by side (fits small windows).
            let cw = (w - 16.0) / 3.0;
            for (i, (key, label, value)) in [
                (
                    "field:theme-bg",
                    t(lang, "Background (#RRGGBB)", "배경 (#RRGGBB)"),
                    &ed.background,
                ),
                (
                    "field:theme-accent",
                    t(lang, "Accent (#RRGGBB)", "강조색 (#RRGGBB)"),
                    &ed.accent,
                ),
                (
                    "field:theme-text",
                    t(lang, "Text (#RRGGBB)", "텍스트 (#RRGGBB)"),
                    &ed.text,
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let x = r.x + i as f32 * (cw + 8.0);
                if let Some(c) = hex_color(value) {
                    u.fill(Rect::new(x, y + 34.0, cw, 4.0), 2.0, c, g);
                }
                u.field(g, key, Rect::new(x, y, cw, 32.0), label, value, "#RRGGBB");
            }
            y += 44.0;
            let third = (w - 8.0) / 3.0;
            for (i, (bd, en, ko)) in [
                (windowsrice_theme::schema::Backdrop::None, "Solid", "불투명"),
                (
                    windowsrice_theme::schema::Backdrop::Acrylic,
                    "Acrylic",
                    "아크릴",
                ),
                (windowsrice_theme::schema::Backdrop::Mica, "Mica", "미카"),
            ]
            .iter()
            .enumerate()
            {
                u.radio(
                    g,
                    &format!("props:tbd:{i}"),
                    Rect::new(r.x + i as f32 * (third + 4.0), y, third, 30.0),
                    t(lang, en, ko),
                    ed.backdrop == *bd,
                    StudioAction::ThemeBackdrop(*bd),
                );
            }
            y += 34.0;
            let ov = format!("{}%", (ed.opacity * 100.0).round());
            u.stepper(
                g,
                "props:topacity",
                Rect::new(r.x, y, w, ROW),
                t(lang, "Opacity", "불투명도"),
                &ov,
                (ed.opacity > 0.30 + 1e-3).then_some(StudioAction::ThemeOpacityDown),
                (ed.opacity < 1.0 - 1e-3).then_some(StudioAction::ThemeOpacityUp),
            );
            y += ROW + 6.0;
            for (i, e2) in ed.errors.iter().take(2).enumerate() {
                u.label(
                    g,
                    &format!("props:terr{i}"),
                    Rect::new(r.x, y, w, 20.0),
                    e2,
                    11.0,
                    400,
                    u.s.danger,
                );
                y += 20.0;
            }
            for (i, w2) in ed.warnings.iter().take(1).enumerate() {
                u.label(
                    g,
                    &format!("props:twarn{i}"),
                    Rect::new(r.x, y, w, 20.0),
                    w2,
                    11.0,
                    400,
                    u.s.warning,
                );
                y += 20.0;
            }
        }
        Section::Presets => {
            u.heading(
                g,
                "props:title",
                Rect::new(r.x, y, w, 28.0),
                t(lang, "Preset", "프리셋"),
            );
            y += 34.0;
            u.field(
                g,
                "field:preset-name",
                Rect::new(r.x, y, w, 32.0),
                t(lang, "Preset name", "프리셋 이름"),
                &m.preset_name,
                t(lang, "Name for a new preset", "새 프리셋 이름"),
            );
            y += 40.0;
            let sel = m.preset_selected.and_then(|i| m.presets.get(i));
            let usable = sel.is_some_and(|p| p.name.is_some());
            let has_name = !m.preset_name.trim().is_empty()
                || m.text_edit
                    .as_ref()
                    .is_some_and(|e| e.key == "field:preset-name" && !e.buffer.trim().is_empty());
            let half = (w - 8.0) / 2.0;
            let rows: [(&str, &str, &str, &str, bool, StudioAction, Kind); 7] = [
                (
                    "props:psave",
                    "\u{E74E}",
                    t(lang, "Save current design", "현재 디자인 저장"),
                    t(
                        lang,
                        "Save the current design as a preset named in the field above",
                        "위 이름으로 현재 디자인을 프리셋으로 저장",
                    ),
                    has_name,
                    StudioAction::SavePreset,
                    Kind::Primary,
                ),
                (
                    "props:pload",
                    "\u{E896}",
                    t(lang, "Load", "불러오기"),
                    t(
                        lang,
                        "Load the selected preset into the draft",
                        "선택한 프리셋을 초안으로 불러오기",
                    ),
                    usable,
                    StudioAction::LoadPreset,
                    Kind::Secondary,
                ),
                (
                    "props:pdup",
                    "\u{E8C8}",
                    t(lang, "Duplicate", "복제"),
                    t(lang, "Duplicate the selected preset", "선택한 프리셋 복제"),
                    usable,
                    StudioAction::DuplicatePreset,
                    Kind::Secondary,
                ),
                (
                    "props:prename",
                    "\u{E8AC}",
                    t(lang, "Rename", "이름 바꾸기"),
                    t(
                        lang,
                        "Rename the selected preset to the name in the field above",
                        "선택한 프리셋 이름을 위 이름으로 바꾸기",
                    ),
                    usable && has_name,
                    StudioAction::RenamePreset,
                    Kind::Secondary,
                ),
                (
                    "props:pexport",
                    "\u{EDE1}",
                    t(lang, "Export…", "내보내기…"),
                    t(
                        lang,
                        "Export the selected preset to a file",
                        "선택한 프리셋을 파일로 내보내기",
                    ),
                    usable,
                    StudioAction::ExportPreset,
                    Kind::Secondary,
                ),
                (
                    "props:pimport",
                    "\u{E8B5}",
                    t(lang, "Import…", "가져오기…"),
                    t(
                        lang,
                        "Import a preset file (it is checked first)",
                        "프리셋 파일 가져오기 (먼저 검사함)",
                    ),
                    true,
                    StudioAction::ImportPreset,
                    Kind::Secondary,
                ),
                (
                    "props:pdelete",
                    "\u{E74D}",
                    t(lang, "Delete", "삭제"),
                    t(
                        lang,
                        "Delete the selected preset (asks first)",
                        "선택한 프리셋 삭제 (먼저 확인)",
                    ),
                    sel.is_some(),
                    StudioAction::DeletePreset,
                    Kind::Danger,
                ),
            ];
            for (i, (key, glyph, label, name, enabled, a, kind)) in rows.into_iter().enumerate() {
                let (x, bw) = if i == 0 {
                    (r.x, w)
                } else if i % 2 == 1 {
                    (r.x, half)
                } else {
                    (r.x + half + 8.0, half)
                };
                u.button(
                    g,
                    key,
                    Rect::new(x, y, bw, 34.0),
                    Some(glyph),
                    label,
                    name,
                    kind,
                    enabled,
                    a,
                );
                if i == 0 || i % 2 == 0 {
                    y += 40.0;
                }
            }
        }
        _ => {
            u.heading(
                g,
                "props:title",
                Rect::new(r.x, y, w, 28.0),
                t(lang, "Readability", "가독성"),
            );
            y += 34.0;
            let lines: Vec<(String, Color)> = if m.blocking.is_empty() && m.warnings.is_empty() {
                vec![(
                    t(
                        lang,
                        "Text contrast meets WCAG AA (4.5:1) in the worst case.",
                        "최악의 경우에도 텍스트 대비가 WCAG AA(4.5:1)를 충족합니다.",
                    )
                    .to_owned(),
                    u.s.success,
                )]
            } else {
                m.blocking
                    .iter()
                    .map(|b| (b.clone(), u.s.danger))
                    .chain(m.warnings.iter().map(|x| (x.clone(), u.s.warning)))
                    .collect()
            };
            for (i, (l, c)) in lines.iter().take(6).enumerate() {
                u.label(
                    g,
                    &format!("props:read{i}"),
                    Rect::new(r.x, y, w, 40.0),
                    l,
                    12.0,
                    400,
                    *c,
                );
                y += 44.0;
            }
            u.label(g, "props:shortcuts", Rect::new(r.x, r.bottom() - 60.0, w, 60.0), t(lang, "Shortcuts: Ctrl+Z undo · Ctrl+Y redo · Ctrl+S apply · Tab and arrows move · Space/Enter choose", "단축키: Ctrl+Z 실행 취소 · Ctrl+Y 다시 실행 · Ctrl+S 적용 · Tab/화살표 이동 · Space/Enter 선택"), 11.0, 400, u.s.text2);
        }
    }
}

/// Applies a pure draft edit. Returns `true` when the draft changed.
/// Actions that need files, dialogs or the theme service are handled by the
/// application and return `false` here.
pub fn apply_draft_action(
    d: &mut Draft,
    a: &StudioAction,
    theme_opacity: f32,
    theme_height: u16,
) -> bool {
    use crate::popup::step_interval;
    let before = d.clone();
    match a {
        StudioAction::MediaRemove => {
            d.bar.style.background_asset = None;
            d.bar.style.background_poster_ms = None;
            return *d != before;
        }
        StudioAction::MediaPoster(delta, max) => {
            let cur = i64::from(d.bar.style.background_poster_ms.unwrap_or(0));
            let v = (cur + i64::from(*delta)).clamp(0, i64::from(*max));
            d.bar.style.background_poster_ms = Some(v as u32);
            return *d != before;
        }
        _ => {}
    }
    if let Some(changed) = motion_edit(d, a) {
        return changed && *d != before;
    }
    if matches!(a, StudioAction::ToggleBottomBar) {
        if d.editing_bottom {
            d.bar.enabled = !d.bar.enabled;
        } else {
            let o = d.other.get_or_insert_with(|| BarConfig {
                enabled: false,
                ..BarConfig::default_bottom()
            });
            o.enabled = !o.enabled;
        }
        return *d != before;
    }
    let b = &mut d.bar;
    let find = |b: &BarConfig, id: &str| b.modules.iter().position(|m| m.id == id);
    match a {
        StudioAction::Mode(p) => d.theme.mode = *p,
        StudioAction::Background(c) => b.style.background.clone_from(c),
        StudioAction::Accent(c) => b.style.accent.clone_from(c),
        StudioAction::TextColour(c) => b.style.text.clone_from(c),
        StudioAction::OpacityDown | StudioAction::OpacityUp => {
            let cur = b.style.opacity.unwrap_or(theme_opacity);
            let step = if *a == StudioAction::OpacityUp {
                0.05
            } else {
                -0.05
            };
            b.style.opacity = Some(((cur + step) * 100.0).round().clamp(30.0, 100.0) / 100.0);
        }
        StudioAction::OpacityFromTheme => b.style.opacity = None,
        StudioAction::Backplates(p) => b.style.backplates = *p,
        StudioAction::ToggleBorder => b.style.border_bottom = !b.style.border_bottom,
        StudioAction::ToggleSeparators => b.style.separators = !b.style.separators,
        StudioAction::ToggleModule(id) => {
            if let Some(i) = find(b, id)
                && b.modules[i].kind != ModuleKind::Menu
            {
                b.modules[i].enabled = !b.modules[i].enabled;
            }
        }
        StudioAction::ModuleEarlier(id) | StudioAction::ModuleLater(id) => {
            if let Some(i) = find(b, id) {
                let region = b.modules[i].region;
                let same: Vec<usize> = (0..b.modules.len())
                    .filter(|&k| b.modules[k].region == region)
                    .collect();
                let p = same.iter().position(|&k| k == i).unwrap_or(0);
                let other = if matches!(a, StudioAction::ModuleEarlier(_)) {
                    p.checked_sub(1)
                } else {
                    (p + 1 < same.len()).then_some(p + 1)
                };
                if let Some(o) = other {
                    b.modules.swap(i, same[o]);
                }
            }
        }
        StudioAction::ModuleRegion(id, rg) => {
            if let Some(i) = find(b, id) {
                move_module(b, i, *rg, usize::MAX);
            }
        }
        StudioAction::RemoveModule(id) => {
            if let Some(i) = find(b, id)
                && b.modules[i].kind != ModuleKind::Menu
            {
                b.modules.remove(i);
            }
        }
        StudioAction::AddModule(k) => {
            if b.modules.len() >= STUDIO_MAX_MODULES
                || (!k.is_custom() && b.modules.iter().any(|m| m.kind == *k))
            {
                return false;
            }
            let base = match k {
                ModuleKind::CustomText => "text",
                ModuleKind::CustomIcon => "icon",
                _ => "",
            };
            let id = if base.is_empty() {
                format!("{k:?}").to_lowercase()
            } else {
                (1..)
                    .map(|n| {
                        if n == 1 {
                            base.to_owned()
                        } else {
                            format!("{base}-{n}")
                        }
                    })
                    .find(|c| find(b, c).is_none())
                    .unwrap_or_default()
            };
            let id = match k {
                ModuleKind::ActiveWindow => "active-window".to_owned(),
                _ => id,
            };
            let mut mcfg = ModuleConfig::new(&id, *k, Region::Right, true);
            if *k == ModuleKind::CustomText {
                mcfg.text = Some("windowsRICE".into());
            }
            if *k == ModuleKind::CustomIcon {
                mcfg.icon = Some("E734".into());
                mcfg.label = Some("Star".into());
            }
            b.modules.push(mcfg);
        }
        StudioAction::IntervalShorter(id) | StudioAction::IntervalLonger(id) => {
            if let Some(i) = find(b, id)
                && let Some(cur) = b.modules[i].interval_ms()
            {
                let next = step_interval(cur, matches!(a, StudioAction::IntervalLonger(_)));
                b.modules[i].interval_ms = if Some(next) == b.modules[i].kind.default_interval_ms()
                {
                    None
                } else {
                    Some(next)
                };
            }
        }
        StudioAction::ToggleShowLabel(id) => {
            if let Some(i) = find(b, id) {
                b.modules[i].show_label = if b.modules[i].show_label == Some(false) {
                    None
                } else {
                    Some(false)
                };
            }
        }
        StudioAction::AllLabels(show) => {
            for m in b
                .modules
                .iter_mut()
                .filter(|m| ModuleConfig::has_label(m.kind))
            {
                m.show_label = if *show { None } else { Some(false) };
            }
        }
        StudioAction::ToggleHour12(id) => {
            if let Some(i) = find(b, id) {
                b.modules[i].hour12 = if b.modules[i].hour12 == Some(true) {
                    None
                } else {
                    Some(true)
                };
            }
        }
        StudioAction::MaxWidthDown(id) | StudioAction::MaxWidthUp(id) => {
            if let Some(i) = find(b, id) {
                let cur = b.modules[i].max_width;
                let next = if matches!(a, StudioAction::MaxWidthUp(_)) {
                    MAX_WIDTH_STEPS
                        .iter()
                        .copied()
                        .find(|&s| cur.is_none_or(|c| s > c))
                } else {
                    MAX_WIDTH_STEPS
                        .iter()
                        .rev()
                        .copied()
                        .find(|&s| cur.is_none_or(|c| s < c))
                };
                if let Some(n) = next {
                    b.modules[i].max_width = Some(n);
                }
            }
        }
        StudioAction::MaxWidthNone(id) => {
            if let Some(i) = find(b, id) {
                b.modules[i].max_width = None;
            }
        }
        StudioAction::CustomIcon(id, code) => {
            if let Some(i) = find(b, id) {
                b.modules[i].icon = Some((*code).to_owned());
                if let Some((_, n)) = ICON_CHOICES.iter().find(|(c, _)| c == code) {
                    b.modules[i].label = Some((*n).to_owned());
                }
            }
        }
        StudioAction::Font(f) => b.style.font_family.clone_from(f),
        StudioAction::FontSizeDown | StudioAction::FontSizeUp => {
            let cur = b.style.font_size.unwrap_or(13);
            let n = if *a == StudioAction::FontSizeUp {
                cur + 1
            } else {
                cur.saturating_sub(1)
            };
            b.style.font_size = Some(n.clamp(11, 18));
        }
        StudioAction::FontWeight(w) => b.style.font_weight = *w,
        StudioAction::HeightDown | StudioAction::HeightUp => {
            let cur = b.height.unwrap_or(theme_height);
            let n = if *a == StudioAction::HeightUp {
                cur + 2
            } else {
                cur.saturating_sub(2)
            };
            b.height = Some(n.clamp(24, 64));
        }
        StudioAction::PaddingDown | StudioAction::PaddingUp => {
            let cur = b.style.padding_x.unwrap_or(12);
            let n = if *a == StudioAction::PaddingUp {
                cur + 4
            } else {
                cur.saturating_sub(4)
            };
            b.style.padding_x = Some(n.min(48));
        }
        StudioAction::SpacingDown => b.spacing = b.spacing.saturating_sub(2),
        StudioAction::SpacingUp => b.spacing = (b.spacing + 2).min(24),
        StudioAction::VAlign(v) => b.style.vertical_align = *v,
        StudioAction::ToggleReserve => b.reserve_work_area = !b.reserve_work_area,
        StudioAction::ToggleAutoHide => b.auto_hide = !b.auto_hide,
        StudioAction::AutoHideDelayDown => {
            b.auto_hide_delay_ms = b.auto_hide_delay_ms.saturating_sub(200).max(200)
        }
        StudioAction::AutoHideDelayUp => {
            b.auto_hide_delay_ms = (b.auto_hide_delay_ms + 200).min(5000)
        }
        StudioAction::Fullscreen(p) => b.fullscreen = *p,
        StudioAction::Monitors(p) => b.monitors = *p,
        StudioAction::Slider(k, v) => return k.set(d, *v),
        StudioAction::GradientDirection(dir) => {
            if let Some(g) = b.style.gradient.as_mut() {
                g.direction = *dir;
            }
        }
        StudioAction::Material(mat) => b.style.material = *mat,
        StudioAction::ToggleHour12All => {
            let on = !b
                .modules
                .iter()
                .any(|m| m.kind == ModuleKind::Clock && m.hour12 == Some(true));
            for m in b.modules.iter_mut().filter(|m| m.kind == ModuleKind::Clock) {
                m.hour12 = on.then_some(true);
            }
        }
        StudioAction::ThemeCard(id, mode) => return apply_theme_card(d, id, *mode),
        StudioAction::ToggleAnimations => b.animations = !b.animations,
        StudioAction::ModuleSet(id, field) => {
            let Some(m) = b.modules.iter_mut().find(|m| &m.id == id) else {
                return false;
            };
            match field {
                ModuleField::Content(c) => m.content = *c,
                ModuleField::Color(c) => m.color.clone_from(c),
                ModuleField::Background(c) => m.background.clone_from(c),
                ModuleField::FontSize(delta) => {
                    let base = i32::from(m.font_size.unwrap_or(13));
                    m.font_size = Some((base + i32::from(*delta)).clamp(10, 20) as u16);
                }
                ModuleField::Density(x) => m.density = *x,
                ModuleField::ValueFormat(v) => {
                    if matches!(m.kind, ModuleKind::Ram | ModuleKind::Vram) {
                        m.value_format = *v;
                    }
                }
                ModuleField::SpeedUnit(v) => {
                    if matches!(m.kind, ModuleKind::Upload | ModuleKind::Download) {
                        m.speed_unit = *v;
                    }
                }
                ModuleField::ShowDate(v) => {
                    if m.kind == ModuleKind::Clock {
                        m.show_date = (!*v).then_some(false);
                    }
                }
                ModuleField::Reset => {
                    m.content = None;
                    m.color = None;
                    m.background = None;
                    m.font_size = None;
                    m.density = None;
                    m.value_format = None;
                    m.speed_unit = None;
                    m.show_date = None;
                }
            }
        }
        _ => return false,
    }
    *d != before
}

/// Moves module `i` to `region` at `index` within that region (clamped).
pub fn move_module(b: &mut BarConfig, i: usize, region: Region, index: usize) {
    if i >= b.modules.len() {
        return;
    }
    let mut m = b.modules.remove(i);
    m.region = region;
    let positions: Vec<usize> = (0..b.modules.len())
        .filter(|&k| b.modules[k].region == region)
        .collect();
    let at = match positions.get(index) {
        Some(&p) => p,
        None => positions.last().map_or(b.modules.len(), |&p| p + 1),
    };
    b.modules.insert(at.min(b.modules.len()), m);
}

/// Monitor toggle (adds or updates an override with the stable id).
pub fn toggle_monitor(b: &mut BarConfig, mon: &MonitorEntry) {
    let on = b.monitor_enabled(&mon.device, mon.id.as_deref(), mon.primary);
    let id = mon.id.clone();
    // Edit the override that matching would use for this monitor, so a
    // toggle made while the monitor is unidentified is not shadowed later.
    let pos = b
        .monitor_override(&mon.device, mon.id.as_deref())
        .and_then(|found| {
            b.monitor_overrides
                .iter()
                .position(|o| std::ptr::eq(o, found))
        });
    if let Some(o) = pos.map(|i| &mut b.monitor_overrides[i]) {
        o.enabled = !on;
        // Keep a known id while the monitor is unidentified.
        if id.is_some() {
            o.id.clone_from(&id);
        }
        o.device.clone_from(&mon.device);
    } else {
        b.monitor_overrides
            .push(windowsrice_core::bar::MonitorOverride {
                id,
                device: mon.device.clone(),
                enabled: !on,
                height: None,
            });
    }
}

/// Validates a `#RRGGBB` field.
pub fn valid_hex(s: &str) -> bool {
    parse_hex_color(s.trim()).is_some()
}

/// Groups of keyboard-reachable elements, for tests and diagnostics.
pub fn section_keys() -> HashMap<&'static str, Section> {
    Section::ALL.iter().map(|s| (s.key(), *s)).collect()
}

/// Section to open for a saved page key: the Home page of earlier versions
/// (and unknown keys) open Templates.
pub fn section_from_key(key: &str) -> Section {
    Section::ALL
        .iter()
        .copied()
        .find(|s| s.key() == key && *s != Section::Home)
        .unwrap_or(Section::Styles)
}

/// One per-module appearance setting (Phase 9).
#[derive(Debug, Clone, PartialEq)]
pub enum ModuleField {
    Content(Option<windowsrice_core::bar::ModuleContent>),
    Color(Option<String>),
    Background(Option<String>),
    /// Change the text size by this many DIPs.
    FontSize(i8),
    Density(Option<windowsrice_core::bar::Density>),
    ValueFormat(Option<windowsrice_core::bar::ValueFormat>),
    SpeedUnit(Option<windowsrice_core::bar::SpeedUnit>),
    /// Clock: show the date.
    ShowDate(bool),
    /// Back to the bar style.
    Reset,
}

/// Font gallery filter (Phase 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontFilter {
    #[default]
    All,
    /// Families with Hangul.
    Korean,
    /// Fixed-width families.
    Monospace,
}

// ------------------------------------------------------------ animation (Phase 14)

/// Keyframe colours of the animation presets.
pub const MOTION_PRESETS: [(&str, &str, &[&str]); 5] = [
    (
        "Rainbow",
        "무지개",
        &[
            "#FF5A5A", "#FFA94D", "#FFE066", "#69DB7C", "#4DABF7", "#9775FA", "#F783AC",
        ],
    ),
    ("Breathing", "숨쉬기", &["#4CC2FF", "#1E6FA8"]),
    ("Aurora", "오로라", &["#2BD9A5", "#3A7BD5", "#8E54E9"]),
    ("Sunset", "노을", &["#FF7E5F", "#FEB47B", "#C44569"]),
    ("Neon", "네온", &["#FF3DCB", "#4DF2FF"]),
];

fn motion_preset(
    target: windowsrice_core::motion::AnimTarget,
    i: usize,
) -> windowsrice_core::motion::ColorAnimation {
    use windowsrice_core::motion::{AnimDirection, ColorAnimation, Interpolation, Keyframe};
    let (_, _, colors) = MOTION_PRESETS[i.min(MOTION_PRESETS.len() - 1)];
    let n = colors.len() as f32;
    ColorAnimation {
        target,
        keyframes: colors
            .iter()
            .enumerate()
            .map(|(k, c)| Keyframe {
                at: (k as f32 / n * 100.0).round() / 100.0,
                color: (*c).to_owned(),
            })
            .collect(),
        duration_ms: if colors.len() > 3 { 8000 } else { 4000 },
        interpolation: if colors.len() > 3 {
            Interpolation::Linear
        } else {
            Interpolation::Smooth
        },
        direction: if colors.len() == 2 {
            AnimDirection::Alternate
        } else {
            AnimDirection::Normal
        },
        wrap: true,
        enabled: true,
    }
}

/// Applies an Animation-page draft edit; `None` for other actions.
fn motion_edit(d: &mut Draft, a: &StudioAction) -> Option<bool> {
    use windowsrice_core::motion::{
        Keyframe, MAX_ANIMATIONS, MAX_DURATION_MS, MAX_KEYFRAMES, MIN_DURATION_MS,
    };
    let list = &mut d.bar.style.animations;
    let find = |list: &mut Vec<windowsrice_core::motion::ColorAnimation>, t| {
        list.iter().position(|x| x.target == t)
    };
    let r = match a {
        StudioAction::MotionToggle(t) => match find(list, *t) {
            Some(i) => {
                list[i].enabled = !list[i].enabled;
                true
            }
            None if list.len() < MAX_ANIMATIONS => {
                list.push(motion_preset(*t, 0));
                true
            }
            None => false,
        },
        StudioAction::MotionPreset(t, p) => {
            let new = motion_preset(*t, usize::from(*p));
            match find(list, *t) {
                Some(i) => list[i] = new,
                None if list.len() < MAX_ANIMATIONS => list.push(new),
                None => return Some(false),
            }
            true
        }
        StudioAction::MotionAddKey(t) => {
            let Some(i) = find(list, *t) else {
                return Some(false);
            };
            let an = &mut list[i];
            if an.keyframes.len() >= MAX_KEYFRAMES {
                return Some(false);
            }
            // In the middle of the widest gap (including the loop gap).
            let mut pos: Vec<f32> = an.keyframes.iter().map(|k| k.at).collect();
            pos.sort_by(f32::total_cmp);
            let mut best = (0.5f32, 0.0f32);
            for w in pos.windows(2) {
                if w[1] - w[0] > best.1 {
                    best = ((w[0] + w[1]) / 2.0, w[1] - w[0]);
                }
            }
            if let (Some(f), Some(l)) = (pos.first(), pos.last())
                && 1.0 - l + f > best.1
            {
                best = (((l + 1.0 + f) / 2.0) % 1.0, 1.0 - l + f);
            }
            let at = (best.0 * 100.0).round() / 100.0;
            let c = an
                .color_at_phase(at)
                .map(|c| c.to_u8())
                .unwrap_or((255, 255, 255));
            an.keyframes.push(Keyframe {
                at,
                color: format!("#{:02X}{:02X}{:02X}", c.0, c.1, c.2),
            });
            an.keyframes.sort_by(|a, b| a.at.total_cmp(&b.at));
            true
        }
        StudioAction::MotionRemoveKey(t, k) => {
            let Some(i) = find(list, *t) else {
                return Some(false);
            };
            if list[i].keyframes.len() <= 2 || *k >= list[i].keyframes.len() {
                return Some(false);
            }
            list[i].keyframes.remove(*k);
            true
        }
        StudioAction::MotionMoveKey(t, k, step) => {
            let Some(i) = find(list, *t) else {
                return Some(false);
            };
            let ks = &mut list[i].keyframes;
            let Some(kf) = ks.get(*k) else {
                return Some(false);
            };
            // Between its neighbours, so the order is kept.
            let lo = if *k == 0 { 0.0 } else { ks[*k - 1].at + 0.01 };
            let hi = if *k + 1 >= ks.len() {
                1.0
            } else {
                ks[*k + 1].at - 0.01
            };
            let at = ((kf.at + f32::from(*step) * 0.05) * 100.0).round() / 100.0;
            ks[*k].at = at.clamp(lo, hi.max(lo));
            true
        }
        StudioAction::MotionInterp(t, v) => match find(list, *t) {
            Some(i) => {
                list[i].interpolation = *v;
                true
            }
            None => false,
        },
        StudioAction::MotionDirection(t, v) => match find(list, *t) {
            Some(i) => {
                list[i].direction = *v;
                true
            }
            None => false,
        },
        StudioAction::MotionWrap(t) => match find(list, *t) {
            Some(i) => {
                list[i].wrap = !list[i].wrap;
                true
            }
            None => false,
        },
        StudioAction::MotionSpeed(t, delta) => match find(list, *t) {
            Some(i) => {
                let v = (i64::from(list[i].duration_ms) + i64::from(*delta))
                    .clamp(i64::from(MIN_DURATION_MS), i64::from(MAX_DURATION_MS));
                list[i].duration_ms = v as u32;
                true
            }
            None => false,
        },
        _ => return None,
    };
    Some(r)
}

fn motion_page(u: &mut Ui, g: ElementId, r: Rect) {
    use windowsrice_core::motion::{AnimDirection, AnimTarget, Interpolation};
    let lang = u.lang;
    let m = u.m;
    let target = m.motion_target;
    let mut y = section_title(
        u,
        g,
        r,
        t(lang, "Animation", "애니메이션"),
        t(
            lang,
            "Animated colours for text, rainbow text, the accent, icons, the clock or the background. Animations pause while the bar is hidden and stay still when Windows \"Animation effects\" is off. Cycles shorter than a second are not allowed.",
            "글자, 무지개 글자, 강조색, 아이콘, 시계, 배경의 움직이는 색입니다. 바가 숨겨지면 멈추고, Windows \"애니메이션 효과\"가 꺼져 있으면 움직이지 않습니다. 1초보다 짧은 주기는 쓸 수 없습니다.",
        ),
    );
    let targets: [(AnimTarget, &str, &str); 6] = [
        (AnimTarget::TextGradient, "Rainbow text", "무지개 글자"),
        (AnimTarget::Text, "Text", "글자"),
        (AnimTarget::Accent, "Accent", "강조색"),
        (AnimTarget::Icons, "Icons", "아이콘"),
        (AnimTarget::Clock, "Clock", "시계"),
        (AnimTarget::Background, "Background", "배경"),
    ];
    let cw = ((r.w - 5.0 * 8.0) / 6.0).max(80.0);
    for (i, (tg, en, ko)) in targets.iter().enumerate() {
        let on = m
            .draft
            .bar
            .style
            .animations
            .iter()
            .any(|a| a.target == *tg && a.enabled);
        let label = if on {
            format!("{} ●", t(lang, en, ko))
        } else {
            t(lang, en, ko).to_owned()
        };
        u.radio(
            g,
            &format!("motion:target:{i}"),
            Rect::new(r.x + i as f32 * (cw + 8.0), y, cw, ROW),
            &label,
            *tg == target,
            StudioAction::MotionTarget(*tg),
        );
    }
    y += ROW + 12.0;
    let an = m
        .draft
        .bar
        .style
        .animations
        .iter()
        .find(|a| a.target == target);
    u.toggle(
        g,
        "motion:on",
        Rect::new(r.x, y, (r.w - 16.0) / 2.0, ROW),
        t(lang, "Animate", "움직이기"),
        an.is_some_and(|a| a.enabled),
        true,
        None,
        StudioAction::MotionToggle(target),
    );
    y += ROW + 10.0;
    // Presets.
    let pw = ((r.w - 4.0 * 8.0) / 5.0).min(150.0);
    for (i, (en, ko, _)) in MOTION_PRESETS.iter().enumerate() {
        u.button(
            g,
            &format!("motion:preset:{i}"),
            Rect::new(r.x + i as f32 * (pw + 8.0), y, pw, 32.0),
            None,
            t(lang, en, ko),
            t(lang, en, ko),
            Kind::Secondary,
            true,
            StudioAction::MotionPreset(target, i as u8),
        );
    }
    y += 44.0;
    let Some(an) = an else {
        u.label(
            g,
            "motion:none",
            Rect::new(r.x, y, r.w, 24.0),
            t(
                lang,
                "No animation yet: choose Animate or a preset.",
                "아직 애니메이션이 없습니다. 움직이기나 프리셋을 고르세요.",
            ),
            13.0,
            400,
            u.s.text2,
        );
        return;
    };
    // Timeline: the cycle as a gradient strip, keyframe markers and the
    // preview position.
    let tl = Rect::new(r.x, y + 8.0, r.w, 36.0);
    let strip = u.group(
        g,
        "motion:timeline",
        t(lang, "Timeline", "타임라인"),
        Rect::new(r.x, y, r.w, 64.0),
    );
    let n = 64;
    for s in 0..n {
        let p = s as f32 / n as f32;
        let c = an.color_at_phase(p).map(|c| c.to_u8()).unwrap_or((0, 0, 0));
        u.fill(
            Rect::new(tl.x + p * tl.w, tl.y, tl.w / n as f32 + 0.5, tl.h),
            0.0,
            Color::rgb(c.0, c.1, c.2),
            strip,
        );
    }
    let px = tl.x + tl.w * f32::from(m.motion_scrub) / 100.0;
    u.fill(
        Rect::new(px - 1.0, tl.y - 4.0, 2.0, tl.h + 8.0),
        1.0,
        u.s.text,
        strip,
    );
    for (k, kf) in an.keyframes.iter().enumerate() {
        let x = tl.x + tl.w * kf.at.clamp(0.0, 1.0);
        let name = match lang {
            Lang::En => format!("Keyframe {} at {:.0}%: {}", k + 1, kf.at * 100.0, kf.color),
            Lang::Ko => format!("키프레임 {} ({:.0}%): {}", k + 1, kf.at * 100.0, kf.color),
        };
        let id = u.add(
            strip,
            Element::new(
                key_id(&format!("motion:key:{k}")),
                intern(&format!("motion:key:{k}")),
                Role::ListItem,
                name,
                Rect::new(x - 9.0, tl.bottom() + 2.0, 18.0, 18.0),
            )
            .focusable(true)
            .selected(m.motion_key == Some(k)),
        );
        let col = parse_hex_color(&kf.color).map_or(u.s.text, |(r, g, b)| Color::rgb(r, g, b));
        u.fill(
            Rect::new(x - 7.0, tl.bottom() + 4.0, 14.0, 14.0),
            7.0,
            if m.motion_key == Some(k) {
                u.s.accent
            } else {
                u.s.text
            },
            id,
        );
        u.fill(
            Rect::new(x - 5.0, tl.bottom() + 6.0, 10.0, 10.0),
            5.0,
            col,
            id,
        );
        u.actions.push((id, StudioAction::MotionKey(k)));
    }
    y += 76.0;
    // Preview position (scrub).
    let scrub = match lang {
        Lang::En => format!("Preview at {}% of the cycle", m.motion_scrub),
        Lang::Ko => format!("주기의 {}% 지점 미리 보기", m.motion_scrub),
    };
    u.button(
        g,
        "motion:scrub-back",
        Rect::new(r.x, y, 32.0, 32.0),
        Some("\u{E76B}"),
        "",
        t(lang, "Earlier in the cycle", "주기의 앞쪽"),
        Kind::Secondary,
        true,
        StudioAction::MotionScrub(-10),
    );
    u.label(
        g,
        "motion:scrub",
        Rect::new(r.x + 40.0, y, 260.0, 32.0),
        &scrub,
        13.0,
        400,
        u.s.text,
    );
    u.button(
        g,
        "motion:scrub-on",
        Rect::new(r.x + 300.0, y, 32.0, 32.0),
        Some("\u{E76C}"),
        "",
        t(lang, "Later in the cycle", "주기의 뒤쪽"),
        Kind::Secondary,
        true,
        StudioAction::MotionScrub(10),
    );
    y += 44.0;
    // Selected keyframe.
    if let Some(k) = m.motion_key.filter(|k| *k < an.keyframes.len()) {
        let kf = &an.keyframes[k];
        let label = match lang {
            Lang::En => format!("Keyframe {} colour (#RRGGBB)", k + 1),
            Lang::Ko => format!("키프레임 {} 색 (#RRGGBB)", k + 1),
        };
        u.field(
            g,
            "field:motion-color",
            Rect::new(r.x, y, 260.0, 32.0),
            &label,
            &kf.color,
            "#RRGGBB",
        );
        let bx = r.x + 272.0;
        u.button(
            g,
            "motion:key-left",
            Rect::new(bx, y, 32.0, 32.0),
            Some("\u{E72B}"),
            "",
            t(lang, "Move keyframe earlier", "키프레임을 앞으로"),
            Kind::Secondary,
            true,
            StudioAction::MotionMoveKey(target, k, -1),
        );
        u.button(
            g,
            "motion:key-right",
            Rect::new(bx + 40.0, y, 32.0, 32.0),
            Some("\u{E72A}"),
            "",
            t(lang, "Move keyframe later", "키프레임을 뒤로"),
            Kind::Secondary,
            true,
            StudioAction::MotionMoveKey(target, k, 1),
        );
        u.button(
            g,
            "motion:key-remove",
            Rect::new(bx + 80.0, y, 120.0, 32.0),
            Some("\u{E74D}"),
            t(lang, "Remove", "삭제"),
            t(lang, "Remove keyframe", "키프레임 삭제"),
            Kind::Secondary,
            an.keyframes.len() > 2,
            StudioAction::MotionRemoveKey(target, k),
        );
        y += 44.0;
    }
    u.button(
        g,
        "motion:key-add",
        Rect::new(r.x, y, 180.0, 32.0),
        Some("\u{E710}"),
        t(lang, "Add keyframe", "키프레임 추가"),
        t(lang, "Add keyframe", "키프레임 추가"),
        Kind::Secondary,
        an.keyframes.len() < windowsrice_core::motion::MAX_KEYFRAMES,
        StudioAction::MotionAddKey(target),
    );
    y += 48.0;
    // Interpolation, direction, loop, speed.
    let third = ((r.w - 16.0) / 3.0).min(200.0);
    for (i, (v, en, ko)) in [
        (Interpolation::Linear, "Linear", "선형"),
        (Interpolation::Smooth, "Smooth", "부드럽게"),
        (Interpolation::Step, "Step", "단계"),
    ]
    .iter()
    .enumerate()
    {
        u.radio(
            g,
            &format!("motion:interp:{i}"),
            Rect::new(r.x + i as f32 * (third + 8.0), y, third, ROW),
            t(lang, en, ko),
            an.interpolation == *v,
            StudioAction::MotionInterp(target, *v),
        );
    }
    y += ROW + 6.0;
    for (i, (v, en, ko)) in [
        (AnimDirection::Normal, "Forward", "정방향"),
        (AnimDirection::Reverse, "Reverse", "역방향"),
        (AnimDirection::Alternate, "Back and forth", "왕복"),
    ]
    .iter()
    .enumerate()
    {
        u.radio(
            g,
            &format!("motion:dir:{i}"),
            Rect::new(r.x + i as f32 * (third + 8.0), y, third, ROW),
            t(lang, en, ko),
            an.direction == *v,
            StudioAction::MotionDirection(target, *v),
        );
    }
    y += ROW + 6.0;
    u.toggle(
        g,
        "motion:wrap",
        Rect::new(r.x, y, (r.w - 16.0) / 2.0, ROW),
        t(lang, "Seamless loop", "자연스러운 반복"),
        an.wrap,
        true,
        None,
        StudioAction::MotionWrap(target),
    );
    y += ROW + 6.0;
    let speed = match lang {
        Lang::En => format!("One cycle: {:.1} s", f64::from(an.duration_ms) / 1000.0),
        Lang::Ko => format!("한 주기: {:.1}초", f64::from(an.duration_ms) / 1000.0),
    };
    u.button(
        g,
        "motion:faster",
        Rect::new(r.x, y, 32.0, 32.0),
        Some("\u{E738}"),
        "",
        t(lang, "Faster (shorter cycle)", "빠르게 (짧은 주기)"),
        Kind::Secondary,
        an.duration_ms > windowsrice_core::motion::MIN_DURATION_MS,
        StudioAction::MotionSpeed(target, -1000),
    );
    u.label(
        g,
        "motion:speed",
        Rect::new(r.x + 40.0, y, 220.0, 32.0),
        &speed,
        13.0,
        400,
        u.s.text,
    );
    u.button(
        g,
        "motion:slower",
        Rect::new(r.x + 260.0, y, 32.0, 32.0),
        Some("\u{E710}"),
        "",
        t(lang, "Slower (longer cycle)", "느리게 (긴 주기)"),
        Kind::Secondary,
        an.duration_ms < windowsrice_core::motion::MAX_DURATION_MS,
        StudioAction::MotionSpeed(target, 1000),
    );
    u.content_bottom = u.content_bottom.max(y + 48.0);
}

fn media_page(u: &mut Ui, g: ElementId, r: Rect) {
    let lang = u.lang;
    let m = u.m;
    let mut y = section_title(
        u,
        g,
        r,
        t(lang, "Image and video", "이미지와 동영상"),
        t(
            lang,
            "A picture or a frame of a video behind the bar. Files are checked by their content (PNG, JPEG, MP4 only), limited in size, resolution and length, and copied into the design library under a generated name. In this version a video is shown as the still frame you choose; continuous playback is not available.",
            "바 뒤에 그림이나 동영상의 한 프레임을 둡니다. 파일은 내용으로 검사하며(PNG, JPEG, MP4만), 크기·해상도·길이를 제한하고 생성된 이름으로 디자인 라이브러리에 복사합니다. 이 버전은 동영상을 고른 정지 프레임으로 보여 주며, 계속 재생은 지원하지 않습니다.",
        ),
    );
    let bw = ((r.w - 8.0) / 2.0).min(240.0);
    u.button(
        g,
        "media:choose",
        Rect::new(r.x, y, bw, 34.0),
        Some("\u{E8E5}"),
        t(lang, "Choose image or video…", "이미지나 동영상 고르기…"),
        t(lang, "Choose image or video", "이미지나 동영상 고르기"),
        Kind::Primary,
        true,
        StudioAction::MediaChoose,
    );
    let has = m.draft.bar.style.background_asset.is_some();
    u.button(
        g,
        "media:remove",
        Rect::new(r.x + bw + 8.0, y, bw, 34.0),
        Some("\u{E74D}"),
        t(lang, "Remove background", "배경 제거"),
        t(lang, "Remove background", "배경 제거"),
        Kind::Secondary,
        has,
        StudioAction::MediaRemove,
    );
    y += 46.0;
    let info = match (&m.media_info, has, lang) {
        (_, false, Lang::En) => "No background image or video.".to_owned(),
        (_, false, Lang::Ko) => "배경 이미지나 동영상이 없습니다.".to_owned(),
        (None, true, Lang::En) => {
            "The background file is missing from the design library.".to_owned()
        }
        (None, true, Lang::Ko) => "디자인 라이브러리에 배경 파일이 없습니다.".to_owned(),
        (Some((false, w, h, _)), true, Lang::En) => format!("Image, {w}×{h} pixels."),
        (Some((false, w, h, _)), true, Lang::Ko) => format!("이미지, {w}×{h} 픽셀."),
        (Some((true, w, h, ms)), true, Lang::En) => {
            format!("Video, {w}×{h}, {:.1} s.", *ms as f64 / 1000.0)
        }
        (Some((true, w, h, ms)), true, Lang::Ko) => {
            format!("동영상, {w}×{h}, {:.1}초.", *ms as f64 / 1000.0)
        }
    };
    u.label(
        g,
        "media:info",
        Rect::new(r.x, y, r.w, 24.0),
        &info,
        13.0,
        400,
        u.s.text,
    );
    y += 34.0;
    if let Some((true, _, _, ms)) = m.media_info.filter(|_| has) {
        let max = u32::try_from(ms).unwrap_or(u32::MAX);
        let at = m.draft.bar.style.background_poster_ms.unwrap_or(0);
        let label = match lang {
            Lang::En => format!(
                "Frame shown: {:.2} s of {:.1} s",
                f64::from(at) / 1000.0,
                ms as f64 / 1000.0
            ),
            Lang::Ko => format!(
                "표시 프레임: {:.1}초 중 {:.2}초",
                ms as f64 / 1000.0,
                f64::from(at) / 1000.0
            ),
        };
        let steps: [(&str, &str, &str, &str, i32); 4] = [
            (
                "media:back-1s",
                "\u{E892}",
                "1 second earlier",
                "1초 앞",
                -1000,
            ),
            (
                "media:back-frame",
                "\u{E76B}",
                "One frame earlier",
                "한 프레임 앞",
                -40,
            ),
            (
                "media:on-frame",
                "\u{E76C}",
                "One frame later",
                "한 프레임 뒤",
                40,
            ),
            ("media:on-1s", "\u{E893}", "1 second later", "1초 뒤", 1000),
        ];
        for (i, (key, glyph, en, ko, d)) in steps.iter().enumerate() {
            let x = if i < 2 {
                r.x + i as f32 * 40.0
            } else {
                r.x + 340.0 + (i - 2) as f32 * 40.0
            };
            u.button(
                g,
                key,
                Rect::new(x, y, 32.0, 32.0),
                Some(glyph),
                "",
                t(lang, en, ko),
                Kind::Secondary,
                true,
                StudioAction::MediaPoster(*d, max),
            );
        }
        u.label(
            g,
            "media:frame",
            Rect::new(r.x + 84.0, y, 250.0, 32.0),
            &label,
            13.0,
            400,
            u.s.text,
        );
        y += 44.0;
    }
    u.content_bottom = u.content_bottom.max(y + 12.0);
}
