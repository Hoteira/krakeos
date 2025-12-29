#![no_std]
#![no_main]
mod disk;

use core::arch::asm;
use core::arch::global_asm;
use core::include_str;
use core::panic::PanicInfo;

global_asm!(include_str!("stage1.asm"));

const NEXT_STAGE_RAM: u16 = 0x7e00;
const NEXT_STAGE_LBA: u64 = 2048;

#[unsafe(no_mangle)]
pub extern "C" fn _boot() -> ! {
    disk::read_stub();
    unsafe {
        asm!("jmp {0:x}", in(reg) NEXT_STAGE_RAM);
    }

    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn fail() -> ! {
    loop {}
}
