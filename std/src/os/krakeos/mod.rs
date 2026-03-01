pub mod graphics;
pub use graphics::*;

pub mod user;
pub use user::*;

pub mod net;

pub mod events;
pub use events::*;

#[cfg(feature = "userland")]
pub mod wasi;

#[cfg(not(target_arch = "wasm32"))]
pub use crate::sys::{syscall, syscall4, syscall5, syscall6};

use crate::sync::Mutex;
use core::task::Waker;
use crate::rust_alloc::collections::BTreeMap;
use crate::rust_alloc::format;
use crate::rust_alloc::string::String;
use crate::rust_alloc::vec::Vec;

// --- Process method_export! bindings ---

method_export!("krakeos:system/process@0.2.0", "spawn",
    pub unsafe fn process_spawn(path_ptr: *const u8, path_len: usize, args_ptr: *const u8, args_len: usize, fds_ptr: *const u8, fds_len: usize) -> u64 {
        crate::sys::syscall6(59,
            path_ptr as u64,
            path_len as u64,
            args_ptr as u64,
            args_len as u64,
            fds_ptr as u64,
            fds_len as u64,
        )
    }
);

method_export!("krakeos:system/process@0.2.0", "waitpid",
    pub unsafe fn process_waitpid(pid: u64) -> i32 {
        crate::sys::syscall(61, pid, 0, 0) as i32
    }
);

method_export!("krakeos:system/process@0.2.0", "pipe",
    pub unsafe fn process_pipe(fds_ptr: *mut u8) -> i32 {
        crate::sys::syscall(22, fds_ptr as u64, 0, 0) as i32
    }
);

method_export!("krakeos:system/memory@0.2.0", "shm-get",
    pub unsafe fn shm_get_raw(name_ptr: *const u8, name_len: usize, size: usize) -> u64 {
        crate::sys::syscall(120, name_ptr as u64, name_len as u64, size as u64)
    }
);

method_export!("wasi:random/random@0.2.0", "get-random-bytes",
    pub unsafe fn get_random_bytes(len: u64, result_ptr: *mut u8) {
        // Pseudo-random for native
        static mut STATE: u64 = 1574;
        if STATE == 1574 {
            STATE = crate::sys::syscall(109, 0, 0, 0).wrapping_add(0xACE1BADE);
        }
        for i in 0..len as usize {
            STATE ^= STATE << 13;
            STATE ^= STATE >> 17;
            STATE ^= STATE << 5;
            *result_ptr.add(i) = (STATE & 0xFF) as u8;
        }
    }
);

method_export!("krakeos:system/process@0.2.0", "ioctl",
    pub unsafe fn process_ioctl(fd: u64, request: u64, arg: u64) -> i32 {
        crate::sys::syscall(16, fd, request, arg) as i32
    }
);

method_export!("krakeos:system/process@0.2.0", "set-nonblock",
    pub unsafe fn process_set_nonblock(fd: u64, nonblock: u64) -> i32 {
        crate::sys::syscall(133, fd, nonblock, 0) as i32
    }
);

method_export!("krakeos:system/process@0.2.0", "get-pid",
    pub unsafe fn process_get_pid() -> u64 {
        crate::sys::syscall(39, 0, 0, 0)
    }
);

// --- Networking method_export! bindings ---

method_export!("krakeos:system/network@0.2.0", "socket-create",
    pub unsafe fn socket_create(family: u64, ty: u64) -> u64 {
        crate::sys::syscall(41, family, ty, 0)
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-connect",
    pub unsafe fn socket_connect(fd: u64, addr_ptr: *const u8, addr_len: u64) -> u64 {
        crate::sys::syscall6(42, fd, addr_ptr as u64, addr_len, 0, 0, 0)
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-finish-connect",
    pub unsafe fn socket_finish_connect(fd: u64) -> u64 {
        crate::sys::syscall6(54, fd, 0, 0, 0, 0, 0)
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-bind",
    pub unsafe fn socket_bind(fd: u64, addr_ptr: *const u8, addr_len: u64) -> u64 {
        crate::sys::syscall6(49, fd, addr_ptr as u64, addr_len, 0, 0, 0)
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-listen",
    pub unsafe fn socket_listen(fd: u64, backlog: u64) -> u64 {
        crate::sys::syscall6(51, fd, backlog, 0, 0, 0, 0)
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-accept",
    pub unsafe fn socket_accept(fd: u64) -> u64 {
        crate::sys::syscall6(43, fd, 0, 0, 0, 0, 0)
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-send",
    pub unsafe fn socket_send(fd: u64, buf_ptr: *const u8, len: u64) -> u64 {
        crate::sys::syscall6(52, fd, buf_ptr as u64, len, 0, 0, 0)
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-recv",
    pub unsafe fn socket_recv(fd: u64, buf_ptr: *mut u8, len: u64) -> u64 {
        crate::sys::syscall6(53, fd, buf_ptr as u64, len, 0, 0, 0)
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-udp-send",
    pub unsafe fn socket_udp_send(fd: u64, buf_ptr: *const u8, len: u64, addr_ptr: *const u8, addr_len: u64) -> u64 {
        crate::sys::syscall6(44, fd, buf_ptr as u64, len, 0, addr_ptr as u64, addr_len)
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-udp-recv",
    pub unsafe fn socket_udp_recv(fd: u64, buf_ptr: *mut u8, len: u64, addr_ptr: *mut u8, addr_len_ptr: *mut u32) -> u64 {
        crate::sys::syscall6(45, fd, buf_ptr as u64, len, 0, addr_ptr as u64, addr_len_ptr as u64)
    }
);

method_export!("krakeos:system/network@0.2.0", "raw-send",
    pub unsafe fn net_send(ptr: *const u8, len: u32) -> i32 {
        -1
    }
);

method_export!("krakeos:system/network@0.2.0", "raw-recv",
    pub unsafe fn net_recv(ptr: *mut u8, len: u32) -> i32 {
        0
    }
);

method_export!("krakeos:system/process@0.2.0", "yield",
    pub unsafe fn process_yield() {
        core::arch::asm!("int 0x81");
    }
);

method_export!("krakeos:system/memory@0.2.0", "brk",
    pub unsafe fn memory_brk(addr: u64) -> u64 {
        crate::sys::syscall(12, addr, 0, 0)
    }
);

method_export!("krakeos:system/process@0.2.0", "get-list",
    pub unsafe fn process_get_list(buf_ptr: *mut u8, max_count: u64) -> u64 {
        crate::sys::syscall(110, buf_ptr as u64, max_count, 0)
    }
);

method_export!("krakeos:system/process@0.2.0", "poll",
    pub unsafe fn process_poll(fds_ptr: *mut u8, count: u64, timeout: u64) -> i32 {
        crate::sys::syscall(7, fds_ptr as u64, count, timeout) as i32
    }
);

// --- Reactor ---

pub struct Reactor {
    pub read_waiters: BTreeMap<i32, Waker>,
    pub write_waiters: BTreeMap<i32, Waker>,
}

pub static REACTOR: Mutex<Reactor> = Mutex::new(Reactor {
    read_waiters: BTreeMap::new(),
    write_waiters: BTreeMap::new(),
});

// --- Simplified public API ---

pub fn print(s: &str) {
    file_write(1, s.as_bytes());
}

pub fn debug_print(s: &str) {
    unsafe {
        let stderr = crate::io::host::get_stderr();
        let mut res = [0u8; 8];
        crate::io::host::output_stream_blocking_write_and_flush(stderr, s.as_ptr(), s.len(), res.as_mut_ptr());
    }
}

pub fn sleep(ms: u64) {
    crate::debugln!("CALLING TCP FN sleep WITH ARGS: ms={}", ms);
    crate::time::sleep(core::time::Duration::from_millis(ms));
    crate::debugln!("TCP RESULT: sleep finished");
}

pub fn yield_task() {
    unsafe { process_yield(); }
}

pub fn file_read(fd: usize, buffer: &mut [u8]) -> usize {
    crate::debugln!("CALLING TCP FN file_read WITH ARGS: fd={}, len={}", fd, buffer.len());
    let mut file = crate::fs::File::from_raw_fd(fd);
    let res = match crate::io::Read::read(&mut file, buffer) {
        Ok(n) => n,
        Err(_) => 0,
    };
    core::mem::forget(file);
    crate::debugln!("TCP RESULT: file_read RESULT: {}", res);
    res
}

pub fn file_write(fd: usize, buffer: &[u8]) -> usize {
    let mut file = crate::fs::File::from_raw_fd(fd);
    let res = match crate::io::Write::write(&mut file, buffer) {
        Ok(n) => n,
        Err(_) => 0,
    };
    core::mem::forget(file);
    res
}

pub fn file_close(fd: usize) -> i32 {
    crate::debugln!("CALLING TCP FN file_close WITH ARGS: fd={}", fd);
    unsafe {
        crate::fs::descriptor_drop(fd as i32);
    }
    crate::debugln!("TCP RESULT: file_close finished");
    0
}

pub fn exit(code: u64) -> ! {
    crate::debugln!("CALLING TCP FN exit WITH ARGS: code={}", code);
    unsafe {
        crate::io::host::exit(code as i32);
    }
}

pub fn spawn_with_fds(path: &str, args: &[&str], fds: &[(u8, u8)]) -> usize {
    crate::debugln!("CALLING TCP FN spawn_with_fds WITH ARGS: path={}, args_len={}, fds_len={}", path, args.len(), fds.len());
    
    let mut c_args = Vec::new();
    for &a in args {
        let mut s = String::from(a);
        s.push('\0');
        c_args.push(s);
    }
    
    let arg_ptrs: Vec<*const u8> = c_args.iter().map(|s| s.as_ptr()).collect();
    
    let res = unsafe {
        process_spawn(
            path.as_ptr(), path.len(),
            arg_ptrs.as_ptr() as *const u8, arg_ptrs.len(),
            fds.as_ptr() as *const u8, fds.len()
        ) as usize
    };
    crate::debugln!("TCP RESULT: spawn_with_fds RESULT: {}", res);
    res
}

pub fn spawn(path: &str) -> usize {
    spawn_with_fds(path, &[], &[(0, 0), (1, 1), (2, 2)])
}

pub fn get_system_ticks() -> u64 {
    unsafe { crate::time::host::monotonic_clock_now() / 1_000_000 }
}

pub fn brk(addr: usize) -> usize {
    crate::debugln!("CALLING TCP FN brk WITH ARGS: addr={:#x}", addr);
    let res = unsafe { memory_brk(addr as u64) as usize };
    crate::debugln!("TCP RESULT: brk RESULT: {:#x}", res);
    res
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct PollFd {
    pub fd: i32,
    pub events: i16,
    pub revents: i16,
}

pub const POLLIN: i16 = 0x001;
pub const POLLOUT: i16 = 0x004;

pub fn poll(fds: &mut [PollFd], timeout: i32) -> i32 {
    crate::debugln!("CALLING TCP FN poll WITH ARGS: fds_len={}, timeout={}", fds.len(), timeout);
    let res = unsafe { process_poll(fds.as_mut_ptr() as *mut u8, fds.len() as u64, timeout as u64) };
    crate::debugln!("TCP RESULT: poll RESULT: {}", res);
    res
}

pub fn set_nonblock(fd: usize, nonblock: bool) -> i32 {
    crate::debugln!("CALLING TCP FN set_nonblock WITH ARGS: fd={}, nonblock={}", fd, nonblock);
    let res = unsafe { process_set_nonblock(fd as u64, nonblock as u64) };
    crate::debugln!("TCP RESULT: set_nonblock RESULT: {}", res);
    res
}

pub fn get_date() -> (u8, u8, u16) {
    crate::debugln!("CALLING TCP FN get_date WITH ARGS");
    unsafe {
        let mut buf = [0u8; 16];
        crate::time::host::wall_clock_now(buf.as_mut_ptr());
        let secs = core::ptr::read_unaligned(buf.as_ptr() as *const u64);
        let days = secs / 86400;
        let (y, m, d) = epoch_to_date(days);
        crate::debugln!("TCP RESULT: get_date RESULT: {}/{}/{}", d, m, y);
        (d as u8, m as u8, y as u16)
    }
}

pub fn get_time() -> (u8, u8, u8) {
    crate::debugln!("CALLING TCP FN get_time WITH ARGS");
    unsafe {
        let mut buf = [0u8; 16];
        crate::time::host::wall_clock_now(buf.as_mut_ptr());
        let secs = core::ptr::read_unaligned(buf.as_ptr() as *const u64);
        let s = (secs % 60) as u8;
        let m = ((secs / 60) % 60) as u8;
        let h = ((secs / 3600) % 24) as u8;
        crate::debugln!("TCP RESULT: get_time RESULT: {}:{}:{}", h, m, s);
        (h, m, s)
    }
}

fn epoch_to_date(mut days: u64) -> (u64, u64, u64) {
    days += 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (y, m, d)
}

pub fn file_truncate(fd: usize, size: u64) -> i32 {
    crate::debugln!("CALLING TCP FN file_truncate WITH ARGS: fd={}, size={}", fd, size);
    let mut res = 0u8;
    unsafe { crate::fs::set_size(fd as i32, size, &mut res); }
    let ret = if res == 0 { 0 } else { -1 };
    crate::debugln!("TCP RESULT: file_truncate RESULT: {}", ret);
    ret
}

pub fn file_seek(fd: usize, offset: i64, whence: i32) -> i64 {
    crate::debugln!("CALLING TCP FN file_seek WITH ARGS: fd={}, off={}, wh={}", fd, offset, whence);
    let mut res_buf = [0u8; 16];
    unsafe { crate::fs::seek(fd as i32, offset as u64, whence, res_buf.as_mut_ptr()); }
    let ret = if res_buf[0] == 0 {
        unsafe { core::ptr::read_unaligned(res_buf.as_ptr().add(8) as *const u64) as i64 }
    } else {
        -1
    };
    crate::debugln!("TCP RESULT: file_seek RESULT: {}", ret);
    ret
}

pub fn pipe(fds: &mut [i32; 2]) -> i32 {
    crate::debugln!("CALLING TCP FN pipe WITH ARGS");
    let mut bytes = [0u8; 8];
    let res = unsafe { process_pipe(bytes.as_mut_ptr()) };
    if res == 0 {
        fds[0] = i32::from_le_bytes(bytes[0..4].try_into().unwrap());
        fds[1] = i32::from_le_bytes(bytes[4..8].try_into().unwrap());
    }
    crate::debugln!("TCP RESULT: pipe RESULT: {}", res);
    res
}

pub fn waitpid(pid: u64) -> i32 {
    crate::debugln!("CALLING TCP FN waitpid WITH ARGS: pid={}", pid);
    let res = unsafe { process_waitpid(pid) };
    crate::debugln!("TCP RESULT: waitpid RESULT: {}", res);
    res
}

pub use crate::memory::shm_get;

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ProcessInfo {
    pub pid: u64,
    pub state: u64,
    pub name: [u8; 32],
}

pub fn get_process_list() -> crate::rust_alloc::vec::Vec<ProcessInfo> {
    crate::debugln!("CALLING TCP FN get_process_list WITH ARGS");
    let mut buf = crate::rust_alloc::vec![ProcessInfo { pid: 0, state: 0, name: [0; 32] }; 64];
    let count = unsafe { process_get_list(buf.as_mut_ptr() as *mut u8, 64) };
    if count == u64::MAX { return crate::rust_alloc::vec::Vec::new(); }
    buf.truncate(count as usize);
    crate::debugln!("TCP RESULT: get_process_list RESULT: {} items", count);
    buf
}

pub const TIOCGWINSZ: u64 = 0x5413;
pub const TIOCSWINSZ: u64 = 0x5414;

#[repr(C)]
pub struct WinSize {
    pub ws_row: u16,
    pub ws_col: u16,
    pub ws_xpixel: u16,
    pub ws_ypixel: u16,
}

pub fn ioctl(fd: usize, request: u64, arg: u64) -> i32 {
    crate::debugln!("CALLING TCP FN ioctl WITH ARGS: fd={}, req={:#x}, arg={:#x}", fd, request, arg);
    let res = unsafe { process_ioctl(fd as u64, request, arg) };
    crate::debugln!("TCP RESULT: ioctl RESULT: {}", res);
    res
}
