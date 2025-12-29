#![no_std]
#![no_main]

use core::arch::asm;
use crate::debug::debug;

mod debug;
mod disk;

const STACK_ADDR: u64 = 0xA00000;

pub const NEXT_STAGE_LBA: u64 = 6144;
pub const KERNEL_RAM: u32 = 0x10_0000;

#[unsafe(no_mangle)]
#[unsafe(link_section = ".start")]
pub extern "C" fn _start() -> ! {
    let rdi: u64;

    unsafe {
        asm!(
        "mov ax, 0x10",
        "mov ds, ax",
        "mov es, ax",
        "mov fs, ax",
        "mov gs, ax",
        "mov ss, ax",
        "mov rsp, {0}",
        in(reg) STACK_ADDR,
        options(nostack),
        out("rdi") rdi,
        );

    }

    

    unsafe {
        asm!(
        "call {0:r}",
        in(reg) KERNEL_RAM,
        in("rdi") rdi,
        options(nostack),
        );
    }

    loop {}
}

#[panic_handler]
pub fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

