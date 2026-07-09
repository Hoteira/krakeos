use crate::trap::TrapFrame;
use crate::ALLOCATOR;
use crate::fpu::FpuFrame;
use core::alloc::Layout;

#[derive(Copy, Clone, PartialEq)]
pub enum ThreadState {
    Empty,
    Ready,
    Running,
    Waiting,
    Sleeping(u64),
}

#[derive(Copy, Clone)]
pub struct Thread {
    pub sp: usize,
    pub state: ThreadState,
    pub fpu_state: FpuFrame,
}

pub struct Scheduler {
    pub threads: [Thread; 8],
    pub current: usize,
}

pub static mut SCHEDULER: Scheduler = Scheduler {
    threads: [Thread { sp: 0, state: ThreadState::Empty, fpu_state: FpuFrame::new() }; 8],
    current: 0,
};

pub static mut FPU_OWNER: Option<usize> = None;

impl Scheduler {
    pub fn init_main() {
        unsafe {
            SCHEDULER.threads[0].state = ThreadState::Running;
            SCHEDULER.current = 0;
        }
    }

    pub fn spawn(entry: fn()) {
        unsafe {
            let stack_size = 1024 * 1024; // 1MB stack
            let layout = Layout::from_size_align(stack_size, 4096).unwrap();
            let stack_bottom = alloc::alloc::alloc_zeroed(layout) as usize;
            let stack_top = stack_bottom + stack_size;

            let trap_frame_ptr = (stack_top - core::mem::size_of::<TrapFrame>()) as *mut TrapFrame;

            (*trap_frame_ptr).sepc = entry as usize;
            (*trap_frame_ptr).sstatus = (1 << 8) | (1 << 5);
            (*trap_frame_ptr).regs[2] = trap_frame_ptr as usize;

            for i in 0..8 {
                if SCHEDULER.threads[i].state == ThreadState::Empty {
                    SCHEDULER.threads[i].sp = trap_frame_ptr as usize;
                    SCHEDULER.threads[i].state = ThreadState::Ready;
                    return;
                }
            }
        }
    }

    pub fn spawn_user() {
        unsafe {
            let stack_size = 1024 * 1024;
            let layout = Layout::from_size_align(stack_size, 4096).unwrap();
            let kernel_stack = alloc::alloc::alloc_zeroed(layout) as usize;
            let user_layout = Layout::from_size_align(4096, 4096).unwrap();
            let user_stack = alloc::alloc::alloc_zeroed(user_layout) as usize;

            crate::paging::map_page(
                crate::ROOT_PAGE_TABLE, 
                user_stack, 
                user_stack, 
                crate::paging::PTE_R | crate::paging::PTE_W | crate::paging::PTE_U
            );
            crate::csr::sfence_vma();

            let trap_frame_ptr = (kernel_stack + stack_size - core::mem::size_of::<TrapFrame>()) as *mut TrapFrame;

            (*trap_frame_ptr).sepc = core::ptr::addr_of!(crate::user_thread_start) as usize;
            (*trap_frame_ptr).sstatus = 1 << 5; // SPIE=1, SPP=0 (U-mode)
            (*trap_frame_ptr).regs[2] = user_stack + 4096; // User SP

            for i in 0..8 {
                if SCHEDULER.threads[i].state == ThreadState::Empty {
                    SCHEDULER.threads[i].sp = trap_frame_ptr as usize;
                    SCHEDULER.threads[i].state = ThreadState::Ready;
                    return;
                }
            }
        }
    }

    pub fn spawn_user_thread(entry: usize, arg: usize) {
        unsafe {
            let stack_size = 1024 * 1024;
            let layout = Layout::from_size_align(stack_size, 4096).unwrap();
            let kernel_stack = alloc::alloc::alloc_zeroed(layout) as usize;
            let user_layout = Layout::from_size_align(1024 * 1024, 4096).unwrap(); // 1MB user stack
            let user_stack = alloc::alloc::alloc_zeroed(user_layout) as usize;

            crate::paging::map_range(
                crate::ROOT_PAGE_TABLE, 
                user_stack, 
                user_stack, 
                1024 * 1024,
                crate::paging::PTE_R | crate::paging::PTE_W | crate::paging::PTE_U
            );
            crate::csr::sfence_vma();

            let trap_frame_ptr = (kernel_stack + stack_size - core::mem::size_of::<TrapFrame>()) as *mut TrapFrame;

            (*trap_frame_ptr).sepc = entry;
            (*trap_frame_ptr).sstatus = 1 << 5; // SPIE=1, SPP=0 (U-mode)
            (*trap_frame_ptr).regs[2] = user_stack + (1024 * 1024); // User SP
            (*trap_frame_ptr).regs[10] = arg; // a0 = arg

            for i in 0..8 {
                if SCHEDULER.threads[i].state == ThreadState::Empty {
                    SCHEDULER.threads[i].sp = trap_frame_ptr as usize;
                    SCHEDULER.threads[i].state = ThreadState::Ready;
                    return;
                }
            }
        }
    }
}

pub unsafe fn switch(current_sp: usize) -> usize {
    let sched = &mut *core::ptr::addr_of_mut!(SCHEDULER);
    
    let current_time = crate::csr::read_time();
    for i in 0..8 {
        if let ThreadState::Sleeping(wakeup_time) = sched.threads[i].state {
            if current_time >= wakeup_time {
                sched.threads[i].state = ThreadState::Ready;
            }
        }
    }
    
    if sched.threads[sched.current].state == ThreadState::Running {
        sched.threads[sched.current].sp = current_sp;
        sched.threads[sched.current].state = ThreadState::Ready;
    } else if sched.threads[sched.current].state == ThreadState::Waiting || matches!(sched.threads[sched.current].state, ThreadState::Sleeping(_)) {
        sched.threads[sched.current].sp = current_sp;
    }

    let mut next = (sched.current + 1) % 8;
    while sched.threads[next].state != ThreadState::Ready {
        next = (next + 1) % 8;
        if next == sched.current {
            // Idle if no threads are ready
            // We just let the current thread keep running or loop until timer fires
            break;
        }
    }

    let next_sp = sched.threads[next].sp;

    if FPU_OWNER != Some(next) {
        let frame = next_sp as *mut TrapFrame;
        (*frame).sstatus &= !(3 << 13);
    }

    sched.current = next;
    sched.threads[next].state = ThreadState::Running;
    next_sp
}

pub fn handle_fpu_fault() {
    unsafe {
        let current = SCHEDULER.current;
        if FPU_OWNER == Some(current) {
            return;
        }
        
        // Temporarily enable FPU in kernel to perform the swap (FS=1)
        core::arch::asm!("csrs sstatus, {}", in(reg) 1 << 13);
        
        if let Some(owner) = FPU_OWNER {
            crate::fpu::save_fpu(&mut SCHEDULER.threads[owner].fpu_state);
        }
        
        crate::fpu::load_fpu(&SCHEDULER.threads[current].fpu_state);
        FPU_OWNER = Some(current);
    }
}
