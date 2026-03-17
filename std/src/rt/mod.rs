#[cfg(any(feature = "userland", target_arch = "x86_64"))]
#[cfg(feature = "userland")]
pub mod runtime;

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi_snapshot_preview1")]
unsafe extern "C" {
    fn args_sizes_get(argc: *mut usize, argv_buf_size: *mut usize) -> i32;
    fn args_get(argv: *mut *mut u8, argv_buf: *mut u8) -> i32;
    fn environ_sizes_get(environ_count: *mut usize, environ_buf_size: *mut usize) -> i32;
    fn environ_get(environ: *mut *mut u8, environ_buf: *mut u8) -> i32;
}

#[lang = "start"]
fn lang_start<T: crate::process::Termination + 'static>(
    main: fn() -> T,
    _argc: isize,
    _argv: *const *const u8,
    _sigpipe: u8,
) -> isize {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let mut argc: usize = 0;
        let mut argv_buf_size: usize = 0;
        if args_sizes_get(&mut argc, &mut argv_buf_size) == 0 {
            let mut argv = crate::alloc::vec![core::ptr::null_mut::<u8>(); argc];
            let mut argv_buf = crate::alloc::vec![0u8; argv_buf_size];
            if args_get(argv.as_mut_ptr(), argv_buf.as_mut_ptr()) == 0 {
                let mut args = crate::alloc::vec::Vec::with_capacity(argc);
                for i in 0..argc {
                    let ptr = argv[i];
                    let s = core::ffi::CStr::from_ptr(ptr as *const i8).to_string_lossy();
                    args.push(crate::alloc::string::String::from(s));
                }
                crate::env::init_args(&args);
            }
        }

        let mut env_count: usize = 0;
        let mut env_buf_size: usize = 0;
        if environ_sizes_get(&mut env_count, &mut env_buf_size) == 0 {
            let mut env = crate::alloc::vec![core::ptr::null_mut::<u8>(); env_count];
            let mut env_buf = crate::alloc::vec![0u8; env_buf_size];
            if environ_get(env.as_mut_ptr(), env_buf.as_mut_ptr()) == 0 {
                let mut vars = crate::alloc::vec::Vec::with_capacity(env_count);
                for i in 0..env_count {
                    let ptr = env[i];
                    let s = core::ffi::CStr::from_ptr(ptr as *const i8).to_string_lossy();
                    if let Some((k, v)) = s.split_once('=') {
                        vars.push((crate::alloc::string::String::from(k), crate::alloc::string::String::from(v)));
                    }
                }
                crate::env::init_vars(&vars);
            }
        }
    }

    main().report() as isize
}
