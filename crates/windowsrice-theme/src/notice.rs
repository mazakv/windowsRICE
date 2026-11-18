//! User-facing messages produced while loading configuration and themes.

use std::fmt;
use std::path::PathBuf;

use crate::catalog::CatalogIssue;
use crate::schema::Mode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub enum Notice {
    /// No `config.toml`; defaults are in use. Nothing was written.
    ConfigMissing,
    /// `config.toml` could not be used; defaults are in use. The file was
    /// not modified and will be preserved before anything is saved.
    ConfigInvalid {
        error: String,
    },
    /// `config.toml` uses an older schema and was migrated in memory.
    ConfigMigrated {
        from_version: u32,
        notes: Vec<String>,
    },
    /// The original configuration was copied before the first save.
    OriginalConfigPreserved {
        path: PathBuf,
    },
    /// A user theme file was rejected or skipped.
    CatalogIssue(CatalogIssue),
    /// The selected theme id does not exist in the catalog.
    ThemeNotFound {
        requested: String,
    },
    /// The selected theme exists as a file but was rejected.
    ThemeUnavailable {
        requested: String,
        reason: String,
    },
    /// The renderer refused the theme.
    ThemeApplyFailed {
        requested: String,
        message: String,
    },
    FellBackToDefault {
        requested: String,
    },
    /// No candidate (the selected theme, then windowsRICE Default) was
    /// accepted. `kept` names the last known-good theme that is still
    /// applied, or `None` if no theme is confirmed on screen.
    ActivationFailed {
        attempted: Vec<String>,
        message: String,
        kept: Option<String>,
    },
    ModeUnavailable {
        theme: String,
        requested: Mode,
        used: Mode,
    },
    /// The active theme has validation warnings (for example translucency).
    ThemeWarnings {
        theme: String,
        warnings: Vec<String>,
    },
}

fn mode_name(m: Mode) -> &'static str {
    match m {
        Mode::Dark => "dark",
        Mode::Light => "light",
    }
}

impl Notice {
    pub fn level(&self) -> NoticeLevel {
        match self {
            Notice::ConfigMissing
            | Notice::OriginalConfigPreserved { .. }
            | Notice::ConfigMigrated { .. } => NoticeLevel::Info,
            Notice::ModeUnavailable { .. }
            | Notice::ThemeWarnings { .. }
            | Notice::CatalogIssue(_) => NoticeLevel::Warning,
            Notice::ConfigInvalid { .. }
            | Notice::ThemeNotFound { .. }
            | Notice::ThemeUnavailable { .. }
            | Notice::ThemeApplyFailed { .. }
            | Notice::FellBackToDefault { .. }
            | Notice::ActivationFailed { .. } => NoticeLevel::Error,
        }
    }
}

impl fmt::Display for Notice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Notice::ConfigMissing => write!(
                f,
                "No configuration file was found; default settings are in use."
            ),
            Notice::ConfigInvalid { error } => write!(
                f,
                "The configuration file could not be used ({error}). Default settings are in use; \
                 the file was not changed and will be backed up before any new settings are saved."
            ),
            Notice::ConfigMigrated {
                from_version,
                notes,
            } => {
                write!(
                    f,
                    "The configuration was upgraded from schema version {from_version}"
                )?;
                if notes.is_empty() {
                    write!(f, ".")
                } else {
                    write!(f, ": {}.", notes.join("; "))
                }
            }
            Notice::OriginalConfigPreserved { path } => {
                write!(
                    f,
                    "The previous configuration file was preserved at {}.",
                    path.display()
                )
            }
            Notice::CatalogIssue(issue) => write!(f, "Theme file {issue}"),
            Notice::ThemeNotFound { requested } => {
                write!(f, "The selected theme \"{requested}\" was not found.")
            }
            Notice::ThemeUnavailable { requested, reason } => {
                write!(
                    f,
                    "The selected theme \"{requested}\" could not be used: {reason}"
                )
            }
            Notice::ThemeApplyFailed { requested, message } => {
                write!(
                    f,
                    "The theme \"{requested}\" could not be applied: {message}"
                )
            }
            Notice::FellBackToDefault { requested } => write!(
                f,
                "windowsRICE Default is applied instead of \"{requested}\". Your selection was kept in the \
                 configuration and will be used again once the theme is available."
            ),
            Notice::ActivationFailed {
                attempted,
                message,
                kept,
            } => {
                write!(
                    f,
                    "No theme could be applied (tried {}): {message}. ",
                    attempted.join(", ")
                )?;
                match kept {
                    Some(id) => write!(
                        f,
                        "The previously applied theme \"{id}\" remains on screen."
                    ),
                    None => write!(f, "No theme is confirmed on screen."),
                }
            }
            Notice::ModeUnavailable {
                theme,
                requested,
                used,
            } => write!(
                f,
                "The theme \"{theme}\" has no {} mode; its {} mode is used.",
                mode_name(*requested),
                mode_name(*used)
            ),
            Notice::ThemeWarnings { theme, warnings } => {
                write!(
                    f,
                    "The theme \"{theme}\" has {} accessibility warning(s): {}",
                    warnings.len(),
                    warnings.join(" | ")
                )
            }
        }
    }
}
