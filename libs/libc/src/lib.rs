#![feature(c_variadic)]
#![no_std]
#![feature(naked_functions)]

#[macro_use]
extern crate std;
extern crate alloc;

use core::ffi::{c_int, c_char};

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

unsafe extern "C" {
    fn main(argc: c_int, argv: *mut *mut c_char) -> c_int;
}

#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        "mov rdi, rsp",
        "and rsp, -16",
        "call rust_start",
        "hlt",
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rust_start(stack: *const usize) -> ! {
    let size = 128 * 1024 * 1024;
    let ptr = std::memory::malloc(size) as *mut u8;
    std::memory::heap::init_heap(ptr, size);
    let argc = *stack as c_int;
    let argv = stack.add(1) as *mut *mut c_char;
    let result = main(argc, argv);
    stdlib::exit(result);
}

#[unsafe(no_mangle)]
pub static mut errno: c_int = 0;

#[panic_handler] pub fn panic(i: &core::panic::PanicInfo) -> ! { std::println!("[USER PANIC] {}", i); loop {} }
