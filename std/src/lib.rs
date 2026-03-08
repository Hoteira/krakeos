#![no_std]
#![feature(lang_items)]
#![feature(core_intrinsics)]
#![feature(asm_experimental_arch)]

extern crate alloc as alloc;

#[macro_export]
macro_rules! method_export {
    ($module:literal, $method:literal,
     pub fn $name:ident($($arg:ident: $ty:ty),* $(,)?) $(-> $ret:ty)? $body:block
    ) => {
        #[cfg(target_arch = "wasm32")]
        pub fn $name($($arg: $ty),*) $(-> $ret)? {
            #[link(wasm_import_module = $module)]
            unsafe extern "C" {
                #[link_name = $method]
                fn __raw($($arg: $ty),*) $(-> $ret)?;
            }
            unsafe { __raw($($arg),*) }
        }

        #[cfg(not(target_arch = "wasm32"))]
        pub fn $name($($arg: $ty),*) $(-> $ret)? {
            unsafe { (|| $body)() }
        }
    };
}

#[cfg(any(feature = "userland", target_arch = "x86_64"))]
#[macro_use]
pub mod wasm;

pub mod allocator;
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

#[cfg(any(feature = "userland", target_arch = "wasm32"))]
#[cfg_attr(not(test), panic_handler)]
fn panic(info: &core::panic::PanicInfo) -> ! {
    crate::os::debug_print("[USER PANIC] A panic occurred in WASM land!\n");
    crate::debugln!("[USER PANIC] {}", info);
    crate::os::exit(1);
}

pub use crate::io::serial::{_debug_print, _print};
