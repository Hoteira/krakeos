use core::arch::asm;
use core::fmt;

pub fn outb(port: u16, value: u8) {
    unsafe {
        asm!(
        "out dx, al",
        in("dx") port,
        in("al") value,
        options(nomem, nostack, preserves_flags)
        );
    }
}

pub fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        asm!(
        "in al, dx",
        out("al") value,
        in("dx") port,
        options(nomem, nostack, preserves_flags)
        );
    }
    value
}

fn wait_for_ready() {
    while (inb(0x3F8 + 5) & 0x20) == 0 {}
}

pub fn write_byte(byte: u8) {
    wait_for_ready();
    match byte {
        b'\n' => {
            outb(0x3F8, b'\r');
            wait_for_ready();
            outb(0x3F8, b'\n');
        }
        byte => {
            outb(0x3F8, byte);
        }
    }
}

pub fn debug(s: &str) {
    for byte in s.bytes() {
        match byte {
            0x20..=0x7e | b'\n' => write_byte(byte),
            _ => write_byte(0xfe),
        }
    }
}


pub struct Terminal;

impl Terminal {
    pub fn new() -> Self {
        Terminal
    }
}

impl fmt::Write for Terminal {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        debug(s);
        Ok(())
    }
}


#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::debug::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    Terminal::new().write_fmt(args).unwrap();
}