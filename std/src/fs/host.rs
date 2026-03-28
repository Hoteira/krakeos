// Filesystem bindings — all via method_export!

method_export!("wasi:filesystem/types@0.2.0", "[method]descriptor.open-at",
    pub fn open_at(
        _dir_handle: i32,
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
            // Write handle at offset 4, matching File::open in mod.rs
            core::ptr::write_unaligned(result_ptr.add(4) as *mut i32, res as i32);
        }
    }
);

method_export!("wasi:filesystem/types@0.2.0", "[method]descriptor.stat",
    pub fn stat(handle: i32, result_ptr: *mut u8) {
        let mut s = core::mem::zeroed::<crate::fs::Stat>();
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
);

method_export!("wasi:filesystem/types@0.2.0", "[method]descriptor.set-size",
    pub fn set_size(handle: i32, size: u64, result_ptr: *mut u8) {
        let res = crate::sys::syscall(77, handle as u64, size, 0);
        if res == u64::MAX {
            *result_ptr = 1;
        } else {
            *result_ptr = 0;
        }
    }
);

method_export!("wasi:filesystem/types@0.2.0", "[method]descriptor.seek",
    pub fn seek(handle: i32, offset: u64, whence: i32, result_ptr: *mut u8) {
        let res = crate::sys::syscall(8, handle as u64, offset, whence as u64);
        if res == u64::MAX {
            *result_ptr = 1;
        } else {
            *result_ptr = 0;
            core::ptr::write_unaligned(result_ptr.add(8) as *mut u64, res);
        }
    }
);

method_export!("wasi:filesystem/types@0.2.0", "[resource-drop]descriptor",
    pub fn descriptor_drop(handle: i32) {
        crate::sys::syscall(3, handle as u64, 0, 0);
    }
);

method_export!("wasi:filesystem/types@0.2.0", "[method]descriptor.create-directory-at",
    pub fn create_directory_at(
        _handle: i32,
        path_ptr: *const u8,
        path_len: usize,
        result_ptr: *mut u8,
    ) {
        let res = crate::sys::syscall(83, path_ptr as u64, path_len as u64, 0);
        *result_ptr = if res == u64::MAX { 1 } else { 0 };
    }
);

method_export!("wasi:filesystem/types@0.2.0", "[method]descriptor.unlink-file-at",
    pub fn unlink_file_at(
        _handle: i32,
        path_ptr: *const u8,
        path_len: usize,
        result_ptr: *mut u8,
    ) {
        let res = crate::sys::syscall(87, path_ptr as u64, path_len as u64, 0);
        *result_ptr = if res == u64::MAX { 1 } else { 0 };
    }
);

method_export!("wasi:filesystem/types@0.2.0", "[method]descriptor.remove-directory-at",
    pub fn remove_directory_at(
        _handle: i32,
        path_ptr: *const u8,
        path_len: usize,
        result_ptr: *mut u8,
    ) {
        let res = crate::sys::syscall(84, path_ptr as u64, path_len as u64, 0);
        *result_ptr = if res == u64::MAX { 1 } else { 0 };
    }
);

method_export!("wasi:filesystem/types@0.2.0", "[method]descriptor.rename-at",
    pub fn rename_at(
        _handle: i32,
        old_path_ptr: *const u8,
        old_path_len: usize,
        _new_handle: i32,
        new_path_ptr: *const u8,
        new_path_len: usize,
        result_ptr: *mut u8,
    ) {
        let res = crate::sys::syscall4(
            82,
            old_path_ptr as u64,
            old_path_len as u64,
            new_path_ptr as u64,
            new_path_len as u64,
        );
        *result_ptr = if res == u64::MAX { 1 } else { 0 };
    }
);

// DirStream used by read_directory native impl
#[cfg(not(target_arch = "wasm32"))]
struct DirStream {
    fd: i32,
    buffer: [u8; 1024],
    buf_size: usize,
    offset: usize,
}

#[cfg(not(target_arch = "wasm32"))]
static mut DIR_STREAM_TABLE: [Option<*mut DirStream>; 128] = [None; 128];

#[cfg(not(target_arch = "wasm32"))]
fn alloc_stream(s: *mut DirStream) -> i32 {
    unsafe {
        for i in 0..128 {
            if DIR_STREAM_TABLE[i].is_none() {
                DIR_STREAM_TABLE[i] = Some(s);
                return i as i32;
            }
        }
        -1
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn get_stream(handle: i32) -> *mut DirStream {
    unsafe {
        if handle >= 0 && (handle as usize) < 128 {
            DIR_STREAM_TABLE[handle as usize].unwrap_or(core::ptr::null_mut())
        } else {
            core::ptr::null_mut()
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn free_stream(handle: i32) -> *mut DirStream {
    unsafe {
        if handle >= 0 && (handle as usize) < 128 {
            let s = DIR_STREAM_TABLE[handle as usize];
            DIR_STREAM_TABLE[handle as usize] = None;
            s.unwrap_or(core::ptr::null_mut())
        } else {
            core::ptr::null_mut()
        }
    }
}

method_export!("wasi:filesystem/types@0.2.0", "[method]descriptor.read-directory",
    pub fn read_directory(handle: i32, result_ptr: *mut u8) {
        use crate::alloc::alloc::{alloc, Layout};
        let layout = Layout::new::<DirStream>();
        let ptr = unsafe { alloc(layout) as *mut DirStream };
        if ptr.is_null() {
            *result_ptr = 1; // Err
            return;
        }
        unsafe {
            (*ptr).fd = handle;
            (*ptr).buf_size = 0;
            (*ptr).offset = 0;
        }

        let handle = alloc_stream(ptr);
        if handle == -1 {
            unsafe { crate::alloc::alloc::dealloc(ptr as *mut u8, layout) };
            *result_ptr = 1;
            return;
        }

        *result_ptr = 0; // Ok
        core::ptr::write_unaligned(result_ptr.add(4) as *mut i32, handle);
    }
);

method_export!("wasi:filesystem/types@0.2.0", "[method]directory-entry-stream.read-directory-entry",
    pub fn read_directory_entry(stream: i32, result_ptr: *mut u8) {
        let state_ptr = get_stream(stream);
        if state_ptr.is_null() {
            *result_ptr = 1;
            return;
        }
        let state = unsafe { &mut *state_ptr };

        loop {
            if state.offset < state.buf_size {
                if state.offset + 2 > state.buf_size {
                     state.offset = 0; state.buf_size = 0; continue;
                }

                let type_byte = state.buffer[state.offset];
                let name_len = state.buffer[state.offset+1] as usize;

                if state.offset + 2 + name_len > state.buf_size {
                    state.offset = 0; state.buf_size = 0; continue;
                }

                let name_ptr = crate::memory::malloc(name_len);
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        state.buffer.as_ptr().add(state.offset + 2),
                        name_ptr as *mut u8,
                        name_len
                    );
                }

                state.offset += 2 + name_len;

                // Map type: KrakeOS: 1=File, 2=Dir, 3=Device
                // WASI: descriptor-type (regular-file=6, directory=3, character-device=2)
                let wasi_type: u8 = match type_byte {
                    1 => 6,
                    2 => 3,
                    3 => 2,
                    _ => 0,
                };

                // WASI Result layout (WASM32 aligned):
                // [0..4]   = 0 (Result::Ok)
                // [4..8]   = 1 (Option::Some)
                // [8..9]   = type (u8)
                // [9..12]  = padding
                // [12..16] = name_ptr (u32)
                // [16..20] = name_len (u32)
                // [24..32] = inode (u64) - if we had one

                *result_ptr = 0; // Ok
                core::ptr::write_unaligned(result_ptr.add(4) as *mut u32, 1); // Some
                core::ptr::write_unaligned(result_ptr.add(8) as *mut u8, wasi_type);
                
                core::ptr::write_unaligned(result_ptr.add(12) as *mut u32, name_ptr as u32);
                core::ptr::write_unaligned(result_ptr.add(16) as *mut u32, name_len as u32);
                core::ptr::write_unaligned(result_ptr.add(24) as *mut u64, 0); // inode stub

                return;
            }

            // Refill
            let res = crate::sys::syscall(78, state.fd as u64, state.buffer.as_mut_ptr() as u64, state.buffer.len() as u64);
            if res == u64::MAX || res == 0 {
                *result_ptr = 0; // Ok
                core::ptr::write_unaligned(result_ptr.add(4) as *mut u32, 0); // None
                return;
            }
            state.buf_size = res as usize;
            state.offset = 0;
        }
    }
);

method_export!("wasi:filesystem/types@0.2.0", "[resource-drop]directory-entry-stream",
    pub fn drop_directory_entry_stream(stream: i32) {
        let ptr = free_stream(stream);
        if !ptr.is_null() {
            unsafe {
                crate::sys::syscall(3, (*ptr).fd as u64, 0, 0);
            }
            use crate::alloc::alloc::{dealloc, Layout};
            let layout = Layout::new::<DirStream>();
            unsafe { dealloc(ptr as *mut u8, layout) };
        }
    }
);

method_export!("krakeos:system/filesystem@0.2.0", "mount",
    pub fn mount_host(disk_id: u64, fs_type_ptr: *const u8, fs_type_len: usize) -> u64 {
        crate::sys::syscall(165, disk_id, fs_type_ptr as u64, fs_type_len as u64)
    }
);

// --- Helper functions that call the above bindings ---

pub fn create_dir(path: &str) -> i32 {
    let mut result_buf = [0u8; 4];
    create_directory_at(3, path.as_ptr(), path.len(), result_buf.as_mut_ptr());
    if result_buf[0] == 0 { 0 } else { -1 }
}

pub fn remove_file(path: &str) -> i32 {
    let mut result_buf = [0u8; 4];
    unlink_file_at(3, path.as_ptr(), path.len(), result_buf.as_mut_ptr());
    if result_buf[0] == 0 { 0 } else { -1 }
}

pub fn remove_dir(path: &str) -> i32 {
    let mut result_buf = [0u8; 4];
    remove_directory_at(3, path.as_ptr(), path.len(), result_buf.as_mut_ptr());
    if result_buf[0] == 0 { 0 } else { -1 }
}

pub fn rename(from: &str, to: &str) -> i32 {
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
