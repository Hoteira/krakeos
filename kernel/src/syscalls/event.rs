use crate::task::event_manager::{AsyncEvent, EVENT_MANAGER};
use crate::task::{CPUState, ThreadState, TASK_MANAGER};

pub fn handle_wait_for_event(context: &mut CPUState) {
    let event_type = context.rdi;
    let event_val = context.rsi;

    let event = match event_type {
        0 => AsyncEvent::Generic(event_val),
        1 => AsyncEvent::IO(event_val as i32),
        2 => AsyncEvent::Timer(event_val),
        3 => AsyncEvent::Read(event_val as i32),
        4 => AsyncEvent::Write(event_val as i32),
        _ => {
            context.rax = u64::MAX;
            return;
        }
    };

    let mut tm = TASK_MANAGER.int_lock();
    if let Some(current_idx) = tm.current_task_idx() {
        let mut em = EVENT_MANAGER.int_lock();

        // Check if event is already pending
        if em.check_pending(current_idx, event) {
            context.rax = 0;
            return;
        }

        if let Some(thread) = tm.tasks.get_mut(&current_idx) {
            thread.state = ThreadState::WaitingForEvent;
            em.register(current_idx, event);
        }
    }

    drop(tm);

    // Yield immediately
    unsafe {
        core::arch::asm!("sti");
        core::arch::asm!("int 0x81");
        core::arch::asm!("cli");
    }
}

pub fn handle_register_event(context: &mut CPUState) {
    let event_type = context.rdi;
    let event_val = context.rsi;

    let event = match event_type {
        0 => AsyncEvent::Generic(event_val),
        1 => AsyncEvent::IO(event_val as i32),
        2 => AsyncEvent::Timer(event_val),
        _ => {
            context.rax = u64::MAX;
            return;
        }
    };

    let tm = TASK_MANAGER.int_lock();
    if let Some(current_idx) = tm.current_task_idx() {
        let mut em = EVENT_MANAGER.int_lock();

        // Check if event is already pending
        if em.check_pending(current_idx, event) {
            // Re-add it if we just want to "peek" or handle it later?
            // Actually, for register_event, if it's pending, we don't need to register.
            // But we should probably NOT consume it here.
            // Let's just check existence.
            let mut exists = false;
            for p in &em.pending {
                if p.thread_idx == current_idx && p.event == event {
                    exists = true;
                    break;
                }
            }
            if exists {
                context.rax = 0;
                return;
            }

            em.register(current_idx, event);
            context.rax = 0;
        } else {
            em.register(current_idx, event);
            context.rax = 0;
        }
    }
}

pub fn handle_signal_event(context: &mut CPUState) {
    let event_type = context.rdi;
    let event_val = context.rsi;

    let event = match event_type {
        0 => AsyncEvent::Generic(event_val),
        1 => AsyncEvent::IO(event_val as i32),
        2 => AsyncEvent::Timer(event_val),
        _ => {
            context.rax = u64::MAX;
            return;
        }
    };

    // signal_event will lock TM then EM internally
    crate::task::event_manager::signal_event(event);
    context.rax = 0;
}
