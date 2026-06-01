use crate::task::manager::{self, TASK_MANAGER};
use core::arch::{asm, naked_asm};
use core::sync::atomic::{AtomicU64, Ordering};

/// Global preemption tick counter. Each CPU's LAPIC timer increments this, so it
/// is shared across all cores; `AtomicU64` makes the per-tick increment race-free
/// (previously a `static mut` written outside any lock — a real SMP data race).
#[unsafe(no_mangle)]
pub static SYSTEM_TICKS: AtomicU64 = AtomicU64::new(0);

pub fn idle() {
    loop {
        unsafe { asm!("hlt") };
    }
}

/// Generates a naked interrupt entry that snapshots every GPR into a `CPUState`
/// frame on the current stack, passes a pointer to it (in `rdi`) to `$switch`,
/// then resumes on the stack pointer that `$switch` returns in `rax`.
///
/// `timer_handler` and `yield_handler` are byte-for-byte identical apart from
/// the switch routine they call, so they share this macro.
macro_rules! context_switch_entry {
    ($name:ident, $switch:ident) => {
        #[unsafe(naked)]
        pub extern "C" fn $name() {
            unsafe {
                naked_asm!(
                    "push rbp",
                    "push rax",
                    "push rbx",
                    "push rcx",
                    "push rdx",
                    "push rsi",
                    "push rdi",
                    "push r8",
                    "push r9",
                    "push r10",
                    "push r11",
                    "push r12",
                    "push r13",
                    "push r14",
                    "push r15",
                    "mov rdi, rsp",
                    "and rsp, -16",
                    concat!("call ", stringify!($switch)),
                    "mov rsp, rax",
                    "pop r15",
                    "pop r14",
                    "pop r13",
                    "pop r12",
                    "pop r11",
                    "pop r10",
                    "pop r9",
                    "pop r8",
                    "pop rdi",
                    "pop rsi",
                    "pop rdx",
                    "pop rcx",
                    "pop rbx",
                    "pop rax",
                    "pop rbp",
                    "iretq",
                );
            }
        }
    };
}

context_switch_entry!(timer_handler, switch_timer);
context_switch_entry!(yield_handler, switch_yield);

#[unsafe(no_mangle)]
pub extern "C" fn switch_timer(rsp: u64) -> u64 {
    unsafe { common_switch(rsp, true) }
}

#[unsafe(no_mangle)]
pub extern "C" fn switch_yield(rsp: u64) -> u64 {
    unsafe { common_switch(rsp, false) }
}
unsafe fn common_switch(rsp: u64, is_timer: bool) -> u64 {
    unsafe {
        let cpu = crate::task::cpu::get_cpu_id() as usize;

        // Bump the shared tick counter atomically (every core's timer hits this).
        let ticks = if is_timer {
            SYSTEM_TICKS.fetch_add(1, Ordering::Relaxed).wrapping_add(1)
        } else {
            SYSTEM_TICKS.load(Ordering::Relaxed)
        };

        // Timer wakeups are processed on a single core. This is the ONLY point on
        // the per-tick path that takes the task-map lock, and only on CPU 0 — every
        // other core's context switch below runs entirely lock-free w.r.t. the map.
        if is_timer && cpu == 0 {
            {
                let mut tm = TASK_MANAGER.lock();
                crate::task::event_manager::EVENT_MANAGER
                    .lock()
                    .check_timers(&mut tm, ticks);
            } // task-map lock dropped before the disk safety poll (which re-takes it)
            // Safety net for a missed disk-completion IRQ.
            crate::fs::virtio::disk_safety_poll();
        }

        // --- Save the outgoing thread (per-CPU `current` slot; no map lock) ---
        let cur = manager::sched_take_current(cpu);
        if let Some(ref c) = cur {
            c.cpu_state_ptr.store(rsp, Ordering::Release);
            // Idle threads never execute FPU/SSE instructions, so their save area
            // holds nothing meaningful (and is never restored from) — skip the
            // ~512-byte fxsave whenever we're leaving an idle thread.
            if !c.is_idle {
                let fpu_ptr = c.fpu_ptr();
                asm!("fxsave [{}]", in(reg) fpu_ptr);
            }
        }

        // Re-enqueue the thread deferred on the previous switch: by now this CPU has
        // left that thread's kernel stack, so another core may safely run it.
        manager::sched_flush_prev(cpu);

        // --- Pick the next thread (per-CPU run-queue locks + atomics only) ---
        let next = manager::sched_pick_next(cpu);

        // Restore its FPU and read its switch frame before handing the Arc to the
        // per-CPU `current` slot. Idle won't read the FPU, so skip the fxrstor when
        // switching into it (the physical regs keep the prior thread's now-saved
        // values, which is harmless since idle never touches them).
        if !next.is_idle {
            let fpu_ptr = next.fpu_ptr();
            asm!("fxrstor [{}]", in(reg) fpu_ptr);
        }
        let k_stack = next.kernel_stack;
        let next_tid = next.tid as i64;
        let new_rsp = next.cpu_state_ptr.load(Ordering::Acquire);

        // Publish `next` as current; defer `cur` for re-enqueue on the next switch.
        manager::sched_set_prev_current(cpu, cur, next);

        asm!("mov gs:[24], {}", in(reg) next_tid);

        if k_stack != 0 {
            crate::arch::x86_64::tss::set_tss(k_stack);
            asm!("mov gs:[8], {}", in(reg) k_stack);
        }

        if is_timer {
            crate::arch::x86_64::exceptions::end_interrupt(crate::arch::x86_64::exceptions::TIMER_INT);
        }

        new_rsp
    }
}
