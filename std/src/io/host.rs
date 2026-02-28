// I/O streams, poll, CLI bindings — all via method_export!

method_export!("wasi:io/streams@0.2.0", "[method]output-stream.blocking-write-and-flush",
    pub unsafe fn output_stream_blocking_write_and_flush(handle: i32, ptr: *const u8, len: usize, result_ptr: *mut u8) {
        let res = crate::sys::syscall(1, handle as u64, ptr as u64, len as u64);
        if res != u64::MAX {
            core::ptr::write_unaligned(result_ptr as *mut u32, 0); // Ok tag
        } else {
            core::ptr::write_unaligned(result_ptr as *mut u32, 1); // Err tag
            core::ptr::write_unaligned(result_ptr.add(4) as *mut u32, 5); // EIO
        }
    }
);

method_export!("wasi:io/streams@0.2.0", "[method]input-stream.read",
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
);

method_export!("wasi:io/poll@0.2.0", "poll",
    pub unsafe fn poll_poll(_in_ptr: *const u8, _in_len: u32, _ret_ptr: *mut u8) {
        // Native poll implementation — no-op (subscriptions handled by kernel)
    }
);

method_export!("wasi:io/poll@0.2.0", "[method]pollable.block",
    pub unsafe fn poll_block(handle: i32) {
        // Shim: handle is sleep duration in ms (from clocks subscribe-duration)
        crate::sys::syscall(35, handle as u64, 0, 0);
        crate::sys::yield_task();
    }
);

method_export!("wasi:io/poll@0.2.0", "[resource-drop]pollable",
    pub unsafe fn pollable_drop(_handle: i32) {
        // No-op for shim
    }
);

method_export!("wasi:io/error@0.2.0", "[resource-drop]error",
    pub unsafe fn error_drop(_handle: i32) {
        // No-op for now on native
    }
);

method_export!("wasi:cli/stdout@0.2.0", "get-stdout",
    pub unsafe fn get_stdout() -> i32 {
        1 // FD 1
    }
);

method_export!("wasi:cli/stdin@0.2.0", "get-stdin",
    pub unsafe fn get_stdin() -> i32 {
        0 // FD 0
    }
);

method_export!("wasi:cli/stderr@0.2.0", "get-stderr",
    pub unsafe fn get_stderr() -> i32 {
        2 // FD 2
    }
);

// cli exit has `-> !` which needs special handling
#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi:cli/exit@0.2.0")]
unsafe extern "C" {
    #[link_name = "exit"]
    pub fn exit(status: i32) -> !;
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn exit(status: i32) -> ! {
    crate::sys::syscall1(60, status as u64);
    loop {}
}
