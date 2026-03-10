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
    pub slot_id: u16,
    pub parent_pid: Option<u64>,
    pub children: Mutex<Vec<u64>>,
    pub fd_table: Mutex<[i16; 16]>,
    pub socket_table: Mutex<[Option<usize>; 16]>,
    pub fd_nonblock: Mutex<[bool; 16]>,
    pub cwd: Mutex<[u8; 128]>,
    pub terminal_width: Mutex<u16>,
    pub terminal_height: Mutex<u16>,
    pub linear_memory_base: u64,
    pub code_base: u64,
    pub stack_base: u64,
    pub heap_start: u64,
    pub heap_limit: u64,
    pub heap_end: Mutex<u64>,
    /// (header_ptr, buf_ptr, capacity) — all zero when no queue is registered.
    /// Set by SYS_REGISTER_EVENT_QUEUE (138), cleared by SYS_DEREGISTER_EVENT_QUEUE (139)
    /// and on process kill. Pointers are real virtual addresses in the SAS.
    pub event_queue: Mutex<(u64, u64, u32)>,
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
    WaitingForEvent,
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
    pub fn new(pid: u64, parent_pid: Option<u64>) -> Arc<Self> {
        let mut cwd = [0; 128];
        let root = b"@0xE0/";
        cwd[..root.len()].copy_from_slice(root);

        let slot_id = crate::memory::address_space::allocate_slot().expect("SAS: Out of process slots!");
        let linear_memory_base = crate::memory::address_space::allocate_linear_memory(pid, slot_id);
        let code_base = crate::memory::address_space::allocate_code(pid, slot_id);
        let stack_top = crate::memory::address_space::allocate_stack(pid, slot_id);
        
        let heap_start = linear_memory_base;
        let heap_limit = heap_start + crate::memory::address_space::LINEAR_MEMORY_SLOT_SIZE - 4096;

        Arc::new(Self {
            pid,
            slot_id,
            parent_pid,
            children: Mutex::new(Vec::new()),
            fd_table: Mutex::new([-1; 16]),
            socket_table: Mutex::new([None; 16]),
            fd_nonblock: Mutex::new([false; 16]),
            cwd: Mutex::new(cwd),
            terminal_width: Mutex::new(80),
            terminal_height: Mutex::new(25),
            linear_memory_base,
            code_base,
            stack_base: stack_top,
            heap_start,
            heap_limit,
            heap_end: Mutex::new(heap_start),
            event_queue: Mutex::new((0, 0, 0)),
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
        // Initialize x87 FCW at offset 0 to 0x037F (all x87 exceptions masked, double precision)
        fpu_state[0] = 0x7F;
        fpu_state[1] = 0x03;
        // Initialize MXCSR at offset 24 to 0x1F80 (all SSE exceptions masked)
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
            let kernel_proc = Process::new(0, None);
            idle_thread.process = Some(kernel_proc);

            let stack_pages = (STACK_SIZE / 4096) as usize;
            let stack_phys = pmm::allocate_frames(stack_pages, 0).expect("Idle stack allocation failed");
            idle_thread.kernel_stack = stack_phys + STACK_SIZE + paging::HHDM_OFFSET;

            let state_size = core::mem::size_of::<CPUState>();
            let state_ptr = (idle_thread.kernel_stack - state_size as u64) as *mut CPUState;
            idle_thread.cpu_state_ptr = state_ptr as u64;

            (*state_ptr).rip = idle as u64;
            (*state_ptr).cs = 0x28; // 64-bit kernel code segment (GDT index 5)
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

    pub fn schedule(&mut self, cpu_state: *mut CPUState) -> (*mut CPUState, u64) {
        // Stack pointer sanity check
        let sp = cpu_state as u64;
        if sp < 0x1000 {
            crate::debugln!("CRITICAL: Stack pointer is dangerously low: {:#x}", sp);
        }

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
            return (cpu_state, 0);
        }

        let thread = self.tasks[self.current_task as usize].as_ref().unwrap();
        let next_state = thread.cpu_state_ptr as *const CPUState;
        unsafe {
            let p = next_state;
            let rip = core::ptr::addr_of!((*p).rip).read_unaligned();
            let cs = core::ptr::addr_of!((*p).cs).read_unaligned();
            let rsp = core::ptr::addr_of!((*p).rsp).read_unaligned();
            let ss = core::ptr::addr_of!((*p).ss).read_unaligned();
            let rflags = core::ptr::addr_of!((*p).rflags).read_unaligned();
            if rip == 0 || (rip < 0xFFFF_8000_0000_0000 && cs == 0x08) {
                crate::debugln!("CRITICAL: Scheduling task {} RIP={:#x} CS={:#x} RSP={:#x} SS={:#x} RFLAGS={:#x} state_ptr={:#x} k_stack={:#x}",
                    self.current_task, rip, cs, rsp, ss, rflags,
                    thread.cpu_state_ptr, thread.kernel_stack);
            }
        }

        (
            thread.cpu_state_ptr as *mut CPUState,
            thread.kernel_stack,
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
                        // Clear the event queue registration before the WASM heap is freed
                        // so the kernel never writes to a dangling pointer.
                        *proc.event_queue.lock() = (0, 0, 0);
                        unsafe {
                            (*(&raw mut crate::window_manager::composer::COMPOSER)).remove_windows_by_pid(pid);
                        }
                    }
                }
            }
        }
    }

    pub fn init_user_task(&mut self, slot: usize, entry_point: u64, _pml4: u64, args: Option<&[&str]>, fd_table: Option<[i16; 16]>, name: &[u8], terminal_size: (u16, u16), parent_pid: Option<u64>) -> Result<(), pmm::FrameError> {
        let pid = slot as u64;
        let mut thread = Thread::new(name);

        let proc = Process::new(pid, parent_pid);
        let slot_id = proc.slot_id;

        if let Some(fds) = fd_table {
            *proc.fd_table.lock() = fds;
        }
        *proc.terminal_width.lock() = terminal_size.0;
        *proc.terminal_height.lock() = terminal_size.1;

        thread.process = Some(proc.clone());

        let k_frame = pmm::allocate_frames(16, pid).ok_or(pmm::FrameError::NoMemory)?;
        thread.kernel_stack = k_frame + 4096 * 16 + paging::HHDM_OFFSET;

        let stack_pages = (STACK_SIZE / 4096) as usize;
        let u_frame_phys = pmm::allocate_frames(stack_pages, pid).ok_or(pmm::FrameError::NoMemory)?;

        let u_stack_top = proc.stack_base;
        let u_stack_base = u_stack_top - STACK_SIZE;

        // Map user stack, but leave the bottom-most page unmapped as a guard
        for i in 1..stack_pages {
            let offset = i as u64 * 4096;
            vmm::map_page(u_stack_base + offset, PhysAddr::new(u_frame_phys + offset),
                          paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER,
                          None);
        }
        thread.user_stack = u_stack_top;

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
                let offset = current_virt_sp - u_stack_base;
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
                let offset = current_virt_sp - u_stack_base;
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

        thread.state = if entry_point == 0 { ThreadState::Reserved } else { ThreadState::Ready };
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

        let k_frame = pmm::allocate_frames(64, tid as u64).ok_or(pmm::FrameError::NoMemory)?;
        thread.kernel_stack = k_frame + 4096 * 64 + paging::HHDM_OFFSET;

        let state_size = core::mem::size_of::<CPUState>();
        let state_ptr = (thread.kernel_stack - state_size as u64) as *mut CPUState;
        thread.cpu_state_ptr = state_ptr as u64;

        crate::debugln!("[spawn_thread] TID {} state_ptr={:#x} stack_top={:#x}", tid, thread.cpu_state_ptr, thread.kernel_stack);

        unsafe {
            core::ptr::write_bytes(state_ptr, 0, 1);
            (*state_ptr).rip = entry_point;
            
            if user_stack == 0 {
                // Native Kernel Thread (Ring 0)
                (*state_ptr).cs = 0x28; // 64-bit kernel code segment (GDT index 5)
                (*state_ptr).ss = 0x10;
                // System V ABI: RSP must be 16n + 8 upon function entry
                (*state_ptr).rsp = thread.kernel_stack - state_size as u64 - 8; // Use kernel stack
                (*state_ptr).rflags = 0x202;
            } else {
                // User Thread (Ring 3)
                (*state_ptr).cs = 0x33;
                (*state_ptr).ss = 0x23;
                (*state_ptr).rsp = (user_stack & !15) - 8;
                (*state_ptr).rflags = 0x202;
            }
            
            (*state_ptr).rdi = arg;
            let p = state_ptr as *const CPUState;
            crate::debugln!("[spawn_thread] TID {} CPUState @ {:#x}: RIP={:#x} CS={:#x} RSP={:#x} SS={:#x} RFLAGS={:#x}",
                tid, state_ptr as u64,
                core::ptr::addr_of!((*p).rip).read_unaligned(),
                core::ptr::addr_of!((*p).cs).read_unaligned(),
                core::ptr::addr_of!((*p).rsp).read_unaligned(),
                core::ptr::addr_of!((*p).ss).read_unaligned(),
                core::ptr::addr_of!((*p).rflags).read_unaligned());
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

        // Periodically poll network stack on every switch (tick or yield)
        crate::drivers::network::virtio::poll_rx();

        let mut tm = TASK_MANAGER.lock();

        let current_task_idx = tm.current_task;

        if is_timer {


            // Check timers and wake up threads

            crate::interrupts::event_manager::EVENT_MANAGER.lock().check_timers(&mut tm, SYSTEM_TICKS);
        }


        if current_task_idx >= 0 {
            if let Some(thread) = &mut tm.tasks[current_task_idx as usize] {
                thread.cpu_state_ptr = rsp;
                let fpu_ptr = thread.fpu_state.as_mut_ptr();

                asm!("fxsave [{}]", in(reg) fpu_ptr);
            }
        }


        let (new_state, k_stack) = tm.schedule(rsp as *mut CPUState);
        let new_task_idx = tm.current_task;

        if new_task_idx >= 0 {
            if let Some(thread) = &tm.tasks[new_task_idx as usize] {
                let fpu_ptr = thread.fpu_state.as_ptr();

                asm!("fxrstor [{}]", in(reg) fpu_ptr);
            }
        }

        let new_cpu_state_ptr = new_state as u64;

        // DEBUG: Always dump state when switching to a new task
        if new_task_idx != current_task_idx {
            let p = new_cpu_state_ptr as *const CPUState;
            let rip = core::ptr::addr_of!((*p).rip).read_unaligned();
            let cs = core::ptr::addr_of!((*p).cs).read_unaligned();
            let rsp_val = core::ptr::addr_of!((*p).rsp).read_unaligned();
            let ss = core::ptr::addr_of!((*p).ss).read_unaligned();
            let rflags = core::ptr::addr_of!((*p).rflags).read_unaligned();
            /*crate::debugln!("[SWITCH] {} -> {} state_ptr={:#x} RIP={:#x} CS={:#x} RSP={:#x} SS={:#x} RFLAGS={:#x}",
                current_task_idx, new_task_idx, new_cpu_state_ptr, rip, cs, rsp_val, ss, rflags);

            // Also dump raw bytes at the CPUState to check for corruption
            let bytes = core::slice::from_raw_parts(new_cpu_state_ptr as *const u8, 160);
            crate::debugln!("[SWITCH] First 40 bytes: {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} | {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} | {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} | {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} | {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}",
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
                bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23],
                bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31],
                bytes[32], bytes[33], bytes[34], bytes[35], bytes[36], bytes[37], bytes[38], bytes[39]);
            // Last 40 bytes (the iret frame: RIP, CS, RFLAGS, RSP, SS)
            crate::debugln!("[SWITCH] iret frame (offset 120-159): {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} | {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} | {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} | {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} | {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}",
                bytes[120], bytes[121], bytes[122], bytes[123], bytes[124], bytes[125], bytes[126], bytes[127],
                bytes[128], bytes[129], bytes[130], bytes[131], bytes[132], bytes[133], bytes[134], bytes[135],
                bytes[136], bytes[137], bytes[138], bytes[139], bytes[140], bytes[141], bytes[142], bytes[143],
                bytes[144], bytes[145], bytes[146], bytes[147], bytes[148], bytes[149], bytes[150], bytes[151],
                bytes[152], bytes[153], bytes[154], bytes[155], bytes[156], bytes[157], bytes[158], bytes[159]);

             */
        }

        if k_stack != 0 {
            crate::tss::set_tss(k_stack);

            KERNEL_STACK_PTR = k_stack;
        }


        if is_timer {
            (*(&raw const crate::interrupts::pic::PICS)).end_interrupt(crate::interrupts::exceptions::TIMER_INT);
        }


        new_state as u64
    }
}
