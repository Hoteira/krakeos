use core::ffi::{c_int, c_uint};

#[repr(C)]
pub struct termios {
    pub c_iflag: c_uint,
    pub c_oflag: c_uint,
    pub c_cflag: c_uint,
    pub c_lflag: c_uint,
    pub c_line: u8,
    pub c_cc: [u8; 32],
    pub c_ispeed: c_uint,
    pub c_ospeed: c_uint,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcgetattr(fd: c_int, termios_p: *mut termios) -> c_int {
    std::os::ioctl(fd as usize, 0x5401, termios_p as u64)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcsetattr(fd: c_int, _optional_actions: c_int, termios_p: *const termios) -> c_int {
    std::os::ioctl(fd as usize, 0x5402, termios_p as u64)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cfgetispeed(termios_p: *const termios) -> c_uint { (*termios_p).c_ispeed }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cfgetospeed(termios_p: *const termios) -> c_uint { (*termios_p).c_ospeed }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cfsetispeed(termios_p: *mut termios, speed: c_uint) -> c_int { (*termios_p).c_ispeed = speed; 0 }
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cfsetospeed(termios_p: *mut termios, speed: c_uint) -> c_int { (*termios_p).c_ospeed = speed; 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn cfmakeraw(termios_p: *mut termios) {
    (*termios_p).c_iflag &= !(0000001 | 0000002 | 0000010 | 0000040 | 0000100 | 0000200 | 0000400 | 0002000);
    (*termios_p).c_oflag &= !0000001;
    (*termios_p).c_lflag &= !(0000010 | 0000100 | 0000002 | 0000001 | 0100000);
    (*termios_p).c_cflag &= !(0000060 | 0000400);
    (*termios_p).c_cflag |= 0000060;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ioctl(fd: c_int, request: core::ffi::c_long, arg: u64) -> c_int {
    if request == 0x5413 { // TIOCGWINSZ
        let winsize = arg as *mut u16;
        if !winsize.is_null() {
            let cols = std::os::syscall(44, 0, 0, 0) as u16;
            let rows = std::os::syscall(45, 0, 0, 0) as u16;
            *winsize = rows;
            *winsize.add(1) = cols;
            return 0;
        }
    }
    std::os::ioctl(fd as usize, request as u64, arg)
}

#[repr(C)]
pub struct WINDOW {
    pub curr_y: c_int, pub curr_x: c_int,
    pub max_y: c_int, pub max_x: c_int,
}

#[unsafe(no_mangle)] pub static mut stdscr: *mut WINDOW = core::ptr::null_mut();
#[unsafe(no_mangle)] pub static mut curscr: *mut WINDOW = core::ptr::null_mut();
#[unsafe(no_mangle)] pub static mut LINES: c_int = 25;
#[unsafe(no_mangle)] pub static mut COLS: c_int = 80;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn initscr() -> *mut WINDOW {
    LINES = std::os::syscall(45, 0, 0, 0) as c_int;
    COLS = std::os::syscall(44, 0, 0, 0) as c_int;
    let win = crate::stdlib::malloc(core::mem::size_of::<WINDOW>()) as *mut WINDOW;
    (*win).curr_y = 0; (*win).curr_x = 0;
    (*win).max_y = LINES; (*win).max_x = COLS;
    stdscr = win;
    curscr = win;
    win
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn endwin() -> c_int { 0 }
#[unsafe(no_mangle)] pub unsafe extern "C" fn isendwin() -> c_int { 0 }
