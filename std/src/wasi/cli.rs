#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi:cli/stdout@0.2.0")]
unsafe extern "C" {
    #[link_name = "get-stdout"]
    pub fn get_stdout() -> i32;
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn get_stdout() -> i32 {
    1 // FD 1
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi:cli/stdin@0.2.0")]
unsafe extern "C" {
    #[link_name = "get-stdin"]
    pub fn get_stdin() -> i32;
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn get_stdin() -> i32 {
    0 // FD 0
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi:cli/stderr@0.2.0")]
unsafe extern "C" {
    #[link_name = "get-stderr"]
    pub fn get_stderr() -> i32;
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn get_stderr() -> i32 {
    2 // FD 2
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi:cli/exit@0.2.0")]
unsafe extern "C" {
    #[link_name = "exit"]
    pub fn exit(status: i32) -> !;
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn exit(status: i32) -> ! {
    crate::sys::syscall1(60, status as u64);
    loop {}
}