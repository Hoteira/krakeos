use core::fmt;

struct StdoutWriter;

impl fmt::Write for StdoutWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        crate::os::print(s);
        Ok(())
    }
}

struct DebugWriter;

impl fmt::Write for DebugWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        crate::os::debug_print(s);
        Ok(())
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::io::serial::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[macro_export]
macro_rules! debug_print {
    ($($arg:tt)*) => ($crate::io::serial::_debug_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! debugln {
    () => ($crate::debug_print!("\n"));
    ($($arg:tt)*) => ($crate::debug_print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    let mut writer = StdoutWriter;
    let _ = writer.write_fmt(args);
}

#[doc(hidden)]
pub fn _debug_print(args: fmt::Arguments) {
    use core::fmt::Write;
    let mut writer = DebugWriter;
    let _ = writer.write_fmt(args);
}