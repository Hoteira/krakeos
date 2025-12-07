use core::arch::{asm, naked_asm};
use crate::debugln;
use crate::memory::{paging, pmm, vmm};

#[allow(dead_code)]
const STACK_SIZE: u64 = 64 * 1024; // 64KB
const MAX_TASKS: usize = 125;

use alloc::vec::Vec;
use alloc::boxed::Box;

#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct Task {
    pub kernel_stack: u64,
    pub stack: u64,
    pub cpu_state_ptr: u64,
    pub state: TaskState,
    pub pml4_phys: u64,
    // We can't derive Copy/Clone if we have Vec/Box.
    // Task must be non-copy now? 
    // But `MAX_TASKS` array requires Copy/Clone initialization?
    // `[NULL_TASK; MAX_TASKS]` requires Copy.
    // We might need to change `tasks` array or `Task` definition.
    // Or use a fixed size array for FDs?
    // Or Wrap Vec in something?
    // The current TaskManager usage relies on array initialization.
}

// ...
// Wait, modifying Task to be non-Copy is a big change.
// `[NULL_TASK; 125]` works because `Task` is Copy.
// `Vec` is not Copy.
// I should probably make `file_descriptors` a fixed array or `Option<Box...>` is not copy either.
// `Box` is not Copy.
// I need to change `tasks` to `[Option<Task>; MAX_TASKS]` or similar?
// Or `Vec<Task>`?
// `TaskManager` uses `tasks: [Task; MAX_TASKS]`.
//
// Alternative: Global FD table? No, per task.
// Alternative: Fixed array of FDs in Task? `[Option<FileHandle>; 16]`.
// `FileHandle` contains `Box`. `Box` is unique. Not Copy.
//
// So `Task` cannot be Copy.
// We must change `TaskManager`.
//
// `pub tasks: [Option<Task>; MAX_TASKS]`?
// Then `NULL_TASK` is just `None`.
//
// Let's try to keep it simple.
// Can we use a raw pointer for `FileHandle`?
// Or make `Task` not Copy and initialize array manually?
// `const MAX_TASKS: usize = 125;`
// `tasks: [Task; MAX_TASKS]` -> `tasks: Vec<Task>`?
// Or `tasks: [Option<Task>; MAX_TASKS]` initialized with `[const { None }; MAX_TASKS]`.
//
// Let's change `TaskManager` to use `Vec<Task>` or `[Option<Task>; 125]`.
// `[Option<Task>; 125]` is easiest if we impl `Default`.


#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TaskState {
    Null,
    Ready,
    Zombie,
}

// Adapted for x86_64
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct CPUState {
    // Pushed by push_all (manual)
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,
    pub(crate) rdi: u64,
    pub(crate) rsi: u64,
    rdx: u64,
    rcx: u64,
    rbx: u64,
    pub(crate) rax: u64,
    rbp: u64,

    // Pushed by CPU on interrupt
    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: u64,
    ss: u64,
}

static NULL_TASK: Task = Task {
    stack: 0,
    kernel_stack: 0,
    cpu_state_ptr: 0,
    state: TaskState::Null,
    pml4_phys: 0, // Initialize the new field
};

impl Task {
    // Adapted for 64-bit (u64 args, etc)
    pub fn init(&mut self, entry_point: u64, args: Option<&[u64]>) {
        self.state = TaskState::Ready;
        
        // Set to Kernel PML4
        unsafe {
             self.pml4_phys = (*(&raw const crate::boot::BOOT_INFO)).pml4;
        }

        // Allocate stack (Kernel Stack for kernel tasks)
        self.stack = pmm::allocate_frame().expect("Task init: OOM");
        
        let stack_top = self.stack + 4096;
        self.kernel_stack = stack_top; // Used for TSS RSP0 (and matching logic)

        let state_size = core::mem::size_of::<CPUState>();
        let state_ptr = (stack_top - state_size as u64) as *mut CPUState;
        self.cpu_state_ptr = state_ptr as u64;

        let mut arg_count = 0;
        if args.is_some() {
            arg_count = core::cmp::min(args.unwrap().len(), 4);
        }

        unsafe {
            (*state_ptr).rax = 0;
            (*state_ptr).rbx = if arg_count > 0 { args.unwrap()[0] } else { 0 };
            (*state_ptr).rcx = if arg_count > 1 { args.unwrap()[1] } else { 0 };
            (*state_ptr).rdx = if arg_count > 2 { args.unwrap()[2] } else { 0 };
            (*state_ptr).rsi = if arg_count > 3 { args.unwrap()[3] } else { 0 };

            (*state_ptr).rdi = 0;
            (*state_ptr).rbp = 0;
            (*state_ptr).rsp = stack_top; 
            (*state_ptr).rip = entry_point;
            (*state_ptr).cs = 0x28; // Kernel Code 64-bit
            (*state_ptr).rflags = 0x202; // Interrupts enabled
            (*state_ptr).ss = 0x10; // Kernel Data
        }
    }

    #[allow(dead_code)]
    pub fn init_user(&mut self, entry_point: u64, pml4_phys: u64, args: Option<&[u64]>) {
        self.state = TaskState::Ready;

        // Use the provided PML4
        self.pml4_phys = pml4_phys;

        // 1. Allocate Kernel Stack (for TSS RSP0 and context saving)
        let k_frame = pmm::allocate_frame().expect("Task init_user: OOM (kstack)");
        // FIX: Ensure kernel stack frame is mapped in the kernel so we can write to it
        unsafe { vmm::map_page(k_frame, k_frame, paging::PAGE_PRESENT | paging::PAGE_WRITABLE, None); }
        self.kernel_stack = k_frame + 4096;

        // 2. Allocate User Stack (physical frame)
        let u_frame = pmm::allocate_frame().expect("Task init_user: OOM (ustack)");
        self.stack = u_frame; 
        
        // Map it to virtual user space
        let u_stack_virt = vmm::map_user_stack(u_frame, self.pml4_phys);
        let u_stack_top = u_stack_virt + 4096; // This will be used as the User RSP

        // 3. Setup CPU State on the KERNEL Stack
        let state_size = core::mem::size_of::<CPUState>();
        let state_ptr = (self.kernel_stack - state_size as u64) as *mut CPUState;
        self.cpu_state_ptr = state_ptr as u64;

        let mut arg_count = 0;
        if args.is_some() {
            arg_count = core::cmp::min(args.unwrap().len(), 4);
        }

        unsafe {
            (*state_ptr).rax = 0;
            (*state_ptr).rbx = if arg_count > 0 { args.unwrap()[0] } else { 0 };
            (*state_ptr).rcx = if arg_count > 1 { args.unwrap()[1] } else { 0 };
            (*state_ptr).rdx = if arg_count > 2 { args.unwrap()[2] } else { 0 };
            (*state_ptr).rsi = if arg_count > 3 { args.unwrap()[3] } else { 0 };

            (*state_ptr).rdi = 0;
            (*state_ptr).rbp = 0;
            
            // User Context
            (*state_ptr).rip = entry_point; 
            (*state_ptr).cs = 0x33; // User Code (0x30) | RPL 3
            (*state_ptr).rflags = 0x202; // Interrupts enabled
            (*state_ptr).rsp = u_stack_top; // User Stack
            (*state_ptr).ss = 0x23; // User Data (0x20) | RPL 3
        }
    }
}

pub struct TaskManager {
    pub tasks: [Task; MAX_TASKS],
    task_count: usize,
    current_task: isize,
}

// Using a spinlock for thread safety (even though we are single core, interrupts exist)
#[allow(dead_code)]
pub struct LockedTaskManager {
    inner: spin::Mutex<TaskManager>,
}

// Helper since we don't have the user's mutex lib
pub mod spin {
    use core::sync::atomic::{AtomicBool, Ordering};
    use core::cell::UnsafeCell;

    pub struct Mutex<T> {
        lock: AtomicBool,
        data: UnsafeCell<T>,
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
                core::hint::spin_loop();
            }
            MutexGuard {
                lock: &self.lock,
                data: unsafe { &mut *self.data.get() },
            }
        }
    }

    impl<'a, T> core::ops::Deref for MutexGuard<'a, T> {
        type Target = T;
        fn deref(&self) -> &Self::Target {
            self.data
        }
    }
    impl<'a, T> core::ops::DerefMut for MutexGuard<'a, T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            self.data
        }
    }
    impl<'a, T> Drop for MutexGuard<'a, T> {
        fn drop(&mut self) {
            self.lock.store(false, Ordering::Release);
        }
    }
}

pub static TASK_MANAGER: spin::Mutex<TaskManager> =
    spin::Mutex::new(TaskManager {
        tasks: [NULL_TASK; MAX_TASKS],
        task_count: 0,
        current_task: -1,
    });

// Global pointer to the current task's kernel stack (Top of stack).
// Updated by switch() on every context switch.
#[unsafe(no_mangle)]
pub static mut KERNEL_STACK_PTR: u64 = 0;

#[unsafe(no_mangle)]
pub static mut SCRATCH: u64 = 0;

impl TaskManager {
    pub fn init(&mut self) {
        self.add_task(idle as u64, None);
    }

    pub fn add_task(&mut self, entry_point: u64, args: Option<&[u64]>) {
        if self.task_count < MAX_TASKS {
            let free_slot = self.get_free_slot();
            self.tasks[free_slot].init(entry_point, args);
            self.task_count += 1;
        }
    }

    #[allow(dead_code)]
    pub fn add_user_task(&mut self, entry_point: u64, pml4_phys: u64, args: Option<&[u64]>) {
        if self.task_count < MAX_TASKS {
            let free_slot = self.get_free_slot();
            self.tasks[free_slot].init_user(entry_point, pml4_phys, args);
            self.task_count += 1;
        }
    }

    pub fn schedule(&mut self, cpu_state: *mut CPUState) -> (*mut CPUState, u64, u64) {
        if self.current_task >= 0 {
            self.tasks[self.current_task as usize].cpu_state_ptr = cpu_state as u64;

            if self.tasks[self.current_task as usize].state == TaskState::Zombie {
                let task = &mut self.tasks[self.current_task as usize];
                
                // Clean up stacks
                let k_frame = task.kernel_stack - 4096;
                // Check if kernel stack is different from main stack (User Task)
                // For Kernel Task: stack == k_frame
                // For User Task: stack != k_frame (stack is user stack)
                if k_frame != task.stack {
                     pmm::free_frame(k_frame);
                }
                pmm::free_frame(task.stack);

                *task = NULL_TASK;
                self.task_count -= 1;
            }
        }

        self.current_task = self.get_next_task();
        if self.current_task < 0 {
            return (cpu_state, 0, 0);
        }

        (
            self.tasks[self.current_task as usize].cpu_state_ptr as *mut CPUState,
            self.tasks[self.current_task as usize].kernel_stack,
            self.tasks[self.current_task as usize].pml4_phys,
        )
    }

    pub fn get_next_task(&self) -> isize {
        let mut i = self.current_task + 1;
        let limit = MAX_TASKS as isize;
        
        // Simple round-robin
        let start_i = i;
        
        loop {
            if i >= limit {
                i = 0;
            }
            
            if self.tasks[i as usize].state == TaskState::Ready {
                return i;
            }
            
            i += 1;
            if i == start_i {
                // Looped all the way around, no tasks ready
                // Should at least have idle
                break;
            }
        }
        
        // Fallback to idle if nothing else is found (usually index 0)
        if self.tasks[0].state == TaskState::Ready {
            0
        } else {
            -1
        }
    }

    fn get_free_slot(&self) -> usize {
        for i in 0..MAX_TASKS {
            if self.tasks[i].state == TaskState::Null {
                return i;
            }
        }

        panic!("No free slots available!");
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
            // 1. Save all registers (x86_64 calling convention + extras)
            // The CPU has already pushed SS, RSP, RFLAGS, CS, RIP
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

            // 2. Pass the current stack pointer (RSP) as the first argument (RDI) to switch
            "mov rdi, rsp",
            "call switch",

            // 3. The switch function returns the new RSP in RAX
            "mov rsp, rax",

            // 4. Restore registers
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

            // 5. Return using iretq (pops RIP, CS, RFLAGS, RSP, SS)
            "iretq",
        );
    }
}


#[unsafe(no_mangle)]
pub extern "C" fn switch(rsp: u64) -> u64 {
    unsafe {
        let mut tm = TASK_MANAGER.lock();
        let (new_state, k_stack, _pml4_phys) = tm.schedule(rsp as *mut CPUState);
        
        // Update TSS RSP0 if the task has a kernel stack (for user->kernel transition)
        if k_stack != 0 {
             crate::tss::set_tss(k_stack);
             // Update the global kernel stack pointer for syscalls to use
             KERNEL_STACK_PTR = k_stack;
        }
        
        debugln!("[Switch] Returning RSP: {:#x}", new_state as u64);
        
        // debugln!("[Switch] Switching to task. RIP: {:#x}, RSP: {:#x}, CR3: {:#x}", (*new_state).rip, (*new_state).rsp, pml4_phys);

        /* 
           CR3 Switching Logic Removed:
           Since new_user_pml4 currently returns the shared Boot PML4, 
           switching CR3 is unnecessary and risky if we are executing on a stack 
           that relies on the current mapping. We assume all tasks share the 
           same address space for now.
        */

        (*(&raw const crate::interrupts::pic::PICS)).end_interrupt(crate::interrupts::exceptions::TIMER_INT);

        new_state as u64
    }
}