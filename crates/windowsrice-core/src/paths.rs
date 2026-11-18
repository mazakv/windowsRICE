//! Per-user application directories.
//!
//! Configuration roams with the user profile (`%APPDATA%\windowsRICE`).
//! Machine-local state such as logs, the change journal and backups lives in
//! `%LOCALAPPDATA%\windowsRICE`. Resolving paths never touches the file
//! system; only [`AppPaths::ensure_dirs`] creates directories.

use std::path::{Path, PathBuf};

use crate::{APP_DIR_NAME, CoreError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub journal_dir: PathBuf,
    pub backup_dir: PathBuf,
    pub state_dir: PathBuf,
}

impl AppPaths {
    /// Resolves paths from `%APPDATA%` and `%LOCALAPPDATA%`.
    pub fn from_env() -> Result<Self> {
        let roaming = std::env::var_os("APPDATA").ok_or(CoreError::MissingEnv("APPDATA"))?;
        let local =
            std::env::var_os("LOCALAPPDATA").ok_or(CoreError::MissingEnv("LOCALAPPDATA"))?;
        Ok(Self::from_roots(Path::new(&roaming), Path::new(&local)))
    }

    /// Resolves paths beneath explicit roots. Used by tests and portable mode.
    pub fn from_roots(roaming_root: &Path, local_root: &Path) -> Self {
        let config_dir = roaming_root.join(APP_DIR_NAME);
        let data_dir = local_root.join(APP_DIR_NAME);
        Self {
            log_dir: data_dir.join("logs"),
            journal_dir: data_dir.join("journal"),
            backup_dir: data_dir.join("backups"),
            state_dir: data_dir.join("state"),
            config_dir,
            data_dir,
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn journal_file(&self) -> PathBuf {
        self.journal_dir.join("changes.jsonl")
    }

    /// User-installed theme files (`*.toml`). Roams with the profile. Not
    /// created automatically; a missing directory means no user themes.
    pub fn user_themes_dir(&self) -> PathBuf {
        self.config_dir.join("themes")
    }

    /// Creates all application directories. Read-only tools must not call this.
    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in [
            &self.config_dir,
            &self.data_dir,
            &self.log_dir,
            &self.journal_dir,
            &self.backup_dir,
            &self.state_dir,
        ] {
            std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir, e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_separates_roaming_config_from_local_state() {
        let p = AppPaths::from_roots(Path::new(r"C:\R"), Path::new(r"C:\L"));
        assert_eq!(p.config_dir, Path::new(r"C:\R\windowsRICE"));
        assert_eq!(p.config_file(), Path::new(r"C:\R\windowsRICE\config.toml"));
        assert_eq!(p.log_dir, Path::new(r"C:\L\windowsRICE\logs"));
        assert_eq!(
            p.journal_file(),
            Path::new(r"C:\L\windowsRICE\journal\changes.jsonl")
        );
    }

    #[test]
    fn resolving_paths_does_not_create_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let p = AppPaths::from_roots(&tmp.path().join("r"), &tmp.path().join("l"));
        assert!(!p.config_dir.exists());
        assert!(!p.data_dir.exists());
    }

    #[test]
    fn ensure_dirs_creates_everything() {
        let tmp = tempfile::tempdir().unwrap();
        let p = AppPaths::from_roots(&tmp.path().join("r"), &tmp.path().join("l"));
        p.ensure_dirs().unwrap();
        for d in [
            &p.config_dir,
            &p.log_dir,
            &p.journal_dir,
            &p.backup_dir,
            &p.state_dir,
        ] {
            assert!(d.is_dir(), "{} missing", d.display());
        }
    }
}
