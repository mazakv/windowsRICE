//! Visual gallery sections of the Design Studio (Phase 8).
//!
//! Every card shows the real thing: a miniature of the bar built by
//! [`build_bar`] on a copy of the draft with the card applied (resolved
//! through the same theme logic as the running bar), the actual gradient,
//! or text set in the actual font family. Cards are whole-card targets: a
//! `ListItem` with the SelectionItem pattern whose action applies the card.

use super::catalog::{self, Ctx, Gallery};
use super::*;

const GAP: f32 = 12.0;
/// Modules shown in miniature bars (enough to see the style at card size).
const SAMPLE: [ModuleKind; 7] = [
    ModuleKind::Menu,
    ModuleKind::ActiveWindow,
    ModuleKind::Clock,
    ModuleKind::Volume,
    ModuleKind::Cpu,
    ModuleKind::Ram,
    ModuleKind::Notifications,
];

/// Extra swatches offered by the colour editor (besides `SWATCHES`).
const MORE_SWATCHES: [&str; 14] = [
    "#000000", "#FFFFFF", "#0B1020", "#3D8BFF", "#4FB3FF", "#36D1C4", "#34D399", "#8CC83C",
    "#FFD400", "#FB923C", "#FB7185", "#FF2BD6", "#A78BFA", "#9CA3AF",
];

fn ctx<'a>(u: &Ui<'a>) -> Ctx<'a> {
    Ctx {
        background: Color {
            a: 255,
            ..u.p.style.surface
        },
        accent: u.p.style.accent,
        text: u.p.style.text_primary,
        fonts: &u.m.fonts,
    }
}

/// Cards of `n` items in columns at least `min_w` wide. Returns the card
/// rectangles and the bottom of the grid.
fn grid_rects(x: f32, y: f32, w: f32, n: usize, min_w: f32, h: f32) -> (usize, Vec<Rect>, f32) {
    let cols = (((w + GAP) / (min_w + GAP)).floor() as usize).clamp(1, n.max(1));
    let cw = (w - GAP * (cols as f32 - 1.0)) / cols as f32;
    let rects = (0..n)
        .map(|i| {
            Rect::new(
                x + (i % cols) as f32 * (cw + GAP),
                y + (i / cols) as f32 * (h + GAP),
                cw,
                h,
            )
        })
        .collect();
    let rows = n.div_ceil(cols).max(1);
    (cols, rects, y + rows as f32 * (h + GAP) - GAP)
}

pub(super) fn paint_backdrop(u: &mut Ui, r: Rect, owner: ElementId) {
    paint_backdrop_kind(u, r, owner, u.m.preview_backdrop);
}

fn paint_backdrop_kind(u: &mut Ui, r: Rect, owner: ElementId, kind: PreviewBackdrop) {
    match kind {
        PreviewBackdrop::Dark => u.fill(r, 6.0, Color::rgb(0x14, 0x17, 0x1D), owner),
        PreviewBackdrop::Light => u.fill(r, 6.0, Color::rgb(0xE9, 0xEC, 0xF1), owner),
        PreviewBackdrop::Colour => {
            if owner == HIDDEN || !u.visible(r) {
                return;
            }
            u.draw.push(DrawCmd::Gradient {
                rect: r,
                radius: 6.0,
                from: (0.0, 0.5),
                to: (1.0, 0.5),
                stops: vec![
                    (0.0, Color::rgb(0xFF, 0x3C, 0x8C)),
                    (0.33, Color::rgb(0xFF, 0xB7, 0x1C)),
                    (0.66, Color::rgb(0x1C, 0xC8, 0x8C)),
                    (1.0, Color::rgb(0x2A, 0x7C, 0xFF)),
                ],
                owner: Some(owner),
            });
        }
    }
}

/// A miniature of the bar for draft `d`, drawn into `r`.
fn mini_bar(u: &mut Ui, r: Rect, d: &Draft, owner: ElementId) {
    mini_bar_over(u, r, d, owner, u.m.preview_backdrop);
}

/// A miniature over a chosen backdrop (background cards always use the
/// colourful one, so translucency is visible).
fn mini_bar_over(u: &mut Ui, r: Rect, d: &Draft, owner: ElementId, backdrop: PreviewBackdrop) {
    if owner == HIDDEN || !u.visible(r) {
        return;
    }
    let Some((style, plan)) = (u.p.resolve)(d) else {
        u.text(
            r,
            t(u.lang, "Theme not available", "테마를 사용할 수 없음"),
            u.ts(11.0, 400),
            u.s.text2,
            owner,
        );
        return;
    };
    let modules: Vec<ModuleConfig> = d
        .bar
        .modules
        .iter()
        .filter(|m| SAMPLE.contains(&m.kind))
        .cloned()
        .collect();
    let bh = d.bar.height.map_or(style.height, f32::from);
    let zoom = ((r.h - 8.0) / bh).min(0.62).min(r.w / 760.0);
    let bw = r.w / zoom;
    let mut readings = u.m.readings.clone();
    readings.active_window = Some(t(u.lang, "Documents", "문서").to_owned());
    let bs = build_bar(
        &BarModel {
            modules,
            readings,
            decor: Decor::from_style(&d.bar.style),
            ..Default::default()
        },
        &style,
        &plan,
        u.measure,
        Size { w: bw, h: bh },
        1.0,
        u.strings,
        f32::from(d.bar.spacing),
    );
    u.draw.push(DrawCmd::PushClip { rect: r });
    paint_backdrop_kind(u, r, owner, backdrop);
    let y = r.y + (r.h - bh * zoom) / 2.0;
    u.draw
        .cmds
        .extend(transformed(&bs.scene.draw.cmds, r.x, y, zoom));
    u.draw.push(DrawCmd::PopClip);
}

/// A whole-card target with a picture, its name directly below and a
/// one-line description.
#[allow(clippy::too_many_arguments)]
fn card(
    u: &mut Ui,
    parent: ElementId,
    key: &str,
    r: Rect,
    name: &str,
    desc: &str,
    selected: bool,
    enabled: bool,
    a: StudioAction,
    thumb_h: f32,
    thumb: impl FnOnce(&mut Ui, Rect, ElementId),
) -> ElementId {
    let r = r.snap(u.scale);
    let full = if desc.is_empty() {
        name.to_owned()
    } else {
        format!("{name}. {desc}")
    };
    let id = u.add(
        parent,
        Element::new(key_id(key), intern(key), Role::ListItem, full, r)
            .focusable(true)
            .enabled(enabled)
            .selected(selected),
    );
    if id == HIDDEN {
        return id;
    }
    u.fill(r, 10.0, u.s.card, id);
    u.decorate(id, r, 10.0);
    let tr = Rect::new(r.x + 8.0, r.y + 8.0, r.w - 16.0, thumb_h);
    thumb(u, tr, id);
    if !enabled {
        u.fill(
            tr,
            6.0,
            Color {
                a: 0xB0,
                ..u.s.card
            },
            id,
        );
        u.text(
            Rect::new(tr.x + 10.0, tr.y, tr.w - 20.0, tr.h),
            t(u.lang, "Not available", "사용할 수 없음"),
            u.ts(12.0, 500),
            u.s.text2,
            id,
        );
    }
    let fg = if enabled { u.s.text } else { u.s.disabled };
    u.text(
        Rect::new(r.x + 12.0, tr.bottom() + 6.0, r.w - 24.0, 20.0),
        name,
        u.ts(13.0, 600),
        fg,
        id,
    );
    if !desc.is_empty() {
        u.text(
            Rect::new(r.x + 12.0, tr.bottom() + 26.0, r.w - 24.0, 16.0),
            desc,
            u.ts(11.0, 400),
            u.s.text2,
            id,
        );
    }
    if selected {
        u.stroke(r.inflate(-1.0), 10.0, 2.0, u.s.accent, id);
        let b = Rect::new(tr.right() - 26.0, tr.y + 6.0, 20.0, 20.0);
        u.fill(b, 10.0, u.s.accent, id);
        u.text(
            Rect::new(b.x + 4.0, b.y, 14.0, b.h),
            "\u{E73E}",
            u.icon(11.0),
            u.s.on_accent,
            id,
        );
    } else {
        u.stroke(r.inflate(-0.5), 10.0, 1.0, u.s.border, id);
    }
    u.actions.push((id, a));
    id
}

/// Section heading inside the scrolled area (key must be unique).
fn subheading(u: &mut Ui, g: ElementId, key: &str, x: f32, y: f32, w: f32, text: &str) -> f32 {
    u.label(g, key, Rect::new(x, y, w, 26.0), text, 14.0, 600, u.s.text);
    y + 32.0
}

/// A gallery of catalog cards with miniature bars.
fn catalog_grid(u: &mut Ui, g: ElementId, x: f32, y: f32, w: f32, gal: Gallery, min_w: f32) -> f32 {
    let items = catalog::items(gal);
    let c = ctx(u);
    let (cols, rects, bottom) = grid_rects(x, y, w, items.len(), min_w, 110.0);
    let prefix = format!("card:{}", gal.key());
    let lang = u.lang;
    for (i, (it, r)) in items.iter().zip(&rects).enumerate() {
        let mut preview = u.m.draft.clone();
        catalog::apply(gal, i, &mut preview, &c);
        let enabled = catalog::available(gal, i, &c) && !(gal == Gallery::Effect && i == 6);
        let selected = catalog::is_selected(gal, i, &u.m.draft, &c);
        card(
            u,
            g,
            &format!("{prefix}:{i}"),
            *r,
            t(lang, it.en, it.ko),
            t(lang, it.desc_en, it.desc_ko),
            selected,
            enabled,
            StudioAction::Card(gal, i),
            48.0,
            |u, tr, id| {
                if gal == Gallery::Palette {
                    let cols = catalog::palette_colors(i);
                    let sw = tr.w / cols.len() as f32;
                    for (k, h) in cols.iter().enumerate() {
                        if let Some(col) = hex_color(h) {
                            u.fill(
                                Rect::new(tr.x + k as f32 * sw, tr.y, sw, 12.0),
                                0.0,
                                col,
                                id,
                            );
                        }
                    }
                    mini_bar(
                        u,
                        Rect::new(tr.x, tr.y + 14.0, tr.w, tr.h - 14.0),
                        &preview,
                        id,
                    );
                } else if matches!(gal, Gallery::Background | Gallery::Gradient) {
                    mini_bar_over(u, tr, &preview, id, PreviewBackdrop::Colour);
                } else {
                    mini_bar(u, tr, &preview, id);
                }
            },
        );
    }
    u.grids.push(GridInfo {
        prefix,
        cols,
        rects,
    });
    bottom
}

impl Ui<'_> {
    /// A labelled slider: drag or click the track, or use the arrow keys.
    fn slider(&mut self, parent: ElementId, kind: SliderKind, r: Rect) -> f32 {
        let v = kind.get(&self.m.draft, &self.m.theme_defaults);
        let (lo, hi, _) = kind.range();
        let lang = self.lang;
        let name = kind.name(lang);
        let vt = kind.text(v);
        let key = format!("slider:{kind:?}");
        let track = Rect::new(r.x, r.y + 22.0, r.w, 22.0).snap(self.scale);
        let id = self.add(
            parent,
            Element::new(key_id(&key), intern(&key), Role::Slider, name, track)
                .focusable(true)
                .value(vt.clone())
                .help(t(
                    lang,
                    "Left and Right arrows change the value",
                    "왼쪽/오른쪽 화살표로 값 변경",
                )),
        );
        self.text(
            Rect::new(r.x, r.y, r.w - 60.0, 20.0),
            name,
            self.ts(12.0, 400),
            self.s.text,
            id,
        );
        self.text(
            Rect::new(r.right() - 56.0, r.y, 56.0, 20.0),
            &vt,
            self.ts(12.0, 500),
            self.s.text2,
            id,
        );
        let f = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
        let line = Rect::new(track.x + 8.0, track.center_y() - 2.0, track.w - 16.0, 4.0);
        self.fill(line, 2.0, self.s.control, id);
        self.fill(
            Rect::new(line.x, line.y, line.w * f, line.h),
            2.0,
            self.s.accent,
            id,
        );
        let kx = line.x + line.w * f;
        self.decorate(id, Rect::new(kx - 10.0, track.y, 20.0, track.h), 10.0);
        self.fill(
            Rect::new(kx - 8.0, track.center_y() - 8.0, 16.0, 16.0),
            8.0,
            self.s.accent,
            id,
        );
        self.fill(
            Rect::new(kx - 4.0, track.center_y() - 4.0, 8.0, 8.0),
            4.0,
            self.s.on_accent,
            id,
        );
        if id != HIDDEN {
            self.sliders.push(SliderInfo {
                element: id,
                track: line,
                kind,
            });
            self.fields.push((key, id));
            self.actions.push((id, StudioAction::Slider(kind, v)));
        }
        r.y + 52.0
    }
}

/// Sliders in two columns. Returns the bottom.
fn sliders(u: &mut Ui, g: ElementId, x: f32, y: f32, w: f32, kinds: &[SliderKind]) -> f32 {
    let half = (w - 24.0) / 2.0;
    let mut bottom = y;
    for (i, k) in kinds.iter().enumerate() {
        let cx = if i % 2 == 0 { x } else { x + half + 24.0 };
        let cy = y + (i / 2) as f32 * 56.0;
        bottom = u.slider(g, *k, Rect::new(cx, cy, half, 48.0)).max(bottom);
    }
    bottom
}

fn note(u: &mut Ui, g: ElementId, key: &str, x: f32, y: f32, w: f32, text: &str) -> f32 {
    u.label(g, key, Rect::new(x, y, w, 40.0), text, 12.0, 400, u.s.text2);
    y + 44.0
}

// ------------------------------------------------------------------ sections

pub(super) fn home(u: &mut Ui, g: ElementId, r: Rect) {
    let lang = u.lang;
    let m = u.m;
    let mut y = section_title(
        u,
        g,
        r,
        t(
            lang,
            "windowsRICE Design Studio",
            "windowsRICE 디자인 스튜디오",
        ),
        t(
            lang,
            "Pick a look, then fine-tune colours, fonts, icons, layout and effects. The preview above is the real bar; Apply updates the running bar.",
            "디자인을 고른 뒤 색, 글꼴, 아이콘, 배치, 효과를 다듬으세요. 위 미리 보기는 실제 바이며, 적용을 누르면 실행 중인 바가 바뀝니다.",
        ),
    );
    let c = ctx(u);
    let style = (0..catalog::STYLES.len())
        .find(|i| catalog::is_selected(Gallery::Style, *i, &m.draft, &c))
        .map_or_else(
            || t(lang, "Custom", "사용자 지정").to_owned(),
            |i| t(lang, catalog::STYLES[i].en, catalog::STYLES[i].ko).to_owned(),
        );
    let theme = m
        .themes
        .iter()
        .find(|e| e.id == m.draft.theme.active)
        .map_or(m.draft.theme.active.clone(), |e| e.name.clone());
    let font = m
        .draft
        .bar
        .style
        .font_family
        .clone()
        .unwrap_or_else(|| m.theme_font.clone());
    let summary = match lang {
        Lang::En => format!("Style: {style} · Theme: {theme} · Font: {font}"),
        Lang::Ko => format!("스타일: {style} · 테마: {theme} · 글꼴: {font}"),
    };
    u.label(
        g,
        "home:summary",
        Rect::new(r.x, y, r.w, 24.0),
        &summary,
        13.0,
        500,
        u.s.text,
    );
    y += 36.0;
    let secs: Vec<Section> = Section::ALL
        .iter()
        .copied()
        .filter(|s| *s != Section::Home)
        .collect();
    let (cols, rects, bottom) = grid_rects(r.x, y, r.w, secs.len(), 230.0, 72.0);
    for (i, (sec, rr)) in secs.iter().zip(&rects).enumerate() {
        let key = format!("home:{i}");
        let rr = rr.snap(u.scale);
        let id = u.add(
            g,
            Element::new(
                key_id(&key),
                intern(&key),
                Role::Button,
                format!("{}. {}", sec.name(lang), sec.hint(lang)),
                rr,
            )
            .focusable(true)
            .invokable(),
        );
        u.fill(rr, 10.0, u.s.card, id);
        u.stroke(rr.inflate(-0.5), 10.0, 1.0, u.s.border, id);
        u.decorate(id, rr, 10.0);
        u.text(
            Rect::new(rr.x + 16.0, rr.y, 24.0, rr.h),
            sec.glyph(),
            u.icon(20.0),
            u.s.accent,
            id,
        );
        u.text(
            Rect::new(rr.x + 52.0, rr.y + 14.0, rr.w - 60.0, 22.0),
            sec.name(lang),
            u.ts(14.0, 600),
            u.s.text,
            id,
        );
        u.text(
            Rect::new(rr.x + 52.0, rr.y + 38.0, rr.w - 60.0, 18.0),
            sec.hint(lang),
            u.ts(11.0, 400),
            u.s.text2,
            id,
        );
        u.actions.push((id, StudioAction::Nav(*sec)));
    }
    u.grids.push(GridInfo {
        prefix: "home".into(),
        cols,
        rects,
    });
    y = bottom + 20.0;
    for (i, (l, col)) in m
        .blocking
        .iter()
        .map(|b| (b, u.s.danger))
        .chain(m.warnings.iter().map(|w| (w, u.s.warning)))
        .take(4)
        .enumerate()
    {
        u.label(
            g,
            &format!("home:warn{i}"),
            Rect::new(r.x, y, r.w, 40.0),
            l,
            12.0,
            400,
            col,
        );
        y += 44.0;
    }
}

pub(super) fn styles(u: &mut Ui, g: ElementId, r: Rect) {
    let lang = u.lang;
    let y = section_title(
        u,
        g,
        r,
        t(lang, "Templates", "템플릿"),
        t(
            lang,
            "Complete designs to start from: theme, colours, type, module tiles, icons, effects and module layout together. Each module keeps its own settings; Undo returns to your design. Customise anything afterwards, or start from scratch.",
            "시작점이 되는 완성된 디자인입니다: 테마, 색, 글꼴, 모듈 타일, 아이콘, 효과, 모듈 배치를 한 번에 바꿉니다. 각 모듈의 설정은 유지되며 실행 취소로 되돌릴 수 있습니다. 고른 뒤 무엇이든 다듬거나 처음부터 만드세요.",
        ),
    );
    let y = creator_row(u, g, r, y, false);
    catalog_grid(u, g, r.x, y, r.w, Gallery::Style, 270.0);
}

pub(super) fn themes(u: &mut Ui, g: ElementId, r: Rect) {
    let lang = u.lang;
    let m = u.m;
    let mut y = section_title(
        u,
        g,
        r,
        t(lang, "Themes", "테마"),
        t(
            lang,
            "Choosing a theme shows its own colours: colour overrides are cleared (Undo restores them). Duplicate a theme to edit, rename or delete the copy.",
            "테마를 고르면 테마 자체의 색이 보이도록 색 재정의를 지웁니다 (실행 취소로 복원). 테마를 복제하면 사본을 편집, 이름 변경, 삭제할 수 있습니다.",
        ),
    );
    let featured: [(&str, ThemeModePreference, &str, &str); 4] = [
        (
            "windowsrice-light",
            ThemeModePreference::Light,
            "Light",
            "라이트",
        ),
        (
            "windowsrice-default",
            ThemeModePreference::Dark,
            "Dark",
            "다크",
        ),
        (
            "windowsrice-glass",
            ThemeModePreference::Light,
            "Glass Light",
            "글래스 라이트",
        ),
        (
            "windowsrice-glass",
            ThemeModePreference::Dark,
            "Glass Dark",
            "글래스 다크",
        ),
    ];
    u.button(
        g,
        "themes:new",
        Rect::new(r.x, y, 300.0_f32.min(r.w), 34.0),
        Some("\u{E710}"),
        t(lang, "New theme from this design", "이 디자인으로 새 테마"),
        t(
            lang,
            "Create a custom theme from the colours of the current design",
            "현재 디자인의 색으로 사용자 테마 만들기",
        ),
        Kind::Secondary,
        true,
        StudioAction::NewTheme,
    );
    y += 46.0;
    y = subheading(
        u,
        g,
        "themes:featured",
        r.x,
        y,
        r.w,
        t(lang, "Essentials", "기본"),
    );
    let (cols, rects, bottom) = grid_rects(r.x, y, r.w, featured.len(), 220.0, 110.0);
    for (i, ((id, mode, en, ko), rr)) in featured.iter().zip(&rects).enumerate() {
        let avail = m.themes.iter().any(|e| e.id == *id);
        let mut d = m.draft.clone();
        apply_theme_card(&mut d, id, *mode);
        let selected = m.draft.theme.active == *id && m.draft.theme.mode == *mode;
        card(
            u,
            g,
            &format!("theme-f:{i}"),
            *rr,
            t(lang, en, ko),
            "",
            selected,
            avail,
            StudioAction::ThemeCard((*id).to_owned(), *mode),
            48.0,
            |u, tr, cid| mini_bar(u, tr, &d, cid),
        );
    }
    u.grids.push(GridInfo {
        prefix: "theme-f".into(),
        cols,
        rects,
    });
    y = subheading(
        u,
        g,
        "themes:all",
        r.x,
        bottom + 16.0,
        r.w,
        t(lang, "All themes", "모든 테마"),
    );
    let list = m.themes.clone();
    let (cols, rects, _) = grid_rects(r.x, y, r.w, list.len(), 220.0, 110.0);
    for (i, (e, rr)) in list.iter().zip(&rects).enumerate() {
        let mode = if e.has_dark && e.has_light {
            m.draft.theme.mode
        } else if e.has_dark {
            ThemeModePreference::Dark
        } else {
            ThemeModePreference::Light
        };
        let mut d = m.draft.clone();
        apply_theme_card(&mut d, &e.id, mode);
        let kind = match (lang, e.builtin, e.has_dark && e.has_light) {
            (Lang::En, true, true) => "Built-in · dark and light",
            (Lang::En, true, false) => "Built-in",
            (Lang::En, false, _) => "Custom · editable",
            (Lang::Ko, true, true) => "기본 · 어둡게와 밝게",
            (Lang::Ko, true, false) => "기본",
            (Lang::Ko, false, _) => "사용자 · 편집 가능",
        };
        card(
            u,
            g,
            &format!("theme:{i}"),
            *rr,
            &e.name,
            kind,
            m.draft.theme.active == e.id,
            true,
            StudioAction::ThemeCard(e.id.clone(), mode),
            48.0,
            |u, tr, cid| mini_bar(u, tr, &d, cid),
        );
    }
    u.grids.push(GridInfo {
        prefix: "theme".into(),
        cols,
        rects,
    });
}

/// Effective colour of a role (the override or the theme's).
fn effective(u: &Ui, ct: ColorTarget) -> Color {
    if let Some(c) = ct.get(&u.m.draft.bar.style).as_deref().and_then(hex_color) {
        return c;
    }
    let p = u.p.style;
    let opaque = |c: Color| {
        c.over(Color {
            a: 255,
            ..p.surface
        })
    };
    match ct {
        ColorTarget::Background | ColorTarget::GradientStop(_) => Color {
            a: 255,
            ..p.surface
        },
        ColorTarget::ModuleBackground => opaque(Color {
            a: 0x1A,
            ..p.text_primary
        }),
        ColorTarget::Accent => p.accent,
        ColorTarget::Text | ColorTarget::Clock | ColorTarget::Values => p.text_primary,
        ColorTarget::Icons => match u.m.draft.bar.style.icon_tint {
            IconTint::Accent => p.accent,
            IconTint::Secondary => p.text_secondary,
            _ => p.text_primary,
        },
        ColorTarget::SecondaryText | ColorTarget::Separator => p.text_secondary,
        ColorTarget::Border => opaque(p.border),
        ColorTarget::Hover => opaque(p.hover),
        ColorTarget::Selected => opaque(p.pressed),
        ColorTarget::Focus => p.focus_ring,
    }
}

pub(super) fn colors(u: &mut Ui, g: ElementId, r: Rect) {
    let lang = u.lang;
    let m = u.m;
    let mut y = section_title(
        u,
        g,
        r,
        t(lang, "Colours", "색"),
        t(
            lang,
            "Choose a palette, or pick a colour role below and edit it on the right. Unreadable combinations are refused.",
            "팔레트를 고르거나, 아래에서 색 역할을 골라 오른쪽에서 편집하세요. 읽기 어려운 조합은 거부됩니다.",
        ),
    );
    y = subheading(
        u,
        g,
        "colors:palettes",
        r.x,
        y,
        r.w,
        t(lang, "Palettes", "팔레트"),
    );
    y = catalog_grid(u, g, r.x, y, r.w, Gallery::Palette, 230.0) + 18.0;
    y = subheading(
        u,
        g,
        "colors:roles",
        r.x,
        y,
        r.w,
        t(lang, "Colour roles", "색 역할"),
    );
    let mut roles: Vec<ColorTarget> = ColorTarget::ROLES.to_vec();
    if let Some(gr) = &m.draft.bar.style.gradient {
        for i in 0..gr.colors.len().min(4) {
            roles.push(ColorTarget::GradientStop(i as u8));
        }
    }
    let (cols, rects, _) = grid_rects(r.x, y, r.w, roles.len(), 210.0, 58.0);
    for (i, (ct, rr)) in roles.iter().zip(&rects).enumerate() {
        let key = format!("role:{i}");
        let rr = rr.snap(u.scale);
        let sel = m.color_target == *ct;
        let value = ct.get(&m.draft.bar.style);
        let vtext = value
            .clone()
            .unwrap_or_else(|| t(lang, "Theme colour", "테마 색").to_owned());
        let id = u.add(
            g,
            Element::new(
                key_id(&key),
                intern(&key),
                Role::ListItem,
                format!("{}: {vtext}. {}", ct.name(lang), ct.hint(lang)),
                rr,
            )
            .focusable(true)
            .selected(sel),
        );
        u.fill(rr, 10.0, if sel { u.s.selection } else { u.s.card }, id);
        u.stroke(
            rr.inflate(-0.5),
            10.0,
            if sel { 2.0 } else { 1.0 },
            if sel { u.s.accent } else { u.s.border },
            id,
        );
        u.decorate(id, rr, 10.0);
        let sw = Rect::new(rr.x + 12.0, rr.y + 13.0, 32.0, 32.0);
        let c = effective(u, *ct);
        u.fill(sw, 16.0, c, id);
        u.stroke(sw.inflate(-0.5), 16.0, 1.0, u.s.control, id);
        u.text(
            Rect::new(rr.x + 54.0, rr.y + 10.0, rr.w - 60.0, 20.0),
            &ct.name(lang),
            u.ts(13.0, 500),
            u.s.text,
            id,
        );
        u.text(
            Rect::new(rr.x + 54.0, rr.y + 30.0, rr.w - 60.0, 18.0),
            &vtext,
            u.ts(11.0, 400),
            u.s.text2,
            id,
        );
        u.actions.push((id, StudioAction::ColorTarget(*ct)));
    }
    u.grids.push(GridInfo {
        prefix: "role".into(),
        cols,
        rects,
    });
}

pub(super) fn background(u: &mut Ui, g: ElementId, r: Rect) {
    let lang = u.lang;
    let m = u.m;
    let mut y = section_title(
        u,
        g,
        r,
        t(lang, "Background", "배경"),
        t(
            lang,
            "Translucent backgrounds show what is behind the bar, without blur: Windows does not blur a bar that never takes focus. Text keeps readable backplates when needed.",
            "반투명 배경은 바 뒤를 흐림 없이 보여 줍니다. Windows는 포커스를 받지 않는 바를 흐리게 하지 않습니다. 필요하면 글자 배경판으로 가독성을 지킵니다.",
        ),
    );
    y = catalog_grid(u, g, r.x, y, r.w, Gallery::Background, 230.0) + 18.0;
    y = subheading(
        u,
        g,
        "bg:gradients",
        r.x,
        y,
        r.w,
        t(lang, "Gradients", "그라데이션"),
    );
    y = catalog_grid(u, g, r.x, y, r.w, Gallery::Gradient, 230.0) + 14.0;
    if m.draft.bar.style.gradient.is_some() {
        use windowsrice_core::bar::GradientDirection as D;
        let w3 = (r.w - 16.0) / 3.0;
        for (i, (dir, en, ko)) in [
            (D::Horizontal, "Horizontal", "가로"),
            (D::Vertical, "Vertical", "세로"),
            (D::Diagonal, "Diagonal", "대각선"),
        ]
        .into_iter()
        .enumerate()
        {
            let sel = m
                .draft
                .bar
                .style
                .gradient
                .as_ref()
                .is_some_and(|gr| gr.direction == dir);
            u.radio(
                g,
                &format!("bg:dir:{i}"),
                Rect::new(r.x + i as f32 * (w3 + 8.0), y, w3, ROW),
                t(lang, en, ko),
                sel,
                StudioAction::GradientDirection(dir),
            );
        }
        y += ROW + 6.0;
        y = note(
            u,
            g,
            "bg:gradnote",
            r.x,
            y,
            r.w,
            t(
                lang,
                "Edit each gradient colour under Colours › Colour roles.",
                "그라데이션 색은 색 › 색 역할에서 하나씩 편집합니다.",
            ),
        );
    }
    y = subheading(
        u,
        g,
        "bg:transparency",
        r.x,
        y + 6.0,
        r.w,
        t(lang, "Transparency and readability", "투명도와 가독성"),
    );
    y = sliders(u, g, r.x, y, r.w, &[SliderKind::Opacity]) + 4.0;
    let w3 = (r.w - 16.0) / 3.0;
    for (i, (p, en, ko)) in [
        (
            BackplatePolicy::Auto,
            "Backplates when needed",
            "필요할 때 배경판",
        ),
        (BackplatePolicy::Always, "Always backplates", "항상 배경판"),
        (BackplatePolicy::Never, "No backplates", "배경판 없음"),
    ]
    .into_iter()
    .enumerate()
    {
        u.radio(
            g,
            &format!("bg:bp:{i}"),
            Rect::new(r.x + i as f32 * (w3 + 8.0), y, w3, ROW),
            t(lang, en, ko),
            m.draft.bar.style.backplates == p,
            StudioAction::Backplates(p),
        );
    }
    y += ROW + 8.0;
    u.toggle(
        g,
        "bg:border",
        Rect::new(r.x, y, r.w.min(420.0), ROW),
        t(lang, "Bottom border", "아래 테두리"),
        m.draft.bar.style.border_bottom,
        true,
        None,
        StudioAction::ToggleBorder,
    );
}

pub(super) fn typography(u: &mut Ui, g: ElementId, r: Rect) {
    let lang = u.lang;
    let m = u.m;
    let mut y = section_title(
        u,
        g,
        r,
        t(lang, "Typography", "글꼴"),
        t(
            lang,
            "Every sample is set in its own font. Fonts without Hangul show only Latin text: Korean then uses Malgun Gothic.",
            "모든 견본은 해당 글꼴로 그립니다. 한글이 없는 글꼴은 영문만 보여 주며, 한글은 맑은 고딕으로 표시됩니다.",
        ),
    );
    // Typography presets: a real sample in each preset's family and size.
    y = subheading(
        u,
        g,
        "typo:presets",
        r.x,
        y,
        r.w,
        t(lang, "Typography presets", "글꼴 프리셋"),
    );
    let c = ctx(u);
    let items = catalog::TYPOGRAPHY;
    let (cols, rects, bottom) = grid_rects(r.x, y, r.w, items.len(), 210.0, 100.0);
    for (i, (it, rr)) in items.iter().zip(&rects).enumerate() {
        let family = catalog::typo_family(i).map_or_else(|| m.theme_font.clone(), str::to_owned);
        let mut d = m.draft.clone();
        catalog::apply(Gallery::Typography, i, &mut d, &c);
        let size = d.bar.style.font_size.map_or(13.0, f32::from);
        let weight = d.bar.style.font_weight.unwrap_or(400);
        let clock = d.bar.style.clock_size.map_or(size, f32::from);
        let fam = intern(&family);
        card(
            u,
            g,
            &format!("card:typo:{i}"),
            *rr,
            t(lang, it.en, it.ko),
            t(lang, it.desc_en, it.desc_ko),
            catalog::is_selected(Gallery::Typography, i, &m.draft, &c),
            catalog::available(Gallery::Typography, i, &c),
            StudioAction::Card(Gallery::Typography, i),
            40.0,
            |u, tr, id| {
                u.fill(tr, 6.0, u.s.background, id);
                let st = TextStyle {
                    font: FontRole::Family(fam),
                    size: clock,
                    weight: weight.clamp(500, 600),
                };
                let tw = u.width("12:45", &st);
                u.text(
                    Rect::new(tr.x + 8.0, tr.y, tw + 4.0, tr.h),
                    "12:45",
                    st,
                    u.s.text,
                    id,
                );
                u.text(
                    Rect::new(tr.x + 16.0 + tw, tr.y, tr.w - tw - 24.0, tr.h),
                    "CPU 12%  가나다",
                    TextStyle {
                        font: FontRole::Family(fam),
                        size,
                        weight,
                    },
                    u.s.text2,
                    id,
                );
            },
        );
    }
    u.grids.push(GridInfo {
        prefix: "card:typo".into(),
        cols,
        rects,
    });
    y = bottom + 18.0;
    // Font gallery.
    y = subheading(
        u,
        g,
        "typo:fonts",
        r.x,
        y,
        r.w,
        t(lang, "Installed fonts", "설치된 글꼴"),
    );
    let query = m
        .text_edit
        .as_ref()
        .filter(|e| e.key == "field:font-search")
        .map_or(m.font_query.clone(), |e| e.buffer.clone())
        .to_lowercase();
    let list: Vec<&FontEntry> = m
        .fonts
        .iter()
        .filter(|f| query.is_empty() || f.family.to_lowercase().contains(&query))
        .filter(|f| match m.font_filter {
            FontFilter::All => true,
            FontFilter::Korean => f.hangul,
            FontFilter::Monospace => f.mono,
        })
        .collect();
    u.field(
        g,
        "field:font-search",
        Rect::new(r.x, y, (r.w * 0.5).max(240.0), ROW),
        t(lang, "Search fonts", "글꼴 검색"),
        &m.font_query,
        t(lang, "Search by name…", "이름으로 검색…"),
    );
    let count = match lang {
        Lang::En => format!("{} of {} fonts", list.len(), m.fonts.len()),
        Lang::Ko => format!("글꼴 {}개 중 {}개", m.fonts.len(), list.len()),
    };
    u.label(
        g,
        "typo:count",
        Rect::new(r.x + (r.w * 0.5).max(240.0) + 12.0, y, 200.0, ROW),
        &count,
        12.0,
        400,
        u.s.text2,
    );
    y += ROW + 8.0;
    let w4 = ((r.w - 24.0) / 4.0).min(170.0);
    for (i, (ff, en, ko)) in [
        (FontFilter::All, "All fonts", "모든 글꼴"),
        (FontFilter::Korean, "Korean supported", "한글 지원"),
        (FontFilter::Monospace, "Fixed width", "고정 폭"),
    ]
    .into_iter()
    .enumerate()
    {
        u.radio(
            g,
            &format!("typo:filter:{i}"),
            Rect::new(r.x + i as f32 * (w4 + 8.0), y, w4, ROW),
            t(lang, en, ko),
            m.font_filter == ff,
            StudioAction::FontFilter(ff),
        );
    }
    y += ROW + 12.0;
    let current = m
        .draft
        .bar
        .style
        .font_family
        .clone()
        .unwrap_or_else(|| m.theme_font.clone());
    let (cols, rects, _) = grid_rects(r.x, y, r.w, list.len(), 220.0, 128.0);
    for (i, (f, rr)) in list.iter().zip(&rects).enumerate() {
        if !u.visible(*rr) {
            continue;
        }
        let fam = intern(&f.family);
        let desc = match (lang, f.hangul) {
            (Lang::En, true) => format!("Korean supported · {}", weights_en(f.weights.len())),
            (Lang::En, false) => format!("Latin only · {}", weights_en(f.weights.len())),
            (Lang::Ko, true) => format!("한글 지원 · 굵기 {}개", f.weights.len()),
            (Lang::Ko, false) => format!("영문 전용 · 굵기 {}개", f.weights.len()),
        };
        let hangul = f.hangul;
        card(
            u,
            g,
            &format!("font:{i}"),
            *rr,
            &f.family,
            &desc,
            current == f.family,
            true,
            StudioAction::Font(Some(f.family.clone())),
            70.0,
            |u, tr, id| {
                u.fill(tr, 6.0, u.s.background, id);
                u.text(
                    Rect::new(tr.x + 10.0, tr.y + 2.0, tr.w - 20.0, 40.0),
                    "Aa Bb 123",
                    TextStyle {
                        font: FontRole::Family(fam),
                        size: 24.0,
                        weight: 400,
                    },
                    u.s.text,
                    id,
                );
                if hangul {
                    u.text(
                        Rect::new(tr.x + 10.0, tr.y + 40.0, tr.w - 20.0, 26.0),
                        "가나다 한글 0123",
                        TextStyle {
                            font: FontRole::Family(fam),
                            size: 16.0,
                            weight: 400,
                        },
                        u.s.text2,
                        id,
                    );
                } else {
                    u.text(
                        Rect::new(tr.x + 10.0, tr.y + 40.0, tr.w - 20.0, 26.0),
                        t(
                            lang,
                            "No Hangul: Korean uses Malgun Gothic",
                            "한글 없음: 한글은 맑은 고딕으로 표시",
                        ),
                        u.ts(11.0, 400),
                        u.s.warning,
                        id,
                    );
                }
            },
        );
    }
    // Keep the grid's full height for scrolling.
    if let Some(last) = rects.last() {
        u.content_bottom = u.content_bottom.max(last.bottom());
    }
    u.grids.push(GridInfo {
        prefix: "font".into(),
        cols,
        rects,
    });
}

pub(super) fn icons(u: &mut Ui, g: ElementId, r: Rect) {
    let lang = u.lang;
    let mut y = section_title(
        u,
        g,
        r,
        t(lang, "Icons", "아이콘"),
        t(
            lang,
            "Icon fonts that ship with Windows, in the colour you choose. Which modules show icons is set under Modules › Module appearance.",
            "Windows에 포함된 아이콘 글꼴을 원하는 색으로 씁니다. 아이콘을 표시할 모듈은 모듈 › 모듈 모양에서 정합니다.",
        ),
    );
    y = catalog_grid(u, g, r.x, y, r.w, Gallery::Icons, 230.0) + 18.0;
    y = sliders(u, g, r.x, y, r.w, &[SliderKind::IconSize]);
    u.button(
        g,
        "icons:color",
        Rect::new(r.x, y + 4.0, 220.0, 34.0),
        Some("\u{E2B1}"),
        t(lang, "Icon colour…", "아이콘 색…"),
        t(lang, "Choose a custom icon colour", "아이콘 색 직접 고르기"),
        Kind::Secondary,
        true,
        StudioAction::GoColor(ColorTarget::Icons),
    );
    note(
        u,
        g,
        "icons:note",
        r.x,
        y + 48.0,
        r.w,
        t(
            lang,
            "Other icon families (duotone, geometric and so on) are not offered: they are not installed with Windows.",
            "다른 아이콘 계열(듀오톤, 기하학 등)은 Windows에 설치되어 있지 않아 제공하지 않습니다.",
        ),
    );
}

pub(super) fn modules(u: &mut Ui, g: ElementId, r: Rect) {
    let lang = u.lang;
    let mut y = r.y;
    u.label(
        g,
        "looks:title",
        Rect::new(r.x, y, r.w, 28.0),
        t(lang, "Module appearance", "모듈 모양"),
        17.0,
        600,
        u.s.text,
    );
    u.label(
        g,
        "looks:hint",
        Rect::new(r.x, y + 28.0, r.w, 22.0),
        t(
            lang,
            "What each module shows and how it sits on the bar.",
            "각 모듈에 표시할 내용과 바 위 모양.",
        ),
        12.0,
        400,
        u.s.text2,
    );
    y += 60.0;
    y = catalog_grid(u, g, r.x, y, r.w, Gallery::ModuleLook, 220.0) + 24.0;
    let editor_h = r.h.max(520.0);
    super::modules(u, g, Rect::new(r.x, y, r.w, editor_h));
    module_look(u, g, r.x, y + editor_h + 24.0, r.w);
}

pub(super) fn layout(u: &mut Ui, g: ElementId, r: Rect) {
    let lang = u.lang;
    let m = u.m;
    let mut y = section_title(
        u,
        g,
        r,
        t(lang, "Layout and spacing", "레이아웃과 간격"),
        t(
            lang,
            "Arrangements move modules between the left, centre and right groups; your own order is kept within a group where possible.",
            "배치는 모듈을 왼쪽, 가운데, 오른쪽 그룹으로 옮깁니다. 그룹 안의 순서는 가능한 한 유지합니다.",
        ),
    );
    // Top and bottom bars (Phase 14): every page edits the bar chosen here.
    let d = &m.draft;
    let bottom_on = d.bottom().is_some_and(|b| b.enabled);
    let half = ((r.w - 16.0) / 2.0).min(300.0);
    u.radio(
        g,
        "bars:top",
        Rect::new(r.x, y, half, ROW),
        t(lang, "Edit the top bar", "위쪽 바 편집"),
        !d.editing_bottom,
        StudioAction::EditBottomBar(false),
    );
    u.radio(
        g,
        "bars:bottom",
        Rect::new(r.x + half + 16.0, y, half, ROW),
        t(lang, "Edit the bottom bar", "아래쪽 바 편집"),
        d.editing_bottom,
        StudioAction::EditBottomBar(true),
    );
    y += ROW + 6.0;
    u.toggle(
        g,
        "bars:show-bottom",
        Rect::new(r.x, y, (r.w - 16.0) / 2.0, ROW),
        t(lang, "Show the bottom bar", "아래쪽 바 표시"),
        bottom_on,
        true,
        Some(t(
            lang,
            "A second bar at the bottom of the screen, with its own modules and look. It sits above the Windows taskbar and never replaces it.",
            "화면 아래쪽의 두 번째 바로, 모듈과 모양을 따로 정합니다. Windows 작업 표시줄 위에 놓이며 이를 대신하지 않습니다.",
        )),
        StudioAction::ToggleBottomBar,
    );
    y += ROW + 18.0;
    y = catalog_grid(u, g, r.x, y, r.w, Gallery::Layout, 250.0) + 18.0;
    y = subheading(
        u,
        g,
        "lay:sizes",
        r.x,
        y,
        r.w,
        t(lang, "Size and spacing", "크기와 간격"),
    );
    y = sliders(
        u,
        g,
        r.x,
        y,
        r.w,
        &[
            SliderKind::Height,
            SliderKind::Spacing,
            SliderKind::Padding,
            SliderKind::RegionGap,
        ],
    ) + 4.0;
    let w3 = (r.w - 16.0) / 3.0;
    for (i, (v, en, ko)) in [
        (VerticalAlign::Center, "Centre", "가운데"),
        (VerticalAlign::Top, "Top", "위"),
        (VerticalAlign::Bottom, "Bottom", "아래"),
    ]
    .into_iter()
    .enumerate()
    {
        u.radio(
            g,
            &format!("lay:valign:{i}"),
            Rect::new(r.x + i as f32 * (w3 + 8.0), y, w3, ROW),
            &match lang {
                Lang::En => format!("Align {en}"),
                Lang::Ko => format!("{ko} 정렬"),
            },
            m.draft.bar.style.vertical_align == v,
            StudioAction::VAlign(v),
        );
    }
    y += ROW + 8.0;
    note(
        u,
        g,
        "lay:overflow",
        r.x,
        y,
        r.w,
        t(
            lang,
            "When modules do not fit, the window title shortens first, then modules hide from the end of the right group and a +N item names them. Per-module width limits are under Modules.",
            "모듈이 넘치면 창 제목을 먼저 줄이고, 그다음 오른쪽 그룹 끝의 모듈을 숨기고 +N 항목으로 알려 줍니다. 모듈별 최대 너비는 모듈에서 정합니다.",
        ),
    );
}

pub(super) fn effects(u: &mut Ui, g: ElementId, r: Rect) {
    let lang = u.lang;
    let mut y = section_title(
        u,
        g,
        r,
        t(lang, "Effects and details", "효과와 세부"),
        t(
            lang,
            "Each card turns an effect on or off. Only effects the bar can really draw are offered.",
            "카드마다 효과를 켜고 끕니다. 바가 실제로 그릴 수 있는 효과만 제공합니다.",
        ),
    );
    y = catalog_grid(u, g, r.x, y, r.w, Gallery::Effect, 220.0) + 18.0;
    y = subheading(u, g, "fx:amounts", r.x, y, r.w, t(lang, "Amounts", "세기"));
    sliders(
        u,
        g,
        r.x,
        y,
        r.w,
        &[
            SliderKind::BorderWidth,
            SliderKind::SeparatorWidth,
            SliderKind::Radius,
            SliderKind::ModuleOpacity,
            SliderKind::Shadow,
            SliderKind::Highlight,
            SliderKind::Hover,
        ],
    );
}

// ------------------------------------------------------------------ property panel

pub(super) fn properties(u: &mut Ui, g: ElementId, r: Rect) {
    match u.m.section {
        Section::Colors => color_editor(u, g, r),
        Section::Typography => typography_props(u, g, r),
        _ => legacy_properties(u, g, r),
    }
}

fn color_editor(u: &mut Ui, g: ElementId, r: Rect) {
    let lang = u.lang;
    let m = u.m;
    let ct = m.color_target;
    let mut y = r.y;
    u.heading(
        g,
        "props:title",
        Rect::new(r.x, y, r.w, 26.0),
        &ct.name(lang),
    );
    y += 26.0;
    u.label(
        g,
        "props:hint",
        Rect::new(r.x, y, r.w, 18.0),
        ct.hint(lang),
        11.0,
        400,
        u.s.text2,
    );
    y += 24.0;
    let value = ct.get(&m.draft.bar.style);
    let c = effective(u, ct);
    let big = Rect::new(r.x, y, r.w, 44.0);
    let id = u.add(
        g,
        Element::new(
            key_id("color:current"),
            "color:current",
            Role::Text,
            match lang {
                Lang::En => format!("Current colour {}", catalog::hex(c)),
                Lang::Ko => format!("현재 색 {}", catalog::hex(c)),
            },
            big,
        ),
    );
    u.fill(big, 8.0, c, id);
    u.stroke(big.inflate(-0.5), 8.0, 1.0, u.s.control, id);
    let on = crate::bar::ensure_contrast(
        if windowsrice_theme::color::contrast_ratio(Color::WHITE, c) > 4.5 {
            Color::WHITE
        } else {
            Color::rgb(0x10, 0x11, 0x14)
        },
        Color::WHITE,
        c,
        4.5,
    );
    let label = match &value {
        Some(h) => h.clone(),
        None => format!(
            "{} · {}",
            catalog::hex(c),
            t(lang, "theme colour", "테마 색")
        ),
    };
    u.text(
        Rect::new(big.x + 12.0, big.y, big.w - 24.0, big.h),
        &label,
        u.ts(13.0, 500),
        on,
        id,
    );
    y += 52.0;
    // Swatches.
    let all: Vec<&str> = SWATCHES
        .iter()
        .map(|(_, h)| *h)
        .chain(MORE_SWATCHES)
        .collect();
    let per = ((r.w + 4.0) / 32.0).floor().max(1.0) as usize;
    let sz = (r.w + 4.0) / per as f32;
    for (i, h) in all.iter().enumerate() {
        let sr = Rect::new(
            r.x + (i % per) as f32 * sz,
            y + (i / per) as f32 * sz,
            sz - 2.0,
            sz - 2.0,
        );
        let sel = value.as_deref().is_some_and(|v| v.eq_ignore_ascii_case(h));
        u.swatch(
            g,
            &format!("color:sw:{i}"),
            sr,
            hex_color(h),
            h,
            sel,
            StudioAction::SetColor(Some((*h).to_owned())),
        );
    }
    y += all.len().div_ceil(per) as f32 * sz + 6.0;
    // A colour the bar draws differently for readability (Phase 12) is
    // reported right here, where it was chosen.
    let notice =
        u.m.warnings
            .iter()
            .find(|w| w.starts_with("Readability:") || w.starts_with("가독성:"))
            .cloned();
    if let Some(w) = notice {
        let col = u.s.warning;
        u.label(
            g,
            "color:readability",
            Rect::new(r.x, y, r.w, 40.0),
            &w,
            12.0,
            400,
            col,
        );
        y += 44.0;
    }
    // Hex and RGB.
    u.field(
        g,
        "field:color-hex",
        Rect::new(r.x, y, r.w, 32.0),
        t(lang, "Colour as #RRGGBB", "#RRGGBB 색"),
        value.as_deref().unwrap_or(""),
        &catalog::hex(c),
    );
    y += 38.0;
    let third = r.w / 3.0;
    for (ch, (en, ko, v)) in [("R", "빨강", c.r), ("G", "초록", c.g), ("B", "파랑", c.b)]
        .into_iter()
        .enumerate()
    {
        let x = r.x + ch as f32 * third;
        let id = u.add(
            g,
            Element::new(
                key_id(&format!("color:ch{ch}")),
                intern(&format!("color:ch{ch}")),
                Role::Text,
                format!("{} {v}", t(lang, en, ko)),
                Rect::new(x, y, third - 64.0, 30.0),
            ),
        );
        u.text(
            Rect::new(x, y, third - 64.0, 30.0),
            &format!("{en} {v}"),
            u.ts(12.0, 500),
            u.s.text,
            id,
        );
        for (k, (glyph, delta, en2, ko2)) in [
            ("\u{E738}", -16i16, "Less", "줄이기"),
            ("\u{E710}", 16, "More", "늘리기"),
        ]
        .into_iter()
        .enumerate()
        {
            u.button(
                g,
                &format!("color:ch{ch}:{k}"),
                Rect::new(x + third - 62.0 + k as f32 * 30.0, y + 1.0, 28.0, 28.0),
                Some(glyph),
                "",
                &format!("{} {}", t(lang, en, ko), t(lang, en2, ko2)),
                Kind::Secondary,
                true,
                StudioAction::ColorChannel(ch as u8, delta),
            );
        }
    }
    y += 36.0;
    let half = (r.w - 8.0) / 2.0;
    u.button(
        g,
        "color:pick",
        Rect::new(r.x, y, half, 32.0),
        Some("\u{E790}"),
        t(lang, "More colours…", "더 많은 색…"),
        t(
            lang,
            "Open the Windows colour picker",
            "Windows 색 선택 열기",
        ),
        Kind::Secondary,
        true,
        StudioAction::PickColor,
    );
    u.button(
        g,
        "color:theme",
        Rect::new(r.x + half + 8.0, y, half, 32.0),
        Some("\u{E7A7}"),
        t(lang, "Theme colour", "테마 색"),
        t(lang, "Use the theme's colour", "테마 색 사용"),
        Kind::Secondary,
        value.is_some() && !matches!(ct, ColorTarget::GradientStop(_)),
        StudioAction::SetColor(None),
    );
    y += 40.0;
    // Recent colours only when the panel has room (small windows).
    if !m.recent_colors.is_empty() && y + 20.0 + sz <= r.bottom() {
        u.label(
            g,
            "color:recent",
            Rect::new(r.x, y, r.w, 18.0),
            t(lang, "Recent", "최근"),
            11.0,
            500,
            u.s.text2,
        );
        y += 20.0;
        for (i, h) in m.recent_colors.iter().take(per).enumerate() {
            let sr = Rect::new(r.x + i as f32 * sz, y, sz - 2.0, sz - 2.0);
            u.swatch(
                g,
                &format!("color:recent:{i}"),
                sr,
                hex_color(h),
                h,
                false,
                StudioAction::SetColor(Some(h.clone())),
            );
        }
    }
}

fn typography_props(u: &mut Ui, g: ElementId, r: Rect) {
    let lang = u.lang;
    let m = u.m;
    let family = m
        .draft
        .bar
        .style
        .font_family
        .clone()
        .unwrap_or_else(|| m.theme_font.clone());
    let entry = m.fonts.iter().find(|f| f.family == family);
    let mut y = r.y;
    u.heading(g, "props:title", Rect::new(r.x, y, r.w, 26.0), &family);
    y += 28.0;
    let fam = intern(&family);
    let size = SliderKind::FontSize.get(&m.draft, &m.theme_defaults);
    let weight = m.draft.bar.style.font_weight.unwrap_or(400);
    let sample = Rect::new(r.x, y, r.w, 56.0);
    let sid = u.add(
        g,
        Element::new(
            key_id("typo:sample"),
            "typo:sample",
            Role::Text,
            format!("{family} {size}"),
            sample,
        ),
    );
    u.fill(sample, 8.0, u.s.background, sid);
    u.text(
        Rect::new(sample.x + 10.0, sample.y + 4.0, sample.w - 20.0, 28.0),
        "12:45  CPU 12%",
        TextStyle {
            font: FontRole::Family(fam),
            size: size + 3.0,
            weight,
        },
        u.s.text,
        sid,
    );
    let k = if entry.is_some_and(|e| e.hangul) {
        "가나다라 한글 견본".to_owned()
    } else {
        t(
            lang,
            "Korean text uses Malgun Gothic",
            "한글은 맑은 고딕으로 표시",
        )
        .to_owned()
    };
    u.text(
        Rect::new(sample.x + 10.0, sample.y + 30.0, sample.w - 20.0, 22.0),
        &k,
        TextStyle {
            font: FontRole::Family(fam),
            size: 13.0,
            weight: 400,
        },
        u.s.text2,
        sid,
    );
    y += 64.0;
    y = u.slider(g, SliderKind::FontSize, Rect::new(r.x, y, r.w, 48.0));
    y = u.slider(g, SliderKind::ClockSize, Rect::new(r.x, y, r.w, 48.0));
    u.label(
        g,
        "typo:weights",
        Rect::new(r.x, y, r.w, 20.0),
        t(
            lang,
            "Weight (available in this font)",
            "굵기 (이 글꼴에 있는 것)",
        ),
        12.0,
        500,
        u.s.text,
    );
    y += 22.0;
    let weights: Vec<u16> = entry
        .map(|e| {
            e.weights
                .iter()
                .copied()
                .filter(|w| (300..=600).contains(w))
                .collect()
        })
        .unwrap_or_else(|| vec![400]);
    let n = weights.len().max(1);
    let ww = (r.w - 6.0 * (n as f32 - 1.0)) / n as f32;
    for (i, w) in weights.iter().enumerate() {
        let rr = Rect::new(r.x + i as f32 * (ww + 6.0), y, ww, 40.0).snap(u.scale);
        let key = format!("font:weight:{i}");
        let sel = weight == *w;
        let id = u.add(
            g,
            Element::new(
                key_id(&key),
                intern(&key),
                Role::RadioButton,
                format!("{} {w}", t(lang, "Weight", "굵기")),
                rr,
            )
            .focusable(true)
            .selected(sel),
        );
        u.fill(rr, 6.0, if sel { u.s.selection } else { u.s.card }, id);
        u.stroke(
            rr.inflate(-0.5),
            6.0,
            if sel { 2.0 } else { 1.0 },
            if sel { u.s.accent } else { u.s.border },
            id,
        );
        u.decorate(id, rr, 6.0);
        u.text(
            Rect::new(rr.x + 6.0, rr.y, rr.w - 12.0, 24.0),
            "Aa",
            TextStyle {
                font: FontRole::Family(fam),
                size: 16.0,
                weight: *w,
            },
            u.s.text,
            id,
        );
        u.text(
            Rect::new(rr.x + 6.0, rr.y + 22.0, rr.w - 12.0, 16.0),
            &w.to_string(),
            u.ts(10.0, 400),
            u.s.text2,
            id,
        );
        u.actions.push((id, StudioAction::FontWeight(Some(*w))));
    }
    y += 48.0;
    let all_labels = m
        .draft
        .bar
        .modules
        .iter()
        .filter(|x| ModuleConfig::has_label(x.kind))
        .all(|x| x.show_label != Some(false));
    u.toggle(
        g,
        "font:labels",
        Rect::new(r.x, y, r.w, ROW),
        t(lang, "Labels (CPU, RAM …)", "레이블 (CPU, RAM …)"),
        all_labels,
        true,
        None,
        StudioAction::AllLabels(!all_labels),
    );
    y += ROW + 4.0;
    let h12 = m
        .draft
        .bar
        .modules
        .iter()
        .any(|x| x.kind == ModuleKind::Clock && x.hour12 == Some(true));
    u.toggle(
        g,
        "font:hour12",
        Rect::new(r.x, y, r.w, ROW),
        t(lang, "12-hour clock", "12시간제 시계"),
        h12,
        true,
        None,
        StudioAction::ToggleHour12All,
    );
}

fn weights_en(n: usize) -> String {
    if n == 1 {
        "1 weight".to_owned()
    } else {
        format!("{n} weights")
    }
}

/// Advanced section plus the animation switch (Phase 9).
pub(super) fn advanced(u: &mut Ui, g: ElementId, r: Rect) {
    super::advanced(u, g, r);
    let lang = u.lang;
    let y = u.content_bottom + 16.0;
    let y = subheading(u, g, "adv:motion", r.x, y, r.w, t(lang, "Motion", "움직임"));
    u.toggle(
        g,
        "adv:animations",
        // Same width as the switches above, so the toggles line up.
        Rect::new(r.x, y, (r.w - 16.0) / 2.0, ROW),
        t(lang, "Animations (short fades)", "애니메이션 (짧은 페이드)"),
        u.m.draft.bar.animations,
        true,
        Some(t(
            lang,
            "Off: no fades. Windows' Animation effects setting is also respected.",
            "끄면 페이드가 없습니다. Windows의 애니메이션 효과 설정도 따릅니다.",
        )),
        StudioAction::ToggleAnimations,
    );
    note(
        u,
        g,
        "adv:motion-note",
        r.x,
        y + ROW + 6.0,
        r.w,
        t(
            lang,
            "Fades run on the compositor without redrawing the bar. When Windows' Animation effects are off, windowsRICE does not animate.",
            "페이드는 바를 다시 그리지 않고 합성기에서 실행됩니다. Windows 애니메이션 효과가 꺼져 있으면 windowsRICE도 애니메이션하지 않습니다.",
        ),
    );
}

/// Appearance of the selected module (Phase 9): what it shows, its own
/// colours, size and density, and kind-specific options.
fn module_look(u: &mut Ui, g: ElementId, x: f32, y: f32, w: f32) {
    use windowsrice_core::bar::{Density, ModuleContent, SpeedUnit, ValueFormat};
    let lang = u.lang;
    let m = u.m;
    let Some(mc) = m
        .selected_module
        .as_ref()
        .and_then(|id| m.draft.bar.modules.iter().find(|x| &x.id == id))
    else {
        note(
            u,
            g,
            "look:none",
            x,
            y,
            w,
            t(
                lang,
                "Select a module card above to change that module's own appearance.",
                "위에서 모듈 카드를 선택하면 그 모듈만의 모양을 바꿀 수 있습니다.",
            ),
        );
        return;
    };
    let id = mc.id.clone();
    let name = crate::module::module_name(mc, lang);
    let mut y = subheading(
        u,
        g,
        "look:title",
        x,
        y,
        w,
        &match lang {
            Lang::En => format!("Appearance of {name}"),
            Lang::Ko => format!("{name} 모양"),
        },
    );
    let set = |f: ModuleField| StudioAction::ModuleSet(id.clone(), f);
    // What it shows.
    let contents = [
        (None, "Bar style", "바 스타일"),
        (Some(ModuleContent::IconOnly), "Icon only", "아이콘만"),
        (Some(ModuleContent::ValueOnly), "Value only", "값만"),
        (Some(ModuleContent::Default), "Icon + value", "아이콘 + 값"),
        (Some(ModuleContent::Full), "Everything", "모두"),
    ];
    let cw = (w - 8.0 * 4.0) / 5.0;
    for (i, (c, en, ko)) in contents.into_iter().enumerate() {
        u.radio(
            g,
            &format!("look:content:{i}"),
            Rect::new(x + i as f32 * (cw + 8.0), y, cw, ROW),
            t(lang, en, ko),
            mc.content == c,
            set(ModuleField::Content(c)),
        );
    }
    y += ROW + 12.0;
    // Colours.
    for (row, (label_en, label_ko, current, make)) in [
        (
            "Accent (icon and value)",
            "강조색 (아이콘과 값)",
            mc.color.clone(),
            ModuleField::Color as fn(Option<String>) -> ModuleField,
        ),
        (
            "Background",
            "배경",
            mc.background.clone(),
            ModuleField::Background as fn(Option<String>) -> ModuleField,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        u.label(
            g,
            &format!("look:clabel:{row}"),
            Rect::new(x, y, 200.0, ROW),
            t(lang, label_en, label_ko),
            12.0,
            500,
            u.s.text,
        );
        let sx = x + 210.0;
        let sz = 34.0;
        u.swatch(
            g,
            &format!("look:c{row}:none"),
            Rect::new(sx, y, sz, sz),
            None,
            t(lang, "Bar style", "바 스타일"),
            current.is_none(),
            set(make(None)),
        );
        for (i, (sname, h)) in SWATCHES.iter().enumerate() {
            let rr = Rect::new(sx + (i as f32 + 1.0) * (sz + 4.0), y, sz, sz);
            if rr.right() > x + w {
                break;
            }
            u.swatch(
                g,
                &format!("look:c{row}:{i}"),
                rr,
                hex_color(h),
                sname,
                current.as_deref() == Some(*h),
                set(make(Some((*h).to_owned()))),
            );
        }
        y += sz + 10.0;
    }
    // Size and density.
    let half = (w - 24.0) / 2.0;
    let fs = mc.font_size.map_or_else(
        || t(lang, "bar style", "바 스타일").to_owned(),
        |s| s.to_string(),
    );
    u.stepper(
        g,
        "look:size",
        Rect::new(x, y, half, ROW),
        t(lang, "Text size", "글자 크기"),
        &fs,
        (mc.font_size.unwrap_or(13) > 10).then(|| set(ModuleField::FontSize(-1))),
        (mc.font_size.unwrap_or(13) < 20).then(|| set(ModuleField::FontSize(1))),
    );
    let dw = (half - 24.0) / 4.0;
    for (i, (dv, en, ko)) in [
        (None, "Bar", "바"),
        (Some(Density::Compact), "Compact", "촘촘"),
        (Some(Density::Normal), "Normal", "보통"),
        (Some(Density::Expanded), "Expanded", "넉넉"),
    ]
    .into_iter()
    .enumerate()
    {
        u.radio(
            g,
            &format!("look:density:{i}"),
            Rect::new(x + half + 24.0 + i as f32 * (dw + 8.0), y, dw, ROW),
            t(lang, en, ko),
            mc.density == dv,
            set(ModuleField::Density(dv)),
        );
    }
    y += ROW + 12.0;
    // Kind-specific options.
    let w3 = (w - 16.0) / 3.0;
    match mc.kind {
        ModuleKind::Ram | ModuleKind::Vram => {
            for (i, (v, en, ko)) in [
                (None, "Percent", "퍼센트"),
                (
                    Some(ValueFormat::Amount),
                    "Amount in use (GB)",
                    "사용량 (GB)",
                ),
            ]
            .into_iter()
            .enumerate()
            {
                u.radio(
                    g,
                    &format!("look:format:{i}"),
                    Rect::new(x + i as f32 * (w3 + 8.0), y, w3, ROW),
                    t(lang, en, ko),
                    mc.value_format.filter(|f| *f == ValueFormat::Amount) == v,
                    set(ModuleField::ValueFormat(v)),
                );
            }
            y += ROW + 12.0;
        }
        ModuleKind::Upload | ModuleKind::Download => {
            for (i, (v, en, ko)) in [
                (None, "Bytes per second (KB/s)", "초당 바이트 (KB/s)"),
                (
                    Some(SpeedUnit::Bits),
                    "Bits per second (Mb/s)",
                    "초당 비트 (Mb/s)",
                ),
            ]
            .into_iter()
            .enumerate()
            {
                u.radio(
                    g,
                    &format!("look:unit:{i}"),
                    Rect::new(x + i as f32 * (w3 + 8.0), y, w3, ROW),
                    t(lang, en, ko),
                    mc.speed_unit.filter(|s| *s == SpeedUnit::Bits) == v,
                    set(ModuleField::SpeedUnit(v)),
                );
            }
            y += ROW + 12.0;
        }
        ModuleKind::Clock => {
            u.toggle(
                g,
                "look:date",
                Rect::new(x, y, w.min(420.0), ROW),
                t(lang, "Show the date", "날짜 표시"),
                mc.show_date != Some(false),
                true,
                None,
                set(ModuleField::ShowDate(mc.show_date == Some(false))),
            );
            y += ROW + 12.0;
        }
        _ => {}
    }
    let custom = mc.content.is_some()
        || mc.color.is_some()
        || mc.background.is_some()
        || mc.font_size.is_some()
        || mc.density.is_some()
        || mc.value_format.is_some()
        || mc.speed_unit.is_some()
        || mc.show_date.is_some();
    u.button(
        g,
        "look:reset",
        Rect::new(x, y, 260.0_f32.min(w), 34.0),
        Some("\u{E7A7}"),
        t(lang, "Use the bar style", "바 스타일 사용"),
        t(
            lang,
            "Remove this module's own appearance settings",
            "이 모듈만의 모양 설정 지우기",
        ),
        Kind::Secondary,
        custom,
        set(ModuleField::Reset),
    );
}
