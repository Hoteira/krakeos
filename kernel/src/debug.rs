use core::fmt;
use crate::drivers::port::{inb, Port};
#[allow(unused_imports)]

pub const COM1: u16 = 0x3F8;

pub struct SerialDebug {
    port: Port,
}

impl SerialDebug {
    pub fn new() -> Self {
        SerialDebug {
            port: Port::new(COM1),
        }
    }

    pub fn write_byte(&self, byte: u8) {
        
        while (inb(COM1 + 5) & 0x20) == 0 {}
        match byte {
            b'\n' => {
                self.port.outb(b'\r');
                
                while (inb(COM1 + 5) & 0x20) == 0 {}
                self.port.outb(b'\n');
            }
            byte => {
                self.port.outb(byte);
            }
        }
    }

    pub fn write_string(&self, s: &str) {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                b'\r' => {}, 
                _ => self.write_byte(0xfe),
            }
        }
    }

    pub fn write_kb(&self, s: &str) {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' => {
                    self.write_byte(byte);
                }
                b'\r' => {}, 

                _ => {
                    self.write_byte(0xfe);
                }
            }
        }
    }
}

impl fmt::Write for SerialDebug {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

pub fn serial_print_str(s: &str) {
    SerialDebug::new().write_string(s);
}

#[doc(hidden)]
pub fn _debug_print(args: fmt::Arguments) {
    use core::fmt::Write;
    SerialDebug::new().write_fmt(args).unwrap();
}

#[macro_export]
macro_rules! debug_print {
    ($($arg:tt)*) => ($crate::debug::_debug_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! debugln {
    () => ($crate::debug_print!("\n"));
    ($($arg:tt)*) => ($crate::debug_print!("{}\n", format_args!($($arg)*)));
}



#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::debug_print!("{}", format_args!($($arg)*)));
}



#[macro_export]

macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));

}
