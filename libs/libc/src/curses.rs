use core::ffi::{c_char, c_int, c_void};

pub type chtype = u32;


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
pub unsafe extern "C" fn wcwidth(_c: u32) -> c_int { 1 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn towlower(c: u32) -> u32 {
    if c >= 'A' as u32 && c <= 'Z' as u32 { c + 32 } else { c }
}

#[unsafe(no_mangle)]
pub static mut COLS: c_int = 80;
#[unsafe(no_mangle)]
pub static mut LINES: c_int = 25;

#[repr(C)]
pub struct WINDOW {
    pub cury: c_int,
    pub curx: c_int,
    pub maxy: c_int,
    pub maxx: c_int,
    pub begy: c_int,
    pub begx: c_int,
    pub flags: c_int,
    pub attrs: c_int,
    pub bkgd: chtype,
    pub _delay: bool,
}

#[unsafe(no_mangle)]
pub static mut stdscr: *mut WINDOW = core::ptr::null_mut();
#[unsafe(no_mangle)]
pub static mut curscr: *mut WINDOW = core::ptr::null_mut();


#[unsafe(no_mangle)]
pub unsafe extern "C" fn initscr() -> *mut WINDOW {
    let mut ws = core::mem::zeroed::<crate::sys::winsize>();
    let res = std::os::syscall(16, 0, 0x5413, &mut ws as *mut _ as u64);
    if res == 0 {
        COLS = ws.ws_col as c_int;
        LINES = ws.ws_row as c_int;
        crate::stdio::krake_debug_printf(b"initscr: TIOCGWINSZ success, size %d x %d\n\0".as_ptr() as *const c_char, COLS, LINES);
    } else {
        crate::stdio::krake_debug_printf(b"initscr: TIOCGWINSZ FAILED with %d, using defaults\n\0".as_ptr() as *const c_char, res as c_int);
        COLS = 80;
        LINES = 25;
    }
    let win = newwin(LINES, COLS, 0, 0);
    stdscr = win;
    curscr = win;
    win
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn endwin() -> c_int { 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn newwin(nlines: c_int, ncols: c_int, begin_y: c_int, begin_x: c_int) -> *mut WINDOW {
    let ptr = crate::stdlib::malloc(core::mem::size_of::<WINDOW>()) as *mut WINDOW;
    if !ptr.is_null() {
        (*ptr).cury = 0;
        (*ptr).curx = 0;
        (*ptr).maxy = nlines;
        (*ptr).maxx = ncols;
        (*ptr).begy = begin_y;
        (*ptr).begx = begin_x;
        (*ptr).flags = 0;
        (*ptr).attrs = 0;
        (*ptr).bkgd = 0;
        (*ptr)._delay = false;
    }
    ptr
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn delwin(win: *mut WINDOW) -> c_int {
    if !win.is_null() {
        crate::stdlib::free(win as *mut c_void);
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wmove(win: *mut WINDOW, y: c_int, x: c_int) -> c_int {
    if win.is_null() {
        return -1;
    }
    (*win).cury = y;
    (*win).curx = x;

    let abs_y = (*win).begy + y;
    let abs_x = (*win).begx + x;

    let mut buf = [0u8; 32];
    let mut pos = 0;


    buf[pos] = 0x1B;
    pos += 1;
    buf[pos] = b'[';
    pos += 1;

    let mut row = abs_y + 1;
    let mut row_digits = [0u8; 10];
    let mut rd = 0;
    if row == 0 {
        row_digits[rd] = b'0';
        rd += 1;
    }
    while row > 0 {
        row_digits[rd] = (row % 10) as u8 + b'0';
        row /= 10;
        rd += 1;
    }
    while rd > 0 {
        rd -= 1;
        buf[pos] = row_digits[rd];
        pos += 1;
    }

    buf[pos] = b';';
    pos += 1;

    let mut col = abs_x + 1;
    let mut col_digits = [0u8; 10];
    let mut cd = 0;
    if col == 0 {
        col_digits[cd] = b'0';
        cd += 1;
    }
    while col > 0 {
        col_digits[cd] = (col % 10) as u8 + b'0';
        col /= 10;
        cd += 1;
    }
    while cd > 0 {
        cd -= 1;
        buf[pos] = col_digits[cd];
        pos += 1;
    }

    buf[pos] = b'H';
    pos += 1;

    let s = unsafe { core::str::from_utf8_unchecked(&buf[..pos]) };
    std::os::print(s);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wgetch(win: *mut WINDOW) -> c_int {
    let non_blocking = if !win.is_null() { (*win)._delay } else { false };

    loop {
        let mut buf = [0u8; 1];
        let n = std::os::file_read(0, &mut buf);
        if n == 1 {
            return buf[0] as c_int;
        } else if n == usize::MAX {
            return -1;
        } else if n == 0 {
            if non_blocking {
                return -1;
            }
        }
        std::os::yield_task();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ungetch(_ch: c_int) -> c_int { 0 }

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
pub unsafe extern "C" fn werase(win: *mut WINDOW) -> c_int {
    if win.is_null() { return -1; }


    let spaces = b"                                                                                                                                ";

    for y in 0..(*win).maxy {
        wmove(win, y, 0);
        let mut remaining = (*win).maxx as usize;
        while remaining > 0 {
            let to_write = core::cmp::min(remaining, spaces.len());
            std::os::print(unsafe { core::str::from_utf8_unchecked(&spaces[..to_write]) });
            remaining -= to_write;
        }
    }
    wmove(win, 0, 0);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wclear(win: *mut WINDOW) -> c_int {
    werase(win)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wclrtoeol(_win: *mut WINDOW) -> c_int {
    std::os::print("\x1B[K");
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn isendwin() -> c_int { 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn curs_set(visibility: c_int) -> c_int {
    if visibility == 0 {
        std::os::print("\x1B[?25l");
    } else {
        std::os::print("\x1B[?25h");
    }
    1
}

static mut WADDCH_COUNT: usize = 0;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn waddch(win: *mut WINDOW, ch: u32) -> c_int {
    let mut buf = [0u8; 4];
    let char_code = ch & 0xFF;
    if char_code == 0 { return 0; }
    if let Some(c) = char::from_u32(char_code) {
        let s = c.encode_utf8(&mut buf);
        std::os::print(s);
        if !win.is_null() {
            (*win).curx += 1;
        }
    } else {}
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mvwaddch(win: *mut WINDOW, y: c_int, x: c_int, ch: u32) -> c_int {
    wmove(win, y, x);
    waddch(win, ch)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn waddstr(win: *mut WINDOW, s: *const c_char) -> c_int {
    if s.is_null() { return -1; }
    let cow = core::ffi::CStr::from_ptr(s).to_string_lossy();
    std::os::print(&cow);
    if !win.is_null() {
        (*win).curx += cow.len() as c_int;
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mvwaddstr(win: *mut WINDOW, y: c_int, x: c_int, s: *const c_char) -> c_int {
    wmove(win, y, x);
    waddstr(win, s)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn waddnstr(win: *mut WINDOW, s: *const c_char, n: c_int) -> c_int {
    if s.is_null() { return -1; }
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
    if !win.is_null() {
        (*win).curx += i as c_int;
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mvwaddnstr(win: *mut WINDOW, y: c_int, x: c_int, s: *const c_char, n: c_int) -> c_int {
    wmove(win, y, x);
    waddnstr(win, s, n)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mvwprintw(win: *mut WINDOW, y: c_int, x: c_int, fmt: *const c_char, mut args: ...) -> c_int {
    let mut buf = [0u8; 1024];
    let n = crate::stdio::vsnprintf(buf.as_mut_ptr() as *mut c_char, 1024, fmt, args.as_va_list());
    wmove(win, y, x);
    let len = core::cmp::min(n as usize, 1023);
    let s = core::str::from_utf8_unchecked(&buf[..len]);
    std::os::print(s);
    if !win.is_null() {
        (*win).curx += len as c_int;
    }
    n
}

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
pub unsafe extern "C" fn beep() -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn doupdate() -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wrefresh(_win: *mut WINDOW) -> c_int { 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn has_colors() -> bool { true }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn start_color() -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn init_pair(_pair: c_int, _f: c_int, _b: c_int) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn use_default_colors() -> c_int { 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn intrflush(_win: *mut WINDOW, _bf: bool) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keypad(_win: *mut WINDOW, _bf: bool) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nodelay(win: *mut WINDOW, bf: bool) -> c_int {
    if !win.is_null() {
        (*win)._delay = bf;
    }
    0
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cbreak() -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn noecho() -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nonl() -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn raw() -> c_int { 0 }


#[unsafe(no_mangle)]
pub unsafe extern "C" fn regcomp(_preg: *mut c_void, _regex: *const c_char, _cflags: c_int) -> c_int { 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn regexec(_preg: *const c_void, _string: *const c_char, _nmatch: usize, _pmatch: *mut c_void, _eflags: c_int) -> c_int { 1 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn regerror(_errcode: c_int, _preg: *const c_void, _errbuf: *mut c_char, _errbuf_size: usize) -> usize {
    if _errbuf_size > 0 {
        let msg = b"Regex error\0";
        let len = core::cmp::min(_errbuf_size - 1, msg.len());
        core::ptr::copy_nonoverlapping(msg.as_ptr(), _errbuf as *mut u8, len);
        *(_errbuf.add(len)) = 0;
    }
    0
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn regfree(_preg: *mut c_void) {}


#[unsafe(no_mangle)]
pub unsafe extern "C" fn wctomb(s: *mut c_char, wc: u32) -> c_int {
    if s.is_null() { return 0; }
    *s = wc as u8 as c_char;
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigaction(_sig: c_int, _act: *const c_void, _oact: *mut c_void) -> c_int { 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tgetstr(_id: *const c_char, _area: *mut *mut c_char) -> *mut c_char { core::ptr::null_mut() }


#[unsafe(no_mangle)]
pub unsafe extern "C" fn dirname(path: *mut c_char) -> *mut c_char {
    let len = crate::string::strlen(path);
    if len == 0 {
        *path = b'.' as c_char;
        *path.add(1) = 0;
        return path;
    }
    let mut i = len - 1;
    while i > 0 {
        if *path.add(i) as u8 == b'/' {
            if i == 0 {
                *path.add(1) = 0;
            } else {
                *path.add(i) = 0;
            }
            return path;
        }
        i -= 1;
    }
    if *path as u8 != b'/' {
        *path = b'.' as c_char;
        *path.add(1) = 0;
    } else {
        *path.add(1) = 0;
    }
    path
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn basename(path: *mut c_char) -> *mut c_char {
    let len = crate::string::strlen(path);
    if len == 0 { return path; }
    let mut i = len - 1;
    while i > 0 {
        if *path.add(i) as u8 == b'/' {
            return path.add(i + 1);
        }
        i -= 1;
    }
    if *path as u8 == b'/' {
        return path.add(1);
    }
    path
}