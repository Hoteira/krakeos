#![no_std]
#![no_main]

use crate::debug::debug;
use core::arch::asm;

mod debug;
mod disk;

const STACK_ADDR: u64 = 0xA00000;

pub const NEXT_STAGE_LBA: u64 = 6144;
pub const KERNEL_RAM: u64 = 0xFFFFFFFF00100000;

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

    debug("Stage 4 loaded.\n");

    unsafe {
        asm!(
        "call {0}",
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

