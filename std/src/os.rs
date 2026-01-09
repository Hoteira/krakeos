use core::arch::asm;
pub use crate::sys::{syscall, syscall4, syscall5, syscall6};
use crate::debugln;

pub fn print(s: &str) {
    file_write(1, s.as_bytes());
}

pub fn debug_print(s: &str) {
    unsafe {
        syscall(999, s.as_ptr() as u64, s.len() as u64, 0);
    }
}

pub fn sleep(ms: u64) {
    if ms > 10 {
        unsafe {
            syscall(35, ms, 0, 0);
        }

        yield_task();
    }
}

pub fn yield_task() {
    unsafe {
        asm!("int 0x81");
    }
}


pub fn read(buffer: &mut [u8]) -> usize {
    unsafe {
        syscall(0, 0, buffer.as_mut_ptr() as u64, buffer.len() as u64) as usize
    }
}

pub fn file_read(fd: usize, buffer: &mut [u8]) -> usize {
    unsafe {
        syscall(0, fd as u64, buffer.as_mut_ptr() as u64, buffer.len() as u64) as usize
    }
}

pub fn file_write(fd: usize, buffer: &[u8]) -> usize {
    let mut total_written = 0;
    while total_written < buffer.len() {
        let n = unsafe {
            syscall(1, fd as u64, buffer[total_written..].as_ptr() as u64, (buffer.len() - total_written) as u64) as usize
        };
        if n == 0 || n == usize::MAX {
            break;
        }
        total_written += n;
        if total_written < buffer.len() {
            yield_task();
        }
    }
    total_written
}

pub fn file_seek(fd: usize, offset: i64, whence: usize) -> u64 {
    unsafe {
        syscall(8, fd as u64, offset as u64, whence as u64)
    }
}

pub fn pipe(fds: &mut [i32; 2]) -> i32 {
    unsafe {
        syscall(22, fds.as_mut_ptr() as u64, 0, 0) as i32
    }
}

#[repr(C)]
pub struct WinSize {
    pub ws_row: u16,
    pub ws_col: u16,
    pub ws_xpixel: u16,
    pub ws_ypixel: u16,
}

pub const TIOCGWINSZ: u64 = 0x5413;
pub const TIOCSWINSZ: u64 = 0x5414;

pub fn ioctl(fd: usize, request: u64, arg: u64) -> i32 {
    unsafe {
        syscall(16, fd as u64, request, arg) as i32
    }
}

pub fn file_close(fd: usize) -> i32 {
    unsafe {
        syscall(3, fd as u64, 0, 0) as i32
    }
}

pub fn exit(code: u64) -> ! {
    unsafe {
        syscall(60, code, 0, 0);

        loop { asm!("hlt"); }
    }
}

pub fn exec(path: &str) {
    spawn_ext(path, &[]);
}

pub fn spawn(path: &str) -> usize {
    spawn_ext(path, &[])
}

pub fn spawn_ext(path: &str, args: &[&str]) -> usize {
    spawn_with_fds(path, args, &[])
}

pub fn spawn_with_fds(path: &str, args: &[&str], fds: &[(u8, u8)]) -> usize {
    use rust_alloc::vec::Vec;
    use rust_alloc::string::String;

    
    let mut c_args = Vec::new();
    for &a in args {
        let mut s = String::from(a);
        s.push('\0');
        c_args.push(s);
    }

    let arg_ptrs: Vec<*const u8> = c_args.iter().map(|s| s.as_ptr()).collect();

    unsafe {
        syscall6(59, 
            path.as_ptr() as u64, 
            path.len() as u64, 
            arg_ptrs.as_ptr() as u64, 
            arg_ptrs.len() as u64, 
            fds.as_ptr() as u64,
            fds.len() as u64
        ) as usize
    }
}

pub fn waitpid(pid: usize) -> usize {
    unsafe {
        loop {
            let status = syscall(61, pid as u64, 0, 0);
            if status != u64::MAX {
                return status as usize;
            }
            yield_task();
        }
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
    unsafe {
        syscall(7, fds.as_mut_ptr() as u64, fds.len() as u64, timeout as u64) as i32
    }
}

pub fn get_time() -> (u8, u8, u8) {
    let res = unsafe { syscall(108, 0, 0, 0) };
    let h = ((res >> 16) & 0xFF) as u8;
    let m = ((res >> 8) & 0xFF) as u8;
    let s = (res & 0xFF) as u8;
    (h, m, s)
}

pub fn get_system_ticks() -> u64 {
    unsafe { syscall(109, 0, 0, 0) }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ProcessInfo {
    pub pid: u64,
    pub state: u64,
    pub name: [u8; 32],
}

pub fn get_process_list() -> rust_alloc::vec::Vec<ProcessInfo> {
    let max_count = 128;
    let mut processes = rust_alloc::vec::Vec::with_capacity(max_count);

    
    processes.resize(max_count, ProcessInfo { pid: 0, state: 0, name: [0; 32] });

    let count = unsafe {
        syscall(110, processes.as_mut_ptr() as u64, max_count as u64, 0) as usize
    };

    
    if count <= max_count {
        processes.truncate(count);
    }
    processes
}

pub fn get_process_memory(pid: u64) -> usize {
    unsafe { syscall(111, pid, 0, 0) as usize }
}


pub fn brk(addr: usize) -> usize {
    unsafe { syscall(12, addr as u64, 0, 0) as usize }
}

pub fn mmap(addr: usize, len: usize) -> usize {
    unsafe { syscall6(9, addr as u64, len as u64, 0, 0, 0, 0) as usize }
}