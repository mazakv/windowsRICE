//! Design Studio: RICE Design Code and `.ricepack` import and export, and
//! "Create custom design" (Phase 12).
//!
//! Imported content is data only. It is analysed by
//! `windowsrice_core::design_code` (and, for packages, checked by
//! `windowsrice_core::design_pack`) before it can be previewed, saved or
//! applied, and it never replaces the applied design by itself: Preview
//! loads it into the Studio draft (Undo returns) and only Apply writes it.
//! An optional Microsoft Defender scan of an opened package is offered when
//! Defender's command-line scanner is present; its result is reported as
//! such and never as a guarantee.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FileOpenDialog, FileSaveDialog, IFileOpenDialog, IFileSaveDialog, SIGDN_FILESYSPATH,
};
use windows::core::{HSTRING, w};
use windowsrice_core::bar::Region;
use windowsrice_core::config::ThemeSelection;
use windowsrice_core::design_code::{
    AnalyzeOptions, AssetKind, AssetRef, CheckStatus, DesignCode, Diagnostic, MAX_DESIGN_BYTES,
    Severity as DSev, analyze,
};
use windowsrice_core::design_pack::{self, MAX_PACKAGE_BYTES, Package};
use windowsrice_core::preset::{Preset, preset_file_name};
use windowsrice_ui::strings::Lang;
use windowsrice_ui::studio::catalog::template_modules;
use windowsrice_ui::studio::{ImportCheck, ImportView, Section, Severity, StudioAction};

use crate::controller::{Controller, Deferred};

/// A package opened on the Import page.
#[derive(Debug)]
pub struct OpenPackage {
    pub path: PathBuf,
    pub pkg: Package,
    /// Microsoft Defender scan of the package file, once run.
    pub scan: Option<ScanResult>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanResult {
    /// Defender reported no threat in the file.
    NoThreat,
    /// Defender reported a threat.
    Threat,
    /// The scan could not run (reason).
    Unavailable(String),
}

/// Runs Microsoft Defender's command-line scanner on one file, when it is
/// installed. Read-only: remediation is disabled, nothing is quarantined or
/// changed by windowsRICE.
pub fn defender_scan(file: &Path) -> ScanResult {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let Some(pf) = std::env::var_os("ProgramFiles") else {
        return ScanResult::Unavailable("Program Files is unknown".into());
    };
    let exe = Path::new(&pf).join("Windows Defender").join("MpCmdRun.exe");
    if !exe.exists() {
        return ScanResult::Unavailable("Microsoft Defender's scanner is not installed".into());
    }
    let child = std::process::Command::new(&exe)
        .args(["-Scan", "-ScanType", "3", "-DisableRemediation", "-File"])
        .arg(file)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return ScanResult::Unavailable(format!("could not start the scanner: {e}")),
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return match status.code() {
                    Some(0) => ScanResult::NoThreat,
                    Some(2) => ScanResult::Threat,
                    Some(c) => ScanResult::Unavailable(format!(
                        "the scanner ended with code {c} (Defender may not be the active antivirus)"
                    )),
                    None => ScanResult::Unavailable("the scanner was stopped".into()),
                };
            }
            Ok(None) if start.elapsed() < Duration::from_secs(120) => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Ok(None) => {
                let _ = child.kill();
                return ScanResult::Unavailable("the scan took longer than 2 minutes".into());
            }
            Err(e) => return ScanResult::Unavailable(e.to_string()),
        }
    }
}

fn status_check(s: CheckStatus) -> ImportCheck {
    match s {
        CheckStatus::Passed => ImportCheck::Passed,
        CheckStatus::Failed => ImportCheck::Failed,
        CheckStatus::NotRun => ImportCheck::NotRun,
    }
}

fn open_design_dialog(owner: HWND, title: &str) -> Option<PathBuf> {
    // SAFETY: the common item dialog on our STA thread; strings outlive the calls.
    unsafe {
        let d: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let filters = [
            COMDLG_FILTERSPEC {
                pszName: w!("windowsRICE design (*.ricepack; *.json)"),
                pszSpec: w!("*.ricepack;*.json;*.txt"),
            },
            COMDLG_FILTERSPEC {
                pszName: w!("Design package (*.ricepack)"),
                pszSpec: w!("*.ricepack"),
            },
        ];
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

fn save_package_dialog(owner: HWND, title: &str, file: &str) -> Option<PathBuf> {
    // SAFETY: as in `open_design_dialog`; the dialog asks before overwriting.
    unsafe {
        let d: IFileSaveDialog =
            CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let filters = [COMDLG_FILTERSPEC {
            pszName: w!("Design package (*.ricepack)"),
            pszSpec: w!("*.ricepack"),
        }];
        let _ = d.SetFileTypes(&filters);
        let _ = d.SetDefaultExtension(w!("ricepack"));
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

/// Open dialog for the Import page, run outside the controller borrow.
pub fn run_open_design() {
    let Some(owner) = crate::controller::with_ctl(|c| c.studio_hwnd()).flatten() else {
        return;
    };
    if let Some(path) = open_design_dialog(owner, "Open windowsRICE design") {
        crate::controller::with_ctl(|c| {
            c.import_file(&path);
            c.studio_rebuild();
        });
    }
}

/// Save dialog for Export .ricepack, run outside the controller borrow.
pub fn run_save_design(name: &str) {
    let Some(owner) = crate::controller::with_ctl(|c| c.studio_hwnd()).flatten() else {
        return;
    };
    if let Some(path) = save_package_dialog(owner, "Export windowsRICE design", name) {
        crate::controller::with_ctl(|c| {
            c.export_package(&path);
            c.studio_rebuild();
        });
    }
}

/// A file name (without extension) from a design name.
fn file_stem(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == ' ' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim().to_owned();
    if s.is_empty() { "design".into() } else { s }
}

/// Writes through a temporary file and a rename.
fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

impl Controller {
    /// The design library (imported images and videos), next to config.toml.
    pub(crate) fn library_dir(&self) -> Option<PathBuf> {
        self.studio_config_dir().map(|d| d.join("library"))
    }

    fn import_view(&mut self) -> Option<&mut ImportView> {
        self.studio.as_mut().map(|s| &mut s.model.import)
    }

    fn say(&mut self, en: &str, ko: &str, sev: Severity) {
        let msg = if self.lang() == Lang::En { en } else { ko };
        self.studio_status(msg.to_owned(), sev);
    }

    /// Handles the Phase 12 design actions; `None` for other actions.
    pub(crate) fn design_action(&mut self, a: &StudioAction) -> Option<Deferred> {
        match a {
            StudioAction::ImportPaste => {
                let owner = self.studio_hwnd()?;
                match crate::studio_win::clipboard_text(owner) {
                    Some(text) if !text.trim().is_empty() => {
                        self.set_import_text(text, "Pasted code", None);
                    }
                    _ => self.say(
                        "The clipboard has no text. Copy the design code first.",
                        "클립보드에 텍스트가 없습니다. 먼저 디자인 코드를 복사하세요.",
                        Severity::Error,
                    ),
                }
                Some(Deferred::Nothing)
            }
            StudioAction::ImportOpen => Some(Deferred::OpenDesign),
            StudioAction::ImportAnalyze => {
                self.import_analyze();
                Some(Deferred::Nothing)
            }
            StudioAction::ImportSelect(i) => {
                let i = *i;
                if let Some(v) = self.import_view() {
                    v.selected = Some(i);
                }
                Some(Deferred::Nothing)
            }
            StudioAction::ImportClear => {
                if let Some(st) = &mut self.studio {
                    st.model.import = ImportView::default();
                    st.import_pkg = None;
                }
                Some(Deferred::Nothing)
            }
            StudioAction::ImportScan => {
                self.import_scan();
                Some(Deferred::Nothing)
            }
            StudioAction::ImportPreview => {
                self.import_use(false);
                Some(Deferred::Nothing)
            }
            StudioAction::ImportSave => {
                self.import_use(true);
                Some(Deferred::Nothing)
            }
            StudioAction::CopyDesignCode => {
                self.copy_design_code();
                Some(Deferred::Nothing)
            }
            StudioAction::ExportPackage => {
                let name = self
                    .studio
                    .as_ref()
                    .map(|s| s.model.preset_name.trim().to_owned())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "My design".into());
                Some(Deferred::SaveDesign(format!(
                    "{}.ricepack",
                    file_stem(&name)
                )))
            }
            StudioAction::CreateCustom => {
                self.create_custom();
                Some(Deferred::Nothing)
            }
            _ => None,
        }
    }

    fn set_import_text(&mut self, text: String, source: &str, pkg: Option<(PathBuf, Package)>) {
        if let Some(st) = &mut self.studio {
            st.model.import = ImportView {
                text,
                source: source.to_owned(),
                package: pkg.is_some(),
                ..Default::default()
            };
            st.import_pkg = pkg.map(|(path, pkg)| OpenPackage {
                path,
                pkg,
                scan: None,
            });
        }
    }

    /// Reads a design file (`.ricepack` or design code) into the Import page
    /// and analyses it. Nothing is changed until Preview, Save or Apply.
    pub(crate) fn import_file(&mut self, path: &Path) {
        let name = path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let is_pack = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ricepack"));
        let limit = if is_pack {
            MAX_PACKAGE_BYTES
        } else {
            MAX_DESIGN_BYTES + 1
        };
        if len as usize > limit {
            self.say(
                "The file is too large to be a windowsRICE design.",
                "파일이 너무 커서 windowsRICE 디자인일 수 없습니다.",
                Severity::Error,
            );
            return;
        }
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                self.studio_status(e.to_string(), Severity::Error);
                return;
            }
        };
        if is_pack || bytes.starts_with(b"PK") {
            match design_pack::read_package(&bytes, &self.import_options(None)) {
                Ok(pkg) => {
                    let text = pkg.design_text.clone();
                    self.set_import_text(text, &name, Some((path.to_path_buf(), pkg)));
                    self.import_analyze();
                }
                Err(e) => {
                    self.set_import_text(String::new(), &name, None);
                    if let Some(v) = self.import_view() {
                        v.analyzed = true;
                        v.checks = vec![("Package".into(), ImportCheck::Failed)];
                        v.diagnostics = vec![Diagnostic {
                            severity: DSev::Error,
                            path: String::new(),
                            line: None,
                            column: None,
                            message: format!("the package was refused: {e}"),
                            suggestion: Some(
                                "Ask for the design again, exported from windowsRICE (Export .ricepack)."
                                    .into(),
                            ),
                        }];
                    }
                }
            }
        } else {
            match String::from_utf8(bytes) {
                Ok(text) => {
                    self.set_import_text(text, &name, None);
                    self.import_analyze();
                }
                Err(_) => self.say(
                    "The file is not text design code.",
                    "파일이 텍스트 디자인 코드가 아닙니다.",
                    Severity::Error,
                ),
            }
        }
    }

    fn import_options_owned(&self) -> (Vec<String>, Vec<String>) {
        let fonts = self
            .studio
            .as_ref()
            .map(|s| s.model.fonts.iter().map(|f| f.family.clone()).collect())
            .unwrap_or_default();
        let themes = self.theme_catalog_ids();
        (fonts, themes)
    }

    fn import_options(&self, _files: Option<&[String]>) -> AnalyzeOptions<'static> {
        // Package reading itself only needs the default options; fonts and
        // themes are added when the design code is analysed.
        AnalyzeOptions::default()
    }

    /// Analyses the Import page's text (with the package's files, when a
    /// package is open) and adds the Studio's own checks.
    pub(crate) fn import_analyze(&mut self) {
        let (fonts, themes) = self.import_options_owned();
        let Some(st) = &self.studio else { return };
        let text = st.model.import.text.clone();
        let files: Option<Vec<String>> = st
            .import_pkg
            .as_ref()
            .map(|p| p.pkg.assets.iter().map(|(f, _)| f.name.clone()).collect());
        let scan = st.import_pkg.as_ref().map(|p| p.scan.clone());
        let fonts_opt = if fonts.is_empty() {
            None
        } else {
            Some(fonts.as_slice())
        };
        let analysis = analyze(
            &text,
            &AnalyzeOptions {
                fonts: fonts_opt,
                themes: Some(&themes),
                package_files: files.as_deref(),
            },
        );
        let mut diags = analysis.diagnostics.clone();
        // Studio checks on a parsed design: readability and fit.
        if let Some(d) = &analysis.design {
            let plain = windowsrice_ui::style::resolve_bar_theme(
                self.theme.catalog(),
                &d.theme,
                &Default::default(),
                crate::controller::system_mode(&self.args),
            );
            if let Some(plain) = plain {
                for (role, from, to) in
                    windowsrice_ui::style::readability_fixes(&plain, &d.top_bar.style)
                {
                    diags.push(Diagnostic {
                        severity: DSev::Warning,
                        path: "top_bar.style".into(),
                        line: None,
                        column: None,
                        message: format!(
                            "{role} #{:02X}{:02X}{:02X} has too little contrast and is shown as #{:02X}{:02X}{:02X}",
                            from.r, from.g, from.b, to.r, to.g, to.b
                        ),
                        suggestion: Some("Choose colours with at least 4.5:1 contrast to keep the original colour.".into()),
                    });
                }
            }
            let on = d.top_bar.modules.iter().filter(|m| m.enabled).count();
            if on > 14 {
                diags.push(Diagnostic {
                    severity: DSev::Warning,
                    path: "top_bar.modules".into(),
                    line: None,
                    column: None,
                    message: format!("{on} modules are turned on; on narrow screens some move to the overflow (…) menu"),
                    suggestion: Some("Turn off modules you do not need.".into()),
                });
            }
        }
        let errors = diags.iter().filter(|d| d.severity == DSev::Error).count();
        let checks = vec![
            ("Syntax".to_owned(), status_check(analysis.syntax)),
            ("Schema".to_owned(), status_check(analysis.schema)),
            (
                "No executable content accepted".to_owned(),
                status_check(analysis.no_executable_content),
            ),
            ("Asset checks".to_owned(), status_check(analysis.assets)),
            (
                "Antivirus scan".to_owned(),
                match scan {
                    None => ImportCheck::NotRun,
                    Some(None) => ImportCheck::NotRun,
                    Some(Some(ScanResult::NoThreat)) => ImportCheck::Passed,
                    Some(Some(ScanResult::Threat)) => ImportCheck::Failed,
                    Some(Some(ScanResult::Unavailable(_))) => ImportCheck::Unavailable,
                },
            ),
            (
                "Review required".to_owned(),
                if errors == 0 && analysis.design.is_some() {
                    ImportCheck::NotRun
                } else {
                    ImportCheck::Failed
                },
            ),
        ];
        let can = analysis.design.is_some() && errors == 0;
        let threat = scan == Some(Some(ScanResult::Threat));
        let name = analysis.design.as_ref().map(|d| d.name.clone());
        if let Some(v) = self.import_view() {
            v.analyzed = true;
            v.diagnostics = diags;
            v.checks = checks;
            v.can_apply = can && !threat;
            v.name = name;
            v.selected = v
                .diagnostics
                .iter()
                .position(|d| d.severity == DSev::Error && d.line.is_some());
        }
    }

    fn import_scan(&mut self) {
        let Some(path) = self
            .studio
            .as_ref()
            .and_then(|s| s.import_pkg.as_ref())
            .map(|p| p.path.clone())
        else {
            return;
        };
        let r = defender_scan(&path);
        let (en, ko, sev) = match &r {
            ScanResult::NoThreat => (
                "Microsoft Defender reported no threat in the package. This is one scan result, not a guarantee.".to_owned(),
                "Microsoft Defender가 패키지에서 위협을 보고하지 않았습니다. 하나의 검사 결과일 뿐 보증은 아닙니다.".to_owned(),
                Severity::Success,
            ),
            ScanResult::Threat => (
                "Microsoft Defender reported a threat in the package. Do not use it.".to_owned(),
                "Microsoft Defender가 패키지에서 위협을 보고했습니다. 사용하지 마세요.".to_owned(),
                Severity::Error,
            ),
            ScanResult::Unavailable(why) => (
                format!("Antivirus scan unavailable: {why}."),
                format!("바이러스 검사를 사용할 수 없습니다: {why}."),
                Severity::Warning,
            ),
        };
        if let Some(p) = self.studio.as_mut().and_then(|s| s.import_pkg.as_mut()) {
            p.scan = Some(r);
        }
        self.import_analyze();
        self.say(&en, &ko, sev);
    }

    /// Preview (load into the draft) or save to My designs.
    fn import_use(&mut self, save: bool) {
        self.import_analyze();
        let Some(st) = &self.studio else { return };
        if !st.model.import.can_apply {
            self.say(
                "Fix the errors first (Analyze shows them).",
                "먼저 오류를 고치세요 (분석에 표시됨).",
                Severity::Error,
            );
            return;
        }
        let text = st.model.import.text.clone();
        let Some(mut design) = analyze(&text, &AnalyzeOptions::default()).design else {
            return;
        };
        // Package assets go into the design library under generated names.
        let lib = self.library_dir();
        if let (Some(p), Some(lib)) = (
            self.studio.as_ref().and_then(|s| s.import_pkg.as_ref()),
            lib,
        ) {
            let mut pkg = p.pkg.clone();
            pkg.analysis.design = Some(design.clone());
            match design_pack::import_assets(&pkg, &lib) {
                Ok(d) => design = d,
                Err(e) => {
                    self.studio_status(e.to_string(), Severity::Error);
                    return;
                }
            }
        }
        if save {
            let Some(dir) = self.studio_presets_dir() else {
                return;
            };
            let _ = std::fs::create_dir_all(&dir);
            let mut name = design.name.trim().to_owned();
            let mut path = dir.join(preset_file_name(&name));
            let mut n = 2;
            while path.exists() {
                name = format!("{} ({n})", design.name.trim());
                path = dir.join(preset_file_name(&name));
                n += 1;
            }
            let p = Preset::new(&name, design.theme.clone(), design.top_bar.clone());
            match p.save(&path) {
                Ok(()) => {
                    self.studio_refresh();
                    let (en, ko) = (
                        format!("Saved {name:?} to My designs."),
                        format!("{name:?}을(를) 내 디자인에 저장했습니다."),
                    );
                    self.say(&en, &ko, Severity::Success);
                }
                Err(e) => self.studio_status(e.to_string(), Severity::Error),
            }
        } else {
            self.studio_edit(|d| {
                d.theme = design.theme.clone();
                d.bar = design.top_bar.clone();
                true
            });
            let (en, ko) = (
                format!(
                    "Previewing {:?}. Apply to use it on the bar; Undo returns to your design.",
                    design.name
                ),
                format!(
                    "{:?} 미리 보기 중입니다. 바에 쓰려면 적용하고, 실행 취소로 되돌릴 수 있습니다.",
                    design.name
                ),
            );
            self.say(&en, &ko, Severity::Success);
        }
    }

    fn current_design(&self) -> Option<DesignCode> {
        let st = self.studio.as_ref()?;
        let name = st.model.preset_name.trim();
        let name = if name.is_empty() { "My design" } else { name };
        Some(DesignCode::new(
            name,
            st.model.draft.theme.clone(),
            st.model.draft.bar.clone(),
        ))
    }

    fn copy_design_code(&mut self) {
        let Some(mut d) = self.current_design() else {
            return;
        };
        let Some(owner) = self.studio_hwnd() else {
            return;
        };
        // A background file cannot travel in text; a package carries it.
        let had_asset = d.top_bar.style.background_asset.take().is_some();
        crate::studio_win::clipboard_set(owner, &d.to_text());
        if had_asset {
            self.say(
                "Design code copied. The background image or video is not included in code; use Export .ricepack to share it.",
                "디자인 코드를 복사했습니다. 배경 이미지나 동영상은 코드에 포함되지 않으니 .ricepack 내보내기를 쓰세요.",
                Severity::Success,
            );
        } else {
            self.say(
                "Design code copied to the clipboard.",
                "디자인 코드를 클립보드에 복사했습니다.",
                Severity::Success,
            );
        }
    }

    /// Writes the current design as a `.ricepack` (with its background
    /// file from the library, when it has one).
    pub(crate) fn export_package(&mut self, to: &Path) {
        let Some(mut d) = self.current_design() else {
            return;
        };
        let mut files = Vec::new();
        if let (Some(id), Some(lib)) =
            (d.top_bar.style.background_asset.clone(), self.library_dir())
        {
            let found = ["png", "jpg", "mp4"]
                .iter()
                .map(|e| lib.join(format!("{id}.{e}")))
                .find(|p| p.exists());
            match found.and_then(|p| std::fs::read(&p).ok().map(|b| (p, b))) {
                Some((p, bytes)) => {
                    let ext = p
                        .extension()
                        .map(|e| e.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let kind = if ext == "mp4" {
                        AssetKind::Video
                    } else {
                        AssetKind::Image
                    };
                    let file = format!("assets/background.{ext}");
                    d.assets.push(AssetRef {
                        id: "background".into(),
                        kind,
                        file: file.clone(),
                        trim_start_ms: None,
                        trim_end_ms: None,
                        poster_ms: None,
                        r#loop: None,
                        speed: None,
                    });
                    d.top_bar.style.background_asset = Some("background".into());
                    files.push((file, bytes));
                }
                None => d.top_bar.style.background_asset = None,
            }
        }
        let bytes = design_pack::build_package(&d, None, &files);
        match write_atomic(to, &bytes) {
            Ok(()) => {
                let (en, ko) = (
                    format!("Exported to {}.", to.display()),
                    format!("{}(으)로 내보냈습니다.", to.display()),
                );
                self.say(&en, &ko, Severity::Success);
            }
            Err(e) => self.studio_status(e.to_string(), Severity::Error),
        }
    }

    /// Starts a design from scratch: the default theme and appearance with
    /// only the menu and the clock, then opens Bar layout › Modules.
    fn create_custom(&mut self) {
        self.studio_edit(|d| {
            d.theme = ThemeSelection {
                mode: d.theme.mode,
                ..ThemeSelection::default()
            };
            d.bar.style = Default::default();
            d.bar.height = None;
            d.bar.spacing = 4;
            d.bar.modules = template_modules(
                &d.bar.modules,
                &[("menu", Region::Left), ("clock", Region::Center)],
            );
            true
        });
        let _ = self.studio_action(StudioAction::Nav(Section::Modules));
        self.say(
            "New design: only the menu and the clock are on. Turn on and arrange modules here, then style them in Appearance. Undo returns to your previous design.",
            "새 디자인: 메뉴와 시계만 켜져 있습니다. 여기서 모듈을 켜고 배치한 뒤 모양에서 꾸미세요. 실행 취소로 이전 디자인으로 돌아갑니다.",
            Severity::Success,
        );
    }

    /// The Import page line editor: replaces the selected problem's line
    /// and analyses again.
    pub(crate) fn import_replace_line(&mut self, v: &str) {
        let Some(st) = &mut self.studio else { return };
        let iv = &mut st.model.import;
        let Some(line) = iv.selected_line() else {
            return;
        };
        let mut lines: Vec<String> = iv.text.lines().map(str::to_owned).collect();
        if let Some(l) = lines.get_mut(line as usize - 1) {
            *l = v.to_owned();
        }
        iv.text = lines.join("\n");
        self.import_analyze();
    }

    /// The text of the selected problem's line (line editor).
    pub(crate) fn import_line_text(&self) -> String {
        self.studio
            .as_ref()
            .and_then(|s| {
                let iv = &s.model.import;
                iv.selected_line()
                    .and_then(|l| iv.text.lines().nth(l as usize - 1))
                    .map(str::to_owned)
            })
            .unwrap_or_default()
    }
}
