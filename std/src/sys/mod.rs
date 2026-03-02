// Raw host primitives (arch-gated via method_export!)

method_export!("krakeos:system/process@0.2.0", "yield",
    pub fn host_yield() {
        core::arch::asm!("int 0x81");
    }
);

method_export!("krakeos:system/memory@0.2.0", "brk",
    pub fn host_brk(addr: u64) -> u64 {
        crate::sys::syscall(12, addr, 0, 0)
    }
);

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn syscall(num: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    let result: u64;
    core::arch::asm!(
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

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn syscall4(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> u64 {
    let result: u64;
    core::arch::asm!(
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

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn syscall5(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> u64 {
    let result: u64;
    core::arch::asm!(
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

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn syscall6(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64, arg6: u64) -> u64 {
    let result: u64;
    core::arch::asm!(
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
    host_yield();
}

pub fn hlt_loop() -> ! {
    loop {
        yield_task();
        #[cfg(not(target_arch = "wasm32"))]
        unsafe { core::arch::asm!("hlt"); }
    }
}

pub unsafe fn alloc_pages(size: usize) -> *mut u8 {
    #[cfg(target_arch = "wasm32")]
    {
        let pages = (size + 65535) / 65536;
        let prev = core::arch::wasm32::memory_grow(0, pages);
        if prev == usize::MAX { core::ptr::null_mut() } else { (prev * 65536) as *mut u8 }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let current_brk = host_brk(0) as usize;
        if current_brk == 0 || current_brk == usize::MAX { return core::ptr::null_mut(); }
        let new_brk = (current_brk + size + 0xFFF) & !0xFFF;
        let res = host_brk(new_brk as u64) as usize;
        if res != usize::MAX && res >= new_brk {
            current_brk as *mut u8
        } else {
            core::ptr::null_mut()
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn syscall1(num: u64, a1: u64) -> u64 { syscall(num, a1, 0, 0) }
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn syscall2(num: u64, a1: u64, a2: u64) -> u64 { syscall(num, a1, a2, 0) }
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn syscall3(num: u64, a1: u64, a2: u64, a3: u64) -> u64 { syscall(num, a1, a2, a3) }