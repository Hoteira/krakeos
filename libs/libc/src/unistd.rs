use crate::sys::krake_sleep;
use core::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn usleep(usec: c_uint) -> c_int {
    let ms = (usec + 999) / 1000;
    krake_sleep(ms as usize);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn time(t: *mut c_long) -> c_long {
    let ticks = std::os::syscall(109, 0, 0, 0); // SYS_GET_TICKS
    let seconds = (ticks / 1000) as c_long;
    if !t.is_null() {
        *t = seconds;
    }
    seconds
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn open(path: *const c_char, _flags: c_int, _mode: c_int) -> c_int {
    let path_str = core::ffi::CStr::from_ptr(path).to_string_lossy();
    
    if let Ok(f) = std::fs::File::open(&path_str) {
        let fd = f.as_raw_fd();
        core::mem::forget(f);
        fd as c_int
    } else {
        
        
        
        if let Ok(f) = std::fs::File::create(&path_str) {
            let fd = f.as_raw_fd();
            core::mem::forget(f);
            fd as c_int
        } else {
            -1
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ioctl(fd: c_int, request: c_ulong, mut args: ...) -> c_int {
    let arg = args.arg::<u64>();
    std::os::syscall(16, fd as u64, request as u64, arg) as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fcntl(_fd: c_int, _cmd: c_int, ...) -> c_int { 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn close(fd: c_int) -> c_int {
    std::os::file_close(fd as usize) as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize {
    let slice = core::slice::from_raw_parts_mut(buf as *mut u8, count);
    let res = std::os::file_read(fd as usize, slice);
    if res == usize::MAX { -1 } else { res as isize }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn write(fd: c_int, buf: *const c_void, count: usize) -> isize {
    let slice = core::slice::from_raw_parts(buf as *const u8, count);
    let res = std::os::file_write(fd as usize, slice);
    if res == usize::MAX { -1 } else { res as isize }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn access(path: *const c_char, _mode: c_int) -> c_int {
    let path_str = core::ffi::CStr::from_ptr(path).to_string_lossy();
    if let Ok(f) = std::fs::File::open(&path_str) {
        core::mem::drop(f);
        0
    } else {
        -1
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn isatty(fd: c_int) -> c_int {
    if fd >= 0 && fd <= 2 { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpid() -> c_int {
    std::os::syscall(39, 0, 0, 0) as c_int // SYS_GETPID
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn unlink(path: *const c_char) -> c_int {
    let path_str = core::ffi::CStr::from_ptr(path).to_string_lossy();
    if std::fs::remove_file(&path_str).is_ok() { 0 } else { -1 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gethostname(name: *mut c_char, len: usize) -> c_int {
    let host = b"krakeos\0";
    if len < host.len() { return -1; }
    core::ptr::copy_nonoverlapping(host.as_ptr(), name as *mut u8, host.len());
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fsync(_fd: c_int) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fchown(_fd: c_int, _owner: u32, _group: u32) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chmod(_path: *const c_char, _mode: u32) -> c_int { 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wait(status: *mut c_int) -> c_int {
    waitpid(-1, status, 0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn waitpid(pid: c_int, status: *mut c_int, _options: c_int) -> c_int {
    let res = std::os::waitpid(pid as usize);
    if !status.is_null() {
        *status = (res as c_int) << 8; 
    }
    pid
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fork() -> c_int { -1 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pipe(fds: *mut c_int) -> c_int {
    let mut safe_fds = [0i32; 2];
    let res = std::os::pipe(&mut safe_fds);
    if res == 0 {
        *fds.add(0) = safe_fds[0];
        *fds.add(1) = safe_fds[1];
    }
    res
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dup2(_oldfd: c_int, _newfd: c_int) -> c_int { -1 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcsetattr(_fd: c_int, _opt: c_int, _termios: *const c_void) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcgetattr(_fd: c_int, _termios: *mut c_void) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn execl(_path: *const c_char, _arg0: *const c_char, ...) -> c_int { -1 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getuid() -> u32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn geteuid() -> u32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getgid() -> u32 { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getegid() -> u32 { 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpwuid(_uid: u32) -> *mut c_void { core::ptr::null_mut() }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn chdir(path: *const c_char) -> c_int {
    let path_str = core::ffi::CStr::from_ptr(path).to_string_lossy();
    if std::os::syscall(80, path_str.as_ptr() as u64, path_str.len() as u64, 0) == 0 { 0 } else { -1 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getcwd(buf: *mut c_char, size: usize) -> *mut c_char {
    let root = b"/\0";
    if size >= root.len() && !buf.is_null() {
        core::ptr::copy_nonoverlapping(root.as_ptr(), buf as *mut u8, root.len());
        buf
    } else {
        core::ptr::null_mut()
    }
}
