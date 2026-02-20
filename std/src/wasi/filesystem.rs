#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi:filesystem/types@0.2.0")]
unsafe extern "C" {
    #[link_name = "[method]descriptor.open-at"]
    pub fn open_at(dir_handle: i32, flags: u32, path_ptr: *const u8, path_len: usize, oflags: u32, flags_val: u32, result_ptr: *mut u8);
    #[link_name = "[method]descriptor.stat"]
    pub fn stat(handle: i32, result_ptr: *mut u8);
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn open_at(dir_handle: i32, _flags: u32, path_ptr: *const u8, path_len: usize, oflags: u32, _flags_val: u32, result_ptr: *mut u8) {
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
pub unsafe fn stat(handle: i32, result_ptr: *mut u8) {
    let mut s = unsafe { core::mem::zeroed::<crate::fs::Stat>() };
    let res = crate::sys::syscall(5, handle as u64, 0, &mut s as *mut _ as u64);
    if res == u64::MAX {
        *result_ptr = 1; // Err
    } else {
        *result_ptr = 0; // Ok
        // Translate KrakeOS Stat to WASI DescriptorStat if needed, but for now just copy raw if possible?
        // Actually, let's just use the result_ptr to return success/failure and the Stat struct.
        core::ptr::copy_nonoverlapping(&s as *const _ as *const u8, result_ptr.add(8), core::mem::size_of::<crate::fs::Stat>());
    }
}

pub unsafe fn create_dir(path: &str) -> i32 {
    crate::sys::syscall(83, path.as_ptr() as u64, path.len() as u64, 0) as i32
}

pub unsafe fn remove_file(path: &str) -> i32 {
    crate::sys::syscall(87, path.as_ptr() as u64, path.len() as u64, 0) as i32
}

pub unsafe fn remove_dir(path: &str) -> i32 {
    crate::sys::syscall(84, path.as_ptr() as u64, path.len() as u64, 0) as i32
}

pub unsafe fn rename(from: &str, to: &str) -> i32 {
    crate::sys::syscall4(82, from.as_ptr() as u64, from.len() as u64, to.as_ptr() as u64, to.len() as u64) as i32
}
