//! windowsRICE native renderer.
//!
//! Pipeline, per window:
//!
//! 1. A Direct3D 11 device with BGRA support (hardware; WARP if no hardware
//!    device can be created).
//! 2. A DXGI swap chain *for composition* (flip-sequential, two buffers,
//!    premultiplied alpha), so transparent pixels show the DWM backdrop.
//! 3. A Direct2D device context drawing into the back buffer at the
//!    window's DPI, so all drawing is in DIPs.
//! 4. DirectWrite text with an explicit font fallback (Hangul → the theme's
//!    fallback family, normally Malgun Gothic) and exact variable-font weights.
//! 5. A DirectComposition visual hosting the swap chain on a
//!    `WS_EX_NOREDIRECTIONBITMAP` window, with an effect group whose opacity
//!    can be animated by DWM (used for the show fade).
//!
//! Frames are drawn only when the caller asks (event-driven). Device loss is
//! reported as [`RenderError::DeviceLost`]; [`Renderer::recover`] rebuilds
//! every device-dependent object while keeping DirectWrite state.

mod device;
mod error;
mod image;
mod text;

pub use device::{FrameStats, RenderInfo, Renderer};
pub use error::RenderError;
pub use text::{FontConfig, InstalledFamily, TextSystem};
