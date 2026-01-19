#![feature(c_variadic)]
#![no_std]
#![feature(naked_functions)]

extern crate alloc;
#[macro_use]
extern crate std;

use core::ffi::c_int;

pub mod string;
pub mod ctype;
pub mod stdlib;
pub mod stdio;
pub mod math;
pub mod unistd;
pub mod sys;
pub mod curses;
pub mod dirent;
pub mod locale;
pub mod time;
pub mod pthread;

#[cfg(feature = "userland")]
pub use std::runtime::*;

#[unsafe(no_mangle)]
pub static mut errno: c_int = 0;
