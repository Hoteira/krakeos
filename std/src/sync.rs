use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};

/// A simple spinlock for very short critical sections.
/// Disables interrupts on the current CPU while held.
pub struct Spinlock<T> {
    lock: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for Spinlock<T> {}
unsafe impl<T: Send> Send for Spinlock<T> {}

pub struct SpinlockGuard<'a, T> {
    lock: &'a AtomicBool,
    data: &'a mut T,
    rflags: u64,
}

impl<T> Spinlock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            lock: AtomicBool::new(false),
            data: UnsafeCell::new(data),
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

        let mut count = 0;
        while self.lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
            #[cfg(target_arch = "wasm32")]
            {
                count += 1;
                if count >= 1000 {
                    count = 0;
                    crate::os::yield_task();
                }
            }
        }

        SpinlockGuard {
            lock: &self.lock,
            data: unsafe { &mut *self.data.get() },
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

        if self.lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            Some(SpinlockGuard {
                lock: &self.lock,
                data: unsafe { &mut *self.data.get() },
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
    fn deref(&self) -> &Self::Target { self.data }
}

impl<'a, T> core::ops::DerefMut for SpinlockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target { self.data }
}

impl<'a, T> Drop for SpinlockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.store(false, Ordering::Release);
        #[cfg(not(target_arch = "wasm32"))]
        unsafe {
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
    lock: AtomicBool,
    data: UnsafeCell<T>,
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
    lock: &'a AtomicBool,
    data: &'a mut T,
}

impl<T> Mutex<T> {
    pub const fn new(data: T) -> Self {
        Self {
            lock: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, T> {
        while self.lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            #[cfg(not(target_arch = "wasm32"))]
            unsafe { core::arch::asm!("int 0x81"); }
            #[cfg(target_arch = "wasm32")]
            crate::os::yield_task();
        }
        MutexGuard {
            lock: &self.lock,
            data: unsafe { &mut *self.data.get() },
        }
    }

    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        if self.lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            Some(MutexGuard {
                lock: &self.lock,
                data: unsafe { &mut *self.data.get() },
            })
        } else {
            None
        }
    }
}

impl<'a, T> core::ops::Deref for MutexGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target { self.data }
}

impl<'a, T> core::ops::DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target { self.data }
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.store(false, Ordering::Release);
    }
}

/// A yielding Reader-Writer lock.
pub struct RwLock<T> {
    state: AtomicI32, // negative = writer, positive = readers, 0 = free
    data: UnsafeCell<T>,
}

unsafe impl<T: Send + Sync> Sync for RwLock<T> {}
unsafe impl<T: Send + Sync> Send for RwLock<T> {}

pub struct RwLockReadGuard<'a, T> {
    lock: &'a AtomicI32,
    data: &'a T,
}

pub struct RwLockWriteGuard<'a, T> {
    lock: &'a AtomicI32,
    data: &'a mut T,
}

impl<T> RwLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            state: AtomicI32::new(0),
            data: UnsafeCell::new(data),
        }
    }

    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        loop {
            let current = self.state.load(Ordering::Acquire);
            if current >= 0 {
                if self.state.compare_exchange(current, current + 1, Ordering::Release, Ordering::Relaxed).is_ok() {
                    break;
                }
            } else {
                #[cfg(not(target_arch = "wasm32"))]
                unsafe { core::arch::asm!("int 0x81"); }
                #[cfg(target_arch = "wasm32")]
                crate::os::yield_task();
            }
        }
        RwLockReadGuard {
            lock: &self.state,
            data: unsafe { &*self.data.get() },
        }
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        loop {
            if self.state.compare_exchange(0, -1, Ordering::Acquire, Ordering::Relaxed).is_ok() {
                break;
            }
            #[cfg(not(target_arch = "wasm32"))]
            unsafe { core::arch::asm!("int 0x81"); }
            #[cfg(target_arch = "wasm32")]
            crate::os::yield_task();
        }
        RwLockWriteGuard {
            lock: &self.state,
            data: unsafe { &mut *self.data.get() },
        }
    }
}

impl<'a, T> core::ops::Deref for RwLockReadGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target { self.data }
}

impl<'a, T> Drop for RwLockReadGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.fetch_sub(1, Ordering::Release);
    }
}

impl<'a, T> core::ops::Deref for RwLockWriteGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target { self.data }
}

impl<'a, T> core::ops::DerefMut for RwLockWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target { self.data }
}

impl<'a, T> Drop for RwLockWriteGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.store(0, Ordering::Release);
    }
}

/// A non-yielding Reader-Writer spinlock.
/// Safe for use in interrupt contexts.
pub struct RwSpinlock<T> {
    state: AtomicI32,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send + Sync> Sync for RwSpinlock<T> {}
unsafe impl<T: Send + Sync> Send for RwSpinlock<T> {}

pub struct RwSpinlockReadGuard<'a, T> {
    lock: &'a AtomicI32,
    data: &'a T,
    rflags: u64,
}

pub struct RwSpinlockWriteGuard<'a, T> {
    lock: &'a AtomicI32,
    data: &'a mut T,
    rflags: u64,
}

impl<T> RwSpinlock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            state: AtomicI32::new(0),
            data: UnsafeCell::new(data),
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

        loop {
            let current = self.state.load(Ordering::Acquire);
            if current >= 0 {
                if self.state.compare_exchange(current, current + 1, Ordering::Release, Ordering::Relaxed).is_ok() {
                    break;
                }
            }
            core::hint::spin_loop();
        }
        RwSpinlockReadGuard {
            lock: &self.state,
            data: unsafe { &*self.data.get() },
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

        loop {
            if self.state.compare_exchange(0, -1, Ordering::Acquire, Ordering::Relaxed).is_ok() {
                break;
            }
            core::hint::spin_loop();
        }
        RwSpinlockWriteGuard {
            lock: &self.state,
            data: unsafe { &mut *self.data.get() },
            rflags,
        }
    }
}

impl<'a, T> core::ops::Deref for RwSpinlockReadGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target { self.data }
}

impl<'a, T> Drop for RwSpinlockReadGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.fetch_sub(1, Ordering::Release);
        #[cfg(not(target_arch = "wasm32"))]
        unsafe {
            if (self.rflags & 0x200) != 0 {
                core::arch::asm!("sti");
            }
        }
    }
}

impl<'a, T> core::ops::Deref for RwSpinlockWriteGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target { self.data }
}

impl<'a, T> core::ops::DerefMut for RwSpinlockWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target { self.data }
}

impl<'a, T> Drop for RwSpinlockWriteGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.store(0, Ordering::Release);
        #[cfg(not(target_arch = "wasm32"))]
        unsafe {
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
