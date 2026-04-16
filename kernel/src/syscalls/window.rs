use crate::task::CPUState;
use crate::window_manager::composer::COMPOSER;
use crate::window_manager::display::DISPLAY_SERVER;
use crate::window_manager::input::MOUSE;
use crate::window_manager::window::Window;

pub fn handle_add_window(context: &mut CPUState) {
    let ptr = context.rdi;
    crate::debugln!("[Syscall] handle_add_window called! ptr={:#x}", ptr);
    let win_size = core::mem::size_of::<Window>() as u64;
    if !super::validate_user_buf(context, ptr, win_size) { 
        crate::debugln!("[Syscall] handle_add_window FAILED validation!");
        return; 
    }

    let mut tm = crate::task::TASK_MANAGER.int_lock();
    if let Some(current) = tm.current_task_idx() {
        if let Some(thread) = tm.tasks.get(&(current)) {
            let proc = thread.process.as_ref().expect("Thread has no process");

            let mut w = unsafe { *(context.rdi as *const Window) };
            w.pid = proc.pid;

            drop(tm);
            let mut composer = COMPOSER.write();
            let id = composer.add_window(w);
            if w.w_type == crate::window_manager::window::Items::Window {
                crate::window_manager::composer::CLICKED_WINDOW_ID.store(id as usize, core::sync::atomic::Ordering::SeqCst);
                composer.focus_window(id);
            }
            context.rax = id as u64;
        } else {
            context.rax = u64::MAX;
        }
    } else {
        context.rax = u64::MAX;
    }
}

pub fn handle_update_window(context: &mut CPUState) {
    let win_size = core::mem::size_of::<Window>() as u64;
    if !super::validate_user_buf(context, context.rdi, win_size) { return; }

    let w = unsafe { *(context.rdi as *const Window) };

    // Phase 1: data mutation — hold COMPOSER.write() only for the state update.
    // update_window_data returns the dirty rect without calling update_window_area_rect,
    // so we drop the exclusive write lock before touching DISPLAY_SERVER.
    let dirty_rect: Option<(i32, i32, u32, u32)> = {
        let mut composer = COMPOSER.write();
        let mut tm = crate::task::TASK_MANAGER.int_lock();
        if let Some(current) = tm.current_task_idx() {
            if let Some(thread) = tm.tasks.get(&(current)) {
                let proc = thread.process.as_ref().expect("Thread has no process");
                if let Some(existing_win) = composer.find_window_id(w.id as u64) {
                    if existing_win.pid == proc.pid {
                        drop(tm);
                        context.rax = 1;
                        composer.update_window_data(w)
                    } else {
                        context.rax = 0;
                        None
                    }
                } else {
                    context.rax = 0;
                    None
                }
            } else {
                context.rax = 0;
                None
            }
        } else {
            context.rax = 0;
            None
        }
    }; // COMPOSER.write() dropped here — interrupts re-enabled

    // Phase 2: render — read lock only, interrupts re-enabled between composite and flush.
    if let Some((dx, dy, dw, dh)) = dirty_rect {
        let composer = COMPOSER.read();
        composer.update_window_area_rect(dx, dy, dw, dh);
    }
}

pub fn handle_update_window_area(context: &mut CPUState) {
    let wid = context.rdi as u64;
    let x = context.rsi as i32;
    let y = context.rdx as i32;
    let w = context.r10 as u32;
    let h = context.r8 as u32;

    let composer = COMPOSER.read();
    if let Some(win) = composer.find_window_id_immut(wid) {
        let global_x = win.x as i32 + x;
        let global_y = win.y as i32 + y;
        composer.update_window_area_rect(global_x, global_y, w, h);
    }
    context.rax = 1;
}

pub fn handle_get_events(context: &mut CPUState) {
    use core::sync::atomic::Ordering;
    use crate::window_manager::events::{Event, EventQueueHeader, GLOBAL_EVENT_QUEUE};
    use crate::task::TASK_MANAGER;

    let wid = context.rdi as u32;
    let max_events = context.rdx as usize;
    let buf_size = (max_events as u64) * (core::mem::size_of::<Event>() as u64);
    if !super::validate_user_buf(context, context.rsi, buf_size) { return; }

    let mut count = 0usize;
    let buf_ptr = context.rsi as *mut Event;

    // Phase 1: Local Event Queue
    {
        let tm = TASK_MANAGER.int_lock();
        if let Some(idx) = tm.current_task_idx() {
            if let Some(thread) = tm.tasks.get(&(idx)) {
                if let Some(proc) = thread.process.as_ref() {
                    let (header_ptr, buf_virt, capacity) = *proc.event_queue.lock();
                    if header_ptr != 0 {
                        let header = unsafe { &*(header_ptr as *const EventQueueHeader) };
                        while count < max_events {
                            let tail = header.tail.load(Ordering::Relaxed);
                            if tail == header.head.load(Ordering::Acquire) {
                                break;
                            }
                            let event = unsafe { (buf_virt as *const Event).add(tail as usize).read() };
                            
                            if event.get_window_id() == wid || wid == 0 {
                                // Match! Remove from queue and copy to user
                                header.tail.store((tail + 1) % capacity, Ordering::Release);
                                unsafe { buf_ptr.add(count).write(event); }
                                count += 1;
                            } else {
                                // Not for this window. Stop draining local queue so we don't
                                // discard events for other windows.
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    // Phase 2: Global Event Queue
    if count < max_events {
        let mut global_queue = GLOBAL_EVENT_QUEUE.int_lock();
        let global_events = global_queue.get_and_remove_events(wid, max_events - count);
        for evt in global_events {
            unsafe { buf_ptr.add(count).write(evt); }
            count += 1;
        }
    }

    context.rax = count as u64;
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

    let mut tm = crate::task::TASK_MANAGER.int_lock();
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
    let tm = crate::task::TASK_MANAGER.int_lock();
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
    context.rax = DISPLAY_SERVER.lock().width;
}

pub fn handle_get_height(context: &mut CPUState) {
    context.rax = DISPLAY_SERVER.lock().height;
}

pub fn handle_get_mouse(context: &mut CPUState) {
    let mouse = MOUSE.lock();
    context.rax = ((mouse.x as u64) << 32) | (mouse.y as u64);
}