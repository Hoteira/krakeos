use core::arch::naked_asm;

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rust_start(stack: *const usize) -> ! {
    let argc = *stack as i32;
    let argv = stack.add(1) as *const *const u8;
    let envp = stack.add(argc as usize + 2) as *const *const u8;

    let heap_start = crate::os::brk(0);
    let heap_size = 10 * 1024 * 1024;
    crate::os::brk(heap_start + heap_size);
    crate::allocator::init_heap(heap_start as *mut u8, heap_size);

    let mut args = crate::alloc::vec::Vec::new();
    for i in 0..argc {
        let ptr = *argv.add(i as usize);
        if !ptr.is_null() {
            let c_str = core::ffi::CStr::from_ptr(ptr as *const i8);
            args.push(crate::alloc::string::String::from(c_str.to_string_lossy()));
        }
    }
    crate::env::init_args(&args);

    let mut vars = crate::alloc::vec::Vec::new();
    let mut env_ptr = envp;
    while !(*env_ptr).is_null() {
        let ptr = *env_ptr;
        let c_str = core::ffi::CStr::from_ptr(ptr as *const i8);
        let s = crate::alloc::string::String::from(c_str.to_string_lossy());
        if let Some((k, v)) = s.split_once('=') {
            vars.push((crate::alloc::string::String::from(k), crate::alloc::string::String::from(v)));
        }
        env_ptr = env_ptr.add(1);
    }
    crate::env::init_vars(&vars);

    let result = main(argc, argv);
    crate::os::exit(result as u64);
}

unsafe extern "C" {
    fn main(argc: i32, argv: *const *const u8) -> i32;
}
