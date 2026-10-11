// WASI preview 1 host bridge for the AOT engine: every import thunk calls
// dispatch, which takes args from the wasm operand stack (mirrors main.rs).

use alloc::vec::Vec;
use wasmaot::aot::AotContext;

use crate::{sys_close, sys_fsop, sys_fstat, sys_kill, sys_open, sys_read, sys_write, time_ns, wasi_filestat, wasi_readdir, KILLED_STATUS};
use crate::{FSOP_MKDIR, FSOP_RENAME, FSOP_RMDIR, FSOP_STAT, FSOP_UNLINK};

// The subset of WASI p1 (plus a couple of env shims) our apps import.
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
    PathUnlinkFile,
    PathRemoveDirectory,
    PathCreateDirectory,
    PathRename,
    ClockTimeGet,
    RandomGet,
    EnvironSizesGet,
    EnvironGet,
    ArgsSizesGet,
    ArgsGet,
    ProcExit,
    PollOneoff,
    FdReaddir,
    FdTell,
    FdPread,
    FdPwrite,
    PathReadlink,
    // Calls that only need to succeed (sync, advice, flags, timestamps).
    Ok(u8),
    // Calls KrakeOS can't do (links, sockets); fail with NOTSUP.
    NotSup(u8),
    KrakeKill,
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
        ("wasi_snapshot_preview1", "path_unlink_file") => WasiFn::PathUnlinkFile,
        ("wasi_snapshot_preview1", "path_remove_directory") => WasiFn::PathRemoveDirectory,
        ("wasi_snapshot_preview1", "path_create_directory") => WasiFn::PathCreateDirectory,
        ("wasi_snapshot_preview1", "path_rename") => WasiFn::PathRename,
        ("wasi_snapshot_preview1", "clock_time_get") => WasiFn::ClockTimeGet,
        ("wasi_snapshot_preview1", "random_get") => WasiFn::RandomGet,
        ("wasi_snapshot_preview1", "environ_sizes_get") => WasiFn::EnvironSizesGet,
        ("wasi_snapshot_preview1", "environ_get") => WasiFn::EnvironGet,
        ("wasi_snapshot_preview1", "args_sizes_get") => WasiFn::ArgsSizesGet,
        ("wasi_snapshot_preview1", "args_get") => WasiFn::ArgsGet,
        ("wasi_snapshot_preview1", "proc_exit") => WasiFn::ProcExit,
        ("wasi_snapshot_preview1", "poll_oneoff") => WasiFn::PollOneoff,
        ("wasi_snapshot_preview1", "fd_readdir") => WasiFn::FdReaddir,
        ("wasi_snapshot_preview1", "fd_tell") => WasiFn::FdTell,
        ("wasi_snapshot_preview1", "fd_pread") => WasiFn::FdPread,
        ("wasi_snapshot_preview1", "fd_pwrite") => WasiFn::FdPwrite,
        ("wasi_snapshot_preview1", "path_readlink") => WasiFn::PathReadlink,
        ("wasi_snapshot_preview1", "sched_yield") => WasiFn::Ok(0),
        ("wasi_snapshot_preview1", "fd_datasync" | "fd_sync") => WasiFn::Ok(1),
        ("wasi_snapshot_preview1", "fd_fdstat_set_flags") => WasiFn::Ok(2),
        ("wasi_snapshot_preview1", "fd_advise" | "fd_filestat_set_times") => WasiFn::Ok(4),
        ("wasi_snapshot_preview1", "path_filestat_set_times") => WasiFn::Ok(7),
        ("wasi_snapshot_preview1", "fd_filestat_set_size") => WasiFn::NotSup(2),
        ("wasi_snapshot_preview1", "sock_shutdown") => WasiFn::NotSup(2),
        ("wasi_snapshot_preview1", "sock_accept") => WasiFn::NotSup(3),
        ("wasi_snapshot_preview1", "path_symlink") => WasiFn::NotSup(5),
        ("wasi_snapshot_preview1", "sock_send") => WasiFn::NotSup(5),
        ("wasi_snapshot_preview1", "sock_recv") => WasiFn::NotSup(6),
        ("wasi_snapshot_preview1", "path_link") => WasiFn::NotSup(7),
        ("krake", "kill") => WasiFn::KrakeKill,
        ("env", "__wasi_init_tp") | ("env", "__wasm_call_dtors") => WasiFn::Noop,
        ("env", "__wasi_proc_exit") => WasiFn::ProcExit,
        _ => WasiFn::Unknown,
    }
}

// Runner-side state the dispatcher reaches via ctx.user_data.
pub struct AotHostState {
    pub imports: Vec<WasiFn>,
    pub fd_offsets: [usize; MAX_TRACKED_FD],
    pub args: crate::Args,
    pub env: crate::Args,
    // fds path_open opened on a directory.
    pub dir_fds: Vec<usize>,
    rng: u64,
}

// Environment every app sees. The terminal is no ANSI terminal, so ask
// programs (Python) for plain output and the basic line-mode prompt.
const ENV: &[&str] = &["HOME=/home", "TERM=dumb", "PYTHON_COLORS=0", "PYTHON_BASIC_REPL=1"];

pub const MAX_TRACKED_FD: usize = 4096;

// Trap code (>= 100) meaning "the guest called proc_exit"; carries the code.
pub const TRAP_PROC_EXIT_BASE: u32 = 100;

impl AotHostState {
    pub fn new() -> Self {
        AotHostState {
            imports: Vec::new(),
            fd_offsets: [0; MAX_TRACKED_FD],
            args: crate::Args::default(),
            env: crate::Args::new(ENV),
            dir_fds: Vec::new(),
            rng: crate::time_ns() | 1,
        }
    }
}

// Bounds-checked view of guest linear memory.
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

// Result of an import with n params: it replaces the first param, or is
// pushed below the stack top for a call without params.
#[inline]
unsafe fn set_ret(wsp: *mut u64, n: usize, v: u32) {
    *wsp.offset(n as isize - 1) = v as u64;
}

const ERRNO_NOTSUP: u32 = 58;
const FILETYPE_CHAR_DEVICE: u8 = 2;
const FILETYPE_DIRECTORY: u8 = 3;
const FILETYPE_REGULAR_FILE: u8 = 4;
// The preopened root directory.
const PREOPEN_FD: usize = 3;
const RIGHT_FD_SEEK: u64 = 1 << 2;
const RIGHT_FD_TELL: u64 = 1 << 5;

impl AotHostState {
    fn file_type(&self, fd: usize) -> u8 {
        if fd <= 2 {
            // stdio is a terminal, so isatty holds and Python gives a prompt
            FILETYPE_CHAR_DEVICE
        } else if fd == PREOPEN_FD || self.dir_fds.contains(&fd) {
            FILETYPE_DIRECTORY
        } else {
            FILETYPE_REGULAR_FILE
        }
    }

    // xorshift64 seeded from the clock; enough for hash seeds and random.
    fn next_random(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }
}

const ERRNO_OK: u32 = 0;
const ERRNO_BADF: u32 = 8;
const ERRNO_INVAL: u32 = 28;
const ERRNO_FAULT: u32 = 21;
const ERRNO_NOENT: u32 = 44;
const ERRNO_NOTDIR: u32 = 54;

fn wasi_exists(path: &str) -> bool {
    sys_fsop(FSOP_STAT, path, "") != usize::MAX
}

// The host_dispatch callback wired into every AOT module.
pub extern "C" fn dispatch(ctx: *mut AotContext, import_idx: u64, wsp: *mut u64) -> u64 {
    let ctxr = unsafe { &mut *ctx };
    if ctxr.user_data == 0 {
        return 1; // no host state, trap
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
                    let mut br = sys_read(fd, offset, data);
                    // stdin from the terminal: wait until a line is typed
                    while fd == 0 && br == crate::WOULD_BLOCK {
                        if crate::sys_sleep(15) != 0 {
                            ctxr.trap_code = TRAP_PROC_EXIT_BASE + KILLED_STATUS;
                            return 1;
                        }
                        br = sys_read(fd, offset, data);
                    }
                    if br == crate::WOULD_BLOCK || br == usize::MAX {
                        br = 0;
                    }
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
                if fd > 2 {
                    sys_close(fd);
                }
                set_ret(wsp, n, ERRNO_OK);
            }

            WasiFn::FdFdstatGet => {
                let n = 2;
                let fd = arg(wsp, n, 0) as u32 as usize;
                let buf_ptr = arg(wsp, n, 1) as u32 as usize;
                let ftype = state.file_type(fd);
                let rights = if ftype == FILETYPE_CHAR_DEVICE {
                    u64::MAX & !(RIGHT_FD_SEEK | RIGHT_FD_TELL)
                } else {
                    u64::MAX
                };
                if let Some(b) = mem.slice(buf_ptr, 24) {
                    b.fill(0);
                    b[0] = ftype;
                    b[8..16].copy_from_slice(&rights.to_le_bytes());
                    b[16..24].copy_from_slice(&u64::MAX.to_le_bytes());
                    set_ret(wsp, n, ERRNO_OK);
                } else {
                    set_ret(wsp, n, ERRNO_FAULT);
                }
            }

            WasiFn::FdTell => {
                let n = 2;
                let fd = arg(wsp, n, 0) as u32 as usize;
                let off = fd_offset(state, fd) as u64;
                let ok = mem.write_u64(arg(wsp, n, 1) as u32 as usize, off);
                set_ret(wsp, n, if ok { ERRNO_OK } else { ERRNO_FAULT });
            }

            WasiFn::FdPread | WasiFn::FdPwrite => {
                let n = 5;
                let fd = arg(wsp, n, 0) as u32 as usize;
                let iovs_ptr = arg(wsp, n, 1) as u32 as usize;
                let iovs_len = arg(wsp, n, 2) as u32 as usize;
                let mut offset = arg(wsp, n, 3) as usize;
                let done_ptr = arg(wsp, n, 4) as u32 as usize;
                let mut total = 0usize;
                for i in 0..iovs_len {
                    let iov = iovs_ptr + i * 8;
                    let (Some(buf_ptr), Some(buf_len)) = (mem.read_u32(iov), mem.read_u32(iov + 4)) else {
                        set_ret(wsp, n, ERRNO_FAULT);
                        return 0;
                    };
                    let Some(data) = mem.slice(buf_ptr as usize, buf_len as usize) else {
                        set_ret(wsp, n, ERRNO_FAULT);
                        return 0;
                    };
                    let r = if f == WasiFn::FdPread { sys_read(fd, offset, data) } else { sys_write(fd, offset, data) };
                    if r == usize::MAX || r == crate::WOULD_BLOCK {
                        break;
                    }
                    total += r;
                    offset += r;
                    if r < buf_len as usize {
                        break;
                    }
                }
                mem.write_u32(done_ptr, total as u32);
                set_ret(wsp, n, ERRNO_OK);
            }

            WasiFn::PathReadlink => {
                // No symlinks on FatSquid.
                set_ret(wsp, 6, ERRNO_INVAL);
            }

            WasiFn::Ok(n) => set_ret(wsp, n as usize, ERRNO_OK),
            WasiFn::NotSup(n) => set_ret(wsp, n as usize, ERRNO_NOTSUP),

            WasiFn::FdFilestatGet => {
                let n = 2;
                let fd = arg(wsp, n, 0) as usize;
                let buf_ptr = arg(wsp, n, 1) as usize;
                let size = sys_fstat(fd) as u64;
                if let Some(stat) = mem.slice(buf_ptr, 64) {
                    stat.fill(0);
                    stat[16] = state.file_type(fd);
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
                let path = guest_str(&mem, arg(wsp, n, 2) as usize, arg(wsp, n, 3) as usize);
                let buf_ptr = arg(wsp, n, 4) as usize;
                let ret = match path.map(|p| wasi_filestat(&p)) {
                    None => ERRNO_FAULT,
                    Some(Err(e)) => e,
                    Some(Ok(stat)) => match mem.slice(buf_ptr, 64) {
                        Some(b) => {
                            b.copy_from_slice(&stat);
                            ERRNO_OK
                        }
                        None => ERRNO_FAULT,
                    },
                };
                set_ret(wsp, n, ret);
            }

            WasiFn::PathUnlinkFile | WasiFn::PathRemoveDirectory | WasiFn::PathCreateDirectory => {
                let n = 3;
                let op = match f {
                    WasiFn::PathUnlinkFile => FSOP_UNLINK,
                    WasiFn::PathRemoveDirectory => FSOP_RMDIR,
                    _ => FSOP_MKDIR,
                };
                let ret = match guest_str(&mem, arg(wsp, n, 1) as usize, arg(wsp, n, 2) as usize) {
                    Some(p) => sys_fsop(op, &p, "") as u32,
                    None => ERRNO_FAULT,
                };
                set_ret(wsp, n, ret);
            }

            WasiFn::PathRename => {
                let n = 6;
                let a = guest_str(&mem, arg(wsp, n, 1) as usize, arg(wsp, n, 2) as usize);
                let b = guest_str(&mem, arg(wsp, n, 4) as usize, arg(wsp, n, 5) as usize);
                let ret = match (a, b) {
                    (Some(a), Some(b)) => sys_fsop(FSOP_RENAME, &a, &b) as u32,
                    _ => ERRNO_FAULT,
                };
                set_ret(wsp, n, ret);
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
                    let is_dir = sys_fsop(FSOP_STAT, path, "");
                    let root = matches!(path.trim_matches('/'), "" | ".");
                    let is_dir = root || (is_dir != usize::MAX && is_dir >> 32 & 1 != 0);
                    const O_DIRECTORY: usize = 2;
                    if oflags & O_DIRECTORY != 0 && !is_dir {
                        set_ret(wsp, n, if wasi_exists(path) { ERRNO_NOTDIR } else { ERRNO_NOENT });
                        return 0;
                    }
                    let fd = sys_open(path, oflags);
                    if fd != usize::MAX {
                        state.dir_fds.retain(|&d| d != fd);
                        if is_dir {
                            state.dir_fds.push(fd);
                        }
                        if (fd as usize) < MAX_TRACKED_FD {
                            state.fd_offsets[fd] = 0;
                        }
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
                    for chunk in b.chunks_mut(8) {
                        let r = state.next_random().to_le_bytes();
                        chunk.copy_from_slice(&r[..chunk.len()]);
                    }
                    set_ret(wsp, n, ERRNO_OK);
                } else {
                    set_ret(wsp, n, ERRNO_FAULT);
                }
            }

            WasiFn::EnvironSizesGet | WasiFn::ArgsSizesGet => {
                let n = 2;
                let count_ptr = arg(wsp, n, 0) as usize;
                let size_ptr = arg(wsp, n, 1) as usize;
                let list = if f == WasiFn::ArgsSizesGet { &state.args } else { &state.env };
                mem.write_u32(count_ptr, list.offsets.len() as u32);
                mem.write_u32(size_ptr, list.blob.len() as u32);
                set_ret(wsp, n, ERRNO_OK);
            }

            WasiFn::ArgsGet | WasiFn::EnvironGet => {
                let n = 2;
                let argv_ptr = arg(wsp, n, 0) as u32 as usize;
                let buf_ptr = arg(wsp, n, 1) as u32 as usize;
                let list = if f == WasiFn::ArgsGet { &state.args } else { &state.env };
                let blob = &list.blob;
                let mut ok = match mem.slice(buf_ptr, blob.len()) {
                    Some(dst) => {
                        dst.copy_from_slice(blob);
                        true
                    }
                    None => false,
                };
                for (i, off) in list.offsets.iter().enumerate() {
                    ok &= mem.write_u32(argv_ptr + i * 4, buf_ptr as u32 + off);
                }
                set_ret(wsp, n, if ok { ERRNO_OK } else { ERRNO_FAULT });
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
                    let mut timeout_ns = u64::from_le_bytes([
                        sub_copy[24], sub_copy[25], sub_copy[26], sub_copy[27],
                        sub_copy[28], sub_copy[29], sub_copy[30], sub_copy[31],
                    ]);
                    // SUBSCRIPTION_CLOCK_ABSTIME: a deadline on the clock
                    // clock_time_get reports (Python's time.sleep uses it).
                    if sub_copy[40] & 1 != 0 {
                        timeout_ns = timeout_ns.saturating_sub(time_ns());
                    }
                    if crate::sys_sleep((timeout_ns / 1_000_000) as usize) != 0 {
                        // Killed: unwind out of wasm like proc_exit.
                        ctxr.trap_code = TRAP_PROC_EXIT_BASE + KILLED_STATUS;
                        return 1;
                    }
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

            WasiFn::FdReaddir => {
                let n = 5;
                let fd = arg(wsp, n, 0) as u32 as usize;
                let buf_ptr = arg(wsp, n, 1) as u32 as usize;
                let buf_len = arg(wsp, n, 2) as u32 as usize;
                let cookie = arg(wsp, n, 3);
                let bufused_ptr = arg(wsp, n, 4) as u32 as usize;
                let Some(out) = mem.slice(buf_ptr, buf_len) else {
                    set_ret(wsp, n, ERRNO_FAULT);
                    return 0;
                };
                match wasi_readdir(fd, cookie, out) {
                    Ok(used) => {
                        mem.write_u32(bufused_ptr, used as u32);
                        set_ret(wsp, n, ERRNO_OK);
                    }
                    Err(errno) => set_ret(wsp, n, errno),
                }
            }

            WasiFn::KrakeKill => {
                let n = 1;
                let tid = arg(wsp, n, 0) as u32 as usize;
                let r = if sys_kill(tid) == usize::MAX { u32::MAX } else { 0 };
                set_ret(wsp, n, r);
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

fn guest_str(mem: &Mem, ptr: usize, len: usize) -> Option<alloc::string::String> {
    let b = mem.slice(ptr, len)?;
    core::str::from_utf8(b).ok().map(alloc::string::String::from)
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
