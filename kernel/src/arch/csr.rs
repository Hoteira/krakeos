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
        // sie bit 5: STIE
        asm!("csrs sie, {}", in(reg) 1 << 5);
    }
}

pub fn enable_global_interrupts() {
    unsafe {
        // sstatus bit 1: SIE
        asm!("csrs sstatus, {}", in(reg) 1 << 1);
    }
}

pub fn write_satp(val: u64) {
    unsafe {
        asm!("csrw satp, {}", in(reg) val);
    }
}

// All harts share the page table, so flush remotely once SMP is up.
pub fn sfence_vma() {
    sfence_vma_local();
    if crate::arch::smp::SMP.load(core::sync::atomic::Ordering::Relaxed) {
        crate::arch::sbi::remote_sfence_vma_all();
    }
}

pub fn sfence_vma_local() {
    unsafe {
        asm!("sfence.vma zero, zero");
    }
}

pub fn fence_i() {
    unsafe {
        asm!("fence.i");
    }
}
