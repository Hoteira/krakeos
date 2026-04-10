pub use std::sync::{Spinlock, Semaphore};

pub type Mutex<T> = std::sync::Spinlock<T>;
pub type MutexGuard<'a, T> = std::sync::SpinlockGuard<'a, T>;
pub type IntMutexGuard<'a, T> = std::sync::SpinlockGuard<'a, T>;

pub type YieldMutex<T> = std::sync::Mutex<T>;
pub type YieldMutexGuard<'a, T> = std::sync::MutexGuard<'a, T>;
