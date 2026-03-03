use core::cell::UnsafeCell;
use core::hint::{self};
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU32, Ordering};

pub struct RwSpinLock<T> {
    inner: UnsafeCell<T>,
    state: AtomicU32,
}

impl<T> RwSpinLock<T> {
    pub fn new(value: T) -> Self {
        Self {
            inner: UnsafeCell::new(value),
            state: AtomicU32::new(0),
        }
    }
    pub fn read(&self) -> ReadLockGuard<'_, T> {
        let mut s = self.state.load(Ordering::Relaxed);
        let mut count = 0;
        loop {
            if s % 2 == 0 && s < u32::MAX - 2 {
                match self.state.compare_exchange_weak(
                    s,
                    s + 2,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => return ReadLockGuard { lock: self },
                    Err(update_s) => s = update_s,
                }
            }
            if s % 2 == 1 {
                hint::spin_loop();
                s = self.state.load(Ordering::Relaxed);
            }
            count += 1;
            if count == 10000000 {
                crate::os::debug_print("[std] RwSpinLock: Possible DEADLOCK in read()\n");
            }
        }
    }
    pub fn write(&self) -> WriteLockGuard<'_, T> {
        let mut s = self.state.load(Ordering::Relaxed);
        let mut count = 0;
        loop {
            if s <= 1 {
                match self
                    .state
                    .compare_exchange(s, u32::MAX, Ordering::Acquire, Ordering::Relaxed)
                {
                    Ok(_) => return WriteLockGuard { lock: self },
                    Err(updated_s) => {
                        s = updated_s;
                        continue;
                    }
                }
            }
            if s % 2 == 0 {
                match self
                    .state
                    .compare_exchange(s, s + 1, Ordering::Relaxed, Ordering::Relaxed)
                {
                    Ok(_) => s += 1,
                    Err(updated_s) => {
                        s = updated_s;
                        continue;
                    }
                }
            }
            hint::spin_loop();
            s = self.state.load(Ordering::Relaxed);
            count += 1;
            if count == 10000000 {
                crate::os::debug_print("[std] RwSpinLock: Possible DEADLOCK in write()\n");
            }
        }
    }
}

unsafe impl<T> Sync for RwSpinLock<T>
where
    T: Send + Sync,
{}

impl<T: Default> Default for RwSpinLock<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

pub struct ReadLockGuard<'a, T> {
    lock: &'a RwSpinLock<T>,
}

impl<T> Deref for ReadLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.inner.get() }
    }
}

impl<T> Drop for ReadLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.state.fetch_sub(2, Ordering::Release);
    }
}

pub struct WriteLockGuard<'a, T> {
    lock: &'a RwSpinLock<T>,
}

impl<T> Deref for WriteLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.inner.get() }
    }
}

impl<T> DerefMut for WriteLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.inner.get() }
    }
}

impl<T> Drop for WriteLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.state.store(0, Ordering::Release);
    }
}
