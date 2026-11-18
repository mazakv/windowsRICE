//! Theme activation with validation, mode resolution and rollback.
//!
//! The engine tracks three separate pieces of state:
//!
//! - **selected** — the user's persisted choice ([`ThemeSelection`]). It is
//!   never changed by a failed activation or a fallback.
//! - **applied** — the theme the renderer ([`ThemeSink`]) actually accepted,
//!   or `None` when no theme is known to be on screen.
//! - **status** — the outcome of the most recent activation attempt:
//!   [`ActivationStatus::Pending`], [`ActivationStatus::Applied`] or
//!   [`ActivationStatus::Failed`].
//!
//! A theme is reported as applied only after the sink accepted it. When the
//! selected theme cannot be applied, windowsRICE Default is tried next; if the
//! sink refuses every candidate, the status is `Failed` and the last
//! known-good theme is re-applied where possible.

use windowsrice_core::config::{ThemeModePreference, ThemeSelection};

use crate::catalog::{CatalogEntry, ThemeCatalog, ThemeSource};
use crate::notice::Notice;
use crate::presets::DEFAULT_THEME_ID;
use crate::schema::{Mode, Palette, Theme};
use crate::validate::validate;
use crate::version::ThemeVersionInfo;

/// A theme resolved to one color mode, ready to render.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTheme {
    pub id: String,
    pub name: String,
    pub mode: Mode,
    pub version: ThemeVersionInfo,
    pub source: ThemeSource,
    pub palette: Palette,
    pub theme: Theme,
}

/// Receives themes to display. Returning `Err` rejects the theme.
pub trait ThemeSink {
    fn apply(&mut self, theme: &ResolvedTheme) -> Result<(), String>;
}

/// Accepts every theme. Used until a renderer exists.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopSink;

impl ThemeSink for NoopSink {
    fn apply(&mut self, _theme: &ResolvedTheme) -> Result<(), String> {
        Ok(())
    }
}

/// Outcome of the most recent activation attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationStatus {
    /// Nothing has been sent to the renderer yet.
    Pending,
    /// The applied theme was accepted by the renderer.
    Applied,
    /// The renderer refused every candidate. `applied` holds the last
    /// known-good theme if it could be restored, otherwise `None`.
    Failed { message: String },
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ActivationError {
    #[error("theme \"{id}\" was not found")]
    NotFound { id: String },
    #[error("theme \"{id}\" is unavailable: {reason}")]
    Unavailable { id: String, reason: String },
    #[error(
        "theme \"{id}\" could not be applied ({message}); {}",
        match (restored, restore_error) {
            (Some(r), _) => format!("\"{r}\" remains applied"),
            (None, Some(e)) => format!("re-applying the previous theme also failed ({e}); no theme is confirmed on screen"),
            (None, None) => "no theme was applied before, so none is applied now".to_owned(),
        }
    )]
    ApplyFailed {
        id: String,
        message: String,
        /// The theme that is still applied after the failure, if any.
        restored: Option<String>,
        restore_error: Option<String>,
    },
    #[error("there is no previous theme to return to")]
    NoPrevious,
}

/// Result of a successful activation.
#[derive(Debug, Clone)]
pub struct Activated {
    pub theme: ResolvedTheme,
    pub notices: Vec<Notice>,
}

/// Picks the color mode for `theme`.
pub fn resolve_mode(
    theme: &Theme,
    preference: ThemeModePreference,
    system_mode: Option<Mode>,
) -> (Mode, Option<Notice>) {
    let has = |m: Mode| theme.palette.get(m).is_some();
    let fallback = theme.meta.default_mode;
    let wanted = match preference {
        ThemeModePreference::Dark => Some(Mode::Dark),
        ThemeModePreference::Light => Some(Mode::Light),
        ThemeModePreference::Auto => None,
    };
    match wanted {
        Some(m) if has(m) => (m, None),
        Some(m) => (
            fallback,
            Some(Notice::ModeUnavailable {
                theme: theme.meta.name.clone(),
                requested: m,
                used: fallback,
            }),
        ),
        // Auto: follow Windows when the theme supports that mode.
        None => (system_mode.filter(|m| has(*m)).unwrap_or(fallback), None),
    }
}

fn resolve_entry(
    entry: &CatalogEntry,
    preference: ThemeModePreference,
    system_mode: Option<Mode>,
) -> Result<(ResolvedTheme, Vec<Notice>), ActivationError> {
    // Re-validate immediately before activation; catalog entries were valid
    // when loaded, and this guarantees no unvalidated theme is ever applied.
    let report = validate(&entry.theme);
    if report.has_errors() {
        return Err(ActivationError::Unavailable {
            id: entry.id().to_owned(),
            reason: report.to_string(),
        });
    }
    let (mode, mode_notice) = resolve_mode(&entry.theme, preference, system_mode);
    let Some(palette) = entry.theme.palette.get(mode).cloned() else {
        return Err(ActivationError::Unavailable {
            id: entry.id().to_owned(),
            reason: "the theme has no palette for the resolved mode".into(),
        });
    };
    let mut notices: Vec<Notice> = mode_notice.into_iter().collect();
    let warnings: Vec<String> = report
        .warnings()
        .map(|w| format!("{}: {}", w.path, w.message))
        .collect();
    if !warnings.is_empty() {
        notices.push(Notice::ThemeWarnings {
            theme: entry.theme.meta.name.clone(),
            warnings,
        });
    }
    Ok((
        ResolvedTheme {
            id: entry.id().to_owned(),
            name: entry.theme.meta.name.clone(),
            mode,
            version: entry.version.clone(),
            source: entry.source.clone(),
            palette,
            theme: entry.theme.clone(),
        },
        notices,
    ))
}

fn lookup(
    catalog: &ThemeCatalog,
    id: &str,
    preference: ThemeModePreference,
    system_mode: Option<Mode>,
) -> Result<(ResolvedTheme, Vec<Notice>), ActivationError> {
    match catalog.get(id) {
        Some(entry) => resolve_entry(entry, preference, system_mode),
        None => Err(match catalog.issue_for(id) {
            Some(issue) => ActivationError::Unavailable {
                id: id.to_owned(),
                reason: issue.to_string(),
            },
            None => ActivationError::NotFound { id: id.to_owned() },
        }),
    }
}

#[derive(Debug)]
pub struct ThemeEngine {
    catalog: ThemeCatalog,
    selected: ThemeSelection,
    applied: Option<ResolvedTheme>,
    status: ActivationStatus,
    previous: Option<ResolvedTheme>,
}

impl ThemeEngine {
    /// Creates an engine without contacting the renderer (status `Pending`).
    pub fn prepare(catalog: ThemeCatalog, selection: &ThemeSelection) -> Self {
        Self {
            catalog,
            selected: selection.clone(),
            applied: None,
            status: ActivationStatus::Pending,
            previous: None,
        }
    }

    /// Prepares the engine and applies the selection (see [`Self::apply_selection`]).
    pub fn start(
        catalog: ThemeCatalog,
        selection: &ThemeSelection,
        system_mode: Option<Mode>,
        sink: &mut dyn ThemeSink,
    ) -> (Self, Vec<Notice>) {
        let mut engine = Self::prepare(catalog, selection);
        let notices = engine.apply_selection(system_mode, sink);
        (engine, notices)
    }

    /// Applies the selected theme, or windowsRICE Default if the selected
    /// theme is missing, rejected or refused. The selection itself is never
    /// changed. If every candidate is refused, the status becomes `Failed`
    /// and the last known-good theme (if any) is re-applied.
    pub fn apply_selection(
        &mut self,
        system_mode: Option<Mode>,
        sink: &mut dyn ThemeSink,
    ) -> Vec<Notice> {
        let mut notices = Vec::new();
        let requested = self.selected.active.clone();
        let mut candidates = vec![requested.clone()];
        if requested != DEFAULT_THEME_ID {
            candidates.push(DEFAULT_THEME_ID.to_owned());
        }

        let mut last_error = String::new();
        for (index, id) in candidates.iter().enumerate() {
            match lookup(&self.catalog, id, self.selected.mode, system_mode) {
                Ok((resolved, mut extra)) => match sink.apply(&resolved) {
                    Ok(()) => {
                        if index > 0 {
                            notices.push(Notice::FellBackToDefault {
                                requested: requested.clone(),
                            });
                        }
                        notices.append(&mut extra);
                        self.commit_applied(resolved);
                        return notices;
                    }
                    Err(message) => {
                        last_error.clone_from(&message);
                        notices.push(Notice::ThemeApplyFailed {
                            requested: id.clone(),
                            message,
                        });
                    }
                },
                Err(ActivationError::NotFound { id }) => {
                    last_error = format!("theme \"{id}\" was not found");
                    notices.push(Notice::ThemeNotFound { requested: id });
                }
                Err(ActivationError::Unavailable { id, reason }) => {
                    last_error.clone_from(&reason);
                    notices.push(Notice::ThemeUnavailable {
                        requested: id,
                        reason,
                    });
                }
                Err(other) => {
                    last_error = other.to_string();
                    notices.push(Notice::ThemeUnavailable {
                        requested: id.clone(),
                        reason: other.to_string(),
                    });
                }
            }
        }

        // Every candidate failed. Keep the last known-good theme on screen if
        // the renderer still accepts it; never claim a refused theme is applied.
        if let Some(known_good) = self.applied.clone()
            && let Err(e) = sink.apply(&known_good)
        {
            last_error = format!(
                "{last_error}; re-applying \"{}\" also failed: {e}",
                known_good.id
            );
            self.applied = None;
        }
        notices.push(Notice::ActivationFailed {
            attempted: candidates,
            message: last_error.clone(),
            kept: self.applied.as_ref().map(|t| t.id.clone()),
        });
        self.status = ActivationStatus::Failed {
            message: last_error,
        };
        notices
    }

    /// Applies theme `id` without changing the selection. On failure the
    /// previously applied theme is re-applied; if that also fails, no theme
    /// is reported as applied and the status becomes `Failed`.
    pub fn activate(
        &mut self,
        id: &str,
        preference: ThemeModePreference,
        system_mode: Option<Mode>,
        sink: &mut dyn ThemeSink,
    ) -> Result<Activated, ActivationError> {
        let (resolved, notices) = lookup(&self.catalog, id, preference, system_mode)?;
        self.apply_or_restore(resolved.clone(), sink)?;
        Ok(Activated {
            theme: resolved,
            notices,
        })
    }

    /// Re-applies the theme that was applied before the current one.
    pub fn revert(&mut self, sink: &mut dyn ThemeSink) -> Result<(), ActivationError> {
        let previous = self.previous.clone().ok_or(ActivationError::NoPrevious)?;
        self.apply_or_restore(previous, sink)
    }

    /// Records a selection that has been persisted. Does not apply it.
    pub fn set_selection(&mut self, selection: ThemeSelection) {
        self.selected = selection;
    }

    fn commit_applied(&mut self, theme: ResolvedTheme) {
        self.previous = self.applied.replace(theme);
        self.status = ActivationStatus::Applied;
    }

    fn apply_or_restore(
        &mut self,
        next: ResolvedTheme,
        sink: &mut dyn ThemeSink,
    ) -> Result<(), ActivationError> {
        match sink.apply(&next) {
            Ok(()) => {
                self.commit_applied(next);
                Ok(())
            }
            Err(message) => {
                // The sink may have partially applied `next`; restore what was there.
                let (restored, restore_error) = match &self.applied {
                    Some(current) => match sink.apply(current) {
                        Ok(()) => (Some(current.id.clone()), None),
                        Err(e) => (None, Some(e)),
                    },
                    None => (None, None),
                };
                if restored.is_none() {
                    self.applied = None;
                    self.status = ActivationStatus::Failed {
                        message: format!("theme \"{}\" was refused: {message}", next.id),
                    };
                }
                Err(ActivationError::ApplyFailed {
                    id: next.id,
                    message,
                    restored,
                    restore_error,
                })
            }
        }
    }

    /// The user's persisted selection (unchanged by fallbacks and failures).
    pub fn selected(&self) -> &ThemeSelection {
        &self.selected
    }

    /// The theme the renderer accepted, if any.
    pub fn applied(&self) -> Option<&ResolvedTheme> {
        self.applied.as_ref()
    }

    pub fn status(&self) -> &ActivationStatus {
        &self.status
    }

    /// True when a theme other than the selected one is applied.
    pub fn is_fallback(&self) -> bool {
        self.applied
            .as_ref()
            .is_some_and(|a| a.id != self.selected.active)
    }

    pub fn previous(&self) -> Option<&ResolvedTheme> {
        self.previous.as_ref()
    }

    pub fn catalog(&self) -> &ThemeCatalog {
        &self.catalog
    }
}
