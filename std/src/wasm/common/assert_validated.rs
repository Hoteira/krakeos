use core::fmt::Debug;

pub trait UnwrapValidatedExt<T> {
    fn unwrap_validated(self) -> T;
}

impl<T> UnwrapValidatedExt<T> for Option<T> {
    fn unwrap_validated(self) -> T {
        match self {
            Some(t) => t,
            None => {
                crate::debugln!("Validation guarantees this to be `Some(_)`, but it is `None`.");
                crate::os::exit(1);
            }
        }
    }
}

impl<T, E: Debug> UnwrapValidatedExt<T> for Result<T, E> {
    fn unwrap_validated(self) -> T {
        match self {
            Ok(t) => t,
            Err(e) => {
                crate::debugln!("Validation guarantees this to be `Ok(_)`, but it is `Err({e:?})`. This indicates a bug in the validator or interpreter state.");
                crate::os::exit(1);
            }
        }
    }
}

pub trait UnreachableValidatedExt {
    fn unreachable_validated(self) -> !;
}

impl<T> UnreachableValidatedExt for Option<T> {
    fn unreachable_validated(self) -> ! {
        crate::debugln!("Validation guarantees this to be `Some(_)`, but it is `None` (unreachable)");
        crate::os::exit(1);
    }
}

impl<T, E: Debug> UnreachableValidatedExt for Result<T, E> {
    fn unreachable_validated(self) -> ! {
        match self {
            Ok(_) => {
                crate::debugln!("Validation guarantees this to be `Ok(_)`, but it is `Ok(_)` (unreachable)");
            }
            Err(e) => {
                crate::debugln!("Validation guarantees this to be `Ok(_)`, but it is `Err({e:?})` (unreachable)");
            }
        }
        crate::os::exit(1);
    }
}

#[macro_export]
macro_rules! unreachable_validated {
    () => {
        {
            $crate::debugln!("because of prior validation (unreachable)");
            $crate::os::exit(1);
        }
    };
}
