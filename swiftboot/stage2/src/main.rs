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

use crate::debug::debug;
use crate::mmap::{get_mmap, MemoryMap};
use crate::rsdp::{get_rsdp, Rsdp};
use crate::vbe::{find_vbe_mode, get_vbe_info, VbeInfoBlock, VbeModeInfoBlock};
use core::arch::asm;
use core::panic::PanicInfo;


static mut BOOT: BootInfo = unsafe { core::mem::zeroed() };
static mut VBE_MODE: VbeModeInfoBlock = unsafe { core::mem::zeroed() };

pub const STAGE3_RAM: u16 = 0xFE00;
pub const STAGE3_LBA: u64 = 3072;

pub const STAGE4_RAM_SEG: u16 = 0x1000;
pub const STAGE4_RAM_OFF: u16 = 0x7E00;
pub const STAGE4_LBA: u64 = 5120;

pub const KERNEL_LBA: u64 = 6144;
pub const KERNEL_TARGET: u32 = 0x100000;

pub const MAX_BPP: u8 = 32;
pub const MIN_BPP: u8 = 32;

pub const MAX_WIDTH: u16 = 1280;
pub const MIN_WIDTH: u16 = 1280;
pub const MAX_HEIGHT: u16 = 720;
pub const MIN_HEIGHT: u16 = 720;

pub const MODE: u16 = 0x1;


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

    disk::read(0, STAGE3_RAM, STAGE3_LBA, 32);

    debug("[+] Loading stage 4 ...\n");

    disk::read(STAGE4_RAM_SEG, STAGE4_RAM_OFF, STAGE4_LBA, 32);

    debug("[+] Loading kernel ...\n");
    load_kernel();

    debug("[+] Jumping to protected mode ...\n");

    protected_mode();

    loop {}
}

fn load_kernel() {
    let sectors_per_chunk = 64;
    let bytes_per_chunk = sectors_per_chunk * 512;
    let total_sectors = 16384;
    let chunks = total_sectors / sectors_per_chunk;

    for i in 0..chunks {
        let lba = KERNEL_LBA + (i as u64 * sectors_per_chunk as u64);
        let target = KERNEL_TARGET + (i as u32 * bytes_per_chunk as u32);


        disk::read(0x8000, 0x0000, lba, sectors_per_chunk as u16);

        move_memory_block(0x80000, target, 0x4000);
    }
}

#[repr(C, packed)]
struct MoveGdt {
    null1: u64,
    null2: u64,
    source: u64,
    dest: u64,
    null3: u64,
    null4: u64,
}

fn move_memory_block(source: u32, dest: u32, words: u16) {
    let gdt = MoveGdt {
        null1: 0,
        null2: 0,
        source: make_desc(source, 0xFFFF),
        dest: make_desc(dest, 0xFFFF),
        null3: 0,
        null4: 0,
    };

    unsafe {
        asm!(
        "push es",
        "mov ax, ds",
        "mov es, ax",
        "mov {2:x}, si",
        "mov si, {0:x}",
        "mov cx, {1:x}",
        "mov ah, 0x87",
        "int 0x15",
        "mov si, {2:x}",
        "pop es",
        in(reg) &gdt as *const MoveGdt as u16,
        in(reg) words,
        out(reg) _,
        out("ax") _,
        out("cx") _,
        );
    }
}

fn make_desc(base: u32, limit: u16) -> u64 {
    let base = base as u64;
    let limit = limit as u64;
    let access = 0x93u64;

    let mut desc = limit & 0xFFFF;
    desc |= (base & 0xFFFFFF) << 16;
    desc |= access << 40;
    desc |= ((limit >> 16) & 0xF) << 48;
    desc |= ((base >> 24) & 0xFF) << 56;

    desc
}

fn protected_mode() {
    unsafe {
        let tss_addr = (*(&raw mut GDT)).write_tss();
        (*(&raw mut GDT)).load();


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

        asm!("mov eax, cr0", "or eax, 1 << 0", "mov cr0, eax", );

        asm!("mov bx, {0:x}", in(reg) addr_of!(BOOT) as u16);

        asm!("ljmp $0x8, ${}", const STAGE3_RAM, options(att_syntax));
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