#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi:io/streams@0.2.0")]
unsafe extern "C" {
    #[link_name = "[method]output-stream.blocking-write-and-flush"]
    pub fn output_stream_blocking_write_and_flush(handle: i32, ptr: *const u8, len: usize, result_ptr: *mut u8);

    #[link_name = "[method]input-stream.read"]
    pub fn input_stream_read(handle: i32, len: u64, result_ptr: *mut u8);
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn output_stream_blocking_write_and_flush(handle: i32, ptr: *const u8, len: usize, result_ptr: *mut u8) {
    let res = crate::sys::syscall(1, handle as u64, ptr as u64, len as u64);
    if res != u64::MAX {
        core::ptr::write_unaligned(result_ptr as *mut u32, 0); // Ok tag
    } else {
        core::ptr::write_unaligned(result_ptr as *mut u32, 1); // Err tag
        core::ptr::write_unaligned(result_ptr.add(4) as *mut u32, 5); // EIO
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn input_stream_read(handle: i32, len: u64, result_ptr: *mut u8) {
    let buf = crate::memory::malloc(len as usize) as *mut u8;
    if buf.is_null() {
        core::ptr::write_unaligned(result_ptr as *mut u32, 1); // Err tag
        core::ptr::write_unaligned(result_ptr.add(4) as *mut u32, 12); // ENOMEM
        return;
    }

    let res = crate::sys::syscall(0, handle as u64, buf as u64, len);
    if res != u64::MAX {
        core::ptr::write_unaligned(result_ptr as *mut u32, 0); // Ok tag
        core::ptr::write_unaligned(result_ptr.add(8) as *mut u64, buf as u64);
        core::ptr::write_unaligned(result_ptr.add(16) as *mut u64, res);
    } else {
        crate::memory::free(buf as usize, len as usize);
        core::ptr::write_unaligned(result_ptr as *mut u32, 1); // Err tag
        core::ptr::write_unaligned(result_ptr.add(4) as *mut u32, 5); // EIO
    }
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi:io/error@0.2.0")]
unsafe extern "C" {
    #[link_name = "[resource-drop]error"]
    pub fn error_drop(handle: i32);
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn error_drop(_handle: i32) {
    // No-op for now on native
}
