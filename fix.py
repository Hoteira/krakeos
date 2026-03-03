import sys
with open('kernel/src/interrupts/syscalls/process.rs', 'rb') as f:
    content = f.read().decode('utf-8', 'ignore')

parts = content.split('pub fn handle_thread_exit')
new_content = parts[0] + '''pub fn handle_thread_exit(context: &mut CPUState) {
    let exit_code = context.rdi;
    debugln!("[Syscall] Thread exited");
    {
        let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 {
            if let Some(task) = tm.tasks[current as usize].as_mut() {
                task.state = crate::interrupts::task::ThreadState::Zombie;
                task.exit_code = 0;
            }
        }
    }

    unsafe {
        core::arch::asm!("sti");
        loop { core::arch::asm!("hlt"); }
    }
}
'''
with open('kernel/src/interrupts/syscalls/process.rs', 'w', encoding='utf-8') as f:
    f.write(new_content)
