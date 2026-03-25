use crate::context::Ring3Context;
use crate::syscall::{syscall1, syscall2, syscall3, syscall4, syscall5};

// Kernel syscall numbers
const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_OPEN: u64 = 2;
const SYS_CLOSE: u64 = 3;
const SYS_STAT: u64 = 4;
const SYS_FSTAT: u64 = 5;
const SYS_LSEEK: u64 = 8;
const SYS_PREAD64: u64 = 17;
const SYS_FTRUNCATE: u64 = 77;
const SYS_GETDENTS: u64 = 78;
const SYS_RENAME: u64 = 82;
const SYS_MKDIR: u64 = 83;
const SYS_RMDIR: u64 = 84;
const SYS_UNLINK: u64 = 87;
const SYS_EXIT: u64 = 60;
const SYS_YIELD: u64 = 129;
const SYS_LINKAT: u64 = 265;
const SYS_SYMLINKAT: u64 = 266;
const SYS_READLINKAT: u64 = 267;
const SYS_ARGS_SIZES_GET: u64 = 202;
const SYS_ARGS_GET: u64 = 203;
const SYS_ENVIRON_SIZES_GET: u64 = 205;
const SYS_ENVIRON_GET: u64 = 204;
const SYS_CLOCK_TIME_GET: u64 = 207;
const SYS_RANDOM_GET: u64 = 208;
const SYS_FD_PRESTAT_GET: u64 = 209;
const SYS_FD_PRESTAT_DIR_NAME: u64 = 210;
const SYS_POLL: u64 = 7;
const SYS_DEBUG_PRINT: u64 = 999;

// WASI errno constants
const ERRNO_SUCCESS: u32 = 0;
const ERRNO_BADF: u32 = 8;
const ERRNO_INVAL: u32 = 28;
const ERRNO_NOSYS: u32 = 52;
const ERRNO_NOENT: u32 = 44;

// WASI filetype constants
const FILETYPE_UNKNOWN: u8 = 0;
const FILETYPE_DIR: u8 = 3;
const FILETYPE_REGULAR: u8 = 4;
const FILETYPE_SYMLINK: u8 = 7;
const FILETYPE_CHAR_DEVICE: u8 = 2;

// Kernel Stat struct layout (must match kernel/src/fs/vfs.rs::Stat)
#[repr(C)]
struct KernelStat {
    dev: u64,
    ino: u64,
    mode: u32,
    uid: u32,
    gid: u32,
    nlink: u32,
    size: u64,
    atime: u64,
    mtime: u64,
    ctime: u64,
}

fn mode_to_filetype(mode: u32) -> u8 {
    let fmt = mode & 0o170000;
    match fmt {
        0o040000 => FILETYPE_DIR,
        0o100000 => FILETYPE_REGULAR,
        0o120000 => FILETYPE_SYMLINK,
        0o020000 => FILETYPE_CHAR_DEVICE,
        _ => FILETYPE_UNKNOWN,
    }
}

// Helper: write a u32 to WASM linear memory
#[inline(always)]
unsafe fn mem_write_u32(mem: *mut u8, offset: u32, val: u32) {
    core::ptr::write_unaligned(mem.add(offset as usize) as *mut u32, val);
}

// Helper: write a u64 to WASM linear memory
#[inline(always)]
unsafe fn mem_write_u64(mem: *mut u8, offset: u32, val: u64) {
    core::ptr::write_unaligned(mem.add(offset as usize) as *mut u64, val);
}

// Helper: read a u32 from WASM linear memory
#[inline(always)]
unsafe fn mem_read_u32(mem: *mut u8, offset: u32) -> u32 {
    core::ptr::read_unaligned(mem.add(offset as usize) as *const u32)
}

// =============================================================================
// WASI Preview 1 Stubs
// =============================================================================

/// fd_write(fd, iovs, iovs_len, nwritten) -> errno
#[no_mangle]
pub extern "C" fn wasi_fd_write(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let nwritten_ptr = (*sp.add(0)) as u32;
        let iovs_len     = (*sp.add(1)) as u32;
        let iovs_offset  = (*sp.add(2)) as u32;
        let fd           = (*sp.add(3)) as i32;

        let mem = ctx.memory_base;
        let mut total_written: u32 = 0;

        for i in 0..iovs_len {
            let iov_addr = mem.add((iovs_offset + i * 8) as usize);
            let buf_offset = core::ptr::read_unaligned(iov_addr as *const u32);
            let buf_len    = core::ptr::read_unaligned(iov_addr.add(4) as *const u32);

            let buf_ptr = mem.add(buf_offset as usize);
            let ret = syscall3(SYS_WRITE, fd as u64, buf_ptr as u64, buf_len as u64);

            if ret > buf_len as u64 {
                let result_sp = sp.add(4).sub(1);
                *result_sp = ERRNO_BADF as u128;
                return result_sp;
            }
            total_written += ret as u32;
        }

        mem_write_u32(mem, nwritten_ptr, total_written);

        let result_sp = sp.add(4).sub(1);
        *result_sp = ERRNO_SUCCESS as u128;
        result_sp
    }
}

/// fd_read(fd, iovs, iovs_len, nread) -> errno
#[no_mangle]
pub extern "C" fn wasi_fd_read(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let nread_ptr   = (*sp.add(0)) as u32;
        let iovs_len    = (*sp.add(1)) as u32;
        let iovs_offset = (*sp.add(2)) as u32;
        let fd          = (*sp.add(3)) as i32;

        let mem = ctx.memory_base;
        let mut total_read: u32 = 0;

        for i in 0..iovs_len {
            let iov_addr = mem.add((iovs_offset + i * 8) as usize);
            let buf_offset = core::ptr::read_unaligned(iov_addr as *const u32);
            let buf_len    = core::ptr::read_unaligned(iov_addr.add(4) as *const u32);

            let buf_ptr = mem.add(buf_offset as usize);
            let ret = syscall3(SYS_READ, fd as u64, buf_ptr as u64, buf_len as u64);

            if ret > buf_len as u64 {
                let result_sp = sp.add(4).sub(1);
                *result_sp = ERRNO_BADF as u128;
                return result_sp;
            }
            total_read += ret as u32;
            if ret < buf_len as u64 { break; }
        }

        mem_write_u32(mem, nread_ptr, total_read);

        let result_sp = sp.add(4).sub(1);
        *result_sp = ERRNO_SUCCESS as u128;
        result_sp
    }
}

/// fd_close(fd) -> errno
#[no_mangle]
pub extern "C" fn wasi_fd_close(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let fd = (*sp.add(0)) as i32;
        let res = syscall1(SYS_CLOSE, fd as u64);
        let result_sp = sp.add(1).sub(1);
        *result_sp = (if res == u64::MAX { ERRNO_BADF } else { ERRNO_SUCCESS }) as u128;
        result_sp
    }
}

/// proc_exit(rval) -> !
#[no_mangle]
pub extern "C" fn wasi_proc_exit(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let exit_code = (*sp.add(0)) as u64;
        syscall1(SYS_EXIT, exit_code);
    }
    loop {}
}

/// args_sizes_get(argc_ptr, argv_buf_size_ptr) -> errno
/// Kernel SYS_ARGS_SIZES_GET returns: rax=count, rdi=total_size
#[no_mangle]
pub extern "C" fn wasi_args_sizes_get(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let buf_size_ptr = (*sp.add(0)) as u32;
        let argc_ptr     = (*sp.add(1)) as u32;
        let mem = ctx.memory_base;

        let rax: u64;
        let rdi: u64;
        core::arch::asm!(
            "syscall",
            in("rax") SYS_ARGS_SIZES_GET,
            in("rdi") 0u64,
            lateout("rax") rax,
            lateout("rdi") rdi,
            lateout("rcx") _,
            lateout("r11") _,
            lateout("rsi") _,
            options(nostack)
        );

        if rax == u64::MAX {
            let result_sp = sp.add(2).sub(1);
            *result_sp = ERRNO_INVAL as u128;
            return result_sp;
        }

        mem_write_u32(mem, argc_ptr, rax as u32);
        mem_write_u32(mem, buf_size_ptr, rdi as u32);

        let result_sp = sp.add(2).sub(1);
        *result_sp = ERRNO_SUCCESS as u128;
        result_sp
    }
}

/// args_get(argv, argv_buf) -> errno
/// Kernel writes u64 absolute pointers to argv. We use a stack buffer, then convert.
#[no_mangle]
pub extern "C" fn wasi_args_get(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let argv_buf_ptr = (*sp.add(0)) as u32;
        let argv_ptr     = (*sp.add(1)) as u32;
        let mem = ctx.memory_base;

        // Get argc first
        let argc: u64;
        core::arch::asm!(
            "syscall",
            in("rax") SYS_ARGS_SIZES_GET,
            in("rdi") 0u64,
            lateout("rax") argc,
            lateout("rdi") _,
            lateout("rcx") _,
            lateout("r11") _,
            lateout("rsi") _,
            options(nostack)
        );

        if argc == 0 || argc == u64::MAX {
            let result_sp = sp.add(2).sub(1);
            *result_sp = ERRNO_SUCCESS as u128;
            return result_sp;
        }

        // Use stack buffer for kernel's u64 pointers (max 64 args)
        let max_args = if argc > 64 { 64 } else { argc as usize };
        let mut ptr_buf = [0u64; 64];

        let argv_buf_abs = mem.add(argv_buf_ptr as usize);
        let res = syscall2(SYS_ARGS_GET, ptr_buf.as_mut_ptr() as u64, argv_buf_abs as u64);

        if res == u64::MAX {
            let result_sp = sp.add(2).sub(1);
            *result_sp = ERRNO_INVAL as u128;
            return result_sp;
        }

        // Convert absolute pointers to u32 WASM offsets
        let base = mem as u64;
        for i in 0..max_args {
            let wasm_offset = (ptr_buf[i] - base) as u32;
            mem_write_u32(mem, argv_ptr + (i as u32) * 4, wasm_offset);
        }

        let result_sp = sp.add(2).sub(1);
        *result_sp = ERRNO_SUCCESS as u128;
        result_sp
    }
}

/// environ_sizes_get(count_ptr, buf_size_ptr) -> errno
#[no_mangle]
pub extern "C" fn wasi_environ_sizes_get(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let buf_size_ptr = (*sp.add(0)) as u32;
        let count_ptr    = (*sp.add(1)) as u32;
        let mem = ctx.memory_base;

        let rax: u64;
        let rdi: u64;
        core::arch::asm!(
            "syscall",
            in("rax") SYS_ENVIRON_SIZES_GET,
            in("rdi") 0u64,
            lateout("rax") rax,
            lateout("rdi") rdi,
            lateout("rcx") _,
            lateout("r11") _,
            lateout("rsi") _,
            options(nostack)
        );

        mem_write_u32(mem, count_ptr, rax as u32);
        mem_write_u32(mem, buf_size_ptr, rdi as u32);

        let result_sp = sp.add(2).sub(1);
        *result_sp = ERRNO_SUCCESS as u128;
        result_sp
    }
}

/// environ_get(env, env_buf) -> errno
#[no_mangle]
pub extern "C" fn wasi_environ_get(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let env_buf_ptr = (*sp.add(0)) as u32;
        let env_ptr     = (*sp.add(1)) as u32;
        let mem = ctx.memory_base;

        // Get environ count first
        let env_count: u64;
        core::arch::asm!(
            "syscall",
            in("rax") SYS_ENVIRON_SIZES_GET,
            in("rdi") 0u64,
            lateout("rax") env_count,
            lateout("rdi") _,
            lateout("rcx") _,
            lateout("r11") _,
            lateout("rsi") _,
            options(nostack)
        );

        if env_count == 0 || env_count == u64::MAX {
            let result_sp = sp.add(2).sub(1);
            *result_sp = ERRNO_SUCCESS as u128;
            return result_sp;
        }

        // Use stack buffer for kernel's u64 pointers
        let max_envs = if env_count > 64 { 64 } else { env_count as usize };
        let mut ptr_buf = [0u64; 64];

        let env_buf_abs = mem.add(env_buf_ptr as usize);
        let res = syscall2(SYS_ENVIRON_GET, ptr_buf.as_mut_ptr() as u64, env_buf_abs as u64);

        if res == u64::MAX {
            let result_sp = sp.add(2).sub(1);
            *result_sp = ERRNO_INVAL as u128;
            return result_sp;
        }

        // Convert absolute pointers to u32 WASM offsets
        let base = mem as u64;
        for i in 0..max_envs {
            let wasm_offset = (ptr_buf[i] - base) as u32;
            mem_write_u32(mem, env_ptr + (i as u32) * 4, wasm_offset);
        }

        let result_sp = sp.add(2).sub(1);
        *result_sp = ERRNO_SUCCESS as u128;
        result_sp
    }
}

/// clock_time_get(id, precision, time_ptr) -> errno
#[no_mangle]
pub extern "C" fn wasi_clock_time_get(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let time_ptr  = (*sp.add(0)) as u32;
        let _precision = (*sp.add(1)) as u64;
        let _id        = (*sp.add(2)) as u32;
        let mem = ctx.memory_base;

        let nanos = syscall1(SYS_CLOCK_TIME_GET, 0);
        mem_write_u64(mem, time_ptr, nanos);

        let result_sp = sp.add(3).sub(1);
        *result_sp = ERRNO_SUCCESS as u128;
        result_sp
    }
}

/// random_get(buf, buf_len) -> errno
#[no_mangle]
pub extern "C" fn wasi_random_get(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let buf_len = (*sp.add(0)) as u32;
        let buf_ptr = (*sp.add(1)) as u32;
        let mem = ctx.memory_base;

        let abs_ptr = mem.add(buf_ptr as usize);
        syscall2(SYS_RANDOM_GET, abs_ptr as u64, buf_len as u64);

        let result_sp = sp.add(2).sub(1);
        *result_sp = ERRNO_SUCCESS as u128;
        result_sp
    }
}

/// fd_prestat_get(fd, prestat_ptr) -> errno
#[no_mangle]
pub extern "C" fn wasi_fd_prestat_get(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let prestat_ptr = (*sp.add(0)) as u32;
        let fd          = (*sp.add(1)) as i32;
        let mem = ctx.memory_base;

        let buf_abs = mem.add(prestat_ptr as usize);
        let res = syscall2(SYS_FD_PRESTAT_GET, fd as u64, buf_abs as u64);

        let result_sp = sp.add(2).sub(1);
        *result_sp = res as u128; // kernel returns 0 on success, 8 (EBADF) on failure
        result_sp
    }
}

/// fd_prestat_dir_name(fd, path, path_len) -> errno
#[no_mangle]
pub extern "C" fn wasi_fd_prestat_dir_name(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let path_len = (*sp.add(0)) as u32;
        let path_ptr = (*sp.add(1)) as u32;
        let fd       = (*sp.add(2)) as i32;
        let mem = ctx.memory_base;

        let buf_abs = mem.add(path_ptr as usize);
        let res = syscall3(SYS_FD_PRESTAT_DIR_NAME, fd as u64, buf_abs as u64, path_len as u64);

        let result_sp = sp.add(3).sub(1);
        *result_sp = res as u128;
        result_sp
    }
}

/// fd_fdstat_get(fd, fdstat_ptr) -> errno
/// WASI fdstat layout: filetype(1) + pad(1) + flags(2) + pad(4) + rights_base(8) + rights_inheriting(8) = 24 bytes
#[no_mangle]
pub extern "C" fn wasi_fd_fdstat_get(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let fdstat_ptr = (*sp.add(0)) as u32;
        let fd         = (*sp.add(1)) as i32;
        let mem = ctx.memory_base;

        // Use fstat to get the file type, then fill in the fdstat structure
        let mut stat = core::mem::zeroed::<KernelStat>();
        let stat_ptr = &mut stat as *mut KernelStat;

        // SYS_FSTAT: rdi=fd, rdx=stat_ptr (the kernel expects rdx for the stat output)
        let res = syscall3(SYS_FSTAT, fd as u64, 0, stat_ptr as u64);

        let base = fdstat_ptr;
        if res == u64::MAX {
            // For stdio fds (0,1,2), return char device type even if fstat fails
            if fd >= 0 && fd <= 2 {
                *mem.add(base as usize) = FILETYPE_CHAR_DEVICE;
                *mem.add(base as usize + 1) = 0; // pad
                mem_write_u32(mem, base + 2, 0); // flags (pad included) — actually just 2 bytes for flags
                core::ptr::write_unaligned(mem.add(base as usize + 2) as *mut u16, 0); // fdflags
                mem_write_u32(mem, base + 4, 0); // pad
                mem_write_u64(mem, base + 8, u64::MAX); // rights_base - all rights
                mem_write_u64(mem, base + 16, u64::MAX); // rights_inheriting
                let result_sp = sp.add(2).sub(1);
                *result_sp = ERRNO_SUCCESS as u128;
                return result_sp;
            }
            let result_sp = sp.add(2).sub(1);
            *result_sp = ERRNO_BADF as u128;
            return result_sp;
        }

        let filetype = mode_to_filetype(stat.mode);
        *mem.add(base as usize) = filetype;
        *mem.add(base as usize + 1) = 0;
        core::ptr::write_unaligned(mem.add(base as usize + 2) as *mut u16, 0); // fdflags
        mem_write_u32(mem, base + 4, 0); // pad
        mem_write_u64(mem, base + 8, u64::MAX);
        mem_write_u64(mem, base + 16, u64::MAX);

        let result_sp = sp.add(2).sub(1);
        *result_sp = ERRNO_SUCCESS as u128;
        result_sp
    }
}

/// fd_filestat_get(fd, filestat_ptr) -> errno
/// WASI filestat layout: dev(8) + ino(8) + filetype(1) + pad(7) + nlink(8) + size(8) + atim(8) + mtim(8) + ctim(8) = 64 bytes
#[no_mangle]
pub extern "C" fn wasi_fd_filestat_get(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let filestat_ptr = (*sp.add(0)) as u32;
        let fd           = (*sp.add(1)) as i32;
        let mem = ctx.memory_base;

        let mut stat = core::mem::zeroed::<KernelStat>();
        let stat_ptr = &mut stat as *mut KernelStat;
        let res = syscall3(SYS_FSTAT, fd as u64, 0, stat_ptr as u64);

        if res == u64::MAX {
            let result_sp = sp.add(2).sub(1);
            *result_sp = ERRNO_BADF as u128;
            return result_sp;
        }

        let base = filestat_ptr;
        mem_write_u64(mem, base, stat.dev);
        mem_write_u64(mem, base + 8, stat.ino);
        *mem.add(base as usize + 16) = mode_to_filetype(stat.mode);
        // pad 7 bytes
        for i in 1..8 { *mem.add(base as usize + 16 + i) = 0; }
        mem_write_u64(mem, base + 24, stat.nlink as u64);
        mem_write_u64(mem, base + 32, stat.size);
        mem_write_u64(mem, base + 40, stat.atime);
        mem_write_u64(mem, base + 48, stat.mtime);
        mem_write_u64(mem, base + 56, stat.ctime);

        let result_sp = sp.add(2).sub(1);
        *result_sp = ERRNO_SUCCESS as u128;
        result_sp
    }
}

/// fd_filestat_set_size(fd, size) -> errno
#[no_mangle]
pub extern "C" fn wasi_fd_filestat_set_size(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let size = (*sp.add(0)) as u64;
        let fd   = (*sp.add(1)) as i32;

        let res = syscall2(SYS_FTRUNCATE, fd as u64, size);

        let result_sp = sp.add(2).sub(1);
        *result_sp = (if res == u64::MAX { ERRNO_BADF } else { ERRNO_SUCCESS }) as u128;
        result_sp
    }
}

/// fd_seek(fd, offset, whence, newoffset_ptr) -> errno
#[no_mangle]
pub extern "C" fn wasi_fd_seek(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let newoffset_ptr = (*sp.add(0)) as u32;
        let whence        = (*sp.add(1)) as u32;
        let offset        = (*sp.add(2)) as i64;
        let fd            = (*sp.add(3)) as i32;
        let mem = ctx.memory_base;

        let res = syscall3(SYS_LSEEK, fd as u64, offset as u64, whence as u64);

        if res == u64::MAX {
            let result_sp = sp.add(4).sub(1);
            *result_sp = ERRNO_BADF as u128;
            return result_sp;
        }

        mem_write_u64(mem, newoffset_ptr, res);

        let result_sp = sp.add(4).sub(1);
        *result_sp = ERRNO_SUCCESS as u128;
        result_sp
    }
}

/// fd_pread(fd, iovs, iovs_len, offset, nread_ptr) -> errno
#[no_mangle]
pub extern "C" fn wasi_fd_pread(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let nread_ptr   = (*sp.add(0)) as u32;
        let offset      = (*sp.add(1)) as u64;
        let iovs_len    = (*sp.add(2)) as u32;
        let iovs_offset = (*sp.add(3)) as u32;
        let fd          = (*sp.add(4)) as i32;
        let mem = ctx.memory_base;

        let mut total_read: u32 = 0;
        let mut cur_offset = offset;

        for i in 0..iovs_len {
            let iov_addr = mem.add((iovs_offset + i * 8) as usize);
            let buf_offset = core::ptr::read_unaligned(iov_addr as *const u32);
            let buf_len    = core::ptr::read_unaligned(iov_addr.add(4) as *const u32);

            let buf_ptr = mem.add(buf_offset as usize);
            let ret = syscall4(SYS_PREAD64, fd as u64, buf_ptr as u64, buf_len as u64, cur_offset);

            if ret > buf_len as u64 {
                let result_sp = sp.add(5).sub(1);
                *result_sp = ERRNO_BADF as u128;
                return result_sp;
            }
            total_read += ret as u32;
            cur_offset += ret;
            if ret < buf_len as u64 { break; }
        }

        mem_write_u32(mem, nread_ptr, total_read);

        let result_sp = sp.add(5).sub(1);
        *result_sp = ERRNO_SUCCESS as u128;
        result_sp
    }
}

/// fd_readdir(fd, buf, buf_len, cookie, bufused_ptr) -> errno
#[no_mangle]
pub extern "C" fn wasi_fd_readdir(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let bufused_ptr = (*sp.add(0)) as u32;
        let cookie      = (*sp.add(1)) as u64;
        let buf_len     = (*sp.add(2)) as u32;
        let buf_ptr     = (*sp.add(3)) as u32;
        let fd          = (*sp.add(4)) as i32;
        let mem = ctx.memory_base;

        let abs_buf = mem.add(buf_ptr as usize);
        // Use SYS_GETDENTS: rdi=fd, rsi=buf, rdx=buf_len
        let res = syscall3(SYS_GETDENTS, fd as u64, abs_buf as u64, buf_len as u64);

        if res == u64::MAX {
            let result_sp = sp.add(5).sub(1);
            *result_sp = ERRNO_BADF as u128;
            return result_sp;
        }

        mem_write_u32(mem, bufused_ptr, res as u32);

        let result_sp = sp.add(5).sub(1);
        *result_sp = ERRNO_SUCCESS as u128;
        result_sp
    }
}

/// path_open(fd, dirflags, path, path_len, oflags, fs_rights_base, fs_rights_inheriting, fdflags, opened_fd_ptr) -> errno
#[no_mangle]
pub extern "C" fn wasi_path_open(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let opened_fd_ptr       = (*sp.add(0)) as u32;
        let _fdflags            = (*sp.add(1)) as u32;
        let _fs_rights_inherit  = (*sp.add(2)) as u64;
        let _fs_rights_base     = (*sp.add(3)) as u64;
        let oflags              = (*sp.add(4)) as u32;
        let path_len            = (*sp.add(5)) as u32;
        let path_ptr            = (*sp.add(6)) as u32;
        let _dirflags           = (*sp.add(7)) as u32;
        let _dirfd              = (*sp.add(8)) as i32;
        let mem = ctx.memory_base;

        // Build absolute path from WASM memory
        let path_abs = mem.add(path_ptr as usize);

        // SYS_OPEN: rdi=path_ptr, rsi=path_len (kernel resolves the path)
        let res = syscall2(SYS_OPEN, path_abs as u64, path_len as u64);

        if res == u64::MAX {
            // If OFLAGS_CREAT is set and open failed, we could try to create,
            // but for now return ENOENT
            let result_sp = sp.add(9).sub(1);
            *result_sp = ERRNO_NOENT as u128;
            return result_sp;
        }

        mem_write_u32(mem, opened_fd_ptr, res as u32);

        let result_sp = sp.add(9).sub(1);
        *result_sp = ERRNO_SUCCESS as u128;
        result_sp
    }
}

/// path_filestat_get(fd, flags, path, path_len, filestat_ptr) -> errno
#[no_mangle]
pub extern "C" fn wasi_path_filestat_get(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let filestat_ptr = (*sp.add(0)) as u32;
        let path_len     = (*sp.add(1)) as u32;
        let path_ptr     = (*sp.add(2)) as u32;
        let _flags       = (*sp.add(3)) as u32;
        let _fd          = (*sp.add(4)) as i32;
        let mem = ctx.memory_base;

        let path_abs = mem.add(path_ptr as usize);
        // SYS_STAT: rdi=path_ptr, rsi=path_len, rdx=stat_ptr
        let mut stat = core::mem::zeroed::<KernelStat>();
        let stat_ptr = &mut stat as *mut KernelStat;
        let res = syscall3(SYS_STAT, path_abs as u64, path_len as u64, stat_ptr as u64);

        if res == u64::MAX {
            let result_sp = sp.add(5).sub(1);
            *result_sp = ERRNO_NOENT as u128;
            return result_sp;
        }

        let base = filestat_ptr;
        mem_write_u64(mem, base, stat.dev);
        mem_write_u64(mem, base + 8, stat.ino);
        *mem.add(base as usize + 16) = mode_to_filetype(stat.mode);
        for i in 1..8 { *mem.add(base as usize + 16 + i) = 0; }
        mem_write_u64(mem, base + 24, stat.nlink as u64);
        mem_write_u64(mem, base + 32, stat.size);
        mem_write_u64(mem, base + 40, stat.atime);
        mem_write_u64(mem, base + 48, stat.mtime);
        mem_write_u64(mem, base + 56, stat.ctime);

        let result_sp = sp.add(5).sub(1);
        *result_sp = ERRNO_SUCCESS as u128;
        result_sp
    }
}

/// path_create_directory(fd, path, path_len) -> errno
#[no_mangle]
pub extern "C" fn wasi_path_create_directory(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let path_len = (*sp.add(0)) as u32;
        let path_ptr = (*sp.add(1)) as u32;
        let _fd      = (*sp.add(2)) as i32;
        let mem = ctx.memory_base;

        let path_abs = mem.add(path_ptr as usize);
        let res = syscall2(SYS_MKDIR, path_abs as u64, path_len as u64);

        let result_sp = sp.add(3).sub(1);
        *result_sp = (if res == u64::MAX { ERRNO_NOENT } else { ERRNO_SUCCESS }) as u128;
        result_sp
    }
}

/// path_unlink_file(fd, path, path_len) -> errno
#[no_mangle]
pub extern "C" fn wasi_path_unlink_file(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let path_len = (*sp.add(0)) as u32;
        let path_ptr = (*sp.add(1)) as u32;
        let _fd      = (*sp.add(2)) as i32;
        let mem = ctx.memory_base;

        let path_abs = mem.add(path_ptr as usize);
        let res = syscall2(SYS_UNLINK, path_abs as u64, path_len as u64);

        let result_sp = sp.add(3).sub(1);
        *result_sp = (if res == u64::MAX { ERRNO_NOENT } else { ERRNO_SUCCESS }) as u128;
        result_sp
    }
}

/// path_remove_directory(fd, path, path_len) -> errno
#[no_mangle]
pub extern "C" fn wasi_path_remove_directory(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let path_len = (*sp.add(0)) as u32;
        let path_ptr = (*sp.add(1)) as u32;
        let _fd      = (*sp.add(2)) as i32;
        let mem = ctx.memory_base;

        let path_abs = mem.add(path_ptr as usize);
        let res = syscall2(SYS_RMDIR, path_abs as u64, path_len as u64);

        let result_sp = sp.add(3).sub(1);
        *result_sp = (if res == u64::MAX { ERRNO_NOENT } else { ERRNO_SUCCESS }) as u128;
        result_sp
    }
}

/// path_rename(fd, old_path, old_path_len, new_fd, new_path, new_path_len) -> errno
#[no_mangle]
pub extern "C" fn wasi_path_rename(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let new_path_len = (*sp.add(0)) as u32;
        let new_path_ptr = (*sp.add(1)) as u32;
        let _new_fd      = (*sp.add(2)) as i32;
        let old_path_len = (*sp.add(3)) as u32;
        let old_path_ptr = (*sp.add(4)) as u32;
        let _old_fd      = (*sp.add(5)) as i32;
        let mem = ctx.memory_base;

        let old_abs = mem.add(old_path_ptr as usize);
        let new_abs = mem.add(new_path_ptr as usize);
        let res = syscall4(SYS_RENAME, old_abs as u64, old_path_len as u64, new_abs as u64, new_path_len as u64);

        let result_sp = sp.add(6).sub(1);
        *result_sp = (if res == u64::MAX { ERRNO_NOENT } else { ERRNO_SUCCESS }) as u128;
        result_sp
    }
}

/// path_link(old_fd, old_flags, old_path, old_path_len, new_fd, new_path, new_path_len) -> errno
#[no_mangle]
pub extern "C" fn wasi_path_link(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let new_path_len = (*sp.add(0)) as u32;
        let new_path_ptr = (*sp.add(1)) as u32;
        let _new_fd      = (*sp.add(2)) as i32;
        let old_path_len = (*sp.add(3)) as u32;
        let old_path_ptr = (*sp.add(4)) as u32;
        let _old_flags   = (*sp.add(5)) as u32;
        let _old_fd      = (*sp.add(6)) as i32;
        let mem = ctx.memory_base;

        let old_abs = mem.add(old_path_ptr as usize);
        let new_abs = mem.add(new_path_ptr as usize);
        let res = syscall4(SYS_LINKAT, old_abs as u64, old_path_len as u64, new_abs as u64, new_path_len as u64);

        let result_sp = sp.add(7).sub(1);
        *result_sp = (if res == u64::MAX { ERRNO_NOENT } else { ERRNO_SUCCESS }) as u128;
        result_sp
    }
}

/// path_symlink(old_path, old_path_len, fd, new_path, new_path_len) -> errno
#[no_mangle]
pub extern "C" fn wasi_path_symlink(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let new_path_len = (*sp.add(0)) as u32;
        let new_path_ptr = (*sp.add(1)) as u32;
        let _fd          = (*sp.add(2)) as i32;
        let old_path_len = (*sp.add(3)) as u32;
        let old_path_ptr = (*sp.add(4)) as u32;
        let mem = ctx.memory_base;

        let old_abs = mem.add(old_path_ptr as usize);
        let new_abs = mem.add(new_path_ptr as usize);
        let res = syscall4(SYS_SYMLINKAT, old_abs as u64, old_path_len as u64, new_abs as u64, new_path_len as u64);

        let result_sp = sp.add(5).sub(1);
        *result_sp = (if res == u64::MAX { ERRNO_NOENT } else { ERRNO_SUCCESS }) as u128;
        result_sp
    }
}

/// path_readlink(fd, path, path_len, buf, buf_len, bufused_ptr) -> errno
#[no_mangle]
pub extern "C" fn wasi_path_readlink(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let bufused_ptr = (*sp.add(0)) as u32;
        let buf_len     = (*sp.add(1)) as u32;
        let buf_ptr     = (*sp.add(2)) as u32;
        let path_len    = (*sp.add(3)) as u32;
        let path_ptr    = (*sp.add(4)) as u32;
        let _fd         = (*sp.add(5)) as i32;
        let mem = ctx.memory_base;

        let path_abs = mem.add(path_ptr as usize);
        let buf_abs = mem.add(buf_ptr as usize);
        let res = syscall4(SYS_READLINKAT, path_abs as u64, path_len as u64, buf_abs as u64, buf_len as u64);

        if res == u64::MAX {
            let result_sp = sp.add(6).sub(1);
            *result_sp = ERRNO_NOENT as u128;
            return result_sp;
        }

        mem_write_u32(mem, bufused_ptr, res as u32);

        let result_sp = sp.add(6).sub(1);
        *result_sp = ERRNO_SUCCESS as u128;
        result_sp
    }
}

/// poll_oneoff(in_ptr, out_ptr, nsubscriptions, nevents_ptr) -> errno
#[no_mangle]
pub extern "C" fn wasi_poll_oneoff(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let nevents_ptr = (*sp.add(0)) as u32;
        let nsubs       = (*sp.add(1)) as u32;
        let out_ptr     = (*sp.add(2)) as u32;
        let in_ptr      = (*sp.add(3)) as u32;
        let mem = ctx.memory_base;

        // Each subscription is 48 bytes, each event is 32 bytes
        // For now, handle the simple case: clock subscriptions (sleep)
        // Parse subscriptions to find clock timeouts
        let mut min_timeout_ns: u64 = u64::MAX;
        let mut found_clock = false;

        for i in 0..nsubs {
            let sub_base = mem.add((in_ptr + i * 48) as usize);
            let sub_type = *sub_base.add(8); // offset 8 is the type tag

            if sub_type == 0 {
                // Clock subscription
                let timeout = core::ptr::read_unaligned(sub_base.add(16) as *const u64);
                let _precision = core::ptr::read_unaligned(sub_base.add(24) as *const u64);
                let flags = core::ptr::read_unaligned(sub_base.add(32) as *const u16);

                if flags & 1 == 0 {
                    // Relative timeout
                    if timeout < min_timeout_ns {
                        min_timeout_ns = timeout;
                    }
                }
                found_clock = true;
            }
        }

        if found_clock && min_timeout_ns < u64::MAX {
            // Sleep for the timeout duration (convert ns to ms for nanosleep-like syscall)
            let ms = min_timeout_ns / 1_000_000;
            if ms > 0 {
                // Use SYS_YIELD in a loop to approximate sleep
                // TODO: proper nanosleep syscall
                let start_ticks = syscall1(109, 0); // SYS_GET_TICKS
                let target_ticks = start_ticks + ms;
                loop {
                    syscall1(SYS_YIELD, 0);
                    let now = syscall1(109, 0);
                    if now >= target_ticks { break; }
                }
            }
        }

        // Write events: mark all subscriptions as ready
        for i in 0..nsubs {
            let sub_base = mem.add((in_ptr + i * 48) as usize);
            let userdata = core::ptr::read_unaligned(sub_base as *const u64);

            let evt_base = mem.add((out_ptr + i * 32) as usize);
            core::ptr::write_unaligned(evt_base as *mut u64, userdata);
            core::ptr::write_unaligned(evt_base.add(8) as *mut u16, 0); // errno = success
            *evt_base.add(10) = *sub_base.add(8); // type
            // pad remaining bytes
            for j in 11..32 {
                *evt_base.add(j) = 0;
            }
        }

        mem_write_u32(mem, nevents_ptr, nsubs);

        let result_sp = sp.add(4).sub(1);
        *result_sp = ERRNO_SUCCESS as u128;
        result_sp
    }
}

/// sched_yield() -> errno
#[no_mangle]
pub extern "C" fn wasi_sched_yield(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        syscall1(SYS_YIELD, 0);
        let result_sp = sp.sub(1);
        *result_sp = ERRNO_SUCCESS as u128;
        result_sp
    }
}

/// clock_res_get(id, resolution_ptr) -> errno
#[no_mangle]
pub extern "C" fn wasi_clock_res_get(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let res_ptr = (*sp.add(0)) as u32;
        let _id     = (*sp.add(1)) as u32;
        let mem = ctx.memory_base;

        // 1ms resolution
        mem_write_u64(mem, res_ptr, 1_000_000);

        let result_sp = sp.add(2).sub(1);
        *result_sp = ERRNO_SUCCESS as u128;
        result_sp
    }
}

// =============================================================================
// Dispatch
// =============================================================================

/// Main dispatch: looks up import_stub_table and calls the corresponding ring3-rt stub.
/// No more u64::MAX fallback to SYS_WASM_HOST_CALL.
#[no_mangle]
pub extern "C" fn call_host_dispatch(ctx: &mut Ring3Context, sp: *mut u128, idx: u64) -> *mut u128 {
    if idx >= ctx.num_imported_funcs as u64 {
        crate::traps::trap_host(ctx, sp);
    }

    unsafe {
        let stub_idx = *ctx.import_stub_table.add(idx as usize);

        if stub_idx == u64::MAX {
            // Unknown host function — trap immediately.
            // We can't safely return because we don't know the expected stack adjustment.
            crate::traps::trap_host(ctx, sp);
        }

        let blob_base = ctx.blob_base;
        let jump_table = blob_base as *const u64;
        let stub_addr = *jump_table.add(stub_idx as usize);

        if stub_addr == 0 {
            crate::traps::trap_host(ctx, sp);
        }

        let stub_fn: unsafe extern "C" fn(&mut Ring3Context, *mut u128) -> *mut u128 = core::mem::transmute(stub_addr);
        stub_fn(ctx, sp)
    }
}

// =============================================================================
// Misc stubs
// =============================================================================

#[no_mangle]
pub extern "C" fn wasi_serial_print(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let len = (*sp.add(0)) as usize;
        let ptr = (*sp.add(1)) as u32;

        let mem = ctx.memory_base;
        let buf_ptr = mem.add(ptr as usize);

        syscall3(SYS_DEBUG_PRINT, buf_ptr as u64, len as u64, 0);

        let result_sp = sp.add(2);
        result_sp
    }
}
