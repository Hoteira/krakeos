use core::arch::naked_asm;

#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn _start() -> ! {
    naked_asm!(
        "xor rbp, rbp",      
        "mov rdi, rsp",      
        "and rsp, -16",      
        "call rust_start",
        "1: hlt",            
        "jmp 1b",
    )
}

#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rust_start(stack: *const usize) -> ! {
    let argc = *stack as i32;
    let argv = stack.add(1) as *const *const u8;
    let envp = stack.add(argc as usize + 2) as *const *const u8;

    // Parse arguments for std::env
    let mut args = crate::rust_alloc::vec::Vec::new();
    for i in 0..argc {
        let ptr = *argv.add(i as usize);
        if !ptr.is_null() {
            let c_str = core::ffi::CStr::from_ptr(ptr as *const i8);
            args.push(crate::rust_alloc::string::String::from(c_str.to_string_lossy()));
        }
    }
    crate::env::init_args(&args);

    // Parse environment for std::env
    let mut vars = crate::rust_alloc::vec::Vec::new();
    let mut env_ptr = envp;
    while !(*env_ptr).is_null() {
        let ptr = *env_ptr;
        let c_str = core::ffi::CStr::from_ptr(ptr as *const i8);
        let s = crate::rust_alloc::string::String::from(c_str.to_string_lossy());
        if let Some((k, v)) = s.split_once('=') {
            vars.push((crate::rust_alloc::string::String::from(k), crate::rust_alloc::string::String::from(v)));
        }
        env_ptr = env_ptr.add(1);
    }
    crate::env::init_vars(&vars);

    let result = main(argc, argv);
    crate::os::exit(result as u64);
}

#[cfg(all(target_arch = "wasm32", not(target_os = "wasi")))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start() {
    main(0, core::ptr::null());
    crate::os::exit(0);
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn __wasm_call_dtors() {}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cabi_realloc(ptr: *mut u8, old_size: usize, align: usize, new_size: usize) -> *mut u8 {
    use core::alloc::{GlobalAlloc, Layout};
    if ptr.is_null() {
        if new_size == 0 { return align as *mut u8; }
        let layout = Layout::from_size_align(new_size, align).unwrap();
        let res = crate::alloc::ALLOCATOR.alloc(layout);
        if res.is_null() {
            crate::debugln!("cabi_realloc: ALLOC FAILED (size: {}, align: {})", new_size, align);
        }
        res
    } else {
        let layout = Layout::from_size_align(old_size, align).unwrap();
        if new_size == 0 {
            crate::alloc::ALLOCATOR.dealloc(ptr, layout);
            return core::ptr::null_mut();
        }
        let res = crate::alloc::ALLOCATOR.realloc(ptr, layout, new_size);
        if res.is_null() {
            crate::debugln!("cabi_realloc: REALLOC FAILED (old: {}, new: {}, align: {})", old_size, new_size, align);
        }
        res
    }
}

unsafe extern "C" {
    fn main(argc: i32, argv: *const *const u8) -> i32;
}
