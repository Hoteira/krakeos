use core::ffi::{c_char, c_int, c_uint};
use alloc::string::String;
use crate::string::{strlen, strcpy, strcmp};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn setlocale(_category: c_int, _locale: *const c_char) -> *mut c_char {
    b"C\0".as_ptr() as *mut c_char
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nl_langinfo(_item: c_int) -> *mut c_char {
    b"UTF-8\0".as_ptr() as *mut c_char
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getenv(name: *const c_char) -> *mut c_char {
    let name_len = strlen(name);
    static mut ENV_BUF: [u8; 1024] = [0; 1024];
    let buf_ptr = core::ptr::addr_of_mut!(ENV_BUF) as *mut u8;
    let res = std::os::syscall4(84, name as u64, name_len as u64, buf_ptr as u64, 1024);
    if res != u64::MAX { buf_ptr as *mut c_char } else { core::ptr::null_mut() }
}

#[repr(C)]
pub struct passwd {
    pub pw_name: *mut c_char,
    pub pw_uid: c_int,
    pub pw_gid: c_int,
    pub pw_dir: *mut c_char,
    pub pw_shell: *mut c_char,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpwuid(_uid: c_uint) -> *mut passwd {
    static mut PW: passwd = passwd {
        pw_name: b"guest\0".as_ptr() as *mut c_char,
        pw_uid: 0,
        pw_gid: 0,
        pw_dir: b"@0xE0/users/guest\0".as_ptr() as *mut c_char,
        pw_shell: b"/sys/bin/shell.elf\0".as_ptr() as *mut c_char,
    };
    &raw mut PW
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn getpwent() -> *mut passwd { getpwuid(0) }
#[unsafe(no_mangle)] pub unsafe extern "C" fn endpwent() { }

pub unsafe fn resolve_path_rust(path: *const c_char) -> String {
    if path.is_null() { return String::new(); }
    let raw_p = core::str::from_utf8_unchecked(core::slice::from_raw_parts(path as *const u8, strlen(path)));
    let p = raw_p.trim();
    let absolute = if p.starts_with('@') { String::from(p) } else {
        let mut cwd_buf = [0i8; 4096];
        crate::unistd::getcwd(cwd_buf.as_mut_ptr(), 4096);
        let cwd_len = strlen(cwd_buf.as_ptr());
        let cwd = core::str::from_utf8_unchecked(core::slice::from_raw_parts(cwd_buf.as_ptr() as *const u8, cwd_len));
        let mut abs = String::from(cwd);
        if !abs.ends_with('/') && !p.starts_with('/') { abs.push('/'); }
        abs.push_str(p);
        abs
    };

    let mut parts = alloc::vec::Vec::new();
    let mut disk_prefix = String::new();
    let current_path = absolute.as_str();
    if let Some(idx) = current_path.find('/') {
        if current_path[..idx].contains('@') {
            disk_prefix = String::from(&current_path[..idx]);
            let rest = &current_path[idx..];
            for part in rest.split('/') {
                if part.is_empty() || part == "." { continue; }
                if part == ".." { parts.pop(); }
                else { parts.push(part); }
            }
        } else {
            for part in current_path.split('/') {
                if part.is_empty() || part == "." { continue; }
                if part == ".." { parts.pop(); }
                else { parts.push(part); }
            }
        }
    } else if current_path.contains('@') {
        disk_prefix = String::from(current_path);
    } else {
        parts.push(current_path);
    }

    let mut result = disk_prefix;
    result.push('/');
    for (i, part) in parts.iter().enumerate() {
        result.push_str(part);
        if i < parts.len() - 1 { result.push('/'); }
    }
    if p.ends_with('/') && !result.ends_with('/') && result.len() > 1 { result.push('/'); }
    result
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn toupper(c: c_int) -> c_int { if c >= b'a' as c_int && c <= b'z' as c_int { c - 32 } else { c } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn tolower(c: c_int) -> c_int { if c >= b'A' as c_int && c <= b'Z' as c_int { c + 32 } else { c } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn isspace(c: c_int) -> c_int { if c == b' ' as c_int || c == b'\t' as c_int || c == b'\n' as c_int || c == b'\r' as c_int || c == 0x0B || c == 0x0C { 1 } else { 0 } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn isdigit(c: c_int) -> c_int { if c >= b'0' as c_int && c <= b'9' as c_int { 1 } else { 0 } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn isxdigit(c: c_int) -> c_int { if (c >= b'0' as c_int && c <= b'9' as c_int) || (c >= b'a' as c_int && c <= b'f' as c_int) || (c >= b'A' as c_int && c <= b'F' as c_int) { 1 } else { 0 } }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mkstemps(template: *mut c_char, suffixlen: c_int) -> c_int {
    let len = strlen(template);
    if len < (6 + suffixlen as usize) { return -1; }
    let mut i = 0;
    loop {
        let ticks = std::os::get_system_ticks();
        for j in 0..6 {
            let rand_char = (b'a' + ((ticks.wrapping_add(i * j as u64)) % 26) as u8) as c_char;
            *template.add(len - (6 + suffixlen as usize) + j) = rand_char;
        }
        let fd = crate::unistd::open(template, 2 | 64); // O_RDWR | O_CREAT
        if fd >= 0 { return fd; }
        i += 1;
        if i > 100 { return -1; }
    }
}
