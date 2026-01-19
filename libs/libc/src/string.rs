use crate::ctype::toupper;
use core::ffi::{c_char, c_int, c_void};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(s: *mut c_void, c: c_int, n: usize) -> *mut c_void {
    core::arch::asm!(
    "rep stosb",
    inout("rdi") s => _,
    in("al") c as u8,
    inout("rcx") n => _,
    options(nostack, preserves_flags)
    );

    s
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(d: *mut c_void, s: *const c_void, n: usize) -> *mut c_void {
    core::arch::asm!(
    "rep movsb",
    inout("rdi") d => _,
    inout("rsi") s => _,
    inout("rcx") n => _,
    options(nostack, preserves_flags)
    );

    d
}


#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(d: *mut c_void, s: *const c_void, n: usize) -> *mut c_void {
    if d > s as *mut c_void {
        core::arch::asm!(
        "std",
        "rep movsb",
        "cld",
        inout("rdi") (d as usize + n).wrapping_sub(1) => _,
        inout("rsi") (s as usize + n).wrapping_sub(1) => _,
        inout("rcx") n => _,
        options(nostack)
        );
    } else {
        core::arch::asm!(
        "rep movsb",
        inout("rdi") d => _,
        inout("rsi") s => _,
        inout("rcx") n => _,
        options(nostack, preserves_flags)
        );
    }

    d
}


#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(s1: *const c_void, s2: *const c_void, n: usize) -> c_int {
    let p1 = s1 as *const u8;
    let p2 = s2 as *const u8;
    for i in 0..n {
        let a = *p1.add(i);
        let b = *p2.add(i);
        if a != b {
            return (a as c_int) - (b as c_int);
        }
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memchr(s: *const c_void, c: c_int, n: usize) -> *mut c_void {
    let p = s as *const u8;
    for i in 0..n {
        if *p.add(i) == c as u8 {
            return p.add(i) as *mut c_void;
        }
    }
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strspn(s: *const c_char, accept: *const c_char) -> usize {
    let mut i = 0;
    while *s.add(i) != 0 {
        if strchr(accept, *s.add(i) as c_int).is_null() {
            return i;
        }
        i += 1;
    }
    i
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcspn(s: *const c_char, reject: *const c_char) -> usize {
    let mut i = 0;
    while *s.add(i) != 0 {
        if !strchr(reject, *s.add(i) as c_int).is_null() {
            return i;
        }
        i += 1;
    }
    i
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcoll(s1: *const c_char, s2: *const c_char) -> c_int {
    strcmp(s1, s2)
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
        let c1 = toupper(*s1.add(i) as c_int);
        let c2 = toupper(*s2.add(i) as c_int);
        if c1 != c2 { return c1 - c2; }
        if c1 == 0 { return 0; }
        i += 1;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strncasecmp(s1: *const c_char, s2: *const c_char, n: usize) -> c_int {
    let mut i = 0;
    while i < n {
        let c1 = toupper(*s1.add(i) as c_int);
        let c2 = toupper(*s2.add(i) as c_int);
        if c1 != c2 { return c1 - c2; }
        if c1 == 0 { return 0; }
        i += 1;
    }
    0
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

        *d.add(i) = c;
        i += 1;
    }

    while i < n {
        *d.add(i) = 0;
        i += 1;
    }

    d
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
    let ptr = crate::stdlib::malloc(len + 1) as *mut c_char;
    if !ptr.is_null() { strcpy(ptr, s); }
    ptr
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
pub unsafe extern "C" fn strpbrk(s: *const c_char, accept: *const c_char) -> *mut c_char {
    let mut s_ptr = s;
    while *s_ptr != 0 {
        let mut a_ptr = accept;
        while *a_ptr != 0 {
            if *s_ptr == *a_ptr { return s_ptr as *mut c_char; }
            a_ptr = a_ptr.add(1);
        }
        s_ptr = s_ptr.add(1);
    }
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strerror(_errnum: c_int) -> *mut c_char {
    b"Unknown error\0".as_ptr() as *mut c_char
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcat(dest: *mut c_char, src: *const c_char) -> *mut c_char {
    let dest_len = strlen(dest);
    strcpy(dest.add(dest_len), src);
    dest
}
