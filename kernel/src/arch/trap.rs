use core::arch::naked_asm;
use crate::{csr, sbi, scheduler, println};

#[repr(C)]
pub struct TrapFrame {
    pub regs: [usize; 32],
    pub sepc: usize,
    pub sstatus: usize,
}

#[unsafe(naked)]
pub unsafe extern "C" fn trap_vector() {
    naked_asm!(
        "csrrw sp, sscratch, sp",
        "bnez sp, 1f",
        "csrrw sp, sscratch, sp",
        "1:",
        "addi sp, sp, -272",
        
        "sd t0, 5*8(sp)", // Save t0 (x5) first
        
        "csrr t0, sscratch",
        "csrw sscratch, zero", // Clear sscratch so nested traps know they came from S-mode
        "bnez t0, 2f",
        "addi t0, sp, 272",
        "2:",
        "sd t0, 2*8(sp)", // Save x2
        
        "sd x1, 1*8(sp)",
        "sd x3, 3*8(sp)",
        "sd x4, 4*8(sp)",
        // 5 is saved above
        "sd x6, 6*8(sp)",
        "sd x7, 7*8(sp)",
        "sd x8, 8*8(sp)",
        "sd x9, 9*8(sp)",
        "sd x10, 10*8(sp)",
        "sd x11, 11*8(sp)",
        "sd x12, 12*8(sp)",
        "sd x13, 13*8(sp)",
        "sd x14, 14*8(sp)",
        "sd x15, 15*8(sp)",
        "sd x16, 16*8(sp)",
        "sd x17, 17*8(sp)",
        "sd x18, 18*8(sp)",
        "sd x19, 19*8(sp)",
        "sd x20, 20*8(sp)",
        "sd x21, 21*8(sp)",
        "sd x22, 22*8(sp)",
        "sd x23, 23*8(sp)",
        "sd x24, 24*8(sp)",
        "sd x25, 25*8(sp)",
        "sd x26, 26*8(sp)",
        "sd x27, 27*8(sp)",
        "sd x28, 28*8(sp)",
        "sd x29, 29*8(sp)",
        "sd x30, 30*8(sp)",
        "sd x31, 31*8(sp)",
        
        "csrr t0, sepc",
        "sd t0, 32*8(sp)",
        "csrr t0, sstatus",
        "sd t0, 33*8(sp)",

        "mv a0, sp",
        "call trap_handler",
        
        "mv sp, a0",

        "ld t0, 33*8(sp)",
        "csrw sstatus, t0",
        "ld t0, 32*8(sp)",
        "csrw sepc, t0",

        "ld t0, 33*8(sp)", // Load sstatus again into t0 before testing SPP
        "andi t0, t0, 0x100", // SPP bit
        "bnez t0, 3f",
        "ld t1, 2*8(sp)",
        "csrw sscratch, t1",
        "3:",

        "ld x1, 1*8(sp)",
        "ld x3, 3*8(sp)",
        "ld x4, 4*8(sp)",
        // Skip 5
        "ld x6, 6*8(sp)",
        "ld x7, 7*8(sp)",
        "ld x8, 8*8(sp)",
        "ld x9, 9*8(sp)",
        "ld x10, 10*8(sp)",
        "ld x11, 11*8(sp)",
        "ld x12, 12*8(sp)",
        "ld x13, 13*8(sp)",
        "ld x14, 14*8(sp)",
        "ld x15, 15*8(sp)",
        "ld x16, 16*8(sp)",
        "ld x17, 17*8(sp)",
        "ld x18, 18*8(sp)",
        "ld x19, 19*8(sp)",
        "ld x20, 20*8(sp)",
        "ld x21, 21*8(sp)",
        "ld x22, 22*8(sp)",
        "ld x23, 23*8(sp)",
        "ld x24, 24*8(sp)",
        "ld x25, 25*8(sp)",
        "ld x26, 26*8(sp)",
        "ld x27, 27*8(sp)",
        "ld x28, 28*8(sp)",
        "ld x29, 29*8(sp)",
        "ld x30, 30*8(sp)",
        "ld x31, 31*8(sp)",

        "csrr t0, sstatus",
        "andi t0, t0, 0x100",
        "bnez t0, 4f",
        
        "ld t0, 5*8(sp)",
        "addi sp, sp, 272",
        "csrrw sp, sscratch, sp",
        "sret",
        
        "4:",
        "csrw sscratch, zero",
        "ld t0, 5*8(sp)",
        "addi sp, sp, 272",
        "sret",
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn trap_handler(sp: usize) -> usize {
    let scause: usize;
    unsafe {
        core::arch::asm!("csrr {}, scause", out(reg) scause);
    }

    let is_interrupt = (scause >> 63) != 0;
    let code = scause & !(1 << 63);
    let frame = unsafe { &mut *(sp as *mut TrapFrame) };

    if is_interrupt {
        match code {
            5 => {
                let next_tick = csr::read_time() + 10_000;
                sbi::set_timer(next_tick);
                
                return unsafe { scheduler::switch(sp) };
            }
            _ => {
                println!("Unhandled interrupt: {}", code);
            }
        }
    } else {
        if code == 8 { // U-mode ecall
            let new_sp = crate::sys::syscall::dispatch(frame);
            frame.sepc += 4;
            return new_sp.unwrap_or(sp);
        }

        if code == 2 { // Illegal Instruction
            let fs = (frame.sstatus >> 13) & 3;
            if fs == 0 {
                scheduler::handle_fpu_fault();
                frame.sstatus |= 1 << 13;
                return sp;
            }
        }

        let sepc: usize;
        let stval: usize;
        unsafe {
            core::arch::asm!("csrr {}, sepc", out(reg) sepc);
            core::arch::asm!("csrr {}, stval", out(reg) stval);
        }
        println!("KERNEL EXCEPTION!");
        println!("scause: {:#x}", scause);
        println!("sepc:   {:#x}", sepc);
        println!("stval:  {:#x}", stval);
        loop {}
    }
    
    sp
}
