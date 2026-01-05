use crate::string::strlen;
use alloc::boxed::Box;
use core::ffi::{c_char, c_double, c_int, c_long, c_uint, c_void, VaList};

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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn putchar(c: c_int) -> c_int {
    let b = [c as u8];
    std::os::print(core::str::from_utf8_unchecked(&b));
    c
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn puts(s: *const c_char) -> c_int {
    printf(s);
    putchar(b'\n' as i32);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn krake_debug_printf(f: *const c_char, mut args: ...) -> c_int {
    printf_core(|b| {
        let buf = [b];
        std::os::debug_print(core::str::from_utf8_unchecked(&buf));
    }, f, &mut args.as_va_list())
}

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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fflush(_s: *mut c_void) -> c_int { 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fdopen(fd: c_int, _mode: *const c_char) -> *mut c_void {
    let file = std::fs::File::from_raw_fd(fd as usize);
    Box::into_raw(Box::new(file)) as *mut c_void
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fopen(filename: *const c_char, _mode: *const c_char) -> *mut c_void {
    let path = core::str::from_utf8_unchecked(core::slice::from_raw_parts(filename as *const u8, strlen(filename)));
    if let Ok(file) = std::fs::File::open(path) { Box::into_raw(Box::new(file)) as *mut c_void } else { core::ptr::null_mut() }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fclose(s: *mut c_void) -> c_int {
    if !s.is_null() {
        drop(Box::from_raw(s as *mut std::fs::File));
        0
    } else { -1 }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fread(p: *mut c_void, s: usize, n: usize, st: *mut c_void) -> usize {
    if st.is_null() { return 0; }
    let f = &mut *(st as *mut std::fs::File);
    if let Ok(got) = f.read(core::slice::from_raw_parts_mut(p as *mut u8, s * n)) { got / s } else { 0 }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fwrite(p: *const c_void, s: usize, n: usize, st: *mut c_void) -> usize {
    if st.is_null() { return 0; }
    let f = &mut *(st as *mut std::fs::File);
    if let Ok(put) = f.write(core::slice::from_raw_parts(p as *const u8, s * n)) { put / s } else { 0 }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fseek(st: *mut c_void, o: c_long, w: c_int) -> c_int {
    if st.is_null() { return -1; }
    let f = &mut *(st as *mut std::fs::File);
    if std::os::file_seek(f.as_raw_fd(), o as i64, w as usize) != u64::MAX { 0 } else { -1 }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ftell(st: *mut c_void) -> c_long {
    if st.is_null() { return -1; }
    let f = &mut *(st as *mut std::fs::File);
    let r = std::os::file_seek(f.as_raw_fd(), 0, 1);
    if r != u64::MAX { r as c_long } else { -1 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getc(stream: *mut c_void) -> c_int {
    if stream.is_null() { return -1; }
    let f = &mut *(stream as *mut std::fs::File);
    let mut buf = [0u8; 1];
    if let Ok(n) = f.read(&mut buf) {
        if n == 1 { buf[0] as c_int } else { -1 }
    } else { -1 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ferror(_stream: *mut c_void) -> c_int { 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn putc(c: c_int, stream: *mut c_void) -> c_int {
    let buf = [c as u8];
    if fwrite(buf.as_ptr() as *const c_void, 1, 1, stream) == 1 { c } else { -1 }
}

#[repr(C)]
struct Stat {
    st_dev: u64,
    st_ino: u64,
    st_mode: u32,
    _pad1: u32, // Alignment padding
    st_nlink: u64,
    st_uid: u32,
    st_gid: u32,
    st_rdev: u64,
    st_size: u64,
    st_blksize: u64,
    st_blocks: u64,
    st_atime: u64,
    st_mtime: u64,
    st_ctime: u64,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn stat(path: *const c_char, buf: *mut c_void) -> c_int {
    let mut p_str = core::ffi::CStr::from_ptr(path).to_string_lossy();
    if p_str.is_empty() {
        p_str = alloc::borrow::Cow::Borrowed("@0xE0/");
    }
    if let Ok(file) = std::fs::File::open(&p_str) {
        let size = file.size();
        let is_dir = std::fs::read_dir(&p_str).is_ok();

        let s = &mut *(buf as *mut Stat);
        core::ptr::write_bytes(buf as *mut u8, 0, core::mem::size_of::<Stat>());

        s.st_mode = if is_dir { 0040000 | 0777 } else { 0100000 | 0666 };
        s.st_size = size as u64;
        s.st_blksize = 1024;
        s.st_blocks = (size as u64 + 511) / 512;

        return 0;
    }
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mkdir(_p: *const c_char, _m: u32) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remove(_p: *const c_char) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rename(_o: *const c_char, _n: *const c_char) -> c_int { 0 }


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
            if *p == b'l' as c_char {
                long_cnt += 1;
                p = p.add(1);
            } else if *p == b'z' as c_char {
                size_t_spec = true;
                p = p.add(1);
            } else if *p == b'h' as c_char { p = p.add(1); } else { break; }
        }

        let spec = *p;
        p = p.add(1);

        match spec as u8 {
            b'd' | b'i' => {
                let val = if size_t_spec { args.arg::<usize>() as i64 } else if long_cnt > 0 { args.arg::<i64>() } else { args.arg::<c_int>() as i64 };
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
                let val = if size_t_spec { args.arg::<usize>() as u64 } else if long_cnt > 0 { args.arg::<u64>() } else { args.arg::<c_uint>() as u64 };
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
                let val = if spec == b'p' as c_char || size_t_spec { args.arg::<usize>() as u64 } else if long_cnt > 0 { args.arg::<u64>() } else { args.arg::<c_uint>() as u64 };
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
