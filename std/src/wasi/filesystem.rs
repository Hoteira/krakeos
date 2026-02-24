
#[cfg(target_arch = "wasm32")]
mod wasi_imports {
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

        // P2 directory reading
        #[link_name = "[method]descriptor.read-directory"]
        pub fn read_directory(handle: i32, result_ptr: *mut u8);

        #[link_name = "[method]directory-entry-stream.read-directory-entry"]
        pub fn read_directory_entry(stream: i32, result_ptr: *mut u8);

        #[link_name = "[resource-drop]directory-entry-stream"]
        pub fn drop_directory_entry_stream(stream: i32);
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasi_imports::*;

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn open_at(
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
pub unsafe fn create_directory_at(
    _handle: i32,
    path_ptr: *const u8,
    path_len: usize,
    result_ptr: *mut u8,
) {
    let res = crate::sys::syscall(83, path_ptr as u64, path_len as u64, 0);
    *result_ptr = if res == u64::MAX { 1 } else { 0 };
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn unlink_file_at(
    _handle: i32,
    path_ptr: *const u8,
    path_len: usize,
    result_ptr: *mut u8,
) {
    let res = crate::sys::syscall(87, path_ptr as u64, path_len as u64, 0);
    *result_ptr = if res == u64::MAX { 1 } else { 0 };
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn remove_directory_at(
    _handle: i32,
    path_ptr: *const u8,
    path_len: usize,
    result_ptr: *mut u8,
) {
    let res = crate::sys::syscall(84, path_ptr as u64, path_len as u64, 0);
    *result_ptr = if res == u64::MAX { 1 } else { 0 };
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn rename_at(
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

// Native implementation of P2 read-directory with stream buffering
#[cfg(not(target_arch = "wasm32"))]
struct DirStream {
    fd: i32,
    buffer: [u8; 1024],
    buf_size: usize,
    offset: usize,
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn read_directory(handle: i32, result_ptr: *mut u8) {
    use crate::rust_alloc::alloc::{alloc, Layout};
    let layout = Layout::new::<DirStream>();
    let ptr = alloc(layout) as *mut DirStream;
    if ptr.is_null() {
        *result_ptr = 1; // Err
        return;
    }
    (*ptr).fd = handle;
    (*ptr).buf_size = 0;
    (*ptr).offset = 0;

    *result_ptr = 0; // Ok
    core::ptr::write_unaligned(result_ptr.add(4) as *mut i32, ptr as i32);
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn read_directory_entry(stream: i32, result_ptr: *mut u8) {
    let state = &mut *(stream as *mut DirStream);

    loop {
        if state.offset < state.buf_size {
            // Check if we have enough bytes for the header
            if state.offset + 2 > state.buf_size {
                 state.offset = 0; state.buf_size = 0; continue;
            }

            let type_byte = state.buffer[state.offset];
            let name_len = state.buffer[state.offset+1] as usize;

            if state.offset + 2 + name_len > state.buf_size {
                state.offset = 0; state.buf_size = 0; continue;
            }

            let name_ptr = crate::memory::malloc(name_len);
            core::ptr::copy_nonoverlapping(
                state.buffer.as_ptr().add(state.offset + 2),
                name_ptr as *mut u8,
                name_len
            );

            state.offset += 2 + name_len;

            // Map type
            // KrakeOS: 1=File, 2=Dir, 3=Device
            // WASI: descriptor-type (regular-file=6, directory=3, character-device=2)
            let wasi_type: u8 = match type_byte {
                1 => 6,
                2 => 3,
                3 => 2,
                _ => 0,
            };

            *result_ptr = 0; // Ok
            core::ptr::write_unaligned(result_ptr.add(4) as *mut u32, 1); // Some

            // DirectoryEntry struct (aligned)
            // Offset 8: type
            core::ptr::write_unaligned(result_ptr.add(8) as *mut u8, wasi_type);
            // Offset 12: name ptr
            core::ptr::write_unaligned(result_ptr.add(12) as *mut u32, name_ptr as u32);
            // Offset 16: name len
            core::ptr::write_unaligned(result_ptr.add(16) as *mut u32, name_len as u32);

            return;
        }

        // Refill
        let res = crate::sys::syscall(78, state.fd as u64, state.buffer.as_mut_ptr() as u64, state.buffer.len() as u64);
        if res == u64::MAX || res == 0 {
            // EOF or Err -> None (treat error as end of stream for simple iterator)
            *result_ptr = 0; // Ok
            core::ptr::write_unaligned(result_ptr.add(4) as *mut u32, 0); // None
            return;
        }
        state.buf_size = res as usize;
        state.offset = 0;
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn drop_directory_entry_stream(stream: i32) {
    use crate::rust_alloc::alloc::{dealloc, Layout};
    let layout = Layout::new::<DirStream>();
    dealloc(stream as *mut u8, layout);
}

pub unsafe fn create_dir(path: &str) -> i32 {
    let mut result_buf = [0u8; 4];
    create_directory_at(3, path.as_ptr(), path.len(), result_buf.as_mut_ptr()); // 3 = CWD
    if result_buf[0] == 0 { 0 } else { -1 }
}

pub unsafe fn remove_file(path: &str) -> i32 {
    let mut result_buf = [0u8; 4];
    unlink_file_at(3, path.as_ptr(), path.len(), result_buf.as_mut_ptr());
    if result_buf[0] == 0 { 0 } else { -1 }
}

pub unsafe fn remove_dir(path: &str) -> i32 {
    let mut result_buf = [0u8; 4];
    remove_directory_at(3, path.as_ptr(), path.len(), result_buf.as_mut_ptr());
    if result_buf[0] == 0 { 0 } else { -1 }
}

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
