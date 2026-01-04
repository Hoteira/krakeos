#![no_std]
#![feature(linkage)]
#![feature(naked_functions)]

pub mod io;
pub mod memory;
pub mod os;
pub mod graphics;
pub mod sync;
pub mod fs;

pub use crate::io::serial::_print;
pub use crate::io::serial::_debug_print;

extern crate alloc;

#[cfg(feature = "userland")]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> !
{
    crate::println!("\n\x1B[91mPROCESS PANIC:\x1B[0m {}", info);
    os::exit(1);
}

#[cfg(feature = "userland")]
unsafe extern "C" {
    fn main(argc: i32, argv: *const *const u8) -> i32;
}

#[cfg(feature = "userland")]
#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn _start() -> !
{
    core::arch::naked_asm!(
        "mov rdi, rsp",
        "and rsp, -16",
        "call rust_start",
        "hlt"
    );
}

static mut GLOBAL_ARGS: Option<alloc::vec::Vec<alloc::string::String>> = None;

#[cfg(feature = "userland")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rust_start(stack: *const usize) -> !
{
    // 1. Parse argc/argv from stack (Kernel set this up)
    let argc = *stack as i32;
    let argv = stack.add(1) as *const *const u8;

    // 2. Allocate 10MiB heap
    let heap_size = 10 * 1024 * 1024;
    let heap_ptr = memory::malloc(heap_size);
    memory::heap::init_heap(heap_ptr as *mut u8, heap_size);

    // 3. Populate GLOBAL_ARGS
    let mut args_vec = alloc::vec::Vec::new();
    for i in 0..argc {
        let arg_ptr = *argv.add(i as usize);
        if !arg_ptr.is_null() {
            let mut s = alloc::string::String::new();
            let mut j = 0;
            loop {
                let b = *arg_ptr.add(j);
                if b == 0 { break; }
                s.push(b as char);
                j += 1;
            }
            args_vec.push(s);
        }
    }
    unsafe { GLOBAL_ARGS = Some(args_vec); }

    // 4. Call main
    let code = main(argc, argv);
    
    os::exit(code as u64);
}

// Helper for Rust apps to get arguments as Vec<String>
pub fn args() -> alloc::vec::Vec<alloc::string::String>
{
    unsafe {
        if let Some(ref args) = *core::ptr::addr_of!(GLOBAL_ARGS) {
            args.clone()
        } else {
            alloc::vec::Vec::new()
        }
    }
}
