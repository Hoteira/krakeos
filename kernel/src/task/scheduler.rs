use crate::task::manager::TASK_MANAGER;
use crate::task::thread::CPUState;
use core::arch::{asm, naked_asm};

#[unsafe(no_mangle)]
pub static mut SYSTEM_TICKS: u64 = 0;

pub fn idle() {
    loop {
        unsafe { asm!("hlt") };
    }
}

#[unsafe(naked)]
pub extern "C" fn timer_handler() {
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
            "call switch_timer",
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

#[unsafe(naked)]
pub extern "C" fn yield_handler() {
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
            "call switch_yield",
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
        SYSTEM_TICKS = SYSTEM_TICKS.wrapping_add(1);

        crate::drivers::network::virtio::poll_rx();
        let mut tm = TASK_MANAGER.lock();
        let current_task_idx: i64;
        asm!("mov {}, gs:[24]", out(reg) current_task_idx);

        if is_timer {
            crate::task::event_manager::EVENT_MANAGER
                .lock()
                .check_timers(&mut tm, SYSTEM_TICKS);
        }

        if current_task_idx >= 0 {
            if let Some(thread) = tm.tasks.get_mut(&(current_task_idx as usize)) {
                thread.cpu_state_ptr = rsp;
                let raw_ptr = thread.fpu_state.as_mut_ptr() as u64;
                let fpu_ptr = (raw_ptr + 15) & !15;
                asm!("fxsave [{}]", in(reg) fpu_ptr);
            }
        }

        let (new_state, k_stack, new_task_idx) = tm.schedule(rsp as *mut CPUState, is_timer);
        asm!("mov gs:[24], {}", in(reg) new_task_idx);

        if new_task_idx >= 0 {
            if let Some(thread) = tm.tasks.get(&(new_task_idx as usize)) {
                let raw_ptr = thread.fpu_state.as_ptr() as u64;
                let fpu_ptr = (raw_ptr + 15) & !15;
                asm!("fxrstor [{}]", in(reg) fpu_ptr);
            }
        }

        if k_stack != 0 {
            crate::arch::x86_64::tss::set_tss(k_stack);
            asm!("mov gs:[8], {}", in(reg) k_stack);
        }

        if is_timer {
            crate::arch::x86_64::exceptions::end_interrupt(crate::arch::x86_64::exceptions::TIMER_INT);
        }

        new_state as u64
    }
}
