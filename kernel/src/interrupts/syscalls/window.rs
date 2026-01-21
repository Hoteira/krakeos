use crate::interrupts::task::CPUState;
use crate::window_manager::composer::COMPOSER;
use crate::window_manager::display::DISPLAY_SERVER;
use crate::window_manager::input::MOUSE;
use crate::window_manager::window::Window;

pub fn handle_add_window(context: &mut CPUState) {
    let window_ptr = context.rdi as *const Window;
    unsafe {
        let mut w = *window_ptr;
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        if let Some(current) = tm.current_task_idx() {
            if let Some(thread) = tm.tasks[current].as_ref() {
                let proc = thread.process.as_ref().expect("Thread has no process");
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
    let window_ptr = context.rdi as *const Window;
    unsafe {
        let w = *window_ptr;
        let composer = &mut *(&raw mut COMPOSER);

        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        if let Some(current) = tm.current_task_idx() {
            if let Some(existing_win) = composer.find_window_id(w.id) {
                if let Some(thread) = tm.tasks[current].as_ref() {
                    let proc = thread.process.as_ref().expect("Thread has no process");
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
    let wid = context.rdi as u32;
    let buf_ptr = context.rsi as *mut crate::window_manager::events::Event;
    let max_events = context.rdx as usize;

    unsafe {
        use crate::window_manager::events::GLOBAL_EVENT_QUEUE;
        let events = GLOBAL_EVENT_QUEUE.int_lock().get_and_remove_events(wid, max_events);

        if !events.is_empty() {}

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