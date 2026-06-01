use crate::memory::address::PhysAddr;
use crate::memory::{paging, pmm, vmm};
use crate::sync::Mutex;
use crate::task::process::Process;
use crate::task::thread::{CPUState, Thread, ThreadState, NO_PIN};
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::hash::{BuildHasherDefault, Hasher};
use core::sync::atomic::Ordering;
use hashbrown::HashMap;

pub const MAX_THREADS: usize = 999999;
pub const MAX_PROCESSES: usize = 64;
pub const STACK_SIZE: u64 = 1024 * 1024;

/// Number of CPUs the scheduler is sized for. The per-CPU idle threads occupy
/// task ids `0..MAX_CPUS`; user thread ids therefore start at `MAX_CPUS`.
pub const MAX_CPUS: usize = 64;

/// Hasher for integer task-id keys. The keys are small, dense integers that are
/// already well distributed, so storing them verbatim is a perfectly good hash
/// and far cheaper than a general-purpose one on the context-switch lookup path.
#[derive(Default)]
pub struct TidHasher(u64);

impl Hasher for TidHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 << 8) | b as u64;
        }
    }
    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.0 = i as u64;
    }
}

/// Task table keyed by task id, holding reference-counted threads. `Arc` lets the
/// scheduler keep stable handles to threads inside per-CPU run queues and each
/// CPU's current/prev slots WITHOUT holding the task-map lock on the context-switch
/// hot path — the whole point of this design. A thread stays alive as long as it is
/// referenced (running, queued, or pending re-enqueue) even after removal from the map.
type TaskMap = HashMap<usize, Arc<Thread>, BuildHasherDefault<TidHasher>>;

pub struct TaskManager {
    pub thread_count: usize,
    pub tasks: TaskMap,
    pub next_tid: usize,
}

/// Guards only the task *map* (insert / remove / lookup-by-tid), taken by
/// spawn / exit / syscalls. It is NOT taken on the per-tick run-queue scheduling
/// path, so cores no longer serialize on one global lock every timer tick.
pub static TASK_MANAGER: Mutex<TaskManager> = Mutex::new(TaskManager {
    thread_count: 0,
    tasks: HashMap::with_hasher(BuildHasherDefault::new()),
    next_tid: MAX_CPUS,
});

/// Per-CPU run queues, each with its own independent lock and holding `Arc<Thread>`
/// directly. The scheduler enqueues/dequeues here without touching the task map.
pub static RUN_QUEUES: [Mutex<VecDeque<Arc<Thread>>>; MAX_CPUS] =
    [const { Mutex::new(VecDeque::new()) }; MAX_CPUS];

/// Per-CPU scheduler slots. Each entry is only ever accessed by its owning CPU with
/// interrupts disabled (on the context-switch path), so the `UnsafeCell`s are sound.
pub struct PerCpu {
    /// Thread currently running on this CPU.
    pub current: UnsafeCell<Option<Arc<Thread>>>,
    /// Thread we just switched away from, awaiting re-enqueue. Deferred until the
    /// next switch so this CPU has fully left the thread's kernel stack before any
    /// other CPU can run it (closes the context-switch stack-reuse race).
    pub prev: UnsafeCell<Option<Arc<Thread>>>,
    /// This CPU's idle thread (run when no other thread is Ready).
    pub idle: UnsafeCell<Option<Arc<Thread>>>,
}
unsafe impl Sync for PerCpu {}

pub static PER_CPU: [PerCpu; MAX_CPUS] = [const {
    PerCpu {
        current: UnsafeCell::new(None),
        prev: UnsafeCell::new(None),
        idle: UnsafeCell::new(None),
    }
}; MAX_CPUS];

/// Online CPU count, clamped to the scheduler's capacity.
#[inline]
fn online_cpus() -> usize {
    crate::arch::x86_64::smp::CPU_COUNT
        .load(Ordering::Relaxed)
        .clamp(1, MAX_CPUS)
}

/// Enqueue `t` onto a run queue, routing to its pinned CPU if pinned, else to
/// `target_cpu`. Idempotent: a thread already marked queued is left alone.
fn enqueue_arc(t: Arc<Thread>, target_cpu: usize) {
    // swap returns the previous value; if it was already true it's already queued.
    if t.is_queued.swap(true, Ordering::AcqRel) {
        return;
    }
    let pin = t.pinned_cpu.load(Ordering::Relaxed);
    let cpu = if pin != NO_PIN { pin % MAX_CPUS } else { target_cpu % MAX_CPUS };
    RUN_QUEUES[cpu].lock().push_back(t);
}

/// Re-route an already-queued (is_queued==true) thread to its pinned CPU's queue.
/// Used by `sched_pick_next` to fix up mis-located pinned threads.
fn requeue_pinned(t: Arc<Thread>) {
    let pin = t.pinned_cpu.load(Ordering::Relaxed);
    let cpu = if pin != NO_PIN { pin % MAX_CPUS } else { 0 };
    RUN_QUEUES[cpu].lock().push_back(t);
}

/// Take a clone of the thread currently running on `cpu` (interrupts are disabled
/// on the caller's context-switch path, so this CPU-private slot needs no lock).
pub fn sched_take_current(cpu: usize) -> Option<Arc<Thread>> {
    unsafe { (*PER_CPU[cpu].current.get()).clone() }
}

/// Re-enqueue the thread deferred on the previous switch. By now this CPU has long
/// left that thread's kernel stack, so it is safe for another CPU to pick it up.
pub fn sched_flush_prev(cpu: usize) {
    let prev = unsafe { (*PER_CPU[cpu].prev.get()).take() };
    if let Some(p) = prev {
        // We have now switched off `p`'s kernel stack, so it is safe for another CPU
        // to run it. Clear on_cpu BEFORE reading state: paired with the waker (which
        // sets state=Ready before reading on_cpu), this ordering guarantees exactly
        // one of {this flush, the waker} re-enqueues a just-woken thread — no lost
        // wakeup, and never an enqueue while the thread is still executing.
        p.on_cpu.store(false, Ordering::SeqCst);
        // Idle threads are never queued; only Ready threads get re-enqueued (a
        // blocked/zombie thread is dropped here and re-enqueued, if ever, by its
        // wakeup path).
        if !p.is_idle && p.state.load(Ordering::SeqCst) == ThreadState::Ready {
            enqueue_arc(p, cpu);
        }
    }
}

/// Publish `next` as the current thread on `cpu` and defer the outgoing `cur` for
/// re-enqueue on the next switch (see `prev` field docs).
pub fn sched_set_prev_current(cpu: usize, cur: Option<Arc<Thread>>, next: Arc<Thread>) {
    // `next` is now running on this CPU; mark it so wakers don't re-enqueue it.
    next.on_cpu.store(true, Ordering::SeqCst);
    unsafe {
        *PER_CPU[cpu].current.get() = Some(next);
        *PER_CPU[cpu].prev.get() = cur;
    }
}

/// Choose the next thread to run on `cpu` using only per-CPU run-queue locks and
/// atomic thread fields — never the task-map lock. Returns the CPU's idle thread
/// when nothing else is runnable.
pub fn sched_pick_next(cpu: usize) -> Arc<Thread> {
    // 1. Local queue: first Ready task (single pass).
    let mut mis_pinned: Vec<Arc<Thread>> = Vec::new();
    {
        let mut q = RUN_QUEUES[cpu].lock();
        let n = q.len();
        for _ in 0..n {
            let Some(t) = q.pop_front() else { break };
            let pin = t.pinned_cpu.load(Ordering::Relaxed);
            if pin != NO_PIN && pin % MAX_CPUS != cpu {
                // Belongs to a different CPU; redistribute after releasing the lock.
                mis_pinned.push(t);
                continue;
            }
            if t.state.load(Ordering::Acquire) == ThreadState::Ready {
                t.is_queued.store(false, Ordering::Release);
                drop(q);
                for m in mis_pinned {
                    requeue_pinned(m);
                }
                return t;
            } else {
                // Not ready (Sleeping/WaitingForEvent): keep it queued.
                q.push_back(t);
            }
        }
    }
    for m in mis_pinned {
        requeue_pinned(m);
    }

    // 2. Work-stealing: scan only online CPUs, head-only (O(num_cpus)).
    let cpu_count = online_cpus();
    for i in 1..cpu_count {
        let victim = (cpu + i) % cpu_count;
        let mut vq = RUN_QUEUES[victim].lock();
        if let Some(t) = vq.pop_front() {
            if t.pinned_cpu.load(Ordering::Relaxed) == NO_PIN
                && t.state.load(Ordering::Acquire) == ThreadState::Ready
            {
                t.is_queued.store(false, Ordering::Release);
                return t;
            }
            vq.push_back(t);
        }
    }

    // 3. Nothing runnable: this CPU's idle thread.
    unsafe { (*PER_CPU[cpu].idle.get()).clone().expect("idle thread not initialised") }
}

impl TaskManager {
    pub fn init(&mut self) {
        for cpu_id in 0..MAX_CPUS {
            let mut idle_thread = Thread::new(b"idle");
            idle_thread.tid = cpu_id;
            idle_thread.is_idle = true;
            idle_thread.state.store(ThreadState::Ready, Ordering::Relaxed);

            unsafe {
                let kernel_proc = Process::new(0, 0, 0, None);
                idle_thread.process = Some(kernel_proc);

                let stack_pages = (STACK_SIZE / 4096) as usize;
                let stack_phys =
                    pmm::allocate_frames(stack_pages).expect("Idle stack allocation failed");
                idle_thread.kernel_stack = stack_phys + STACK_SIZE + paging::HHDM_OFFSET;

                let state_size = core::mem::size_of::<CPUState>();
                let state_ptr = (idle_thread.kernel_stack - state_size as u64) as *mut CPUState;
                idle_thread.cpu_state_ptr.store(state_ptr as u64, Ordering::Relaxed);

                (*state_ptr).rip = crate::task::scheduler::idle as u64;
                (*state_ptr).cs = 0x08; // 64-bit kernel code segment (GDT index 1)
                (*state_ptr).rflags = 0x202;
                (*state_ptr).rsp = idle_thread.kernel_stack;
                (*state_ptr).ss = 0x10;
            }

            let arc = Arc::new(idle_thread);
            // Keep a per-CPU handle for the scheduler's idle fallback, plus a map
            // entry (key == cpu_id) so tid-based lookups still resolve it.
            unsafe {
                *PER_CPU[cpu_id].idle.get() = Some(arc.clone());
            }
            self.tasks.insert(cpu_id, arc);
        }
        self.thread_count = MAX_CPUS;
    }

    pub fn current_task_idx(&self) -> Option<usize> {
        let idx = crate::task::cpu::get_current_task_idx();
        if idx >= 0 { Some(idx as usize) } else { None }
    }

    /// Pin a thread to a specific CPU. It will only ever run on that CPU.
    pub fn pin_thread_to_cpu(&self, tid: usize, cpu_id: usize) {
        if let Some(thread) = self.tasks.get(&tid) {
            thread.pinned_cpu.store(cpu_id, Ordering::Relaxed);
        }
    }

    pub fn reserve_pid(&mut self) -> Result<usize, pmm::FrameError> {
        let tid = self.next_tid;
        self.next_tid += 1;
        let mut t = Thread::new(b"reserved");
        t.tid = tid;
        t.state.store(ThreadState::Reserved, Ordering::Relaxed);

        self.tasks.insert(tid, Arc::new(t));
        self.thread_count += 1;
        Ok(tid)
    }

    /// Wake / re-queue an existing thread. Callers set `state = Ready` BEFORE calling
    /// this (the required ordering for the race-free wake protocol). If the thread is
    /// still executing on a CPU (`on_cpu`), we do NOT enqueue it here — that CPU's
    /// `sched_flush_prev` will enqueue it once it has switched off the thread's stack
    /// (and observes the Ready state we just set). This prevents the woken thread from
    /// being run on a second CPU while still live on the first.
    pub fn push_to_run_queue(&self, tid: usize) {
        if tid < MAX_CPUS {
            return;
        }
        if let Some(thread) = self.tasks.get(&tid) {
            if !thread.on_cpu.load(Ordering::SeqCst) {
                enqueue_arc(thread.clone(), crate::task::cpu::get_cpu_id() as usize);
            }
            // else: still on-cpu — flush_prev on its CPU will enqueue it.
        }
    }

    /// Push a newly spawned task to the least-loaded online CPU's run queue (or its
    /// pinned CPU). Use `push_to_run_queue` when re-queuing an existing thread.
    pub fn push_new_task(&self, tid: usize) {
        if tid < MAX_CPUS {
            return;
        }
        let Some(thread) = self.tasks.get(&tid) else { return };
        let thread = thread.clone();
        if thread.pinned_cpu.load(Ordering::Relaxed) != NO_PIN {
            enqueue_arc(thread, 0);
            return;
        }
        // Choose the least-loaded online CPU by current queue length.
        let cpu_count = online_cpus();
        let mut best_cpu = 0usize;
        let mut min_len = usize::MAX;
        for i in 0..cpu_count {
            let len = RUN_QUEUES[i].lock().len();
            if len < min_len {
                min_len = len;
                best_cpu = i;
            }
        }
        enqueue_arc(thread, best_cpu);
    }

    pub fn init_user_task(
        &mut self,
        slot: usize,
        entry_point: u64,
        arg: u64,
        _pml4: u64,
        args: Option<&[&str]>,
        fd_table: Option<Vec<i16>>,
        name: &[u8],
        terminal_size: (u16, u16),
        parent_pid: Option<u64>,
    ) -> Result<(), pmm::FrameError> {
        crate::spawn_debugln!(
            "[TaskManager] init_user_task entry: slot={}, entry={:#x}",
            slot,
            entry_point
        );
        let pid = slot as u64;
        crate::spawn_debugln!("[TaskManager] init_user_task: pid={}", pid);

        // Build a fresh thread for this slot, replacing any existing entry (its Arc
        // is dropped once nothing else references it). We mutate it freely here as an
        // owned local, then wrap it in `Arc` at insertion.
        self.tasks.remove(&slot);
        let mut thread = Thread::new(name);
        thread.tid = slot;
        crate::spawn_debugln!("[TaskManager] init_user_task: thread obtained");

        crate::spawn_debugln!("[TaskManager] init_user_task: resolving uid/gid");
        let (uid, gid) = if let Some(ppid) = parent_pid {
            crate::spawn_debugln!("[TaskManager] init_user_task: parent_pid={}", ppid);
            crate::spawn_debugln!("[TaskManager] init_user_task: calling tasks.values");
            let parent = self
                .tasks
                .values()
                .find(|t| t.process.as_ref().map_or(false, |p| p.pid == ppid));
            crate::spawn_debugln!("[TaskManager] init_user_task: find returned");
            if let Some(parent_thread) = parent {
                crate::spawn_debugln!("[TaskManager] init_user_task: parent found");
                let p = parent_thread.process.as_ref().unwrap();
                (p.uid, p.gid)
            } else {
                crate::spawn_debugln!(
                    "[TaskManager] init_user_task: parent not found, defaulting to 0,0"
                );
                (0, 0)
            }
        } else {
            crate::spawn_debugln!("[TaskManager] init_user_task: no parent_pid, defaulting to 0,0");
            (0, 0)
        };
        crate::spawn_debugln!("[TaskManager] init_user_task: uid={}, gid={}", uid, gid);

        crate::spawn_debugln!("[TaskManager] init_user_task: calling Process::new");
        let proc = Process::new(pid, uid, gid, parent_pid);
        crate::spawn_debugln!("[TaskManager] init_user_task: Process::new returned");

        if let Some(fds) = fd_table {
            crate::spawn_debugln!("[TaskManager] init_user_task: setting fd_table");
            crate::spawn_debugln!("[TaskManager] init_user_task: calling fd_table.lock");
            let mut guard = proc.fd_table.lock();
            crate::spawn_debugln!("[TaskManager] init_user_task: lock acquired");
            *guard = fds;
            crate::spawn_debugln!("[TaskManager] init_user_task: fd_table updated");
        }
        crate::spawn_debugln!("[TaskManager] init_user_task: setting terminal size");
        crate::spawn_debugln!("[TaskManager] init_user_task: calling terminal_width.lock");
        *proc.terminal_width.lock() = terminal_size.0;
        crate::spawn_debugln!("[TaskManager] init_user_task: terminal_width set");
        crate::spawn_debugln!("[TaskManager] init_user_task: calling terminal_height.lock");
        *proc.terminal_height.lock() = terminal_size.1;
        crate::spawn_debugln!("[TaskManager] init_user_task: terminal_height set");

        crate::spawn_debugln!("[TaskManager] init_user_task: calling proc.clone");
        thread.process = Some(proc.clone());
        crate::spawn_debugln!("[TaskManager] init_user_task: proc.clone returned");

        crate::spawn_debugln!("[TaskManager] init_user_task: calling pmm::allocate_frames(256)");
        let k_frame = pmm::allocate_frames(256).ok_or(pmm::FrameError::NoMemory)?;
        crate::spawn_debugln!("[TaskManager] init_user_task: k_frame={:#x}", k_frame);
        thread.kernel_stack = k_frame + 1024 * 1024 + paging::HHDM_OFFSET;
        crate::spawn_debugln!(
            "[TaskManager] init_user_task: kernel_stack={:#x}",
            thread.kernel_stack
        );

        let stack_pages = (STACK_SIZE / 4096) as usize;
        crate::spawn_debugln!(
            "[TaskManager] init_user_task: calling pmm::allocate_frames({})",
            stack_pages
        );
        let u_frame_phys = pmm::allocate_frames(stack_pages).ok_or(pmm::FrameError::NoMemory)?;
        crate::spawn_debugln!(
            "[TaskManager] init_user_task: u_frame_phys={:#x}",
            u_frame_phys
        );

        let u_stack_top = proc.stack_base;
        let u_stack_base = u_stack_top - STACK_SIZE;
        crate::spawn_debugln!(
            "[TaskManager] init_user_task: u_stack_top={:#x}, u_stack_base={:#x}",
            u_stack_top,
            u_stack_base
        );

        crate::spawn_debugln!("[TaskManager] init_user_task: starting stack mapping loop");
        for i in 0..stack_pages {
            let offset = i as u64 * 4096;
            //crate::spawn_debugln!("[TaskManager] init_user_task: mapping stack page {}", i);
            vmm::map_page(
                u_stack_base + offset,
                PhysAddr::new(u_frame_phys + offset),
                paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER,
                None,
            );
        }
        crate::spawn_debugln!("[TaskManager] init_user_task: stack mapping loop finished");
        thread.user_stack = u_stack_top;
        crate::spawn_debugln!("[TaskManager] init_user_task: thread.user_stack set");

        let code_pages = (64 * 1024 * 1024) / 4096;
        crate::spawn_debugln!(
            "[TaskManager] init_user_task: starting code mapping loop, code_pages={}",
            code_pages
        );
        for i in 0..code_pages {
            let virt = proc.code_base + i as u64 * 4096;
            if i < 4096 {
                if i % 1024 == 0 {
                    crate::spawn_debugln!("[TaskManager] init_user_task: mapping code page {}", i);
                }
                if let Some(frame) = pmm::allocate_frame() {
                    vmm::map_page(
                        virt,
                        PhysAddr::new(frame),
                        paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER,
                        None,
                    );
                }
            }
        }
        crate::spawn_debugln!("[TaskManager] init_user_task: code mapping loop finished");

        crate::spawn_debugln!("[TaskManager] init_user_task: calling core::mem::size_of");
        let state_size = core::mem::size_of::<CPUState>();
        crate::spawn_debugln!("[TaskManager] init_user_task: state_size={}", state_size);
        let state_ptr = (thread.kernel_stack - state_size as u64) as *mut CPUState;
        thread.cpu_state_ptr.store(state_ptr as u64, Ordering::Relaxed);
        crate::spawn_debugln!(
            "[TaskManager] init_user_task: thread.cpu_state_ptr={:#x}",
            thread.cpu_state_ptr.load(Ordering::Relaxed)
        );

        unsafe {
            crate::spawn_debugln!(
                "[TaskManager] init_user_task: entering unsafe block for stack preparation"
            );
            let stack_phys_base = u_frame_phys + paging::HHDM_OFFSET;
            let mut current_virt_sp = thread.user_stack;
            crate::spawn_debugln!(
                "[TaskManager] init_user_task: stack_phys_base={:#x}, current_virt_sp={:#x}",
                stack_phys_base,
                current_virt_sp
            );

            crate::spawn_debugln!("[TaskManager] init_user_task: calling Vec::new");
            let mut arg_ptrs = Vec::new();
            crate::spawn_debugln!("[TaskManager] init_user_task: Vec::new returned");

            let mut push_str = |s: &[u8]| {
                let len = s.len() + 1;
                current_virt_sp -= len as u64;
                let offset = current_virt_sp - u_stack_base;
                let dest = (stack_phys_base + offset) as *mut u8;
                crate::spawn_debugln!(
                    "[TaskManager] init_user_task: pushing string of len {}, dest={:#x}",
                    s.len(),
                    dest as u64
                );
                core::ptr::copy_nonoverlapping(s.as_ptr(), dest, s.len());
                *dest.add(s.len()) = 0;
                current_virt_sp
            };

            crate::spawn_debugln!("[TaskManager] init_user_task: pushing name");
            let name_ptr = push_str(name);
            arg_ptrs.push(name_ptr);
            crate::spawn_debugln!(
                "[TaskManager] init_user_task: name pushed at {:#x}",
                name_ptr
            );

            if let Some(a_list) = args {
                crate::spawn_debugln!("[TaskManager] init_user_task: starting args push loop");
                for &a in a_list {
                    crate::spawn_debugln!("[TaskManager] init_user_task: pushing arg");
                    let a_ptr = push_str(a.as_bytes());
                    arg_ptrs.push(a_ptr);
                }
                crate::spawn_debugln!("[TaskManager] init_user_task: args push loop finished");
            }

            current_virt_sp &= !15;
            crate::spawn_debugln!(
                "[TaskManager] init_user_task: current_virt_sp aligned to {:#x}",
                current_virt_sp
            );
            let mut push_u64 = |val: u64| {
                current_virt_sp -= 8;
                let offset = current_virt_sp - u_stack_base;
                let dest = (stack_phys_base + offset) as *mut u64;
                crate::spawn_debugln!(
                    "[TaskManager] init_user_task: pushing u64={:#x} to dest={:#x}",
                    val,
                    dest as u64
                );
                *dest = val;
            };

            crate::spawn_debugln!("[TaskManager] init_user_task: pushing stack frames");
            push_u64(0);
            push_u64(0);
            crate::spawn_debugln!("[TaskManager] init_user_task: starting arg_ptrs rev loop");
            for &ptr in arg_ptrs.iter().rev() {
                push_u64(ptr);
            }
            crate::spawn_debugln!("[TaskManager] init_user_task: arg_ptrs rev loop finished");
            push_u64(arg_ptrs.len() as u64);
            crate::spawn_debugln!(
                "[TaskManager] init_user_task: arg_count={} pushed",
                arg_ptrs.len()
            );

            crate::spawn_debugln!("[TaskManager] init_user_task: calling core::ptr::write_bytes");
            core::ptr::write_bytes(state_ptr, 0, 1);
            crate::spawn_debugln!("[TaskManager] init_user_task: state_ptr zeroed");
            (*state_ptr).rip = entry_point;
            (*state_ptr).rdi = arg;
            (*state_ptr).cs = 0x23;
            (*state_ptr).rflags = 0x202;
            (*state_ptr).rsp = (current_virt_sp & !15) - 8;
            (*state_ptr).ss = 0x1B;
            let final_rsp = (*state_ptr).rsp;
            crate::spawn_debugln!(
                "[TaskManager] init_user_task: CPUState initialized, rsp={:#x}",
                final_rsp
            );
        }

        let final_state = if entry_point == 0 {
            ThreadState::Reserved
        } else {
            ThreadState::Ready
        };
        thread.state.store(final_state, Ordering::Relaxed);
        crate::spawn_debugln!(
            "[TaskManager] init_user_task: final_state={:?}",
            thread.state.load(Ordering::Relaxed)
        );
        let ptr = &thread as *const _ as u64;
        crate::spawn_debugln!(
            "[TaskManager] Initialized User Task {} (Thread at {:#x}, State={:?})",
            slot,
            ptr,
            thread.state.load(Ordering::Relaxed)
        );

        let is_ready = final_state == ThreadState::Ready;
        crate::spawn_debugln!("[TaskManager] init_user_task: calling tasks.insert");
        self.tasks.insert(slot, Arc::new(thread));
        crate::spawn_debugln!("[TaskManager] init_user_task: tasks.insert returned");
        if is_ready {
            crate::spawn_debugln!("[TaskManager] init_user_task: calling push_new_task");
            self.push_new_task(slot);
            crate::spawn_debugln!("[TaskManager] init_user_task: push_new_task returned");
        }
        crate::spawn_debugln!("[TaskManager] init_user_task exit success");
        Ok(())
    }

    pub fn spawn_thread(
        &mut self,
        parent_tid: usize,
        entry_point: u64,
        user_stack: u64,
        arg: u64,
    ) -> Result<usize, pmm::FrameError> {
        let tid = self.reserve_pid()?;

        let parent_process = if let Some(t) = self.tasks.get(&(parent_tid)) {
            if let Some(p) = &t.process {
                p.clone()
            } else {
                return Err(pmm::FrameError::IndexOutOfBounds);
            }
        } else {
            return Err(pmm::FrameError::IndexOutOfBounds);
        };

        let mut thread = Thread::new(b"thread");
        thread.tid = tid;
        thread.process = Some(parent_process.clone());

        let k_frame = pmm::allocate_frames(256).ok_or(pmm::FrameError::NoMemory)?;
        thread.kernel_stack = k_frame + 1024 * 1024 + paging::HHDM_OFFSET;

        let state_size = core::mem::size_of::<CPUState>();
        let state_ptr = (thread.kernel_stack - state_size as u64) as *mut CPUState;
        thread.cpu_state_ptr.store(state_ptr as u64, Ordering::Relaxed);

        unsafe {
            core::ptr::write_bytes(state_ptr, 0, 1);
            (*state_ptr).rip = entry_point;

            if user_stack == 0 {
                (*state_ptr).cs = 0x08;
                (*state_ptr).ss = 0x10;
                (*state_ptr).rsp = thread.kernel_stack - state_size as u64 - 8;
                (*state_ptr).rflags = 0x202;
            } else {
                (*state_ptr).cs = 0x23;
                (*state_ptr).ss = 0x1B;
                (*state_ptr).rsp = (user_stack & !15) - 8;
                (*state_ptr).rflags = 0x202;
            }

            (*state_ptr).rdi = arg;
        }

        thread.state.store(ThreadState::Ready, Ordering::Relaxed);
        self.tasks.insert(tid, Arc::new(thread));
        self.push_new_task(tid);

        Ok(tid)
    }

    pub fn get_tasks(&self) -> hashbrown::hash_map::Values<'_, usize, Arc<Thread>> {
        self.tasks.values()
    }

    pub fn current_thread(&self) -> &Thread {
        // Fall back to this CPU's idle task rather than panicking if the lookup
        // misses (current_task_idx can be briefly -1 during bring-up/teardown).
        let raw = crate::task::cpu::get_current_task_idx();
        let cpu = crate::task::cpu::get_cpu_id() as usize;
        let key = if raw >= 0 && self.tasks.contains_key(&(raw as usize)) {
            raw as usize
        } else {
            cpu
        };
        &**self.tasks.get(&key).expect("idle task missing")
    }
}

pub fn kill_process(pid: u64) {
    {
        let mut composer = crate::window_manager::composer::COMPOSER.write();
        let mut to_remove = alloc::vec::Vec::new();
        for ws in 0..5 {
            for w in composer.workspaces[ws].windows.iter() {
                if w.pid == pid {
                    to_remove.push(w.id);
                }
            }
        }
        if composer.wallpaper.pid == pid {
            to_remove.push(composer.wallpaper.id);
        }
        for wid in to_remove {
            composer.remove_window(wid);
        }
    }

    {
        let tm = TASK_MANAGER.lock();
        for thread in tm.tasks.values() {
            if let Some(proc) = &thread.process {
                if proc.pid == pid {
                    thread.state.store(ThreadState::Zombie, Ordering::Release);
                    *proc.event_queue.lock() = (0, 0, 0);
                }
            }
        }
    }
}
