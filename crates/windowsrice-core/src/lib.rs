//! windowsRICE core library.
//!
//! This crate is platform-independent. It owns the configuration schema,
//! application paths, logging setup, the hotkey model and the capability
//! model. Windows API access lives in `windowsrice-windows`.

#![forbid(unsafe_code)]

pub mod bar;
pub mod capability;
pub mod config;
pub mod design_code;
pub mod design_pack;
pub mod error;
pub mod hotkey;
pub mod logging;
pub mod motion;
pub mod paths;
pub mod preset;

pub use error::{CoreError, Result};

/// Display name of the project.
pub const APP_NAME: &str = "windowsRICE";

/// Directory name used under `%APPDATA%` and `%LOCALAPPDATA%`.
pub const APP_DIR_NAME: &str = "windowsRICE";

/// Version of the windowsRICE workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
