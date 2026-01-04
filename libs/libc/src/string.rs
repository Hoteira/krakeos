use core::ffi::{c_char, c_int, c_uint};
use crate::stdlib::malloc;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(s: *mut core::ffi::c_void, c: c_int, n: usize) -> *mut core::ffi::c_void {
    core::arch::asm!("rep stosb", inout("rdi") s => _, in("al") c as u8, inout("rcx") n => _, options(nostack, preserves_flags));
    s
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(d: *mut core::ffi::c_void, s: *const core::ffi::c_void, n: usize) -> *mut core::ffi::c_void {
    core::arch::asm!("rep movsb", inout("rdi") d => _, inout("rsi") s => _, inout("rcx") n => _, options(nostack, preserves_flags));
    d
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(d: *mut core::ffi::c_void, s: *const core::ffi::c_void, n: usize) -> *mut core::ffi::c_void {
    if d > s as *mut core::ffi::c_void {
        core::arch::asm!("std", "rep movsb", "cld", inout("rdi") (d as usize + n).wrapping_sub(1) => _, inout("rsi") (s as usize + n).wrapping_sub(1) => _, inout("rcx") n => _, options(nostack));
    } else {
        core::arch::asm!("rep movsb", inout("rdi") d => _, inout("rsi") s => _, inout("rcx") n => _, options(nostack, preserves_flags));
    }
    d
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strlen(s: *const c_char) -> usize {
    let mut l = 0;
    while *s.add(l) != 0 { l += 1; }
    l
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcmp(s1: *const c_char, s2: *const c_char) -> c_int {
    let mut i = 0;
    loop {
        let c1 = *s1.add(i) as u8;
        let c2 = *s2.add(i) as u8;
        if c1 != c2 { return (c1 as c_int) - (c2 as c_int); }
        if c1 == 0 { return 0; }
        i += 1;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strncmp(s1: *const c_char, s2: *const c_char, n: usize) -> c_int {
    let mut i = 0;
    while i < n {
        let c1 = *s1.add(i) as u8;
        let c2 = *s2.add(i) as u8;
        if c1 != c2 { return (c1 as c_int) - (c2 as c_int); }
        if c1 == 0 { return 0; }
        i += 1;
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcasecmp(s1: *const c_char, s2: *const c_char) -> c_int {
    let mut i = 0;
    loop {
        let c1 = crate::misc::toupper(*s1.add(i) as c_int);
        let c2 = crate::misc::toupper(*s2.add(i) as c_int);
        if c1 != c2 { return c1 - c2; }
        if c1 == 0 { return 0; }
        i += 1;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strncasecmp(s1: *const c_char, s2: *const c_char, n: usize) -> c_int {
    let mut i = 0;
    while i < n {
        let c1 = crate::misc::toupper(*s1.add(i) as c_int);
        let c2 = crate::misc::toupper(*s2.add(i) as c_int);
        if c1 != c2 { return c1 - c2; }
        if c1 == 0 { return 0; }
        i += 1;
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcasestr(haystack: *const c_char, needle: *const c_char) -> *mut c_char {
    let mut h = haystack;
    let n_len = strlen(needle);
    if n_len == 0 { return haystack as *mut c_char; }
    while *h != 0 {
        if strncasecmp(h, needle, n_len) == 0 { return h as *mut c_char; }
        h = h.add(1);
    }
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcpy(d: *mut c_char, s: *const c_char) -> *mut c_char {
    let mut i = 0;
    loop {
        let c = *s.add(i);
        *d.add(i) = c;
        if c == 0 { break; }
        i += 1;
    }
    d
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strncpy(d: *mut c_char, s: *const c_char, n: usize) -> *mut c_char {
    let mut i = 0;
    while i < n {
        let c = *s.add(i);
        if c == 0 { break; }
        *d.add(i) = c; i += 1;
    }
    while i < n {
        *d.add(i) = 0; i += 1;
    }
    d
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcat(dest: *mut c_char, src: *const c_char) -> *mut c_char {
    let mut d = dest;
    while *d != 0 { d = d.add(1); }
    strcpy(d, src);
    dest
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strpbrk(s: *const c_char, accept: *const c_char) -> *mut c_char {
    let mut s_ptr = s;
    while *s_ptr != 0 {
        let mut a_ptr = accept;
        while *a_ptr != 0 {
            if *s_ptr == *a_ptr {
                return s_ptr as *mut c_char;
            }
            a_ptr = a_ptr.add(1);
        }
        s_ptr = s_ptr.add(1);
    }
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strerror(errnum: c_int) -> *mut c_char {
    match errnum {
        0 => b"Success\0".as_ptr() as *mut c_char,
        2 => b"No such file or directory\0".as_ptr() as *mut c_char,
        4 => b"Interrupted system call\0".as_ptr() as *mut c_char,
        21 => b"Is a directory\0".as_ptr() as *mut c_char,
        34 => b"Numerical result out of range\0".as_ptr() as *mut c_char,
        _ => b"Unknown error\0".as_ptr() as *mut c_char,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strchr(s: *const c_char, c: c_int) -> *mut c_char {
    let mut p = s;
    while *p != 0 {
        if *p as c_int == c { return p as *mut c_char; }
        p = p.add(1);
    }
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strrchr(s: *const c_char, c: c_int) -> *mut c_char {
    let mut res = core::ptr::null_mut();
    let mut p = s;
    while *p != 0 {
        if *p as c_int == c { res = p as *mut c_char; }
        p = p.add(1);
    }
    res
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn basename(path: *mut c_char) -> *mut c_char {
    if path.is_null() || *path == 0 {
        return b".\0".as_ptr() as *mut c_char;
    }
    let len = strlen(path);
    let mut end = len as isize - 1;
    while end >= 0 && *path.offset(end) == b'/' as c_char {
        *path.offset(end) = 0;
        end -= 1;
    }
    if end < 0 {
        return b"/\0".as_ptr() as *mut c_char;
    }
    if let Some(slash) = (0..=end).rev().find(|&i| *path.offset(i) == b'/' as c_char) {
        return path.offset(slash + 1);
    }
    path
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn dirname(path: *mut c_char) -> *mut c_char {
    if path.is_null() || *path == 0 {
        return b".\0".as_ptr() as *mut c_char;
    }
    let len = strlen(path);
    let mut end = len as isize - 1;
    while end >= 0 && *path.offset(end) == b'/' as c_char {
        *path.offset(end) = 0;
        end -= 1;
    }
    if end < 0 {
        return b"/\0".as_ptr() as *mut c_char;
    }
    if let Some(slash) = (0..=end).rev().find(|&i| *path.offset(i) == b'/' as c_char) {
        if slash == 0 {
            *path.offset(1) = 0;
            return path;
        }
        let mut d_end = slash as isize - 1;
        while d_end >= 0 && *path.offset(d_end) == b'/' as c_char {
            d_end -= 1;
        }
        if d_end < 0 {
            *path.offset(1) = 0;
        } else {
            *path.offset(d_end + 1) = 0;
        }
        return path;
    }
    b".\0".as_ptr() as *mut c_char
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strstr(haystack: *const c_char, needle: *const c_char) -> *mut c_char {
    let mut h = haystack;
    let n_len = strlen(needle);
    if n_len == 0 { return haystack as *mut c_char; }
    while *h != 0 {
        if strncmp(h, needle, n_len) == 0 { return h as *mut c_char; }
        h = h.add(1);
    }
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strdup(s: *const c_char) -> *mut c_char {
    let len = strlen(s);
    let ptr = malloc(len + 1) as *mut c_char;
    if !ptr.is_null() { strcpy(ptr, s); }
    ptr
}
