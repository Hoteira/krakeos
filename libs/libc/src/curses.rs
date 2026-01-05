use core::ffi::{c_char, c_int, c_void};

// --- CTYPE / WCHAR stubs ---
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswalnum(c: u32) -> c_int {
    if (c >= 'a' as u32 && c <= 'z' as u32) || (c >= 'A' as u32 && c <= 'Z' as u32) || (c >= '0' as u32 && c <= '9' as u32) { 1 } else { 0 }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswblank(c: u32) -> c_int {
    if c == ' ' as u32 || c == '\t' as u32 { 1 } else { 0 }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iswpunct(c: u32) -> c_int {
    if !((c >= 'a' as u32 && c <= 'z' as u32) || (c >= 'A' as u32 && c <= 'Z' as u32) || (c >= '0' as u32 && c <= '9' as u32) || c == ' ' as u32) { 1 } else { 0 }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcwidth(_c: u32) -> c_int { 1 } // Assume 1 for now
#[unsafe(no_mangle)]
pub unsafe extern "C" fn towlower(c: u32) -> u32 {
    if c >= 'A' as u32 && c <= 'Z' as u32 { c + 32 } else { c }
}

// --- NCURSES stubs ---
// ...
#[unsafe(no_mangle)]
pub unsafe extern "C" fn newwin(_lines: c_int, _cols: c_int, _y: c_int, _x: c_int) -> *mut WINDOW { &raw mut STD_WIN }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn delwin(_win: *mut WINDOW) -> c_int { 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wgetch(_win: *mut WINDOW) -> c_int {
    // Read 1 byte from stdin (fd 0)
    let mut buf = [0u8; 1];
    let n = std::os::file_read(0, &mut buf);
    if n == 1 { 
        buf[0] as c_int 
    } else {
        std::os::yield_task();
        -1 
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ungetch(_ch: c_int) -> c_int { 0 } // TODO: buffer it?

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napms(ms: c_int) -> c_int {
    crate::unistd::usleep((ms * 1000) as u32);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wnoutrefresh(_win: *mut WINDOW) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wredrawln(_win: *mut WINDOW, _beg: c_int, _num: c_int) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wmove(_win: *mut WINDOW, y: c_int, x: c_int) -> c_int {
    let s = alloc::format!("\x1B[{};{}H", y + 1, x + 1);
    std::os::print(&s);
    0
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn isendwin() -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wclrtoeol(_win: *mut WINDOW) -> c_int {
    std::os::print("\x1B[K");
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mvwaddnstr(_win: *mut WINDOW, _y: c_int, _x: c_int, s: *const c_char, n: c_int) -> c_int {
    let mut buf = alloc::vec::Vec::new();
    let mut i = 0;
    while i < n {
        let c = *s.add(i as usize);
        if c == 0 { break; }
        buf.push(c as u8);
        i += 1;
    }
    let s_str = core::str::from_utf8_unchecked(&buf);
    std::os::print(s_str);
    0
}

// --- SIGNAL / SYS ---
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigaction(_sig: c_int, _act: *const c_void, _oact: *mut c_void) -> c_int { 0 }

// --- LOCALE ---
// Moved to locale.rs

// --- STDLIB extra ---
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wctomb(s: *mut c_char, wc: u32) -> c_int {
    if s.is_null() { return 0; }
    // Assume ASCII/UTF-8 single byte for now
    *s = wc as u8 as c_char;
    1
}
// Nano uses WINDOW* which is a pointer. We can just use dummy pointers.
#[repr(C)]
pub struct WINDOW {
    _dummy: c_int,
}
static mut STD_WIN: WINDOW = WINDOW { _dummy: 0 };

#[unsafe(no_mangle)]
pub unsafe extern "C" fn initscr() -> *mut WINDOW { &raw mut STD_WIN }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cbreak() -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn noecho() -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nonl() -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn intrflush(_win: *mut WINDOW, _bf: bool) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keypad(_win: *mut WINDOW, _bf: bool) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nodelay(_win: *mut WINDOW, _bf: bool) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raw() -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn beep() -> c_int {
    // std::os::print("\x07"); // Beep
    0
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn doupdate() -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn curs_set(_visibility: c_int) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn endwin() -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wrefresh(_win: *mut WINDOW) -> c_int {
    // Flush stdout?
    0
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn waddch(_win: *mut WINDOW, ch: u32) -> c_int {
    let mut buf = [0u8; 4];
    if let Some(c) = char::from_u32(ch) {
        let s = c.encode_utf8(&mut buf);
        std::os::print(s);
    }
    0
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mvwaddch(_win: *mut WINDOW, _y: c_int, _x: c_int, ch: u32) -> c_int {
    // Move cursor then add ch
    // std::os::print(format!("\x1B[{};{}H", y+1, x+1).as_str());
    waddch(_win, ch)
}
// We might need more ncurses functions if we want it to work, but these are the ones linker complained about.

// --- UNISTD / SYS stubs ---
// Moved to unistd.rs

// --- REGEX stubs ---
#[unsafe(no_mangle)]
pub unsafe extern "C" fn regcomp(_preg: *mut c_void, _regex: *const c_char, _cflags: c_int) -> c_int { 1 } // Fail
#[unsafe(no_mangle)]
pub unsafe extern "C" fn regerror(_errcode: c_int, _preg: *const c_void, _errbuf: *mut c_char, _errbuf_size: usize) -> usize { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn regfree(_preg: *mut c_void) {}

// ... (previous content) ...

#[unsafe(no_mangle)]
pub static mut COLS: c_int = 80;
#[unsafe(no_mangle)]
pub static mut LINES: c_int = 25;
#[unsafe(no_mangle)]
pub static mut curscr: *mut WINDOW = unsafe { &raw mut STD_WIN };

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wattron(_win: *mut WINDOW, _attrs: c_int) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wattroff(_win: *mut WINDOW, _attrs: c_int) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scrollok(_win: *mut WINDOW, _bf: bool) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wscrl(_win: *mut WINDOW, _n: c_int) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn typeahead(_fd: c_int) -> c_int { 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn waddnstr(_win: *mut WINDOW, s: *const c_char, n: c_int) -> c_int {
    // Like waddstr but max n chars
    let mut buf = alloc::vec::Vec::new();
    let mut i = 0;
    while i < n {
        let c = *s.add(i as usize);
        if c == 0 { break; }
        buf.push(c as u8);
        i += 1;
    }
    let s_str = core::str::from_utf8_unchecked(&buf);
    std::os::print(s_str);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mvwprintw(_win: *mut WINDOW, _y: c_int, _x: c_int, _fmt: *const c_char, mut _args: ...) -> c_int {
    // Variadic stub - we can't easily forward va_list to printf without implementing vprintf
    // For now, just print a placeholder or ignore
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mvwaddstr(_win: *mut WINDOW, _y: c_int, _x: c_int, s: *const c_char) -> c_int {
    let cow = core::ffi::CStr::from_ptr(s).to_string_lossy();
    std::os::print(&cow);
    0
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn waddstr(_win: *mut WINDOW, s: *const c_char) -> c_int {
    let cow = core::ffi::CStr::from_ptr(s).to_string_lossy();
    std::os::print(&cow);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tgetstr(_id: *const c_char, _area: *mut *mut c_char) -> *mut c_char { core::ptr::null_mut() }

// --- REGEX extra ---
#[unsafe(no_mangle)]
pub unsafe extern "C" fn regexec(_preg: *const c_void, _string: *const c_char, _nmatch: usize, _pmatch: *mut c_void, _eflags: c_int) -> c_int { 1 } // No match

// --- PWD extra ---
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpwent() -> *mut c_void { core::ptr::null_mut() }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn endpwent() {}

// --- LIBGEN stubs ---
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dirname(path: *mut c_char) -> *mut c_char {
    // ... (existing dirname)
    let len = crate::string::strlen(path);
    if len == 0 { return path; }
    let mut i = len - 1;
    while i > 0 {
        if *path.add(i) as u8 == b'/' {
            *path.add(i) = 0;
            return path;
        }
        i -= 1;
    }
    // If no slash, return "."
    if *path as u8 != b'/' {
        *path = b'.' as c_char;
        *path.add(1) = 0;
    } else {
        *path.add(1) = 0; // Root /
    }
    path
}
