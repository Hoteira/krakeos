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

    // Initialize heap (10 MiB)
    let heap_start = crate::os::brk(0);
    let heap_size = 10 * 1024 * 1024;
    crate::os::brk(heap_start + heap_size);
    crate::alloc::init_heap(heap_start as *mut u8, heap_size);

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

    // If async_main is provided, use the executor
    #[cfg(feature = "userland")]
    {
        // We use a weak symbol or similar logic if possible, but for now 
        // let's just use block_on if we want to support async main.
        // Actually, let's keep it simple: if the app wants async, it calls block_on in main.
    }

    // Call standard main
    let result = main(argc, argv);
    crate::os::exit(result as u64);
}

#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rust_async_start(stack: *const usize) -> ! {
    let argc = *stack as i32;
    let argv = stack.add(1) as *const *const u8;

    // ... same argument and env parsing as rust_start ...
    let mut args = crate::rust_alloc::vec::Vec::new();
    for i in 0..argc {
        let ptr = *argv.add(i as usize);
        if !ptr.is_null() {
            let c_str = core::ffi::CStr::from_ptr(ptr as *const i8);
            args.push(crate::rust_alloc::string::String::from(c_str.to_string_lossy()));
        }
    }
    crate::env::init_args(&args);

    let mut executor = crate::executor::Executor::new();
    executor.run(); // This never returns in this simple implementation
    crate::os::exit(0);
}

unsafe extern "C" {
    fn main(argc: i32, argv: *const *const u8) -> i32;
    #[cfg(feature = "userland")]
    fn async_main() -> crate::rust_alloc::boxed::Box<dyn core::future::Future<Output=i32> + Send + 'static>;
}
