use core::alloc::Layout;
use core::ffi::{c_char, c_int, c_void};

#[repr(C, align(16))]
struct Header {
    size: usize,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void {
    if size == 0 { return core::ptr::null_mut(); }
    let header_size = core::mem::size_of::<Header>();
    let total = size + header_size;
    let layout = Layout::from_size_align(total, 16).unwrap();
    let ptr = alloc::alloc::alloc(layout);
    if ptr.is_null() { 
        std::debugln!("malloc({}) FAILED!", size);
        return core::ptr::null_mut(); 
    }
    let header = ptr as *mut Header;
    (*header).size = size;
    ptr.add(header_size) as *mut c_void
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn free(ptr: *mut c_void) {
    if ptr.is_null() { return; }
    let header_size = core::mem::size_of::<Header>();
    let real = (ptr as *mut u8).sub(header_size);
    let size = (*(real as *mut Header)).size;
    alloc::alloc::dealloc(real, Layout::from_size_align(size + header_size, 16).unwrap());
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn calloc(n: usize, s: usize) -> *mut c_void {
    let t = n * s;
    let p = malloc(t);

    if !p.is_null() { core::ptr::write_bytes(p as *mut u8, 0, t); }
    p
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
    if ptr.is_null() { return malloc(size); }
    let new = malloc(size);
    if new.is_null() { return core::ptr::null_mut(); }
    let old_size = (*((ptr as *mut u8).sub(core::mem::size_of::<Header>()) as *mut Header)).size;
    core::ptr::copy_nonoverlapping(ptr as *const u8, new as *mut u8, core::cmp::min(size, old_size));
    free(ptr);
    new
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn atoi(s: *const c_char) -> c_int {
    let mut res = 0;
    let mut p = s;
    while *p >= b'0' as i8 && *p <= b'9' as i8 {
        res = res * 10 + (*p - b'0' as i8) as c_int;
        p = p.add(1);
    }
    res
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn atof(s: *const c_char) -> f64 {
    let mut res: f64 = 0.0;
    let mut div: f64 = 1.0;
    let mut p = s;
    let mut dot = false;
    while *p != 0 {
        let c = *p as u8;
        if c == b'.' { dot = true; } else if c >= b'0' && c <= b'9' {
            if !dot { res = res * 10.0 + (c - b'0') as f64; } else {
                div *= 10.0;
                res += (c - b'0') as f64 / div;
            }
        }
        p = p.add(1);
    }
    res
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn abs(j: c_int) -> c_int { if j < 0 { -j } else { j } }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn abort() -> ! {
    std::os::exit(1)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtod(nptr: *const c_char, endptr: *mut *mut c_char) -> f64 {
    let res = atof(nptr);
    if !endptr.is_null() {
        let mut p = nptr;
        while *p != 0 {
            let c = *p as u8;
            if (c >= b'0' && c <= b'9') || c == b'.' || c == b'-' || c == b'+' || c == b'e' || c == b'E' {
                p = p.add(1);
            } else {
                break;
            }
        }
        *endptr = p as *mut c_char;
    }
    res
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn system(_c: *const c_char) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn exit(s: c_int) -> ! { std::os::exit(s as u64) }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getenv(name: *const c_char) -> *mut c_char {
    let s = core::ffi::CStr::from_ptr(name).to_string_lossy();
    if s == "TERM" {
        return b"xterm\0".as_ptr() as *mut c_char;
    }
    core::ptr::null_mut()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn putenv(_string: *mut c_char) -> c_int { 0 }

#[unsafe(no_mangle)]
pub static mut optarg: *mut c_char = core::ptr::null_mut();
#[unsafe(no_mangle)]
pub static mut optind: c_int = 1;
#[unsafe(no_mangle)]
pub static mut opterr: c_int = 1;
#[unsafe(no_mangle)]
pub static mut optopt: c_int = 0;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getopt_long(_argc: c_int, _argv: *mut *mut c_char, _optstring: *const c_char, _longopts: *const c_void, _longindex: *mut c_int) -> c_int {
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mkstemps(template: *mut c_char, suffix_len: c_int) -> c_int {
    let len = crate::string::strlen(template);
    if len < (6 + suffix_len as usize) { return -1; }
    let start = len - 6 - suffix_len as usize;
    for i in 0..6 {
        *template.add(start + i) = b'0' as c_char;
    }
    
    crate::unistd::open(template, 194, 0o600)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn realpath(path: *const c_char, resolved_path: *mut c_char) -> *mut c_char {
    let p_str = core::ffi::CStr::from_ptr(path).to_string_lossy();
    let mut resolved = alloc::string::String::new();

    if p_str == "." || p_str.is_empty() {
        resolved = alloc::string::String::from("/");
    } else if !p_str.starts_with('@') && !p_str.starts_with('/') {
        resolved = alloc::format!("@0xE0/{}", p_str);
    } else {
        resolved = p_str.into_owned();
    }

    let res = if !resolved_path.is_null() {
        resolved_path
    } else {
        crate::stdlib::malloc(resolved.len() + 1) as *mut c_char
    };

    if !res.is_null() {
        let bytes = resolved.as_bytes();
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), res as *mut u8, bytes.len());
        *res.add(bytes.len()) = 0;
    }
    res
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtol(nptr: *const c_char, endptr: *mut *mut c_char, base: c_int) -> i64 {
    let mut s = nptr;
    let mut acc: i64 = 0;
    let mut neg = false;

    
    while *s == 32 || (*s >= 9 && *s <= 13) { s = s.add(1); }

    
    if *s == b'-' as c_char {
        neg = true;
        s = s.add(1);
    } else if *s == b'+' as c_char {
        s = s.add(1);
    }

    let b = if base == 0 { 10 } else { base as i64 };

    loop {
        let c = *s as u8;
        let val;
        if c >= b'0' && c <= b'9' { val = (c - b'0') as i64; } else if c >= b'a' && c <= b'z' { val = (c - b'a' + 10) as i64; } else if c >= b'A' && c <= b'Z' { val = (c - b'A' + 10) as i64; } else { break; }

        if val >= b { break; }

        acc = acc * b + val;
        s = s.add(1);
    }

    if !endptr.is_null() { *endptr = s as *mut c_char; }

    if neg { -acc } else { acc }
}
