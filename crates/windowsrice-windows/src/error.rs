use serde::Serialize;
use windows::Win32::Foundation::WIN32_ERROR;

/// A failed Windows API call, with the code and system message preserved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WinError {
    #[error("{api} failed: {message} (0x{code:08X})")]
    Api {
        api: &'static str,
        code: u32,
        message: String,
    },
    #[error("{0}")]
    Unavailable(String),
}

impl WinError {
    pub(crate) fn from_windows(api: &'static str, e: &windows::core::Error) -> Self {
        Self::Api {
            api,
            code: e.code().0.cast_unsigned(),
            message: e.message().trim().to_owned(),
        }
    }

    pub(crate) fn from_win32(api: &'static str, code: WIN32_ERROR) -> Self {
        Self::from_windows(api, &windows::core::Error::from(code.to_hresult()))
    }
}

/// Adapter for `map_err` on `windows::core::Result`.
pub(crate) fn api(api: &'static str) -> impl FnOnce(windows::core::Error) -> WinError {
    move |e| WinError::from_windows(api, &e)
}
