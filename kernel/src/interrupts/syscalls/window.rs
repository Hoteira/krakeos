use crate::interrupts::task::CPUState;
use crate::window_manager::composer::COMPOSER;
use crate::window_manager::display::DISPLAY_SERVER;
use crate::window_manager::input::MOUSE;
use crate::window_manager::window::Window;

pub fn handle_add_window(context: &mut CPUState) {
    unsafe {
        let win_size = core::mem::size_of::<Window>() as u64;
        if !super::validate_user_buf(context, context.rdi, win_size) { return; }

        let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        if let Some(current) = tm.current_task_idx() {
            if let Some(thread) = tm.tasks.get(&(current)) {
                let proc = thread.process.as_ref().expect("Thread has no process");

                let mut w = *(context.rdi as *const Window);
                w.pid = proc.pid;

                drop(tm);
                let id = (*(&raw mut COMPOSER)).add_window(w);
                context.rax = id as u64;
            } else {
                context.rax = u64::MAX;
            }
        } else {
            context.rax = u64::MAX;
        }
    }
}

pub fn handle_update_window(context: &mut CPUState) {
    unsafe {
        let win_size = core::mem::size_of::<Window>() as u64;
        if !super::validate_user_buf(context, context.rdi, win_size) { return; }

        let composer = &mut *(&raw mut COMPOSER);

        let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        if let Some(current) = tm.current_task_idx() {
            if let Some(thread) = tm.tasks.get(&(current)) {
                let proc = thread.process.as_ref().expect("Thread has no process");
                let w = *(context.rdi as *const Window);

                if let Some(existing_win) = composer.find_window_id(w.id) {
                    if existing_win.pid == proc.pid {
                        drop(tm);
                        composer.resize_window(w);
                        context.rax = 1;
                    } else {
                        context.rax = 0;
                    }
                } else {
                    context.rax = 0;
                }
            } else {
                context.rax = 0;
            }
        } else {
            context.rax = 0;
        }
    }
}

pub fn handle_update_window_area(context: &mut CPUState) {
    let wid = context.rdi as usize;
    let x = context.rsi as i32;
    let y = context.rdx as i32;
    let w = context.r10 as u32;
    let h = context.r8 as u32;

    unsafe {
        let composer = &mut *(&raw mut COMPOSER);
        if let Some(win) = composer.find_window_id(wid) {
            let global_x = win.x as i32 + x;
            let global_y = win.y as i32 + y;
            composer.update_window_area_rect(global_x, global_y, w, h);
        }
    }
    context.rax = 1;
}

pub fn handle_get_events(context: &mut CPUState) {
    use core::sync::atomic::Ordering;
    use crate::window_manager::events::{Event, EventQueueHeader, GLOBAL_EVENT_QUEUE};
    use crate::interrupts::task::TASK_MANAGER;

    let wid = context.rdi as u32;
    let max_events = context.rdx as usize;
    let buf_size = (max_events as u64) * (core::mem::size_of::<Event>() as u64);
    if !super::validate_user_buf(context, context.rsi, buf_size) { return; }

    // If the process has a registered queue, drain from it directly.
    {
        let tm = TASK_MANAGER.int_lock();
        if let Some(idx) = tm.current_task_idx() {
            if let Some(thread) = tm.tasks.get(&(idx)) {
                if let Some(proc) = thread.process.as_ref() {
                    let buf_ptr = context.rsi as *mut Event;

                    let (header_ptr, buf_virt, capacity) = *proc.event_queue.lock();
                    if header_ptr != 0 {
                        let header = unsafe { &*(header_ptr as *const EventQueueHeader) };
                        let mut count = 0usize;
                        while count < max_events {
                            let tail = header.tail.load(Ordering::Relaxed);
                            if tail == header.head.load(Ordering::Acquire) {
                                break;
                            }
                            let event = unsafe { (buf_virt as *const Event).add(tail as usize).read() };
                            header.tail.store((tail + 1) % capacity, Ordering::Release);
                            if event.get_window_id() == wid || wid == 0 {
                                unsafe { buf_ptr.add(count).write(event); }
                                count += 1;
                            }
                        }
                        context.rax = count as u64;
                        return;
                    }
                }
            }
        }
    }

    // Fallback: processes without a registered queue use the global queue.
    unsafe {
        let buf_ptr = context.rsi as *mut Event;

        let events = GLOBAL_EVENT_QUEUE.int_lock().get_and_remove_events(wid, max_events);
        let user_slice = core::slice::from_raw_parts_mut(buf_ptr, max_events);
        let mut count = 0;
        for (i, evt) in events.into_iter().enumerate() {
            if i < max_events {
                user_slice[i] = evt;
                count += 1;
            }
        }
        context.rax = count as u64;
    }
}

pub fn handle_register_event_queue(context: &mut CPUState) {
    let capacity   = context.rdx as u32;
    let header_ptr = context.rdi;
    let buf_ptr    = context.rsi;

    // Validate both header and event buffer pointers
    let header_size = core::mem::size_of::<crate::window_manager::events::EventQueueHeader>() as u64;
    let buf_size = (capacity as u64) * (core::mem::size_of::<crate::window_manager::events::Event>() as u64);
    if !super::validate_user_buf(context, header_ptr, header_size) { return; }
    if !super::validate_user_buf(context, buf_ptr, buf_size) { return; }

    let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    if let Some(idx) = tm.current_task_idx() {
        if let Some(thread) = tm.tasks.get(&(idx)) {
            if let Some(proc) = thread.process.as_ref() {
                *proc.event_queue.lock() = (header_ptr, buf_ptr, capacity);
                crate::debugln!(
                    "[EventQueue] PID {} registered queue: header={:#x} buf={:#x} cap={}",
                    proc.pid, header_ptr, buf_ptr, capacity
                );
                context.rax = 0;
                return;
            }
        }
    }
    context.rax = u64::MAX;
}

pub fn handle_deregister_event_queue(context: &mut CPUState) {
    let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    if let Some(idx) = tm.current_task_idx() {
        if let Some(thread) = tm.tasks.get(&(idx)) {
            if let Some(proc) = thread.process.as_ref() {
                *proc.event_queue.lock() = (0, 0, 0);
                crate::debugln!("[EventQueue] PID {} deregistered queue", proc.pid);
            }
        }
    }
    context.rax = 0;
}

pub fn handle_get_width(context: &mut CPUState) {
    unsafe {
        context.rax = (*(&raw mut DISPLAY_SERVER)).width;
    }
}

pub fn handle_get_height(context: &mut CPUState) {
    unsafe {
        context.rax = (*(&raw mut DISPLAY_SERVER)).height;
    }
}

pub fn handle_get_mouse(context: &mut CPUState) {
    unsafe {
        let mouse = &*(&raw const MOUSE);
        context.rax = ((mouse.x as u64) << 32) | (mouse.y as u64);
    }
}