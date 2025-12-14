#![no_std]
#![no_main]

mod disk;
mod gdt;
mod tss;
mod mmap;
mod vbe;
mod debug;
mod rsdp;

use core::ptr::addr_of;
use gdt::GDT;

use core::arch::asm;
use core::panic::PanicInfo;
use crate::debug::debug;
use crate::mmap::{get_mmap, MemoryMap};
use crate::rsdp::{get_rsdp, Rsdp};
use crate::vbe::{find_vbe_mode, get_vbe_info, VbeInfoBlock, VbeModeInfoBlock};


static mut BOOT: BootInfo = unsafe { core::mem::zeroed() };
static mut VBE_MODE: VbeModeInfoBlock = unsafe { core::mem::zeroed() };

pub const NEXT_STAGE_RAM: u16 = 0xFE00;
pub const NEXT_STAGE_LBA: u64 = 3072;

pub const MAX_BPP: u8 = 32;
pub const MIN_BPP: u8 = 32;

pub const MAX_WIDTH: u16 = u16::MAX; //to get the biggest sire just set it to u16::MAX
pub const MIN_WIDTH: u16 = 0;
pub const MAX_HEIGHT: u16 = u16::MAX;
pub const MIN_HEIGHT: u16 = 0;

pub const MODE: u16 = 0x1; // 0 => VGA, 1 => VBE


#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct BootInfo {
    mmap: MemoryMap,
    rsdp: Rsdp,
    tss: u16,
    vbe: VbeInfoBlock,
    mode: VbeModeInfoBlock,
    pml4: u64,
    kernel_stack: u64,
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".start")]
pub extern "C" fn _start() -> ! {
    debug("[+] Loading stage 3 ...\n");

    disk::read_stub();

    debug("[+] Jumping to protected mode ...\n");

    protected_mode();

    loop {}
}

fn protected_mode() {
    unsafe {
        let tss_addr = (*(&raw mut GDT)).write_tss();
        (*(&raw mut GDT)).load();

        BOOT.rsdp = get_rsdp();
        BOOT.vbe = get_vbe_info();
        BOOT.tss = tss_addr;
        get_mmap();

        if MODE != 0 {
            let best_mode = find_vbe_mode();

            asm!(
            "int 0x10",
            in("ax") 0x4F02,
            in("bx") best_mode
            );
        }

        asm!("mov eax, cr0", "or eax, 1 << 0", "mov cr0, eax",);

        asm!("mov bx, {0:x}", in(reg) addr_of!(BOOT) as u16);

        asm!("ljmp $0x8, ${}", const NEXT_STAGE_RAM, options(att_syntax));
    }
}



#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    debug("[x] Bootloader stage 2 panicked! x_x");
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn fail() -> ! {
    panic!("[x] Disk failed!");
}