//! Logging setup built on `tracing`.
//!
//! Read-only tools log to stderr only. Long-running components use rotating
//! daily log files under `%LOCALAPPDATA%\windowsRICE\logs`.

use std::path::Path;

use serde::{Deserialize, Serialize};
use tracing_subscriber::filter::LevelFilter;

use crate::{CoreError, Result};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

impl From<LogLevel> for LevelFilter {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Error => LevelFilter::ERROR,
            LogLevel::Warn => LevelFilter::WARN,
            LogLevel::Info => LevelFilter::INFO,
            LogLevel::Debug => LevelFilter::DEBUG,
            LogLevel::Trace => LevelFilter::TRACE,
        }
    }
}

/// Initializes logging to stderr. Writes no files.
pub fn init_stderr(level: LogLevel) -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::from(level))
        .with_writer(std::io::stderr)
        .with_target(false)
        .try_init()
        .map_err(|e| CoreError::Logging(e.to_string()))
}

/// Keeps the background log writer alive. Logs are flushed when dropped.
#[derive(Debug)]
pub struct LogGuard {
    _guard: tracing_appender::non_blocking::WorkerGuard,
}

/// Initializes daily-rotating file logging in `dir`, keeping `max_files` files.
pub fn init_file(dir: &Path, level: LogLevel, max_files: usize) -> Result<LogGuard> {
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("windowsrice")
        .filename_suffix("log")
        .max_log_files(max_files.max(1))
        .build(dir)
        .map_err(|e| CoreError::Logging(e.to_string()))?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::from(level))
        .with_writer(writer)
        .with_ansi(false)
        .try_init()
        .map_err(|e| CoreError::Logging(e.to_string()))?;
    Ok(LogGuard { _guard: guard })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_map_to_filters() {
        assert_eq!(LevelFilter::from(LogLevel::Warn), LevelFilter::WARN);
        assert_eq!(LevelFilter::from(LogLevel::default()), LevelFilter::INFO);
    }
}
