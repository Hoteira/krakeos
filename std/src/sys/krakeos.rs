use core::arch::asm;

pub unsafe fn syscall(num: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    let result: u64;
    asm!(
    "syscall",
    in("rax") num,
    in("rdi") arg1,
    in("rsi") arg2,
    in("rdx") arg3,
    lateout("rax") result,
    out("rcx") _,
    out("r11") _,
    options(nostack, preserves_flags)
    );
    result
}

pub unsafe fn syscall1(num: u64, arg1: u64) -> u64 {
    syscall(num, arg1, 0, 0)
}

pub unsafe fn syscall2(num: u64, arg1: u64, arg2: u64) -> u64 {
    syscall(num, arg1, arg2, 0)
}

pub unsafe fn syscall3(num: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    syscall(num, arg1, arg2, arg3)
}

pub unsafe fn syscall4(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> u64 {
    let result: u64;
    asm!(
    "syscall",
    in("rax") num,
    in("rdi") arg1,
    in("rsi") arg2,
    in("rdx") arg3,
    in("r10") arg4,
    lateout("rax") result,
    out("rcx") _,
    out("r11") _,
    options(nostack, preserves_flags)
    );
    result
}

pub unsafe fn syscall5(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> u64 {
    let result: u64;
    asm!(
    "syscall",
    in("rax") num,
    in("rdi") arg1,
    in("rsi") arg2,
    in("rdx") arg3,
    in("r10") arg4,
    in("r8") arg5,
    lateout("rax") result,
    out("rcx") _,
    out("r11") _,
    options(nostack, preserves_flags)
    );
    result
}

pub unsafe fn syscall6(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64, arg6: u64) -> u64 {
    let result: u64;
    asm!(
    "syscall",
    in("rax") num,
    in("rdi") arg1,
    in("rsi") arg2,
    in("rdx") arg3,
    in("r10") arg4,
    in("r8") arg5,
    in("r9") arg6,
    lateout("rax") result,
    out("rcx") _,
    out("r11") _,
    options(nostack, preserves_flags)
    );
    result
}

pub fn yield_task() {
    unsafe {
        asm!("int 0x81");
    }
}

pub fn hlt_loop() -> ! {
    loop {
        unsafe { asm!("hlt"); }
    }
}

pub unsafe fn alloc_pages(size: usize) -> *mut u8 {
    let current_brk = syscall(12, 0, 0, 0) as usize;
    if current_brk == 0 { return core::ptr::null_mut(); }
    let new_brk = (current_brk + size + 0xFFF) & !0xFFF;
    if syscall(12, new_brk as u64, 0, 0) as usize >= new_brk {
        current_brk as *mut u8
    } else {
        core::ptr::null_mut()
    }
}

pub mod preview2_bindings {
    pub unsafe fn krakeos_net_send(_ptr: *const u8, _len: u32) -> i32 { -1 }
    pub unsafe fn krakeos_net_recv(_ptr: *mut u8, _len: u32) -> i32 { 0 }
}
