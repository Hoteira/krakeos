use crate::task::process::Process;
use alloc::sync::Arc;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u64)]
pub enum ThreadState {
    Null,
    Ready,
    Zombie,
    Sleeping,
    Blocked,
    Reserved,
    WaitingForEvent,
}

/// Atomic wrapper over `ThreadState`. The scheduler reads/writes a thread's state
/// from any CPU without the task-map lock, so it must be atomic. `ThreadState` is
/// `#[repr(u64)]`, and only valid discriminants are ever stored, so the transmute
/// on load is sound.
#[repr(transparent)]
pub struct AtomicThreadState(AtomicU64);

impl AtomicThreadState {
    pub const fn new(s: ThreadState) -> Self {
        Self(AtomicU64::new(s as u64))
    }
    #[inline]
    pub fn load(&self, order: Ordering) -> ThreadState {
        unsafe { core::mem::transmute::<u64, ThreadState>(self.0.load(order)) }
    }
    #[inline]
    pub fn store(&self, s: ThreadState, order: Ordering) {
        self.0.store(s as u64, order);
    }
}

/// Sentinel stored in `pinned_cpu` to mean "not pinned" (since we use an atomic
/// integer rather than `Option<usize>`).
pub const NO_PIN: usize = usize::MAX;

#[repr(C)]
pub struct Thread {
    /// x87/SSE state for fxsave/fxrstor. Touched only by the single CPU currently
    /// running this thread, so an `UnsafeCell` (with the manual `Sync` impl below)
    /// is sound.
    pub fpu_state: UnsafeCell<[u8; 528]>,
    /// Map key / scheduler id for this thread (mirrors the `TaskManager.tasks` key).
    pub tid: usize,
    /// Set once at construction; read-only afterwards.
    pub kernel_stack: u64,
    pub user_stack: u64,
    /// Saved stack pointer (points at the `CPUState` frame). Written on every
    /// context switch from any CPU → atomic.
    pub cpu_state_ptr: AtomicU64,
    pub state: AtomicThreadState,
    pub wake_ticks: u64,
    pub exit_code: AtomicU64,
    pub name: [u8; 32],
    pub uid: u32,
    pub gid: u32,
    /// Whether this thread currently sits in a run queue (guards double-enqueue
    /// across cores).
    pub is_queued: AtomicBool,
    /// Per-CPU idle threads are never enqueued and never deferred for re-enqueue.
    pub is_idle: bool,
    /// Set once at construction; read-only afterwards.
    pub process: Option<Arc<Process>>,
    /// CPU this thread is pinned to, or `NO_PIN`. Written by `pin_thread_to_cpu`.
    pub pinned_cpu: AtomicUsize,
}

// A `Thread` is only ever mutated through atomics, or (for `fpu_state`) by the
// single CPU that currently owns it; the scheduler guarantees a thread is in at
// most one place (one run queue, or one CPU's current/prev slot) at a time. That
// invariant makes cross-CPU sharing via `Arc<Thread>` sound.
unsafe impl Sync for Thread {}
unsafe impl Send for Thread {}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct CPUState {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    pub rbp: u64,

    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl Thread {
    pub fn new(name: &[u8]) -> Self {
        let mut t_name = [0; 32];
        let len = core::cmp::min(name.len(), 32);
        t_name[..len].copy_from_slice(&name[..len]);

        let mut fpu_state = [0u8; 528];
        // Initialize x87 FCW at offset 0 to 0x037F (all x87 exceptions masked, double precision)
        fpu_state[0] = 0x7F;
        fpu_state[1] = 0x03;
        // Initialize MXCSR at offset 24 to 0x1F80 (all SSE exceptions masked)
        fpu_state[24] = 0x80;
        fpu_state[25] = 0x1F;

        Self {
            fpu_state: UnsafeCell::new(fpu_state),
            tid: 0,
            kernel_stack: 0,
            user_stack: 0,
            cpu_state_ptr: AtomicU64::new(0),
            state: AtomicThreadState::new(ThreadState::Null),
            wake_ticks: 0,
            exit_code: AtomicU64::new(0),
            name: t_name,
            uid: 0,
            gid: 0,
            is_queued: AtomicBool::new(false),
            is_idle: false,
            process: None,
            pinned_cpu: AtomicUsize::new(NO_PIN),
        }
    }

    /// 16-byte-aligned pointer into the FPU save area for fxsave/fxrstor.
    #[inline]
    pub fn fpu_ptr(&self) -> u64 {
        let raw = self.fpu_state.get() as u64;
        (raw + 15) & !15
    }
}
