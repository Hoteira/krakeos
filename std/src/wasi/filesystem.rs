#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi:filesystem/types@0.2.0")]
unsafe extern "C" {
    #[link_name = "[method]descriptor.open-at"]
    pub fn open_at(
        dir_handle: i32,
        flags: u32,
        path_ptr: *const u8,
        path_len: usize,
        oflags: u32,
        flags_val: u32,
        result_ptr: *mut u8,
    );
    #[link_name = "[method]descriptor.stat"]
    pub fn stat(handle: i32, result_ptr: *mut u8);
    #[link_name = "[method]descriptor.set-size"]
    pub fn set_size(handle: i32, size: u64, result_ptr: *mut u8);
    #[link_name = "[method]descriptor.seek"]
    pub fn seek(handle: i32, offset: u64, whence: i32, result_ptr: *mut u8);
    #[link_name = "[resource-drop]descriptor"]
    pub fn descriptor_drop(handle: i32);
    #[link_name = "[method]descriptor.create-directory-at"]
    pub fn create_directory_at(
        handle: i32,
        path_ptr: *const u8,
        path_len: usize,
        result_ptr: *mut u8,
    );
    #[link_name = "[method]descriptor.unlink-file-at"]
    pub fn unlink_file_at(handle: i32, path_ptr: *const u8, path_len: usize, result_ptr: *mut u8);
    #[link_name = "[method]descriptor.remove-directory-at"]
    pub fn remove_directory_at(
        handle: i32,
        path_ptr: *const u8,
        path_len: usize,
        result_ptr: *mut u8,
    );
    #[link_name = "[method]descriptor.rename-at"]
    pub fn rename_at(
        handle: i32,
        old_path_ptr: *const u8,
        old_path_len: usize,
        new_handle: i32,
        new_path_ptr: *const u8,
        new_path_len: usize,
        result_ptr: *mut u8,
    );
}

// readdir support - uses WASI preview1 fd_readdir on wasm32, KrakeOS syscall 78 on native
#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi_snapshot_preview1")]
unsafe extern "C" {
    pub fn fd_readdir(
        fd: i32,
        buf_ptr: *mut u8,
        buf_len: u32,
        cookie: u64,
        bufused_ptr: *mut u32,
    ) -> i32;
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn open_at(
    dir_handle: i32,
    _flags: u32,
    path_ptr: *const u8,
    path_len: usize,
    oflags: u32,
    _flags_val: u32,
    result_ptr: *mut u8,
) {
    let syscall_num = if (oflags & 0x1) != 0 { 85 } else { 2 };
    let res = crate::sys::syscall(syscall_num, path_ptr as u64, path_len as u64, 0);
    if res == u64::MAX {
        core::ptr::write_unaligned(result_ptr as *mut u32, 1); // Err
    } else {
        core::ptr::write_unaligned(result_ptr as *mut u32, 0); // Ok
        core::ptr::write_unaligned(result_ptr.add(4) as *mut i32, res as i32);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn descriptor_drop(handle: i32) {
    crate::sys::syscall(3, handle as u64, 0, 0);
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn stat(handle: i32, result_ptr: *mut u8) {
    let mut s = unsafe { core::mem::zeroed::<crate::fs::Stat>() };
    let res = crate::sys::syscall(5, handle as u64, 0, &mut s as *mut _ as u64);
    if res == u64::MAX {
        *result_ptr = 1; // Err
    } else {
        *result_ptr = 0; // Ok
        core::ptr::copy_nonoverlapping(
            &s as *const _ as *const u8,
            result_ptr.add(8),
            core::mem::size_of::<crate::fs::Stat>(),
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn set_size(handle: i32, size: u64, result_ptr: *mut u8) {
    let res = crate::sys::syscall(77, handle as u64, size, 0);
    if res == u64::MAX {
        *result_ptr = 1;
    } else {
        *result_ptr = 0;
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn seek(handle: i32, offset: u64, whence: i32, result_ptr: *mut u8) {
    let res = crate::sys::syscall(8, handle as u64, offset, whence as u64);
    if res == u64::MAX {
        *result_ptr = 1;
    } else {
        *result_ptr = 0;
        core::ptr::write_unaligned(result_ptr.add(8) as *mut u64, res);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn create_dir(path: &str) -> i32 {
    crate::sys::syscall(83, path.as_ptr() as u64, path.len() as u64, 0) as i32
}

#[cfg(target_arch = "wasm32")]
pub unsafe fn create_dir(path: &str) -> i32 {
    let mut result_buf = [0u8; 4]; // Just error code
    create_directory_at(3, path.as_ptr(), path.len(), result_buf.as_mut_ptr()); // 3 = CWD
    if result_buf[0] == 0 { 0 } else { -1 }
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn remove_file(path: &str) -> i32 {
    crate::sys::syscall(87, path.as_ptr() as u64, path.len() as u64, 0) as i32
}

#[cfg(target_arch = "wasm32")]
pub unsafe fn remove_file(path: &str) -> i32 {
    let mut result_buf = [0u8; 4];
    unlink_file_at(3, path.as_ptr(), path.len(), result_buf.as_mut_ptr());
    if result_buf[0] == 0 { 0 } else { -1 }
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn remove_dir(path: &str) -> i32 {
    crate::sys::syscall(84, path.as_ptr() as u64, path.len() as u64, 0) as i32
}

#[cfg(target_arch = "wasm32")]
pub unsafe fn remove_dir(path: &str) -> i32 {
    let mut result_buf = [0u8; 4];
    remove_directory_at(3, path.as_ptr(), path.len(), result_buf.as_mut_ptr());
    if result_buf[0] == 0 { 0 } else { -1 }
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn rename(from: &str, to: &str) -> i32 {
    crate::sys::syscall4(
        82,
        from.as_ptr() as u64,
        from.len() as u64,
        to.as_ptr() as u64,
        to.len() as u64,
    ) as i32
}

#[cfg(target_arch = "wasm32")]
pub unsafe fn rename(from: &str, to: &str) -> i32 {
    let mut result_buf = [0u8; 4];
    rename_at(
        3,
        from.as_ptr(),
        from.len(),
        3,
        to.as_ptr(),
        to.len(),
        result_buf.as_mut_ptr(),
    );
    if result_buf[0] == 0 { 0 } else { -1 }
}

// readdir for native - uses KrakeOS kernel syscall 78
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn readdir(fd: i32, buf: &mut [u8]) -> u64 {
    crate::sys::syscall(78, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64)
}
