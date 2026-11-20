//! The Design Studio's visual catalog: complete styles, colour palettes,
//! gradients, backgrounds, typography, icon styles, module looks, layouts
//! and effects.
//!
//! Every card is a real edit of the draft (theme selection and `[bar]`
//! settings), written as a pure function. A card is shown as selected when
//! applying it would not change the draft, so the selection mark always
//! matches what the bar will draw. Cards never touch settings outside their
//! subject: a palette changes colours, a layout changes module placement,
//! and so on (the one exception, "Minimal information", says that it turns
//! modules off, and Undo restores them).

use windowsrice_core::bar::{
    AccentLine, BarStyle, Density, Gradient, GradientDirection, IconSet, IconTint, Material,
    ModuleContent, ModuleKind, ModuleShape, Region,
};
use windowsrice_core::config::ThemeModePreference;
use windowsrice_theme::Color;

use super::{Draft, FontEntry};

/// Which gallery a card belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gallery {
    Style,
    Palette,
    Gradient,
    Background,
    Typography,
    Icons,
    ModuleLook,
    Layout,
    Effect,
}

impl Gallery {
    pub fn key(self) -> &'static str {
        match self {
            Gallery::Style => "style",
            Gallery::Palette => "palette",
            Gallery::Gradient => "gradient",
            Gallery::Background => "background",
            Gallery::Typography => "typo",
            Gallery::Icons => "icons",
            Gallery::ModuleLook => "look",
            Gallery::Layout => "layout",
            Gallery::Effect => "effect",
        }
    }
}

/// Name and one-line description of a card, in English and Korean.
#[derive(Debug, Clone, Copy)]
pub struct Item {
    pub en: &'static str,
    pub ko: &'static str,
    pub desc_en: &'static str,
    pub desc_ko: &'static str,
}

const fn item(
    en: &'static str,
    ko: &'static str,
    desc_en: &'static str,
    desc_ko: &'static str,
) -> Item {
    Item {
        en,
        ko,
        desc_en,
        desc_ko,
    }
}

/// Colours of the current design that some cards build on (gradients are
/// made from the current background and accent).
#[derive(Debug, Clone)]
pub struct Ctx<'a> {
    pub background: Color,
    pub accent: Color,
    pub text: Color,
    pub fonts: &'a [FontEntry],
}

pub fn hex(c: Color) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)
}

fn mix(a: Color, b: Color, k: f32) -> Color {
    let m = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * k).round() as u8;
    Color::rgb(m(a.r, b.r), m(a.g, b.g), m(a.b, b.b))
}

fn s(v: &str) -> Option<String> {
    Some(v.to_owned())
}

// ------------------------------------------------------------------ styles

pub const STYLES: [Item; 15] = [
    item(
        "Minimal",
        "미니멀",
        "Solid, quiet and flat",
        "단색, 차분하고 평평한 디자인",
    ),
    item(
        "Fluent",
        "플루언트",
        "Windows 11 look with soft module tiles",
        "부드러운 모듈 타일의 Windows 11 느낌",
    ),
    item(
        "Glass",
        "글래스",
        "See-through bar with a light top edge",
        "윗면이 밝은 반투명 바",
    ),
    item(
        "Dark Modern",
        "다크 모던",
        "Dark tiles with blue accent icons",
        "파란 강조 아이콘의 어두운 타일",
    ),
    item(
        "Light Modern",
        "라이트 모던",
        "Light tiles with blue accent icons",
        "파란 강조 아이콘의 밝은 타일",
    ),
    item(
        "Midnight",
        "미드나이트",
        "Deep navy gradient with violet accents",
        "보라 강조의 짙은 남색 그라데이션",
    ),
    item(
        "Monochrome",
        "모노크롬",
        "Grays only, with separators",
        "회색만, 구분선 포함",
    ),
    item(
        "Colorful Accent",
        "컬러 강조",
        "Colour-coded icons and an accent line",
        "색으로 구분한 아이콘과 강조선",
    ),
    item(
        "Soft Contrast",
        "부드러운 대비",
        "Muted gray with gentle tiles",
        "은은한 타일의 차분한 회색",
    ),
    item("Compact", "컴팩트", "A low, dense bar", "낮고 촘촘한 바"),
    item(
        "Premium",
        "프리미엄",
        "Espresso gradient with an amber clock",
        "호박색 시계의 에스프레소 그라데이션",
    ),
    item(
        "Futuristic",
        "퓨처리스틱",
        "Neon accent line and square tiles",
        "네온 강조선과 사각 타일",
    ),
    item(
        "Editorial",
        "에디토리얼",
        "Serif type with labels and separators",
        "레이블과 구분선의 세리프 글꼴",
    ),
    item(
        "Soft Rounded",
        "소프트 라운드",
        "Pastel pills on a taller bar",
        "높은 바 위의 파스텔 알약 모양",
    ),
    item(
        "High Contrast",
        "고대비",
        "Maximum contrast and a thick border",
        "최대 대비와 두꺼운 테두리",
    ),
];

fn style_for(
    i: usize,
) -> (
    &'static str,
    ThemeModePreference,
    BarStyle,
    Option<u16>,
    u16,
) {
    use ThemeModePreference::{Auto, Dark, Light};
    let mut st = BarStyle::default();
    let mut height = None;
    let mut spacing = 4;
    let theme = match i {
        0 => {
            st.material = Material::Solid;
            ("windowsrice-minimal", Dark)
        }
        1 => {
            st.module_shape = ModuleShape::Rounded;
            st.module_opacity = Some(60);
            ("windowsrice-default", Auto)
        }
        2 => {
            st.material = Material::Translucent;
            st.opacity = Some(0.62);
            st.highlight = Some(30);
            ("windowsrice-glass", Dark)
        }
        3 => {
            st.module_shape = ModuleShape::Rounded;
            st.icon_tint = IconTint::Accent;
            st.material = Material::Solid;
            ("windowsrice-blue-accent", Dark)
        }
        4 => {
            st.module_shape = ModuleShape::Rounded;
            st.icon_tint = IconTint::Accent;
            st.material = Material::Solid;
            ("windowsrice-blue-accent", Light)
        }
        5 => {
            st.gradient = Some(Gradient {
                direction: GradientDirection::Vertical,
                colors: vec!["#151B33".into(), "#0B1020".into()],
            });
            st.icon_tint = IconTint::Accent;
            st.clock_color = s("#C9BFFF");
            ("windowsrice-midnight", Dark)
        }
        6 => {
            st.icon_tint = IconTint::Secondary;
            st.separators = true;
            st.material = Material::Solid;
            ("windowsrice-monochrome", Dark)
        }
        7 => {
            st.icon_tint = IconTint::Colorful;
            st.accent_line = AccentLine::Bottom;
            st.content = ModuleContent::Full;
            ("windowsrice-default", Dark)
        }
        8 => {
            st.module_shape = ModuleShape::Rounded;
            st.module_opacity = Some(45);
            st.material = Material::Solid;
            ("windowsrice-soft-gray", Dark)
        }
        9 => {
            st.density = Density::Compact;
            st.font_size = Some(12);
            st.padding_x = Some(6);
            height = Some(28);
            spacing = 2;
            ("windowsrice-default", Auto)
        }
        10 => {
            st.gradient = Some(Gradient {
                direction: GradientDirection::Horizontal,
                colors: vec!["#1E1712".into(), "#2E2219".into(), "#1E1712".into()],
            });
            st.clock_color = s("#F0B060");
            st.clock_size = Some(15);
            st.highlight = Some(18);
            st.border_bottom = true;
            st.border_color = s("#6B4A2A");
            st.material = Material::Solid;
            height = Some(38);
            ("windowsrice-warm", Dark)
        }
        11 => {
            st.accent_line = AccentLine::Bottom;
            st.module_shape = ModuleShape::Square;
            st.module_opacity = Some(40);
            st.icon_tint = IconTint::Accent;
            st.font_family = s("Bahnschrift");
            st.material = Material::Solid;
            ("windowsrice-neon", Dark)
        }
        12 => {
            st.font_family = s("Georgia");
            st.content = ModuleContent::Full;
            st.separators = true;
            st.density = Density::Expanded;
            st.material = Material::Solid;
            ("windowsrice-light", Light)
        }
        13 => {
            st.module_shape = ModuleShape::Pill;
            st.module_opacity = Some(80);
            st.density = Density::Expanded;
            st.material = Material::Solid;
            height = Some(40);
            spacing = 6;
            ("windowsrice-pastel", Light)
        }
        _ => {
            st.material = Material::Solid;
            st.border_bottom = true;
            st.border_width = Some(2);
            st.font_weight = Some(500);
            st.content = ModuleContent::Full;
            ("windowsrice-high-contrast", Dark)
        }
    };
    (theme.0, theme.1, st, height, spacing)
}

// ------------------------------------------------------------------ palettes

/// (item, background, module background, accent, text, secondary text).
pub const PALETTES: [(Item, &str, &str, &str, &str, &str); 14] = [
    (
        item("Ocean Blue", "오션 블루", "", ""),
        "#0E2A3F",
        "#163A54",
        "#4FB3FF",
        "#E8F4FC",
        "#B6CFDF",
    ),
    (
        item("Midnight Purple", "미드나이트 퍼플", "", ""),
        "#140F24",
        "#22183A",
        "#A78BFA",
        "#EDE9FE",
        "#C4B8E8",
    ),
    (
        item("Cyber Neon", "사이버 네온", "", ""),
        "#07070D",
        "#141428",
        "#FF2BD6",
        "#F2F2FF",
        "#B8B8E0",
    ),
    (
        item("Emerald", "에메랄드", "", ""),
        "#0D2219",
        "#163428",
        "#34D399",
        "#E7F7EF",
        "#B5D9C7",
    ),
    (
        item("Rose", "로즈", "", ""),
        "#2A1219",
        "#3B1A24",
        "#FB7185",
        "#FDECEF",
        "#E3BAC3",
    ),
    (
        item("Sunset", "선셋", "", ""),
        "#2B1610",
        "#3D2016",
        "#FB923C",
        "#FFF1E6",
        "#E8C2A8",
    ),
    (
        item("Arctic", "아크틱", "", ""),
        "#EAF3F8",
        "#D7E7F0",
        "#0E7490",
        "#0F2430",
        "#3A5563",
    ),
    (
        item("Graphite", "그래파이트", "", ""),
        "#1F2023",
        "#2B2D31",
        "#9CA3AF",
        "#ECECEE",
        "#B9BBC0",
    ),
    (
        item("Warm Sand", "웜 샌드", "", ""),
        "#F1E7D6",
        "#E5D6BE",
        "#9A5B13",
        "#2A2118",
        "#5A4A38",
    ),
    (
        item("Lavender", "라벤더", "", ""),
        "#EEE8F8",
        "#DFD5F2",
        "#6D28D9",
        "#221A33",
        "#4D4266",
    ),
    (
        item("Monochrome", "모노크롬", "", ""),
        "#111111",
        "#222222",
        "#DDDDDD",
        "#F5F5F5",
        "#BDBDBD",
    ),
    (
        item("Classic Windows", "클래식 Windows", "", ""),
        "#202020",
        "#2B2B2B",
        "#4CC2FF",
        "#FFFFFF",
        "#C8C8C8",
    ),
    (
        item("Soft Pastel", "소프트 파스텔", "", ""),
        "#FBF3F9",
        "#F1E3EE",
        "#B4537F",
        "#2E2130",
        "#5C4A5A",
    ),
    (
        item("Deep Contrast", "딥 콘트라스트", "", ""),
        "#000000",
        "#121212",
        "#FFD400",
        "#FFFFFF",
        "#E0E0E0",
    ),
];

// ------------------------------------------------------------------ gradients

pub const GRADIENTS: [Item; 7] = [
    item(
        "Solid",
        "단색",
        "One colour, no gradient",
        "그라데이션 없는 한 가지 색",
    ),
    item(
        "Horizontal",
        "가로",
        "Background to accent tint, left to right",
        "왼쪽에서 오른쪽으로 배경에서 강조색",
    ),
    item("Vertical", "세로", "Lighter at the top", "위쪽이 더 밝음"),
    item(
        "Diagonal",
        "대각선",
        "Corner to corner",
        "모서리에서 모서리로",
    ),
    item(
        "Multi-stop",
        "다중 색",
        "Background, accent tint, background",
        "배경, 강조색, 배경",
    ),
    item(
        "Subtle",
        "은은함",
        "A barely visible shift",
        "거의 보이지 않는 변화",
    ),
    item(
        "Accent",
        "강조색",
        "Accent fading into the background",
        "강조색에서 배경으로",
    ),
];

fn gradient_for(i: usize, c: &Ctx) -> Option<Gradient> {
    let bg = c.background;
    let lighter = mix(bg, c.text, 0.10);
    let tint = mix(bg, c.accent, 0.28);
    let g = |direction, colors: Vec<Color>| {
        Some(Gradient {
            direction,
            colors: colors.into_iter().map(hex).collect(),
        })
    };
    match i {
        0 => None,
        1 => g(GradientDirection::Horizontal, vec![bg, tint]),
        2 => g(GradientDirection::Vertical, vec![lighter, bg]),
        3 => g(GradientDirection::Diagonal, vec![bg, tint]),
        4 => g(GradientDirection::Horizontal, vec![bg, tint, bg]),
        5 => g(GradientDirection::Vertical, vec![mix(bg, c.text, 0.04), bg]),
        _ => g(
            GradientDirection::Horizontal,
            vec![mix(bg, c.accent, 0.40), bg, bg],
        ),
    }
}

// ------------------------------------------------------------------ backgrounds

pub const BACKGROUNDS: [Item; 9] = [
    item("Solid", "단색", "Opaque surface", "불투명한 표면"),
    item(
        "Translucent",
        "반투명",
        "80% opaque; what is behind shows through",
        "80% 불투명, 뒤가 비쳐 보임",
    ),
    item(
        "Glass",
        "글래스",
        "60% opaque with a light top edge",
        "윗면이 밝은 60% 불투명",
    ),
    item(
        "Frosted",
        "서리 유리",
        "Not available: the bar cannot blur what is behind it",
        "사용할 수 없음: 바는 뒤를 흐리게 할 수 없습니다",
    ),
    item(
        "Gradient",
        "그라데이션",
        "Vertical gradient from the background colour",
        "배경색의 세로 그라데이션",
    ),
    item(
        "Dark translucent",
        "어두운 반투명",
        "Near-black at 72% with light text",
        "밝은 글자의 72% 거의 검정",
    ),
    item(
        "Light translucent",
        "밝은 반투명",
        "Near-white at 78% with dark text",
        "어두운 글자의 78% 거의 흰색",
    ),
    item(
        "Custom colour",
        "사용자 색",
        "Pick any background colour in Colours",
        "색에서 아무 배경색이나 고르기",
    ),
    item(
        "High opacity",
        "높은 불투명도",
        "95% opaque: almost solid",
        "95% 불투명: 거의 단색",
    ),
];

/// Backgrounds that cannot be drawn (and why, in the item description).
pub fn available(g: Gallery, i: usize, c: &Ctx) -> bool {
    match g {
        Gallery::Background => i != 3,
        Gallery::Typography => typo_for(i)
            .0
            .is_none_or(|f| c.fonts.iter().any(|x| x.family == f)),
        Gallery::Effect => i != 6,
        _ => true,
    }
}

// ------------------------------------------------------------------ typography

pub const TYPOGRAPHY: [Item; 8] = [
    item(
        "Clean",
        "깔끔하게",
        "Segoe UI Variable, 13",
        "Segoe UI Variable, 13",
    ),
    item(
        "Compact",
        "작게",
        "Segoe UI Variable, 12",
        "Segoe UI Variable, 12",
    ),
    item("Large", "크게", "15 with a 17 clock", "15, 시계 17"),
    item("Big clock", "큰 시계", "13 with an 18 clock", "13, 시계 18"),
    item("Korean", "한글", "Malgun Gothic, 13", "맑은 고딕, 13"),
    item(
        "Technical",
        "테크니컬",
        "Cascadia Code, 12",
        "Cascadia Code, 12",
    ),
    item(
        "Display",
        "디스플레이",
        "Bahnschrift, 14",
        "Bahnschrift, 14",
    ),
    item(
        "Light",
        "가늘게",
        "Segoe UI Variable Light, 13",
        "Segoe UI Variable Light, 13",
    ),
];

/// (family, size, weight, clock size).
fn typo_for(i: usize) -> (Option<&'static str>, u16, u16, Option<u16>) {
    match i {
        0 => (None, 13, 400, None),
        1 => (None, 12, 400, Some(12)),
        2 => (None, 15, 400, Some(17)),
        3 => (None, 13, 400, Some(18)),
        4 => (Some("Malgun Gothic"), 13, 400, None),
        5 => (Some("Cascadia Code"), 12, 400, None),
        6 => (Some("Bahnschrift"), 14, 400, None),
        _ => (None, 13, 300, None),
    }
}

/// Family shown on a typography card (`None`: the theme's family).
pub fn typo_family(i: usize) -> Option<&'static str> {
    typo_for(i).0
}

// ------------------------------------------------------------------ icons

pub const ICONS: [Item; 6] = [
    item(
        "Fluent",
        "플루언트",
        "Segoe Fluent Icons in the text colour",
        "글자색의 Segoe Fluent Icons",
    ),
    item(
        "Classic",
        "클래식",
        "Segoe MDL2 Assets (Windows 10 shapes)",
        "Segoe MDL2 Assets (Windows 10 모양)",
    ),
    item(
        "Accent",
        "강조색",
        "Icons in the accent colour",
        "강조색 아이콘",
    ),
    item(
        "Colourful",
        "컬러풀",
        "A colour per module",
        "모듈마다 다른 색",
    ),
    item(
        "Monochrome",
        "모노크롬",
        "Icons in the secondary text colour",
        "보조 글자색 아이콘",
    ),
    item(
        "Large",
        "크게",
        "Icons 4 DIPs larger",
        "아이콘 4 DIP 더 크게",
    ),
];

fn icons_for(i: usize) -> (IconSet, IconTint, Option<i8>) {
    match i {
        0 => (IconSet::Fluent, IconTint::Text, None),
        1 => (IconSet::Classic, IconTint::Text, None),
        2 => (IconSet::Fluent, IconTint::Accent, None),
        3 => (IconSet::Fluent, IconTint::Colorful, None),
        4 => (IconSet::Fluent, IconTint::Secondary, None),
        _ => (IconSet::Fluent, IconTint::Text, Some(4)),
    }
}

// ------------------------------------------------------------------ module looks

pub const LOOKS: [Item; 10] = [
    item(
        "Icon only",
        "아이콘만",
        "Icons; modules without one show their value",
        "아이콘만, 아이콘이 없으면 값",
    ),
    item(
        "Value only",
        "값만",
        "Values without icons or labels",
        "아이콘·레이블 없이 값만",
    ),
    item("Icon + value", "아이콘 + 값", "The default", "기본값"),
    item(
        "Icon + label + value",
        "아이콘 + 레이블 + 값",
        "Everything, on every module",
        "모든 모듈에 모두 표시",
    ),
    item(
        "Compact",
        "촘촘하게",
        "Less space inside each module",
        "모듈 안 여백을 줄임",
    ),
    item(
        "Expanded",
        "넉넉하게",
        "More space inside each module",
        "모듈 안 여백을 늘림",
    ),
    item(
        "Pill",
        "알약",
        "Fully rounded module backgrounds",
        "완전히 둥근 모듈 배경",
    ),
    item(
        "Flat",
        "평평하게",
        "No module backgrounds",
        "모듈 배경 없음",
    ),
    item(
        "Separated",
        "구분선",
        "Thin lines between modules",
        "모듈 사이 얇은 선",
    ),
    item(
        "Minimal",
        "미니멀",
        "Values only, compact, no backgrounds",
        "값만, 촘촘하게, 배경 없음",
    ),
];

// ------------------------------------------------------------------ layouts

pub const LAYOUTS: [Item; 9] = [
    item(
        "Balanced",
        "균형",
        "Title left, clock centre, status right",
        "제목 왼쪽, 시계 가운데, 상태 오른쪽",
    ),
    item(
        "Left focus",
        "왼쪽 중심",
        "Clock moves next to the title",
        "시계를 제목 옆으로",
    ),
    item(
        "Centered",
        "가운데 중심",
        "Clock and status together in the middle",
        "시계와 상태를 가운데에 함께",
    ),
    item(
        "Right focus",
        "오른쪽 중심",
        "Clock at the right end",
        "시계를 오른쪽 끝으로",
    ),
    item(
        "Symmetrical",
        "대칭",
        "System readings left, connections right",
        "시스템 수치 왼쪽, 연결 오른쪽",
    ),
    item(
        "Compact",
        "컴팩트",
        "Balanced, lower and denser",
        "균형, 더 낮고 촘촘하게",
    ),
    item(
        "Wide spacing",
        "넓은 간격",
        "Balanced with generous gaps",
        "균형, 넉넉한 간격",
    ),
    item(
        "Split regions",
        "영역 분리",
        "Clock left, title centre, status right",
        "시계 왼쪽, 제목 가운데, 상태 오른쪽",
    ),
    item(
        "Minimal information",
        "최소 정보",
        "Turns off everything but title, clock, volume and notifications",
        "제목·시계·음량·알림 외에는 끔",
    ),
];

fn region_of(layout: usize, k: ModuleKind) -> Region {
    use ModuleKind as K;
    use Region::{Center, Left, Right};
    match (layout, k) {
        (_, K::Menu) => Left,
        (7, K::ActiveWindow) => Center,
        (_, K::ActiveWindow) => Left,
        (1 | 7, K::Clock) => Left,
        (3, K::Clock) => Right,
        (_, K::Clock) => Center,
        (2, _) => Center,
        (4, K::Cpu | K::Gpu | K::Ram | K::Vram | K::Battery) => Left,
        _ => Right,
    }
}

/// Order of kinds inside a region (lower first).
fn rank(layout: usize, k: ModuleKind) -> u8 {
    use ModuleKind as K;
    match k {
        K::Menu => 0,
        K::ActiveWindow => 1,
        K::Clock if layout == 3 => 99,
        K::Clock if layout == 2 => 3,
        K::Clock => 2,
        K::Download => 10,
        K::Upload => 11,
        K::Network => 12,
        K::Volume => 13,
        K::Cpu => 20,
        K::Gpu => 21,
        K::Vram => 22,
        K::Ram => 23,
        K::Battery => 24,
        K::CustomText | K::CustomIcon => 30,
        K::Notifications => 40,
    }
}

fn arrange(d: &mut Draft, layout: usize) {
    let mut ms = d.bar.modules.clone();
    for m in &mut ms {
        // Custom modules keep the region the user gave them.
        if !matches!(m.kind, ModuleKind::CustomText | ModuleKind::CustomIcon) {
            m.region = region_of(layout, m.kind);
        }
    }
    // Stable: equal ranks keep their order.
    ms.sort_by_key(|m| rank(layout, m.kind));
    d.bar.modules = ms;
}

// ------------------------------------------------------------------ effects

pub const EFFECTS: [Item; 11] = [
    item(
        "Bottom border",
        "아래 테두리",
        "A line along the bottom edge",
        "아래쪽 가장자리의 선",
    ),
    item(
        "Separators",
        "구분선",
        "Thin lines between modules",
        "모듈 사이의 얇은 선",
    ),
    item(
        "Shadow",
        "그림자",
        "Shading inside the lower edge",
        "아래쪽 가장자리 안쪽의 음영",
    ),
    item(
        "Highlight",
        "하이라이트",
        "A light line along the top edge",
        "위쪽 가장자리의 밝은 선",
    ),
    item(
        "Strong hover",
        "진한 호버",
        "A stronger background under the pointer",
        "포인터 아래 더 진한 배경",
    ),
    item(
        "Selection colour",
        "선택 색",
        "The open menu button in the accent tint",
        "열린 메뉴 버튼을 강조색으로",
    ),
    item(
        "Focus outline",
        "포커스 윤곽선",
        "Always on: keyboard focus must stay visible",
        "항상 켜짐: 키보드 포커스는 항상 보여야 함",
    ),
    item(
        "Rounded tiles",
        "둥근 타일",
        "Rounded module backgrounds",
        "둥근 모듈 배경",
    ),
    item(
        "Flat tiles",
        "평평한 타일",
        "Square module backgrounds",
        "사각 모듈 배경",
    ),
    item(
        "Subtle depth",
        "은은한 입체감",
        "A soft vertical gradient",
        "부드러운 세로 그라데이션",
    ),
    item(
        "Accent indicator",
        "강조 표시",
        "An accent line along the bottom",
        "아래쪽 강조선",
    ),
];

fn effect_on(i: usize, s: &BarStyle) -> bool {
    match i {
        0 => s.border_bottom,
        1 => s.separators,
        2 => s.shadow.is_some_and(|v| v > 0),
        3 => s.highlight.is_some_and(|v| v > 0),
        4 => s.hover_intensity.is_some_and(|v| v >= 80),
        5 => s.selected_color.is_some(),
        6 => true,
        7 => matches!(s.module_shape, ModuleShape::Rounded | ModuleShape::Pill),
        8 => s.module_shape == ModuleShape::Square,
        9 => s
            .gradient
            .as_ref()
            .is_some_and(|g| g.direction == GradientDirection::Vertical),
        _ => s.accent_line != AccentLine::None,
    }
}

// ------------------------------------------------------------------ API

pub fn items(g: Gallery) -> Vec<Item> {
    match g {
        Gallery::Style => STYLES.to_vec(),
        Gallery::Palette => PALETTES.iter().map(|p| p.0).collect(),
        Gallery::Gradient => GRADIENTS.to_vec(),
        Gallery::Background => BACKGROUNDS.to_vec(),
        Gallery::Typography => TYPOGRAPHY.to_vec(),
        Gallery::Icons => ICONS.to_vec(),
        Gallery::ModuleLook => LOOKS.to_vec(),
        Gallery::Layout => LAYOUTS.to_vec(),
        Gallery::Effect => EFFECTS.to_vec(),
    }
}

/// Palette swatches (background, module background, accent, text, secondary).
pub fn palette_colors(i: usize) -> [&'static str; 5] {
    let p = PALETTES[i];
    [p.1, p.2, p.3, p.4, p.5]
}

/// Applies card `i` of gallery `g` to the draft. Returns `true` if the draft
/// changed. Effects toggle.
pub fn apply(g: Gallery, i: usize, d: &mut Draft, c: &Ctx) -> bool {
    if !available(g, i, c) {
        return false;
    }
    let before = d.clone();
    let st = &mut d.bar.style;
    match g {
        Gallery::Style => {
            let (theme, mode, style, height, spacing) = style_for(i);
            d.theme.active = theme.to_owned();
            d.theme.mode = mode;
            d.bar.style = style;
            d.bar.height = height;
            d.bar.spacing = spacing;
        }
        Gallery::Palette => {
            let [bg, mbg, accent, text, text2] = palette_colors(i);
            st.background = s(bg);
            st.module_background = s(mbg);
            st.accent = s(accent);
            st.text = s(text);
            st.text_secondary = s(text2);
            st.gradient = None;
        }
        Gallery::Gradient => st.gradient = gradient_for(i, c),
        Gallery::Background => match i {
            0 => {
                st.material = Material::Solid;
                st.gradient = None;
            }
            1 => {
                st.material = Material::Translucent;
                st.opacity = Some(0.80);
                st.highlight = None;
            }
            2 => {
                st.material = Material::Translucent;
                st.opacity = Some(0.60);
                st.highlight = Some(30);
            }
            4 => st.gradient = gradient_for(2, c),
            5 => {
                st.material = Material::Translucent;
                st.opacity = Some(0.72);
                st.background = s("#101216");
                st.text = s("#F2F3F5");
                st.text_secondary = None;
                st.gradient = None;
            }
            6 => {
                st.material = Material::Translucent;
                st.opacity = Some(0.78);
                st.background = s("#F4F5F7");
                st.text = s("#15171B");
                st.text_secondary = None;
                st.gradient = None;
            }
            7 => {
                st.material = Material::Solid;
                st.gradient = None;
                st.background = Some(hex(c.background));
            }
            _ => {
                st.material = Material::Translucent;
                st.opacity = Some(0.95);
            }
        },
        Gallery::Typography => {
            let (family, size, weight, clock) = typo_for(i);
            st.font_family = family.map(str::to_owned);
            st.font_size = (size != 13).then_some(size);
            st.font_weight = (weight != 400).then_some(weight);
            st.clock_size = clock;
        }
        Gallery::Icons => {
            let (set, tint, size) = icons_for(i);
            st.icon_set = set;
            st.icon_tint = tint;
            st.icon_size = size;
        }
        Gallery::ModuleLook => match i {
            0 => st.content = ModuleContent::IconOnly,
            1 => st.content = ModuleContent::ValueOnly,
            2 => st.content = ModuleContent::Default,
            3 => st.content = ModuleContent::Full,
            4 => st.density = Density::Compact,
            5 => st.density = Density::Expanded,
            6 => {
                st.module_shape = ModuleShape::Pill;
                st.module_opacity = st.module_opacity.or(Some(70));
            }
            7 => st.module_shape = ModuleShape::None,
            8 => {
                st.separators = true;
                st.module_shape = ModuleShape::None;
            }
            _ => {
                st.content = ModuleContent::ValueOnly;
                st.density = Density::Compact;
                st.module_shape = ModuleShape::None;
                st.separators = false;
            }
        },
        Gallery::Layout => {
            let base = match i {
                5 | 6 | 8 => 0,
                n => n,
            };
            arrange(d, base);
            let st = &mut d.bar.style;
            match i {
                5 => {
                    d.bar.height = Some(28);
                    d.bar.spacing = 2;
                    st.density = Density::Compact;
                }
                6 => {
                    d.bar.spacing = 12;
                    st.padding_x = Some(24);
                    st.region_gap = Some(48);
                }
                8 => {
                    for m in &mut d.bar.modules {
                        m.enabled = matches!(
                            m.kind,
                            ModuleKind::Menu
                                | ModuleKind::ActiveWindow
                                | ModuleKind::Clock
                                | ModuleKind::Volume
                                | ModuleKind::Notifications
                        );
                    }
                }
                _ => {}
            }
        }
        Gallery::Effect => {
            let on = effect_on(i, st);
            match i {
                0 => st.border_bottom = !on,
                1 => st.separators = !on,
                2 => st.shadow = (!on).then_some(40),
                3 => st.highlight = (!on).then_some(35),
                4 => st.hover_intensity = (!on).then_some(100),
                5 => st.selected_color = (!on).then(|| hex(mix(c.background, c.accent, 0.45))),
                7 => {
                    st.module_shape = if on {
                        ModuleShape::None
                    } else {
                        ModuleShape::Rounded
                    };
                    if !on {
                        st.module_opacity = st.module_opacity.or(Some(60));
                    }
                }
                8 => {
                    st.module_shape = if on {
                        ModuleShape::None
                    } else {
                        ModuleShape::Square
                    };
                    if !on {
                        st.module_opacity = st.module_opacity.or(Some(60));
                    }
                }
                9 => {
                    st.gradient = if on { None } else { gradient_for(2, c) };
                }
                _ => {
                    st.accent_line = if on {
                        AccentLine::None
                    } else {
                        AccentLine::Bottom
                    };
                }
            }
        }
    }
    *d != before
}

/// Whether card `i` describes the draft as it is.
pub fn is_selected(g: Gallery, i: usize, d: &Draft, c: &Ctx) -> bool {
    match g {
        Gallery::Effect => effect_on(i, &d.bar.style),
        Gallery::Gradient if i == 0 => d.bar.style.gradient.is_none(),
        _ => {
            if !available(g, i, c) {
                return false;
            }
            let mut x = d.clone();
            !apply(g, i, &mut x, c)
        }
    }
}

/// The theme and mode a style card uses (for its preview).
pub fn style_theme(i: usize) -> (&'static str, ThemeModePreference) {
    let (t, m, ..) = style_for(i);
    (t, m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windowsrice_core::bar::BarConfig;
    use windowsrice_core::config::ThemeSelection;

    fn ctx() -> Ctx<'static> {
        static FONTS: std::sync::OnceLock<Vec<FontEntry>> = std::sync::OnceLock::new();
        let fonts = FONTS.get_or_init(|| {
            [
                "Segoe UI Variable",
                "Malgun Gothic",
                "Cascadia Code",
                "Bahnschrift",
                "Georgia",
            ]
            .iter()
            .map(|f| FontEntry {
                family: (*f).to_owned(),
                hangul: *f == "Malgun Gothic",
                weights: vec![400],
                mono: *f == "Cascadia Code",
            })
            .collect()
        });
        Ctx {
            background: Color::rgb(0x1B, 0x1D, 0x23),
            accent: Color::rgb(0x5B, 0xA4, 0xE6),
            text: Color::rgb(0xE8, 0xEA, 0xEE),
            fonts,
        }
    }

    fn draft() -> Draft {
        Draft {
            theme: ThemeSelection::default(),
            bar: BarConfig::default(),
        }
    }

    #[test]
    fn every_card_is_valid_and_selected_after_applying() {
        let c = ctx();
        for g in [
            Gallery::Style,
            Gallery::Palette,
            Gallery::Gradient,
            Gallery::Background,
            Gallery::Typography,
            Gallery::Icons,
            Gallery::ModuleLook,
            Gallery::Layout,
        ] {
            for i in 0..items(g).len() {
                let mut d = draft();
                if !available(g, i, &c) {
                    assert!(!apply(g, i, &mut d, &c), "{g:?} {i} is unavailable");
                    continue;
                }
                apply(g, i, &mut d, &c);
                assert!(
                    d.bar.problems().is_empty(),
                    "{g:?} {i}: {:?}",
                    d.bar.problems()
                );
                assert!(
                    is_selected(g, i, &d, &c),
                    "{g:?} {i} not selected after applying"
                );
            }
        }
    }

    #[test]
    fn styles_change_the_look_but_not_the_modules() {
        let c = ctx();
        let base = draft();
        for i in 0..STYLES.len() {
            let mut d = base.clone();
            assert!(
                apply(Gallery::Style, i, &mut d, &c),
                "style {i} changes something"
            );
            assert_eq!(
                d.bar.modules, base.bar.modules,
                "style {i} kept the modules"
            );
        }
        // Every style is distinct.
        let mut seen = Vec::new();
        for i in 0..STYLES.len() {
            let mut d = base.clone();
            apply(Gallery::Style, i, &mut d, &c);
            assert!(!seen.contains(&d), "style {i} duplicates another");
            seen.push(d);
        }
    }

    #[test]
    fn effects_toggle_and_palettes_keep_modules() {
        let c = ctx();
        let mut d = draft();
        assert!(apply(Gallery::Effect, 0, &mut d, &c));
        assert!(d.bar.style.border_bottom && is_selected(Gallery::Effect, 0, &d, &c));
        assert!(apply(Gallery::Effect, 0, &mut d, &c));
        assert!(!d.bar.style.border_bottom);
        assert!(
            !apply(Gallery::Effect, 6, &mut d, &c),
            "focus outline cannot be turned off"
        );
        let before = d.bar.modules.clone();
        apply(Gallery::Palette, 3, &mut d, &c);
        assert_eq!(d.bar.modules, before);
        assert_eq!(d.bar.style.accent.as_deref(), Some("#34D399"));
    }

    #[test]
    fn layouts_place_modules_and_minimal_turns_modules_off() {
        let c = ctx();
        let mut d = draft();
        apply(Gallery::Layout, 3, &mut d, &c);
        let right: Vec<ModuleKind> = d
            .bar
            .modules
            .iter()
            .filter(|m| m.region == Region::Right)
            .map(|m| m.kind)
            .collect();
        assert_eq!(
            right.last(),
            Some(&ModuleKind::Clock),
            "clock at the right end"
        );
        apply(Gallery::Layout, 8, &mut d, &c);
        let on: Vec<ModuleKind> = d
            .bar
            .modules
            .iter()
            .filter(|m| m.enabled)
            .map(|m| m.kind)
            .collect();
        assert_eq!(on.len(), 5);
        assert!(
            !d.bar.modules.is_empty(),
            "modules are turned off, not removed"
        );
    }

    #[test]
    fn palette_text_is_readable() {
        use windowsrice_theme::color::contrast_ratio;
        let col = |h: &str| {
            let (r, g, b) = windowsrice_core::bar::parse_hex_color(h).unwrap();
            Color::rgb(r, g, b)
        };
        for i in 0..PALETTES.len() {
            let [bg, mbg, _, text, text2] = palette_colors(i);
            for surface in [bg, mbg] {
                for t in [text, text2] {
                    let r = contrast_ratio(col(t), col(surface));
                    assert!(r >= 4.5, "palette {i}: {t} on {surface} is {r:.2}:1");
                }
            }
        }
    }
}
