pub mod preview2_bindings;

#[link(wasm_import_module = "wasi_snapshot_preview1")]
unsafe extern "C" {
    #[link_name = "fd_read"]
    fn wasi_fd_read(fd: i32, iovs_ptr: u32, iovs_len: u32, nread_ptr: u32) -> i32;

    #[link_name = "fd_write"]
    fn wasi_fd_write(fd: i32, iovs_ptr: u32, iovs_len: u32, nwritten_ptr: u32) -> i32;

    #[link_name = "fd_seek"]
    fn wasi_fd_seek(fd: i32, offset: i64, whence: u8, newoffset_ptr: u32) -> i32;

    #[link_name = "fd_close"]
    fn wasi_fd_close(fd: i32) -> i32;

    #[link_name = "proc_exit"]
    fn wasi_proc_exit(code: i32) -> !;

    #[link_name = "sched_yield"]
    fn wasi_sched_yield() -> i32;

    #[link_name = "fd_readdir"]
    fn wasi_fd_readdir(fd: i32, buf_ptr: u32, buf_len: u32, cookie: u64, used_ptr: u32) -> i32;
}

#[unsafe(no_mangle)]
pub extern "C" fn __wasi_proc_exit(code: i32) -> ! {
    unsafe {
        preview2_bindings::exit(code);
    }
}

#[repr(C)]
struct Iov {
    ptr: u32,
    len: u32,
}

pub unsafe fn syscall(num: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    match num {
        0 => { // READ: fd, buf_ptr, buf_len
            let fd = arg1 as i32;
            if fd == 0 {
                let handle = preview2_bindings::get_stdin();
                #[repr(C)]
                struct RetArea { tag: u32, ptr: u32, len: u32 }
                let mut ret_area = RetArea { tag: 1, ptr: 0, len: 0 };
                // unsafe { crate::os::debug_print("Guest: calling input_stream_read\n"); }
                preview2_bindings::input_stream_read(handle, arg3, &mut ret_area as *mut _ as *mut u8);
                // unsafe { 
                //     let msg = crate::rust_alloc::format!("Guest: read done. tag={} ptr={} len={}\n", ret_area.tag, ret_area.ptr, ret_area.len);
                //     crate::os::debug_print(&msg);
                // }
                if ret_area.tag == 0 {
                    let src_len = ret_area.len as usize;
                    let copy_len = if src_len < arg3 as usize { src_len } else { arg3 as usize };
                    if copy_len > 0 {
                        core::ptr::copy_nonoverlapping(ret_area.ptr as *const u8, arg2 as *mut u8, copy_len);
                    }
                    if copy_len == 0 && arg3 > 0 {
                        return u64::MAX - 1; // EWOULDBLOCK
                    }
                    copy_len as u64
                } else {
                    u64::MAX
                }
            } else {
                unsafe { preview2_bindings::krakeos_syscall(num, arg1, arg2, arg3) }
            }
        }
        1 => { // WRITE: fd, buf_ptr, buf_len
            let fd = arg1 as i32;
            if fd == 1 || fd == 2 {
                let handle = if fd == 1 { preview2_bindings::get_stdout() } else { preview2_bindings::get_stderr() };
                #[repr(C)]
                struct ResultVal { tag: u32, _pad: u32, _val: u32 }
                let mut ret_area = ResultVal { tag: 0, _pad: 0, _val: 0 };
                preview2_bindings::output_stream_blocking_write_and_flush(handle, arg2 as *const u8, arg3 as usize, &mut ret_area as *mut _ as *mut u8);
                if ret_area.tag == 0 { arg3 } else { u64::MAX }
            } else {
                unsafe { preview2_bindings::krakeos_syscall(num, arg1, arg2, arg3) }
            }
        }
        106 => { // GET_SCREEN_WIDTH
            preview2_bindings::get_screen_width() as u64
        }
        107 => { // GET_SCREEN_HEIGHT
            preview2_bindings::get_screen_height() as u64
        }
        60 => { // EXIT
            preview2_bindings::exit(arg1 as i32);
        }
        _ => {
            unsafe { preview2_bindings::krakeos_syscall(num, arg1, arg2, arg3) }
        }
    }
}

pub unsafe fn syscall4(num: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> u64 {
    unsafe { preview2_bindings::krakeos_syscall5(num, a1, a2, a3, a4) }
}
pub unsafe fn syscall5(num: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> u64 {
    unsafe { preview2_bindings::krakeos_syscall6(num, a1, a2, a3, a4, a5) }
}
pub unsafe fn syscall6(_num: u64, _a1: u64, _a2: u64, _a3: u64, _a4: u64, _a5: u64, _a6: u64) -> u64 { 0 }

pub fn yield_task() {
    unsafe { let _ = wasi_sched_yield(); }
}

pub fn hlt_loop() -> ! {
    loop { yield_task(); }
}

pub unsafe fn alloc_pages(size: usize) -> *mut u8 {
    let pages = (size + 65535) / 65536;
    let prev = core::arch::wasm32::memory_grow(0, pages);
    if prev == usize::MAX { core::ptr::null_mut() } else { (prev * 65536) as *mut u8 }
}
