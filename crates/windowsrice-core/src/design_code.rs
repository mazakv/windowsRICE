//! RICE Design Code (Phase 12): a versioned, strictly declarative JSON
//! format for sharing complete windowsRICE designs.
//!
//! ```json
//! {
//!   "format": "rice-design",
//!   "version": 1,
//!   "name": "Neon evening",
//!   "theme": { "active": "windowsrice-neon", "mode": "dark" },
//!   "top_bar": { "height": 36, "style": { "accent": "#FF3DCB" }, "modules": [ ... ] },
//!   "bottom_bar": { ... },
//!   "assets": [ { "id": "sky", "kind": "image", "file": "assets/sky.png" } ]
//! }
//! ```
//!
//! `top_bar` and `bottom_bar` hold exactly the `[bar]` (and Phase 14
//! `[bottom_bar]`) content of `config.toml`. Design code is data only: it is
//! parsed against an allowlist of fields (unknown fields are errors) and
//! never executed. [`analyze`] reports every problem with its location,
//! severity, an explanation and a suggested correction.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::bar::{BarConfig, is_valid_asset_id};
use crate::config::{ThemeSelection, is_valid_theme_id};

pub const DESIGN_FORMAT: &str = "rice-design";
pub const DESIGN_VERSION: u32 = 1;
/// Largest accepted design code.
pub const MAX_DESIGN_BYTES: usize = 256 * 1024;
/// Longest accepted text value.
pub const MAX_STRING_CHARS: usize = 400;
/// Most assets in one design.
pub const MAX_ASSETS: usize = 8;

/// Kind of an asset file in a design package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    Image,
    Video,
}

/// An asset of a design package (`.ricepack`). `file` is a path inside the
/// package (`assets/<name>.<png|jpg|jpeg|mp4>`); it is never used as a path
/// on disk: imported files get generated names in the design library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetRef {
    pub id: String,
    pub kind: AssetKind,
    pub file: String,
    /// Video: start of the played part, milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trim_start_ms: Option<u32>,
    /// Video: end of the played part, milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trim_end_ms: Option<u32>,
    /// Video: the frame used as the still image (poster), milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster_ms: Option<u32>,
    /// Video: play again from the start. Default on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#loop: Option<bool>,
    /// Video: playback speed, 0.25–2.0. Default 1.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesignCode {
    pub format: String,
    pub version: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub theme: ThemeSelection,
    pub top_bar: BarConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom_bar: Option<BarConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<AssetRef>,
}

impl DesignCode {
    pub fn new(name: &str, theme: ThemeSelection, top_bar: BarConfig) -> Self {
        Self {
            format: DESIGN_FORMAT.into(),
            version: DESIGN_VERSION,
            name: name.trim().to_owned(),
            author: None,
            description: None,
            theme,
            top_bar,
            bottom_bar: None,
            assets: Vec::new(),
        }
    }

    /// Pretty JSON for export and sharing.
    pub fn to_text(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// One analyzer finding.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Location in the design, such as `top_bar.modules[2].color` (empty for
    /// the whole design).
    pub path: String,
    /// 1-based line and column, when known.
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub message: String,
    pub suggestion: Option<String>,
}

/// What the analysis established, each stated plainly. A design is never
/// described as "safe" or "malware-free": static checks cannot show that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Passed,
    Failed,
    /// Not applicable or not performed (for example asset files when only
    /// code was pasted).
    NotRun,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    /// The parsed design, when syntax and schema allow it (it may still have
    /// errors; check [`Analysis::can_apply`]).
    pub design: Option<DesignCode>,
    pub diagnostics: Vec<Diagnostic>,
    pub syntax: CheckStatus,
    pub schema: CheckStatus,
    /// No script, executable or external reference was accepted.
    pub no_executable_content: CheckStatus,
    pub assets: CheckStatus,
}

impl Analysis {
    pub fn errors(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count()
    }
    pub fn warnings(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count()
    }
    /// The design can be previewed, saved and applied: parsed, and no errors.
    pub fn can_apply(&self) -> bool {
        self.design.is_some() && self.errors() == 0
    }
}

/// What the analyzer may know about the environment.
#[derive(Debug, Clone, Default)]
pub struct AnalyzeOptions<'a> {
    /// Installed font families; `None` skips the font check.
    pub fonts: Option<&'a [String]>,
    /// Available theme ids; `None` skips the theme check.
    pub themes: Option<&'a [String]>,
    /// Files of the package being imported (`file` paths), when analysing a
    /// `.ricepack`; `None` for pasted code.
    pub package_files: Option<&'a [String]>,
}

// ------------------------------------------------------------ locations

/// Maps every value's path to its 1-based (line, column). The text has
/// already passed `serde_json`, so this scanner can be simple.
fn locate(text: &str) -> HashMap<String, (u32, u32)> {
    struct S<'a> {
        c: Vec<(usize, char)>,
        i: usize,
        text: &'a str,
        out: HashMap<String, (u32, u32)>,
    }
    impl S<'_> {
        fn pos(&self) -> (u32, u32) {
            let at = self.c.get(self.i).map_or(self.text.len(), |c| c.0);
            let before = &self.text[..at];
            let line = before.matches('\n').count() as u32 + 1;
            let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) as u32 + 1;
            (line, col)
        }
        fn ws(&mut self) {
            while self.c.get(self.i).is_some_and(|c| c.1.is_whitespace()) {
                self.i += 1;
            }
        }
        fn string(&mut self) -> String {
            let mut s = String::new();
            self.i += 1; // opening quote
            while let Some(&(_, ch)) = self.c.get(self.i) {
                self.i += 1;
                match ch {
                    '"' => break,
                    '\\' => {
                        if let Some(&(_, e)) = self.c.get(self.i) {
                            s.push(e);
                            self.i += 1;
                        }
                    }
                    _ => s.push(ch),
                }
            }
            s
        }
        fn value(&mut self, path: &str, depth: u32) {
            self.ws();
            self.out.insert(path.to_owned(), self.pos());
            if depth > 64 {
                return;
            }
            match self.c.get(self.i).map(|c| c.1) {
                Some('{') => {
                    self.i += 1;
                    loop {
                        self.ws();
                        match self.c.get(self.i).map(|c| c.1) {
                            Some('}') => {
                                self.i += 1;
                                return;
                            }
                            Some(',') => self.i += 1,
                            Some('"') => {
                                let key_pos = self.pos();
                                let k = self.string();
                                let p = if path.is_empty() {
                                    k
                                } else {
                                    format!("{path}.{k}")
                                };
                                self.out.insert(format!("{p}#key"), key_pos);
                                self.ws();
                                if self.c.get(self.i).map(|c| c.1) == Some(':') {
                                    self.i += 1;
                                }
                                self.value(&p, depth + 1);
                            }
                            _ => return,
                        }
                    }
                }
                Some('[') => {
                    self.i += 1;
                    let mut n = 0;
                    loop {
                        self.ws();
                        match self.c.get(self.i).map(|c| c.1) {
                            Some(']') => {
                                self.i += 1;
                                return;
                            }
                            Some(',') => self.i += 1,
                            Some(_) => {
                                self.value(&format!("{path}[{n}]"), depth + 1);
                                n += 1;
                            }
                            None => return,
                        }
                    }
                }
                Some('"') => {
                    self.string();
                }
                Some(_) => {
                    while self
                        .c
                        .get(self.i)
                        .is_some_and(|c| !matches!(c.1, ',' | '}' | ']') && !c.1.is_whitespace())
                    {
                        self.i += 1;
                    }
                }
                None => {}
            }
        }
    }
    let mut s = S {
        c: text.char_indices().collect(),
        i: 0,
        text,
        out: HashMap::new(),
    };
    s.value("", 0);
    s.out
}

struct Report<'a> {
    at: HashMap<String, (u32, u32)>,
    diags: Vec<Diagnostic>,
    _text: &'a str,
}

impl Report<'_> {
    fn push(&mut self, sev: Severity, path: &str, msg: impl Into<String>, fix: Option<&str>) {
        // The closest located ancestor gives the position.
        let mut p = path.to_owned();
        let pos = loop {
            if let Some(v) = self.at.get(&p) {
                break Some(*v);
            }
            match p.rfind(['.', '[']) {
                Some(i) => p.truncate(i),
                None => break self.at.get("").copied(),
            }
        };
        self.diags.push(Diagnostic {
            severity: sev,
            path: path.to_owned(),
            line: pos.map(|p| p.0),
            column: pos.map(|p| p.1),
            message: msg.into(),
            suggestion: fix.map(str::to_owned),
        });
    }
    fn key(&mut self, sev: Severity, path: &str, msg: impl Into<String>, fix: Option<&str>) {
        let pos = self.at.get(&format!("{path}#key")).copied();
        self.diags.push(Diagnostic {
            severity: sev,
            path: path.to_owned(),
            line: pos.map(|p| p.0),
            column: pos.map(|p| p.1),
            message: msg.into(),
            suggestion: fix.map(str::to_owned),
        });
    }
}

// ------------------------------------------------------------ content scan

/// Markers of executable or script content. Design code is data; any of
/// these in a value means the file is not a plain design.
const SCRIPT_MARKERS: [&str; 16] = [
    "<script",
    "javascript:",
    "vbscript:",
    "data:text/html",
    "powershell",
    "pwsh ",
    "cmd.exe",
    "cmd /c",
    "rundll32",
    "regsvr32",
    "mshta",
    "wscript",
    "cscript",
    "eval(",
    "invoke-expression",
    "frombase64string",
];
/// Executable or script file types.
const EXECUTABLE_EXTENSIONS: [&str; 18] = [
    ".exe", ".dll", ".sys", ".scr", ".com", ".bat", ".cmd", ".ps1", ".psm1", ".vbs", ".vbe", ".js",
    ".jse", ".wsf", ".msi", ".lnk", ".hta", ".wasm",
];

fn walk_strings<'v>(v: &'v Value, path: &str, out: &mut Vec<(String, &'v str)>) {
    match v {
        Value::String(s) => out.push((path.to_owned(), s)),
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                walk_strings(x, &format!("{path}[{i}]"), out);
            }
        }
        Value::Object(o) => {
            for (k, x) in o {
                let p = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                out.push((format!("{p}#keyname"), k));
                walk_strings(x, &p, out);
            }
        }
        _ => {}
    }
}

fn looks_like_path(s: &str) -> bool {
    let b = s.as_bytes();
    s.starts_with("\\\\")
        || s.starts_with('/')
        || s.starts_with('\\')
        || (b.len() >= 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && (b[2] == b'\\' || b[2] == b'/'))
        || s.contains("..\\")
        || s.contains("../")
        || s.starts_with('~')
        || s.contains('%') && s.to_ascii_lowercase().contains("%appdata%")
}

fn scan_content(root: &Value, r: &mut Report<'_>) -> bool {
    let mut strings = Vec::new();
    walk_strings(root, "", &mut strings);
    let mut clean = true;
    for (path, s) in strings {
        let key = path.ends_with("#keyname");
        let path = path.trim_end_matches("#keyname");
        let lower = s.to_ascii_lowercase();
        if s.chars().count() > MAX_STRING_CHARS {
            r.push(
                Severity::Error,
                path,
                format!("text longer than {MAX_STRING_CHARS} characters"),
                Some("Shorten the text."),
            );
        }
        if s.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
            r.push(
                Severity::Error,
                path,
                "control characters are not accepted",
                Some("Remove invisible control characters."),
            );
            clean = false;
        }
        if let Some(m) = SCRIPT_MARKERS.iter().find(|m| lower.contains(*m)) {
            r.push(
                Severity::Error,
                path,
                format!("script or command content ({m:?}) is not accepted: design code is data only and is never run"),
                Some("Remove the script or command; describe the design with the documented fields only."),
            );
            clean = false;
        }
        if lower.contains("://") || lower.starts_with("www.") || lower.starts_with("file:") {
            r.push(
                Severity::Error,
                path,
                "external references (web addresses, file URLs) are not accepted: windowsRICE works offline",
                Some("Put images and videos in a .ricepack and refer to them by asset id."),
            );
            clean = false;
        }
        if !key && looks_like_path(s) {
            r.push(
                Severity::Error,
                path,
                "absolute or parent-directory paths are not accepted",
                Some("Refer to package files as assets/<name> inside a .ricepack."),
            );
            clean = false;
        }
        if let Some(ext) = EXECUTABLE_EXTENSIONS.iter().find(|e| lower.ends_with(*e)) {
            r.push(
                Severity::Error,
                path,
                format!("executable or script file type {ext} is not accepted"),
                Some("Only PNG and JPEG images and MP4 videos can be part of a design."),
            );
            clean = false;
        }
    }
    clean
}

// ------------------------------------------------------------ analysis

fn suggestion_for_unknown(field: &str) -> String {
    format!(
        "Remove \"{field}\" or check its spelling against docs/design-code.md (only documented fields are accepted)."
    )
}

/// Turns a `serde_json` error inside `section` into a diagnostic at the
/// right place: an unknown field is reported at its key.
fn serde_problem(r: &mut Report<'_>, section: &str, e: &serde_json::Error) {
    let msg = e.to_string();
    let clean = msg.split(" at line ").next().unwrap_or(&msg).to_owned();
    if let Some(rest) = clean.strip_prefix("unknown field `")
        && let Some(field) = rest.split('`').next()
    {
        // Find the key below `section` (the closest match by path).
        let wanted = format!(".{field}#key");
        let found =
            r.at.keys()
                .filter(|k| {
                    (k.starts_with(&format!("{section}.")) || section.is_empty())
                        && (k.ends_with(&wanted) || **k == format!("{field}#key"))
                })
                .min_by_key(|k| k.len())
                .cloned();
        let path = found
            .map(|k| k.trim_end_matches("#key").to_owned())
            .unwrap_or_else(|| format!("{section}.{field}"));
        let fix = suggestion_for_unknown(field);
        r.key(
            Severity::Error,
            &path,
            format!("unknown field \"{field}\""),
            Some(&fix),
        );
        return;
    }
    if let Some(rest) = clean.strip_prefix("missing field `")
        && let Some(field) = rest.split('`').next()
    {
        let path = if section.is_empty() {
            field.to_owned()
        } else {
            format!("{section}.{field}")
        };
        r.push(
            Severity::Error,
            section,
            format!("missing required field \"{field}\""),
            Some(&format!("Add \"{field}\" (see docs/design-code.md).")),
        );
        let _ = path;
        return;
    }
    r.push(
        Severity::Error,
        section,
        format!("invalid value: {clean}"),
        Some("Use the type and values documented in docs/design-code.md."),
    );
}

/// Maps a `BarConfig::problems` message (`bar.style.accent = ...`) to a
/// design path under `prefix`.
fn problem_path(msg: &str, prefix: &str) -> String {
    let first = msg.split([' ', '=']).next().unwrap_or("");
    match first.strip_prefix("bar") {
        Some(rest) if rest.is_empty() || rest.starts_with('.') => format!("{prefix}{rest}"),
        _ => prefix.to_owned(),
    }
}

fn check_bar(bar: &BarConfig, prefix: &str, r: &mut Report<'_>, opts: &AnalyzeOptions<'_>) {
    for p in bar.problems() {
        let path = problem_path(&p, prefix);
        let msg = p.replacen("bar", prefix, 1);
        let fix = if msg.contains("#RRGGBB") || msg.contains("colour") || msg.contains("color") {
            "Use a colour written as #RRGGBB, for example #4CC2FF."
        } else if msg.contains("outside") {
            "Use a value inside the stated range."
        } else if msg.contains("duplicate") || msg.contains("twice") || msg.contains("second time")
        {
            "Give every item a unique id, or remove the repeated one."
        } else {
            "Correct the value as described."
        };
        r.push(Severity::Error, &path, msg, Some(fix));
    }
    // Duplicate module ids, with the location of the second one.
    let mut seen = HashSet::new();
    for (i, m) in bar.modules.iter().enumerate() {
        if !seen.insert(m.id.as_str()) {
            r.push(
                Severity::Error,
                &format!("{prefix}.modules[{i}].id"),
                format!("module id {:?} is used twice", m.id),
                Some("Give every module a unique id."),
            );
        }
    }
    if let (Some(fonts), Some(f)) = (opts.fonts, &bar.style.font_family)
        && !fonts.iter().any(|x| x.eq_ignore_ascii_case(f))
    {
        r.push(
            Severity::Warning,
            &format!("{prefix}.style.font_family"),
            format!(
                "the font {f:?} is not installed on this PC; Segoe UI Variable is used instead"
            ),
            Some("Choose an installed font in Appearance › Typography, or install the font."),
        );
    }
    let visible = bar.modules.iter().filter(|m| m.enabled).count();
    if visible == 0 {
        r.push(
            Severity::Warning,
            &format!("{prefix}.modules"),
            "no module is turned on; the bar will be empty",
            Some("Turn on at least one module."),
        );
    }
}

/// Analyses design code. Never executes anything.
pub fn analyze(text: &str, opts: &AnalyzeOptions<'_>) -> Analysis {
    let mut out = Analysis {
        design: None,
        diagnostics: Vec::new(),
        syntax: CheckStatus::NotRun,
        schema: CheckStatus::NotRun,
        no_executable_content: CheckStatus::NotRun,
        assets: CheckStatus::NotRun,
    };
    let mut r = Report {
        at: HashMap::new(),
        diags: Vec::new(),
        _text: text,
    };
    if text.len() > MAX_DESIGN_BYTES {
        r.push(
            Severity::Error,
            "",
            format!(
                "the design code is larger than {} KB",
                MAX_DESIGN_BYTES / 1024
            ),
            Some("Design code holds settings only; put images and videos in a .ricepack."),
        );
        out.syntax = CheckStatus::Failed;
        out.diagnostics = r.diags;
        return out;
    }
    let root: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            let msg = e.to_string();
            let clean = msg.split(" at line ").next().unwrap_or(&msg).to_owned();
            r.diags.push(Diagnostic {
                severity: Severity::Error,
                path: String::new(),
                line: u32::try_from(e.line()).ok(),
                column: u32::try_from(e.column()).ok(),
                message: format!("syntax error: {clean}"),
                suggestion: Some(
                    "Design code is JSON: check quotes, commas and brackets near this position."
                        .into(),
                ),
            });
            out.syntax = CheckStatus::Failed;
            out.diagnostics = r.diags;
            return out;
        }
    };
    out.syntax = CheckStatus::Passed;
    r.at = locate(text);

    out.no_executable_content = if scan_content(&root, &mut r) {
        CheckStatus::Passed
    } else {
        CheckStatus::Failed
    };

    let Some(obj) = root.as_object() else {
        r.push(
            Severity::Error,
            "",
            "the design code must be a JSON object { ... }",
            Some("Start from an example in docs/design-code.md."),
        );
        out.schema = CheckStatus::Failed;
        out.diagnostics = r.diags;
        return out;
    };
    // Format and version first: a different format is reported as such.
    match obj.get("format").and_then(Value::as_str) {
        Some(DESIGN_FORMAT) => {}
        Some(other) => r.push(
            Severity::Error,
            "format",
            format!("format is {other:?}, not \"{DESIGN_FORMAT}\""),
            Some("Set \"format\": \"rice-design\"."),
        ),
        None => r.push(
            Severity::Error,
            "format",
            "missing \"format\"",
            Some("Add \"format\": \"rice-design\"."),
        ),
    }
    match obj.get("version").and_then(Value::as_u64) {
        Some(v) if v == u64::from(DESIGN_VERSION) => {}
        Some(0) => r.push(
            Severity::Error,
            "version",
            "version 0 does not exist",
            Some("Set \"version\": 1."),
        ),
        Some(v) => r.push(
            Severity::Error,
            "version",
            format!("version {v} is newer than this windowsRICE supports ({DESIGN_VERSION})"),
            Some("Update windowsRICE, or ask for the design in version 1."),
        ),
        None => r.push(
            Severity::Error,
            "version",
            "missing or invalid \"version\"",
            Some("Add \"version\": 1."),
        ),
    }

    // Typed parsing, section by section, so each problem has a place.
    let mut schema_ok = true;
    let mut sections_ok = true;
    let allowed = [
        "format",
        "version",
        "name",
        "author",
        "description",
        "theme",
        "top_bar",
        "bottom_bar",
        "assets",
    ];
    for k in obj.keys() {
        if !allowed.contains(&k.as_str()) {
            let fix = suggestion_for_unknown(k);
            r.key(
                Severity::Error,
                k,
                format!("unknown field \"{k}\""),
                Some(&fix),
            );
            sections_ok = false;
        }
    }
    for (k, required) in [("name", true), ("theme", true), ("top_bar", true)] {
        if required && !obj.contains_key(k) {
            r.push(
                Severity::Error,
                "",
                format!("missing required field \"{k}\""),
                Some(&format!("Add \"{k}\" (see docs/design-code.md).")),
            );
            sections_ok = false;
        }
    }
    let parse_section = |k: &str, r: &mut Report<'_>| -> bool {
        let Some(v) = obj.get(k) else { return true };
        let res = match k {
            "theme" => serde_json::from_value::<ThemeSelection>(v.clone()).map(|_| ()),
            "top_bar" | "bottom_bar" => {
                // Modules one by one, so each module's problem is located.
                let mut ok = true;
                if let Some(mods) = v.get("modules").and_then(Value::as_array) {
                    for (i, m) in mods.iter().enumerate() {
                        if let Err(e) =
                            serde_json::from_value::<crate::bar::ModuleConfig>(m.clone())
                        {
                            serde_problem(r, &format!("{k}.modules[{i}]"), &e);
                            ok = false;
                        }
                    }
                }
                if let Some(st) = v.get("style")
                    && let Err(e) = serde_json::from_value::<crate::bar::BarStyle>(st.clone())
                {
                    serde_problem(r, &format!("{k}.style"), &e);
                    ok = false;
                }
                if !ok {
                    return false;
                }
                serde_json::from_value::<BarConfig>(v.clone()).map(|_| ())
            }
            "assets" => serde_json::from_value::<Vec<AssetRef>>(v.clone()).map(|_| ()),
            "name" | "author" | "description" => {
                if v.is_string() {
                    Ok(())
                } else {
                    r.push(
                        Severity::Error,
                        k,
                        format!("\"{k}\" must be text"),
                        Some("Write it in quotes."),
                    );
                    return false;
                }
            }
            _ => Ok(()),
        };
        match res {
            Ok(()) => true,
            Err(e) => {
                serde_problem(r, k, &e);
                false
            }
        }
    };
    for k in [
        "name",
        "author",
        "description",
        "theme",
        "top_bar",
        "bottom_bar",
        "assets",
    ] {
        sections_ok &= parse_section(k, &mut r);
    }
    let design = if sections_ok {
        match serde_json::from_value::<DesignCode>(root.clone()) {
            Ok(d) => Some(d),
            Err(e) => {
                serde_problem(&mut r, "", &e);
                None
            }
        }
    } else {
        None
    };
    if design.is_none() {
        schema_ok = false;
    }

    if let Some(d) = &design {
        if d.name.trim().is_empty() {
            r.push(
                Severity::Error,
                "name",
                "the name is empty",
                Some("Give the design a name."),
            );
        }
        if !is_valid_theme_id(&d.theme.active) {
            r.push(
                Severity::Error,
                "theme.active",
                format!("{:?} is not a valid theme id", d.theme.active),
                Some("Use a theme id such as \"windowsrice-default\"."),
            );
        } else if let Some(themes) = opts.themes
            && !themes.contains(&d.theme.active)
        {
            r.push(
                Severity::Warning,
                "theme.active",
                format!(
                    "the theme {:?} is not installed; windowsRICE Default is used instead",
                    d.theme.active
                ),
                Some("Choose an installed theme, or import the theme first."),
            );
        }
        check_bar(&d.top_bar, "top_bar", &mut r, opts);
        if let Some(b) = &d.bottom_bar {
            check_bar(b, "bottom_bar", &mut r, opts);
        }
        out.assets = check_assets(d, &mut r, opts);
    }

    out.schema = if schema_ok
        && !r.diags.iter().any(|d| {
            d.severity == Severity::Error
                && (d.message.starts_with("unknown field")
                    || d.message.starts_with("missing")
                    || d.message.starts_with("invalid value")
                    || d.path == "format"
                    || d.path == "version")
        }) {
        CheckStatus::Passed
    } else {
        CheckStatus::Failed
    };
    r.diags.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then(a.line.cmp(&b.line))
            .then(a.column.cmp(&b.column))
    });
    out.design = design;
    out.diagnostics = r.diags;
    out
}

/// Allowed asset file extensions by kind.
pub fn asset_kind_for(file: &str) -> Option<AssetKind> {
    let l = file.to_ascii_lowercase();
    if l.ends_with(".png") || l.ends_with(".jpg") || l.ends_with(".jpeg") {
        Some(AssetKind::Image)
    } else if l.ends_with(".mp4") {
        Some(AssetKind::Video)
    } else {
        None
    }
}

/// A package path is `assets/<name>` with a plain file name.
pub fn is_valid_package_path(p: &str) -> bool {
    let Some(name) = p.strip_prefix("assets/") else {
        return false;
    };
    !name.is_empty()
        && name.len() <= 100
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !name.contains("..")
}

fn check_assets(d: &DesignCode, r: &mut Report<'_>, opts: &AnalyzeOptions<'_>) -> CheckStatus {
    let mut ok = true;
    if d.assets.len() > MAX_ASSETS {
        r.push(
            Severity::Error,
            "assets",
            format!(
                "{} assets; at most {MAX_ASSETS} are allowed",
                d.assets.len()
            ),
            Some("Remove unused assets."),
        );
        ok = false;
    }
    let mut ids = HashSet::new();
    for (i, a) in d.assets.iter().enumerate() {
        let p = format!("assets[{i}]");
        if !is_valid_asset_id(&a.id) {
            r.push(
                Severity::Error,
                &format!("{p}.id"),
                format!("{:?} is not a valid asset id", a.id),
                Some("Use lowercase letters, digits and -, up to 64 characters."),
            );
            ok = false;
        }
        if !ids.insert(a.id.as_str()) {
            r.push(
                Severity::Error,
                &format!("{p}.id"),
                format!("asset id {:?} is used twice", a.id),
                Some("Give every asset a unique id."),
            );
            ok = false;
        }
        if !is_valid_package_path(&a.file) {
            r.push(
                Severity::Error,
                &format!("{p}.file"),
                format!("{:?} is not a package path", a.file),
                Some("Write assets/<name>.png, .jpg or .mp4 (letters, digits, - _ . only)."),
            );
            ok = false;
        }
        match asset_kind_for(&a.file) {
            Some(k) if k == a.kind => {}
            Some(_) => {
                r.push(
                    Severity::Error,
                    &format!("{p}.kind"),
                    "the kind does not match the file type",
                    Some("Images are .png or .jpg, videos are .mp4."),
                );
                ok = false;
            }
            None => {
                r.push(
                    Severity::Error,
                    &format!("{p}.file"),
                    "only PNG and JPEG images and MP4 videos are supported",
                    Some("Convert the file to PNG, JPEG or MP4."),
                );
                ok = false;
            }
        }
        if a.kind == AssetKind::Image
            && (a.trim_start_ms.is_some()
                || a.trim_end_ms.is_some()
                || a.poster_ms.is_some()
                || a.speed.is_some()
                || a.r#loop.is_some())
        {
            r.push(
                Severity::Warning,
                &p,
                "video settings on an image are ignored",
                Some("Remove trim, poster, speed and loop from images."),
            );
        }
        if let (Some(s), Some(e)) = (a.trim_start_ms, a.trim_end_ms)
            && e <= s
        {
            r.push(
                Severity::Error,
                &format!("{p}.trim_end_ms"),
                "the end of the played part is before its start",
                Some("Make trim_end_ms larger than trim_start_ms."),
            );
            ok = false;
        }
        if let Some(sp) = a.speed
            && !(0.25..=2.0).contains(&sp)
        {
            r.push(
                Severity::Error,
                &format!("{p}.speed"),
                format!("speed {sp} is outside 0.25-2.0"),
                Some("Use a speed between 0.25 and 2.0."),
            );
            ok = false;
        }
        match opts.package_files {
            Some(files) if !files.contains(&a.file) => {
                r.push(
                    Severity::Error,
                    &format!("{p}.file"),
                    format!("the package has no file {:?}", a.file),
                    Some("Add the file to the package or remove the asset."),
                );
                ok = false;
            }
            _ => {}
        }
    }
    for (prefix, bar) in std::iter::once(("top_bar", &d.top_bar))
        .chain(d.bottom_bar.as_ref().map(|b| ("bottom_bar", b)))
    {
        if let Some(id) = &bar.style.background_asset
            && !d.assets.iter().any(|a| a.id == *id)
        {
            r.push(
                Severity::Error,
                &format!("{prefix}.style.background_asset"),
                format!("no asset with id {id:?} in \"assets\""),
                Some("Add the asset, or remove background_asset."),
            );
            ok = false;
        }
    }
    if d.assets.is_empty() {
        return if ok {
            CheckStatus::NotRun
        } else {
            CheckStatus::Failed
        };
    }
    if opts.package_files.is_none() {
        r.push(
            Severity::Warning,
            "assets",
            "asset files can only be checked when a .ricepack is imported",
            Some("Import the design as a .ricepack to include its images and videos."),
        );
        return if ok {
            CheckStatus::NotRun
        } else {
            CheckStatus::Failed
        };
    }
    if ok {
        CheckStatus::Passed
    } else {
        CheckStatus::Failed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> String {
        DesignCode::new("Test", ThemeSelection::default(), BarConfig::default()).to_text()
    }

    #[test]
    fn exported_design_analyses_clean() {
        let a = analyze(&minimal(), &AnalyzeOptions::default());
        assert!(a.can_apply(), "{:#?}", a.diagnostics);
        assert_eq!(a.syntax, CheckStatus::Passed);
        assert_eq!(a.schema, CheckStatus::Passed);
        assert_eq!(a.no_executable_content, CheckStatus::Passed);
    }

    #[test]
    fn syntax_error_has_line_and_column() {
        let a = analyze(
            "{\n  \"format\": \"rice-design\",\n  \"version\": 1\n  \"name\": \"x\"\n}",
            &AnalyzeOptions::default(),
        );
        assert_eq!(a.syntax, CheckStatus::Failed);
        let d = &a.diagnostics[0];
        assert_eq!(d.line, Some(4), "{d:?}");
        assert!(d.column.is_some());
        assert!(d.suggestion.is_some());
    }

    #[test]
    fn unknown_field_is_located_at_its_key() {
        let text = minimal().replacen("\"spacing\"", "\"spacingg\"", 1);
        let a = analyze(&text, &AnalyzeOptions::default());
        let d = a
            .diagnostics
            .iter()
            .find(|d| d.message.contains("spacingg"))
            .expect("unknown field reported");
        assert_eq!(d.path, "top_bar.spacingg");
        let line = text.lines().position(|l| l.contains("spacingg")).unwrap() as u32 + 1;
        assert_eq!(d.line, Some(line));
        assert!(!a.can_apply());
        assert_eq!(a.schema, CheckStatus::Failed);
    }

    #[test]
    fn invalid_colour_and_duplicate_ids_are_found_with_paths() {
        let mut bar = BarConfig::default();
        bar.style.accent = Some("orange".into());
        let first = bar.modules[0].clone();
        bar.modules.push(first);
        let text = DesignCode::new("T", ThemeSelection::default(), bar).to_text();
        let a = analyze(&text, &AnalyzeOptions::default());
        assert!(
            a.diagnostics
                .iter()
                .any(|d| d.path == "top_bar.style.accent" && d.line.is_some()),
            "{:#?}",
            a.diagnostics
        );
        assert!(
            a.diagnostics
                .iter()
                .any(|d| d.message.contains("used twice"))
        );
        assert!(!a.can_apply());
    }

    #[test]
    fn scripts_urls_paths_and_executables_are_rejected() {
        for (needle, value) in [
            ("script", "<script>alert(1)</script>"),
            ("command", "powershell -enc AAAA"),
            ("external", "https://example.com/x.png"),
            ("path", "C:\\Windows\\System32"),
            ("path", "..\\..\\secret"),
            ("executable", "tool.exe"),
        ] {
            let mut bar = BarConfig::default();
            bar.modules[0].text = Some(value.into());
            let a = analyze(
                &DesignCode::new("T", ThemeSelection::default(), bar).to_text(),
                &AnalyzeOptions::default(),
            );
            assert_eq!(a.no_executable_content, CheckStatus::Failed, "{value}");
            assert!(!a.can_apply(), "{value}");
            assert!(
                a.diagnostics
                    .iter()
                    .any(|d| d.message.contains(needle) || d.message.contains("not accepted")),
                "{value}: {:#?}",
                a.diagnostics
            );
        }
    }

    #[test]
    fn format_version_and_missing_fields() {
        let a = analyze(
            "{\"format\":\"windowsrice-preset\",\"version\":9}",
            &AnalyzeOptions::default(),
        );
        assert!(a.diagnostics.iter().any(|d| d.path == "format"));
        assert!(a.diagnostics.iter().any(|d| d.message.contains("newer")));
        assert!(
            a.diagnostics
                .iter()
                .any(|d| d.message.contains("\"top_bar\""))
        );
        assert!(a.design.is_none());
        let a = analyze("[1,2]", &AnalyzeOptions::default());
        assert_eq!(a.schema, CheckStatus::Failed);
    }

    #[test]
    fn assets_are_checked() {
        let mut d = DesignCode::new("T", ThemeSelection::default(), BarConfig::default());
        d.assets.push(AssetRef {
            id: "sky".into(),
            kind: AssetKind::Image,
            file: "assets/sky.png".into(),
            trim_start_ms: None,
            trim_end_ms: None,
            poster_ms: None,
            r#loop: None,
            speed: None,
        });
        d.top_bar.style.background_asset = Some("sky".into());
        let text = d.to_text();
        // Pasted code: assets cannot be checked, which is said plainly.
        let a = analyze(&text, &AnalyzeOptions::default());
        assert_eq!(a.assets, CheckStatus::NotRun);
        assert!(a.can_apply());
        // In a package with the file: passed; without it: an error.
        let files = vec!["assets/sky.png".to_owned()];
        let a = analyze(
            &text,
            &AnalyzeOptions {
                package_files: Some(&files),
                ..Default::default()
            },
        );
        assert_eq!(a.assets, CheckStatus::Passed, "{:#?}", a.diagnostics);
        let none: Vec<String> = Vec::new();
        let a = analyze(
            &text,
            &AnalyzeOptions {
                package_files: Some(&none),
                ..Default::default()
            },
        );
        assert_eq!(a.assets, CheckStatus::Failed);
        // Bad package path and kind mismatch.
        d.assets[0].file = "assets/../sky.mp4".into();
        let a = analyze(&d.to_text(), &AnalyzeOptions::default());
        assert!(
            a.diagnostics.iter().any(|x| x.path == "assets[0].file"),
            "{:#?}",
            a.diagnostics
        );
        assert!(a.diagnostics.iter().any(|x| x.path == "assets[0].kind"));
        // A background that refers to a missing asset.
        let mut d2 = DesignCode::new("T", ThemeSelection::default(), BarConfig::default());
        d2.top_bar.style.background_asset = Some("nope".into());
        let a = analyze(&d2.to_text(), &AnalyzeOptions::default());
        assert!(
            a.diagnostics
                .iter()
                .any(|x| x.path == "top_bar.style.background_asset")
        );
    }

    #[test]
    fn fonts_and_themes_are_warnings() {
        let mut bar = BarConfig::default();
        bar.style.font_family = Some("Nonexistent Sans".into());
        let mut d = DesignCode::new("T", ThemeSelection::default(), bar);
        d.theme.active = "someone-else".into();
        let fonts = vec!["Segoe UI Variable".to_owned()];
        let themes = vec!["windowsrice-default".to_owned()];
        let a = analyze(
            &d.to_text(),
            &AnalyzeOptions {
                fonts: Some(&fonts),
                themes: Some(&themes),
                package_files: None,
            },
        );
        assert_eq!(a.errors(), 0, "{:#?}", a.diagnostics);
        assert_eq!(a.warnings(), 2, "{:#?}", a.diagnostics);
        assert!(a.can_apply());
    }

    #[test]
    fn animation_limits_are_reported() {
        let mut bar = BarConfig::default();
        let mut an = crate::motion::ColorAnimation::rainbow_text();
        an.duration_ms = 100;
        bar.style.animations.push(an);
        let a = analyze(
            &DesignCode::new("T", ThemeSelection::default(), bar).to_text(),
            &AnalyzeOptions::default(),
        );
        assert!(
            a.diagnostics
                .iter()
                .any(|d| d.path == "top_bar.style.animations[0].duration_ms" && d.line.is_some()),
            "{:#?}",
            a.diagnostics
        );
    }

    #[test]
    fn the_documented_example_is_valid() {
        let doc = include_str!("../../../docs/design-code.md");
        let start = doc.rfind("```json").expect("example") + "```json".len();
        let end = start + doc[start..].find("```").expect("end of example");
        let a = analyze(&doc[start..end], &AnalyzeOptions::default());
        assert!(a.can_apply(), "{:#?}", a.diagnostics);
        assert_eq!(a.design.unwrap().top_bar.style.animations.len(), 1);
    }

    #[test]
    fn style_values_outside_their_ranges_are_located() {
        let mut bar = BarConfig::default();
        bar.style.module_opacity = Some(250);
        bar.style.clock_size = Some(90);
        let a = analyze(
            &DesignCode::new("T", ThemeSelection::default(), bar).to_text(),
            &AnalyzeOptions::default(),
        );
        for f in ["module_opacity", "clock_size"] {
            let d = a
                .diagnostics
                .iter()
                .find(|d| d.path == format!("top_bar.style.{f}"))
                .unwrap_or_else(|| panic!("{f}: {:#?}", a.diagnostics));
            assert!(d.line.is_some() && d.suggestion.is_some());
        }
    }

    #[test]
    fn oversized_input_is_refused_before_parsing() {
        let big = " ".repeat(MAX_DESIGN_BYTES + 1);
        let a = analyze(&big, &AnalyzeOptions::default());
        assert_eq!(a.syntax, CheckStatus::Failed);
    }
}
