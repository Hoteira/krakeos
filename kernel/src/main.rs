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
pub mod window_manager;

extern crate alloc;

use crate::boot::{BootInfo, BOOT_INFO};

use crate::fs::ext2::fs::Ext2; 
use core::arch::asm;
use window_manager::display::DISPLAY_SERVER;
use crate::memory::pmm;


const EFER_MSR: u32 = 0xC0000080; 
const STAR_MSR: u32 = 0xC0000081; 
const LSTAR_MSR: u32 = 0xC0000082; 
const SFMASK_MSR: u32 = 0xC0000084; 
const PAT_MSR: u32 = 0x277;       

#[unsafe(no_mangle)]
#[unsafe(link_section = ".start")]
pub extern "C" fn _start(bootinfo_ptr: *const BootInfo) -> ! {
    unsafe {
        asm!("cli" );
    }

    unsafe {
        *(&raw mut BOOT_INFO) = bootinfo_ptr.read();
    };

    
    load_idt();

    
    memory::init();

    
    let heap_size = 0xA0_0000; 
    let heap_pages = heap_size / 4096;
    let heap_phys_addr = pmm::allocate_frames(heap_pages, 0 )
        .expect("Failed to allocate heap memory from PMM");

    std::memory::heap::init_heap(heap_phys_addr as *mut u8, heap_size);

    fs::dma::init();
    crate::fs::virtio::init();

    crate::fs::vfs::init();

    window_manager::events::GLOBAL_EVENT_QUEUE.lock().init();

    tss::init_ists();

    interrupts::task::TASK_MANAGER.int_lock().init();



    unsafe { (*(&raw mut DISPLAY_SERVER)).init(); }

    let first_user_task = interrupts::task::TASK_MANAGER.int_lock()
        .tasks.iter()
        .find(|t| t.state == interrupts::task::TaskState::Ready && t.kernel_stack != 0)
        .map(|t| t.kernel_stack);

    if let Some(kstack) = first_user_task {
        tss::set_tss(kstack);
    }

    drivers::periferics::mouse::init_mouse();
    drivers::periferics::timer::init_pit(100);

    crate::debugln!("Mounting Ext2...");
    match Ext2::new(0xE0, 16384) {
        Ok(fs) => crate::fs::vfs::mount(0xE0, fs),
        Err(e) => {
            crate::debugln!("Failed to mount Ext2: {}", e);
            loop {}
        }
    }

    crate::debugln!("Opening user file...");
    let mut node = match crate::fs::vfs::open(0xE0, "user.elf") {
        Ok(node) => {
            crate::debugln!("File opened! Size: {}", node.size());
            node
        },
        Err(e) => {
            crate::debugln!("Failed to open file: {}", e);
            loop {}
        }
    };

    let size = node.size() as usize;
    let pages = (size + 4095) / 4096;
    let phys_addr = memory::pmm::allocate_frames(pages, 0).expect("Failed to allocate PMM for ELF");
    crate::debugln!("ELF Buffer allocated at PMM: {:#x} - {:#x}", phys_addr, phys_addr + size as u64);

    unsafe {

        let buf = core::slice::from_raw_parts_mut(phys_addr as *mut u8, size);

        let n = node.read(0, buf).unwrap();



        let pml4 = memory::vmm::new_user_pml4();

        
        let pid_idx = interrupts::task::TASK_MANAGER.lock().reserve_pid().expect("Failed to reserve PID for init");
        let pid = pid_idx as u64;

        match crate::fs::elf::load_elf(&buf[0..n], pml4, pid) {

            Ok(entry) => {

                crate::debugln!("ELF loaded successfully. Entry: {:#x}", entry);

                memory::pmm::free_frame(phys_addr);

                if let Err(_) = interrupts::task::TASK_MANAGER.int_lock().init_user_task(pid_idx, entry, pml4, None, None) {

                    panic!("Failed to spawn first user task");

                }

            },

            Err(e) => {

                crate::debugln!("Failed to load ELF: {}", e);

                loop {}

            }

        }

    }

    init_syscall_msrs(); 

    crate::debugln!("Kernel initialized, entering idle loop...");

    unsafe { asm!("sti"); }

    loop {}

}



fn init_pat() {

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





unsafe fn rdmsr(msr: u32) -> u64 {

    let (low, high): (u32, u32);

    unsafe { asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high) };

    ((high as u64) << 32) | (low as u64)

}





unsafe fn wrmsr(msr: u32, value: u64) {

    let low = value as u32;

    let high = (value >> 32) as u32;

    unsafe { asm!("wrmsr", in("ecx") msr, in("eax") low, in("edx") high) };

}



fn init_syscall_msrs() {

    unsafe {

        

        let mut efer = rdmsr(EFER_MSR);

        efer |= 1;

        wrmsr(EFER_MSR, efer);



        let sysret_cs_base = 0x20;



        let syscall_cs_base = 0x8;



        let star_value = ((sysret_cs_base as u64) << 48) | ((syscall_cs_base as u64) << 32);

        wrmsr(STAR_MSR, star_value);

        wrmsr(LSTAR_MSR, interrupts::syscalls::syscall_entry as u64);



        let rflags_mask = (1 << 9) | (1 << 8); 

        wrmsr(SFMASK_MSR, rflags_mask);

    }

}





pub fn load_idt() {

    unsafe {

        (*(&raw mut interrupts::idt::IDT)).init();



        (*(&raw mut interrupts::idt::IDT)).processor_exceptions();

        (*(&raw mut interrupts::idt::IDT)).hardware_interrupts();



        



        (*(&raw mut interrupts::idt::IDT)).load();

        (*(&raw mut interrupts::pic::PICS)).init();

    }

}



#[panic_handler]

fn panic(info: &core::panic::PanicInfo) -> ! {

    crate::debugln!("KERNEL PANIC: {}", info);

    loop {}

}
