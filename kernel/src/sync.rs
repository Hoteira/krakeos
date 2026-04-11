pub use std::sync::{Spinlock, Semaphore, RwLock};

pub type Mutex<T> = std::sync::Spinlock<T>;
pub type MutexGuard<'a, T> = std::sync::SpinlockGuard<'a, T>;
pub type IntMutexGuard<'a, T> = std::sync::SpinlockGuard<'a, T>;

pub type YieldMutex<T> = std::sync::Mutex<T>;
pub type YieldMutexGuard<'a, T> = std::sync::MutexGuard<'a, T>;

pub type YieldRwLock<T> = std::sync::RwLock<T>;
pub type YieldRwLockReadGuard<'a, T> = std::sync::RwLockReadGuard<'a, T>;
pub type YieldRwLockWriteGuard<'a, T> = std::sync::RwLockWriteGuard<'a, T>;

pub type RwSpinlock<T> = std::sync::RwSpinlock<T>;
pub type RwSpinlockReadGuard<'a, T> = std::sync::RwSpinlockReadGuard<'a, T>;
pub type RwSpinlockWriteGuard<'a, T> = std::sync::RwSpinlockWriteGuard<'a, T>;
