#![no_std]

extern crate alloc;

pub mod types;
pub mod event;
pub mod math;
pub mod graphics;
pub mod layout;
pub mod widget;
pub mod window;

pub use window::Window;
pub use widget::Widget;
pub use layout::{Display, FlexDirection};
pub use event::Event;
pub use types::{Color, Size, Align, LinearGradient, GradientDirection, BackgroundStyle};