#![no_std]
#![no_main]

mod disk;
mod debug;
mod boot;
mod paging;
mod gdt;
mod tss;

use core::arch::asm;
use crate::debug::debug;

use core::panic::PanicInfo;
use core::ptr::addr_of;
use crate::boot::BootInfo;
use crate::gdt::GDT;

pub const NEXT_STAGE_RAM: u64 = 0x1_7e00;
pub const NEXT_STAGE_LBA: u64 = 5120;
pub const KERNEL_RAM: u32 = 0x10_0000;
pub const KERNEL_LBA: u64 = 1644;


const STACK_ADDRESS: u64 = 0x200_0000;
const BOOT_MODE: u8 = 64; //32 or 64 bits

#[unsafe(no_mangle)]
#[unsafe(link_section = ".start")]
pub extern "C" fn _start() -> ! {
    let ebx: u32;

    unsafe {
        asm!(
            "mov {0:e}, 0x10",
            "mov ds, {0:e}",
            "mov es, {0:e}",
            "mov ss, {0:e}",

            "mov esp, {1:e}",

            out(reg) _,
            in(reg) STACK_ADDRESS as u32,
            out("ebx") ebx,

            options(nostack),
        );
    }

    let mut bootinfo = ebx as *mut BootInfo;
    unsafe {
        (*bootinfo).kernel_stack = STACK_ADDRESS;
        (*bootinfo).pml4 = 0x2_0000;
    }

    if BOOT_MODE == 32 {
        debug("[+] Jumping to kernel ...\n");

        disk::read(KERNEL_LBA, 2048, KERNEL_RAM as *mut u8);

        unsafe {
            asm!(
                "push {1:e}",
                "call {0:e}",
                in(reg) KERNEL_RAM,
                in(reg) ebx,
                options(nostack),
            );
        }
    } else if BOOT_MODE == 64 {
        debug("[+] Jumping to long mode ...\n");

        disk::read(NEXT_STAGE_LBA, 1024, NEXT_STAGE_RAM as *mut u8);

        unsafe {
            let fb = (*bootinfo).mode.framebuffer;
            let w = (*bootinfo).mode.width as u64;
            let h = (*bootinfo).mode.height as u64;
            let p = (*bootinfo).mode.pitch as u64;
            let size = h * p;
            
            debug("[+] FB Addr: ");
            crate::debug::print_hex(fb as u64);
            debug("\n[+] FB Size: ");
            crate::debug::print_hex(size);
            debug("\n");
            
            paging::setup_paging(fb as u64, size);
        }

        unsafe {

            asm!(
                "mov cr3, {0:e}",
                in(reg) 0x2_0000,
            );

            // Enable PAE (CR4.PAE = 1) + SSE (OSFXSR = 1, OSXMMEXCPT = 1)
            asm!(
                "mov eax, cr4",
                "or eax, 0x620", // (1 << 5) | (1 << 9) | (1 << 10)
                "mov cr4, eax",
            );

            // Set LME bit in EFER MSR
            asm!(
                "mov ecx, 0xC0000080",
                "rdmsr",
                "or eax, 1 << 8",
                "wrmsr",
            );

            // Enable paging, Set MP, Clear EM
            asm!(
                "mov eax, cr0",
                "and eax, 0xFFFFFFFB", // Clear EM (bit 2)
                "or eax, 0x80000002",  // Set PG (bit 31) | MP (bit 1)
                "mov cr0, eax",
            );

            (*(&raw mut GDT)).write_tss();
            (*(&raw mut GDT)).load();
            (*(&raw mut GDT)).load_tss();

            asm!("mov edi, {0:e}", in(reg) ebx);
            asm!("ljmp $0x28, ${}", const NEXT_STAGE_RAM, options(att_syntax));
        }
    }

    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    debug("[x] Bootloader panicked at stage 3!");
    loop {}
}