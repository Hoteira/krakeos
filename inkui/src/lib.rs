// inkui: a small TUI toolkit for KrakeOS apps. Each app owns a full-screen grid
// of cells drawn through /dev/tty; the kernel renders them. There is no mouse.

pub mod app;
pub mod buffer;
pub mod clipboard;
pub mod config;
pub mod event;
pub mod files;
pub mod font;
pub mod icons;
pub mod layout;
pub mod term;
pub mod types;
pub mod widget;

pub use app::{args, run, spawn, App, Control};
pub use buffer::{Buffer, Cell, Line, Span};
pub use event::{Event, Key, KeyEvent};
pub use layout::{Constraint, Rect};
pub use term::Term;
pub use types::{palette, Color, Style};
