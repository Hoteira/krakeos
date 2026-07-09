use core::arch::asm;

pub fn read_time() -> u64 {
    let time: usize;
    unsafe {
        asm!("csrr {}, time", out(reg) time);
    }
    time as u64
}

pub fn enable_timer_interrupt() {
    unsafe {
        // sie (Supervisor Interrupt Enable)
        // Bit 5 is STIE (Supervisor Timer Interrupt Enable)
        asm!("csrs sie, {}", in(reg) 1 << 5);
    }
}

pub fn enable_global_interrupts() {
    unsafe {
        // sstatus (Supervisor Status)
        // Bit 1 is SIE (Supervisor Interrupt Enable)
        asm!("csrs sstatus, {}", in(reg) 1 << 1);
    }
}

pub fn write_satp(val: u64) {
    unsafe {
        asm!("csrw satp, {}", in(reg) val);
    }
}

pub fn sfence_vma() {
    unsafe {
        asm!("sfence.vma zero, zero");
    }
}

pub fn fence_i() {
    unsafe {
        asm!("fence.i");
    }
}
