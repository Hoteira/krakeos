use super::consts::*;
use super::structs::*;
use crate::debugln;
use crate::memory::mmio::{read_16, write_16, write_64};
use crate::memory::pmm;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, Ordering};

pub struct VirtQueue {
    pub desc_phys: u64,
    pub avail_phys: u64,
    pub used_phys: u64,
    pub queue_index: u16,
    pub num: u16,
    pub free_head: u16,
    pub last_used_idx: u16,
    pub last_avail_idx: u16,
    pub notify_addr: u64,
    pub virt_base: u64,
}

pub static mut VIRT_QUEUES: [Option<VirtQueue>; 2] = [None, None];
// Split locks to prevent deadlock between Screen Flush (Q0) and Mouse Move (Q1)
static QUEUE_LOCKS: [AtomicBool; 2] = [AtomicBool::new(false), AtomicBool::new(false)];

pub fn setup_queue(common_cfg: *mut u8, index: u16, notify_base: u64, notify_multiplier: u32) {
    unsafe {
        write_16(common_cfg.add(OFF_QUEUE_SELECT), index);
        let max_size = read_16(common_cfg.add(OFF_QUEUE_SIZE));
        if max_size == 0 { return; }
        let size: u16 = max_size.min(128);
        write_16(common_cfg.add(OFF_QUEUE_SIZE), size);
        if let Some(frame) = pmm::allocate_frame() {
            // Map queue structures as NO_CACHE to ensure the device sees our updates immediately.
            let virt_addr = crate::memory::vmm::map_mmio(frame, 4096);
            let virt_ptr = virt_addr as *mut u8;
            core::ptr::write_bytes(virt_ptr, 0, 4096);
            
            let desc_addr = frame;
            let avail_addr = desc_addr + 2048;
            let used_addr = (avail_addr + 262 + 3) & !3;
            
            let avail_ptr = (virt_addr + 2048) as *mut VirtqAvail;
            write_volatile(core::ptr::addr_of_mut!((*avail_ptr).flags), 1); // VIRTQ_AVAIL_F_NO_INTERRUPT
            
            write_64(common_cfg.add(OFF_QUEUE_DESC), desc_addr);
            write_64(common_cfg.add(OFF_QUEUE_DRIVER), avail_addr);
            write_64(common_cfg.add(OFF_QUEUE_DEVICE), used_addr);
            
            let notify_off = read_16(common_cfg.add(OFF_QUEUE_NOTIFY_OFF));
            let notify_addr = notify_base + (notify_off as u64 * notify_multiplier as u64);
            write_16(common_cfg.add(OFF_QUEUE_ENABLE), 1);
            
            VIRT_QUEUES[index as usize] = Some(VirtQueue {
                desc_phys: desc_addr,
                avail_phys: avail_addr,
                used_phys: used_addr,
                queue_index: index,
                num: size,
                free_head: 0,
                last_used_idx: 0,
                last_avail_idx: 0,
                notify_addr,
                virt_base: virt_addr,
            });
        }
    }
}

pub fn send_command_queue(queue_idx: usize, out_phys: &[u64], out_lens: &[u32], in_phys: &[u64], in_lens: &[u32], wait: bool) -> bool {
    if queue_idx >= 2 { return false; }
    
    let int_enabled = crate::arch::x86_64::idt::interrupts();
    if int_enabled { unsafe { core::arch::asm!("cli"); } }

    while QUEUE_LOCKS[queue_idx].compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        if int_enabled {
            unsafe {
                core::arch::asm!("sti");
                core::arch::asm!("int 0x81");
                core::arch::asm!("cli");
            }
        } else {
            core::hint::spin_loop();
        }
    }
    
    let result = unsafe { send_command_queue_unlocked(queue_idx, out_phys, out_lens, in_phys, in_lens, wait, int_enabled) };
    
    QUEUE_LOCKS[queue_idx].store(false, Ordering::Release);
    
    if int_enabled { unsafe { core::arch::asm!("sti"); } }
    
    result
}

unsafe fn send_command_queue_unlocked(queue_idx: usize, out_phys: &[u64], out_lens: &[u32], in_phys: &[u64], in_lens: &[u32], wait: bool, int_enabled: bool) -> bool {
    let vq = match &mut VIRT_QUEUES[queue_idx] {
        Some(v) => v,
        None => { return false; }
    };
    let total_descs = out_phys.len() + in_phys.len();
    let mut timeout: u64 = 500_000_000;
    
    // Safety check for queue capacity (conservative chain limit)
    while vq.last_avail_idx.wrapping_sub(vq.last_used_idx) >= (vq.num / 4) {
        let used_ptr = (vq.virt_base + (vq.used_phys - vq.desc_phys)) as *mut VirtqUsed;
        vq.last_used_idx = read_volatile(core::ptr::addr_of!((*used_ptr).idx));
        if int_enabled {
            unsafe {
                core::arch::asm!("sti");
                core::arch::asm!("int 0x81");
                core::arch::asm!("cli");
            }
        } else {
            core::hint::spin_loop();
        }
        timeout -= 1;
        if timeout == 0 { return false; }
    }
    
    let virt_desc_base = vq.virt_base as *mut VirtqDesc;
    let mut curr = vq.free_head as usize;
    for i in 0..out_phys.len() {
        let is_last = i == out_phys.len() - 1 && in_phys.len() == 0;
        unsafe {
            core::ptr::write_volatile(virt_desc_base.add(curr), VirtqDesc {
                addr: out_phys[i], len: out_lens[i],
                flags: if is_last { 0 } else { 1 },
                next: if is_last { 0 } else { ((curr + 1) % vq.num as usize) as u16 },
            });
        }
        curr = (curr + 1) % vq.num as usize;
    }
    for i in 0..in_phys.len() {
        let is_last = i == in_phys.len() - 1;
        unsafe {
            core::ptr::write_volatile(virt_desc_base.add(curr), VirtqDesc {
                addr: in_phys[i], len: in_lens[i],
                flags: 2 | (if is_last { 0 } else { 1 }),
                next: if is_last { 0 } else { ((curr + 1) % vq.num as usize) as u16 },
            });
        }
        curr = (curr + 1) % vq.num as usize;
    }

    let avail_ptr = (vq.virt_base + 2048) as *mut VirtqAvail;
    let ring_idx = (read_volatile(core::ptr::addr_of!((*avail_ptr).idx)) % vq.num) as usize;
    write_volatile(core::ptr::addr_of_mut!((*avail_ptr).ring[ring_idx]), vq.free_head);
    
    // Ensure descriptors are visible before the index update
    unsafe { core::arch::asm!("sfence", options(nostack, preserves_flags)); }
    
    let new_idx = read_volatile(core::ptr::addr_of!((*avail_ptr).idx)).wrapping_add(1);
    write_volatile(core::ptr::addr_of_mut!((*avail_ptr).idx), new_idx);
    
    vq.last_avail_idx = vq.last_avail_idx.wrapping_add(1);
    
    // Ensure index is visible before notify
    unsafe { core::arch::asm!("sfence", options(nostack, preserves_flags)); }
    write_volatile(vq.notify_addr as *mut u16, vq.queue_index);
    
    vq.free_head = curr as u16;
    
    if !wait { return true; }
    
    let used_ptr = (vq.virt_base + (vq.used_phys - vq.desc_phys)) as *mut VirtqUsed;
    let mut success = false; timeout = 1_000_000_000;
    let target_idx = vq.last_avail_idx;
    
    loop {
        let used_idx = read_volatile(core::ptr::addr_of!((*used_ptr).idx));
        // Check if host has reached or passed our submitted chain count
        if used_idx.wrapping_sub(target_idx) as i16 >= 0 {
            unsafe { core::arch::asm!("lfence", options(nostack, preserves_flags)); }
            vq.last_used_idx = used_idx;
            success = true;
            break;
        }
        if int_enabled {
            unsafe {
                core::arch::asm!("sti");
                core::arch::asm!("int 0x81");
                core::arch::asm!("cli");
            }
        } else {
            core::hint::spin_loop();
        }
        timeout -= 1;
        if timeout == 0 {
            crate::debugln!("VirtIO GPU: Command timeout! queue={} target={} current={}", vq.queue_index, target_idx, used_idx);
            break;
        }
    }
    success
}
