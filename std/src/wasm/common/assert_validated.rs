use core::fmt::Debug;

pub trait UnwrapValidatedExt<T> {
    fn unwrap_validated(self) -> T;
}

impl<T> UnwrapValidatedExt<T> for Option<T> {
    fn unwrap_validated(self) -> T {
        self.expect("Validation guarantees this to be `Some(_)`, but it is `None`")
    }
}

impl<T, E: Debug> UnwrapValidatedExt<T> for Result<T, E> {
    fn unwrap_validated(self) -> T {
        match self {
            Ok(t) => t,
            Err(e) => {
                panic!("Validation guarantees this to be `Ok(_)`, but it is `Err({e:?})`. This indicates a bug in the validator or interpreter state.");
            }
        }
    }
}

pub trait UnreachableValidatedExt {
    fn unreachable_validated(self) -> !;
}

impl<T> UnreachableValidatedExt for Option<T> {
    fn unreachable_validated(self) -> ! {
        panic!("Validation guarantees this to be `Some(_)`, but it is `None` (unreachable)")
    }
}

impl<T, E: Debug> UnreachableValidatedExt for Result<T, E> {
    fn unreachable_validated(self) -> ! {
        panic!("Validation guarantees this to be `Ok(_)`, but it is `Err({:?})` (unreachable)", self.err().unwrap())
    }
}

#[macro_export]
macro_rules! unreachable_validated {
    () => {
        panic!("because of prior validation (unreachable)")
    };
}
