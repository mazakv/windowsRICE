//! windowsRICE Windows integration layer.
//!
//! Scope: **read-only queries only** (Phase 1 diagnostics, and the Phase 4
//! live state in [`live`]). Nothing in this crate writes to the registry,
//! changes system settings, registers startup entries,
//! modifies the shell or requests elevation. The `read_only_guard` test in
//! `apps/diag` enforces this by rejecting known write APIs in the source.
//!
//! The only process-local state change is opting the *calling process* into
//! Per-Monitor-v2 DPI awareness so that display measurements are reported in
//! physical pixels. That affects no other process and no system setting.

#[cfg(not(windows))]
compile_error!("windowsrice-windows supports Windows only");

pub mod display;
pub mod error;
pub mod gpu;
pub mod live;
pub mod registry_read;
pub mod report;
pub mod session;
pub mod system_info;
mod util;

pub use error::WinError;
pub use report::{DiagnosticsReport, Section, collect};
