use crate::memory::address::PhysAddr;
use crate::memory::{paging, pmm, vmm};
use crate::sync::Mutex;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::arch::{asm, naked_asm};

pub(crate) const MAX_THREADS: usize = 128;
pub(crate) const MAX_PROCESSES: usize = 64;
const STACK_SIZE: u64 = 1024 * 1024;

#[derive(Debug)]
pub struct Process {
    pub pid: u64,
    pub pml4_phys: u64,
    pub fd_table: Mutex<[i16; 16]>,
    pub cwd: Mutex<[u8; 128]>,
    pub terminal_width: Mutex<u16>,
    pub terminal_height: Mutex<u16>,
    pub heap_start: u64,
    pub heap_end: Mutex<u64>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u64)]
pub enum ThreadState {
    Null,
    Ready,
    Zombie,
    Sleeping,
    Blocked,
    Reserved,
}

#[repr(C, align(16))]
pub struct Thread {
    pub fpu_state: [u8; 512],
    pub kernel_stack: u64,
    pub user_stack: u64,
    pub cpu_state_ptr: u64,
    pub state: ThreadState,
    pub wake_ticks: u64,
    pub exit_code: u64,
    pub name: [u8; 32],
    pub process: Option<Arc<Process>>,
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct CPUState {
    pub(crate) r15: u64,
    pub(crate) r14: u64,
    pub(crate) r13: u64,
    pub(crate) r12: u64,
    pub(crate) r11: u64,
    pub(crate) r10: u64,
    pub(crate) r9: u64,
    pub(crate) r8: u64,
    pub(crate) rdi: u64,
    pub(crate) rsi: u64,
    pub(crate) rdx: u64,
    pub(crate) rcx: u64,
    pub(crate) rbx: u64,
    pub(crate) rax: u64,
    pub(crate) rbp: u64,

    pub(crate) rip: u64,
    pub(crate) cs: u64,
    pub(crate) rflags: u64,
    pub(crate) rsp: u64,
    pub(crate) ss: u64,
}

impl Process {
    pub fn new(pid: u64, pml4_phys: u64) -> Arc<Self> {
        let mut cwd = [0; 128];
        let root = b"@0xE0/";
        cwd[..root.len()].copy_from_slice(root);

        Arc::new(Self {
            pid,
            pml4_phys,
            fd_table: Mutex::new([-1; 16]),
            cwd: Mutex::new(cwd),
            terminal_width: Mutex::new(80),
            terminal_height: Mutex::new(25),
            heap_start: 0x40000000,
            heap_end: Mutex::new(0x40000000),
        })
    }
}

// Compatibility aliases
pub type Task = Thread;
pub type TaskState = ThreadState;
pub const MAX_TASKS: usize = MAX_THREADS;
pub const NULL_TASK: Option<Thread> = None;

pub struct TaskManager {
    pub current_task: isize,
    pub thread_count: usize,
    pub tasks: [Option<Thread>; MAX_THREADS],
}

pub static TASK_MANAGER: Mutex<TaskManager> = Mutex::new(TaskManager {
    current_task: -1,
    thread_count: 0,
    tasks: [const { None }; MAX_THREADS],
});

#[unsafe(no_mangle)]
pub static mut KERNEL_STACK_PTR: u64 = 0;

#[unsafe(no_mangle)]
pub static mut SCRATCH: u64 = 0;

impl Thread {
    pub fn new(name: &[u8]) -> Self {
        let mut t_name = [0; 32];
        let len = core::cmp::min(name.len(), 32);
        t_name[..len].copy_from_slice(&name[..len]);

        let mut fpu_state = [0u8; 512];
        // Initialize MXCSR at offset 24 to 0x1F80 (all exceptions masked)
        fpu_state[24] = 0x80;
        fpu_state[25] = 0x1F;

        Self {
            fpu_state,
            kernel_stack: 0,
            user_stack: 0,
            cpu_state_ptr: 0,
            state: ThreadState::Null,
            wake_ticks: 0,
            exit_code: 0,
            name: t_name,
            process: None,
        }
    }
}

pub fn init() {
    let mut tm = TASK_MANAGER.lock();
    tm.init();
}

impl TaskManager {
    pub fn init(&mut self) {
        let mut idle_thread = Thread::new(b"idle");
        idle_thread.state = ThreadState::Ready;

        unsafe {
            let kernel_pml4 = (*(&raw const crate::boot::BOOT_INFO)).pml4;
            let kernel_proc = Process::new(0, kernel_pml4);
            idle_thread.process = Some(kernel_proc);

            let stack_pages = (STACK_SIZE / 4096) as usize;
            let stack_phys = pmm::allocate_frames(stack_pages, 0).expect("Idle stack allocation failed");
            idle_thread.kernel_stack = stack_phys + STACK_SIZE + paging::HHDM_OFFSET;

            let state_size = core::mem::size_of::<CPUState>();
            let state_ptr = (idle_thread.kernel_stack - state_size as u64) as *mut CPUState;
            idle_thread.cpu_state_ptr = state_ptr as u64;

            (*state_ptr).rip = idle as u64;
            (*state_ptr).cs = 0x08;
            (*state_ptr).rflags = 0x202;
            (*state_ptr).rsp = idle_thread.kernel_stack;
            (*state_ptr).ss = 0x10;

            self.tasks[0] = Some(idle_thread);
            self.thread_count = 1;
            self.current_task = 0;
        }
    }

    pub fn current_task_idx(&self) -> Option<usize> {
        if self.current_task >= 0 {
            Some(self.current_task as usize)
        } else {
            None
        }
    }

    pub fn schedule(&mut self, cpu_state: *mut CPUState) -> (*mut CPUState, u64, u64) {
        for i in 0..MAX_THREADS {
            if let Some(thread) = &mut self.tasks[i] {
                if thread.state == ThreadState::Sleeping && unsafe { SYSTEM_TICKS } >= thread.wake_ticks {
                    thread.state = ThreadState::Ready;
                }
            }
        }

        if self.current_task >= 0 {
            if let Some(thread) = &mut self.tasks[self.current_task as usize] {
                thread.cpu_state_ptr = cpu_state as u64;
            }
        }

        self.current_task = self.get_next_thread();
        if self.current_task < 0 {
            return (cpu_state, 0, 0);
        }

        let thread = self.tasks[self.current_task as usize].as_ref().unwrap();
        let pml4 = if let Some(proc) = &thread.process {
            proc.pml4_phys
        } else {
            0
        };

        (
            thread.cpu_state_ptr as *mut CPUState,
            thread.kernel_stack,
            pml4,
        )
    }

    fn get_next_thread(&self) -> isize {
        let mut i = (self.current_task + 1) as usize;
        for _ in 0..MAX_THREADS {
            if i >= MAX_THREADS { i = 0; }
            if let Some(thread) = &self.tasks[i] {
                if thread.state == ThreadState::Ready {
                    return i as isize;
                }
            }
            i += 1;
        }
        -1
    }

    pub fn reserve_pid(&mut self) -> Result<usize, pmm::FrameError> {
        for i in 0..MAX_THREADS {
            if self.tasks[i].is_none() {
                let mut t = Thread::new(b"reserved");
                t.state = ThreadState::Reserved;
                self.tasks[i] = Some(t);
                self.thread_count += 1;
                return Ok(i);
            }
        }
        Err(pmm::FrameError::NoMemory)
    }

    pub fn kill_process(&mut self, pid: u64) {
        for i in 0..MAX_THREADS {
            if let Some(thread) = &mut self.tasks[i] {
                if let Some(proc) = &thread.process {
                    if proc.pid == pid {
                        thread.state = ThreadState::Zombie;
                        unsafe {
                            (*(&raw mut crate::window_manager::composer::COMPOSER)).remove_windows_by_pid(pid);
                        }
                    }
                }
            }
        }
    }

    pub fn init_user_task(&mut self, slot: usize, entry_point: u64, _pml4: u64, args: Option<&[&str]>, fd_table: Option<[i16; 16]>, name: &[u8], terminal_size: (u16, u16)) -> Result<(), pmm::FrameError> {
        let pid = slot as u64;
        let mut thread = Thread::new(name);

        let user_pml4 = unsafe { vmm::create_user_pml4().ok_or(pmm::FrameError::NoMemory)? };
        let proc = Process::new(pid, user_pml4);

        if let Some(fds) = fd_table {
            *proc.fd_table.lock() = fds;
        }
        *proc.terminal_width.lock() = terminal_size.0;
        *proc.terminal_height.lock() = terminal_size.1;

        thread.process = Some(proc);

        let k_frame = pmm::allocate_frames(16, pid).ok_or(pmm::FrameError::NoMemory)?;
        thread.kernel_stack = k_frame + 4096 * 16 + paging::HHDM_OFFSET;

        let stack_pages = (STACK_SIZE / 4096) as usize;
        let u_frame_phys = pmm::allocate_frames(stack_pages, pid).ok_or(pmm::FrameError::NoMemory)?;
        let u_stack_virt = 0x0000_7FFF_FFFF_0000 - STACK_SIZE;

        for i in 0..stack_pages {
            let offset = i as u64 * 4096;
            vmm::map_page(u_stack_virt + offset, PhysAddr::new(u_frame_phys + offset),
                          paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER,
                          Some(user_pml4));
        }
        thread.user_stack = u_stack_virt + STACK_SIZE;

        let state_size = core::mem::size_of::<CPUState>();
        let state_ptr = (thread.kernel_stack - state_size as u64) as *mut CPUState;
        thread.cpu_state_ptr = state_ptr as u64;

        unsafe {
            let stack_phys_base = u_frame_phys + paging::HHDM_OFFSET;
            let mut current_virt_sp = thread.user_stack;

            let mut arg_ptrs = Vec::new();
            let mut push_str = |s: &[u8]| {
                let len = s.len() + 1;
                current_virt_sp -= len as u64;
                let offset = current_virt_sp - u_stack_virt;
                let dest = (stack_phys_base + offset) as *mut u8;
                core::ptr::copy_nonoverlapping(s.as_ptr(), dest, s.len());
                *dest.add(s.len()) = 0;
                current_virt_sp
            };

            arg_ptrs.push(push_str(name));
            if let Some(a_list) = args {
                for &a in a_list {
                    arg_ptrs.push(push_str(a.as_bytes()));
                }
            }

            current_virt_sp &= !15;
            let mut push_u64 = |val: u64| {
                current_virt_sp -= 8;
                let offset = current_virt_sp - u_stack_virt;
                let dest = (stack_phys_base + offset) as *mut u64;
                *dest = val;
            };

            push_u64(0);
            push_u64(0);
            for &ptr in arg_ptrs.iter().rev() { push_u64(ptr); }
            push_u64(arg_ptrs.len() as u64);

            (*state_ptr).rax = 0;
            (*state_ptr).rip = entry_point;
            (*state_ptr).cs = 0x33;
            (*state_ptr).rflags = 0x202;
            (*state_ptr).rsp = current_virt_sp;
            (*state_ptr).ss = 0x23;
        }

        thread.state = ThreadState::Ready;
        self.tasks[slot] = Some(thread);
        Ok(())
    }

    pub fn spawn_thread(&mut self, parent_tid: usize, entry_point: u64, user_stack: u64, arg: u64) -> Result<usize, pmm::FrameError> {
        let tid = self.reserve_pid()?;

        let parent_process = if let Some(t) = &self.tasks[parent_tid] {
            if let Some(p) = &t.process {
                p.clone()
            } else {
                return Err(pmm::FrameError::IndexOutOfBounds);
            }
        } else {
            return Err(pmm::FrameError::IndexOutOfBounds);
        };

        let mut thread = Thread::new(b"thread");
        thread.process = Some(parent_process.clone());

        let k_frame = pmm::allocate_frames(16, tid as u64).ok_or(pmm::FrameError::NoMemory)?;
        thread.kernel_stack = k_frame + 4096 * 16 + paging::HHDM_OFFSET;

        let state_size = core::mem::size_of::<CPUState>();
        let state_ptr = (thread.kernel_stack - state_size as u64) as *mut CPUState;
        thread.cpu_state_ptr = state_ptr as u64;

        unsafe {
            core::ptr::write_bytes(state_ptr, 0, 1);
            (*state_ptr).rip = entry_point;
            (*state_ptr).cs = 0x33;
            (*state_ptr).rflags = 0x202;
            // System V ABI: rsp must be 16-byte aligned before a call.
            // On entry to a function, it should be 16n - 8.
            (*state_ptr).rsp = (user_stack & !15) - 8;
            (*state_ptr).ss = 0x23;
            (*state_ptr).rdi = arg;
        }

        thread.state = ThreadState::Ready;
        self.tasks[tid] = Some(thread);

        Ok(tid)
    }

    pub fn get_tasks(&self) -> &[Option<Thread>; MAX_THREADS] {
        &self.tasks
    }

    pub fn current_thread(&self) -> &Thread {
        self.tasks[self.current_task as usize].as_ref().expect("No current thread")
    }

    pub fn current_thread_mut(&mut self) -> &mut Thread {
        self.tasks[self.current_task as usize].as_mut().expect("No current thread")
    }
}

fn idle() {
    loop {
        unsafe { asm!("hlt") };
    }
}

#[unsafe(naked)]
pub extern "C" fn timer_handler() {
    unsafe {
        naked_asm!(
            "push rbp", "push rax", "push rbx", "push rcx", "push rdx", "push rsi", "push rdi",
            "push r8", "push r9", "push r10", "push r11", "push r12", "push r13", "push r14", "push r15",
            "mov rdi, rsp", "call switch_timer", "mov rsp, rax",
            "pop r15", "pop r14", "pop r13", "pop r12", "pop r11", "pop r10", "pop r9", "pop r8",
            "pop rdi", "pop rsi", "pop rdx", "pop rcx", "pop rbx", "pop rax", "pop rbp",
            "iretq",
        );
    }
}

#[unsafe(naked)]
pub extern "C" fn yield_handler() {
    unsafe {
        naked_asm!(
            "push rbp", "push rax", "push rbx", "push rcx", "push rdx", "push rsi", "push rdi",
            "push r8", "push r9", "push r10", "push r11", "push r12", "push r13", "push r14", "push r15",
            "mov rdi, rsp", "call switch_yield", "mov rsp, rax",
            "pop r15", "pop r14", "pop r13", "pop r12", "pop r11", "pop r10", "pop r9", "pop r8",
            "pop rdi", "pop rsi", "pop rdx", "pop rcx", "pop rbx", "pop rax", "pop rbp",
            "iretq",
        );
    }
}

#[unsafe(no_mangle)]
pub static mut SYSTEM_TICKS: u64 = 0;

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
        if is_timer {
            SYSTEM_TICKS = SYSTEM_TICKS.wrapping_add(10);
        }
        let mut tm = TASK_MANAGER.lock();

        let current_task = tm.current_task;
        if current_task >= 0 {
            if let Some(thread) = &mut tm.tasks[current_task as usize] {
                let fpu_ptr = thread.fpu_state.as_mut_ptr();
                asm!("fxsave [{}]", in(reg) fpu_ptr);
            }
        }

        let (new_state, k_stack, pml4_phys) = tm.schedule(rsp as *mut CPUState);

        let current_task = tm.current_task;
        if current_task >= 0 {
            if let Some(thread) = &tm.tasks[current_task as usize] {
                let fpu_ptr = thread.fpu_state.as_ptr();
                asm!("fxrstor [{}]", in(reg) fpu_ptr);
            }
        }

        if k_stack != 0 {
            crate::tss::set_tss(k_stack);
            KERNEL_STACK_PTR = k_stack;
        }

        if pml4_phys != 0 {
            let current_cr3: u64;
            asm!("mov {}, cr3", out(reg) current_cr3);
            if current_cr3 != pml4_phys {
                asm!("mov cr3, {}", in(reg) pml4_phys);
            }
        }

        if is_timer {
            (*(&raw const crate::interrupts::pic::PICS)).end_interrupt(crate::interrupts::exceptions::TIMER_INT);
        }

        new_state as u64
    }
}