use crate::interrupts::task::CPUState;

pub fn sys_malloc(context: &mut CPUState) {
    let size = context.rdi as usize;
    let pages = (size + 4095) / 4096;
    
    let pid = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        if tm.current_task >= 0 {
            tm.current_task as u64
        } else {
            0
        }
    };

    if let Some(addr) = crate::memory::pmm::allocate_frames(pages, pid) {
        context.rax = addr;
    } else {
        context.rax = 0;
    }
}

pub fn sys_free(context: &mut CPUState) {
    let ptr = context.rdi;
    crate::memory::pmm::free_frame(ptr);
    context.rax = 0;
}

pub fn sys_get_process_mem(context: &mut CPUState) {
    let pid = context.rdi as u64;
    context.rax = crate::memory::pmm::get_memory_usage_by_pid(pid) as u64;
}
