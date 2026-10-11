use crate::trap::TrapFrame;
use crate::fpu::FpuFrame;
use core::alloc::Layout;

pub const MAX_THREADS: usize = 16;

#[derive(Copy, Clone, PartialEq)]
pub enum ThreadState {
    Empty,
    Ready,
    Running,
    Waiting,
    Sleeping(u64),
    // Stacks are freed by reap_dead once no hart is on its kernel stack.
    Dead,
}

pub const THREAD_NAME_LEN: usize = 32;
const KSTACK_SIZE: usize = 1024 * 1024;
const USTACK_SIZE: usize = 1024 * 1024;

#[derive(Copy, Clone)]
pub struct Thread {
    pub sp: usize,
    pub state: ThreadState,
    pub fpu_state: FpuFrame,
    // From spawn_user_thread, freed when the thread dies (0 for the ELF main thread).
    pub kstack: usize,
    pub ustack: usize,
    pub name: [u8; THREAD_NAME_LEN],
    pub name_len: usize,
    // Set by SYS_SETNAME; only killable threads accept SYS_KILL.
    pub killable: bool,
    // Next SYS_SLEEP returns 1 so the runner unwinds the app at a host-call boundary.
    pub kill_pending: bool,
}

impl Thread {
    pub const fn empty() -> Self {
        Thread {
            sp: 0,
            state: ThreadState::Empty,
            fpu_state: FpuFrame::new(),
            kstack: 0,
            ustack: 0,
            name: [0; THREAD_NAME_LEN],
            name_len: 0,
            killable: false,
            kill_pending: false,
        }
    }
}

pub struct Scheduler {
    pub threads: [Thread; MAX_THREADS],
}

pub static mut SCHEDULER: Scheduler = Scheduler {
    threads: [Thread::empty(); MAX_THREADS],
};

use crate::arch::smp::{hart, MAX_HARTS};

// CURRENT value for a hart running its idle loop.
const IDLE: usize = usize::MAX;
static mut CURRENT: [usize; MAX_HARTS] = [IDLE; MAX_HARTS];
// Where each hart's idle loop was interrupted.
static mut IDLE_SP: [usize; MAX_HARTS] = [0; MAX_HARTS];
// Released on switch-out, so a thread can move to another hart.
static mut FPU_OWNER: [Option<usize>; MAX_HARTS] = [None; MAX_HARTS];
// Last thread picked, so the round-robin is shared by all harts.
static mut ROTOR: usize = 0;

// 0 (the reserved kernel slot) when idle.
pub fn current_tid() -> usize {
    let c = unsafe { CURRENT[hart()] };
    if c == IDLE { 0 } else { c }
}

unsafe fn on_cpu(tid: usize) -> bool {
    (0..MAX_HARTS).any(|h| CURRENT[h] == tid)
}

impl Scheduler {
    // Thread 0 is never scheduled; each hart has its own idle loop instead.
    pub fn init_main() {
        unsafe {
            SCHEDULER.threads[0].state = ThreadState::Running;
        }
    }

    pub fn spawn_user_thread(entry: usize, arg: usize) -> Option<usize> {
        unsafe {
            let slot = (0..MAX_THREADS).find(|&i| SCHEDULER.threads[i].state == ThreadState::Empty)?;

            let layout = Layout::from_size_align(KSTACK_SIZE, 4096).unwrap();
            let kernel_stack = alloc::alloc::alloc_zeroed(layout) as usize;
            if kernel_stack == 0 { crate::println!("OOM kernel stack"); return None; }
            let user_layout = Layout::from_size_align(USTACK_SIZE, 4096).unwrap();
            let user_stack = alloc::alloc::alloc_zeroed(user_layout) as usize;
            if user_stack == 0 {
                crate::println!("OOM user stack");
                alloc::alloc::dealloc(kernel_stack as *mut u8, layout);
                return None;
            }

            crate::paging::map_range(
                crate::ROOT_PAGE_TABLE, 
                user_stack, 
                user_stack, 
                USTACK_SIZE,
                crate::paging::PTE_R | crate::paging::PTE_W | crate::paging::PTE_U
            );
            crate::csr::sfence_vma();

            let trap_frame_ptr = (kernel_stack + KSTACK_SIZE - crate::arch::trap::FRAME_GAP - core::mem::size_of::<TrapFrame>()) as *mut TrapFrame;

            (*trap_frame_ptr).sepc = entry;
            (*trap_frame_ptr).sstatus = 1 << 5; // SPIE=1, SPP=0 (U-mode)
            (*trap_frame_ptr).regs[2] = user_stack + USTACK_SIZE;
            (*trap_frame_ptr).regs[10] = arg;

            let t = &mut SCHEDULER.threads[slot];
            *t = Thread::empty();
            t.sp = trap_frame_ptr as usize;
            t.kstack = kernel_stack;
            t.ustack = user_stack;
            t.state = ThreadState::Ready;
            Some(slot)
        }
    }

    // Stacks are freed later by reap_dead.
    pub fn exit_current() {
        unsafe {
            let tid = current_tid();
            crate::sys::desktop::destroy_of(tid);
            crate::sys::appmem::release_all_of(tid);
            crate::sys::pipe::thread_exited(tid);
            SCHEDULER.threads[tid].state = ThreadState::Dead;
            let h = hart();
            if FPU_OWNER[h] == Some(tid) {
                FPU_OWNER[h] = None;
            }
        }
    }

    // The shell and runner-internal threads aren't killable.
    pub fn kill(tid: usize) -> bool {
        unsafe {
            if tid == 0 || tid >= MAX_THREADS { return false; }
            let t = &mut SCHEDULER.threads[tid];
            if !t.killable || matches!(t.state, ThreadState::Empty | ThreadState::Dead) {
                return false;
            }
            t.kill_pending = true;
            // Wake it so the kill is noticed now, not after the current sleep.
            if let ThreadState::Sleeping(_) = t.state {
                t.state = ThreadState::Ready;
            }
            true
        }
    }

    pub fn set_name(tid: usize, name: &[u8], killable: bool) {
        unsafe {
            let t = &mut SCHEDULER.threads[tid];
            let len = name.len().min(THREAD_NAME_LEN);
            t.name[..len].copy_from_slice(&name[..len]);
            t.name_len = len;
            t.killable = killable;
        }
    }

    pub fn list() -> alloc::string::String {
        let mut out = alloc::string::String::from("TID  STATE     MEM    NAME\n");
        unsafe {
            for i in 0..MAX_THREADS {
                let t = &SCHEDULER.threads[i];
                let state = match t.state {
                    ThreadState::Empty | ThreadState::Dead => continue,
                    ThreadState::Ready => "ready",
                    ThreadState::Running => "running",
                    ThreadState::Waiting => "waiting",
                    ThreadState::Sleeping(_) => "sleeping",
                };
                let name = if i == 0 {
                    "kernel"
                } else {
                    core::str::from_utf8(&t.name[..t.name_len]).unwrap_or("?")
                };
                let mem_mb = (crate::sys::appmem::committed_by(i) + (1 << 19)) >> 20;
                let mem = if mem_mb > 0 { alloc::format!("{}M", mem_mb) } else { alloc::string::String::from("-") };
                out.push_str(&alloc::format!("{:<4} {:<9} {:<6} {}\n", i, state, mem, name));
            }
        }
        let free_mb = crate::ALLOCATOR.lock().free() >> 20;
        out.push_str(&alloc::format!("free RAM: {}M\n", free_mb));
        let cpus = crate::arch::smp::ONLINE.load(core::sync::atomic::Ordering::Relaxed);
        out.push_str(&alloc::format!("cpus: {}\n", cpus));
        out
    }
}

// A dead thread's kernel stack is in use until its hart has switched away.
unsafe fn reap_dead(sched: &mut Scheduler) {
    for i in 0..MAX_THREADS {
        if sched.threads[i].state != ThreadState::Dead || on_cpu(i) {
            continue;
        }
        let t = &mut sched.threads[i];
        if t.ustack != 0 {
            // Drop the U bit before the pages go back to the kernel heap.
            crate::paging::map_range(
                crate::ROOT_PAGE_TABLE,
                t.ustack,
                t.ustack,
                USTACK_SIZE,
                crate::paging::PTE_R | crate::paging::PTE_W | crate::paging::PTE_X,
            );
            crate::csr::sfence_vma();
            alloc::alloc::dealloc(t.ustack as *mut u8, Layout::from_size_align(USTACK_SIZE, 4096).unwrap());
        }
        if t.kstack != 0 {
            alloc::alloc::dealloc(t.kstack as *mut u8, Layout::from_size_align(KSTACK_SIZE, 4096).unwrap());
        }
        *t = Thread::empty();
    }
}

// Called with the kernel lock held; trap_vector unlocks only after moving onto
// the returned stack, so no hart starts the outgoing thread while its stack is in use.
pub unsafe fn switch(current_sp: usize) -> usize {
    let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
    let h = hart();
    let cur = CURRENT[h];

    if cur == IDLE {
        IDLE_SP[h] = current_sp;
    } else {
        let t = &mut sched.threads[cur];
        match t.state {
            ThreadState::Running => {
                t.sp = current_sp;
                t.state = ThreadState::Ready;
            }
            ThreadState::Waiting | ThreadState::Sleeping(_) => t.sp = current_sp,
            _ => {}
        }
        // Save FPU state if dirty (FS == 3) so the thread can resume on any hart.
        if FPU_OWNER[h] == Some(cur) {
            let frame = current_sp as *mut TrapFrame;
            if t.state != ThreadState::Dead && ((*frame).sstatus >> 13) & 3 == 3 {
                core::arch::asm!("csrs sstatus, {}", in(reg) 1 << 13);
                crate::fpu::save_fpu(&mut t.fpu_state);
            }
            FPU_OWNER[h] = None;
        }
    }

    reap_dead(sched);

    let current_time = crate::csr::read_time();
    for i in 0..MAX_THREADS {
        if let ThreadState::Sleeping(wakeup_time) = sched.threads[i].state {
            if current_time >= wakeup_time {
                sched.threads[i].state = ThreadState::Ready;
            }
        }
    }

    // Thread 0 never runs; a hart with nothing to run goes back to its idle loop.
    let mut next = IDLE;
    for step in 1..=MAX_THREADS {
        let i = (ROTOR + step) % MAX_THREADS;
        if i != 0 && sched.threads[i].state == ThreadState::Ready {
            next = i;
            break;
        }
    }
    CURRENT[h] = next;
    if next == IDLE {
        return IDLE_SP[h];
    }
    ROTOR = next;

    let next_sp = sched.threads[next].sp;
    let frame = next_sp as *mut TrapFrame;
    if (*frame).sstatus & (1 << 8) == 0 {
        // Returning to U-mode: leave the hart id where the trap entry reloads tp from.
        *((next_sp + core::mem::size_of::<TrapFrame>()) as *mut usize) = h;
    }
    // FPU state is loaded on first use (see handle_fpu_fault).
    (*frame).sstatus &= !(3 << 13);
    // The thread may have written code (the AOT engine) on another hart.
    crate::csr::fence_i();

    sched.threads[next].state = ThreadState::Running;
    next_sp
}

pub fn handle_fpu_fault() {
    unsafe {
        let h = hart();
        let current = current_tid();
        if FPU_OWNER[h] == Some(current) {
            return;
        }

        // FS=1 so the kernel can do the swap.
        core::arch::asm!("csrs sstatus, {}", in(reg) 1 << 13);

        if let Some(owner) = FPU_OWNER[h] {
            crate::fpu::save_fpu(&mut SCHEDULER.threads[owner].fpu_state);
        }

        crate::fpu::load_fpu(&SCHEDULER.threads[current].fpu_state);
        FPU_OWNER[h] = Some(current);
    }
}
