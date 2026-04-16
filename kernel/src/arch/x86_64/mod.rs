pub mod io;
pub mod gdt;
pub mod idt;
pub mod pic;
pub mod tss;
pub mod exceptions;
pub mod acpi;
pub mod apic;
pub mod smp;

use core::arch::asm;

pub static mut USING_APIC: bool = false;

pub const EFER_MSR: u32 = 0xC0000080;
pub const STAR_MSR: u32 = 0xC0000081;
pub const LSTAR_MSR: u32 = 0xC0000082;
pub const SFMASK_MSR: u32 = 0xC0000084;
pub const PAT_MSR: u32 = 0x277;

pub unsafe fn rdmsr(msr: u32) -> u64 {
    let (low, high): (u32, u32);
    asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high);
    ((high as u64) << 32) | (low as u64)
}

pub unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    asm!("wrmsr", in("ecx") msr, in("eax") low, in("edx") high);
}

pub fn init_pat() {
    unsafe {
        let mut pat = rdmsr(PAT_MSR);
        pat &= !(0xFFu64 << 32);
        pat |= 0x01u64 << 32;
        wrmsr(PAT_MSR, pat);
        let cr3: u64;
        asm!("mov {}, cr3", out(reg) cr3);
        asm!("mov cr3, {}", in(reg) cr3);
    }
}

/// Enable SSE/SSE2 for use in kernel and user mode.
/// Must be called on every CPU (BSP in rust_main, each AP in ap_entrance).
pub fn init_fpu() {
    unsafe {
        // Set CR4.OSFXSR (bit 9) and CR4.OSXMMEXCPT (bit 10)
        // Without OSFXSR, SSE/SSE2 instructions raise #UD even in ring 0.
        let mut cr4: u64;
        asm!("mov {}, cr4", out(reg) cr4);
        cr4 |= (1 << 9) | (1 << 10);
        asm!("mov cr4, {}", in(reg) cr4);

        // CR0: set MP (bit 1), clear EM (bit 2) and TS (bit 3)
        let mut cr0: u64;
        asm!("mov {}, cr0", out(reg) cr0);
        cr0 |= 1 << 1;    // MP: monitor FPU (needed for FXSAVE)
        cr0 &= !(1 << 2); // EM=0: no FPU emulation
        cr0 &= !(1 << 3); // TS=0: task not switched (SSE without #NM)
        asm!("mov cr0, {}", in(reg) cr0);
    }
}

pub fn init_syscall_msrs() {
    unsafe {
        let mut efer = rdmsr(EFER_MSR);
        efer |= 1; // SCE
        efer |= 1 << 11; // NXE
        wrmsr(EFER_MSR, efer);
        let sysret_cs_base = 0x10; // SYSRET: CS = base+16 = 0x20|3, SS = base+8 = 0x18|3
        let syscall_cs_base = 0x08; // SYSCALL: CS = 0x08 (kernel_code_64), SS = 0x10 (kernel_data)
        let star_value = ((sysret_cs_base as u64) << 48) | ((syscall_cs_base as u64) << 32);
        wrmsr(STAR_MSR, star_value);
        wrmsr(LSTAR_MSR, crate::syscalls::syscall_entry as u64);
        let rflags_mask = (1 << 9) | (1 << 8);
        wrmsr(SFMASK_MSR, rflags_mask);
    }
}
