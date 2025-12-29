#![feature(c_variadic)]
#![no_std]
#![feature(naked_functions)]

#[macro_use]
extern crate std;
extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::format;
use core::ffi::{c_void, c_char, c_int, c_long, c_double, VaList, c_uint};
use core::alloc::{Layout, GlobalAlloc};

unsafe extern "C" {
    fn main(argc: c_int, argv: *mut *mut c_char) -> c_int;
}


#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        "mov rdi, rsp",
        "and rsp, -16",
        "call rust_start",
        "hlt",
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rust_start(stack: *const usize) -> ! {
    let size = 128 * 1024 * 1024;
    let ptr = std::memory::malloc(size) as *mut u8;
    std::memory::heap::init_heap(ptr, size);
    let argc = *stack as c_int;
    let argv = stack.add(1) as *mut *mut c_char;
    let result = main(argc, argv);
    exit(result);
}

#[unsafe(no_mangle)]
pub static mut errno: c_int = 0;

#[repr(C)]
struct Header { size: usize }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void {
    std::println!("malloc({})", size);
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

#[unsafe(no_mangle)] pub unsafe extern "C" fn strcpy(d: *mut c_char, s: *const c_char) -> *mut c_char {
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
    let ptr = malloc(len + 1) as *mut c_char;
    if !ptr.is_null() { strcpy(ptr, s); }
    ptr
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn toupper(c: c_int) -> c_int { if c >= b'a' as c_int && c <= b'z' as c_int { c - 32 } else { c } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn tolower(c: c_int) -> c_int { if c >= b'A' as c_int && c <= b'Z' as c_int { c + 32 } else { c } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn isspace(c: c_int) -> c_int { if c == b' ' as c_int || c == b'\t' as c_int || c == b'\n' as c_int || c == b'\r' as c_int || c == 0x0B || c == 0x0C { 1 } else { 0 } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn isdigit(c: c_int) -> c_int { if c >= b'0' as c_int && c <= b'9' as c_int { 1 } else { 0 } }

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

unsafe fn write_padded(output: &mut impl FnMut(u8), s: &[u8], width: usize, zero_pad: bool, written: &mut c_int) {
    let len = s.len();
    if len < width {
        let pad_char = if zero_pad { b'0' } else { b' ' };
        for _ in 0..(width - len) {
            output(pad_char);
            *written += 1;
        }
    }
    for &b in s {
        output(b);
        *written += 1;
    }
}

fn itoa(mut n: u64, buf: &mut [u8], base: u64, uppercase: bool) -> usize {
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut len = 0;
    let mut temp = n;
    while temp > 0 {
        temp /= base;
        len += 1;
    }
    let mut i = len;
    while n > 0 {
        let d = (n % base) as u8;
        i -= 1;
        buf[i] = if d < 10 { d + b'0' } else { d - 10 + (if uppercase { b'A' } else { b'a' }) };
        n /= base;
    }
    len
}

fn itoa_signed(n: i64, buf: &mut [u8]) -> usize {
    if n < 0 {
        buf[0] = b'-';
        1 + itoa((-n) as u64, &mut buf[1..], 10, false)
    } else {
        itoa(n as u64, buf, 10, false)
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn snprintf(str: *mut c_char, size: usize, fmt: *const c_char, mut args: ...) -> c_int {
    vsnprintf(str, size, fmt, args.as_va_list())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sprintf(str: *mut c_char, fmt: *const c_char, mut args: ...) -> c_int {
    vsnprintf(str, usize::MAX, fmt, args.as_va_list())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vsnprintf(str: *mut c_char, size: usize, fmt: *const c_char, mut ap: VaList) -> c_int {
    let mut written = 0;
    printf_core(|b| {
        if written < size.saturating_sub(1) {
            *str.add(written) = b as c_char;
        }
        written += 1;
    }, fmt, &mut ap);

    std::println!("Formatted string: {}", unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(str as *const u8, written as usize)) });

    if size > 0 {
        *str.add(core::cmp::min(written, size - 1)) = 0;
    }
    written as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sscanf(str: *const c_char, _fmt: *const c_char, ...) -> c_int {
    let s = core::str::from_utf8_unchecked(core::slice::from_raw_parts(str as *const u8, strlen(str)));
    if let Ok(v) = s.trim().parse::<i32>() { v } else { 0 }
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn putchar(c: c_int) -> c_int { let b = [c as u8]; std::os::print(core::str::from_utf8_unchecked(&b)); c }
#[unsafe(no_mangle)] pub unsafe extern "C" fn puts(s: *const c_char) -> c_int { printf(s); putchar(b'\n' as i32); 0 }

#[unsafe(no_mangle)] 
pub unsafe extern "C" fn printf(f: *const c_char, mut args: ...) -> c_int {
    vfprintf(core::ptr::null_mut(), f, args.as_va_list())
}

#[unsafe(no_mangle)] 
pub unsafe extern "C" fn fprintf(_s: *mut c_void, f: *const c_char, mut args: ...) -> c_int { 
    vfprintf(_s, f, args.as_va_list())
}

#[unsafe(no_mangle)] 
pub unsafe extern "C" fn vfprintf(_st: *mut c_void, f: *const c_char, mut ap: VaList) -> c_int { 
     printf_core(|b| {
         let buf = [b];
         std::os::print(core::str::from_utf8_unchecked(&buf));
     }, f, &mut ap)
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn fflush(_s: *mut c_void) -> c_int { 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fopen(filename: *const c_char, _mode: *const c_char) -> *mut c_void {
    let path = core::str::from_utf8_unchecked(core::slice::from_raw_parts(filename as *const u8, strlen(filename)));
    if let Ok(file) = std::fs::File::open(path) { Box::into_raw(Box::new(file)) as *mut c_void }
    else { core::ptr::null_mut() }
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn fclose(s: *mut c_void) -> c_int { if !s.is_null() { drop(Box::from_raw(s as *mut std::fs::File)); 0 } else { -1 } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn fread(p: *mut c_void, s: usize, n: usize, st: *mut c_void) -> usize { if st.is_null() { return 0; } let f = &mut *(st as *mut std::fs::File); if let Ok(got) = f.read(core::slice::from_raw_parts_mut(p as *mut u8, s * n)) { got / s } else { 0 } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn fwrite(p: *const c_void, s: usize, n: usize, st: *mut c_void) -> usize { if st.is_null() { return 0; } let f = &mut *(st as *mut std::fs::File); if let Ok(put) = f.write(core::slice::from_raw_parts(p as *const u8, s * n)) { put / s } else { 0 } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn fseek(st: *mut c_void, o: c_long, w: c_int) -> c_int { if st.is_null() { return -1; } let f = &mut *(st as *mut std::fs::File); if std::os::file_seek(f.as_raw_fd(), o as i64, w as usize) != u64::MAX { 0 } else { -1 } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn ftell(st: *mut c_void) -> c_long { if st.is_null() { return -1; } let f = &mut *(st as *mut std::fs::File); let r = std::os::file_seek(f.as_raw_fd(), 0, 1); if r != u64::MAX { r as c_long } else { -1 } }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn stat(path: *const c_char, buf: *mut c_void) -> c_int {
    if let Ok(file) = std::fs::File::open(core::str::from_utf8_unchecked(core::slice::from_raw_parts(path as *const u8, strlen(path)))) {
        let size = file.size();
        
        *((buf as usize + 48) as *mut u64) = size as u64;
        return 0;
    }
    -1
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn system(_c: *const c_char) -> c_int { 0 }
#[unsafe(no_mangle)] pub unsafe extern "C" fn mkdir(_p: *const c_char, _m: u32) -> c_int { 0 }
#[unsafe(no_mangle)] pub unsafe extern "C" fn remove(_p: *const c_char) -> c_int { 0 }
#[unsafe(no_mangle)] pub unsafe extern "C" fn rename(_o: *const c_char, _n: *const c_char) -> c_int { 0 }
#[unsafe(no_mangle)] pub unsafe extern "C" fn exit(s: c_int) -> ! { std::os::exit(s as u64) }
#[unsafe(no_mangle)] pub unsafe extern "C" fn getenv(_n: *const c_char) -> *mut c_char { core::ptr::null_mut() }
#[unsafe(no_mangle)] pub unsafe extern "C" fn time(_t: *mut c_long) -> c_long { 0 }

#[unsafe(no_mangle)] pub unsafe extern "C" fn sqrt(x: f64) -> f64 { let mut r: f64; core::arch::asm!("sqrtsd {}, {}", out(xmm_reg) r, in(xmm_reg) x); r }
#[unsafe(no_mangle)] pub unsafe extern "C" fn fabs(x: f64) -> f64 { if x < 0.0 { -x } else { x } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn sin(_x: f64) -> f64 { 0.0 }
#[unsafe(no_mangle)] pub unsafe extern "C" fn cos(_x: f64) -> f64 { 0.0 }
#[unsafe(no_mangle)] pub unsafe extern "C" fn tan(_x: f64) -> f64 { 0.0 }
#[unsafe(no_mangle)] pub unsafe extern "C" fn atan(_x: f64) -> f64 { 0.0 }
#[unsafe(no_mangle)] pub unsafe extern "C" fn ceil(_x: f64) -> f64 { 0.0 }
#[unsafe(no_mangle)] pub unsafe extern "C" fn floor(_x: f64) -> f64 { 0.0 }
#[unsafe(no_mangle)] pub unsafe extern "C" fn pow(_b: f64, _e: f64) -> f64 { 0.0 }

#[unsafe(no_mangle)] pub unsafe extern "C" fn krake_syscall(n: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> u64 { std::os::syscall4(n, a1, a2, a3, a4) }
#[unsafe(no_mangle)] pub unsafe extern "C" fn krake_sleep(ms: usize) {
    std::os::sleep(ms as u64);
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn usleep(usec: c_uint) -> c_int {
    let ms = (usec + 999) / 1000;
    krake_sleep(ms as usize);
    0
}
#[unsafe(no_mangle)] pub unsafe extern "C" fn krake_get_time_ms() -> usize { std::os::syscall(55, 0, 0, 0) as usize }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn krake_get_event(wid: usize, out_event: *mut u32) -> c_int {
    let mut buf = [0u64; 8];
    
    if krake_syscall(52, wid as u64, buf.as_mut_ptr() as u64, 1, 0) == 1 {
        let tag = buf[0] as u32; 
        *out_event.add(0) = tag; 
        
        let data_ptr = buf.as_ptr().add(1) as *const u32; 
        
        match tag {
            0 => { 
                
                
                
                
                
                
                
                let x = *(buf.as_ptr().add(2)) as u32;
                let y = *(buf.as_ptr().add(3)) as u32;
                let buttons_scroll = *(buf.as_ptr().add(4)) as u32;
                
                *out_event.add(1) = x; 
                *out_event.add(2) = y; 
                *out_event.add(3) = buttons_scroll & 0xFFFFFF; 
                *out_event.add(4) = (buttons_scroll >> 24) & 0xFF; 
            },
            1 => { 
                
                
                
                
                
                let wid_key = *(buf.as_ptr().add(1));
                let key = (wid_key >> 32) as u32;
                let flags = *(buf.as_ptr().add(2));
                let pressed = (flags & 0xFF) as u32;
                let repeat = ((flags >> 8) & 0xFFFF) as u32;
                
                *out_event.add(1) = key;     
                *out_event.add(2) = repeat;  
                *out_event.add(3) = pressed; 
            },
            2 => { 
                let w = *(buf.as_ptr().add(2)) as u32;
                let h = *(buf.as_ptr().add(3)) as u32;
                *out_event.add(3) = w; 
                *out_event.add(4) = h; 
            },
            _ => {}
        }
        return 1;
    }
    0
}

#[repr(C)]
struct RawWindow {
    id: usize, buffer: usize, pid: u64, x: isize, y: isize, z: usize, width: usize, height: usize,
    can_move: bool, can_resize: bool, transparent: bool, treat_as_transparent: bool, min_width: usize, min_height: usize,
    event_handler: usize, w_type: u32,
}

static mut DOOM_WINDOW_BUFFER: usize = 0;
static mut DOOM_WINDOW_WIDTH: usize = 0;
static mut DOOM_WINDOW_HEIGHT: usize = 0;
static mut DOOM_WINDOW_TRANSPARENT: bool = true;
static mut DOOM_WINDOW_TREAT_AS_TRANSPARENT: bool = true;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn krake_window_create(width: usize, height: usize, transparent: bool, treat_as_transparent: bool) -> usize {
    let buffer_size = width * height * 4;
    let buffer = malloc(buffer_size) as usize;
    DOOM_WINDOW_BUFFER = buffer;
    DOOM_WINDOW_WIDTH = width;
    DOOM_WINDOW_HEIGHT = height;
    DOOM_WINDOW_TRANSPARENT = transparent;
    DOOM_WINDOW_TREAT_AS_TRANSPARENT = treat_as_transparent;
    let w = RawWindow {
        id: 0, buffer, pid: 0, x: 100, y: 100, z: 0, width, height,
        can_move: true, can_resize: false, transparent, treat_as_transparent, min_width: 0, min_height: 0,
        event_handler: 1, w_type: 3,
    };
    krake_syscall(22, &w as *const _ as u64, 0, 0, 0) as usize
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn krake_window_get_buffer(_wid: usize) -> *mut c_void { DOOM_WINDOW_BUFFER as *mut c_void }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn krake_window_draw(wid: usize) {
    let w = RawWindow {
        id: wid, buffer: DOOM_WINDOW_BUFFER, pid: 0, x: 0, y: 0, z: 0, 
        width: DOOM_WINDOW_WIDTH, height: DOOM_WINDOW_HEIGHT,
        can_move: false, can_resize: false, transparent: DOOM_WINDOW_TRANSPARENT, treat_as_transparent: DOOM_WINDOW_TREAT_AS_TRANSPARENT, min_width: 0, min_height: 0,
        event_handler: 0, w_type: 3,
    };
    krake_syscall(51, &w as *const _ as u64, 0, 0, 0);
}

#[panic_handler] pub fn panic(i: &core::panic::PanicInfo) -> ! { std::println!("[USER PANIC] {}", i); loop {} }

unsafe fn printf_core(mut output: impl FnMut(u8), fmt: *const c_char, args: &mut VaList) -> c_int {
    let mut p = fmt;
    let mut written = 0;
    let mut buf = [0u8; 64];

    while *p != 0 {
        if *p != b'%' as c_char {
            output(*p as u8);
            written += 1;
            p = p.add(1);
            continue;
        }
        p = p.add(1);

        let mut zero_pad = false;
        let mut width = 0;
        let mut precision = -1;
        let mut long_cnt = 0;
        let mut size_t_spec = false;

        while *p == b'0' as c_char {
            zero_pad = true;
            p = p.add(1);
        }

        while *p >= b'0' as c_char && *p <= b'9' as c_char {
            width = width * 10 + (*p as u8 - b'0') as usize;
            p = p.add(1);
        }

        if *p == b'.' as c_char {
            p = p.add(1);
            precision = 0;
            while *p >= b'0' as c_char && *p <= b'9' as c_char {
                precision = precision * 10 + (*p as i32 - b'0' as i32);
                p = p.add(1);
            }
        }

        loop {
            if *p == b'l' as c_char { long_cnt += 1; p = p.add(1); }
            else if *p == b'z' as c_char { size_t_spec = true; p = p.add(1); }
            else if *p == b'h' as c_char { p = p.add(1); } 
            else { break; }
        }

        let spec = *p;
        p = p.add(1);

        match spec as u8 {
            b'd' | b'i' => {
                let val = if size_t_spec { args.arg::<usize>() as i64 }
                else if long_cnt > 0 { args.arg::<i64>() }
                else { args.arg::<c_int>() as i64 };
                let len = itoa_signed(val, &mut buf);

                
                let final_len = if precision >= 0 && len < precision as usize {
                    let pad_count = precision as usize - len;
                    let is_negative = buf[0] == b'-';

                    if is_negative {
                        
                        let mut tmp = [0u8; 64];
                        tmp[0] = b'-';
                        for i in 0..pad_count {
                            tmp[1 + i] = b'0';
                        }
                        for i in 1..len {
                            tmp[pad_count + i] = buf[i];
                        }
                        buf[..pad_count + len].copy_from_slice(&tmp[..pad_count + len]);
                        pad_count + len
                    } else {
                        
                        let mut tmp = [0u8; 64];
                        for i in 0..pad_count {
                            tmp[i] = b'0';
                        }
                        tmp[pad_count..pad_count + len].copy_from_slice(&buf[..len]);
                        buf[..pad_count + len].copy_from_slice(&tmp[..pad_count + len]);
                        pad_count + len
                    }
                } else {
                    len
                };

                write_padded(&mut output, &buf[..final_len], width, zero_pad, &mut written);
            }
            b'u' => {
                let val = if size_t_spec { args.arg::<usize>() as u64 }
                else if long_cnt > 0 { args.arg::<u64>() }
                else { args.arg::<c_uint>() as u64 };
                let len = itoa(val, &mut buf, 10, false);

                
                let final_len = if precision >= 0 && len < precision as usize {
                    let pad_count = precision as usize - len;
                    let mut tmp = [0u8; 64];
                    for i in 0..pad_count {
                        tmp[i] = b'0';
                    }
                    tmp[pad_count..pad_count + len].copy_from_slice(&buf[..len]);
                    buf[..pad_count + len].copy_from_slice(&tmp[..pad_count + len]);
                    pad_count + len
                } else {
                    len
                };

                write_padded(&mut output, &buf[..final_len], width, zero_pad, &mut written);
            }
            b'x' | b'X' | b'p' => {
                let val = if spec == b'p' as c_char || size_t_spec { args.arg::<usize>() as u64 }
                else if long_cnt > 0 { args.arg::<u64>() }
                else { args.arg::<c_uint>() as u64 };
                let len = itoa(val, &mut buf, 16, spec == b'X' as c_char);

                
                let final_len = if precision >= 0 && len < precision as usize {
                    let pad_count = precision as usize - len;
                    let mut tmp = [0u8; 64];
                    for i in 0..pad_count {
                        tmp[i] = b'0';
                    }
                    tmp[pad_count..pad_count + len].copy_from_slice(&buf[..len]);
                    buf[..pad_count + len].copy_from_slice(&tmp[..pad_count + len]);
                    pad_count + len
                } else {
                    len
                };

                write_padded(&mut output, &buf[..final_len], width, zero_pad, &mut written);
            }
            b's' => {
                let ptr = args.arg::<*const c_char>();
                let s_slice = if ptr.is_null() {
                    "(null)".as_bytes()
                } else {
                    let len = strlen(ptr);
                    
                    let actual_len = if precision >= 0 && len > precision as usize {
                        precision as usize
                    } else {
                        len
                    };
                    core::slice::from_raw_parts(ptr as *const u8, actual_len)
                };
                write_padded(&mut output, s_slice, width, false, &mut written);
            }
            b'c' => {
                let c = args.arg::<c_int>() as u8;
                output(c);
                written += 1;
            }
            b'f' => {
                let _v = args.arg::<c_double>();
                
                let s = b"FLOAT";
                write_padded(&mut output, s, width, false, &mut written);
            }
            b'%' => {
                output(b'%');
                written += 1;
            }
            _ => {
                output(b'%');
                output(spec as u8);
                written += 2;
            }
        }
    }
    written
}