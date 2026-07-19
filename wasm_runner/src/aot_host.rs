//! WASI preview 1 host bridge for the AOT engine.
//!
//! The AOT compiler generates one import thunk per imported function; each
//! calls back here through `AotContext::host_dispatch`. This dispatcher reads
//! the guest's arguments from the wasm operand stack, accesses guest linear
//! memory through `ctx.mem_base`, performs the syscall, and writes the result
//! back — mirroring the wasmi host functions in main.rs.

use alloc::vec::Vec;
use wasmaot::aot::AotContext;

use crate::{sys_fstat, sys_open, sys_read, sys_write, time_ns};

/// The subset of WASI p1 (+ a couple of `env` shims) our apps import.
#[derive(Clone, Copy, PartialEq)]
pub enum WasiFn {
    FdWrite,
    FdRead,
    FdSeek,
    FdClose,
    FdFdstatGet,
    FdFilestatGet,
    FdPrestatGet,
    FdPrestatDirName,
    PathFilestatGet,
    PathOpen,
    ClockTimeGet,
    RandomGet,
    EnvironSizesGet,
    EnvironGet,
    ArgsSizesGet,
    ArgsGet,
    ProcExit,
    PollOneoff,
    Noop, // __wasi_init_tp, __wasm_call_dtors
    Unknown,
}

pub fn resolve(module: &str, name: &str) -> WasiFn {
    match (module, name) {
        ("wasi_snapshot_preview1", "fd_write") => WasiFn::FdWrite,
        ("wasi_snapshot_preview1", "fd_read") => WasiFn::FdRead,
        ("wasi_snapshot_preview1", "fd_seek") => WasiFn::FdSeek,
        ("wasi_snapshot_preview1", "fd_close") => WasiFn::FdClose,
        ("wasi_snapshot_preview1", "fd_fdstat_get") => WasiFn::FdFdstatGet,
        ("wasi_snapshot_preview1", "fd_filestat_get") => WasiFn::FdFilestatGet,
        ("wasi_snapshot_preview1", "fd_prestat_get") => WasiFn::FdPrestatGet,
        ("wasi_snapshot_preview1", "fd_prestat_dir_name") => WasiFn::FdPrestatDirName,
        ("wasi_snapshot_preview1", "path_filestat_get") => WasiFn::PathFilestatGet,
        ("wasi_snapshot_preview1", "path_open") => WasiFn::PathOpen,
        ("wasi_snapshot_preview1", "clock_time_get") => WasiFn::ClockTimeGet,
        ("wasi_snapshot_preview1", "random_get") => WasiFn::RandomGet,
        ("wasi_snapshot_preview1", "environ_sizes_get") => WasiFn::EnvironSizesGet,
        ("wasi_snapshot_preview1", "environ_get") => WasiFn::EnvironGet,
        ("wasi_snapshot_preview1", "args_sizes_get") => WasiFn::ArgsSizesGet,
        ("wasi_snapshot_preview1", "args_get") => WasiFn::ArgsGet,
        ("wasi_snapshot_preview1", "proc_exit") => WasiFn::ProcExit,
        ("wasi_snapshot_preview1", "poll_oneoff") => WasiFn::PollOneoff,
        ("env", "__wasi_init_tp") | ("env", "__wasm_call_dtors") => WasiFn::Noop,
        ("env", "__wasi_proc_exit") => WasiFn::ProcExit,
        _ => WasiFn::Unknown,
    }
}

/// Runner-side state referenced by the dispatcher via `ctx.user_data`.
pub struct AotHostState {
    pub imports: Vec<WasiFn>,
    pub fd_offsets: [usize; MAX_TRACKED_FD],
}

pub const MAX_TRACKED_FD: usize = 4096;

/// Trap code (>= 100) meaning "the guest called proc_exit"; carries the code.
pub const TRAP_PROC_EXIT_BASE: u32 = 100;

impl AotHostState {
    pub fn new() -> Self {
        AotHostState {
            imports: Vec::new(),
            fd_offsets: [0; MAX_TRACKED_FD],
        }
    }
}

// ── small guest-memory view ────────────────────────────────────────
struct Mem {
    base: *mut u8,
    len: usize,
}

impl Mem {
    #[inline]
    fn ok(&self, ptr: usize, len: usize) -> bool {
        ptr.checked_add(len).map_or(false, |e| e <= self.len)
    }
    #[inline]
    fn read_u32(&self, ptr: usize) -> Option<u32> {
        if !self.ok(ptr, 4) {
            return None;
        }
        let mut b = [0u8; 4];
        unsafe { core::ptr::copy_nonoverlapping(self.base.add(ptr), b.as_mut_ptr(), 4) };
        Some(u32::from_le_bytes(b))
    }
    #[inline]
    fn write_u32(&self, ptr: usize, v: u32) -> bool {
        if !self.ok(ptr, 4) {
            return false;
        }
        let b = v.to_le_bytes();
        unsafe { core::ptr::copy_nonoverlapping(b.as_ptr(), self.base.add(ptr), 4) };
        true
    }
    #[inline]
    fn write_u64(&self, ptr: usize, v: u64) -> bool {
        if !self.ok(ptr, 8) {
            return false;
        }
        let b = v.to_le_bytes();
        unsafe { core::ptr::copy_nonoverlapping(b.as_ptr(), self.base.add(ptr), 8) };
        true
    }
    #[inline]
    fn slice(&self, ptr: usize, len: usize) -> Option<&mut [u8]> {
        if !self.ok(ptr, len) {
            return None;
        }
        Some(unsafe { core::slice::from_raw_parts_mut(self.base.add(ptr), len) })
    }
}

#[inline]
unsafe fn arg(wsp: *mut u64, n: usize, i: usize) -> u64 {
    *wsp.add(n - 1 - i)
}

#[inline]
unsafe fn set_ret(wsp: *mut u64, n: usize, v: u32) {
    *wsp.add(n - 1) = v as u64;
}

const ERRNO_OK: u32 = 0;
const ERRNO_BADF: u32 = 8;
const ERRNO_INVAL: u32 = 28;
const ERRNO_FAULT: u32 = 21;
const ERRNO_NOENT: u32 = 44;

/// The `host_dispatch` callback wired into every AOT module.
pub extern "C" fn dispatch(ctx: *mut AotContext, import_idx: u64, wsp: *mut u64) -> u64 {
    let ctxr = unsafe { &mut *ctx };
    if ctxr.user_data == 0 {
        return 1; // no host state — trap
    }
    let state = unsafe { &mut *(ctxr.user_data as *mut AotHostState) };
    let f = state
        .imports
        .get(import_idx as usize)
        .copied()
        .unwrap_or(WasiFn::Unknown);
    let mem = Mem {
        base: ctxr.mem_base,
        len: ctxr.mem_size as usize,
    };

    unsafe {
        match f {
            WasiFn::Noop => {}

            WasiFn::ProcExit => {
                let code = arg(wsp, 1, 0) as u32;
                ctxr.trap_code = TRAP_PROC_EXIT_BASE + (code & 0xFF);
                return 1; // trap-unwind out of wasm
            }

            WasiFn::FdWrite => {
                let n = 4;
                let fd = arg(wsp, n, 0) as usize;
                let iovs_ptr = arg(wsp, n, 1) as usize;
                let iovs_len = arg(wsp, n, 2) as usize;
                let nwritten_ptr = arg(wsp, n, 3) as usize;
                let mut total = 0usize;
                for i in 0..iovs_len {
                    let iov = iovs_ptr + i * 8;
                    let (Some(buf_ptr), Some(buf_len)) =
                        (mem.read_u32(iov), mem.read_u32(iov + 4))
                    else {
                        set_ret(wsp, n, ERRNO_FAULT);
                        return 0;
                    };
                    let Some(data) = mem.slice(buf_ptr as usize, buf_len as usize) else {
                        set_ret(wsp, n, ERRNO_FAULT);
                        return 0;
                    };
                    let offset = fd_offset(state, fd);
                    let bw = sys_write(fd, offset, data);
                    total += bw;
                    set_fd_offset(state, fd, offset + bw);
                    if bw < buf_len as usize {
                        break;
                    }
                }
                mem.write_u32(nwritten_ptr, total as u32);
                set_ret(wsp, n, ERRNO_OK);
            }

            WasiFn::FdRead => {
                let n = 4;
                let fd = arg(wsp, n, 0) as usize;
                let iovs_ptr = arg(wsp, n, 1) as usize;
                let iovs_len = arg(wsp, n, 2) as usize;
                let nread_ptr = arg(wsp, n, 3) as usize;
                let mut total = 0usize;
                for i in 0..iovs_len {
                    let iov = iovs_ptr + i * 8;
                    let (Some(buf_ptr), Some(buf_len)) =
                        (mem.read_u32(iov), mem.read_u32(iov + 4))
                    else {
                        set_ret(wsp, n, ERRNO_FAULT);
                        return 0;
                    };
                    let Some(data) = mem.slice(buf_ptr as usize, buf_len as usize) else {
                        set_ret(wsp, n, ERRNO_FAULT);
                        return 0;
                    };
                    let offset = fd_offset(state, fd);
                    let br = sys_read(fd, offset, data);
                    if br > 0 {
                        total += br;
                        set_fd_offset(state, fd, offset + br);
                    }
                    if br < buf_len as usize {
                        break;
                    }
                }
                mem.write_u32(nread_ptr, total as u32);
                set_ret(wsp, n, ERRNO_OK);
            }

            WasiFn::FdSeek => {
                let n = 4;
                let fd = arg(wsp, n, 0) as usize;
                let offset = arg(wsp, n, 1) as i64;
                let whence = arg(wsp, n, 2) as u32;
                let newoff_ptr = arg(wsp, n, 3) as usize;
                if fd >= MAX_TRACKED_FD {
                    set_ret(wsp, n, ERRNO_BADF);
                    return 0;
                }
                let cur = state.fd_offsets[fd] as i64;
                let new = match whence {
                    0 => offset,                       // SET
                    1 => cur + offset,                 // CUR
                    _ => sys_fstat(fd) as i64 + offset, // END
                };
                state.fd_offsets[fd] = new.max(0) as usize;
                mem.write_u64(newoff_ptr, new.max(0) as u64);
                set_ret(wsp, n, ERRNO_OK);
            }

            WasiFn::FdClose => {
                let n = 1;
                let fd = arg(wsp, n, 0) as usize;
                if fd < MAX_TRACKED_FD {
                    state.fd_offsets[fd] = 0;
                }
                set_ret(wsp, n, ERRNO_OK);
            }

            WasiFn::FdFdstatGet => {
                let n = 2;
                set_ret(wsp, n, ERRNO_OK);
            }

            WasiFn::FdFilestatGet => {
                let n = 2;
                let fd = arg(wsp, n, 0) as usize;
                let buf_ptr = arg(wsp, n, 1) as usize;
                let size = sys_fstat(fd) as u64;
                if let Some(stat) = mem.slice(buf_ptr, 64) {
                    stat.fill(0);
                    stat[16] = 4; // regular file
                    stat[24] = 1; // nlink
                    stat[32..40].copy_from_slice(&size.to_le_bytes());
                    set_ret(wsp, n, ERRNO_OK);
                } else {
                    set_ret(wsp, n, ERRNO_FAULT);
                }
            }

            WasiFn::FdPrestatGet => {
                let n = 2;
                let fd = arg(wsp, n, 0) as u32;
                let buf_ptr = arg(wsp, n, 1) as usize;
                if fd == 3 {
                    if let Some(p) = mem.slice(buf_ptr, 8) {
                        p.fill(0);
                        p[0] = 0; // tag = dir
                        p[4] = 1; // name_len = 1 ("/")
                        set_ret(wsp, n, ERRNO_OK);
                    } else {
                        set_ret(wsp, n, ERRNO_FAULT);
                    }
                } else {
                    set_ret(wsp, n, ERRNO_BADF);
                }
            }

            WasiFn::FdPrestatDirName => {
                let n = 3;
                let fd = arg(wsp, n, 0) as u32;
                let path_ptr = arg(wsp, n, 1) as usize;
                let path_len = arg(wsp, n, 2) as usize;
                if fd == 3 && path_len > 0 {
                    if let Some(p) = mem.slice(path_ptr, 1) {
                        p[0] = b'/';
                        set_ret(wsp, n, ERRNO_OK);
                    } else {
                        set_ret(wsp, n, ERRNO_FAULT);
                    }
                } else {
                    set_ret(wsp, n, ERRNO_INVAL);
                }
            }

            WasiFn::PathFilestatGet => {
                let n = 5;
                set_ret(wsp, n, ERRNO_BADF);
            }

            WasiFn::PathOpen => {
                let n = 9;
                let path_ptr = arg(wsp, n, 2) as usize;
                let path_len = arg(wsp, n, 3) as usize;
                let oflags = arg(wsp, n, 4) as usize;
                let fd_out_ptr = arg(wsp, n, 8) as usize;
                let Some(pbuf) = mem.slice(path_ptr, path_len) else {
                    set_ret(wsp, n, ERRNO_FAULT);
                    return 0;
                };
                if let Ok(path) = core::str::from_utf8(pbuf) {
                    let fd = sys_open(path, oflags);
                    if fd != usize::MAX {
                        mem.write_u32(fd_out_ptr, fd as u32);
                        set_ret(wsp, n, ERRNO_OK);
                    } else {
                        set_ret(wsp, n, ERRNO_NOENT);
                    }
                } else {
                    set_ret(wsp, n, ERRNO_INVAL);
                }
            }

            WasiFn::ClockTimeGet => {
                let n = 3;
                let time_ptr = arg(wsp, n, 2) as usize;
                mem.write_u64(time_ptr, time_ns());
                set_ret(wsp, n, ERRNO_OK);
            }

            WasiFn::RandomGet => {
                let n = 2;
                let buf_ptr = arg(wsp, n, 0) as usize;
                let buf_len = arg(wsp, n, 1) as usize;
                if let Some(b) = mem.slice(buf_ptr, buf_len) {
                    b.fill(0);
                    set_ret(wsp, n, ERRNO_OK);
                } else {
                    set_ret(wsp, n, ERRNO_FAULT);
                }
            }

            WasiFn::EnvironSizesGet | WasiFn::ArgsSizesGet => {
                let n = 2;
                let count_ptr = arg(wsp, n, 0) as usize;
                let size_ptr = arg(wsp, n, 1) as usize;
                mem.write_u32(count_ptr, 0);
                mem.write_u32(size_ptr, 0);
                set_ret(wsp, n, ERRNO_OK);
            }

            WasiFn::EnvironGet | WasiFn::ArgsGet => {
                let n = 2;
                set_ret(wsp, n, ERRNO_OK);
            }

            WasiFn::PollOneoff => {
                let n = 4;
                let in_ptr = arg(wsp, n, 0) as usize;
                let out_ptr = arg(wsp, n, 1) as usize;
                let nsubs = arg(wsp, n, 2) as usize;
                let ret_ptr = arg(wsp, n, 3) as usize;
                if nsubs == 0 {
                    set_ret(wsp, n, ERRNO_INVAL);
                    return 0;
                }
                // Read the first subscription; support clock (relative sleep).
                let Some(sub) = mem.slice(in_ptr, 48) else {
                    set_ret(wsp, n, ERRNO_FAULT);
                    return 0;
                };
                let sub_copy = {
                    let mut b = [0u8; 48];
                    b.copy_from_slice(sub);
                    b
                };
                let sub_type = sub_copy[8];
                if sub_type == 0 {
                    let timeout_ns = u64::from_le_bytes([
                        sub_copy[24], sub_copy[25], sub_copy[26], sub_copy[27],
                        sub_copy[28], sub_copy[29], sub_copy[30], sub_copy[31],
                    ]);
                    crate::sys_sleep((timeout_ns / 1_000_000) as usize);
                    if let Some(ev) = mem.slice(out_ptr, 32) {
                        ev.fill(0);
                        ev[0..8].copy_from_slice(&sub_copy[0..8]); // userdata
                    }
                    mem.write_u32(ret_ptr, 1);
                    set_ret(wsp, n, ERRNO_OK);
                } else {
                    set_ret(wsp, n, 58); // ENOTSUP
                }
            }

            WasiFn::Unknown => {
                // Unknown import: fail the call. Assume 1 result slot.
                *wsp = 0;
                return 1;
            }
        }
    }
    0
}

#[inline]
fn fd_offset(state: &AotHostState, fd: usize) -> usize {
    if fd < MAX_TRACKED_FD {
        state.fd_offsets[fd]
    } else {
        0
    }
}

#[inline]
fn set_fd_offset(state: &mut AotHostState, fd: usize, v: usize) {
    if fd < MAX_TRACKED_FD {
        state.fd_offsets[fd] = v;
    }
}
