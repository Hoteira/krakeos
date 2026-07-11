//! Port of the old x86_64 inkui GUI library to the RISC-V/wasm KrakeOS.
//!
//! Differences from ref/inkui:
//! - Renders into an app-owned Vec<u32> and presents by writing
//!   `[x,y,w,h]+pixels` to `/dev/gpu/window` (no compositor syscalls, no
//!   shared-memory double buffer).
//! - Text is rasterized at runtime by titanf (cached, alpha-blended).
//!   Apps load /fonts/ui.ttf, the ASCII subset built by tools/fontsubset,
//!   which parses in milliseconds (the full Nerd font takes ~16s under the
//!   wasmi interpreter).
//! - Events: keyboard only for now (`/dev/input/keyboard`); the kernel does
//!   not yet route mouse events per-window.

pub mod font;
pub mod event;
pub mod graphics;
pub mod layout;
pub mod math;
pub mod types;
pub mod widget;
pub mod window;

pub use font::Font;
pub use event::{Event, KeyboardEvent};
pub use layout::{Display, FlexDirection};
pub use types::{Align, BackgroundStyle, Color, GradientDirection, LinearGradient, Size};
pub use widget::Widget;
pub use window::Window;
