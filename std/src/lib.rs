#![no_std]
#![feature(lang_items)]
#![feature(core_intrinsics)]
#![feature(asm_experimental_arch)]

extern crate alloc as rust_alloc;

pub mod alloc;
pub mod env;
pub mod fs;
pub mod future;
pub mod io;
pub mod math;
pub mod memory;
pub mod net;
pub mod os;
pub mod process;
pub mod random;
pub mod rt;
pub mod sync;
pub mod sys;
pub mod task;
pub mod thread;
pub mod time;
pub mod wasi;

#[cfg(feature = "userland")]
#[macro_use]
pub mod wasm;

#[cfg(feature = "userland")]
#[cfg_attr(not(test), panic_handler)]
fn panic(info: &core::panic::PanicInfo) -> ! {
    crate::debugln!("[USER PANIC] {}", info);
    crate::os::exit(1);
}

pub use crate::io::serial::{_debug_print, _print};