pub mod graphics;
pub use graphics::*;

pub mod user;
pub use user::*;

pub mod events;
pub use events::*;

#[cfg(any(feature = "userland", target_arch = "x86_64"))]
pub mod wasi;

#[cfg(not(target_arch = "wasm32"))]
pub use crate::sys::{syscall, syscall4, syscall5, syscall6};

#[cfg(target_arch = "wasm32")]
pub use crate::sys::syscall;

use crate::sync::Mutex;
use core::task::Waker;
use crate::alloc::collections::BTreeMap;
use crate::alloc::format;
use crate::alloc::string::String;
use crate::alloc::vec::Vec;

// --- Process method_export! bindings ---

method_export!("krakeos:system/process@0.2.0", "spawn",
    pub fn process_spawn(path_ptr: *const u8, path_len: usize, args_ptr: *const u8, args_len: usize, fds_ptr: *const u8, fds_len: usize) -> u64 {
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
    pub fn process_waitpid(pid: u64) -> i32 {
        crate::sys::syscall(61, pid, 0, 0) as i32
    }
);

method_export!("krakeos:system/process@0.2.0", "pipe",
    pub fn process_pipe(fds_ptr: *mut u8) -> i32 {
        crate::sys::syscall(22, fds_ptr as u64, 0, 0) as i32
    }
);

method_export!("krakeos:system/memory@0.2.0", "shm-get",
    pub fn shm_get_raw(name_ptr: *const u8, name_len: usize, size: usize) -> u64 {
        let res = crate::sys::syscall(120, name_ptr as u64, name_len as u64, size as u64);
        crate::debugln!("[std] shm_get_raw: Syscall 120 (GET) returned {:#x}", res);
        res
    }
);

method_export!("krakeos:system/memory@0.2.0", "shm-map",
    pub fn shm_map_raw(name_ptr: *const u8, name_len: usize, target_addr: u64) -> u64 {
        let res = crate::sys::syscall(122, name_ptr as u64, name_len as u64, target_addr);
        crate::debugln!("[std] shm_map_raw: Syscall 122 (MAP) returned {:#x}", res);
        res
    }
);

method_export!("wasi:random/random@0.2.0", "get-random-bytes",
    pub fn get_random_bytes(len: u64, result_ptr: *mut u8) {
        #[cfg(target_arch = "x86_64")]
        {
            let mut i = 0;
            while i + 8 <= len as usize {
                let mut val = 0u64;
                if unsafe { core::arch::x86_64::_rdrand64_step(&mut val) } == 1 {
                    unsafe { core::ptr::copy_nonoverlapping(val.to_le_bytes().as_ptr(), result_ptr.add(i), 8); }
                    i += 8;
                } else {
                    break;
                }
            }
            while i < len as usize {
                let mut val = 0u32;
                if unsafe { core::arch::x86_64::_rdrand32_step(&mut val) } == 1 {
                    unsafe { *result_ptr.add(i) = val as u8; }
                    i += 1;
                } else {
                    break;
                }
            }
            if i == len as usize {
                return;
            }
        }

        // Pseudo-random fallback for native
        static mut STATE: u64 = 1574;
        unsafe {
            if STATE == 1574 {
                STATE = crate::sys::syscall(109, 0, 0, 0).wrapping_add(0xACE1BADE);
            }
            for i in 0..len as usize {
                STATE = STATE.wrapping_mul(6364136223846793005).wrapping_add(1);
                *result_ptr.add(i) = (STATE >> 33) as u8;
            }
        }
    }
);

#[repr(C)]
pub struct SlotInfo {
    pub slot_id: u16,
    pub linear_memory_base: u64,
    pub linear_memory_size: u64,
    pub code_base: u64,
    pub stack_base: u64,
}

method_export!("krakeos:system/process@0.2.0", "get-slot-info",
    pub fn process_get_slot_info(buf_ptr: *mut u8) -> i32 {
        crate::sys::syscall(137, buf_ptr as u64, 0, 0) as i32
    }
);

method_export!("krakeos:system/process@0.2.0", "ioctl",
    pub fn process_ioctl(fd: u64, request: u64, arg: u64) -> i32 {
        crate::sys::syscall(16, fd, request, arg) as i32
    }
);

method_export!("krakeos:system/process@0.2.0", "set-nonblock",
    pub fn process_set_nonblock(fd: u64, nonblock: u64) -> i32 {
        crate::sys::syscall(133, fd, nonblock, 0) as i32
    }
);

method_export!("krakeos:system/process@0.2.0", "get-pid",
    pub fn process_get_pid() -> u64 {
        crate::sys::syscall(39, 0, 0, 0)
    }
);

// --- Networking method_export! bindings ---

method_export!("krakeos:system/network@0.2.0", "socket-create",
    pub fn socket_create(family: u64, ty: u64) -> u64 {
        crate::sys::syscall(41, family, ty, 0)
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-connect",
    pub fn socket_connect(fd: u64, addr_ptr: *const u8, addr_len: u64) -> u64 {
        crate::sys::syscall6(42, fd, addr_ptr as u64, addr_len, 0, 0, 0)
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-finish-connect",
    pub fn socket_finish_connect(fd: u64) -> u64 {
        crate::sys::syscall6(54, fd, 0, 0, 0, 0, 0)
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-bind",
    pub fn socket_bind(fd: u64, addr_ptr: *const u8, addr_len: u64) -> u64 {
        crate::sys::syscall6(49, fd, addr_ptr as u64, addr_len, 0, 0, 0)
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-listen",
    pub fn socket_listen(fd: u64, backlog: u64) -> u64 {
        crate::sys::syscall6(51, fd, backlog, 0, 0, 0, 0)
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-accept",
    pub fn socket_accept(fd: u64) -> u64 {
        crate::sys::syscall6(43, fd, 0, 0, 0, 0, 0)
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-send",
    pub fn socket_send(fd: u64, buf_ptr: *const u8, len: u64) -> u64 {
        crate::sys::syscall6(52, fd, buf_ptr as u64, len, 0, 0, 0)
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-recv",
    pub fn socket_recv(fd: u64, buf_ptr: *mut u8, len: u64) -> u64 {
        crate::sys::syscall6(53, fd, buf_ptr as u64, len, 0, 0, 0)
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-udp-send",
    pub fn socket_udp_send(fd: u64, buf_ptr: *const u8, len: u64, addr_ptr: *const u8, addr_len: u64) -> u64 {
        crate::sys::syscall6(44, fd, buf_ptr as u64, len, 0, addr_ptr as u64, addr_len)
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-udp-recv",
    pub fn socket_udp_recv(fd: u64, buf_ptr: *mut u8, len: u64, addr_ptr: *mut u8, addr_len_ptr: *mut u32) -> u64 {
        crate::sys::syscall6(45, fd, buf_ptr as u64, len, 0, addr_ptr as u64, addr_len_ptr as u64)
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-get-local-addr",
    pub fn socket_get_local_addr(fd: u64, addr_ptr: *mut u8) -> i32 {
        crate::sys::syscall(46, fd, addr_ptr as u64, 0) as i32
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-get-remote-addr",
    pub fn socket_get_remote_addr(fd: u64, addr_ptr: *mut u8) -> i32 {
        crate::sys::syscall(47, fd, addr_ptr as u64, 0) as i32
    }
);

method_export!("krakeos:system/network@0.2.0", "socket-shutdown",
    pub fn socket_shutdown(fd: u64, how: u64) -> i32 {
        crate::sys::syscall(48, fd, how, 0) as i32
    }
);

method_export!("krakeos:system/process@0.2.0", "yield",
    pub fn process_yield() {
        core::arch::asm!("int 0x81");
    }
);

method_export!("krakeos:system/memory@0.2.0", "brk",
    pub fn memory_brk(addr: u64) -> u64 {
        crate::sys::syscall(12, addr, 0, 0)
    }
);

method_export!("krakeos:system/process@0.2.0", "get-list",
    pub fn process_get_list(buf_ptr: *mut u8, max_count: u64) -> u64 {
        crate::sys::syscall(110, buf_ptr as u64, max_count, 0)
    }
);

method_export!("krakeos:system/process@0.2.0", "kill",
    pub fn process_kill(pid: u64, signal: u32) -> i32 {
        crate::sys::syscall(62, pid, signal as u64, 0) as i32
    }
);

method_export!("krakeos:system/memory@0.2.0", "get-total-mem",
    pub fn get_total_mem() -> u64 {
        crate::sys::syscall(134, 0, 0, 0)
    }
);

method_export!("krakeos:system/memory@0.2.0", "get-used-mem",
    pub fn get_used_mem() -> u64 {
        crate::sys::syscall(135, 0, 0, 0)
    }
);

method_export!("krakeos:system/memory@0.2.0", "get-vma-dump",
    pub fn get_vma_dump(buf_ptr: *mut u8, len: u64) -> u64 {
        crate::sys::syscall(136, buf_ptr as u64, len, 0)
    }
);

method_export!("krakeos:system/terminal@0.1.0", "set-window-size",
    pub fn raw_terminal_set_window_size(fd: u32, rows: u16, cols: u16, ret_ptr: *mut u8) {
        let ws = WinSize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let res = process_ioctl(fd as u64, TIOCSWINSZ, &ws as *const _ as u64);
        unsafe { *ret_ptr = if res == 0 { 0 } else { 1 }; }
    }
);

method_export!("krakeos:system/terminal@0.1.0", "get-window-size",
    pub fn raw_terminal_get_window_size(fd: u32, ret_ptr: *mut u8) {
        let mut ws = WinSize {
            ws_row: 0,
            ws_col: 0,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let res = process_ioctl(fd as u64, TIOCGWINSZ, &mut ws as *mut _ as u64);
        unsafe {
            if res == 0 {
                *ret_ptr = 0;
                core::ptr::write_unaligned(ret_ptr.add(4) as *mut u16, ws.ws_row);
                core::ptr::write_unaligned(ret_ptr.add(6) as *mut u16, ws.ws_col);
            } else {
                *ret_ptr = 1;
            }
        }
    }
);

method_export!("krakeos:system/process@0.2.0", "poll",
    pub fn process_poll(fds_ptr: *mut u8, count: u64, timeout: u64) -> i32 {
        crate::sys::syscall(7, fds_ptr as u64, count, timeout) as i32
    }
);

method_export!("krakeos:system/container@0.1.0", "plant",
    pub fn raw_container_plant(bytes_ptr: *const u8, bytes_len: usize, offset: u32, size: u32, fds_ptr: *const u8, fds_len: usize, ret_ptr: *mut u8) {
        // Native stub: not implemented for native execution
    }
);

method_export!("krakeos:system/container@0.1.0", "plant-from-path",
    pub fn raw_container_plant_from_path(path_ptr: *const u8, path_len: usize, offset: u32, size: u32, fds_ptr: *const u8, fds_len: usize, ret_ptr: *mut u8) {
        // Native stub
    }
);

method_export!("krakeos:system/container@0.1.0", "harvest",
    pub fn raw_container_harvest(id: u64, ret_ptr: *mut u8) {
        // Native stub
    }
);

method_export!("krakeos:system/container@0.1.0", "list-children",
    pub fn raw_container_list_children(ret_ptr: *mut u8) {
        // Native stub
    }
);

method_export!("krakeos:system/container@0.1.0", "kill-child",
    pub fn raw_container_kill_child(id: u64, ret_ptr: *mut u8) {
        // Native stub
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
    #[cfg(target_arch = "wasm32")]
    {
        let stderr = crate::io::host::get_stderr();
        let mut res = [0u8; 8];
        crate::io::host::output_stream_blocking_write_and_flush(stderr, s.as_ptr(), s.len(), res.as_mut_ptr());
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        unsafe {
            crate::sys::syscall(999, s.as_ptr() as u64, s.len() as u64, 0);
        }
    }
}

pub fn sleep(ms: u64) {
    crate::time::sleep(core::time::Duration::from_millis(ms));
}

pub fn yield_task() {
    process_yield();
}

method_export!("krakeos:system/process@0.2.0", "native-file-open",
    pub fn native_file_open(path_ptr: *const u8, path_len: u64, flags: u64) -> i64 {
        let res = crate::sys::syscall(2, path_ptr as u64, path_len, flags);
        if res == u64::MAX {
            -1
        } else {
            res as i64
        }
    }
);

method_export!("krakeos:system/process@0.2.0", "native-file-stat",
    pub fn native_file_stat(fd: u64, stat_ptr: *mut u8) -> i32 {
        let res = crate::sys::syscall(5, fd, 0, stat_ptr as u64);
        if res == u64::MAX {
            -1
        } else {
            0
        }
    }
);

method_export!("krakeos:system/process@0.2.0", "file-read",
    pub fn process_file_read(fd: u64, buf_ptr: *mut u8, len: u64) -> i64 {
        let res = crate::sys::syscall(0, fd, buf_ptr as u64, len);
        let pid = crate::os::process_get_pid();
        if res == u64::MAX - 1 {
            -2
        } else if res == u64::MAX {
            crate::debugln!("[std host] PID {} file-read fd={} len={} FAILED", pid, fd, len);
            -1
        } else {
            if res > 0 {
                crate::debugln!("[std host] PID {} file-read fd={} len={} -> {}", pid, fd, len, res);
            }
            res as i64
        }
    }
);

method_export!("krakeos:system/process@0.2.0", "file-write",
    pub fn process_file_write(fd: u64, buf_ptr: *const u8, len: u64) -> i64 {
        let res = crate::sys::syscall(1, fd, buf_ptr as u64, len);
        let pid = crate::os::process_get_pid();
        if res == u64::MAX - 1 {
            -2
        } else if res == u64::MAX {
            crate::debugln!("[std host] PID {} file-write fd={} len={} FAILED", pid, fd, len);
            -1
        } else {
            crate::debugln!("[std host] PID {} file-write fd={} len={} -> {}", pid, fd, len, res);
            res as i64
        }
    }
);

pub fn file_read(fd: usize, buffer: &mut [u8]) -> usize {
    let res = process_file_read(fd as u64, buffer.as_mut_ptr(), buffer.len() as u64);
    if res == -2 {
        return usize::MAX - 1;
    } else if res == -1 {
        return 0;
    }
    res as usize
}

pub fn file_write(fd: usize, buffer: &[u8]) -> usize {
    let res = process_file_write(fd as u64, buffer.as_ptr(), buffer.len() as u64);
    if res == -2 {
        return usize::MAX - 1;
    } else if res == -1 {
        return 0;
    }
    res as usize
}

pub fn file_close(fd: usize) -> i32 {
    crate::fs::descriptor_drop(fd as i32);
    0
}

pub fn exit(code: u64) -> ! {
    crate::io::host::exit(code as i32);
}

pub fn spawn_with_fds(path: &str, args: &[&str], fds: &[(u8, u8)]) -> usize {
    let mut c_args = Vec::new();
    for &a in args {
        let mut s = String::from(a);
        s.push('\0');
        c_args.push(s);
    }
    
    let arg_ptrs: Vec<*const u8> = c_args.iter().map(|s| s.as_ptr()).collect();
    
    let res = process_spawn(
        path.as_ptr(), path.len(),
        arg_ptrs.as_ptr() as *const u8, arg_ptrs.len(),
        fds.as_ptr() as *const u8, fds.len()
    ) as usize;
    res
}

pub fn spawn(path: &str) -> usize {
    spawn_with_fds(path, &[], &[(0, 0), (1, 1), (2, 2)])
}

pub fn get_system_ticks() -> u64 {
    crate::time::host::monotonic_clock_now() / 1_000_000
}

pub fn brk(addr: usize) -> usize {
    let res = memory_brk(addr as u64) as usize;
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
    let res = process_poll(fds.as_mut_ptr() as *mut u8, fds.len() as u64, timeout as u64);
    res
}

pub fn set_nonblock(fd: usize, nonblock: bool) -> i32 {
    let res = process_set_nonblock(fd as u64, nonblock as u64);
    res
}

pub fn get_date() -> (u8, u8, u16) {
    let mut buf = [0u8; 16];
    crate::time::host::wall_clock_now(buf.as_mut_ptr());
    let secs = unsafe { core::ptr::read_unaligned(buf.as_ptr() as *const u64) };
    let days = secs / 86400;
    let (y, m, d) = epoch_to_date(days);
    (d as u8, m as u8, y as u16)
}

pub fn get_time() -> (u8, u8, u8) {
    let mut buf = [0u8; 16];
    crate::time::host::wall_clock_now(buf.as_mut_ptr());
    let secs = unsafe { core::ptr::read_unaligned(buf.as_ptr() as *const u64) };
    let s = (secs % 60) as u8;
    let m = ((secs / 60) % 60) as u8;
    let h = ((secs / 3600) % 24) as u8;
    (h, m, s)
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
    let mut res = 0u8;
    crate::fs::set_size(fd as i32, size, &mut res);
    let ret = if res == 0 { 0 } else { -1 };
    ret
}

pub fn file_seek(fd: usize, offset: i64, whence: i32) -> i64 {
    let mut res_buf = [0u8; 16];
    crate::fs::seek(fd as i32, offset as u64, whence, res_buf.as_mut_ptr());
    let ret = if res_buf[0] == 0 {
        unsafe { core::ptr::read_unaligned(res_buf.as_ptr().add(8) as *const u64) as i64 }
    } else {
        -1
    };
    ret
}

pub fn pipe(fds: &mut [i32; 2]) -> i32 {
    let mut bytes = [0u8; 8];
    let res = process_pipe(bytes.as_mut_ptr());
    if res == 0 {
        fds[0] = i32::from_le_bytes(bytes[0..4].try_into().unwrap());
        fds[1] = i32::from_le_bytes(bytes[4..8].try_into().unwrap());
    }
    res
}

pub fn waitpid(pid: u64) -> i32 {
    let res = process_waitpid(pid);
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

impl ProcessInfo {
    pub fn slot_id(&self) -> u64 { self.pid }
}

pub fn get_process_list() -> crate::alloc::vec::Vec<ProcessInfo> {
    let mut buf = crate::alloc::vec![ProcessInfo { pid: 0, state: 0, name: [0; 32] }; 64];
    let count = process_get_list(buf.as_mut_ptr() as *mut u8, 64);
    if count == u64::MAX { return crate::alloc::vec::Vec::new(); }
    buf.truncate(count as usize);
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
    let res = process_ioctl(fd as u64, request, arg);
    res
}

pub fn terminal_set_window_size(fd: u32, rows: u16, cols: u16) -> Result<(), String> {
    let mut res = 0u8;
    raw_terminal_set_window_size(fd, rows, cols, &mut res);
    if res == 0 { Ok(()) } else { Err(String::from("Failed to set window size")) }
}

pub fn terminal_get_window_size(fd: u32) -> Result<(u16, u16), String> {
    let mut res_buf = [0u8; 8];
    raw_terminal_get_window_size(fd, res_buf.as_mut_ptr());
    if res_buf[0] == 0 {
        let rows = u16::from_le_bytes(res_buf[4..6].try_into().unwrap());
        let cols = u16::from_le_bytes(res_buf[6..8].try_into().unwrap());
        Ok((rows, cols))
    } else {
        Err(String::from("Failed to get window size"))
    }
}

pub fn container_plant(wasm_bytes: &[u8], offset: u32, size: u32, fds: Option<&[(u8, u8)]>) -> Result<u64, String> {
    let mut ret_buf = [0u8; 16];
    let (fds_ptr, fds_len) = if let Some(fds) = fds {
        (fds.as_ptr() as *const u8, fds.len())
    } else {
        (core::ptr::null(), 0)
    };
    raw_container_plant(wasm_bytes.as_ptr(), wasm_bytes.len(), offset, size, fds_ptr, fds_len, ret_buf.as_mut_ptr());
    let tag = u32::from_le_bytes(ret_buf[0..4].try_into().unwrap());
    if tag == 0 {
        Ok(u64::from_le_bytes(ret_buf[8..16].try_into().unwrap()))
    } else {
        Err(String::from("Plant failed"))
    }
}

pub fn container_plant_from_path(path: &str, offset: u32, size: u32, fds: Option<&[(u8, u8)]>) -> Result<u64, String> {
    let mut ret_buf = [0u8; 16];
    let (fds_ptr, fds_len) = if let Some(fds) = fds {
        (fds.as_ptr() as *const u8, fds.len())
    } else {
        (core::ptr::null(), 0)
    };
    raw_container_plant_from_path(path.as_ptr(), path.len(), offset, size, fds_ptr, fds_len, ret_buf.as_mut_ptr());
    let tag = u32::from_le_bytes(ret_buf[0..4].try_into().unwrap());
    if tag == 0 {
        Ok(u64::from_le_bytes(ret_buf[8..16].try_into().unwrap()))
    } else {
        Err(String::from("Plant from path failed"))
    }
}

pub fn container_harvest(id: u64) -> Result<i32, String> {
    let mut ret_buf = [0u8; 16];
    raw_container_harvest(id, ret_buf.as_mut_ptr());
    let tag = u32::from_le_bytes(ret_buf[0..4].try_into().unwrap());
    if tag == 0 {
        Ok(i32::from_le_bytes(ret_buf[4..8].try_into().unwrap()))
    } else {
        Err(String::from("Harvest failed or still running"))
    }
}

pub fn container_list_children() -> Vec<u64> {
    let mut ret_buf = [0u8; 16];
    raw_container_list_children(ret_buf.as_mut_ptr());
    let ptr = u32::from_le_bytes(ret_buf[0..4].try_into().unwrap());
    let len = u32::from_le_bytes(ret_buf[4..8].try_into().unwrap());

    if ptr == 0 || len == 0 { return Vec::new(); }

    let mut result = Vec::with_capacity(len as usize);
    let slice = unsafe { core::slice::from_raw_parts(ptr as *const u64, len as usize) };
    result.extend_from_slice(slice);
    // Note: In a real system we'd need to free the memory allocated by host
    result
}

pub fn container_kill_child(id: u64) -> Result<(), String> {
    let mut ret_buf = [0u8; 16];
    raw_container_kill_child(id, ret_buf.as_mut_ptr());
    let tag = u32::from_le_bytes(ret_buf[0..4].try_into().unwrap());
    if tag == 0 {
        Ok(())
    } else {
        Err(String::from("Kill failed"))
    }
}
