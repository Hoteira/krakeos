use crate::interrupts::task::CPUState;
use crate::window_manager::window::Window;
use crate::window_manager::composer::COMPOSER;
use crate::window_manager::display::DISPLAY_SERVER;

pub fn sys_add_window(context: &mut CPUState) {
    let window_ptr = context.rdi as *const Window;
    unsafe {
        let mut w = *window_ptr;
        if let Some(current) = crate::interrupts::task::TASK_MANAGER.int_lock().current_task_idx() {
            w.pid = current as u64;
        }
        context.rax = (*(&raw mut COMPOSER)).add_window(w) as u64;
    }
}

pub fn sys_update_window(context: &mut CPUState) {
    let window_ptr = context.rdi as *const Window;
    unsafe {
        let w = *window_ptr;
        (*(&raw mut COMPOSER)).resize_window(w);
    }
    context.rax = 1;
}

pub fn sys_update_window_area(context: &mut CPUState) {
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

pub fn sys_get_events(context: &mut CPUState) {
    let wid = context.rdi as u32;
    let buf_ptr = context.rsi as *mut crate::window_manager::events::Event;
    let max_events = context.rdx as usize;
    
    unsafe {
        use crate::window_manager::events::GLOBAL_EVENT_QUEUE;
        let events = GLOBAL_EVENT_QUEUE.lock().get_and_remove_events(wid, max_events);
        
        let user_slice = core::slice::from_raw_parts_mut(buf_ptr, max_events);
        for (i, evt) in events.into_iter().enumerate() {
            if i < max_events {
                user_slice[i] = evt;
            }
        }
    }
    context.rax = 1;
}

pub fn sys_get_width(context: &mut CPUState) {
    unsafe {
        context.rax = (*(&raw mut DISPLAY_SERVER)).width;
    }
}

pub fn sys_get_height(context: &mut CPUState) {
    unsafe {
        context.rax = (*(&raw mut DISPLAY_SERVER)).height;
    }
}
