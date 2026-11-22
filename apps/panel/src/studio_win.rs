//! The Design Studio window: state, input, persistence and Apply.
//!
//! The Studio edits a draft (theme selection plus the whole `[bar]`
//! section). The live preview is the real bar layout run on that draft.
//! Nothing reaches the desktop until Apply, which validates the draft,
//! saves it through the same `ThemeService` paths the bar uses (atomic
//! write, original preserved before the first save of an older file),
//! rolls back if the second save fails, and updates the running bars in
//! place. Theme and preset files are separate resources written only by
//! their explicit buttons.

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, VK_BACK, VK_CONTROL, VK_ESCAPE, VK_RETURN, VK_TAB,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FileOpenDialog, FileSaveDialog, IFileOpenDialog, IFileSaveDialog, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, GetClientRect, IDCANCEL, IDYES, MB_ICONQUESTION,
    MB_ICONWARNING, MB_YESNO, MB_YESNOCANCEL, MessageBoxW, SW_SHOW, SWP_NOACTIVATE, SWP_NOZORDER,
    SetWindowPos, ShowWindow, WS_EX_NOREDIRECTIONBITMAP, WS_OVERLAPPEDWINDOW,
};
use windows::core::{BOOL, HSTRING, w};
use windowsrice_core::bar::{BarConfig, ModuleKind, parse_hex_color};
use windowsrice_core::config::ThemeSelection;
use windowsrice_core::preset::{Preset, list_presets, preset_file_name};
use windowsrice_render::TextSystem;
use windowsrice_theme::Mode;
use windowsrice_theme::engine::ResolvedTheme;
use windowsrice_theme::schema::{Palette, Theme};
use windowsrice_theme::{Color, ThemeError, parse_theme};
use windowsrice_ui::keyboard::{Key, KeyOutcome, Orientation, handle_key_oriented};
use windowsrice_ui::legibility::{SurfacePlan, plan};
use windowsrice_ui::module::Reading;
use windowsrice_ui::strings::Lang;
use windowsrice_ui::studio::catalog::{self, Gallery};
use windowsrice_ui::studio::{
    ColorTarget, Draft, Drag, FontEntry, FontFilter, MIN_SIZE, MonitorEntry, PresetEntry,
    PreviewBackdrop, PreviewInputs, PreviewView, Section, Severity, SliderKind, StudioAction,
    StudioModel, StudioScene, StudioStyle, TextEdit, ThemeDefaults, ThemeEdit, ThemeEntry,
    apply_draft_action, apply_theme_card, build_studio, move_module, toggle_monitor, valid_hex,
};
use windowsrice_ui::style::{PanelStyle, design_contrast};
use windowsrice_ui::{ElementId, PhysRect, Point, Size};
use windowsrice_windows::live::{appearance, monitors};

use crate::controller::{
    Controller, Deferred, Surface, ThemeSource, font_config, map_key, modifiers, request,
    system_mode, track_leave,
};
use crate::window;

pub const CLASS_STUDIO: windows::core::PCWSTR = w!("windowsRICE.Studio");

/// A validated edited theme with its warnings, or the errors that stop it.
type ThemeCheck = Result<(Theme, Vec<String>), Vec<String>>;

/// Questions asked with a message box before destructive actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirm {
    DeleteTheme(String),
    DeletePreset(PathBuf),
    OverwritePreset { from: PathBuf, to: PathBuf },
    ResetAll,
    UnsavedClose,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StudioSettings {
    pub section: String,
    pub appearance: Option<bool>,
    /// Window rectangle in physical pixels.
    pub rect: Option<[i32; 4]>,
    /// Recently chosen colours, newest first.
    pub recent_colors: Vec<String>,
}

pub struct StudioWin {
    pub s: Surface,
    pub model: StudioModel,
    pub undo: Vec<Draft>,
    pub redo: Vec<Draft>,
    pub scene: Option<StudioScene>,
    /// Pointer pressed on a draggable element: (x, y, element).
    pub press: Option<(f32, f32, ElementId)>,
    pub settings_path: Option<PathBuf>,
    /// Slider being dragged with the pointer.
    pub slider_drag: Option<SliderKind>,
    /// Where smooth scrolling is heading.
    pub scroll_target: Option<f32>,
    /// Package opened on the Import page (Phase 12).
    pub import_pkg: Option<crate::design_io::OpenPackage>,
}

/// Timer id of smooth scrolling (Studio window).
const SCROLL_TIMER: usize = 7;

fn hex(c: Color) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)
}

fn col(h: &str) -> Option<Color> {
    parse_hex_color(h.trim()).map(|(r, g, b)| Color::rgb(r, g, b))
}

/// A complete palette from three chosen colours, starting from `base` for
/// the semantic colours. The validator then checks every contrast pair.
fn derive_palette(base: &Palette, bg: Color, accent: Color, text: Color) -> Palette {
    let mix = |c: Color, a: u8, over: Color| Color { a, ..c }.over(over);
    let mut p = base.clone();
    p.background = bg;
    p.surface_base = mix(text, 0x08, bg);
    p.surface_raised = mix(text, 0x10, bg);
    p.surface_overlay = mix(text, 0x16, bg);
    p.text_primary = text;
    p.text_secondary = mix(text, 0xD0, p.surface_overlay);
    p.text_muted = mix(text, 0xB8, p.surface_overlay);
    p.text_disabled = mix(text, 0x60, bg);
    p.accent = accent;
    p.accent_hover = accent;
    p.accent_pressed = accent;
    p.focus_ring = accent;
    p.selection = mix(accent, 0x40, bg);
    p.border_subtle = Color { a: 0x22, ..text };
    p.border_control = Color { a: 0x99, ..text };
    let on_dark = windowsrice_theme::color::contrast_ratio(Color::WHITE, accent);
    let on_light = windowsrice_theme::color::contrast_ratio(Color::rgb(0x0B, 0x10, 0x18), accent);
    p.text_on_accent = if on_dark >= on_light {
        Color::WHITE
    } else {
        Color::rgb(0x0B, 0x10, 0x18)
    };
    p
}

fn slug(name: &str) -> String {
    let mut s = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
        } else if !s.is_empty() && !s.ends_with('-') {
            s.push('-');
        }
    }
    s.trim_matches('-')
        .chars()
        .take(40)
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}

fn message_box(
    owner: HWND,
    text: &str,
    caption: &str,
    flags: windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_STYLE,
) -> i32 {
    // SAFETY: a modal message box owned by our window.
    unsafe {
        MessageBoxW(
            Some(owner),
            &HSTRING::from(text),
            &HSTRING::from(caption),
            flags,
        )
        .0
    }
}

/// Native Open dialog for a preset file. `None` if cancelled.
fn open_dialog(owner: HWND, title: &str) -> Option<PathBuf> {
    // SAFETY: the common item dialog on our STA thread; strings outlive the calls.
    unsafe {
        let d: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let filters = [COMDLG_FILTERSPEC {
            pszName: w!("windowsRICE preset (*.toml)"),
            pszSpec: w!("*.toml"),
        }];
        let _ = d.SetFileTypes(&filters);
        let _ = d.SetTitle(&HSTRING::from(title));
        d.Show(Some(owner)).ok()?;
        let item = d.GetResult().ok()?;
        let p = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0.cast()));
        s.map(PathBuf::from)
    }
}

fn save_dialog(owner: HWND, title: &str, file: &str) -> Option<PathBuf> {
    // SAFETY: as in `open_dialog`; the dialog asks before overwriting.
    unsafe {
        let d: IFileSaveDialog =
            CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let filters = [COMDLG_FILTERSPEC {
            pszName: w!("windowsRICE preset (*.toml)"),
            pszSpec: w!("*.toml"),
        }];
        let _ = d.SetFileTypes(&filters);
        let _ = d.SetDefaultExtension(w!("toml"));
        let _ = d.SetFileName(&HSTRING::from(file));
        let _ = d.SetTitle(&HSTRING::from(title));
        d.Show(Some(owner)).ok()?;
        let item = d.GetResult().ok()?;
        let p = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0.cast()));
        s.map(PathBuf::from)
    }
}

impl Controller {
    pub(crate) fn lang(&self) -> Lang {
        self.strings.lang
    }

    fn tr<'a>(&self, en: &'a str, ko: &'a str) -> &'a str {
        if self.lang() == Lang::En { en } else { ko }
    }

    fn config_dir(&self) -> Option<PathBuf> {
        match &self.theme {
            ThemeSource::Service(s) => s.config_path().parent().map(Path::to_path_buf),
            ThemeSource::Override { .. } => self
                .args
                .config
                .as_ref()
                .and_then(|c| c.parent())
                .map(Path::to_path_buf),
        }
    }

    fn presets_dir(&self) -> Option<PathBuf> {
        self.config_dir().map(|d| d.join("presets"))
    }

    pub(crate) fn studio_config_dir(&self) -> Option<PathBuf> {
        self.config_dir()
    }

    pub(crate) fn studio_presets_dir(&self) -> Option<PathBuf> {
        self.presets_dir()
    }

    pub(crate) fn studio_status(&mut self, text: String, sev: Severity) {
        self.status(text, sev);
    }

    pub(crate) fn studio_refresh(&mut self) {
        self.studio_refresh_lists();
    }

    pub(crate) fn studio_edit(&mut self, f: impl FnOnce(&mut Draft) -> bool) {
        self.edit(f);
    }

    pub(crate) fn theme_catalog_ids(&self) -> Vec<String> {
        self.theme
            .catalog()
            .entries()
            .iter()
            .map(|e| e.id().to_owned())
            .collect()
    }

    fn themes_dir(&self) -> Option<PathBuf> {
        match &self.theme {
            ThemeSource::Service(s) => s.user_themes_dir().map(Path::to_path_buf),
            ThemeSource::Override { .. } => self.config_dir().map(|d| d.join("themes")),
        }
    }

    pub fn is_studio(&self, hwnd: HWND) -> bool {
        self.studio.as_ref().is_some_and(|s| s.s.hwnd == hwnd)
    }

    fn current_draft(&self) -> Draft {
        Draft::new(
            self.config().theme.clone(),
            self.config().bar.clone(),
            self.config().bottom_bar.clone(),
        )
    }

    /// The draft's theme resolved like the bar resolves it, with overrides.
    fn draft_theme(&self, d: &Draft) -> Option<ResolvedTheme> {
        windowsrice_ui::style::resolve_bar_theme(
            self.theme.catalog(),
            &d.theme,
            &d.bar.style,
            system_mode(&self.args),
        )
    }

    fn studio_style(&self, dark: bool) -> StudioStyle {
        if let Some(hc) = window::high_contrast() {
            return StudioStyle::high_contrast(&hc);
        }
        self.studio_style_themed(dark)
    }

    fn studio_style_themed(&self, dark: bool) -> StudioStyle {
        let cat = self.theme.catalog();
        let p = cat
            .get("windowsrice-default")
            .and_then(|e| {
                e.theme
                    .palette
                    .get(if dark { Mode::Dark } else { Mode::Light })
                    .cloned()
            })
            .or_else(|| self.theme.applied().map(|t| t.palette.clone()));
        StudioStyle::from_palette(&p.unwrap_or_else(|| {
            windowsrice_theme::presets::default_theme()
                .theme
                .palette
                .dark
                .expect("default has dark")
        }))
    }

    fn theme_entries(&self) -> Vec<ThemeEntry> {
        self.theme
            .catalog()
            .entries()
            .iter()
            .map(|e| {
                let p = e
                    .theme
                    .palette
                    .get(e.theme.meta.default_mode)
                    .or(e.theme.palette.dark.as_ref())
                    .or(e.theme.palette.light.as_ref());
                let sw = p.map_or([Color::BLACK; 4], |p| {
                    [p.background, p.surface_base, p.accent, p.text_primary]
                });
                ThemeEntry {
                    id: e.id().to_owned(),
                    name: e.theme.meta.name.clone(),
                    builtin: e.is_builtin(),
                    has_dark: e.theme.palette.dark.is_some(),
                    has_light: e.theme.palette.light.is_some(),
                    swatches: sw,
                }
            })
            .collect()
    }

    fn preset_entries(&self) -> (Vec<PresetEntry>, Vec<PathBuf>) {
        let Some(dir) = self.presets_dir() else {
            return (Vec::new(), Vec::new());
        };
        let list = list_presets(&dir);
        let paths = list.iter().map(|(p, _)| p.clone()).collect();
        let entries = list
            .into_iter()
            .map(|(p, r)| PresetEntry {
                file: p
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                name: r.as_ref().ok().map(|x| x.name.clone()),
                error: r.err().map(|e| e.to_string()),
            })
            .collect();
        (entries, paths)
    }

    // ---------------------------------------------------------------- open/close

    pub fn open_studio(&mut self) -> Deferred {
        if let Some(st) = &self.studio {
            return Deferred::Activate(st.s.hwnd.0 as isize);
        }
        let settings_path = self.config_dir().map(|d| d.join("studio.toml"));
        let settings: StudioSettings = settings_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default();
        // Size and position: saved rectangle, else 1440×900 DIPs centred on the primary monitor.
        let mons = monitors::monitors();
        let prim = mons.iter().find(|m| m.primary).or(mons.first()).cloned();
        let scale = prim.as_ref().map_or(1.0, |m| m.dpi as f32 / 96.0);
        let rect = settings
            .rect
            .map(|[x, y, w, h]| PhysRect {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            })
            .filter(|r| {
                mons.iter().any(|m| {
                    r.left < m.work.right
                        && r.right > m.work.left
                        && r.top < m.work.bottom
                        && r.bottom > m.work.top
                })
            });
        let rect = rect.unwrap_or_else(|| {
            let wk = prim.as_ref().map_or(
                PhysRect {
                    left: 0,
                    top: 0,
                    right: 1920,
                    bottom: 1080,
                },
                |m| PhysRect {
                    left: m.work.left,
                    top: m.work.top,
                    right: m.work.right,
                    bottom: m.work.bottom,
                },
            );
            let w = ((1440.0 * scale) as i32).min(wk.width() - 40);
            let h = ((900.0 * scale) as i32).min(wk.height() - 40);
            let x = wk.left + (wk.width() - w) / 2;
            let y = wk.top + (wk.height() - h) / 2;
            PhysRect {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            }
        });
        // SAFETY: creates our own top-level window (standard frame).
        let hwnd = match unsafe {
            CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP,
                CLASS_STUDIO,
                &HSTRING::from(self.tr("windowsRICE Design Studio", "windowsRICE 디자인 스튜디오")),
                WS_OVERLAPPEDWINDOW,
                rect.left,
                rect.top,
                rect.width(),
                rect.height(),
                None,
                None,
                None,
                None,
            )
        } {
            Ok(h) => h,
            Err(e) => {
                self.stats.errors.push(format!("studio window: {e}"));
                return Deferred::Nothing;
            }
        };
        let text = match TextSystem::new(font_config(&self.style, self.lang())) {
            Ok(t) => t,
            Err(e) => {
                self.stats.errors.push(format!("studio DirectWrite: {e}"));
                return Deferred::Nothing;
            }
        };
        let placeholder = windowsrice_ui::panel::Scene {
            tree: windowsrice_ui::UiTree::new(windowsrice_ui::tree::Element::new(
                ElementId(1),
                "studio",
                windowsrice_ui::Role::Window,
                "windowsRICE Design Studio",
                windowsrice_ui::Rect::new(0.0, 0.0, 1.0, 1.0),
            )),
            draw: Default::default(),
            size: Size { w: 1.0, h: 1.0 },
            scale: 1.0,
        };
        let s = match Surface::new(hwnd, text, "windowsRICE.Studio", placeholder) {
            Ok(s) => s,
            Err(e) => {
                self.stats.errors.push(e);
                return Deferred::Nothing;
            }
        };
        let dark = settings
            .appearance
            .unwrap_or_else(|| !appearance::apps_use_light_theme().unwrap_or(false));
        let draft = self.current_draft();
        let section = windowsrice_ui::studio::section_from_key(&settings.section);
        let model = StudioModel {
            section,
            draft: draft.clone(),
            applied: draft,
            can_undo: false,
            can_redo: false,
            themes: Vec::new(),
            theme_selected: Some(self.config().theme.active.clone()),
            theme_edit: None,
            presets: Vec::new(),
            preset_selected: None,
            preset_name: String::new(),
            monitors: Vec::new(),
            fonts: Vec::new(),
            theme_font: self.style.family.clone(),
            selected_module: None,
            readings: self.readings.clone(),
            preview_backdrop: PreviewBackdrop::Dark,
            status: None,
            blocking: Vec::new(),
            warnings: Vec::new(),
            text_edit: None,
            drag: None,
            focus: None,
            keyboard_active: false,
            hover: None,
            dark,
            appearance: settings.appearance,
            bar_height: self.style.height,
            monitor_width: 2560.0,
            theme_opacity: 0.88,
            scroll: 0.0,
            font_query: String::new(),
            color_target: ColorTarget::Background,
            recent_colors: settings.recent_colors.clone(),
            preview_view: PreviewView::All,
            theme_defaults: ThemeDefaults::default(),
            font_filter: FontFilter::All,
            import: Default::default(),
        };
        self.studio = Some(StudioWin {
            s,
            model,
            undo: Vec::new(),
            redo: Vec::new(),
            scene: None,
            press: None,
            settings_path,
            slider_drag: None,
            scroll_target: None,
            import_pkg: None,
        });
        self.studio_set_dark(dark);
        // Installed families: discovered once on a background thread at
        // startup (`prefetch_fonts`) and reused for every opening. If the
        // Studio opens before that finishes, they are read here once.
        let fonts: Vec<FontEntry> = {
            let t0 = Instant::now();
            let cached = FONT_CACHE.lock().ok().and_then(|c| c.clone());
            let (list, source) = match cached {
                Some(l) => (l, "cache"),
                None => {
                    let st = self.studio.as_ref().expect("just created");
                    let l = to_entries(st.s.renderer.text().installed_families());
                    if let Ok(mut c) = FONT_CACHE.lock() {
                        *c = Some(l.clone());
                    }
                    (l, "discovered now")
                }
            };
            self.stats.popup_log.push(format!(
                "{:.2}s studio fonts: {} families ({source}) in {} ms",
                self.stats.now_s(),
                list.len(),
                t0.elapsed().as_millis()
            ));
            list
        };
        if let Some(st) = &mut self.studio {
            st.model.fonts = fonts;
        }
        self.studio_refresh_lists();
        self.studio_resize();
        self.stats
            .popup_log
            .push(format!("{:.2}s studio opened", self.stats.now_s()));
        // SAFETY: shows our own window.
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        Deferred::Activate(hwnd.0 as isize)
    }

    fn studio_set_dark(&mut self, dark: bool) {
        if let Some(st) = &mut self.studio {
            st.model.dark = dark;
            let v: BOOL = dark.into();
            // SAFETY: attribute of our own window.
            unsafe {
                let _ = DwmSetWindowAttribute(
                    st.s.hwnd,
                    DWMWA_USE_IMMERSIVE_DARK_MODE,
                    (&raw const v).cast(),
                    4,
                );
            }
        }
    }

    fn studio_refresh_lists(&mut self) {
        let themes = self.theme_entries();
        let (presets, _) = self.preset_entries();
        let mons: Vec<MonitorEntry> = monitors::monitors()
            .into_iter()
            .map(|m| MonitorEntry {
                name: m
                    .friendly_name
                    .clone()
                    .unwrap_or_else(|| m.device_name.clone()),
                device: m.device_name,
                id: m.stable_id,
                primary: m.primary,
            })
            .collect();
        let width = self
            .bars
            .first()
            .map_or(2560.0, |b| b.rect.width() as f32 / b.s.scale);
        if let Some(st) = &mut self.studio {
            st.model.themes = themes;
            st.model.presets = presets;
            st.model.monitors = mons;
            st.model.monitor_width = width;
            if st
                .model
                .preset_selected
                .is_some_and(|i| i >= st.model.presets.len())
            {
                st.model.preset_selected = None;
            }
        }
    }

    /// Recomputes checks and rebuilds the Studio scene.
    pub fn studio_rebuild(&mut self) {
        let Some(st) = self.studio.as_ref() else {
            return;
        };
        let draft = st.model.draft.clone();
        let dark = st.model.dark;
        let resolved = self.draft_theme(&draft);
        let mut blocking = Vec::new();
        let mut warnings = Vec::new();
        let lang = self.lang();
        let mut cfg = self.config().clone();
        cfg.bar = draft.top().clone();
        cfg.bottom_bar = draft.bottom().cloned();
        cfg.theme = draft.theme.clone();
        if let Err(e) = cfg.validate() {
            blocking.push(e.to_string());
        }
        if self.theme.catalog().get(&draft.theme.active).is_none() {
            blocking.push(match lang {
                Lang::En => format!(
                    "Theme {:?} is not available; choose another in Themes.",
                    draft.theme.active
                ),
                Lang::Ko => format!(
                    "테마 {:?}을(를) 사용할 수 없습니다. 테마에서 다른 테마를 고르세요.",
                    draft.theme.active
                ),
            });
        }
        let (pstyle, pplan, opacity, bar_h) = match &resolved {
            Some(r) => {
                if let Some((what, c)) = design_contrast(r, &draft.bar.style)
                    .into_iter()
                    .chain(windowsrice_ui::style::module_contrast(
                        r,
                        &draft.bar.modules,
                    ))
                    .filter(|(_, c)| *c + 1e-9 < 4.5)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                {
                    blocking.push(match lang {
                        Lang::En => format!("Contrast of {what} is {c:.1}:1, below the 4.5:1 minimum. Choose a darker background or a lighter text colour (or the reverse)."),
                        Lang::Ko => format!("{what} 대비가 {c:.1}:1로 최소 4.5:1보다 낮습니다. 배경을 더 어둡게 하거나 글자를 더 밝게 (또는 반대로) 바꾸세요."),
                    });
                }
                // Colours the bar corrects for readability (Phase 12): shown,
                // never silent. The theme without the design's colours is the
                // reference for what the user chose.
                let plain = windowsrice_ui::style::resolve_bar_theme(
                    self.theme.catalog(),
                    &draft.theme,
                    &Default::default(),
                    system_mode(&self.args),
                );
                if let Some(plain) = plain {
                    for (role, from, to) in
                        windowsrice_ui::style::readability_fixes(&plain, &draft.bar.style)
                    {
                        let hex = |c: windowsrice_theme::Color| {
                            format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)
                        };
                        warnings.push(match lang {
                            Lang::En => format!(
                                "Readability: {role} {} is shown as {} on this background.",
                                hex(from),
                                hex(to)
                            ),
                            Lang::Ko => format!(
                                "가독성: 이 배경에서 {role} {}을(를) {}(으)로 표시합니다.",
                                hex(from),
                                hex(to)
                            ),
                        });
                    }
                }
                let style = PanelStyle::from_theme(
                    r,
                    window::high_contrast().as_ref(),
                    window::animations_enabled(),
                );
                let p: SurfacePlan = plan(
                    &request(
                        r,
                        &self.args,
                        self.backdrop_ok,
                        style.high_contrast,
                        &draft.bar.style,
                    ),
                    &r.palette,
                );
                if !p.meets_aa_worst_case {
                    warnings.push(match lang {
                        Lang::En => format!("Over a very light or dark background the text could drop to {:.1}:1. Text backplates are off; choose “When needed” or raise the opacity to at least {}%.", p.worst_case_text_contrast, (p.min_opacity_for_aa * 100.0).round()),
                        Lang::Ko => format!("아주 밝거나 어두운 배경 위에서 텍스트 대비가 {:.1}:1까지 낮아질 수 있습니다. 텍스트 배경판이 꺼져 있으니 ‘필요할 때’를 고르거나 불투명도를 {}% 이상으로 올리세요.", p.worst_case_text_contrast, (p.min_opacity_for_aa * 100.0).round()),
                    });
                }
                let h = draft.bar.height.map_or(style.height, f32::from);
                let op = windowsrice_ui::style::bar_surface(r, &draft.bar.style).1;
                (Some(style), Some(p), op, h)
            }
            None => (None, None, 0.88, 34.0),
        };
        let r = &self.readings;
        for m in draft.bar.modules.iter().filter(|m| m.enabled) {
            let why = match m.kind {
                ModuleKind::Gpu | ModuleKind::Vram => match (&r.gpu, &r.vram) {
                    (Reading::Unavailable(w), _) | (_, Reading::Unavailable(w)) => Some(w.clone()),
                    _ => None,
                },
                ModuleKind::Volume => {
                    if let Reading::Unavailable(w) = &r.audio {
                        Some(w.clone())
                    } else {
                        None
                    }
                }
                ModuleKind::Network => {
                    if let Reading::Unavailable(w) = &r.network {
                        Some(w.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(w) = why {
                warnings.push(format!(
                    "{}: {w}",
                    windowsrice_ui::module::module_name(m, lang)
                ));
            }
        }
        if draft
            .bar
            .modules
            .iter()
            .any(|m| m.kind == ModuleKind::Battery && m.enabled)
            && matches!(r.battery, Reading::Absent)
        {
            warnings.push(
                self.tr(
                    "Battery: this computer has no battery, so the module stays hidden.",
                    "배터리: 이 컴퓨터에는 배터리가 없어 모듈이 표시되지 않습니다.",
                )
                .to_owned(),
            );
        }
        let style_s = self.studio_style(dark);
        let readings = self.readings.clone();
        let strings = self.strings;
        // Take the Studio out while building so the card resolver can use
        // the theme service.
        let Some(mut st) = self.studio.take() else {
            return;
        };
        st.model.blocking = blocking;
        st.model.warnings = warnings;
        st.model.readings = readings;
        st.model.theme_opacity = opacity;
        st.model.bar_height = bar_h;
        st.model.can_undo = !st.undo.is_empty();
        st.model.can_redo = !st.redo.is_empty();
        st.model.focus = st.s.focus;
        st.model.keyboard_active = st.s.keyboard_active;
        st.model.hover = st.s.hover;
        let (Some(ps), Some(pp)) = (pstyle, pplan) else {
            self.studio = Some(st);
            return;
        };
        st.model.theme_defaults = ThemeDefaults {
            opacity,
            height: ps.height,
            font_size: ps.size_body,
            padding: ps.padding_x,
            gap: ps.gap,
            radius: ps.radius_control,
        };
        let size = Size {
            w: st.s_size().0 as f32 / st.s.scale,
            h: st.s_size().1 as f32 / st.s.scale,
        };
        let hc = window::high_contrast();
        let anim = window::animations_enabled();
        let resolve = |d: &Draft| -> Option<(PanelStyle, SurfacePlan)> {
            let r = self.draft_theme(d)?;
            let style = PanelStyle::from_theme(&r, hc.as_ref(), anim);
            let p = plan(
                &request(&r, &self.args, true, style.high_contrast, &d.bar.style),
                &r.palette,
            );
            Some((style, p))
        };
        let t0 = Instant::now();
        let scene = build_studio(
            &st.model,
            &style_s,
            &PreviewInputs {
                style: &ps,
                plan: &pp,
                resolve: &resolve,
            },
            st.s.renderer.text(),
            size,
            st.s.scale,
            &strings,
        );
        let build_us = t0.elapsed().as_micros();
        st.model.scroll = st.model.scroll.clamp(0.0, scene.scroll_max);
        let problems = scene.scene.tree.problems();
        let events =
            st.s.set_scene(scene.scene.clone(), st.client_origin(), "studio");
        st.scene = Some(scene);
        self.studio = Some(st);
        self.stats.uia_events_raised += u64::from(events);
        self.stats.studio_build_us.push(build_us as u64);
        if !problems.is_empty() {
            self.stats.a11y_problems = problems;
        }
    }
    pub fn studio_resize(&mut self) {
        let Some(st) = self.studio.as_mut() else {
            return;
        };
        // SAFETY: DPI of our own window.
        let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(st.s.hwnd) }.max(96);
        st.s.dpi = dpi;
        st.s.scale = dpi as f32 / 96.0;
        let (w, h) = st.s_size();
        let _ = st.s.renderer.resize(w.max(1) as u32, h.max(1) as u32, dpi);
        self.studio_rebuild();
    }

    /// Minimum outer window size for WM_GETMINMAXINFO.
    pub fn studio_min_size(&self, hwnd: HWND) -> Option<(i32, i32)> {
        if !self.is_studio(hwnd) {
            return None;
        }
        // SAFETY: DPI of our own window.
        let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96) as f32 / 96.0;
        Some((
            (MIN_SIZE.w * dpi) as i32 + 16,
            (MIN_SIZE.h * dpi) as i32 + 40,
        ))
    }

    /// Close request: asks about unapplied changes first.
    pub fn studio_close_request(&mut self) -> Deferred {
        let Some(st) = &self.studio else {
            return Deferred::Nothing;
        };
        if st.model.draft != st.model.applied {
            Deferred::Confirm(Confirm::UnsavedClose)
        } else {
            self.studio_close();
            Deferred::Nothing
        }
    }

    pub fn studio_close(&mut self) {
        let Some(st) = self.studio.take() else { return };
        let mut r = RECT::default();
        // SAFETY: reads our own window rectangle.
        let _ = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowRect(st.s.hwnd, &raw mut r)
        };
        if let Some(p) = &st.settings_path {
            let settings = StudioSettings {
                section: st.model.section.key().to_owned(),
                appearance: st.model.appearance,
                recent_colors: st.model.recent_colors.clone(),
                rect: Some([r.left, r.top, r.right - r.left, r.bottom - r.top]),
            };
            if let Ok(t) = toml::to_string(&settings) {
                if let Some(d) = p.parent() {
                    let _ = std::fs::create_dir_all(d);
                }
                // Written through a temporary file, so an interrupted write
                // never leaves a truncated studio.toml.
                let tmp = p.with_extension("toml.tmp");
                if std::fs::write(&tmp, t)
                    .and_then(|()| std::fs::rename(&tmp, p))
                    .is_err()
                {
                    let _ = std::fs::remove_file(&tmp);
                    self.stats
                        .errors
                        .push(format!("studio settings not saved: {}", p.display()));
                }
            }
        }
        st.s.release_uia();
        let hwnd = st.s.hwnd;
        drop(st);
        // SAFETY: destroying our own window.
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        self.stats
            .popup_log
            .push(format!("{:.2}s studio closed", self.stats.now_s()));
    }

    // ---------------------------------------------------------------- edits

    fn status(&mut self, text: impl Into<String>, sev: Severity) {
        if let Some(st) = &mut self.studio {
            st.model.status = Some((text.into(), sev));
        }
    }

    /// Applies a draft change with undo history.
    fn edit(&mut self, f: impl FnOnce(&mut Draft) -> bool) {
        let Some(st) = self.studio.as_mut() else {
            return;
        };
        let before = st.model.draft.clone();
        if f(&mut st.model.draft) && st.model.draft != before {
            st.undo.push(before);
            if st.undo.len() > 200 {
                st.undo.remove(0);
            }
            st.redo.clear();
            st.model.status = None;
        }
    }

    pub fn studio_action(&mut self, a: StudioAction) -> Deferred {
        if let Some(d) = self.design_action(&a) {
            return d;
        }
        self.stats
            .popup_log
            .push(format!("{:.2}s studio {a:?}", self.stats.now_s()));
        let (op, th) = self.studio.as_ref().map_or((0.88, 34), |s| {
            (s.model.theme_opacity, s.model.bar_height as u16)
        });
        let d = match a.clone() {
            StudioAction::Nav(sec) => {
                if let Some(st) = &mut self.studio {
                    st.model.section = sec;
                    st.model.text_edit = None;
                    st.model.scroll = 0.0;
                    st.scroll_target = None;
                    st.s.focus = None;
                }
                Deferred::Nothing
            }
            StudioAction::Undo => {
                if let Some(st) = &mut self.studio
                    && let Some(prev) = st.undo.pop()
                {
                    st.redo.push(std::mem::replace(&mut st.model.draft, prev));
                    st.model.status = None;
                }
                Deferred::Nothing
            }
            StudioAction::Redo => {
                if let Some(st) = &mut self.studio
                    && let Some(next) = st.redo.pop()
                {
                    st.undo.push(std::mem::replace(&mut st.model.draft, next));
                    st.model.status = None;
                }
                Deferred::Nothing
            }
            StudioAction::EditBottomBar(bottom) => {
                if let Some(st) = &mut self.studio
                    && st.model.draft.editing_bottom != bottom
                {
                    // The draft, the applied copy and the history all swap,
                    // so Undo and "not applied" stay correct.
                    st.model.draft.swap_bars();
                    st.model.applied.swap_bars();
                    for d in st.undo.iter_mut().chain(st.redo.iter_mut()) {
                        d.swap_bars();
                    }
                }
                Deferred::Nothing
            }
            StudioAction::Revert => {
                let mut applied = self.current_draft();
                if self
                    .studio
                    .as_ref()
                    .is_some_and(|s| s.model.draft.editing_bottom)
                {
                    applied.swap_bars();
                }
                self.edit(|d| {
                    *d = applied;
                    true
                });
                let msg = self.tr(
                    "Changes discarded; the draft matches the bar again.",
                    "변경 사항을 버렸습니다. 초안이 다시 바와 같습니다.",
                );
                self.status(msg, Severity::Info);
                Deferred::Nothing
            }
            StudioAction::Apply => self.studio_apply(),
            StudioAction::ResetAll => Deferred::Confirm(Confirm::ResetAll),
            StudioAction::PreviewBackdrop(b) => {
                if let Some(st) = &mut self.studio {
                    st.model.preview_backdrop = b;
                }
                Deferred::Nothing
            }
            StudioAction::SelectModule(id) => {
                if let Some(st) = &mut self.studio {
                    st.model.selected_module = Some(id);
                }
                Deferred::Nothing
            }
            StudioAction::RemoveModule(id) => {
                self.edit(|d| {
                    apply_draft_action(d, &StudioAction::RemoveModule(id.clone()), op, th)
                });
                if let Some(st) = &mut self.studio {
                    st.model.selected_module = None;
                }
                Deferred::Nothing
            }
            StudioAction::AddModule(k) => {
                let before = self
                    .studio
                    .as_ref()
                    .map_or(0, |s| s.model.draft.bar.modules.len());
                self.edit(|d| apply_draft_action(d, &StudioAction::AddModule(k), op, th));
                if let Some(st) = &mut self.studio
                    && st.model.draft.bar.modules.len() > before
                {
                    st.model.selected_module =
                        st.model.draft.bar.modules.last().map(|m| m.id.clone());
                }
                Deferred::Nothing
            }
            StudioAction::ToggleMonitor(i) => {
                let mon = self
                    .studio
                    .as_ref()
                    .and_then(|s| s.model.monitors.get(i).cloned());
                if let Some(mon) = mon {
                    self.edit(|d| {
                        toggle_monitor(&mut d.bar, &mon);
                        true
                    });
                }
                Deferred::Nothing
            }
            StudioAction::Appearance(v) => {
                let dark =
                    v.unwrap_or_else(|| !appearance::apps_use_light_theme().unwrap_or(false));
                if let Some(st) = &mut self.studio {
                    st.model.appearance = v;
                }
                self.studio_set_dark(dark);
                Deferred::Nothing
            }
            StudioAction::EditField(key) => {
                self.studio_begin_edit(&key);
                Deferred::Nothing
            }
            StudioAction::SelectTheme(id) => {
                self.studio_select_theme(&id);
                Deferred::Nothing
            }
            StudioAction::UseTheme(id) => {
                self.edit(|d| {
                    d.theme.active.clone_from(&id);
                    true
                });
                Deferred::Nothing
            }
            StudioAction::DuplicateTheme(id) => {
                self.studio_duplicate_theme(&id);
                Deferred::Nothing
            }
            StudioAction::DeleteTheme(id) => Deferred::Confirm(Confirm::DeleteTheme(id)),
            StudioAction::SaveTheme => {
                self.studio_save_theme();
                Deferred::Nothing
            }
            StudioAction::ThemeDark(dark) => {
                self.theme_edit(|e| e.dark = dark);
                Deferred::Nothing
            }
            StudioAction::ThemeBackdrop(b) => {
                self.theme_edit(|e| e.backdrop = b);
                Deferred::Nothing
            }
            StudioAction::ThemeOpacityDown | StudioAction::ThemeOpacityUp => {
                let up = a == StudioAction::ThemeOpacityUp;
                self.theme_edit(|e| {
                    e.opacity = (((e.opacity + if up { 0.05 } else { -0.05 }) * 100.0).round()
                        / 100.0)
                        .clamp(0.30, 1.0)
                });
                Deferred::Nothing
            }
            StudioAction::SelectPreset(i) => {
                if let Some(st) = &mut self.studio {
                    st.model.preset_selected = Some(i);
                    if let Some(n) = st.model.presets.get(i).and_then(|p| p.name.clone()) {
                        st.model.preset_name = n;
                    }
                }
                Deferred::Nothing
            }
            StudioAction::SavePreset => {
                self.studio_save_preset(None);
                Deferred::Nothing
            }
            StudioAction::LoadPreset => {
                self.studio_load_preset();
                Deferred::Nothing
            }
            StudioAction::DuplicatePreset => {
                self.studio_duplicate_preset();
                Deferred::Nothing
            }
            StudioAction::RenamePreset => {
                self.studio_rename_preset();
                Deferred::Nothing
            }
            StudioAction::DeletePreset => match self.selected_preset_path() {
                Some(p) => Deferred::Confirm(Confirm::DeletePreset(p)),
                None => Deferred::Nothing,
            },
            StudioAction::ImportPreset => Deferred::OpenFile,
            StudioAction::ExportPreset => {
                let name = self
                    .selected_preset_path()
                    .and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()))
                    .unwrap_or_else(|| "preset.toml".into());
                Deferred::SaveFile(name)
            }
            StudioAction::Card(g, i) => {
                let fonts = self
                    .studio
                    .as_ref()
                    .map(|s| s.model.fonts.clone())
                    .unwrap_or_default();
                let (background, accent, text) = self.studio_colors();
                let c = catalog::Ctx {
                    background,
                    accent,
                    text,
                    fonts: &fonts,
                };
                self.edit(|d| catalog::apply(g, i, d, &c));
                if g == Gallery::Background && i == 7 {
                    // "Custom colour": go and pick the background colour.
                    return self.studio_action(StudioAction::GoColor(ColorTarget::Background));
                }
                Deferred::Nothing
            }
            StudioAction::ThemeCard(id, mode) => {
                self.edit(|d| apply_theme_card(d, &id, mode));
                self.studio_select_theme(&id);
                Deferred::Nothing
            }
            StudioAction::ColorTarget(t) => {
                if let Some(st) = &mut self.studio {
                    st.model.color_target = t;
                    st.model.text_edit = None;
                }
                Deferred::Nothing
            }
            StudioAction::GoColor(t) => {
                if let Some(st) = &mut self.studio {
                    st.model.section = Section::Colors;
                    st.model.color_target = t;
                    st.model.scroll = 0.0;
                    st.s.focus = None;
                }
                Deferred::Nothing
            }
            StudioAction::SetColor(v) => {
                self.set_color(v);
                Deferred::Nothing
            }
            StudioAction::ColorChannel(ch, delta) => {
                let c = self.edited_color();
                let adj = |v: u8| (i16::from(v) + delta).clamp(0, 255) as u8;
                let n = match ch {
                    0 => Color::rgb(adj(c.r), c.g, c.b),
                    1 => Color::rgb(c.r, adj(c.g), c.b),
                    _ => Color::rgb(c.r, c.g, adj(c.b)),
                };
                self.set_color(Some(hex(n)));
                Deferred::Nothing
            }
            StudioAction::PickColor => {
                let c = self.edited_color();
                Deferred::PickColor(u32::from(c.r) | (u32::from(c.g) << 8) | (u32::from(c.b) << 16))
            }
            StudioAction::FontFilter(ff) => {
                if let Some(st) = &mut self.studio {
                    st.model.font_filter = ff;
                    st.model.scroll = st.model.scroll.min(500.0);
                }
                Deferred::Nothing
            }
            StudioAction::NewTheme => {
                self.studio_new_theme();
                Deferred::Nothing
            }
            StudioAction::PreviewView(v) => {
                if let Some(st) = &mut self.studio {
                    st.model.preview_view = v;
                }
                Deferred::Nothing
            }
            other => {
                self.edit(|d| apply_draft_action(d, &other, op, th));
                Deferred::Nothing
            }
        };
        self.studio_rebuild();
        d
    }

    /// Background, accent and text of the draft as the bar draws them.
    fn studio_colors(&self) -> (Color, Color, Color) {
        let d = self.studio.as_ref().map(|s| s.model.draft.clone());
        let r = d.as_ref().and_then(|d| self.draft_theme(d));
        r.map_or(
            (
                Color::rgb(0x1B, 0x1D, 0x23),
                Color::rgb(0x5B, 0xA4, 0xE6),
                Color::WHITE,
            ),
            |r| {
                let s = PanelStyle::from_theme(&r, None, true);
                (
                    Color {
                        a: 255,
                        ..s.surface
                    },
                    s.accent,
                    s.text_primary,
                )
            },
        )
    }

    /// The colour of the role being edited, as the bar draws it.
    fn edited_color(&self) -> Color {
        let Some(st) = &self.studio else {
            return Color::BLACK;
        };
        if let Some(c) = st
            .model
            .color_target
            .get(&st.model.draft.bar.style)
            .as_deref()
            .and_then(col)
        {
            return c;
        }
        let (bg, accent, text) = self.studio_colors();
        match st.model.color_target {
            ColorTarget::Accent | ColorTarget::Focus => accent,
            ColorTarget::Background
            | ColorTarget::GradientStop(_)
            | ColorTarget::ModuleBackground => bg,
            _ => text,
        }
    }

    /// Result of the Windows colour dialog (`COLORREF`, `0x00BBGGRR`).
    pub fn studio_picked(&mut self, rgb: u32) {
        let c = Color::rgb(rgb as u8, (rgb >> 8) as u8, (rgb >> 16) as u8);
        self.set_color(Some(hex(c)));
        self.studio_rebuild();
    }

    /// A custom theme made from the current design's colours (Phase 9):
    /// the active theme is duplicated, its palette for the current mode is
    /// rebuilt from the design's background, accent and text, validated and
    /// saved, and the draft switches to it.
    fn studio_new_theme(&mut self) {
        let Some(active) = self
            .studio
            .as_ref()
            .map(|s| s.model.draft.theme.active.clone())
        else {
            return;
        };
        let (bg, accent, text) = self.studio_colors();
        self.studio_duplicate_theme(&active);
        let Some(new_id) = self
            .studio
            .as_ref()
            .and_then(|s| s.model.theme_edit.as_ref().map(|e| e.id.clone()))
            .filter(|id| *id != active)
        else {
            return;
        };
        let dark = bg.relative_luminance() < 0.4;
        self.theme_edit(|e| {
            e.background = hex(bg);
            e.accent = hex(accent);
            e.text = hex(text);
            e.dark = dark;
        });
        self.studio_save_theme();
        let mode = if dark {
            windowsrice_core::config::ThemeModePreference::Dark
        } else {
            windowsrice_core::config::ThemeModePreference::Light
        };
        self.edit(|d| apply_theme_card(d, &new_id, mode));
    }

    fn studio_apply(&mut self) -> Deferred {
        self.studio_rebuild();
        let Some(st) = &self.studio else {
            return Deferred::Nothing;
        };
        if !st.model.blocking.is_empty() {
            let msg = st.model.blocking[0].clone();
            self.status(msg, Severity::Error);
            return Deferred::Nothing;
        }
        let draft = st.model.draft.clone();
        let old = self.current_draft();
        let mode = system_mode(&self.args);
        let lang = self.lang();
        let result: Result<bool, String> = match &mut self.theme {
            ThemeSource::Service(s) => match s.update_config(|c| {
                c.bar = draft.top().clone();
                c.bottom_bar = draft.bottom().cloned();
            }) {
                Err(e) => Err(e),
                Ok(_) if draft.theme != old.theme => {
                    match s.select(&draft.theme.active, draft.theme.mode, mode, &mut self.sink) {
                        Ok(_) => Ok(true),
                        Err(e) => {
                            // Roll back the bar settings so config.toml stays consistent.
                            let rb = s.update_config(|c| {
                                c.bar = old.top().clone();
                                c.bottom_bar = old.bottom().cloned();
                            });
                            Err(match (lang, rb) {
                                (Lang::En, Ok(_)) => format!(
                                    "Theme could not be applied ({e}); bar settings were rolled back."
                                ),
                                (Lang::En, Err(r)) => format!(
                                    "Theme could not be applied ({e}); rolling back the bar settings also failed ({r})."
                                ),
                                (Lang::Ko, Ok(_)) => format!(
                                    "테마를 적용하지 못했습니다 ({e}). 바 설정을 되돌렸습니다."
                                ),
                                (Lang::Ko, Err(r)) => format!(
                                    "테마를 적용하지 못했습니다 ({e}). 바 설정 되돌리기도 실패했습니다 ({r})."
                                ),
                            })
                        }
                    }
                }
                Ok(_) => Ok(true),
            },
            ThemeSource::Override { engine, config } => {
                let mut c = config.clone();
                c.bar = draft.top().clone();
                c.bottom_bar = draft.bottom().cloned();
                match c.validate() {
                    Err(e) => Err(e.to_string()),
                    Ok(()) => match engine.activate(
                        &draft.theme.active,
                        draft.theme.mode,
                        mode,
                        &mut self.sink,
                    ) {
                        Ok(_) => {
                            c.theme = draft.theme.clone();
                            *config = c;
                            Ok(false)
                        }
                        Err(e) => Err(e.to_string()),
                    },
                }
            }
        };
        match result {
            Ok(saved) => {
                self.stats
                    .config_saves
                    .push(format!("{:.2}s studio apply", self.stats.now_s()));
                self.restyle();
                self.scheduler
                    .reconfigure(&crate::controller::all_modules(self.config()));
                self.sync_bars("studio apply");
                for i in 0..self.bars.len() {
                    self.layout_bar(i, "studio apply");
                }
                self.rebuild_all("settings");
                // A short fade of the new design (on the compositor, no
                // redraws), when animations are on in windowsRICE and Windows.
                if self.style.fade_ms > 0 {
                    for b in &self.bars {
                        let _ = b.s.renderer.fade_from(0.35, self.style.fade_ms.min(200));
                    }
                    self.stats.popup_log.push(format!(
                        "{:.2}s studio apply fade {} ms",
                        self.stats.now_s(),
                        self.style.fade_ms.min(200)
                    ));
                }
                let mut applied = self.current_draft();
                if draft.editing_bottom {
                    applied.swap_bars();
                }
                // Confirm that the running bar now uses the new values: a
                // saved file alone does not prove that the bar changed.
                let expected = self.draft_theme(&draft).map(|r| {
                    PanelStyle::from_theme(
                        &r,
                        window::high_contrast().as_ref(),
                        window::animations_enabled(),
                    )
                });
                let mut differs: Vec<&str> = Vec::new();
                if applied.bar != draft.bar {
                    differs.push("bar settings");
                }
                if let Some(e) = &expected {
                    if self.theme.applied().map(|t| t.id.as_str()) != Some(e.theme_id.as_str()) {
                        differs.push("theme");
                    }
                    if self.style.family != e.family
                        || (self.style.size_body - e.size_body).abs() > 0.01
                    {
                        differs.push("font");
                    }
                    if self.style.surface != e.surface
                        || self.style.text_primary != e.text_primary
                        || self.style.accent != e.accent
                    {
                        differs.push("colours");
                    }
                }
                let bars = self.bars.len();
                self.stats.popup_log.push(format!(
                    "{:.2}s studio apply verified: bars={bars} differs={differs:?} theme={} font={} {}",
                    self.stats.now_s(),
                    self.style.theme_id,
                    self.style.family,
                    self.style.size_body
                ));
                if let Some(st) = &mut self.studio {
                    st.model.applied = applied.clone();
                    st.model.draft = applied;
                }
                if differs.is_empty() {
                    let what = format!(
                        "{} · {} {}",
                        self.style.theme_name, self.style.family, self.style.size_body
                    );
                    let msg = match (lang, saved) {
                        (Lang::En, true) => {
                            format!("Applied and saved. Verified on {bars} bar(s): {what}.")
                        }
                        (Lang::En, false) => format!(
                            "Applied for this session only (--theme or --mode was given). Verified on {bars} bar(s): {what}."
                        ),
                        (Lang::Ko, true) => {
                            format!("적용하고 저장했습니다. 바 {bars}개에서 확인: {what}.")
                        }
                        (Lang::Ko, false) => format!(
                            "이번 실행에만 적용했습니다 (--theme 또는 --mode 사용 중). 바 {bars}개에서 확인: {what}."
                        ),
                    };
                    let title = if lang == Lang::En {
                        "Design applied"
                    } else {
                        "디자인 적용됨"
                    };
                    self.notice(title, &what);
                    self.status(msg, Severity::Success);
                } else {
                    self.stats
                        .errors
                        .push(format!("studio apply: bar differs in {differs:?}"));
                    let msg = match lang {
                        Lang::En => format!(
                            "Saved, but the bar does not show the new {} yet.",
                            differs.join(", ")
                        ),
                        Lang::Ko => format!(
                            "저장했지만 바에 새 {}이(가) 아직 보이지 않습니다.",
                            differs.join(", ")
                        ),
                    };
                    self.status(msg, Severity::Warning);
                }
            }
            Err(e) => {
                self.stats.errors.push(format!("studio apply: {e}"));
                let msg = match lang {
                    Lang::En => format!("Not applied: {e}"),
                    Lang::Ko => format!("적용하지 못했습니다: {e}"),
                };
                self.status(msg, Severity::Error);
            }
        }
        Deferred::Nothing
    }

    // ---------------------------------------------------------------- text fields

    fn field_value(&self, key: &str) -> String {
        let Some(st) = &self.studio else {
            return String::new();
        };
        let m = &st.model;
        let sel = m
            .selected_module
            .as_ref()
            .and_then(|id| m.draft.bar.modules.iter().find(|x| &x.id == id));
        let te = |f: fn(&ThemeEdit) -> String| m.theme_edit.as_ref().map(f).unwrap_or_default();
        if let Some(k) = slider_kind(key) {
            return k.text(k.get(&m.draft, &m.theme_defaults));
        }
        match key {
            "field:background" => m.draft.bar.style.background.clone().unwrap_or_default(),
            "field:accent" => m.draft.bar.style.accent.clone().unwrap_or_default(),
            "field:text" => m.draft.bar.style.text.clone().unwrap_or_default(),
            "field:color-hex" => m.color_target.get(&m.draft.bar.style).unwrap_or_default(),
            "field:font-search" => m.font_query.clone(),
            "field:module-text" => sel.and_then(|x| x.text.clone()).unwrap_or_default(),
            "field:module-label" => sel.and_then(|x| x.label.clone()).unwrap_or_default(),
            "field:theme-name" => te(|e| e.name.clone()),
            "field:theme-bg" => te(|e| e.background.clone()),
            "field:theme-accent" => te(|e| e.accent.clone()),
            "field:theme-text" => te(|e| e.text.clone()),
            "field:preset-name" => m.preset_name.clone(),
            "field:import-line" => self.import_line_text(),
            _ => String::new(),
        }
    }

    fn studio_begin_edit(&mut self, key: &str) {
        let v = self.field_value(key);
        if let Some(st) = &mut self.studio {
            st.model.text_edit = Some(TextEdit::new(key, &v));
            if let Some(id) = st
                .scene
                .as_ref()
                .and_then(|s| s.fields.iter().find(|(k, _)| k == key).map(|(_, e)| *e))
            {
                st.s.focus = Some(id);
            }
        }
    }

    /// Sets the colour of the role being edited and remembers it.
    fn set_color(&mut self, v: Option<String>) {
        let Some(target) = self.studio.as_ref().map(|s| s.model.color_target) else {
            return;
        };
        let val = v.clone();
        self.edit(|d| {
            target.set(&mut d.bar.style, val);
            true
        });
        if let (Some(h), Some(st)) = (v, self.studio.as_mut()) {
            st.model
                .recent_colors
                .retain(|x| !x.eq_ignore_ascii_case(&h));
            st.model.recent_colors.insert(0, h);
            st.model.recent_colors.truncate(12);
        }
    }

    /// Commits typed text to whatever the field edits.
    pub fn studio_commit(&mut self, key: &str, value: &str) {
        let v = value.trim().to_owned();
        let lang = self.lang();
        let bad_hex = |lang: Lang| {
            if lang == Lang::En {
                "Colours are written as #RRGGBB, for example #1B1D22."
            } else {
                "색은 #RRGGBB 형식으로 입력합니다. 예: #1B1D22"
            }
        };
        if let Some(k) = slider_kind(key) {
            let n: Option<f32> = v
                .trim_end_matches('%')
                .trim_start_matches('+')
                .trim()
                .parse()
                .ok();
            match n {
                Some(n) => {
                    let n = if k == SliderKind::Opacity {
                        n / 100.0
                    } else {
                        n
                    };
                    self.edit(|d| k.set(d, n));
                }
                None => self.status(
                    self.tr("Type a number.", "숫자를 입력하세요."),
                    Severity::Error,
                ),
            }
            return;
        }
        match key {
            "field:font-search" => {
                if let Some(st) = &mut self.studio {
                    st.model.font_query = v;
                    st.model.scroll = st.model.scroll.min(400.0);
                }
            }
            "field:color-hex" => {
                if v.is_empty() {
                    self.set_color(None);
                } else if valid_hex(&v) {
                    self.set_color(Some(v.to_uppercase()));
                } else {
                    self.status(bad_hex(lang), Severity::Error);
                }
            }
            "field:background" | "field:accent" | "field:text" => {
                if !v.is_empty() && !valid_hex(&v) {
                    self.status(bad_hex(lang), Severity::Error);
                    return;
                }
                let val = (!v.is_empty()).then(|| v.to_uppercase());
                self.edit(|d| {
                    let slot = match key {
                        "field:background" => &mut d.bar.style.background,
                        "field:accent" => &mut d.bar.style.accent,
                        _ => &mut d.bar.style.text,
                    };
                    *slot = val;
                    true
                });
            }
            "field:module-text" | "field:module-label" => {
                let id = self
                    .studio
                    .as_ref()
                    .and_then(|s| s.model.selected_module.clone());
                let n = v.chars().count();
                if key == "field:module-text" && !(1..=64).contains(&n) {
                    self.status(
                        self.tr(
                            "Custom text needs 1 to 64 characters.",
                            "사용자 텍스트는 1~64자여야 합니다.",
                        ),
                        Severity::Error,
                    );
                    return;
                }
                if let Some(id) = id {
                    self.edit(|d| {
                        if let Some(m) = d.bar.modules.iter_mut().find(|m| m.id == id) {
                            if key == "field:module-text" {
                                m.text = Some(v.clone());
                            } else {
                                m.label = (!v.is_empty()).then(|| v.clone());
                            }
                        }
                        true
                    });
                }
            }
            "field:theme-name" => self.theme_edit(|e| e.name.clone_from(&v)),
            "field:theme-bg" | "field:theme-accent" | "field:theme-text" => {
                if !valid_hex(&v) {
                    self.status(bad_hex(lang), Severity::Error);
                    return;
                }
                let up = v.to_uppercase();
                self.theme_edit(|e| match key {
                    "field:theme-bg" => e.background = up,
                    "field:theme-accent" => e.accent = up,
                    _ => e.text = up,
                });
            }
            "field:preset-name" => {
                if let Some(st) = &mut self.studio {
                    st.model.preset_name = v;
                }
            }
            "field:import-line" => self.import_replace_line(&v),
            _ => {}
        }
    }

    pub fn studio_char(&mut self, c: char) {
        let Some(st) = &mut self.studio else { return };
        let Some(te) = &mut st.model.text_edit else {
            return;
        };
        if c.is_control() {
            return;
        }
        let max = if te.key == "field:import-line" {
            400
        } else {
            64
        };
        te.insert(&c.to_string(), max);
        self.studio_rebuild();
    }

    fn end_edit(&mut self, commit: bool) {
        let te = self.studio.as_mut().and_then(|s| s.model.text_edit.take());
        if commit && let Some(te) = te {
            self.studio_commit(&te.key, &te.buffer);
        }
    }

    /// Keys while a text field has focus: caret, selection and clipboard.
    fn edit_key(&mut self, vk: u16, ctrl: bool, shift: bool) -> bool {
        let hwnd = self.studio_hwnd();
        let Some(te) = self
            .studio
            .as_mut()
            .and_then(|s| s.model.text_edit.as_mut())
        else {
            return false;
        };
        match (vk, ctrl) {
            (0x25, _) => te.left(shift),
            (0x27, _) => te.right(shift),
            (0x24, _) => te.home(shift),
            (0x23, _) => te.end(shift),
            (0x2E, _) => te.delete(),
            (v, _) if v == VK_BACK.0 => te.backspace(),
            (0x41, true) => te.select_all(),
            (0x43, true) => {
                if let Some(h) = hwnd {
                    set_clipboard(h, &te.selected_text());
                }
            }
            (0x58, true) => {
                let s = te.cut();
                if let Some(h) = hwnd {
                    set_clipboard(h, &s);
                }
            }
            (0x56, true) => {
                if let Some(s) = hwnd.and_then(get_clipboard) {
                    let max = if te.key == "field:import-line" {
                        400
                    } else {
                        64
                    };
                    te.insert(s.lines().next().unwrap_or(""), max);
                }
            }
            _ => return false,
        }
        true
    }

    // ---------------------------------------------------------------- keyboard and mouse

    pub fn studio_key(&mut self, vk: u16) -> Deferred {
        // SAFETY: GetKeyState has no preconditions.
        let ctrl = unsafe { GetKeyState(i32::from(VK_CONTROL.0)) } < 0;
        // SAFETY: as above.
        let shift = unsafe { GetKeyState(0x10) } < 0;
        if let Some(st) = &mut self.studio {
            st.s.pending_input = Some(Instant::now());
        }
        let editing = self
            .studio
            .as_ref()
            .is_some_and(|s| s.model.text_edit.is_some());
        if editing {
            match vk {
                v if v == VK_RETURN.0 => {
                    self.end_edit(true);
                    self.studio_rebuild();
                    return Deferred::Nothing;
                }
                v if v == VK_ESCAPE.0 => {
                    self.end_edit(false);
                    self.studio_rebuild();
                    return Deferred::Nothing;
                }
                v if v == VK_TAB.0 => self.end_edit(true),
                _ => {
                    if self.edit_key(vk, ctrl, shift) {
                        self.studio_rebuild();
                    }
                    return Deferred::Nothing;
                }
            }
        }
        if ctrl {
            let a = match vk {
                0x5A => Some(StudioAction::Undo),
                0x59 => Some(StudioAction::Redo),
                0x53 => Some(StudioAction::Apply),
                _ => None,
            };
            if let Some(a) = a {
                return self.studio_action(a);
            }
        }
        // Page Up / Page Down scroll the section.
        if vk == 0x21 || vk == 0x22 {
            let page = self
                .studio
                .as_ref()
                .and_then(|s| s.scene.as_ref().map(|sc| sc.viewport.h * 0.85))
                .unwrap_or(400.0);
            self.studio_scroll_by(if vk == 0x21 { -page } else { page });
            return Deferred::Nothing;
        }
        // Arrows on a slider change its value; on a gallery card they move
        // through the grid.
        if (matches!(vk, 0x25..=0x28) || vk == 0x24 || vk == 0x23)
            && (self.slider_key(vk) || self.grid_key(vk))
        {
            return Deferred::Nothing;
        }
        let Some(key) = map_key(vk) else {
            return Deferred::Nothing;
        };
        let outcome = {
            let Some(st) = &self.studio else {
                return Deferred::Nothing;
            };
            let s = st.s.host.state.borrow();
            handle_key_oriented(
                &s.scene.tree,
                st.s.focus,
                key,
                modifiers(),
                Orientation::Vertical,
            )
        };
        match outcome {
            KeyOutcome::Focus(id) => {
                self.studio_focus_element(id);
                Deferred::Nothing
            }
            KeyOutcome::Invoke(id)
            | KeyOutcome::Toggle(id)
            | KeyOutcome::Select(id)
            | KeyOutcome::Expand(id) => self.studio_activate(id),
            KeyOutcome::Ignored if key == Key::Enter => {
                // Enter on a text field starts editing.
                let a = self.studio.as_ref().and_then(|s| {
                    s.s.focus
                        .and_then(|f| s.scene.as_ref().and_then(|sc| sc.action(f).cloned()))
                });
                match a {
                    Some(a @ StudioAction::EditField(_)) => self.studio_action(a),
                    _ => Deferred::Nothing,
                }
            }
            _ => Deferred::Nothing,
        }
    }

    /// Moves keyboard focus, scrolling it into view and telling screen readers.
    fn studio_focus_element(&mut self, id: ElementId) {
        if let Some(st) = &mut self.studio {
            st.s.focus = Some(id);
        }
        self.studio_rebuild();
        if let Some(st) = &self.studio
            && crate::uia::raise_focus(&st.s.host, id)
        {
            self.stats.uia_focus_events += 1;
        }
    }

    pub fn studio_scroll_by(&mut self, dy: f32) {
        let animate = window::animations_enabled() && self.config().bar.animations;
        let Some(st) = &mut self.studio else { return };
        let max = st.scene.as_ref().map_or(0.0, |s| s.scroll_max);
        let from = st.scroll_target.unwrap_or(st.model.scroll);
        let next = (from + dy).clamp(0.0, max);
        if (next - st.model.scroll).abs() <= 0.1 {
            return;
        }
        if animate {
            // Ease towards the target on a short timer; the timer stops as
            // soon as the target is reached (nothing runs while idle).
            st.scroll_target = Some(next);
            // SAFETY: a timer on our own window.
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(
                    Some(st.s.hwnd),
                    SCROLL_TIMER,
                    15,
                    None,
                );
            }
        } else {
            st.model.scroll = next;
            self.studio_rebuild();
        }
    }

    /// `WM_TIMER` of the Studio window.
    pub fn studio_timer(&mut self, id: usize) {
        if id != SCROLL_TIMER {
            return;
        }
        let Some(st) = &mut self.studio else { return };
        let Some(target) = st.scroll_target else {
            // SAFETY: stops our own timer.
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::KillTimer(
                    Some(st.s.hwnd),
                    SCROLL_TIMER,
                );
            }
            return;
        };
        let diff = target - st.model.scroll;
        if diff.abs() < 0.75 {
            st.model.scroll = target;
            st.scroll_target = None;
            // SAFETY: stops our own timer.
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::KillTimer(
                    Some(st.s.hwnd),
                    SCROLL_TIMER,
                );
            }
        } else {
            st.model.scroll += diff * 0.4;
        }
        self.studio_rebuild();
    }

    /// Mouse wheel over the Studio (`delta` in WHEEL_DELTA units).
    pub fn studio_wheel(&mut self, delta: i16) {
        self.studio_scroll_by(-f32::from(delta) / 120.0 * 96.0);
    }

    fn focused_automation_id(&self) -> Option<String> {
        let st = self.studio.as_ref()?;
        let f = st.s.focus?;
        let s = st.s.host.state.borrow();
        s.scene.tree.get(f).map(|e| e.automation_id.to_owned())
    }

    fn slider_key(&mut self, vk: u16) -> bool {
        let Some(k) = self
            .focused_automation_id()
            .as_deref()
            .and_then(slider_kind)
        else {
            return false;
        };
        let Some(st) = &self.studio else { return false };
        let (lo, hi, step) = k.range();
        let v = k.get(&st.model.draft, &st.model.theme_defaults);
        let next = match vk {
            0x25 | 0x28 => v - step,
            0x27 | 0x26 => v + step,
            0x24 => lo,
            _ => hi,
        };
        self.edit(|d| k.set(d, next));
        self.studio_rebuild();
        true
    }

    fn grid_key(&mut self, vk: u16) -> bool {
        let Some(aid) = self.focused_automation_id() else {
            return false;
        };
        let Some((prefix, idx)) = aid
            .rsplit_once(':')
            .and_then(|(p, i)| i.parse::<usize>().ok().map(|i| (p.to_owned(), i)))
        else {
            return false;
        };
        let Some(grid) = self.studio.as_ref().and_then(|s| {
            s.scene
                .as_ref()
                .and_then(|sc| sc.grids.iter().find(|g| g.prefix == prefix).cloned())
        }) else {
            return false;
        };
        let n = grid.rects.len();
        let cols = grid.cols.max(1);
        let next = match vk {
            0x25 => idx.checked_sub(1),
            0x27 => (idx + 1 < n).then_some(idx + 1),
            0x26 => idx.checked_sub(cols),
            0x28 => (idx + cols < n).then_some(idx + cols),
            0x24 => Some(0),
            _ => n.checked_sub(1),
        };
        let Some(next) = next else { return true };
        // Scroll the card into view, then focus it.
        if let Some(st) = &mut self.studio
            && let Some(sc) = &st.scene
        {
            let r = grid.rects[next];
            let v = sc.viewport;
            if r.y < v.y {
                st.model.scroll = (st.model.scroll - (v.y - r.y) - 8.0).max(0.0);
            } else if r.bottom() > v.bottom() {
                st.model.scroll += r.bottom() - v.bottom() + 8.0;
            }
        }
        self.studio_rebuild();
        let key = format!("{prefix}:{next}");
        let id = self.studio.as_ref().and_then(|s| {
            let st = s.s.host.state.borrow();
            st.scene
                .tree
                .iter()
                .find(|e| e.automation_id == key)
                .map(|e| e.id)
        });
        if let Some(id) = id {
            self.studio_focus_element(id);
        }
        true
    }

    /// Pointer pressed on a slider track: set the value there and keep
    /// following the pointer until release.
    fn slider_at(&mut self, p: Point, start: bool) -> bool {
        let hit = self.studio.as_ref().and_then(|s| {
            let sc = s.scene.as_ref()?;
            match (start, s.slider_drag) {
                (true, _) => sc
                    .sliders
                    .iter()
                    .find(|x| x.track.inflate(10.0).contains(p))
                    .map(|x| (x.kind, x.track, x.element)),
                (false, Some(k)) => sc
                    .sliders
                    .iter()
                    .find(|x| x.kind == k)
                    .map(|x| (x.kind, x.track, x.element)),
                _ => None,
            }
        });
        let Some((k, track, el)) = hit else {
            return false;
        };
        let (lo, hi, _) = k.range();
        let f = ((p.x - track.x) / track.w).clamp(0.0, 1.0);
        let v = lo + f * (hi - lo);
        if start {
            self.edit(|d| k.set(d, v));
            if let Some(st) = &mut self.studio {
                st.slider_drag = Some(k);
                st.s.focus = Some(el);
            }
        } else if let Some(st) = &mut self.studio {
            // Dragging continues the same undo step.
            k.set(&mut st.model.draft, v);
        }
        self.studio_rebuild();
        true
    }

    fn studio_activate(&mut self, id: ElementId) -> Deferred {
        let (a, enabled) = {
            let Some(st) = &self.studio else {
                return Deferred::Nothing;
            };
            let a = st.scene.as_ref().and_then(|s| s.action(id).cloned());
            let en =
                st.s.host
                    .state
                    .borrow()
                    .scene
                    .tree
                    .get(id)
                    .is_some_and(|e| e.enabled);
            (a, en)
        };
        if let Some(st) = &mut self.studio {
            st.s.focus = Some(id);
        }
        match a {
            Some(a) if enabled => {
                // A click elsewhere ends text entry (keeping what was typed).
                if !matches!(a, StudioAction::EditField(_)) {
                    self.end_edit(true);
                }
                self.studio_action(a)
            }
            _ => Deferred::Nothing,
        }
    }

    fn studio_point(&self, x: i32, y: i32) -> Point {
        let s = self.studio.as_ref().map_or(1.0, |s| s.s.scale);
        Point {
            x: x as f32 / s,
            y: y as f32 / s,
        }
    }

    pub fn studio_mouse_down(&mut self, x: i32, y: i32) {
        let p = self.studio_point(x, y);
        if self.slider_at(p, true) {
            if let Some(st) = &self.studio {
                // SAFETY: capture the mouse for our own window while dragging.
                unsafe {
                    SetCapture(st.s.hwnd);
                }
            }
            return;
        }
        let Some(st) = &mut self.studio else { return };
        let hit = st.s.tree_hit(x, y);
        st.press = hit.and_then(|h| {
            st.scene.as_ref().and_then(|s| {
                s.draggable
                    .iter()
                    .any(|(e, _, _)| *e == h)
                    .then_some((p.x, p.y, h))
            })
        });
        if st.press.is_some() {
            // SAFETY: capture the mouse for our own window while dragging.
            unsafe {
                SetCapture(st.s.hwnd);
            }
        }
    }

    pub fn studio_mouse_move(&mut self, x: i32, y: i32) {
        let p = self.studio_point(x, y);
        if self
            .studio
            .as_ref()
            .is_some_and(|s| s.slider_drag.is_some())
        {
            self.slider_at(p, false);
            return;
        }
        let Some(st) = &mut self.studio else { return };
        // Start or update a drag.
        if let Some((px, py, el)) = st.press {
            let moved = (p.x - px).abs() + (p.y - py).abs() > 6.0;
            if moved || st.model.drag.is_some() {
                let src = st
                    .scene
                    .as_ref()
                    .and_then(|s| s.draggable.iter().find(|(e, _, _)| *e == el).cloned());
                if let Some((_, module, kind)) = src {
                    let target = st.scene.as_ref().and_then(|s| s.drop_target(p.x, p.y));
                    st.model.drag = Some(Drag {
                        module,
                        kind,
                        x: p.x,
                        y: p.y,
                        target,
                    });
                    self.studio_rebuild();
                    return;
                }
            }
        }
        let hit =
            st.s.tree_hit(x, y)
                .filter(|h| st.scene.as_ref().is_some_and(|s| s.action(*h).is_some()));
        if hit != st.s.hover {
            st.s.hover = hit;
            let hwnd = st.s.hwnd;
            track_leave(hwnd);
            self.studio_rebuild();
        }
    }

    pub fn studio_mouse_up(&mut self, x: i32, y: i32) -> Deferred {
        let Some(st) = &mut self.studio else {
            return Deferred::Nothing;
        };
        st.s.pending_input = Some(Instant::now());
        if st.slider_drag.take().is_some() {
            // SAFETY: releases our capture.
            unsafe {
                let _ = ReleaseCapture();
            }
            return Deferred::Nothing;
        }
        let press = st.press.take();
        // SAFETY: releases our capture.
        unsafe {
            let _ = ReleaseCapture();
        }
        if let Some(drag) = st.model.drag.take() {
            if let Some((region, index)) = drag.target {
                let (op, th) = (st.model.theme_opacity, st.model.bar_height as u16);
                match drag.module {
                    Some(id) => self.edit(|d| {
                        let Some(i) = d.bar.modules.iter().position(|m| m.id == id) else {
                            return false;
                        };
                        // Index among the region's other modules after removal.
                        let same_before = d.bar.modules[..i]
                            .iter()
                            .filter(|m| m.region == region)
                            .count();
                        let idx = if d.bar.modules[i].region == region && index > same_before {
                            index - 1
                        } else {
                            index
                        };
                        move_module(&mut d.bar, i, region, idx);
                        true
                    }),
                    None => {
                        let before = self
                            .studio
                            .as_ref()
                            .map_or(0, |s| s.model.draft.bar.modules.len());
                        self.edit(|d| {
                            if !apply_draft_action(d, &StudioAction::AddModule(drag.kind), op, th) {
                                return false;
                            }
                            let i = d.bar.modules.len() - 1;
                            move_module(&mut d.bar, i, region, index);
                            true
                        });
                        if let Some(st) = &mut self.studio
                            && st.model.draft.bar.modules.len() > before
                        {
                            st.model.selected_module = st
                                .model
                                .draft
                                .bar
                                .modules
                                .iter()
                                .rev()
                                .find(|m| m.kind == drag.kind)
                                .map(|m| m.id.clone());
                        }
                    }
                }
                self.stats.popup_log.push(format!(
                    "{:.2}s studio drop {:?} {index}",
                    self.stats.now_s(),
                    region
                ));
            }
            self.studio_rebuild();
            return Deferred::Nothing;
        }
        let hit = st.s.tree_hit(x, y);
        let _ = press;
        match hit {
            Some(id) => self.studio_activate(id),
            None => Deferred::Nothing,
        }
    }

    pub fn studio_mouse_leave(&mut self) {
        if let Some(st) = &mut self.studio
            && st.s.hover.take().is_some()
        {
            self.studio_rebuild();
        }
    }

    pub fn studio_focus(&mut self, gained: bool) {
        if let Some(st) = &mut self.studio {
            st.s.keyboard_active = gained;
            if gained && st.s.focus.is_none() {
                st.s.focus =
                    st.s.host
                        .state
                        .borrow()
                        .scene
                        .tree
                        .focus_order()
                        .first()
                        .copied();
            }
        }
        self.studio_rebuild();
    }

    pub fn studio_uia(&mut self, action: crate::uia::UiaAction, id: ElementId) -> Deferred {
        if action == crate::uia::UiaAction::SetValue {
            let pending = self
                .studio
                .as_ref()
                .and_then(|s| s.s.host.pending_value.borrow_mut().take());
            if let Some((eid, value)) = pending {
                let key = self.studio.as_ref().and_then(|s| {
                    s.scene.as_ref().and_then(|sc| {
                        sc.fields
                            .iter()
                            .find(|(_, e)| *e == eid)
                            .map(|(k, _)| k.clone())
                    })
                });
                if let Some(k) = key {
                    self.studio_commit(&k, &value);
                    self.studio_rebuild();
                }
            }
            return Deferred::Nothing;
        }
        if action == crate::uia::UiaAction::Focus {
            if let Some(st) = &mut self.studio {
                st.s.focus = Some(id);
            }
            self.studio_rebuild();
            return Deferred::Nothing;
        }
        self.studio_activate(id)
    }

    // ---------------------------------------------------------------- themes

    fn theme_edit(&mut self, f: impl FnOnce(&mut ThemeEdit)) {
        if let Some(e) = self
            .studio
            .as_mut()
            .and_then(|s| s.model.theme_edit.as_mut())
        {
            f(e);
            e.dirty = true;
        }
        self.validate_theme_edit();
    }

    fn studio_select_theme(&mut self, id: &str) {
        let edit = self
            .theme
            .catalog()
            .get(id)
            .filter(|e| !e.is_builtin())
            .map(|e| {
                let dark = e.theme.palette.dark.is_some();
                let p = e
                    .theme
                    .palette
                    .get(if dark { Mode::Dark } else { Mode::Light })
                    .cloned()
                    .unwrap_or_else(|| {
                        e.theme
                            .palette
                            .light
                            .clone()
                            .expect("a theme has a palette")
                    });
                ThemeEdit {
                    id: id.to_owned(),
                    name: e.theme.meta.name.clone(),
                    dark,
                    background: hex(p.background),
                    accent: hex(p.accent),
                    text: hex(p.text_primary),
                    backdrop: e.theme.materials.backdrop,
                    opacity: e.theme.materials.surface_opacity,
                    errors: Vec::new(),
                    warnings: Vec::new(),
                    dirty: false,
                }
            });
        if let Some(st) = &mut self.studio {
            st.model.theme_selected = Some(id.to_owned());
            st.model.theme_edit = edit;
        }
        self.validate_theme_edit();
    }

    /// Builds the edited theme and validates it with the theme rules.
    fn edited_theme(&self) -> Option<ThemeCheck> {
        let ed = self.studio.as_ref()?.model.theme_edit.clone()?;
        let e = self.theme.catalog().get(&ed.id)?;
        let mut th = e.theme.clone();
        th.meta.name = ed.name.trim().to_owned();
        let (Some(bg), Some(ac), Some(tx)) = (col(&ed.background), col(&ed.accent), col(&ed.text))
        else {
            return Some(Err(vec![
                self.tr(
                    "Colours are written as #RRGGBB.",
                    "색은 #RRGGBB 형식입니다.",
                )
                .to_owned(),
            ]));
        };
        let mode = if ed.dark { Mode::Dark } else { Mode::Light };
        let base = th
            .palette
            .get(mode)
            .or(th.palette.dark.as_ref())
            .or(th.palette.light.as_ref())
            .cloned()
            .expect("a theme has a palette");
        let p = derive_palette(&base, bg, ac, tx);
        match mode {
            Mode::Dark => th.palette.dark = Some(p),
            Mode::Light => th.palette.light = Some(p),
        }
        if th.palette.get(th.meta.default_mode).is_none() {
            th.meta.default_mode = mode;
        }
        th.materials.backdrop = ed.backdrop;
        th.materials.surface_opacity = ed.opacity;
        let text = match th.to_toml_string() {
            Ok(t) => t,
            Err(e) => return Some(Err(vec![e.to_string()])),
        };
        Some(match parse_theme(&text) {
            Ok(v) => Ok((
                v.theme,
                v.report
                    .warnings()
                    .map(|i| format!("{}: {}", i.path, i.message))
                    .collect(),
            )),
            Err(ThemeError::Invalid(report)) => Err(report
                .errors()
                .map(|i| format!("{}: {}", i.path, i.message))
                .collect()),
            Err(e) => Err(vec![e.to_string()]),
        })
    }

    fn validate_theme_edit(&mut self) {
        let r = self.edited_theme();
        if let Some(e) = self
            .studio
            .as_mut()
            .and_then(|s| s.model.theme_edit.as_mut())
        {
            match r {
                Some(Ok((_, w))) => {
                    e.errors.clear();
                    e.warnings = w;
                }
                Some(Err(errs)) => {
                    e.errors = errs;
                    e.warnings.clear();
                }
                None => {}
            }
        }
    }

    fn reload_themes(&mut self) {
        let mode = system_mode(&self.args);
        let notices = match &mut self.theme {
            ThemeSource::Service(s) => s.reload_catalog(mode, &mut self.sink),
            ThemeSource::Override { .. } => Vec::new(),
        };
        self.notices
            .extend(notices.iter().map(crate::controller::notice_text));
        self.restyle();
        self.rebuild_all("theme files");
        self.studio_refresh_lists();
    }

    fn studio_duplicate_theme(&mut self, id: &str) {
        let Some(dir) = self.themes_dir() else {
            self.status(
                self.tr(
                    "No folder for custom themes is available.",
                    "사용자 테마 폴더를 사용할 수 없습니다.",
                ),
                Severity::Error,
            );
            return;
        };
        let Some(e) = self.theme.catalog().get(id) else {
            return;
        };
        let mut th = e.theme.clone();
        let base = slug(&th.meta.name);
        let taken: Vec<String> = self
            .theme
            .catalog()
            .entries()
            .iter()
            .map(|x| x.id().to_owned())
            .collect();
        let new_id = (1..100)
            .map(|n| format!("custom-{base}-{n}"))
            .find(|c| !taken.contains(c) && windowsrice_core::config::is_valid_theme_id(c))
            .unwrap_or_else(|| "custom-theme".into());
        let names: Vec<String> = self
            .theme
            .catalog()
            .entries()
            .iter()
            .map(|x| x.theme.meta.name.clone())
            .collect();
        let suffix = self.tr("copy", "사본");
        let new_name = (1..100)
            .map(|n| {
                if n == 1 {
                    format!("{} {suffix}", th.meta.name)
                } else {
                    format!("{} {suffix} {n}", th.meta.name)
                }
            })
            .find(|c| !names.contains(c))
            .unwrap_or_default();
        th.meta.id = new_id.clone();
        th.meta.name = new_name.clone();
        th.meta.author = "windowsRICE Design Studio".into();
        th.meta.version = "1.0.0".into();
        let text = match th.to_toml_string() {
            Ok(t) => t,
            Err(e) => {
                self.status(e.to_string(), Severity::Error);
                return;
            }
        };
        if let Err(e) = parse_theme(&text) {
            self.status(e.to_string(), Severity::Error);
            return;
        }
        let path = dir.join(format!("{new_id}.toml"));
        let write = std::fs::create_dir_all(&dir).and_then(|()| {
            let tmp = path.with_extension("toml.tmp");
            std::fs::write(&tmp, &text)?;
            std::fs::rename(&tmp, &path)
        });
        match write {
            Ok(()) => {
                self.reload_themes();
                self.studio_select_theme(&new_id);
                let msg = match self.lang() {
                    Lang::En => format!(
                        "Created {new_name}. Edit its colours on the right, then Save theme."
                    ),
                    Lang::Ko => format!(
                        "{new_name}을(를) 만들었습니다. 오른쪽에서 색을 바꾼 뒤 테마 저장을 누르세요."
                    ),
                };
                self.status(msg, Severity::Success);
            }
            Err(e) => self.status(format!("{}: {e}", path.display()), Severity::Error),
        }
    }

    fn studio_save_theme(&mut self) {
        let Some(id) = self
            .studio
            .as_ref()
            .and_then(|s| s.model.theme_edit.as_ref().map(|e| e.id.clone()))
        else {
            return;
        };
        let path = match self.theme.catalog().get(&id).map(|e| e.source.clone()) {
            Some(windowsrice_theme::catalog::ThemeSource::User { path }) => path,
            _ => return,
        };
        match self.edited_theme() {
            Some(Ok((th, _))) => {
                let text = match th.to_toml_string() {
                    Ok(t) => t,
                    Err(e) => {
                        self.status(e.to_string(), Severity::Error);
                        return;
                    }
                };
                let tmp = path.with_extension("toml.tmp");
                match std::fs::write(&tmp, &text).and_then(|()| std::fs::rename(&tmp, &path)) {
                    Ok(()) => {
                        self.reload_themes();
                        self.studio_select_theme(&id);
                        self.status(
                            self.tr(
                                "Theme saved. Bars using it were updated.",
                                "테마를 저장했습니다. 이 테마를 쓰는 바가 바뀌었습니다.",
                            ),
                            Severity::Success,
                        );
                    }
                    Err(e) => self.status(format!("{}: {e}", path.display()), Severity::Error),
                }
            }
            Some(Err(errs)) => {
                self.status(errs.first().cloned().unwrap_or_default(), Severity::Error)
            }
            None => {}
        }
    }

    fn delete_theme(&mut self, id: &str) {
        if self.config().theme.active == id
            || self
                .studio
                .as_ref()
                .is_some_and(|s| s.model.draft.theme.active == id)
        {
            self.status(
                self.tr(
                    "This theme is in use. Choose another theme first.",
                    "사용 중인 테마입니다. 먼저 다른 테마를 고르세요.",
                ),
                Severity::Error,
            );
            return;
        }
        if let Some(windowsrice_theme::catalog::ThemeSource::User { path }) =
            self.theme.catalog().get(id).map(|e| e.source.clone())
        {
            match std::fs::remove_file(&path) {
                Ok(()) => {
                    self.reload_themes();
                    if let Some(st) = &mut self.studio {
                        st.model.theme_selected = None;
                        st.model.theme_edit = None;
                    }
                    self.status(
                        self.tr("Theme deleted.", "테마를 삭제했습니다."),
                        Severity::Success,
                    );
                }
                Err(e) => self.status(format!("{}: {e}", path.display()), Severity::Error),
            }
        }
    }

    // ---------------------------------------------------------------- presets

    fn selected_preset_path(&self) -> Option<PathBuf> {
        let i = self.studio.as_ref()?.model.preset_selected?;
        self.preset_entries().1.get(i).cloned()
    }

    fn typed_preset_name(&self) -> Option<String> {
        let st = self.studio.as_ref()?;
        let n = st
            .model
            .text_edit
            .as_ref()
            .filter(|e| e.key == "field:preset-name")
            .map_or(st.model.preset_name.clone(), |e| e.buffer.clone());
        let n = n.trim().to_owned();
        (!n.is_empty()).then_some(n)
    }

    fn studio_save_preset(&mut self, name: Option<String>) {
        self.end_edit(true);
        let Some(name) = name.or_else(|| self.typed_preset_name()) else {
            self.status(
                self.tr(
                    "Type a name for the preset first.",
                    "먼저 프리셋 이름을 입력하세요.",
                ),
                Severity::Error,
            );
            return;
        };
        let Some(dir) = self.presets_dir() else {
            return;
        };
        let Some(st) = &self.studio else { return };
        let p = Preset::new(
            &name,
            st.model.draft.theme.clone(),
            st.model.draft.bar.clone(),
        );
        let path = dir.join(preset_file_name(&name));
        if path.exists() {
            let msg = match self.lang() {
                Lang::En => format!(
                    "A preset named {name:?} already exists. Choose another name or delete it first."
                ),
                Lang::Ko => {
                    format!("{name:?} 프리셋이 이미 있습니다. 다른 이름을 쓰거나 먼저 삭제하세요.")
                }
            };
            self.status(msg, Severity::Error);
            return;
        }
        match p.save(&path) {
            Ok(()) => {
                self.studio_refresh_lists();
                let msg = match self.lang() {
                    Lang::En => format!("Saved preset {name:?}."),
                    Lang::Ko => format!("{name:?} 프리셋을 저장했습니다."),
                };
                self.status(msg, Severity::Success);
            }
            Err(e) => self.status(e.to_string(), Severity::Error),
        }
    }

    fn studio_load_preset(&mut self) {
        let Some(path) = self.selected_preset_path() else {
            return;
        };
        match Preset::load(&path) {
            Ok(p) => {
                self.edit(|d| {
                    d.theme = p.theme.clone();
                    d.bar = p.bar.clone();
                    true
                });
                let msg = match self.lang() {
                    Lang::En => format!("Loaded {:?} into the draft. Apply to use it.", p.name),
                    Lang::Ko => format!(
                        "{:?}을(를) 초안으로 불러왔습니다. 적용해야 사용됩니다.",
                        p.name
                    ),
                };
                self.status(msg, Severity::Info);
            }
            Err(e) => self.status(e.to_string(), Severity::Error),
        }
    }

    fn studio_duplicate_preset(&mut self) {
        let Some(path) = self.selected_preset_path() else {
            return;
        };
        match Preset::load(&path) {
            Ok(p) => {
                let names: Vec<String> = self
                    .studio
                    .as_ref()
                    .map(|s| {
                        s.model
                            .presets
                            .iter()
                            .filter_map(|x| x.name.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                let suffix = self.tr("copy", "사본");
                let n = (1..100)
                    .map(|i| {
                        if i == 1 {
                            format!("{} {suffix}", p.name)
                        } else {
                            format!("{} {suffix} {i}", p.name)
                        }
                    })
                    .find(|c| !names.contains(c))
                    .unwrap_or_default();
                let Some(dir) = self.presets_dir() else {
                    return;
                };
                let mut q = p.clone();
                q.name.clone_from(&n);
                match q.save(&dir.join(preset_file_name(&n))) {
                    Ok(()) => {
                        self.studio_refresh_lists();
                        self.status(format!("{n:?}"), Severity::Success);
                    }
                    Err(e) => self.status(e.to_string(), Severity::Error),
                }
            }
            Err(e) => self.status(e.to_string(), Severity::Error),
        }
    }

    fn studio_rename_preset(&mut self) {
        self.end_edit(true);
        let (Some(path), Some(name)) = (self.selected_preset_path(), self.typed_preset_name())
        else {
            return;
        };
        let Some(dir) = self.presets_dir() else {
            return;
        };
        match Preset::load(&path) {
            Ok(mut p) => {
                let to = dir.join(preset_file_name(&name));
                if to != path && to.exists() {
                    self.status(
                        self.tr(
                            "Another preset already uses that name.",
                            "다른 프리셋이 이미 그 이름을 씁니다.",
                        ),
                        Severity::Error,
                    );
                    return;
                }
                p.name.clone_from(&name);
                match p.save(&to).and_then(|()| {
                    if to != path {
                        std::fs::remove_file(&path)
                            .map_err(|e| windowsrice_core::preset::PresetError::Io(e.to_string()))
                    } else {
                        Ok(())
                    }
                }) {
                    Ok(()) => {
                        self.studio_refresh_lists();
                        self.status(format!("{name:?}"), Severity::Success);
                    }
                    Err(e) => self.status(e.to_string(), Severity::Error),
                }
            }
            Err(e) => self.status(e.to_string(), Severity::Error),
        }
    }

    /// Validates an imported file and copies it into the presets folder.
    pub fn studio_import(&mut self, from: &Path) -> Deferred {
        match Preset::load(from) {
            Err(e) => {
                let msg = match self.lang() {
                    Lang::En => format!("Not imported: {e}"),
                    Lang::Ko => format!("가져오지 못했습니다: {e}"),
                };
                self.status(msg, Severity::Error);
                Deferred::Nothing
            }
            Ok(p) => {
                let Some(dir) = self.presets_dir() else {
                    return Deferred::Nothing;
                };
                let to = dir.join(preset_file_name(&p.name));
                if to.exists() {
                    return Deferred::Confirm(Confirm::OverwritePreset {
                        from: from.to_path_buf(),
                        to,
                    });
                }
                self.finish_import(&p, &to);
                Deferred::Nothing
            }
        }
    }

    fn finish_import(&mut self, p: &Preset, to: &Path) {
        match p.save(to) {
            Ok(()) => {
                self.studio_refresh_lists();
                let msg = match self.lang() {
                    Lang::En => format!(
                        "Imported {:?}. Select it and choose Load to preview it.",
                        p.name
                    ),
                    Lang::Ko => format!(
                        "{:?}을(를) 가져왔습니다. 선택하고 불러오기를 누르면 미리 볼 수 있습니다.",
                        p.name
                    ),
                };
                self.status(msg, Severity::Success);
            }
            Err(e) => self.status(e.to_string(), Severity::Error),
        }
    }

    pub fn studio_export(&mut self, to: &Path) {
        let Some(path) = self.selected_preset_path() else {
            return;
        };
        match Preset::load(&path).and_then(|p| p.save(to)) {
            Ok(()) => {
                let msg = match self.lang() {
                    Lang::En => format!("Exported to {}.", to.display()),
                    Lang::Ko => format!("{}(으)로 내보냈습니다.", to.display()),
                };
                self.status(msg, Severity::Success);
            }
            Err(e) => self.status(e.to_string(), Severity::Error),
        }
        self.studio_rebuild();
    }

    /// Result of a confirmation (`true` = Yes; `None` = Cancel).
    pub fn studio_confirmed(&mut self, c: &Confirm, answer: Option<bool>) {
        match (c, answer) {
            (Confirm::DeleteTheme(id), Some(true)) => self.delete_theme(id),
            (Confirm::DeletePreset(p), Some(true)) => match std::fs::remove_file(p) {
                Ok(()) => {
                    if let Some(st) = &mut self.studio {
                        st.model.preset_selected = None;
                    }
                    self.studio_refresh_lists();
                    self.status(
                        self.tr("Preset deleted.", "프리셋을 삭제했습니다."),
                        Severity::Success,
                    );
                }
                Err(e) => self.status(e.to_string(), Severity::Error),
            },
            (Confirm::OverwritePreset { from, to }, Some(true)) => {
                if let Ok(p) = Preset::load(from) {
                    self.finish_import(&p, to);
                }
            }
            (Confirm::ResetAll, Some(true)) => {
                self.edit(|d| {
                    d.theme = ThemeSelection::default();
                    d.bar = BarConfig::default();
                    true
                });
                self.status(
                    self.tr(
                        "Everything was reset to the defaults in the draft. Apply to use it.",
                        "초안을 모두 기본값으로 재설정했습니다. 적용해야 사용됩니다.",
                    ),
                    Severity::Info,
                );
            }
            (Confirm::UnsavedClose, Some(true)) => {
                self.studio_apply();
                if self
                    .studio
                    .as_ref()
                    .is_some_and(|s| s.model.draft == s.model.applied)
                {
                    self.studio_close();
                }
                return;
            }
            (Confirm::UnsavedClose, Some(false)) => {
                self.studio_close();
                return;
            }
            _ => {}
        }
        self.studio_rebuild();
    }

    pub fn confirm_text(&self, c: &Confirm) -> (String, bool) {
        let lang = self.lang();
        match c {
            Confirm::DeleteTheme(id) => {
                let name = self.theme.catalog().get(id).map_or(id.clone(), |e| e.theme.meta.name.clone());
                (match lang { Lang::En => format!("Delete the theme {name:?}? Its file is removed and cannot be restored."), Lang::Ko => format!("{name:?} 테마를 삭제할까요? 파일이 삭제되며 되돌릴 수 없습니다.") }, false)
            }
            Confirm::DeletePreset(p) => (match lang { Lang::En => format!("Delete the preset file {}?", p.display()), Lang::Ko => format!("프리셋 파일 {}을(를) 삭제할까요?", p.display()) }, false),
            Confirm::OverwritePreset { to, .. } => (match lang { Lang::En => format!("A preset with this name exists ({}). Replace it with the imported one?", to.display()), Lang::Ko => format!("같은 이름의 프리셋이 있습니다 ({}). 가져온 프리셋으로 바꿀까요?", to.display()) }, false),
            Confirm::ResetAll => (self.tr("Reset the theme and every bar setting in the draft to the defaults? Nothing changes until you apply.", "초안의 테마와 모든 바 설정을 기본값으로 재설정할까요? 적용하기 전에는 아무것도 바뀌지 않습니다.").to_owned(), false),
            Confirm::UnsavedClose => (self.tr("Apply your changes before closing Design Studio?\n\nYes applies them, No discards them, Cancel keeps the Studio open.", "디자인 스튜디오를 닫기 전에 변경 사항을 적용할까요?\n\n예: 적용, 아니요: 버림, 취소: 계속 편집").to_owned(), true),
        }
    }

    pub fn studio_hwnd(&self) -> Option<HWND> {
        self.studio.as_ref().map(|s| s.s.hwnd)
    }

    pub fn studio_dpi_changed(&mut self, suggested: Option<RECT>) {
        if let (Some(st), Some(r)) = (&self.studio, suggested) {
            // SAFETY: moves our own window to the rectangle Windows suggests.
            unsafe {
                let _ = SetWindowPos(
                    st.s.hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
        self.studio_resize();
    }
}

impl StudioWin {
    /// Client size in physical pixels.
    pub fn s_size(&self) -> (i32, i32) {
        let mut r = RECT::default();
        // SAFETY: reads our own client rectangle.
        let _ = unsafe { GetClientRect(self.s.hwnd, &raw mut r) };
        (r.right - r.left, r.bottom - r.top)
    }

    /// Client origin in screen coordinates (for UI Automation bounds).
    pub fn client_origin(&self) -> (i32, i32) {
        let mut p = windows::Win32::Foundation::POINT::default();
        // SAFETY: converts a point for our own window.
        let _ = unsafe { windows::Win32::Graphics::Gdi::ClientToScreen(self.s.hwnd, &raw mut p) };
        (p.x, p.y)
    }
}

/// Runs a confirmation outside the controller borrow.
pub fn run_confirm(c: Confirm) {
    let Some((owner, (text, three))) =
        crate::controller::with_ctl(|ctl| (ctl.studio_hwnd(), ctl.confirm_text(&c)))
            .and_then(|(h, t)| h.map(|h| (h, t)))
    else {
        return;
    };
    let caption = "windowsRICE Design Studio";
    let r = if three {
        message_box(owner, &text, caption, MB_YESNOCANCEL | MB_ICONQUESTION)
    } else {
        message_box(owner, &text, caption, MB_YESNO | MB_ICONWARNING)
    };
    let answer = if r == IDCANCEL.0 {
        None
    } else {
        Some(r == IDYES.0)
    };
    crate::controller::with_ctl(|ctl| ctl.studio_confirmed(&c, answer));
}

pub fn run_open_file() {
    let Some(owner) = crate::controller::with_ctl(|c| c.studio_hwnd()).flatten() else {
        return;
    };
    if let Some(path) = open_dialog(owner, "Import windowsRICE preset") {
        let next = crate::controller::with_ctl(|c| c.studio_import(&path));
        if let Some(Deferred::Confirm(c)) = next {
            run_confirm(c);
        }
        crate::controller::with_ctl(Controller::studio_rebuild);
    }
}

pub fn run_save_file(name: &str) {
    let Some(owner) = crate::controller::with_ctl(|c| c.studio_hwnd()).flatten() else {
        return;
    };
    if let Some(path) = save_dialog(owner, "Export windowsRICE preset", name) {
        crate::controller::with_ctl(|c| c.studio_export(&path));
    }
}

/// The slider a field key such as `slider:Height` refers to.
fn slider_kind(key: &str) -> Option<SliderKind> {
    let name = key.strip_prefix("slider:")?;
    [
        SliderKind::Opacity,
        SliderKind::ModuleOpacity,
        SliderKind::Height,
        SliderKind::Spacing,
        SliderKind::Padding,
        SliderKind::RegionGap,
        SliderKind::FontSize,
        SliderKind::ClockSize,
        SliderKind::IconSize,
        SliderKind::BorderWidth,
        SliderKind::SeparatorWidth,
        SliderKind::Radius,
        SliderKind::Shadow,
        SliderKind::Highlight,
        SliderKind::Hover,
    ]
    .into_iter()
    .find(|k| format!("{k:?}") == name)
}

/// Unicode text on the clipboard, if any.
fn get_clipboard(owner: HWND) -> Option<String> {
    use windows::Win32::Foundation::{HANDLE, HGLOBAL};
    use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    const CF_UNICODETEXT: u32 = 13;
    // SAFETY: standard clipboard read: open, lock the global text, copy it
    // out up to the terminating NUL, unlock and close.
    unsafe {
        OpenClipboard(Some(owner)).ok()?;
        let out = (|| {
            let h: HANDLE = GetClipboardData(CF_UNICODETEXT).ok()?;
            let g = HGLOBAL(h.0);
            let p = GlobalLock(g) as *const u16;
            if p.is_null() {
                return None;
            }
            let mut n = 0usize;
            while *p.add(n) != 0 && n < 4096 {
                n += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
            let _ = GlobalUnlock(g);
            Some(s)
        })();
        let _ = CloseClipboard();
        out
    }
}

/// Puts `text` on the clipboard.
/// Clipboard text for the Import page.
pub(crate) fn clipboard_text(owner: HWND) -> Option<String> {
    get_clipboard(owner)
}

/// Puts text on the clipboard (Copy design code).
pub(crate) fn clipboard_set(owner: HWND, text: &str) {
    set_clipboard(owner, text);
}

fn set_clipboard(owner: HWND, text: &str) {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
    const CF_UNICODETEXT: u32 = 13;
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: standard clipboard write; after SetClipboardData succeeds
    // the system owns the memory.
    unsafe {
        if OpenClipboard(Some(owner)).is_err() {
            return;
        }
        let _ = EmptyClipboard();
        if let Ok(g) = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) {
            let p = GlobalLock(g) as *mut u16;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
                let _ = GlobalUnlock(g);
                let _ = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(g.0)));
            }
        }
        let _ = CloseClipboard();
    }
}

/// Shows the Windows colour dialog, outside the controller borrow.
pub fn run_pick_color(initial: u32) {
    use windows::Win32::Foundation::COLORREF;
    use windows::Win32::UI::Controls::Dialogs::{
        CC_ANYCOLOR, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW, ChooseColorW,
    };
    let Some(owner) = crate::controller::with_ctl(|c| c.studio_hwnd()).flatten() else {
        return;
    };
    let mut custom = [COLORREF(0x00FF_FFFF); 16];
    let mut cc = CHOOSECOLORW {
        lStructSize: size_of::<CHOOSECOLORW>() as u32,
        hwndOwner: owner,
        rgbResult: COLORREF(initial),
        lpCustColors: custom.as_mut_ptr(),
        Flags: CC_RGBINIT | CC_FULLOPEN | CC_ANYCOLOR,
        ..Default::default()
    };
    // SAFETY: modal dialog owned by our window; the structure and the
    // custom colour array outlive the call.
    if unsafe { ChooseColorW(&raw mut cc) }.as_bool() {
        let rgb = cc.rgbResult.0;
        crate::controller::with_ctl(|c| c.studio_picked(rgb));
    }
}

/// Installed font families for the Studio, filled once per process.
static FONT_CACHE: std::sync::Mutex<Option<Vec<FontEntry>>> = std::sync::Mutex::new(None);

fn to_entries(list: Vec<windowsrice_render::InstalledFamily>) -> Vec<FontEntry> {
    list.into_iter()
        .filter(|f| !f.symbol)
        .map(|f| FontEntry {
            family: f.name,
            hangul: f.hangul,
            weights: f.weights,
            mono: f.mono,
        })
        .collect()
}

/// Reads the installed font families on a background thread (opening
/// font files can take a second when they are not cached by Windows), so
/// the Design Studio opens without waiting.
pub fn prefetch_fonts() {
    let _ = std::thread::Builder::new()
        .name("font-discovery".into())
        .spawn(|| {
            crate::stats::mark("font_prefetch_started");
            let config = windowsrice_render::FontConfig {
                family: "Segoe UI Variable".into(),
                fallback: vec!["Malgun Gothic".into()],
                icon_family: "Segoe Fluent Icons".into(),
                icon_fallback: "Segoe MDL2 Assets".into(),
                locale: "en-US".into(),
            };
            if let Ok(t) = TextSystem::new(config) {
                let list = to_entries(t.installed_families());
                if let Ok(mut c) = FONT_CACHE.lock()
                    && c.is_none()
                {
                    *c = Some(list);
                }
            }
            crate::stats::mark("font_prefetch_done");
        });
}
