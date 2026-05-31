use core::sync::atomic::{AtomicI32, Ordering};
use spin::{Mutex as InnerMutex, MutexGuard as InnerMutexGuard};
use spin::{RwLock as InnerRwLock, RwLockReadGuard as InnerRwLockReadGuard, RwLockWriteGuard as InnerRwLockWriteGuard};
use core::mem::ManuallyDrop;

/// A simple spinlock for very short critical sections.
/// Disables interrupts on the current CPU while held.
pub struct Spinlock<T> {
    inner: InnerMutex<T>,
}

unsafe impl<T: Send> Sync for Spinlock<T> {}
unsafe impl<T: Send> Send for Spinlock<T> {}

pub struct SpinlockGuard<'a, T> {
    inner: ManuallyDrop<InnerMutexGuard<'a, T>>,
    rflags: u64,
}

impl<T> Spinlock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            inner: InnerMutex::new(data),
        }
    }

    pub fn lock(&self) -> SpinlockGuard<'_, T> {
        let rflags: u64;
        #[cfg(not(target_arch = "wasm32"))]
        unsafe {
            core::arch::asm!("pushfq; pop {}", out(reg) rflags);
            core::arch::asm!("cli");
        }
        #[cfg(target_arch = "wasm32")]
        { rflags = 0; }

        SpinlockGuard {
            inner: ManuallyDrop::new(self.inner.lock()),
            rflags,
        }
    }

    pub fn try_lock(&self) -> Option<SpinlockGuard<'_, T>> {
        let rflags: u64;
        #[cfg(not(target_arch = "wasm32"))]
        unsafe {
            core::arch::asm!("pushfq; pop {}", out(reg) rflags);
            core::arch::asm!("cli");
        }
        #[cfg(target_arch = "wasm32")]
        { rflags = 0; }

        if let Some(guard) = self.inner.try_lock() {
            Some(SpinlockGuard {
                inner: ManuallyDrop::new(guard),
                rflags,
            })
        } else {
            #[cfg(not(target_arch = "wasm32"))]
            unsafe {
                if (rflags & 0x200) != 0 {
                    core::arch::asm!("sti");
                }
            }
            None
        }
    }

    pub fn int_lock(&self) -> SpinlockGuard<'_, T> {
        self.lock()
    }
}

impl<'a, T> core::ops::Deref for SpinlockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target { &*self.inner }
}

impl<'a, T> core::ops::DerefMut for SpinlockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut *self.inner }
}

impl<'a, T> Drop for SpinlockGuard<'a, T> {
    fn drop(&mut self) {
        unsafe {
            // Drop the lock FIRST
            ManuallyDrop::drop(&mut self.inner);
            
            // THEN restore interrupts
            #[cfg(not(target_arch = "wasm32"))]
            if (self.rflags & 0x200) != 0 {
                core::arch::asm!("sti");
            }
        }
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for Spinlock<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.try_lock() {
            Some(guard) => f.debug_struct("Spinlock").field("data", &*guard).finish(),
            None => f.debug_struct("Spinlock").field("data", &"<locked>").finish(),
        }
    }
}

/// A Mutex that yields the CPU if the lock is held.
/// Suitable for longer critical sections where spinning is wasteful.
pub struct Mutex<T> {
    inner: InnerMutex<T>,
}

impl<T: core::fmt::Debug> core::fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.try_lock() {
            Some(guard) => f.debug_struct("Mutex").field("data", &*guard).finish(),
            None => f.debug_struct("Mutex").field("data", &"<locked>").finish(),
        }
    }
}

unsafe impl<T: Send> Sync for Mutex<T> {}
unsafe impl<T: Send> Send for Mutex<T> {}

pub struct MutexGuard<'a, T> {
    inner: InnerMutexGuard<'a, T>,
}

impl<T> Mutex<T> {
    pub const fn new(data: T) -> Self {
        Self {
            inner: InnerMutex::new(data),
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, T> {
        loop {
            if let Some(guard) = self.inner.try_lock() {
                return MutexGuard { inner: guard };
            }
            #[cfg(not(target_arch = "wasm32"))]
            unsafe { core::arch::asm!("int 0x81"); }
            #[cfg(target_arch = "wasm32")]
            crate::os::yield_task();
        }
    }

    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        self.inner.try_lock().map(|guard| MutexGuard { inner: guard })
    }
}

impl<'a, T> core::ops::Deref for MutexGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target { &*self.inner }
}

impl<'a, T> core::ops::DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut *self.inner }
}

/// A yielding Reader-Writer lock.
pub struct RwLock<T> {
    inner: InnerRwLock<T>,
}

unsafe impl<T: Send + Sync> Sync for RwLock<T> {}
unsafe impl<T: Send + Sync> Send for RwLock<T> {}

pub struct RwLockReadGuard<'a, T> {
    inner: InnerRwLockReadGuard<'a, T>,
}

pub struct RwLockWriteGuard<'a, T> {
    inner: InnerRwLockWriteGuard<'a, T>,
}

impl<T> RwLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            inner: InnerRwLock::new(data),
        }
    }

    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        loop {
            if let Some(guard) = self.inner.try_read() {
                return RwLockReadGuard { inner: guard };
            }
            #[cfg(not(target_arch = "wasm32"))]
            unsafe { core::arch::asm!("int 0x81"); }
            #[cfg(target_arch = "wasm32")]
            crate::os::yield_task();
        }
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        loop {
            if let Some(guard) = self.inner.try_write() {
                return RwLockWriteGuard { inner: guard };
            }
            #[cfg(not(target_arch = "wasm32"))]
            unsafe { core::arch::asm!("int 0x81"); }
            #[cfg(target_arch = "wasm32")]
            crate::os::yield_task();
        }
    }
}

impl<'a, T> core::ops::Deref for RwLockReadGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target { &*self.inner }
}

impl<'a, T> core::ops::Deref for RwLockWriteGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target { &*self.inner }
}

impl<'a, T> core::ops::DerefMut for RwLockWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut *self.inner }
}

/// A non-yielding Reader-Writer spinlock.
/// Safe for use in interrupt contexts.
pub struct RwSpinlock<T> {
    inner: InnerRwLock<T>,
}

unsafe impl<T: Send + Sync> Sync for RwSpinlock<T> {}
unsafe impl<T: Send + Sync> Send for RwSpinlock<T> {}

pub struct RwSpinlockReadGuard<'a, T> {
    inner: ManuallyDrop<InnerRwLockReadGuard<'a, T>>,
    rflags: u64,
}

pub struct RwSpinlockWriteGuard<'a, T> {
    inner: ManuallyDrop<InnerRwLockWriteGuard<'a, T>>,
    rflags: u64,
}

impl<T> RwSpinlock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            inner: InnerRwLock::new(data),
        }
    }

    pub fn read(&self) -> RwSpinlockReadGuard<'_, T> {
        let rflags: u64;
        #[cfg(not(target_arch = "wasm32"))]
        unsafe {
            core::arch::asm!("pushfq; pop {}", out(reg) rflags);
            core::arch::asm!("cli");
        }
        #[cfg(target_arch = "wasm32")]
        { rflags = 0; }

        RwSpinlockReadGuard {
            inner: ManuallyDrop::new(self.inner.read()),
            rflags,
        }
    }

    pub fn write(&self) -> RwSpinlockWriteGuard<'_, T> {
        let rflags: u64;
        #[cfg(not(target_arch = "wasm32"))]
        unsafe {
            core::arch::asm!("pushfq; pop {}", out(reg) rflags);
            core::arch::asm!("cli");
        }
        #[cfg(target_arch = "wasm32")]
        { rflags = 0; }

        RwSpinlockWriteGuard {
            inner: ManuallyDrop::new(self.inner.write()),
            rflags,
        }
    }
}

impl<'a, T> core::ops::Deref for RwSpinlockReadGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target { &*self.inner }
}

impl<'a, T> Drop for RwSpinlockReadGuard<'a, T> {
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.inner);
            #[cfg(not(target_arch = "wasm32"))]
            if (self.rflags & 0x200) != 0 {
                core::arch::asm!("sti");
            }
        }
    }
}

impl<'a, T> core::ops::Deref for RwSpinlockWriteGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target { &*self.inner }
}

impl<'a, T> core::ops::DerefMut for RwSpinlockWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut *self.inner }
}

impl<'a, T> Drop for RwSpinlockWriteGuard<'a, T> {
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.inner);
            #[cfg(not(target_arch = "wasm32"))]
            if (self.rflags & 0x200) != 0 {
                core::arch::asm!("sti");
            }
        }
    }
}

/// A semaphore for controlling access to a pool of resources.
pub struct Semaphore {
    count: AtomicI32,
}

impl Semaphore {
    pub const fn new(initial: i32) -> Self {
        Self { count: AtomicI32::new(initial) }
    }

    pub fn wait(&self) {
        loop {
            let current = self.count.load(Ordering::Acquire);
            if current > 0 {
                if self.count.compare_exchange(current, current - 1, Ordering::Release, Ordering::Relaxed).is_ok() {
                    break;
                }
            } else {
                #[cfg(not(target_arch = "wasm32"))]
                unsafe { core::arch::asm!("int 0x81"); }
                #[cfg(target_arch = "wasm32")]
                crate::os::yield_task();
            }
        }
    }

    pub fn signal(&self) {
        self.count.fetch_add(1, Ordering::Release);
    }
}
