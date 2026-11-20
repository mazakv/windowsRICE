//! Theme selection connected to `config.toml`.
//!
//! [`ThemeService`] loads the configuration (migrating older schemas in
//! memory), builds the catalog, starts the engine, and persists the user's
//! selection. Persistence rules:
//!
//! - Nothing is written at startup, even when falling back to the default
//!   theme; the user's selection stays in the file so it applies again once
//!   the theme is available.
//! - Before the first save over a migrated or unreadable configuration file,
//!   the original is copied to a uniquely named backup that is never
//!   overwritten.
//! - A selection is only kept if it was both applied and saved. If saving
//!   fails, the previous theme is re-applied and the configuration in memory
//!   is unchanged.

use std::path::{Path, PathBuf};

use windowsrice_core::config::{
    Config, LoadOutcome, ThemeModePreference, ThemeSelection, sibling_with_suffix,
};
use windowsrice_core::paths::AppPaths;

use crate::catalog::ThemeCatalog;
use crate::engine::{ActivationError, ActivationStatus, ResolvedTheme, ThemeEngine, ThemeSink};
use crate::notice::Notice;
use crate::schema::Mode;

#[derive(Debug, thiserror::Error)]
pub enum SelectError {
    #[error(transparent)]
    Activation(#[from] ActivationError),
    #[error(
        "the theme was applied but the selection could not be saved ({message}); {}",
        match revert_error {
            None => "the previous theme was restored".to_owned(),
            Some(e) => format!("restoring the previous theme also failed: {e}"),
        }
    )]
    PersistFailed {
        message: String,
        revert_error: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Preserve {
    Migrated { from_version: u32 },
    Invalid,
}

#[derive(Debug)]
pub struct ThemeService {
    config_path: PathBuf,
    user_themes_dir: Option<PathBuf>,
    config: Config,
    engine: ThemeEngine,
    notices: Vec<Notice>,
    preserve: Option<Preserve>,
}

fn unix_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

impl ThemeService {
    /// Loads configuration and themes and activates the selected theme.
    /// Writes nothing.
    pub fn start(paths: &AppPaths, system_mode: Option<Mode>, sink: &mut dyn ThemeSink) -> Self {
        Self::start_with(
            &paths.config_file(),
            Some(&paths.user_themes_dir()),
            system_mode,
            sink,
        )
    }

    /// Like [`ThemeService::start`] with explicit locations.
    pub fn start_with(
        config_path: &Path,
        user_themes_dir: Option<&Path>,
        system_mode: Option<Mode>,
        sink: &mut dyn ThemeSink,
    ) -> Self {
        let mut notices = Vec::new();
        let (config, outcome) = Config::load_or_default(config_path);
        let preserve = match outcome {
            LoadOutcome::Loaded => None,
            LoadOutcome::Missing => {
                notices.push(Notice::ConfigMissing);
                None
            }
            LoadOutcome::Migrated {
                from_version,
                notes,
            } => {
                notices.push(Notice::ConfigMigrated {
                    from_version,
                    notes,
                });
                Some(Preserve::Migrated { from_version })
            }
            LoadOutcome::Invalid { error } => {
                notices.push(Notice::ConfigInvalid { error });
                Some(Preserve::Invalid)
            }
        };

        let catalog = ThemeCatalog::load(user_themes_dir);
        notices.extend(catalog.issues().iter().cloned().map(Notice::CatalogIssue));
        let (engine, mut engine_notices) =
            ThemeEngine::start(catalog, &config.theme, system_mode, sink);
        notices.append(&mut engine_notices);

        Self {
            config_path: config_path.to_path_buf(),
            user_themes_dir: user_themes_dir.map(Path::to_path_buf),
            config,
            engine,
            notices,
            preserve,
        }
    }

    /// Activates `id` with `mode` and saves the selection. Either both
    /// succeed or neither takes effect.
    pub fn select(
        &mut self,
        id: &str,
        mode: ThemeModePreference,
        system_mode: Option<Mode>,
        sink: &mut dyn ThemeSink,
    ) -> Result<(ResolvedTheme, Vec<Notice>), SelectError> {
        let activated = self.engine.activate(id, mode, system_mode, sink)?;
        let mut notices = activated.notices;

        let selection = ThemeSelection {
            active: id.to_owned(),
            mode,
        };
        let mut next = self.config.clone();
        next.theme = selection.clone();
        let saved = self
            .preserve_original()
            .map(|preserved| {
                if let Some(path) = preserved {
                    notices.push(Notice::OriginalConfigPreserved { path });
                }
            })
            .and_then(|()| {
                next.save_atomic(&self.config_path)
                    .map_err(|e| e.to_string())
            });

        match saved {
            Ok(()) => {
                self.config = next;
                self.engine.set_selection(selection);
                Ok((activated.theme, notices))
            }
            Err(message) => {
                let revert_error = self.engine.revert(sink).err().map(|e| e.to_string());
                Err(SelectError::PersistFailed {
                    message,
                    revert_error,
                })
            }
        }
    }

    /// Copies the original configuration file once, before the first save
    /// that would replace a migrated or unreadable file.
    fn preserve_original(&mut self) -> Result<Option<PathBuf>, String> {
        let Some(reason) = self.preserve else {
            return Ok(None);
        };
        if !self.config_path.is_file() {
            self.preserve = None;
            return Ok(None);
        }
        let tag = match reason {
            Preserve::Migrated { from_version } => format!("v{from_version}"),
            Preserve::Invalid => "invalid".to_owned(),
        };
        let base = format!(".{tag}-{}", unix_ms());
        let mut target = sibling_with_suffix(&self.config_path, &format!("{base}.bak"));
        let mut n = 1;
        while target.exists() {
            target = sibling_with_suffix(&self.config_path, &format!("{base}-{n}.bak"));
            n += 1;
        }
        std::fs::copy(&self.config_path, &target)
            .map_err(|e| format!("could not preserve {}: {e}", self.config_path.display()))?;
        self.preserve = None;
        Ok(Some(target))
    }

    /// Changes configuration other than the theme selection (for example the
    /// `[bar]` section) and saves it. The change is validated first; if it is
    /// invalid or cannot be saved, nothing changes in memory or on disk.
    /// Before the first save over a migrated or unreadable file, the original
    /// is preserved exactly as for [`ThemeService::select`]. The theme
    /// selection is kept as it is; use `select` to change it.
    pub fn update_config(
        &mut self,
        change: impl FnOnce(&mut Config),
    ) -> Result<Vec<Notice>, String> {
        let mut next = self.config.clone();
        change(&mut next);
        next.theme = self.config.theme.clone();
        next.validate().map_err(|e| e.to_string())?;
        let mut notices = Vec::new();
        if let Some(path) = self.preserve_original()? {
            notices.push(Notice::OriginalConfigPreserved { path });
        }
        next.save_atomic(&self.config_path)
            .map_err(|e| e.to_string())?;
        self.config = next;
        Ok(notices)
    }

    /// Folder of user theme files, if known.
    pub fn user_themes_dir(&self) -> Option<&Path> {
        self.user_themes_dir.as_deref()
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    /// Reloads the theme catalog (after a user theme file was created,
    /// changed or deleted) and re-applies the persisted selection. Writes
    /// nothing; if the selected theme disappeared, the usual fallback to
    /// windowsRICE Default applies and is reported in the notices.
    pub fn reload_catalog(
        &mut self,
        system_mode: Option<Mode>,
        sink: &mut dyn ThemeSink,
    ) -> Vec<Notice> {
        let catalog = ThemeCatalog::load(self.user_themes_dir.as_deref());
        let mut notices: Vec<Notice> = catalog
            .issues()
            .iter()
            .cloned()
            .map(Notice::CatalogIssue)
            .collect();
        let (engine, mut n) = ThemeEngine::start(catalog, &self.config.theme, system_mode, sink);
        self.engine = engine;
        notices.append(&mut n);
        notices
    }

    /// Re-applies the persisted selection, for example after Windows
    /// switched between light and dark mode while the mode preference is
    /// Auto. Writes nothing; the selection itself is unchanged.
    pub fn reapply(&mut self, system_mode: Option<Mode>, sink: &mut dyn ThemeSink) -> Vec<Notice> {
        self.engine.apply_selection(system_mode, sink)
    }

    /// The theme the renderer accepted, if any.
    pub fn applied(&self) -> Option<&ResolvedTheme> {
        self.engine.applied()
    }

    /// The persisted selection (may differ from the applied theme).
    pub fn selected(&self) -> &ThemeSelection {
        self.engine.selected()
    }

    pub fn status(&self) -> &ActivationStatus {
        self.engine.status()
    }

    pub fn engine(&self) -> &ThemeEngine {
        &self.engine
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Messages gathered at startup.
    pub fn notices(&self) -> &[Notice] {
        &self.notices
    }
}
