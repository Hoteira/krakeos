use core::ffi::{c_char, c_int, c_uint};
use crate::string::strlen;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn open(path: *const c_char, _flags: c_int, ...) -> c_int {
    let resolved = crate::misc::resolve_path_rust(path);
    std::os::debug_print("open: ");
    std::os::debug_print(&resolved);
    std::os::debug_print("\n");
    let res = std::os::syscall(61, resolved.as_ptr() as u64, resolved.len() as u64, 0);
    if res != u64::MAX { res as c_int } else { -1 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn read(fd: c_int, buf: *mut core::ffi::c_void, count: usize) -> isize {
    let res = std::os::file_read(fd as usize, core::slice::from_raw_parts_mut(buf as *mut u8, count));
    if res != usize::MAX { res as isize } else { -1 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn write(fd: c_int, buf: *const core::ffi::c_void, count: usize) -> isize {
    let res = std::os::file_write(fd as usize, core::slice::from_raw_parts(buf as *const u8, count));
    if res != usize::MAX { res as isize } else { -1 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn close(fd: c_int) -> c_int {
    std::os::file_close(fd as usize)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lseek(fd: c_int, offset: core::ffi::c_long, whence: c_int) -> core::ffi::c_long {
    let res = std::os::file_seek(fd as usize, offset as i64, whence as usize);
    if res != u64::MAX { res as core::ffi::c_long } else { -1 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn isatty(fd: c_int) -> c_int {
    if std::os::isatty(fd as usize) { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn access(path: *const c_char, _mode: c_int) -> c_int {
    let mut stats = [0u64; 2];
    let resolved = crate::misc::resolve_path_rust(path);
    if std::os::stat(&resolved, &mut stats) == 0 { 0 } else { -1 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpid() -> c_int {
    std::os::syscall(85, 0, 0, 0) as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn geteuid() -> c_uint {
    std::os::syscall(86, 0, 0, 0) as c_uint
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gethostname(name: *mut c_char, len: usize) -> c_int {
    let host = b"krakeos\0";
    let to_copy = core::cmp::min(len, host.len());
    core::ptr::copy_nonoverlapping(host.as_ptr() as *const c_char, name, to_copy);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getcwd(buf: *mut c_char, size: usize) -> *mut c_char {
    let pwd = crate::misc::getenv(b"PWD\0".as_ptr() as *const c_char);
    if pwd.is_null() {
        let cwd = b"@0xE0/\0";
        if size < cwd.len() { return core::ptr::null_mut(); }
        core::ptr::copy_nonoverlapping(cwd.as_ptr() as *const c_char, buf, cwd.len());
        return buf;
    }
    let len = strlen(pwd);
    if size < len + 1 { return core::ptr::null_mut(); }
    crate::string::strcpy(buf, pwd);
    buf
}

#[unsafe(no_mangle)] 
pub unsafe extern "C" fn usleep(usec: c_uint) -> c_int {
    std::os::syscall(76, usec as u64, 0, 0);
    0
}
