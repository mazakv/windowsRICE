use std::path::PathBuf;

/// Errors produced by the core crate.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse configuration {path}: {message}")]
    ConfigParse { path: PathBuf, message: String },

    #[error("invalid configuration: {0}")]
    ConfigInvalid(String),

    #[error("failed to serialize configuration: {0}")]
    ConfigSerialize(String),

    #[error("invalid hotkey \"{input}\": {reason}")]
    Hotkey { input: String, reason: String },

    #[error("required environment variable {0} is not set")]
    MissingEnv(&'static str),

    #[error("logging initialization failed: {0}")]
    Logging(String),
}

impl CoreError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;
