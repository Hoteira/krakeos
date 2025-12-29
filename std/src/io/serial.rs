use core::fmt;

struct SyscallWriter;

impl fmt::Write for SyscallWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        crate::os::print(s);
        Ok(())
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    let mut writer = SyscallWriter;
    let _ = writer.write_fmt(args);
}
