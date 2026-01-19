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

#[link(wasm_import_module = "wasi:cli/exit@0.2.0")]
unsafe extern "C" {
    #[link_name = "exit"]
    pub fn exit(status: i32) -> !;
}
