use core::ffi::{c_char, c_int, c_long, c_void};
use crate::string::strlen;

#[repr(C)]
pub struct DIR { pub fd: c_int }

#[repr(C)]
pub struct dirent {
    pub d_ino: c_long,
    pub d_name: [c_char; 256],
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn opendir(name: *const c_char) -> *mut DIR {
    let fd = crate::unistd::open(name, 0);
    if fd < 0 { return core::ptr::null_mut(); }
    let dir = crate::stdlib::malloc(core::mem::size_of::<DIR>()) as *mut DIR;
    (*dir).fd = fd;
    dir
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn readdir(dirp: *mut DIR) -> *mut dirent {
    static mut DE: dirent = dirent { d_ino: 0, d_name: [0; 256] };
    let mut buf = [0u8; 512];
    let res = std::os::syscall(64, (*dirp).fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64);
    if res == u64::MAX || res == 0 { return core::ptr::null_mut(); }
    let name_len = buf[1] as usize;
    core::ptr::write_bytes((*(&raw mut DE)).d_name.as_mut_ptr(), 0, 256);
    core::ptr::copy_nonoverlapping(buf.as_ptr().add(2), (*(&raw mut DE)).d_name.as_mut_ptr() as *mut u8, name_len);
    &raw mut DE
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn closedir(dirp: *mut DIR) -> c_int {
    let fd = (*dirp).fd;
    crate::stdlib::free(dirp as *mut c_void);
    crate::unistd::close(fd)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn stat(path: *const c_char, buf: *mut c_void) -> c_int {
    let mut stats = [0u64; 2];
    let resolved = crate::misc::resolve_path_rust(path);
    std::os::debug_print("stat: ");
    std::os::debug_print(&resolved);
    std::os::debug_print("\n");

    if std::os::stat(&resolved, &mut stats) == 0 {
        let size = stats[0];
        let kind = stats[1];
        let mode = match kind {
            1 => 0o100644, // S_IFREG
            2 => 0o040755, // S_IFDIR
            3 => 0o020666, // S_IFCHR
            _ => 0,
        };
        let p = buf as *mut u8;
        core::ptr::write_unaligned(p as *mut u64, 0); // st_ino (8)
        core::ptr::write_unaligned(p.add(8) as *mut u32, mode); // st_mode (4)
        core::ptr::write_unaligned(p.add(12) as *mut u32, 0); // __pad0 (4)
        core::ptr::write_unaligned(p.add(16) as *mut u64, size); // st_size (8)
        core::ptr::write_unaligned(p.add(24) as *mut u32, 0); // st_uid (4)
        core::ptr::write_unaligned(p.add(28) as *mut u32, 0); // st_gid (4)
        return 0;
    }
    -1
}
