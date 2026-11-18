use crate::validate::ValidationReport;

#[derive(Debug, Clone, thiserror::Error)]
pub enum ThemeError {
    #[error("theme file is {size} bytes; the maximum is {max} bytes")]
    TooLarge { size: usize, max: usize },

    #[error("theme file could not be parsed: {0}")]
    Parse(String),

    /// The file declares a newer schema than this build understands and does
    /// not parse under the current schema.
    #[error(
        "theme uses schema version {found}, but this build of windowsRICE supports version {supported}; \
         update windowsRICE or use a theme made for this version ({detail})"
    )]
    Incompatible {
        found: u32,
        supported: u32,
        detail: String,
    },

    #[error("theme is invalid:\n{0}")]
    Invalid(ValidationReport),

    #[error("theme could not be serialized: {0}")]
    Serialize(String),
}
