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
            let buf_ptr = arg2 as *mut u8;
            let buf_len = arg3 as usize;

            if fd == 0 {
                let handle = preview2_bindings::get_stdin();
                #[repr(C)]
                struct RetArea {
                    tag: u32,
                    ptr: u32,
                    len: u32,
                }
                let mut ret_area = RetArea { tag: 1, ptr: 0, len: 0 };

                preview2_bindings::input_stream_read(handle, buf_len as u64, &mut ret_area as *mut _ as *mut u8);

                if ret_area.tag == 0 {
                    let src_ptr = ret_area.ptr as *const u8;
                    let src_len = ret_area.len as usize;
                    let copy_len = if src_len < buf_len { src_len } else { buf_len };
                    if copy_len > 0 {
                        core::ptr::copy_nonoverlapping(src_ptr, buf_ptr, copy_len);
                        // Ideally free src_ptr here using cabi_realloc/free logic
                    }
                    copy_len as u64
                } else {
                    u64::MAX
                }
            } else {
                let mut nread: u32 = 0;
                let iov = Iov { ptr: arg2 as u32, len: arg3 as u32 };
                if wasi_fd_read(fd, &iov as *const _ as u32, 1, &nread as *const _ as u32) == 0 {
                    nread as u64
                } else {
                    u64::MAX
                }
            }
        }
        1 => { // WRITE: fd, buf_ptr, buf_len
            let fd = arg1 as i32;
            if fd == 1 || fd == 2 {
                let handle = if fd == 1 { preview2_bindings::get_stdout() } else { preview2_bindings::get_stderr() };
                #[repr(C)]
                struct ResultVal {
                    tag: u32,
                    _pad: u32,
                    _val: u32,
                }
                let mut ret_area = ResultVal { tag: 0, _pad: 0, _val: 0 };
                preview2_bindings::output_stream_blocking_write_and_flush(handle, arg2 as *const u8, arg3 as usize, &mut ret_area as *mut _ as *mut u8);
                if ret_area.tag == 0 { arg3 } else { u64::MAX }
            } else {
                let mut nwritten: u32 = 0;
                let iov = Iov { ptr: arg2 as u32, len: arg3 as u32 };
                if wasi_fd_write(fd, &iov as *const _ as u32, 1, &nwritten as *const _ as u32) == 0 {
                    nwritten as u64
                } else {
                    u64::MAX
                }
            }
        }
        3 => { // CLOSE: fd
            wasi_fd_close(arg1 as i32) as u64
        }
        8 => { // SEEK: fd, offset, whence
            let mut new_offset: u64 = 0;
            if wasi_fd_seek(arg1 as i32, arg2 as i64, arg3 as u8, &new_offset as *const _ as u32) == 0 {
                new_offset
            } else {
                u64::MAX
            }
        }
        60 => { // EXIT: code
            preview2_bindings::exit(arg1 as i32);
        }
        78 => { // READDIR: fd, buf_ptr, buf_len
            let mut used: u32 = 0;
            if wasi_fd_readdir(arg1 as i32, arg2 as u32, arg3 as u32, 0, &used as *const _ as u32) == 0 {
                used as u64
            } else {
                u64::MAX
            }
        }
        _ => {
            0
        }
    }
}

pub unsafe fn syscall4(_num: u64, _a1: u64, _a2: u64, _a3: u64, _a4: u64) -> u64 { 0 }
pub unsafe fn syscall5(_num: u64, _a1: u64, _a2: u64, _a3: u64, _a4: u64, _a5: u64) -> u64 { 0 }
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
