#![no_std]
#![no_main]

extern crate alloc;
use std::println;

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        println!("Usage: wasm_loader <wasm_file> [args...]");
        return 1;
    }

    let wasm_path_ptr = unsafe { *argv.add(1) };
    let wasm_path = unsafe { core::ffi::CStr::from_ptr(wasm_path_ptr as *const i8) }.to_string_lossy();

    println!("[WASM Loader] Running {}...", wasm_path);

    // Pass the remaining arguments to the WASM app
    let mut args = alloc::vec::Vec::new();
    for i in 1..argc {
        let arg_ptr = unsafe { *argv.add(i as usize) };
        let s = unsafe { core::ffi::CStr::from_ptr(arg_ptr as *const i8) }.to_string_lossy().into_owned();
        args.push(s);
    }

    // std::wasm::run_with_args will handle the execution
    // Root path is "/" for now. FDs 0, 1, 2 are inherited.
    // AOT is true for performance.
    std::wasm::run_with_args(&wasm_path, args, "/", &[(0, 0), (1, 1), (2, 2)], true)
}
