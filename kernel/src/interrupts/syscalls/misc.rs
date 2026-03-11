use crate::interrupts::task::CPUState;
use alloc::string::String;

pub fn handle_debug_print(context: &mut CPUState) {
    let ptr = context.rdi;
    let len = context.rsi as usize;

    if !super::validate_user_buf(context, ptr, len as u64) { return; }
    let s = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
    let str_val = String::from_utf8_lossy(s);

    crate::debug_print!("{}", str_val);

    context.rax = len as u64;
}

pub fn handle_time(context: &mut CPUState) {
    let (h, m, s) = crate::drivers::rtc::get_time();
    context.rax = ((h as u64) << 16) | ((m as u64) << 8) | (s as u64);
}

pub fn handle_date(context: &mut CPUState) {
    let (d, m, y) = crate::drivers::rtc::get_date();
    context.rax = ((y as u64) << 16) | ((m as u64) << 8) | (d as u64);
}

pub fn handle_ticks(context: &mut CPUState) {
    unsafe {
        context.rax = crate::interrupts::task::SYSTEM_TICKS;
    }
}

pub fn handle_get_total_mem(context: &mut CPUState) {
    context.rax = crate::memory::pmm::get_total_memory() as u64;
}

pub fn handle_get_used_mem(context: &mut CPUState) {
    context.rax = crate::memory::pmm::get_used_memory() as u64;
}

pub fn handle_get_vma_dump(context: &mut CPUState) {
    let ptr = context.rdi as *mut u8;
    let len = context.rsi as usize;
    if ptr.is_null() || len == 0 {
        context.rax = 0;
        return;
    }
    if !super::validate_user_buf(context, ptr as u64, len as u64) { return; }
    let buf = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
    let written = crate::memory::vma::GLOBAL_VMA.lock().dump_to_buffer(buf);
    context.rax = written as u64;
}
