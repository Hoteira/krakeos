// wasi:io/streams@0.2.0 host functions

method_export!("wasi:io/streams@0.2.0", "[method]output-stream.blocking-write-and-flush",
    pub fn output_stream_blocking_write_and_flush(handle: i32, ptr: *const u8, len: usize, result_ptr: *mut u8) {
        // Extreme Tracing — raw syscall used here since it's the host side of the shim.
        // Avoid using debugln! here if it recurses back into this.
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
    pub fn input_stream_read(handle: i32, len: u64, result_ptr: *mut u8) {
        let buf = crate::memory::malloc(len as usize) as *mut u8;
        if buf.is_null() {
            core::ptr::write_unaligned(result_ptr as *mut u32, 1);
            core::ptr::write_unaligned(result_ptr.add(4) as *mut u32, 12);
            return;
        }

        let res = crate::sys::syscall(0, handle as u64, buf as u64, len);
        if res == u64::MAX - 1 {
            crate::memory::free(buf as usize, len as usize);
            core::ptr::write_unaligned(result_ptr as *mut u32, 0);
            core::ptr::write_unaligned(result_ptr.add(8) as *mut u64, 0);
            core::ptr::write_unaligned(result_ptr.add(16) as *mut u64, res);
        } else if res != u64::MAX {
            // Reallocate to match the actual number of bytes read.
            // If res is 0, this might return a non-null but dangling pointer, which is fine as len will be 0.
            let actual_buf = crate::memory::realloc(buf as usize, len as usize, res as usize, 8);
            
            core::ptr::write_unaligned(result_ptr as *mut u32, 0);
            core::ptr::write_unaligned(result_ptr.add(8) as *mut u64, actual_buf as u64);
            core::ptr::write_unaligned(result_ptr.add(16) as *mut u64, res);
        } else {
            crate::memory::free(buf as usize, len as usize);
            core::ptr::write_unaligned(result_ptr as *mut u32, 1);
            core::ptr::write_unaligned(result_ptr.add(4) as *mut u32, 5);
        }
    }
);

method_export!("wasi:io/poll@0.2.0", "poll",
    pub fn poll_poll(_in_ptr: *const u8, _in_len: u32, _ret_ptr: *mut u8) {
    }
);

method_export!("wasi:io/poll@0.2.0", "[method]pollable.block",
    pub fn poll_block(handle: i32) {
        crate::sys::syscall(35, handle as u64, 0, 0);
        crate::sys::yield_task();
    }
);

method_export!("wasi:io/poll@0.2.0", "[resource-drop]pollable",
    pub fn pollable_drop(_handle: i32) {
    }
);

method_export!("wasi:io/error@0.2.0", "[resource-drop]error",
    pub fn error_drop(_handle: i32) {
    }
);

method_export!("wasi:cli/stdout@0.2.0", "get-stdout",
    pub fn get_stdout() -> i32 {
        1
    }
);

method_export!("wasi:cli/stdin@0.2.0", "get-stdin",
    pub fn get_stdin() -> i32 {
        0
    }
);

method_export!("wasi:cli/stderr@0.2.0", "get-stderr",
    pub fn get_stderr() -> i32 {
        2
    }
);

method_export!("wasi:cli/exit@0.2.0", "exit",
    pub fn exit(status: i32) -> ! {
        unsafe {
            crate::sys::syscall1(60, status as u64);
        }
        loop {}
    }
);
