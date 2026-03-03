pub trait Termination {
    fn report(self) -> i32;
}

#[cfg(any(feature = "userland", target_arch = "x86_64"))]
pub mod wasi;

impl Termination for () {
    fn report(self) -> i32 {
        0
    }
}

impl Termination for i32 {
    fn report(self) -> i32 {
        self
    }
}

impl<T: Termination, E: core::fmt::Debug> Termination for Result<T, E> {
    fn report(self) -> i32 {
        match self {
            Ok(val) => val.report(),
            Err(err) => {
                crate::debugln!("Error: {:?}", err);
                1
            }
        }
    }
}

pub fn exit(code: i32) -> ! {
    crate::os::exit(code as u64)
}

pub fn get_pid() -> u64 {
    crate::os::krakeos::process_get_pid()
}
