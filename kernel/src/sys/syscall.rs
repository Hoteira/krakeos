use crate::arch::trap::TrapFrame;

pub static mut USER_BRK: usize = 0;

pub const SYS_EXIT: usize = 1;
pub const SYS_READ: usize = 2;
pub const SYS_WRITE: usize = 3;
pub const SYS_OPEN: usize = 4;
pub const SYS_SBRK: usize = 5;
pub const SYS_FB_FLUSH: usize = 10;
pub const SYS_WAIT_FS_EVENT: usize = 11;
pub const SYS_SPAWN: usize = 12;

pub fn dispatch(frame: &mut TrapFrame) {
    let id = frame.regs[17]; // a7
    let a0 = frame.regs[10];
    let a1 = frame.regs[11];
    let a2 = frame.regs[12];
    let a5 = frame.regs[15];
    
    // Enable Supervisor User Memory (SUM) access (bit 18 of sstatus)
    unsafe {
        core::arch::asm!("csrs sstatus, {}", in(reg) 1 << 18);
    }

    match id {
        SYS_EXIT => {
            crate::println!("Process exited with code: {}", a0);
            
            // If it's the last thread, shutdown, otherwise mark as Empty and yield
            unsafe {
                let current_tid = crate::sys::scheduler::SCHEDULER.current;
                crate::sys::scheduler::SCHEDULER.threads[current_tid].state = crate::sys::scheduler::ThreadState::Empty;
                
                let mut has_ready = false;
                for i in 0..8 {
                    if crate::sys::scheduler::SCHEDULER.threads[i].state != crate::sys::scheduler::ThreadState::Empty {
                        has_ready = true;
                        break;
                    }
                }
                
                if !has_ready {
                    crate::arch::sbi::shutdown();
                } else {
                    // We must wait for the next timer interrupt to context switch.
                    // Or we could trigger an explicit context switch here, but waiting is simpler for now.
                    // Just infinite loop in kernel until timer fires and context switches!
                    loop {
                        core::arch::asm!("wfi");
                    }
                }
            }
        }
        SYS_READ => {
            let fd = a0;
            let ptr = a1 as *mut u8;
            let len = a2;
            let offset = a5;
            
            // Validate ptr is in user space (basic check)
            if ptr as usize >= 0x8000_0000 {
                frame.regs[10] = 0; // Error / read 0
            } else {
                let buf = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
                
                let bytes_read = if fd > 0 && fd < 1024 {
                    crate::fs::read_file(fd, offset, buf)
                } else if fd >= 1024 {
                    crate::fs::ramfs::read_file(fd, offset, buf)
                } else {
                    0
                };
                
                frame.regs[10] = bytes_read;
            }
        }
        SYS_WRITE => {
            let fd = a0;
            let ptr = a1 as *const u8;
            let len = a2;
            let offset = a5;
            
            if ptr as usize >= 0x8000_0000 {
                frame.regs[10] = 0;
            } else {
                let buf = unsafe { core::slice::from_raw_parts(ptr, len) };
                
                if fd == 1 || fd == 2 {
                    let mut uart = crate::uart::Uart::new(0x1000_0000);
                    for &b in buf {
                        uart.put(b);
                    }
                    frame.regs[10] = len;
                } else {
                    // write to file
                    let bw = if fd >= 1024 {
                        crate::fs::ramfs::write_file(fd, offset, buf)
                    } else {
                        crate::fs::write_file(fd, offset, buf)
                    };
                    frame.regs[10] = bw;
                }
            }
        }
        SYS_OPEN => {
            let ptr = a0 as *const u8;
            let len = a1;
            let flags = a2;
            
            if ptr as usize >= 0x8000_0000 {
                frame.regs[10] = usize::MAX; // Error
            } else {
                let buf = unsafe { core::slice::from_raw_parts(ptr, len) };
                if let Ok(path) = core::str::from_utf8(buf) {
                    if let Some(desc_idx) = crate::fs::find_file(path) {
                        frame.regs[10] = desc_idx;
                    } else if (flags & 1) != 0 {
                        if let Some(desc_idx) = crate::fs::create_file(path) {
                            frame.regs[10] = desc_idx;
                        } else {
                            frame.regs[10] = usize::MAX;
                        }
                    } else {
                        frame.regs[10] = usize::MAX; // Error
                    }
                } else {
                    frame.regs[10] = usize::MAX; // Error
                }
            }
        }
        SYS_SBRK => {
            let increment = a0;
            unsafe {
                let old_brk = USER_BRK;
                if increment == 0 {
                    frame.regs[10] = old_brk;
                } else {
                    let mut mapped_vaddr = (old_brk + 4095) & !4095;
                    let required_vaddr = old_brk + increment;
                    let required_mapped = (required_vaddr + 4095) & !4095;
                    
                    let layout = core::alloc::Layout::from_size_align(4096, 4096).unwrap();
                    let mut oom = false;
                    while mapped_vaddr < required_mapped {
                        let phys_addr = alloc::alloc::alloc_zeroed(layout) as usize;
                        if phys_addr == 0 {
                            crate::println!("OUT OF MEMORY during sys_sbrk!");
                            oom = true;
                            break;
                        }
                        crate::arch::paging::map_page(
                            crate::ROOT_PAGE_TABLE, 
                            mapped_vaddr, 
                            phys_addr, 
                            crate::arch::paging::PTE_R | crate::arch::paging::PTE_W | crate::arch::paging::PTE_X | crate::arch::paging::PTE_U
                        );
                        mapped_vaddr += 4096;
                    }
                    if !oom {
                        USER_BRK = required_vaddr;
                        crate::csr::sfence_vma();
                        frame.regs[10] = old_brk;
                    } else {
                        frame.regs[10] = usize::MAX;
                    }
                }
            }
        }
        SYS_FB_FLUSH => {
            let buf_ptr = a0 as *const u32;
            let len = a1;
            crate::println!("SYS_FB_FLUSH: buf_ptr = {:p}, len = {}", buf_ptr, len);
            if len <= (crate::drivers::virtio_gpu::FB_WIDTH * crate::drivers::virtio_gpu::FB_HEIGHT) as usize {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        buf_ptr,
                        core::ptr::addr_of_mut!(crate::drivers::virtio_gpu::FB_MEM.0) as *mut u32,
                        len
                    );
                }
                crate::drivers::virtio_gpu::flush();
                frame.regs[10] = 0;
            } else {
                frame.regs[10] = 1; // Error
            }
        }
        SYS_WAIT_FS_EVENT => {
            let desc_idx = a0;
            let tid = unsafe { crate::sys::scheduler::SCHEDULER.current };
            if desc_idx >= crate::fs::ramfs::RAMFS_DESC_OFFSET {
                if crate::fs::ramfs::wait_for_event(desc_idx, tid) {
                    unsafe {
                        crate::sys::scheduler::SCHEDULER.threads[tid].state = crate::sys::scheduler::ThreadState::Waiting;
                    }
                    frame.regs[10] = 0;
                    
                    // We need to yield! We can't call scheduler::switch easily from here,
                    // so we enable S-mode interrupts and wait for the next timer tick to context switch us out.
                    unsafe {
                        core::arch::asm!("csrs sstatus, 2"); // Enable SIE
                        loop {
                            if crate::sys::scheduler::SCHEDULER.threads[tid].state != crate::sys::scheduler::ThreadState::Waiting {
                                break;
                            }
                            core::arch::asm!("wfi");
                        }
                        core::arch::asm!("csrc sstatus, 2"); // Disable SIE again
                    }
                    return;
                } else {
                    frame.regs[10] = 1; // Error
                }
            } else {
                frame.regs[10] = 1; // Error, not a ram file
            }
        }
        SYS_SPAWN => {
            let entry = a0;
            let arg = a1;
            crate::sys::scheduler::Scheduler::spawn_user_thread(entry, arg);
            frame.regs[10] = 0;
        }
        _ => {
            crate::println!("Unknown syscall: {}", id);
            frame.regs[10] = usize::MAX;
        }
    }
    
    // Disable SUM access
    unsafe {
        core::arch::asm!("csrc sstatus, {}", in(reg) 1 << 18);
    }
}
