use crate::interrupts::task::{CPUState, TASK_MANAGER, TaskState, SYSTEM_TICKS};
use crate::drivers::periferics::keyboard::KEYBOARD_BUFFER;
use crate::window_manager::input::MOUSE;
use alloc::string::String;

pub fn sys_read(context: &mut CPUState) {
    let user_ptr = context.rdi as *mut u8; 
    let user_len = context.rsi as usize;
    let mut bytes_written_to_user = 0;

    if user_ptr.is_null() {
        context.rax = 0;
        return;
    }

    let mut keyboard_buffer = KEYBOARD_BUFFER.lock();
    while bytes_written_to_user < user_len {
        if let Some(keycode) = keyboard_buffer.pop_front() {
            unsafe {
                *user_ptr.add(bytes_written_to_user) = keycode as u8;
            }
            bytes_written_to_user += 1;
        } else {
            break;
        }
    }
    context.rax = bytes_written_to_user as u64;
}

pub fn sys_print(context: &mut CPUState) {
    let ptr = context.rdi; 
    let len = context.rsi as usize;
    let s = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
    let str_val = String::from_utf8_lossy(s);
    crate::debug_print!("{}", str_val);
    context.rax = len as u64;
}

pub fn sys_get_mouse(context: &mut CPUState) {
    unsafe {
        let mouse = &*(&raw const MOUSE);
        context.rax = ((mouse.x as u64) << 32) | (mouse.y as u64);
    }
}

pub fn sys_get_time(context: &mut CPUState) {
    let (h, m, s) = crate::drivers::rtc::get_time();
    context.rax = ((h as u64) << 16) | ((m as u64) << 8) | (s as u64);
}

pub fn sys_get_ticks(context: &mut CPUState) {
    unsafe {
        context.rax = SYSTEM_TICKS;
    }
}

pub fn sys_sleep(context: &mut CPUState) {
    let duration = context.rdi;
    let mut tm = TASK_MANAGER.int_lock();
    let current = tm.current_task;

    if current >= 0 {
        let task = &mut tm.tasks[current as usize];
        task.wake_ticks = unsafe { SYSTEM_TICKS } + duration;
        task.state = TaskState::Sleeping;
    }
}
