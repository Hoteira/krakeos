// Raw host primitives (arch-gated via method_export!)

method_export!("krakeos:system/process@0.2.0", "yield",
    pub fn host_yield() {
        #[cfg(not(target_arch = "wasm32"))]
        unsafe { core::arch::asm!("int 0x81"); }
    }
);

method_export!("krakeos:system/memory@0.2.0", "brk",
    pub fn host_brk(addr: u64) -> u64 {
        #[cfg(not(target_arch = "wasm32"))]
        unsafe { crate::sys::syscall(12, addr, 0, 0) }
        #[cfg(target_arch = "wasm32")]
        0
    }
);

method_export!("krakeos:system/process@0.2.0", "spawn-thread",
    pub fn host_spawn_thread(entry: u64, stack: u64, arg: u64) -> u64 {
        #[cfg(not(target_arch = "wasm32"))]
        unsafe { crate::sys::syscall(112, entry, stack, arg) }
        #[cfg(target_arch = "wasm32")]
        0
    }
);

#[cfg(not(target_arch = "wasm32"))]
#[cfg(not(feature = "userland"))]
unsafe extern "C" {
    pub fn syscall_dispatcher(
        num: u64,
        arg1: u64,
        arg2: u64,
        arg3: u64,
        arg4: u64,
        arg5: u64,
        arg6: u64,
    ) -> u64;
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn syscall(num: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    #[cfg(feature = "userland")]
    {
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
    #[cfg(not(feature = "userland"))]
    {
        syscall_dispatcher(num, arg1, arg2, arg3, 0, 0, 0)
    }
}

#[cfg(target_arch = "wasm32")]
pub unsafe fn syscall(_num: u64, _arg1: u64, _arg2: u64, _arg3: u64) -> u64 {
    0
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn syscall4(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> u64 {
    #[cfg(feature = "userland")]
    {
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
    #[cfg(not(feature = "userland"))]
    {
        syscall_dispatcher(num, arg1, arg2, arg3, arg4, 0, 0)
    }
}

#[cfg(target_arch = "wasm32")]
pub unsafe fn syscall4(_num: u64, _a1: u64, _a2: u64, _a3: u64, _a4: u64) -> u64 { 0 }

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn syscall5(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> u64 {
    #[cfg(feature = "userland")]
    {
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
    #[cfg(not(feature = "userland"))]
    {
        syscall_dispatcher(num, arg1, arg2, arg3, arg4, arg5, 0)
    }
}

#[cfg(target_arch = "wasm32")]
pub unsafe fn syscall5(_num: u64, _a1: u64, _a2: u64, _a3: u64, _a4: u64, _a5: u64) -> u64 { 0 }

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn syscall6(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64, arg6: u64) -> u64 {
    #[cfg(feature = "userland")]
    {
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
    #[cfg(not(feature = "userland"))]
    {
        syscall_dispatcher(num, arg1, arg2, arg3, arg4, arg5, arg6)
    }
}

#[cfg(target_arch = "wasm32")]
pub unsafe fn syscall6(_num: u64, _a1: u64, _a2: u64, _a3: u64, _a4: u64, _a5: u64, _a6: u64) -> u64 { 0 }

pub unsafe fn syscall1(num: u64, a1: u64) -> u64 { syscall(num, a1, 0, 0) }
pub unsafe fn syscall2(num: u64, a1: u64, a2: u64) -> u64 { syscall(num, a1, a2, 0) }
pub unsafe fn syscall3(num: u64, a1: u64, a2: u64, a3: u64) -> u64 { syscall(num, a1, a2, a3) }

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
        // crate::os::debug_print("[std] alloc_pages: calling memory_grow...\n");
        let prev = core::arch::wasm32::memory_grow(0, pages);
        if prev == usize::MAX {
            // crate::os::debug_print("[std] alloc_pages: memory_grow FAILED\n");
            core::ptr::null_mut()
        } else {
            // crate::os::debug_print("[std] alloc_pages: memory_grow success\n");
            (prev * 65536) as *mut u8
        }
    }    #[cfg(not(target_arch = "wasm32"))]
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

method_export!("krakeos:system/process@0.2.0", "thread-exit",
    pub fn host_thread_exit() {
        crate::sys::syscall(113, 0, 0, 0);
    }
);

pub fn spawn_thread<F>(f: F) -> u64
where
    F: FnOnce() + Send + 'static,
{
    use crate::alloc::boxed::Box;

    let main = Box::new(f);
    let main_ptr = Box::into_raw(main);

    extern "C" fn thread_entry<F>(main_ptr: *mut F)
    where
        F: FnOnce() + Send + 'static,
    {
        let main = unsafe { Box::from_raw(main_ptr) };
        main();
        host_thread_exit();
        loop {} // Safety
    }

    let stack_size = 1024 * 1024; // 1MB
    let stack_base = unsafe { alloc_pages(stack_size) } as u64;
    let stack_top = stack_base + stack_size as u64;

    host_spawn_thread(thread_entry::<F> as u64, stack_top, main_ptr as u64)
}
