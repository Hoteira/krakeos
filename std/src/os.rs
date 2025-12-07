use core::arch::asm;

pub unsafe fn syscall(num: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    let result: u64;

    unsafe {
        asm!(
            "syscall",
            in("rax") num,    // Syscall number in RAX
            in("rdi") arg1,   // Arg1 in RDI
            in("rsi") arg2,   // Arg2 in RSI
            in("rdx") arg3,   // Arg3 in RDX
            // RCX and R11 are clobbered by syscall/sysret, so they don't need to be listed in `out` or `lateout`.
            // RBP, RBX, R12-R15 are preserved by convention.
            lateout("rax") result, // Return value in RAX
            options(nostack, preserves_flags)
        );
    }

    result
}

pub fn print(s: &str) {
    unsafe {
        syscall(1, s.as_ptr() as u64, s.len() as u64, 0);
    }
}

pub fn yield_task() {
    unsafe {
        asm!("int 0x20");
    }

}



pub fn read(buffer: &mut [u8]) -> usize {
    unsafe {
        syscall(0, buffer.as_mut_ptr() as u64, buffer.len() as u64, 0) as usize
    }
}



pub fn exit(code: u64) -> ! {

    unsafe {

        syscall(60, code, 0, 0);

        loop { asm!("hlt"); }

    }

}