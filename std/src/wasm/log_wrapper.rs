#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        // crate::debugln!("TRACE: {}", format_args!($($arg)*));
    };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        // crate::debugln!("DEBUG: {}", format_args!($($arg)*));
    };
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        crate::debugln!("ERROR: {}", format_args!($($arg)*));
    };
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        crate::debugln!("INFO: {}", format_args!($($arg)*));
    };
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        crate::debugln!("WARN: {}", format_args!($($arg)*));
    };
}
