use crate::arch::paging::{map_page, PTE_R, PTE_W, PTE_X, PTE_U};
use crate::arch::trap::TrapFrame;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct Elf64Ehdr {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct Elf64Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

pub fn load_elf_and_spawn(elf_data: &[u8]) {
    if elf_data.len() < core::mem::size_of::<Elf64Ehdr>() {
        crate::println!("Invalid ELF: too small");
        return;
    }
    
    let header = unsafe { &*(elf_data.as_ptr() as *const Elf64Ehdr) };
    
    if &header.e_ident[0..4] != b"\x7fELF" {
        crate::println!("Invalid ELF magic");
        return;
    }
    
    crate::println!("ELF entry point: {:#x}", header.e_entry);
    
    let phoff = header.e_phoff as usize;
    let phnum = header.e_phnum as usize;
    let phentsize = header.e_phentsize as usize;
    
    for i in 0..phnum {
        let offset = phoff + i * phentsize;
        let phdr = unsafe { &*(elf_data.as_ptr().add(offset) as *const Elf64Phdr) };
        
        if phdr.p_type == 1 { // PT_LOAD
            crate::println!("Loading segment: vaddr={:#x}, memsz={:#x}, filesz={:#x}", phdr.p_vaddr, phdr.p_memsz, phdr.p_filesz);
            
            let num_pages = (phdr.p_memsz as usize + 4095) / 4096;
            let layout = core::alloc::Layout::from_size_align(4096, 4096).unwrap();
            
            for page in 0..num_pages {
                let v = phdr.p_vaddr as usize + page * 4096;
                let phys_addr = unsafe { alloc::alloc::alloc_zeroed(layout) } as usize;
                
                if phys_addr == 0 {
                    crate::println!("  -> OUT OF MEMORY during ELF load!");
                    loop {}
                }
                
                // Copy data if within filesz
                let page_offset = page * 4096;
                if page_offset < phdr.p_filesz as usize {
                    let mut copy_len = 4096;
                    if page_offset + 4096 > phdr.p_filesz as usize {
                        copy_len = phdr.p_filesz as usize - page_offset;
                    }
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            elf_data.as_ptr().add(phdr.p_offset as usize + page_offset),
                            phys_addr as *mut u8,
                            copy_len,
                        );
                    }
                }
                
                // Map the virtual address to this physical page with PTE_U
                unsafe {
                    map_page(crate::ROOT_PAGE_TABLE, v, phys_addr, PTE_R | PTE_W | PTE_X | PTE_U);
                    
                    let end_v = v + 4096;
                    if end_v > crate::sys::syscall::USER_BRK {
                        crate::sys::syscall::USER_BRK = end_v;
                    }
                }
            }
        }
    }
    
    crate::csr::sfence_vma();
    crate::csr::fence_i();

    // Allocate User Stack
    let stack_size = 1024 * 1024; // 1MB user stack
    let layout = core::alloc::Layout::from_size_align(stack_size, 4096).unwrap();
    let phys_stack = unsafe { alloc::alloc::alloc_zeroed(layout) } as usize;
    crate::println!("User stack phys_addr={:#x}", phys_stack);
    
    // Map User Stack at a fixed high address, say 0x4000_0000
    let v_stack_base = 0x5000_0000;
    for page in 0..(stack_size / 4096) {
        unsafe {
            map_page(crate::ROOT_PAGE_TABLE, v_stack_base + page * 4096, phys_stack + page * 4096, PTE_R | PTE_W | PTE_U);
        }
    }
    
    crate::csr::sfence_vma();

    crate::println!("User stack mapped at {:#x}", v_stack_base);
    
    // Prepare kernel stack for the trap frame to return to U-mode
    let k_stack_size = 4096 * 4;
    let k_layout = core::alloc::Layout::from_size_align(k_stack_size, 4096).unwrap();
    let k_stack = unsafe { alloc::alloc::alloc_zeroed(k_layout) } as usize;
    
    let trap_frame_ptr = (k_stack + k_stack_size - core::mem::size_of::<TrapFrame>()) as *mut TrapFrame;
    unsafe {
        (*trap_frame_ptr).sepc = header.e_entry as usize;
        // SSTATUS: SPIE=1 (5), SPP=0 (8)
        (*trap_frame_ptr).sstatus = 1 << 5; // SPP is 0 for U-mode
        (*trap_frame_ptr).regs[2] = v_stack_base + stack_size; // User SP
    }
    
    crate::println!("Spawning U-Mode thread...");
    
    // Add to scheduler
    unsafe {
        for i in 0..crate::sys::scheduler::MAX_THREADS {
            if crate::sys::scheduler::SCHEDULER.threads[i].state == crate::sys::scheduler::ThreadState::Empty {
                crate::sys::scheduler::SCHEDULER.threads[i].sp = trap_frame_ptr as usize;
                crate::sys::scheduler::SCHEDULER.threads[i].state = crate::sys::scheduler::ThreadState::Ready;
                return;
            }
        }
    }
}
