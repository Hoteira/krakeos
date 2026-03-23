use crate::task::CPUState;
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
        context.rax = crate::task::SYSTEM_TICKS;
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

pub fn handle_get_dmesg(context: &mut CPUState) {
    let ptr = context.rdi as *mut u8;
    let len = context.rsi as usize;
    if ptr.is_null() || len == 0 {
        context.rax = 0;
        return;
    }
    if !super::validate_user_buf(context, ptr as u64, len as u64) { return; }
    let buf = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
    let written = crate::debug::DMESG.lock().read(buf);
    context.rax = written as u64;
}

pub fn handle_args_sizes_get(context: &mut CPUState) {
    if let Some(proc) = super::get_current_process() {
        let args = proc.args.lock();
        let count = args.len() as u64;
        let total_size: usize = args.iter().map(|s| s.len() + 1).sum();
        context.rax = count;
        context.rdi = total_size as u64;
    } else {
        context.rax = u64::MAX;
    }
}

pub fn handle_args_get(context: &mut CPUState) {
    let argv_ptr = context.rdi as *mut *mut u8;
    let argv_buf_ptr = context.rsi as *mut u8;
    if let Some(proc) = super::get_current_process() {
        let args = proc.args.lock();
        let mut offset = 0;
        for (i, arg) in args.iter().enumerate() {
            let p = unsafe { argv_buf_ptr.add(offset) };
            unsafe {
                core::ptr::write_unaligned(argv_ptr.add(i), p);
                core::ptr::copy_nonoverlapping(arg.as_ptr(), p, arg.len());
                *p.add(arg.len()) = 0;
            }
            offset += arg.len() + 1;
        }
        context.rax = 0;
    } else {
        context.rax = u64::MAX;
    }
}

pub fn handle_environ_sizes_get(context: &mut CPUState) {
    if let Some(proc) = super::get_current_process() {
        let env_vars = proc.env_vars.lock();
        let count = env_vars.len() as u64;
        let total_size: usize = env_vars.iter().map(|(k, v)| k.len() + v.len() + 2).sum();
        context.rax = count;
        context.rdi = total_size as u64;
    } else {
        context.rax = u64::MAX;
    }
}

pub fn handle_environ_get(context: &mut CPUState) {
    let env_ptr = context.rdi as *mut *mut u8;
    let env_buf_ptr = context.rsi as *mut u8;
    if let Some(proc) = super::get_current_process() {
        let env_vars = proc.env_vars.lock();
        let mut offset = 0;
        for (i, (k, v)) in env_vars.iter().enumerate() {
            let p = unsafe { env_buf_ptr.add(offset) };
            let entry = alloc::format!("{}={}", k, v);
            unsafe {
                core::ptr::write_unaligned(env_ptr.add(i), p);
                core::ptr::copy_nonoverlapping(entry.as_ptr(), p, entry.len());
                *p.add(entry.len()) = 0;
            }
            offset += entry.len() + 1;
        }
        context.rax = 0;
    } else {
        context.rax = u64::MAX;
    }
}

pub fn handle_clock_res_get(context: &mut CPUState) {
    // Return 1ms resolution
    context.rax = 1_000_000;
}

pub fn handle_clock_time_get(context: &mut CPUState) {
    let (h, m, s) = crate::drivers::rtc::get_time();
    let (d, mo, y) = crate::drivers::rtc::get_date();
    
    // Simplistic epoch conversion
    let yrs = if y >= 1970 { (y - 1970) as u64 } else { 0 };
    let secs = yrs * 31_536_000
        + (mo as u64).saturating_sub(1) * 2_592_000
        + (d as u64).saturating_sub(1) * 86_400
        + (h as u64) * 3600
        + (m as u64) * 60
        + s as u64;
    context.rax = secs * 1_000_000_000;
}

pub fn handle_random_get(context: &mut CPUState) {
    let buf_ptr = context.rdi as *mut u8;
    let len = context.rsi as usize;
    if !super::validate_user_buf(context, buf_ptr as u64, len as u64) { return; }
    
    for i in 0..len {
        unsafe {
            // Using RDRAND if available, or fallback to something simple
            let mut val: u64 = 0;
            core::arch::asm!("rdrand {}", out(reg) val);
            *buf_ptr.add(i) = val as u8;
        }
    }
    context.rax = 0;
}
