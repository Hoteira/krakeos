use core::ffi::{c_void, c_char, c_int, c_long, c_double, VaList, c_uint};
use alloc::boxed::Box;
use crate::string::strlen;

#[unsafe(no_mangle)] pub unsafe extern "C" fn putchar(c: c_int) -> c_int { let b = [c as u8]; std::os::print(core::str::from_utf8_unchecked(&b)); c }
#[unsafe(no_mangle)] pub unsafe extern "C" fn puts(s: *const c_char) -> c_int { printf(s); putchar(b'\n' as i32); 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fgetc(st: *mut c_void) -> c_int {
    let mut b = 0u8;
    if fread(&mut b as *mut u8 as *mut c_void, 1, 1, st) == 1 { b as c_int }
    else { -1 }
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn getc(st: *mut c_void) -> c_int { fgetc(st) }
#[unsafe(no_mangle)] pub unsafe extern "C" fn putc(c: c_int, st: *mut c_void) -> c_int { if fwrite(&c as *const c_int as *const c_void, 1, 1, st) == 1 { c } else { -1 } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn ungetc(_c: c_int, _st: *mut c_void) -> c_int { _c }
#[unsafe(no_mangle)] pub unsafe extern "C" fn ferror(_st: *mut c_void) -> c_int { 0 }
#[unsafe(no_mangle)] pub unsafe extern "C" fn fflush(_s: *mut c_void) -> c_int { 0 }

#[unsafe(no_mangle)] pub unsafe extern "C" fn printf(f: *const c_char, mut args: ...) -> c_int {
    vfprintf(1 as *mut c_void, f, args.as_va_list())
}

#[unsafe(no_mangle)] 
pub unsafe extern "C" fn fprintf(s: *mut c_void, f: *const c_char, mut args: ...) -> c_int { 
    vfprintf(s, f, args.as_va_list())
}

#[unsafe(no_mangle)] 
pub unsafe extern "C" fn vfprintf(st: *mut c_void, f: *const c_char, mut ap: VaList) -> c_int { 
     let mut buffer = alloc::vec::Vec::new();
     let res = printf_core(|b| {
         buffer.push(b);
     }, f, &mut ap);
     let fd = if st.is_null() || st as usize == 1 || st as usize == 2 {
         1
     } else if st as usize == 0 {
         0
     } else {
         let file = &mut *(st as *mut std::fs::File);
         file.as_raw_fd()
     };
     std::os::file_write(fd, &buffer);
     res
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn fopen(filename: *const c_char, _mode: *const c_char) -> *mut c_void {
    let path = core::str::from_utf8_unchecked(core::slice::from_raw_parts(filename as *const u8, strlen(filename)));
    if let Ok(file) = std::fs::File::open(path) { Box::into_raw(Box::new(file)) as *mut c_void } else { core::ptr::null_mut() }
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn fdopen(fd: c_int, _mode: *const c_char) -> *mut c_void {
    let file = std::fs::File::from_raw_fd(fd as usize);
    Box::into_raw(Box::new(file)) as *mut c_void
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn fclose(s: *mut c_void) -> c_int { if !s.is_null() && s as usize > 2 { drop(Box::from_raw(s as *mut std::fs::File)); 0 } else { 0 } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn fread(p: *mut c_void, s: usize, n: usize, st: *mut c_void) -> usize { if st.is_null() { return 0; } let fd = if st as usize == 1 || st as usize == 2 { 1 } else if st as usize == 0 { 0 } else { (*(st as *mut std::fs::File)).as_raw_fd() }; let res = std::os::file_read(fd, core::slice::from_raw_parts_mut(p as *mut u8, s * n)); if res != usize::MAX { res / s } else { 0 } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn fwrite(p: *const c_void, s: usize, n: usize, st: *mut c_void) -> usize { if st.is_null() { return 0; } let fd = if st as usize == 1 || st as usize == 2 { 1 } else if st as usize == 0 { 0 } else { (*(st as *mut std::fs::File)).as_raw_fd() }; let res = std::os::file_write(fd, core::slice::from_raw_parts(p as *const u8, s * n)); if res != usize::MAX { res / s } else { 0 } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn fseek(st: *mut c_void, o: c_long, w: c_int) -> c_int { if st.is_null() || st as usize <= 2 { return -1; } let f = &mut *(st as *mut std::fs::File); if std::os::file_seek(f.as_raw_fd(), o as i64, w as usize) != u64::MAX { 0 } else { -1 } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn ftell(st: *mut c_void) -> c_long { if st.is_null() || st as usize <= 2 { return -1; } let f = &mut *(st as *mut std::fs::File); let r = std::os::file_seek(f.as_raw_fd(), 0, 1); if r != u64::MAX { r as c_long } else { -1 } }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getline(lineptr: *mut *mut c_char, n: *mut usize, stream: *mut c_void) -> isize {
    if lineptr.is_null() || n.is_null() || stream.is_null() { return -1; }
    if (*lineptr).is_null() {
        *n = 128;
        *lineptr = crate::stdlib::malloc(*n) as *mut c_char;
    }
    let mut count = 0;
    loop {
        if count + 1 >= *n {
            *n *= 2;
            *lineptr = crate::stdlib::realloc(*lineptr as *mut c_void, *n) as *mut c_char;
        }
        let mut b = 0u8;
        if fread(&mut b as *mut u8 as *mut c_void, 1, 1, stream) != 1 {
            if count == 0 { return -1; }
            break;
        }
        *(*lineptr).add(count) = b as c_char;
        count += 1;
        if b == b'\n' { break; }
    }
    *(*lineptr).add(count) = 0;
    count as isize
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn snprintf(str: *mut c_char, size: usize, fmt: *const c_char, mut args: ...) -> c_int {
    vsnprintf(str, size, fmt, args.as_va_list())
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn sprintf(str: *mut c_char, fmt: *const c_char, mut args: ...) -> c_int {
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

#[unsafe(no_mangle)] pub unsafe extern "C" fn vsprintf(str: *mut c_char, fmt: *const c_char, ap: VaList) -> c_int {
    vsnprintf(str, usize::MAX, fmt, ap)
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn sscanf(str: *const c_char, _fmt: *const c_char, ...) -> c_int {
    let s = core::str::from_utf8_unchecked(core::slice::from_raw_parts(str as *const u8, strlen(str)));
    if let Ok(v) = s.trim().parse::<i32>() { v } else { 0 }
}

pub unsafe fn printf_core(mut output: impl FnMut(u8), fmt: *const c_char, args: &mut VaList) -> c_int {
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
                    let pad_count = (precision as usize).min(63).saturating_sub(len);
                    let is_negative = buf[0] == b'-';
                    if is_negative {
                        let mut tmp = [0u8; 64];
                        tmp[0] = b'-';
                        for i in 0..pad_count { tmp[1 + i] = b'0'; }
                        for i in 1..len { tmp[pad_count + i] = buf[i]; }
                        buf[..pad_count + len].copy_from_slice(&tmp[..pad_count + len]);
                        pad_count + len
                    } else {
                        let mut tmp = [0u8; 64];
                        for i in 0..pad_count { tmp[i] = b'0'; }
                        tmp[pad_count..pad_count + len].copy_from_slice(&buf[..len]);
                        buf[..pad_count + len].copy_from_slice(&tmp[..pad_count + len]);
                        pad_count + len
                    }
                } else { len };
                write_padded(&mut output, &buf[..final_len], width, zero_pad, &mut written);
            }
            b'u' => {
                let val = if size_t_spec { args.arg::<usize>() as u64 } 
                else if long_cnt > 0 { args.arg::<u64>() } 
                else { args.arg::<c_uint>() as u64 };
                let len = itoa(val, &mut buf, 10, false);
                let final_len = if precision >= 0 && len < precision as usize {
                    let pad_count = (precision as usize).min(63).saturating_sub(len);
                    let mut tmp = [0u8; 64];
                    for i in 0..pad_count { tmp[i] = b'0'; }
                    tmp[pad_count..pad_count + len].copy_from_slice(&buf[..len]);
                    buf[..pad_count + len].copy_from_slice(&tmp[..pad_count + len]);
                    pad_count + len
                } else { len };
                write_padded(&mut output, &buf[..final_len], width, zero_pad, &mut written);
            }
            b'x' | b'X' | b'p' => {
                let val = if spec == b'p' as c_char || size_t_spec { args.arg::<usize>() as u64 } 
                else if long_cnt > 0 { args.arg::<u64>() } 
                else { args.arg::<c_uint>() as u64 };
                let len = itoa(val, &mut buf, 16, spec == b'X' as c_char);
                let final_len = if precision >= 0 && len < precision as usize {
                    let pad_count = (precision as usize).min(63).saturating_sub(len);
                    let mut tmp = [0u8; 64];
                    for i in 0..pad_count { tmp[i] = b'0'; }
                    tmp[pad_count..pad_count + len].copy_from_slice(&buf[..len]);
                    buf[..pad_count + len].copy_from_slice(&tmp[..pad_count + len]);
                    pad_count + len
                } else { len };
                write_padded(&mut output, &buf[..final_len], width, zero_pad, &mut written);
            }
            b's' => {
                let ptr = args.arg::<*const c_char>();
                let s_slice = if ptr.is_null() { "(null)".as_bytes() } else {
                    let len = strlen(ptr);
                    let actual_len = if precision >= 0 && len > precision as usize { precision as usize } else { len };
                    core::slice::from_raw_parts(ptr as *const u8, actual_len)
                };
                write_padded(&mut output, s_slice, width, false, &mut written);
            }
            b'c' => { output(args.arg::<c_int>() as u8); written += 1; }
            b'f' => { let _v = args.arg::<c_double>(); write_padded(&mut output, b"FLOAT", width, false, &mut written); }
            b'%' => { output(b'%'); written += 1; }
            _ => { output(b'%'); output(spec as u8); written += 2; }
        }
    }
    written
}

fn itoa(mut n: u64, buf: &mut [u8], base: u64, uppercase: bool) -> usize {
    if n == 0 { buf[0] = b'0'; return 1; }
    let mut len = 0;
    let mut temp = n;
    while temp > 0 { temp /= base; len += 1; }
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
    if n < 0 { buf[0] = b'-'; 1 + itoa((-n) as u64, &mut buf[1..], 10, false) } else { itoa(n as u64, buf, 10, false) }
}

unsafe fn write_padded(output: &mut impl FnMut(u8), s: &[u8], width: usize, zero_pad: bool, written: &mut c_int) {
    let len = s.len();
    if len < width {
        let pad_char = if zero_pad { b'0' } else { b' ' };
        for _ in 0..(width - len) { output(pad_char); *written += 1; }
    }
    for &b in s { output(b); *written += 1; }
}
