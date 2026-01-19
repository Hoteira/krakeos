use core::arch::asm;
use core::fmt;

pub fn write_byte(byte: u8) {
    match byte {
        b'\n' => outb(0x3F8, '\n' as u8),
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