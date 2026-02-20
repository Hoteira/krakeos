#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "krakeos:core/system@0.2.0")]
unsafe extern "C" {
    #[link_name = "syscall"]
    pub fn krakeos_syscall(num: u64, arg1: u64, arg2: u64, arg3: u64) -> u64;
    #[link_name = "syscall5"]
    pub fn krakeos_syscall5(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> u64;
    #[link_name = "syscall6"]
    pub fn krakeos_syscall6(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> u64;
    #[link_name = "syscall7"]
    pub fn krakeos_syscall7(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64, arg6: u64) -> u64;
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi_snapshot_preview1")]
unsafe extern "C" {
    fn sched_yield() -> i32;
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn krakeos_syscall(num: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
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
pub unsafe fn krakeos_syscall5(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> u64 {
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
pub unsafe fn krakeos_syscall6(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> u64 {
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
pub unsafe fn krakeos_syscall7(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64, arg6: u64) -> u64 {
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
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let _ = sched_yield();
    }
    #[cfg(not(target_arch = "wasm32"))]
    unsafe {
        core::arch::asm!("int 0x81");
    }
}

pub fn hlt_loop() -> ! {
    loop {
        #[cfg(target_arch = "wasm32")]
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
        let current_brk = krakeos_syscall(12, 0, 0, 0) as usize;
        if current_brk == 0 || current_brk == usize::MAX { return core::ptr::null_mut(); }
        let new_brk = (current_brk + size + 0xFFF) & !0xFFF;
        let res = krakeos_syscall(12, new_brk as u64, 0, 0) as usize;
        if res != usize::MAX && res >= new_brk {
            current_brk as *mut u8
        } else {
            core::ptr::null_mut()
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "krakeos:graphics/screen@0.2.0")]
unsafe extern "C" {
    #[link_name = "get-width"]
    pub fn get_screen_width() -> u32;
    #[link_name = "get-height"]
    pub fn get_screen_height() -> u32;
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn get_screen_width() -> u32 {
    krakeos_syscall(106, 0, 0, 0) as u32
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn get_screen_height() -> u32 {
    krakeos_syscall(107, 0, 0, 0) as u32
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "krakeos:net/raw@0.2.0")]
unsafe extern "C" {
    #[link_name = "send"]
    pub fn krakeos_net_send(ptr: *const u8, len: u32) -> i32;
    #[link_name = "recv"]
    pub fn krakeos_net_recv(ptr: *mut u8, len: u32) -> i32;
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn krakeos_net_send(_ptr: *const u8, _len: u32) -> i32 {
    -1
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn krakeos_net_recv(_ptr: *mut u8, _len: u32) -> i32 {
    0
}
