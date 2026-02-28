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

// --- Process method_export! bindings (from wasi/krakeos.rs) ---

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

// --- krakeos net stubs (raw packet send/recv) ---

#[cfg(target_arch = "wasm32")]
pub unsafe fn krakeos_net_send(_ptr: *const u8, _len: u32) -> i32 {
    -1
}

#[cfg(target_arch = "wasm32")]
pub unsafe fn krakeos_net_recv(_ptr: *mut u8, _len: u32) -> i32 {
    0
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn krakeos_net_send(_ptr: *const u8, _len: u32) -> i32 {
    -1
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn krakeos_net_recv(_ptr: *mut u8, _len: u32) -> i32 {
    0
}

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
    #[cfg(not(target_arch = "wasm32"))]
    unsafe {
        syscall(999, s.as_ptr() as u64, s.len() as u64, 0);
    }
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let stderr = crate::io::host::get_stderr();
        let mut res = [0u8; 8];
        crate::io::host::output_stream_blocking_write_and_flush(stderr, s.as_ptr(), s.len(), res.as_mut_ptr());
    }
}

pub fn sleep(ms: u64) {
    crate::time::sleep(core::time::Duration::from_millis(ms));
}

pub fn yield_task() {
    crate::sys::yield_task();
}

pub fn file_read(fd: usize, buffer: &mut [u8]) -> usize {
    let mut file = crate::fs::File::from_raw_fd(fd);
    let res = match crate::io::Read::read(&mut file, buffer) {
        Ok(n) => n,
        Err(_) => 0,
    };
    core::mem::forget(file); // Don't close it
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
    unsafe {
        crate::fs::descriptor_drop(fd as i32);
    }
    0
}

pub fn exit(code: u64) -> ! {
    unsafe {
        crate::io::host::exit(code as i32);
    }
}

pub fn spawn_with_fds(path: &str, args: &[&str], fds: &[(u8, u8)]) -> usize {
    use crate::rust_alloc::vec::Vec;
    use crate::rust_alloc::string::String;

    let mut c_args = Vec::new();
    for &a in args {
        let mut s = String::from(a);
        s.push('\0');
        c_args.push(s);
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let arg_ptrs: Vec<*const u8> = c_args.iter().map(|s| s.as_ptr()).collect();
        unsafe {
            process_spawn(
                path.as_ptr(), path.len(),
                arg_ptrs.as_ptr() as *const u8, arg_ptrs.len(),
                fds.as_ptr() as *const u8, fds.len()
            ) as usize
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let arg_ptrs: Vec<u32> = c_args.iter().map(|s| s.as_ptr() as u32).collect();
        unsafe {
            process_spawn(
                path.as_ptr(), path.len(),
                arg_ptrs.as_ptr() as *const u8, arg_ptrs.len(),
                fds.as_ptr() as *const u8, fds.len()
            ) as usize
        }
    }
}

pub fn spawn(path: &str) -> usize {
    spawn_with_fds(path, &[], &[(0, 0), (1, 1), (2, 2)])
}

pub fn get_system_ticks() -> u64 {
    unsafe { crate::time::host::monotonic_clock_now() / 1_000_000 }
}

pub fn brk(addr: usize) -> usize {
    #[cfg(not(target_arch = "wasm32"))]
    unsafe { syscall(12, addr as u64, 0, 0) as usize }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = addr;
        0
    }
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
    #[cfg(not(target_arch = "wasm32"))]
    unsafe {
        syscall(7, fds.as_mut_ptr() as u64, fds.len() as u64, timeout as u64) as i32
    }
    #[cfg(target_arch = "wasm32")]
    unsafe {
        use crate::rust_alloc::vec::Vec;
        let mut handles = Vec::new();
        for pfd in fds.iter() {
            handles.push(pfd.fd);
        }
        let mut handles_bytes = Vec::new();
        for h in &handles {
            handles_bytes.extend_from_slice(&h.to_le_bytes());
        }

        let mut ready_indices_ptr = 0u32;
        let _ = timeout;

        crate::io::host::poll_poll(
            handles_bytes.as_ptr(),
            handles.len() as u32,
            &mut ready_indices_ptr as *mut u32 as *mut u8
        );

        0 // Stub for now
    }
}

pub fn set_nonblock(fd: usize, nonblock: bool) -> i32 {
    #[cfg(not(target_arch = "wasm32"))]
    unsafe {
        syscall(133, fd as u64, nonblock as u64, 0) as i32
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (fd, nonblock);
        0
    }
}

pub fn get_date() -> (u8, u8, u16) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let res = unsafe { syscall(115, 0, 0, 0) };
        let y = (res >> 16) as u16;
        let m = (res >> 8) as u8;
        let d = res as u8;
        (d, m, y)
    }
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let mut buf = [0u8; 16];
        crate::time::host::wall_clock_now(buf.as_mut_ptr());
        let secs = core::ptr::read_unaligned(buf.as_ptr() as *const u64);
        let days = secs / 86400;
        let (y, m, d) = epoch_to_date(days);
        (d as u8, m as u8, y as u16)
    }
}

pub fn get_time() -> (u8, u8, u8) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let res = unsafe { syscall(108, 0, 0, 0) };
        let h = (res >> 16) as u8;
        let m = (res >> 8) as u8;
        let s = res as u8;
        (h, m, s)
    }
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let mut buf = [0u8; 16];
        crate::time::host::wall_clock_now(buf.as_mut_ptr());
        let secs = core::ptr::read_unaligned(buf.as_ptr() as *const u64);
        let s = (secs % 60) as u8;
        let m = ((secs / 60) % 60) as u8;
        let h = ((secs / 3600) % 24) as u8;
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
    let mut res = 0u8;
    unsafe {
        crate::fs::set_size(fd as i32, size, &mut res);
    }
    if res == 0 { 0 } else { -1 }
}

pub fn file_seek(fd: usize, offset: i64, whence: i32) -> i64 {
    let mut res_buf = [0u8; 16];
    unsafe {
        crate::fs::seek(fd as i32, offset as u64, whence, res_buf.as_mut_ptr());
    }
    if res_buf[0] == 0 {
        unsafe { core::ptr::read_unaligned(res_buf.as_ptr().add(8) as *const u64) as i64 }
    } else {
        -1
    }
}

pub fn pipe(fds: &mut [i32; 2]) -> i32 {
    let mut bytes = [0u8; 8];
    let res = unsafe { process_pipe(bytes.as_mut_ptr()) };
    if res == 0 {
        fds[0] = i32::from_le_bytes(bytes[0..4].try_into().unwrap());
        fds[1] = i32::from_le_bytes(bytes[4..8].try_into().unwrap());
        0
    } else {
        -1
    }
}

pub fn waitpid(pid: u64) -> i32 {
    unsafe { process_waitpid(pid) }
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
    #[cfg(not(target_arch = "wasm32"))]
    unsafe {
        let mut buf = crate::rust_alloc::vec![ProcessInfo { pid: 0, state: 0, name: [0; 32] }; 64];
        let count = syscall(110, buf.as_mut_ptr() as u64, 64, 0);
        if count == u64::MAX { return crate::rust_alloc::vec::Vec::new(); }
        buf.truncate(count as usize);
        buf
    }
    #[cfg(target_arch = "wasm32")]
    {
        crate::rust_alloc::vec::Vec::new()
    }
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
    #[cfg(not(target_arch = "wasm32"))]
    unsafe {
        syscall(16, fd as u64, request, arg) as i32
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (fd, request, arg);
        0
    }
}
