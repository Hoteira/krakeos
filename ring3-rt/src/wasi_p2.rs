use crate::context::Ring3Context;
use crate::syscall::{syscall1, syscall2, syscall3};

const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_CLOSE: u64 = 3;
const SYS_FSTAT: u64 = 5;
const SYS_OPEN: u64 = 2;
const SYS_NANOSLEEP: u64 = 35;
const SYS_EXIT: u64 = 60;
const SYS_FTRUNCATE: u64 = 77;
const SYS_MKDIR: u64 = 83;
const SYS_RMDIR: u64 = 84;
const SYS_UNLINK: u64 = 87;
const SYS_LSEEK: u64 = 8;
const SYS_GET_TICKS: u64 = 109;
const SYS_YIELD: u64 = 129;
const SYS_RANDOM_GET: u64 = 208;
const SYS_RENAME: u64 = 82;
const SYS_GETDENTS: u64 = 78;

// =============================================================================
// wasi:cli
// =============================================================================

/// exit(status: i32) -> !
#[no_mangle]
pub extern "C" fn wasi_p2_exit(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let status = (*sp.add(0)) as u64;
        syscall1(SYS_EXIT, status);
    }
    loop {}
}

/// get-stdout() -> i32
#[no_mangle]
pub extern "C" fn wasi_p2_get_stdout(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_sp = sp.sub(1);
        *result_sp = 1u128; // stdout fd = 1
        result_sp
    }
}

/// get-stdin() -> i32
#[no_mangle]
pub extern "C" fn wasi_p2_get_stdin(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_sp = sp.sub(1);
        *result_sp = 0u128; // stdin fd = 0
        result_sp
    }
}

/// get-stderr() -> i32
#[no_mangle]
pub extern "C" fn wasi_p2_get_stderr(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_sp = sp.sub(1);
        *result_sp = 2u128; // stderr fd = 2
        result_sp
    }
}

// =============================================================================
// wasi:io/streams
// =============================================================================

/// [method]output-stream.blocking-write-and-flush(handle: i32, ptr: u32, len: u32, result_ptr: u32)
#[no_mangle]
pub extern "C" fn wasi_p2_output_stream_write(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_offset = (*sp.add(0)) as u32;
        let len = (*sp.add(1)) as u64;
        let ptr = (*sp.add(2)) as u32;
        let handle = (*sp.add(3)) as i32;
        let mem = ctx.memory_base;

        let abs_buf = mem.add(ptr as usize);
        let ret = syscall3(SYS_WRITE, handle as u64, abs_buf as u64, len);

        let result_abs = mem.add(result_offset as usize);
        if ret <= len {
            // Success: discriminant=0
            *result_abs = 0;
        } else {
            // Error: discriminant=1
            *result_abs = 1;
        }
        sp.add(4)
    }
}

/// [method]input-stream.read(handle: i32, len: u64, result_ptr: u32)
#[no_mangle]
pub extern "C" fn wasi_p2_input_stream_read(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_offset = (*sp.add(0)) as u32;
        let len = (*sp.add(1)) as u64;
        let handle = (*sp.add(2)) as i32;
        let mem = ctx.memory_base;

        // Allocate a temporary buffer in WASM memory after result_ptr
        let buf_offset = result_offset + 12;
        let abs_buf = mem.add(buf_offset as usize);
        let ret = syscall3(SYS_READ, handle as u64, abs_buf as u64, len);

        let result_abs = mem.add(result_offset as usize);
        if ret != u64::MAX {
            // Success: discriminant=0, then list ptr and len
            *result_abs = 0;
            core::ptr::write_unaligned(result_abs.add(4) as *mut u32, buf_offset);
            core::ptr::write_unaligned(result_abs.add(8) as *mut u32, ret as u32);
        } else {
            // Error: discriminant=1
            *result_abs = 1;
        }
        sp.add(3)
    }
}

// =============================================================================
// wasi:io/poll
// =============================================================================

/// poll(in_ptr: u32, in_len: u32, ret_ptr: u32)
#[no_mangle]
pub extern "C" fn wasi_p2_poll(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        // No-op for now — just return empty ready list
        sp.add(3)
    }
}

/// [method]pollable.block(handle: i32)
#[no_mangle]
pub extern "C" fn wasi_p2_pollable_block(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let handle = (*sp.add(0)) as u64; // handle is ms duration
        if handle > 0 {
            syscall1(SYS_NANOSLEEP, handle);
            syscall1(SYS_YIELD, 0);
        } else {
            syscall1(SYS_YIELD, 0);
        }
        sp.add(1)
    }
}

/// [resource-drop]pollable(handle: i32)
#[no_mangle]
pub extern "C" fn wasi_p2_pollable_drop(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe { sp.add(1) }
}

/// [resource-drop]error(handle: i32)
#[no_mangle]
pub extern "C" fn wasi_p2_error_drop(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe { sp.add(1) }
}

// =============================================================================
// wasi:clocks/monotonic-clock
// =============================================================================

/// now() -> u64
#[no_mangle]
pub extern "C" fn wasi_p2_monotonic_now(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let ticks = syscall1(SYS_GET_TICKS, 0);
        let ns = ticks * 1_000_000; // ticks are in ms, convert to ns
        let result_sp = sp.sub(1);
        *result_sp = ns as u128;
        result_sp
    }
}

/// resolution() -> u64
#[no_mangle]
pub extern "C" fn wasi_p2_monotonic_resolution(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_sp = sp.sub(1);
        *result_sp = 1_000_000u128; // 1ms resolution in ns
        result_sp
    }
}

/// subscribe-duration(duration_ns: u64) -> i32 (pollable handle = ms)
#[no_mangle]
pub extern "C" fn wasi_p2_subscribe_duration(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let duration_ns = (*sp.add(0)) as u64;
        let ms = duration_ns / 1_000_000;
        let result_sp = sp.add(1).sub(1);
        *result_sp = ms as u128;
        result_sp
    }
}

// =============================================================================
// wasi:clocks/wall-clock
// =============================================================================

/// now(result_ptr: u32) — writes {seconds: u64, nanoseconds: u32}
#[no_mangle]
pub extern "C" fn wasi_p2_wall_clock_now(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_offset = (*sp.add(0)) as u32;
        let mem = ctx.memory_base;
        let nanos = syscall1(207, 0); // SYS_CLOCK_TIME_GET
        let secs = nanos / 1_000_000_000;
        let ns = (nanos % 1_000_000_000) as u32;
        let dest = mem.add(result_offset as usize);
        core::ptr::write_unaligned(dest as *mut u64, secs);
        core::ptr::write_unaligned(dest.add(8) as *mut u32, ns);
        sp.add(1)
    }
}

// =============================================================================
// wasi:filesystem/types
// =============================================================================

/// [resource-drop]descriptor(handle: i32) — close the fd
#[no_mangle]
pub extern "C" fn wasi_p2_descriptor_drop(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let handle = (*sp.add(0)) as u64;
        syscall1(SYS_CLOSE, handle);
        sp.add(1)
    }
}

/// [method]descriptor.open-at(dir_handle, flags, path_flags, path_ptr, path_len, oflags, result_ptr)
#[no_mangle]
pub extern "C" fn wasi_p2_descriptor_open_at(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_offset = (*sp.add(0)) as u32;
        let _oflags = (*sp.add(1)) as u32;
        let path_len = (*sp.add(2)) as u32;
        let path_ptr = (*sp.add(3)) as u32;
        let _path_flags = (*sp.add(4)) as u32;
        let _flags = (*sp.add(5)) as u32;
        let _dir_handle = (*sp.add(6)) as i32;
        let mem = ctx.memory_base;

        let abs_path = mem.add(path_ptr as usize);
        let ret = syscall2(SYS_OPEN, abs_path as u64, path_len as u64);

        let result_abs = mem.add(result_offset as usize);
        if ret != u64::MAX {
            *result_abs = 0; // Ok
            core::ptr::write_unaligned(result_abs.add(4) as *mut u32, ret as u32);
        } else {
            *result_abs = 1; // Err
        }
        sp.add(7)
    }
}

/// [method]descriptor.stat(handle: i32, result_ptr: u32)
#[no_mangle]
pub extern "C" fn wasi_p2_descriptor_stat(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_offset = (*sp.add(0)) as u32;
        let handle = (*sp.add(1)) as i32;
        let mem = ctx.memory_base;

        #[repr(C)]
        struct KernelStat { dev: u64, ino: u64, mode: u32, uid: u32, gid: u32, nlink: u32, size: u64, atime: u64, mtime: u64, ctime: u64 }

        let mut stat = core::mem::zeroed::<KernelStat>();
        let ret = syscall3(SYS_FSTAT, handle as u64, 0, &mut stat as *mut _ as u64);

        let result_abs = mem.add(result_offset as usize);
        if ret != u64::MAX {
            *result_abs = 0; // Ok
            let data = result_abs.add(8);
            // descriptor-type
            let dt = match stat.mode & 0o170000 {
                0o040000 => 3u8, // directory
                0o100000 => 4u8, // regular-file
                0o120000 => 5u8, // symbolic-link
                _ => 0u8,        // unknown
            };
            *data = dt;
            core::ptr::write_unaligned(data.add(8) as *mut u64, stat.nlink as u64);
            core::ptr::write_unaligned(data.add(16) as *mut u64, stat.size);
            // data-access-timestamp
            core::ptr::write_unaligned(data.add(24) as *mut u64, stat.atime);
            core::ptr::write_unaligned(data.add(32) as *mut u32, 0);
            // data-modification-timestamp
            core::ptr::write_unaligned(data.add(40) as *mut u64, stat.mtime);
            core::ptr::write_unaligned(data.add(48) as *mut u32, 0);
            // status-change-timestamp
            core::ptr::write_unaligned(data.add(56) as *mut u64, stat.ctime);
            core::ptr::write_unaligned(data.add(64) as *mut u32, 0);
        } else {
            *result_abs = 1; // Err
        }
        sp.add(2)
    }
}

/// [method]descriptor.set-size(handle: i32, size: u64, result_ptr: u32)
#[no_mangle]
pub extern "C" fn wasi_p2_descriptor_set_size(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_offset = (*sp.add(0)) as u32;
        let size = (*sp.add(1)) as u64;
        let handle = (*sp.add(2)) as i32;
        let mem = ctx.memory_base;
        let ret = syscall2(SYS_FTRUNCATE, handle as u64, size);
        let result_abs = mem.add(result_offset as usize);
        *result_abs = if ret != u64::MAX { 0 } else { 1 };
        sp.add(3)
    }
}

/// [method]descriptor.seek(handle: i32, offset: u64, whence: i32, result_ptr: u32)
#[no_mangle]
pub extern "C" fn wasi_p2_descriptor_seek(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_offset = (*sp.add(0)) as u32;
        let whence = (*sp.add(1)) as u64;
        let offset = (*sp.add(2)) as u64;
        let handle = (*sp.add(3)) as i32;
        let mem = ctx.memory_base;
        let ret = syscall3(SYS_LSEEK, handle as u64, offset, whence);
        let result_abs = mem.add(result_offset as usize);
        if ret != u64::MAX {
            *result_abs = 0;
            core::ptr::write_unaligned(result_abs.add(8) as *mut u64, ret);
        } else {
            *result_abs = 1;
        }
        sp.add(4)
    }
}

/// [method]descriptor.create-directory-at(handle: i32, path_ptr: u32, path_len: u32, result_ptr: u32)
#[no_mangle]
pub extern "C" fn wasi_p2_descriptor_create_dir(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_offset = (*sp.add(0)) as u32;
        let path_len = (*sp.add(1)) as u64;
        let path_ptr = (*sp.add(2)) as u32;
        let _handle = (*sp.add(3)) as i32;
        let mem = ctx.memory_base;
        let abs_path = mem.add(path_ptr as usize);
        let ret = syscall2(SYS_MKDIR, abs_path as u64, path_len);
        let result_abs = mem.add(result_offset as usize);
        *result_abs = if ret != u64::MAX { 0 } else { 1 };
        sp.add(4)
    }
}

/// [method]descriptor.unlink-file-at(handle: i32, path_ptr: u32, path_len: u32, result_ptr: u32)
#[no_mangle]
pub extern "C" fn wasi_p2_descriptor_unlink(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_offset = (*sp.add(0)) as u32;
        let path_len = (*sp.add(1)) as u64;
        let path_ptr = (*sp.add(2)) as u32;
        let _handle = (*sp.add(3)) as i32;
        let mem = ctx.memory_base;
        let abs_path = mem.add(path_ptr as usize);
        let ret = syscall2(SYS_UNLINK, abs_path as u64, path_len);
        let result_abs = mem.add(result_offset as usize);
        *result_abs = if ret != u64::MAX { 0 } else { 1 };
        sp.add(4)
    }
}

/// [method]descriptor.remove-directory-at(handle: i32, path_ptr: u32, path_len: u32, result_ptr: u32)
#[no_mangle]
pub extern "C" fn wasi_p2_descriptor_rmdir(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_offset = (*sp.add(0)) as u32;
        let path_len = (*sp.add(1)) as u64;
        let path_ptr = (*sp.add(2)) as u32;
        let _handle = (*sp.add(3)) as i32;
        let mem = ctx.memory_base;
        let abs_path = mem.add(path_ptr as usize);
        let ret = syscall2(SYS_RMDIR, abs_path as u64, path_len);
        let result_abs = mem.add(result_offset as usize);
        *result_abs = if ret != u64::MAX { 0 } else { 1 };
        sp.add(4)
    }
}

/// [method]descriptor.rename-at(handle, old_path_ptr, old_path_len, new_handle, new_path_ptr, new_path_len, result_ptr)
#[no_mangle]
pub extern "C" fn wasi_p2_descriptor_rename(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_offset = (*sp.add(0)) as u32;
        let new_path_len = (*sp.add(1)) as u64;
        let new_path_ptr = (*sp.add(2)) as u32;
        let _new_handle = (*sp.add(3)) as i32;
        let old_path_len = (*sp.add(4)) as u64;
        let old_path_ptr = (*sp.add(5)) as u32;
        let _handle = (*sp.add(6)) as i32;
        let mem = ctx.memory_base;
        let abs_old = mem.add(old_path_ptr as usize);
        let abs_new = mem.add(new_path_ptr as usize);
        let ret = crate::syscall::syscall4(SYS_RENAME, abs_old as u64, old_path_len, abs_new as u64, new_path_len);
        let result_abs = mem.add(result_offset as usize);
        *result_abs = if ret != u64::MAX { 0 } else { 1 };
        sp.add(7)
    }
}

/// [method]descriptor.read-directory(handle: i32, result_ptr: u32)
#[no_mangle]
pub extern "C" fn wasi_p2_descriptor_read_directory(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_offset = (*sp.add(0)) as u32;
        let handle = (*sp.add(1)) as i32;
        let mem = ctx.memory_base;
        let result_abs = mem.add(result_offset as usize);
        // Return a directory stream handle = the fd itself
        *result_abs = 0; // Ok
        core::ptr::write_unaligned(result_abs.add(4) as *mut u32, handle as u32);
        sp.add(2)
    }
}

/// [resource-drop]directory-entry-stream(handle: i32)
#[no_mangle]
pub extern "C" fn wasi_p2_dir_stream_drop(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe { sp.add(1) }
}

// =============================================================================
// wasi:random/random
// =============================================================================

/// get-random-bytes(len: u64, result_ptr: u32)
#[no_mangle]
pub extern "C" fn wasi_p2_get_random_bytes(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_offset = (*sp.add(0)) as u32;
        let len = (*sp.add(1)) as u64;
        let mem = ctx.memory_base;
        let abs_ptr = mem.add(result_offset as usize);
        syscall2(SYS_RANDOM_GET, abs_ptr as u64, len);
        sp.add(2)
    }
}

// =============================================================================
// wasi:sockets/instance-network
// =============================================================================

/// instance-network() -> i32
#[no_mangle]
pub extern "C" fn wasi_p2_instance_network(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_sp = sp.sub(1);
        *result_sp = 0u128;
        result_sp
    }
}
