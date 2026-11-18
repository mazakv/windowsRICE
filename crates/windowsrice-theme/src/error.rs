use crate::validate::ValidationReport;

#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    #[error("theme file is {size} bytes; the maximum is {max} bytes")]
    TooLarge { size: usize, max: usize },

    #[error("theme file could not be parsed: {0}")]
    Parse(String),

    #[error("theme is invalid:\n{0}")]
    Invalid(ValidationReport),

    #[error("theme could not be serialized: {0}")]
    Serialize(String),
}
