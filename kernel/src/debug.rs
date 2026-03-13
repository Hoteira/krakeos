use crate::arch::x86_64::io::Port;
use core::fmt;
#[allow(unused_imports)]

pub const COM1: u16 = 0x3F8;

const DMESG_SIZE: usize = 128 * 1024;
pub struct Dmesg {
    pub buffer: [u8; DMESG_SIZE],
    pub head: usize,
    pub count: usize,
}

pub static DMESG: crate::sync::Mutex<Dmesg> = crate::sync::Mutex::new(Dmesg {
    buffer: [0; DMESG_SIZE],
    head: 0,
    count: 0,
});

impl Dmesg {
    pub fn write_byte(&mut self, b: u8) {
        self.buffer[self.head] = b;
        self.head = (self.head + 1) % DMESG_SIZE;
        if self.count < DMESG_SIZE {
            self.count += 1;
        }
    }

    pub fn read(&self, user_buf: &mut [u8]) -> usize {
        let to_read = core::cmp::min(user_buf.len(), self.count);
        let start = (self.head + DMESG_SIZE - self.count) % DMESG_SIZE;
        
        for i in 0..to_read {
            user_buf[i] = self.buffer[(start + i) % DMESG_SIZE];
        }
        to_read
    }
}

pub struct SerialDebug {
    port: Port,
}

impl SerialDebug {
    pub fn new() -> Self {
        SerialDebug {
            port: Port::new(COM1),
        }
    }

    fn wait_for_ready(&self) {
        let status_port = Port::new(COM1 + 5);
        while (status_port.inb() & 0x20) == 0 {}
    }

    pub fn write_byte(&self, byte: u8) {
        // Log to dmesg
        if let Some(mut dmesg) = DMESG.try_lock() {
            dmesg.write_byte(byte);
        }

        self.wait_for_ready();
        match byte {
            b'\n' => {
                self.port.outb(b'\r');
                self.wait_for_ready();
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
                b'\r' => {}
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
                b'\r' => {}

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

pub struct ArrayWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> ArrayWriter<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.pos]).unwrap_or("")
    }
}

impl<'a> fmt::Write for ArrayWriter<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        let len = bytes.len();
        if self.pos + len > self.buf.len() {
            return Err(fmt::Error);
        }
        self.buf[self.pos..self.pos + len].copy_from_slice(bytes);
        self.pos += len;
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
