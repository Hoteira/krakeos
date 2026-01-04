#![feature(c_variadic)]
#![no_std]
#![feature(naked_functions)]

#[macro_use]
extern crate std;
extern crate alloc;

pub mod string;
pub mod stdlib;
pub mod stdio;
pub mod unistd;
pub mod terminal;
pub mod fs;
pub mod misc;
pub mod process;

use core::ffi::c_int;

#[unsafe(no_mangle)]
pub static mut errno: c_int = 0;

// Force inclusion of std's runtime entry point
#[unsafe(no_mangle)]
pub static _RUNTIME_REF: unsafe extern "C" fn() -> ! = std::_start;

pub use terminal::termios;

pub const O_RDONLY: c_int = 0;
pub const O_WRONLY: c_int = 1;
pub const O_RDWR: c_int = 2;
pub const O_CREAT: c_int = 64;