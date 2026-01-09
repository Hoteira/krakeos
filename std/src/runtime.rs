use crate::println;
use core::arch::naked_asm;

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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rust_start(stack: *const usize) -> ! {
    let heap_size = 10 * 1024 * 1024; 
    let heap_ptr = crate::memory::malloc(heap_size);
    if heap_ptr == 0 || heap_ptr == usize::MAX {
        crate::os::exit(1);
    }
    crate::alloc::init_heap(heap_ptr as *mut u8, heap_size);

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

    // Call the application's entry point.
    // If the user has "fn main()", rustc generates a "main" symbol that calls "std::rt::lang_start".
    // That "main" typically has the signature (argc, argv).
    let result = main(argc, argv);
    crate::os::exit(result as u64);
}

unsafe extern "C" {
    fn main(argc: i32, argv: *const *const u8) -> i32;
}
