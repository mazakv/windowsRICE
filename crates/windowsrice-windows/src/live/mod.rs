//! Read-only live system state for windowsRICE surfaces (Phase 4).
//!
//! Everything here observes the system without changing it: processor and
//! memory load, GPU engine utilisation (performance counters), the default
//! audio endpoint's volume, network connectivity, the foreground window,
//! fullscreen state, local time, monitors and appearance settings. The
//! `read_only_guard` test covers these files like the rest of the crate.
//!
//! Functions that use COM (`audio`, `network`) require COM to be initialised
//! on the calling thread.

pub mod appearance;
pub mod audio;
pub mod clock;
pub mod foreground;
pub mod metrics;
pub mod monitors;
pub mod network;
pub mod power;
pub mod traffic;
