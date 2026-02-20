#[link(wasm_import_module = "wasi:cli/stdout@0.2.0")]
unsafe extern "C" {
    #[link_name = "get-stdout"]
    pub fn get_stdout() -> i32;
}

#[link(wasm_import_module = "wasi:cli/stdin@0.2.0")]
unsafe extern "C" {
    #[link_name = "get-stdin"]
    pub fn get_stdin() -> i32;
}

#[link(wasm_import_module = "wasi:cli/stderr@0.2.0")]
unsafe extern "C" {
    #[link_name = "get-stderr"]
    pub fn get_stderr() -> i32;
}

#[link(wasm_import_module = "wasi:io/streams@0.2.0")]
unsafe extern "C" {
    #[link_name = "[method]output-stream.blocking-write-and-flush"]
    pub fn output_stream_blocking_write_and_flush(handle: i32, ptr: *const u8, len: usize, result_ptr: *mut u8);

    #[link_name = "[method]input-stream.read"]
    pub fn input_stream_read(handle: i32, len: u64, result_ptr: *mut u8);
}

#[link(wasm_import_module = "wasi:io/error@0.2.0")]
unsafe extern "C" {
    #[link_name = "[resource-drop]error"]
    pub fn error_drop(handle: i32);
}

#[link(wasm_import_module = "krakeos:core/system@0.2.0")]
unsafe extern "C" {
    #[link_name = "syscall"]
    pub fn krakeos_syscall(num: u64, arg1: u64, arg2: u64, arg3: u64) -> u64;
    #[link_name = "syscall5"]
    pub fn krakeos_syscall5(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> u64;
    #[link_name = "syscall6"]
    pub fn krakeos_syscall6(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> u64;
    #[link_name = "syscall7"]
    pub fn krakeos_syscall7(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64, arg6: u64) -> u64;
}

#[link(wasm_import_module = "krakeos:graphics/screen@0.2.0")]
unsafe extern "C" {
    #[link_name = "get-width"]
    pub fn get_screen_width() -> u32;
    #[link_name = "get-height"]
    pub fn get_screen_height() -> u32;
}

#[link(wasm_import_module = "wasi:cli/exit@0.2.0")]
unsafe extern "C" {
    #[link_name = "exit"]
    pub fn exit(status: i32) -> !;
}

#[link(wasm_import_module = "krakeos:net/raw@0.2.0")]
unsafe extern "C" {
    #[link_name = "send"]
    pub fn krakeos_net_send(ptr: *const u8, len: u32) -> i32;
    #[link_name = "recv"]
    pub fn krakeos_net_recv(ptr: *mut u8, len: u32) -> i32;
}
