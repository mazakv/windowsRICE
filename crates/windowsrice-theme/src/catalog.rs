//! Theme catalog: the built-in presets plus user theme files.
//!
//! User themes are loaded from one directory (non-recursive). Every file is
//! treated as untrusted: symbolic links and other non-regular entries are
//! skipped, size is checked before reading, the content must be UTF-8 TOML,
//! and the theme must pass full validation. User themes may not reuse a
//! built-in id or display name, so a file cannot impersonate a built-in theme.
//! Rejected files are kept as [`CatalogIssue`]s with a readable reason.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::presets::{BUILTIN_PRESETS, DEFAULT_THEME_ID};
use crate::schema::Theme;
use crate::validate::ValidationReport;
use crate::version::{ThemeVersion, ThemeVersionInfo, fingerprint};
use crate::{MAX_THEME_FILE_BYTES, ThemeError, declared_theme_id, parse_theme};

/// Maximum number of `*.toml` files read from the user theme directory.
pub const MAX_USER_THEME_FILES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeSource {
    Builtin { file_name: &'static str },
    User { path: PathBuf },
}

impl fmt::Display for ThemeSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThemeSource::Builtin { file_name } => write!(f, "built-in ({file_name})"),
            ThemeSource::User { path } => write!(f, "{}", path.display()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub theme: Theme,
    pub source: ThemeSource,
    /// Validation warnings (errors are impossible for catalog entries).
    pub report: ValidationReport,
    pub version: ThemeVersionInfo,
}

impl CatalogEntry {
    pub fn id(&self) -> &str {
        &self.theme.meta.id
    }

    pub fn is_builtin(&self) -> bool {
        matches!(self.source, ThemeSource::Builtin { .. })
    }
}

#[derive(Debug, Clone)]
pub enum IssueKind {
    /// The directory or file could not be read.
    Io(String),
    /// A symbolic link, directory or other non-regular entry.
    NotRegularFile,
    TooLarge {
        size: u64,
    },
    NotUtf8,
    /// Parse, schema-compatibility or validation failure.
    Theme(ThemeError),
    ReservedBuiltinId,
    ReservedBuiltinName {
        name: String,
    },
    DuplicateId {
        first: PathBuf,
    },
    TooManyFiles {
        limit: usize,
    },
}

#[derive(Debug, Clone)]
pub struct CatalogIssue {
    pub path: PathBuf,
    /// The theme id declared in the file, when it could be read.
    pub theme_id: Option<String>,
    pub kind: IssueKind,
}

impl fmt::Display for CatalogIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: ", self.path.display())?;
        match &self.kind {
            IssueKind::Io(e) => write!(f, "could not be read: {e}"),
            IssueKind::NotRegularFile => write!(
                f,
                "skipped: not a regular file (links and folders are not loaded)"
            ),
            IssueKind::TooLarge { size } => {
                write!(
                    f,
                    "skipped: {size} bytes exceeds the {MAX_THEME_FILE_BYTES}-byte limit"
                )
            }
            IssueKind::NotUtf8 => write!(f, "rejected: the file is not UTF-8 text"),
            IssueKind::Theme(e) => write!(f, "rejected: {e}"),
            IssueKind::ReservedBuiltinId => write!(
                f,
                "rejected: id \"{}\" belongs to a built-in theme; give the theme its own id",
                self.theme_id.as_deref().unwrap_or("?")
            ),
            IssueKind::ReservedBuiltinName { name } => {
                write!(
                    f,
                    "rejected: name \"{name}\" belongs to a built-in theme; give the theme its own name"
                )
            }
            IssueKind::DuplicateId { first } => write!(
                f,
                "rejected: id \"{}\" is already used by {}",
                self.theme_id.as_deref().unwrap_or("?"),
                first.display()
            ),
            IssueKind::TooManyFiles { limit } => {
                write!(
                    f,
                    "only the first {limit} theme files (by name) were loaded"
                )
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ThemeCatalog {
    entries: Vec<CatalogEntry>,
    issues: Vec<CatalogIssue>,
}

fn version_info(theme: &Theme, bytes: &[u8]) -> ThemeVersionInfo {
    ThemeVersionInfo {
        id: theme.meta.id.clone(),
        // `meta.version` passed validation, which requires MAJOR.MINOR.PATCH.
        version: theme.meta.version.parse().unwrap_or(ThemeVersion {
            major: 0,
            minor: 0,
            patch: 0,
        }),
        schema_version: theme.meta.schema_version,
        fingerprint: fingerprint(bytes),
    }
}

impl ThemeCatalog {
    /// The eight built-in presets only.
    pub fn builtin_only() -> Self {
        let mut catalog = Self::default();
        for preset in &BUILTIN_PRESETS {
            match parse_theme(preset.source) {
                Ok(v) => catalog.entries.push(CatalogEntry {
                    version: version_info(&v.theme, preset.source.as_bytes()),
                    theme: v.theme,
                    report: v.report,
                    source: ThemeSource::Builtin {
                        file_name: preset.file_name,
                    },
                }),
                // Guarded by tests; recorded rather than panicking.
                Err(e) => catalog.issues.push(CatalogIssue {
                    path: PathBuf::from(format!("builtin:{}", preset.file_name)),
                    theme_id: Some(preset.id.to_owned()),
                    kind: IssueKind::Theme(e),
                }),
            }
        }
        catalog
    }

    /// Built-in presets plus valid themes from `user_dir`. A missing
    /// directory is not an error.
    pub fn load(user_dir: Option<&Path>) -> Self {
        let mut catalog = Self::builtin_only();
        if let Some(dir) = user_dir {
            catalog.load_user_dir(dir);
        }
        catalog
    }

    fn load_user_dir(&mut self, dir: &Path) {
        let read_dir = match std::fs::read_dir(dir) {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => {
                self.issue(dir.to_path_buf(), None, IssueKind::Io(e.to_string()));
                return;
            }
        };
        let mut paths: Vec<PathBuf> = read_dir
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("toml"))
            })
            .collect();
        paths.sort();
        if paths.len() > MAX_USER_THEME_FILES {
            paths.truncate(MAX_USER_THEME_FILES);
            self.issue(
                dir.to_path_buf(),
                None,
                IssueKind::TooManyFiles {
                    limit: MAX_USER_THEME_FILES,
                },
            );
        }
        for path in paths {
            self.load_user_file(path);
        }
    }

    fn issue(&mut self, path: PathBuf, theme_id: Option<String>, kind: IssueKind) {
        self.issues.push(CatalogIssue {
            path,
            theme_id,
            kind,
        });
    }

    fn load_user_file(&mut self, path: PathBuf) {
        // symlink_metadata does not follow links, so links are never loaded.
        let meta = match std::fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) => return self.issue(path, None, IssueKind::Io(e.to_string())),
        };
        if !meta.file_type().is_file() {
            return self.issue(path, None, IssueKind::NotRegularFile);
        }
        if meta.len() > MAX_THEME_FILE_BYTES as u64 {
            return self.issue(path, None, IssueKind::TooLarge { size: meta.len() });
        }
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => return self.issue(path, None, IssueKind::Io(e.to_string())),
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            return self.issue(path, None, IssueKind::NotUtf8);
        };
        let declared_id = declared_theme_id(text);
        let validated = match parse_theme(text) {
            Ok(v) => v,
            Err(e) => return self.issue(path, declared_id, IssueKind::Theme(e)),
        };
        let theme = validated.theme;
        let id = theme.meta.id.clone();

        if BUILTIN_PRESETS.iter().any(|p| p.id == id) {
            return self.issue(path, Some(id), IssueKind::ReservedBuiltinId);
        }
        let builtin_name = self.entries.iter().filter(|e| e.is_builtin()).any(|e| {
            e.theme
                .meta
                .name
                .eq_ignore_ascii_case(theme.meta.name.trim())
        });
        if builtin_name {
            let name = theme.meta.name.clone();
            return self.issue(path, Some(id), IssueKind::ReservedBuiltinName { name });
        }
        if let Some(first) = self.entries.iter().find(|e| e.id() == id) {
            let first = match &first.source {
                ThemeSource::User { path } => path.clone(),
                ThemeSource::Builtin { file_name } => PathBuf::from(*file_name),
            };
            return self.issue(path, Some(id), IssueKind::DuplicateId { first });
        }

        self.entries.push(CatalogEntry {
            version: version_info(&theme, &bytes),
            theme,
            report: validated.report,
            source: ThemeSource::User { path },
        });
    }

    pub fn entries(&self) -> &[CatalogEntry] {
        &self.entries
    }

    pub fn issues(&self) -> &[CatalogIssue] {
        &self.issues
    }

    pub fn get(&self, id: &str) -> Option<&CatalogEntry> {
        self.entries.iter().find(|e| e.id() == id)
    }

    /// The first rejected file that declared `id`, explaining why a theme
    /// the user selected is unavailable.
    pub fn issue_for(&self, id: &str) -> Option<&CatalogIssue> {
        self.issues
            .iter()
            .find(|i| i.theme_id.as_deref() == Some(id))
    }

    pub fn default_entry(&self) -> Option<&CatalogEntry> {
        self.get(DEFAULT_THEME_ID)
    }
}
