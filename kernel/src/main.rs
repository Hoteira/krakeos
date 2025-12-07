#![feature(abi_x86_interrupt)]
#![no_std]
#![no_main]

pub mod boot;
mod interrupts;
mod drivers;
mod fs;
mod memory;
mod tss;
pub mod debug;
pub mod display;
pub mod composer;

extern crate alloc;

use crate::boot::{BootInfo, BOOT_INFO};
use crate::fs::vfs::FileSystem; 
use crate::fs::ext2::fs::Ext2; // Added Ext2 for font loading
use core::arch::asm;

// MSRs for SYSCALL/SYSRET
const EFER_MSR: u32 = 0xC0000080; // Extended Feature Enable Register
const STAR_MSR: u32 = 0xC0000081; // SYSCALL Target Address Register
const LSTAR_MSR: u32 = 0xC0000082; // Long Mode SYSCALL Target Address Register
const SFMASK_MSR: u32 = 0xC0000084; // SYSCALL Flag Mask

#[unsafe(no_mangle)]
#[unsafe(link_section = ".start")]
pub extern "C" fn _start(bootinfo_ptr: *const BootInfo) -> ! {
    unsafe {
        asm!("cli" );
    }

    unsafe { 
        *(&raw mut BOOT_INFO) = bootinfo_ptr.read();
    };

    memory::init();

    fs::dma::init();
    // Move heap to 48MB to avoid collision with PMM Bitmap (10MB-43MB)
    std::memory::heap::init_heap(0x03000000 as *mut u8, 0x100_0000);

    debugln!("[KERNEL] Hello World!");

    tss::init_ists();

    interrupts::task::TASK_MANAGER.lock().init();

    interrupts::task::TASK_MANAGER.lock().add_task(test_task as u64, None);

        unsafe { (*(&raw mut composer::DISPLAY_SERVER)).init(); }

    

        debugln!("[KERNEL] Setting initial TSS for user tasks");

        let first_user_task = interrupts::task::TASK_MANAGER.lock()

            .tasks.iter()

            .find(|t| t.state == interrupts::task::TaskState::Ready && t.kernel_stack != 0)

            .map(|t| t.kernel_stack);

    

        if let Some(kstack) = first_user_task {

            tss::set_tss(kstack);

            debugln!("[KERNEL] TSS RSP0 set to {:#x}", kstack);

        }

        

        debugln!("[KERNEL] Initializing Ext2...");

    

        match Ext2::new(0, 16384) {

            Ok(mut fs) => {

                debugln!("[EXT2] Superblock found!");

                debugln!(" - Magic: {:#x}", fs.superblock.magic + 0);

                

                match fs.root() {

                    Ok(mut root) => {

                         match root.find("user") {

                             Ok(mut user_file) => {

                                 let file_size = user_file.size();

                                 debugln!("[ELF] User file size: {} bytes", file_size);

                                 let mut elf_buf = alloc::vec![0u8; file_size as usize];

                                 if let Ok(bytes) = user_file.read(0, &mut elf_buf) {

                                     debugln!("[ELF] Read {} bytes from disk.", bytes);

                                     if bytes != file_size as usize {

                                         debugln!("[ELF] WARNING: Read bytes mismatch file size!");

                                     }

                                     

                                     unsafe {

                                         let user_pml4 = memory::vmm::new_user_pml4();

                                         match crate::fs::elf::load_elf(&elf_buf[0..bytes], user_pml4) {

                                             Ok(entry_point) => {

                                                 debugln!("[ELF] Loaded user program. Entry: {:#x}", entry_point);

                                                 interrupts::task::TASK_MANAGER.lock().add_user_task(entry_point, user_pml4, None);

                                             }

                                             Err(e) => debugln!("[ELF] Load failed: {}", e),

                                         }

                                     }

                                 }

                             }

                             Err(e) => debugln!("[EXT2] Failed to find user binary: {}", e),

                         }

                    }

                    Err(e) => debugln!("[EXT2] Root error: {}", e),

                }

            }

            Err(e) => debugln!("[EXT2] Failed to mount Disk 0 @ 16384: {}", e),

        }

    

        debugln!("[KERNEL] Starting Kernel Tasks...");

        load_idt();

        debugln!("[KERNEL] IDT loaded, interrupts enabled.");

        init_syscall_msrs(); // Initialize SYSCALL MSRs

        debugln!("[KERNEL] SYSCALL MSRs configured.");

    

        loop {}
}

fn test_task() {
    loop {
        debugln!(".");
        for _ in 0..10000000 { unsafe { core::arch::asm!("nop") } }
    }
}

/// Reads an MSR.
unsafe fn rdmsr(msr: u32) -> u64 {
    let (low, high): (u32, u32);
    unsafe { asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high) };
    ((high as u64) << 32) | (low as u64)
}

/// Writes an MSR.
unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    unsafe { asm!("wrmsr", in("ecx") msr, in("eax") low, in("edx") high) };
}

fn init_syscall_msrs() {
    unsafe {
        // 1. Enable SYSCALL/SYSRET in EFER MSR
        let mut efer = rdmsr(EFER_MSR);
        efer |= 1; // Set SCE (SYSCALL Enable) bit
        wrmsr(EFER_MSR, efer);
        debugln!("[SYSCALL] EFER MSR configured.");

        // 2. Configure STAR MSR
        // STAR MSR format:
        // Bits 63:48 -> SYSRET CS (user code segment, shifted left by 16)
        // Bits 47:32 -> SYSCALL CS (kernel code segment, shifted left by 16)
        
        // Swiftboot GDT:
        // 0x20: User Data
        // 0x28: Kernel Code 64
        // 0x30: User Code 64
        
        // Sysret Target: We want CS=0x30 (User Code).
        // SYSRET CS = Base + 16. -> Base = 0x30 - 16 = 0x20.
        // SYSRET SS = Base + 8  = 0x20 + 8  = 0x28 (Kernel Code 64). 
        // (Note: We don't use sysret, we use iretq, so the SS mismatch here is less critical for now, 
        // but if we did use sysret, we'd need a different GDT).
        let sysret_cs_base = 0x20;
        
        // Syscall Target: We want CS=0x28 (Kernel Code).
        // SYSCALL CS = Base. -> Base = 0x28.
        // SYSCALL SS = Base + 8 = 0x30 (User Code 64).
        // (Note: This loads User Code selector into Kernel SS. This is invalid but often ignored in 64-bit 
        // mode or we switch stack immediately anyway).
        let syscall_cs_base = 0x28;

        let star_value = ((sysret_cs_base as u64) << 48) | ((syscall_cs_base as u64) << 32);
        wrmsr(STAR_MSR, star_value);
        debugln!("[SYSCALL] STAR MSR configured. Kernel Base: {:#x}, User Base: {:#x}", syscall_cs_base, sysret_cs_base);

        // 3. Configure LSTAR MSR (Long Mode SYSCALL Target Address)
        // This points to our syscall_entry function.
        wrmsr(LSTAR_MSR, interrupts::syscalls::syscall_entry as u64);
        debugln!("[SYSCALL] LSTAR MSR configured to syscall_entry at {:#x}", interrupts::syscalls::syscall_entry as u64);

        // 4. Configure SFMASK MSR (SYSCALL Flag Mask)
        // Bits in RFLAGS corresponding to set bits in SFMASK are cleared on SYSCALL entry.
        // We want to clear the Interrupt Flag (IF) (bit 9) and Trap Flag (TF) (bit 8)
        // to prevent interrupts/traps immediately after entering the kernel.
        // Also clear other flags that user shouldn't control in kernel mode.
        let rflags_mask = (1 << 9) | (1 << 8); // IF and TF
        wrmsr(SFMASK_MSR, rflags_mask);
        debugln!("[SYSCALL] SFMASK MSR configured with mask {:#x}", rflags_mask);
    }
}


pub fn load_idt() {
    unsafe {
        (*(&raw mut interrupts::idt::IDT)).init();

        (*(&raw mut interrupts::idt::IDT)).processor_exceptions();
        (*(&raw mut interrupts::idt::IDT)).hardware_interrupts();

        // 3. Load the IDT into the CPU

        (*(&raw mut interrupts::idt::IDT)).load();
        (*(&raw mut interrupts::pic::PICS)).init();

        core::arch::asm!("sti"); // Interrupts disabled for stability during mounting
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    debugln!("[KERNEL_PANIC]\n{}", info);
    loop {}
}