#![no_std]
#![feature(lang_items)]

extern crate alloc as rust_alloc;
pub mod io;
pub mod memory;
pub mod alloc;
pub mod os;
pub mod sys;
pub mod graphics;
pub use common as event_queue;
pub mod sync;
pub mod fs;
pub mod math;
pub mod time;
pub mod thread;
pub mod env;
pub mod process;
pub mod rt;
pub mod future;
pub mod task;
pub mod executor;
#[macro_use]
pub mod wasm;

#[cfg(feature = "userland")]
pub mod runtime;

#[cfg(feature = "userland")]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    crate::debugln!("[USER PANIC] {}", info);
    crate::os::exit(1);
}

pub use crate::io::serial::{_debug_print, _print};
