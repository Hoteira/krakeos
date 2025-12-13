use core::arch::asm;

pub unsafe fn syscall(num: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    let result: u64;

    unsafe {
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