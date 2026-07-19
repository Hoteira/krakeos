//! Port of the old x86_64 inkui GUI library to the RISC-V/wasm KrakeOS.
//!
//! Differences from ref/inkui:
//! - Renders into an app-owned Vec<u32> and presents by writing
//!   `[x,y,w,h]+pixels` to `/dev/gpu/window` (no compositor syscalls, no

//! - Events: keyboard only for now (`/dev/input/keyboard`); the kernel does
//!   not yet route mouse events per-window.

pub mod event;
pub mod font;
pub mod graphics;
pub mod layout;
pub mod math;
pub mod types;
pub mod widget;
pub mod window;

pub use event::{Event, KeyboardEvent};
pub use font::Font;
pub use layout::{Display, FlexDirection};
pub use types::{Align, BackgroundStyle, Color, GradientDirection, LinearGradient, Size};
pub use widget::Widget;
pub use window::Window;
