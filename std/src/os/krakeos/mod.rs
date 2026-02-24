pub mod graphics;
pub use graphics::*;

pub mod user;
pub use user::*;

pub mod net;

pub mod events;
pub use events::*;

#[cfg(not(target_arch = "wasm32"))]
pub use crate::sys::{syscall, syscall4, syscall5, syscall6};

use crate::sync::Mutex;
use core::task::Waker;
use crate::rust_alloc::collections::BTreeMap;

pub struct Reactor {
    pub read_waiters: BTreeMap<i32, Waker>,
    pub write_waiters: BTreeMap<i32, Waker>,
}

pub static REACTOR: Mutex<Reactor> = Mutex::new(Reactor {
    read_waiters: BTreeMap::new(),
    write_waiters: BTreeMap::new(),
});

pub fn print(s: &str) {
    file_write(1, s.as_bytes());
}

pub fn debug_print(s: &str) {
    let stderr = unsafe { crate::wasi::cli::get_stderr() };
    let mut res = [0u8; 8]; // result buffer
    unsafe {
        crate::wasi::io::output_stream_blocking_write_and_flush(stderr, s.as_ptr(), s.len(), res.as_mut_ptr());
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
        crate::wasi::filesystem::descriptor_drop(fd as i32);
    }
    0
}

pub fn exit(code: u64) -> ! {
    unsafe {
        crate::wasi::cli::exit(code as i32);
    }
}

pub fn spawn_with_fds(path: &str, args: &[&str], fds: &[(u8, u8)]) -> usize {
    use crate::rust_alloc::vec::Vec;
    use crate::rust_alloc::string::String;

    // Unified logic: prepare args as pointers.
    // However, native wants *const *const u8, WASM wants *const u32 (pointers).
    // The underlying representation of *const u8 is a pointer (u64 or u32).
    // On native, Vec<*const u8> works.
    // On WASM, Vec<u32> works.
    // I can use `usize` to be generic?

    let mut c_args = Vec::new();
    for &a in args {
        let mut s = String::from(a);
        s.push('\0');
        c_args.push(s);
    }

    // We need an array of pointers to the strings.
    // The strings are in c_args.
    let arg_ptrs: Vec<*const u8> = c_args.iter().map(|s| s.as_ptr()).collect();

    // On WASM, pointers are u32. *const u8 is u32.
    // On Native, pointers are u64.

    unsafe {
        crate::wasi::krakeos::process_spawn(
            path.as_ptr(), path.len(),
            arg_ptrs.as_ptr() as *const u8, arg_ptrs.len(),
            fds.as_ptr() as *const u8, fds.len()
        ) as usize
    }
}

pub fn spawn(path: &str) -> usize {
    spawn_with_fds(path, &[], &[(0, 0), (1, 1), (2, 2)])
}

pub fn get_system_ticks() -> u64 {
    // monotonic_clock_now returns ns.
    // get_system_ticks used to return ticks (ms on native).
    // WASM impl divided by 1_000_000.
    // I will unify to use monotonic_clock_now / 1_000_000.
    unsafe {
        crate::wasi::clocks::monotonic_clock_now() / 1_000_000
    }
}

pub fn brk(addr: usize) -> usize {
    #[cfg(not(target_arch = "wasm32"))]
    unsafe { syscall(12, addr as u64, 0, 0) as usize }
    #[cfg(target_arch = "wasm32")]
    {
        // WASM handles memory via grow
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
        // Map PollFd to WASI poll
        // WASI poll uses handles (subscription).
        // For now, use wasi::io::poll
        // Prepare simple buffer of i32 handles for poll
        use crate::rust_alloc::vec::Vec;
        let mut handles = Vec::new();
        for pfd in fds.iter() {
            handles.push(pfd.fd);
        }
        // Also add timer if timeout >= 0
        let mut handles_bytes = Vec::new();
        for h in &handles {
            handles_bytes.extend_from_slice(&h.to_le_bytes());
        }
        
        let mut ready_indices_ptr = 0u32;
        let mut count = 0u32;
        
        crate::wasi::io::poll_poll(
            handles_bytes.as_ptr(),
            handles.len() as u32,
            &mut ready_indices_ptr as *mut u32 as *mut u8
        );
        
        0 // Stub
    }
}

pub fn set_nonblock(fd: usize, nonblock: bool) -> i32 {
    #[cfg(not(target_arch = "wasm32"))]
    unsafe {
        syscall(133, fd as u64, nonblock as u64, 0) as i32
    }
    #[cfg(target_arch = "wasm32")]
    { 0 }
}

pub fn get_date() -> (u8, u8, u16) {
    unsafe {
        let mut buf = [0u8; 16];
        crate::wasi::clocks::wall_clock_now(buf.as_mut_ptr());
        let secs = core::ptr::read_unaligned(buf.as_ptr() as *const u64);
        
        let days = secs / 86400;
        let (y, m, d) = epoch_to_date(days);
        (d as u8, m as u8, y as u16)
    }
}

pub fn get_time() -> (u8, u8, u8) {
    unsafe {
        let mut buf = [0u8; 16];
        crate::wasi::clocks::wall_clock_now(buf.as_mut_ptr());
        let secs = core::ptr::read_unaligned(buf.as_ptr() as *const u64);
        
        let s = (secs % 60) as u8;
        let m = ((secs / 60) % 60) as u8;
        let h = ((secs / 3600) % 24) as u8;
        (h, m, s)
    }
}

fn epoch_to_date(mut days: u64) -> (u64, u64, u64) {
    // 1970-01-01 was Thursday
    days += 719468; // Adjust to 0000-03-01
    let era = days / 146097;
    let doe = days - era * 146097; // Day of era
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // Year of era
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // Day of year
    let mp = (5 * doy + 2) / 153; // Month in Mar..Feb cycle
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { 153 }; // No, standard algo is: m = mp + 3; if m > 12 { m -= 12; y += 1; }
    // My previous code: mp + if mp < 10 { 3 } else { -9 } (as i64).
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    
    (y, m, d)
}

pub fn file_truncate(fd: usize, size: u64) -> i32 {
    unsafe {
        let mut res = 0u8;
        crate::wasi::filesystem::set_size(fd as i32, size, &mut res);
        if res == 0 { 0 } else { -1 }
    }
}

pub fn file_seek(fd: usize, offset: i64, whence: i32) -> i64 {
    unsafe {
        let mut res_buf = [0u8; 16];
        crate::wasi::filesystem::seek(fd as i32, offset as u64, whence, res_buf.as_mut_ptr());
        if res_buf[0] == 0 {
            let new_offset = core::ptr::read_unaligned(res_buf.as_ptr().add(8) as *const u64);
            new_offset as i64
        } else {
            -1
        }
    }
}

pub fn pipe(fds: &mut [i32; 2]) -> i32 {
    unsafe {
        // Use krakeos:system/process pipe shim
        // It expects *mut u8 (generic pointer)
        // Since i32 is 4 bytes, [i32; 2] is 8 bytes.
        // On native, shim calls syscall(22, fds.as_mut_ptr(), ...).
        // On WASM, it calls process_pipe(fds.as_mut_ptr()).
        let res = crate::wasi::krakeos::process_pipe(fds.as_mut_ptr() as *mut u8);
        res
    }
}

pub fn waitpid(pid: u64) -> i32 {
    unsafe {
        crate::wasi::krakeos::process_waitpid(pid)
    }
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
    { 0 }
}
