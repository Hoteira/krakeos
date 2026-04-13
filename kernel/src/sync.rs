// ============================================================================
// Lock type rules
// ============================================================================
//
//  Mutex<T>      alias for Spinlock<T>
//    - Disables interrupts (pushfq/cli on acquire, conditional sti on drop).
//    - Safe in ANY context including ISRs.
//    - Hold duration MUST be short (< ~100 instructions). Never perform I/O,
//      disk access, or GPU flush while holding a Mutex.
//
//  RwSpinlock<T>
//    - Both read() and write() disable interrupts exactly like Mutex.
//    - Safe in ISR contexts, but same hold-time rules apply.
//    - Prefer read() when no mutation is needed to allow concurrent reads.
//
//  YieldMutex<T> alias for std::sync::Mutex<T>   (yielding, uses int 0x81)
//  YieldRwLock<T>
//    - Does NOT disable interrupts. Issues int 0x81 (yield) on contention.
//    - MUST NOT be used in ISR handlers (keyboard, mouse, timer, page-fault).
//    - Suitable for longer critical sections in thread context only.
//
// ============================================================================
// Global lock ordering (always acquire in this order to prevent deadlock)
// ============================================================================
//
//   PMM → VMM → Tasks → EventManager → Ext2 → VirtioBlk → VirtioGPU
//       → DisplayServer → Composer → Events → Mouse
//
//  Rules:
//  1. Never acquire a lock that appears EARLIER in the order while holding
//     one that appears LATER.
//  2. ISR handlers may hold: PMM, VMM, Tasks (int_lock), DisplayServer,
//     Composer, Events, Mouse — all as Mutex/RwSpinlock (interrupt-safe).
//     ISR handlers MUST NOT hold YieldMutex or YieldRwLock.
//  3. When compositing (Composer + DisplayServer), always drop
//     COMPOSER.write() before acquiring DISPLAY_SERVER if possible.
//     Use COMPOSER.read() for render-only operations (update_window_area_rect,
//     recompose_all) to allow concurrent read access.
//  4. TASK_MANAGER must never be acquired while holding COMPOSER (write).
//     Deliver events to processes AFTER releasing COMPOSER.
//
// ============================================================================

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
