use core::ffi::{c_void, c_int, c_long};
use core::alloc::Layout;
use crate::string::strlen;

#[repr(C)]
struct Header { size: usize }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void {
    let total = size + core::mem::size_of::<Header>();
    let layout = Layout::from_size_align(total, 8).unwrap();
    let ptr = alloc::alloc::alloc(layout);
    if ptr.is_null() { return core::ptr::null_mut(); }
    let header = ptr as *mut Header;
    (*header).size = size;
    ptr.add(core::mem::size_of::<Header>()) as *mut c_void
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn free(ptr: *mut c_void) {
    if ptr.is_null() { return; }
    let real = (ptr as *mut u8).sub(core::mem::size_of::<Header>());
    let size = (*(real as *mut Header)).size;
    alloc::alloc::dealloc(real, Layout::from_size_align(size + core::mem::size_of::<Header>(), 8).unwrap());
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
pub unsafe extern "C" fn atoi(s: *const core::ffi::c_char) -> c_int {
    let mut res = 0;
    let mut p = s;
    while *p >= b'0' as i8 && *p <= b'9' as i8 {
        res = res * 10 + (*p - b'0' as i8) as c_int;
        p = p.add(1);
    }
    res
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtol(s: *const core::ffi::c_char, endptr: *mut *mut core::ffi::c_char, base: c_int) -> c_long {
    let mut res: i64 = 0;
    let mut p = s;
    let mut neg = false;
    if *p == b'-' as i8 { neg = true; p = p.add(1); }
    else if *p == b'+' as i8 { p = p.add(1); }
    let b = if base == 0 {
        if *p == b'0' as i8 {
            if *p.add(1) == b'x' as i8 || *p.add(1) == b'X' as i8 { p = p.add(2); 16 }
            else { 8 }
        } else { 10 }
    } else { base as u64 };
    loop {
        let c = *p as u8;
        let v = match c {
            b'0'..=b'9' => (c - b'0') as i64,
            b'a'..=b'z' => (c - b'a' + 10) as i64,
            b'A'..=b'Z' => (c - b'A' + 10) as i64,
            _ => break,
        };
        if v >= b as i64 { break; }
        res = res * b as i64 + v;
        p = p.add(1);
    }
    if !endptr.is_null() { *endptr = p as *mut core::ffi::c_char; }
    if neg { -res as c_long } else { res as c_long }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn atof(s: *const core::ffi::c_char) -> f64 {
    let mut res: f64 = 0.0;
    let mut div: f64 = 1.0;
    let mut p = s;
    let mut dot = false;
    while *p != 0 {
        let c = *p as u8;
        if c == b'.' { dot = true; }
        else if c >= b'0' && c <= b'9' {
            if !dot { res = res * 10.0 + (c - b'0') as f64; }
            else { div *= 10.0; res += (c - b'0') as f64 / div; }
        }
        p = p.add(1);
    }
    res
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn abs(j: c_int) -> c_int { if j < 0 { -j } else { j } }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn exit(status: c_int) -> ! {
    std::os::syscall(60, status as u64, 0, 0);
    loop {}
}
