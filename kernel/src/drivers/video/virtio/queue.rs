use super::structs::*;
use super::consts::*;
use super::util::*;
use crate::memory::pmm;
use core::ptr::{read_volatile, write_volatile};
use crate::println;

pub struct VirtQueue {
    pub desc_phys: u64,
    pub avail_phys: u64,
    pub used_phys: u64,
    pub queue_index: u16,
    pub num: u16,
    pub free_head: u16,
    pub last_used_idx: u16,
    pub notify_addr: u64,
}

pub static mut VIRT_QUEUES: [Option<VirtQueue>; 2] = [None, None];

pub unsafe fn setup_queue(common_cfg: *mut u8, index: u16, notify_base: u64, notify_multiplier: u32) {
    write_common_u16(common_cfg, OFF_QUEUE_SELECT, index);
    
    let max_size = read_common_u16(common_cfg, OFF_QUEUE_SIZE);
    if max_size == 0 { return; }
    
    let size: u16 = 32;
    write_common_u16(common_cfg, OFF_QUEUE_SIZE, size);

    if let Some(frame) = pmm::allocate_frame(0) {
        core::ptr::write_bytes(frame as *mut u8, 0, 4096);
        
        let desc_addr = frame;
        let avail_addr = desc_addr + 512;
        let used_addr = (avail_addr + 70 + 3) & !3;

        // Disable Interrupts (Polling Mode)
        let avail_ptr = avail_addr as *mut VirtqAvail;
        (*avail_ptr).flags = 1;

        write_common_u64(common_cfg, OFF_QUEUE_DESC, desc_addr);
        write_common_u64(common_cfg, OFF_QUEUE_DRIVER, avail_addr);
        write_common_u64(common_cfg, OFF_QUEUE_DEVICE, used_addr);

        let notify_off = read_common_u16(common_cfg, OFF_QUEUE_NOTIFY_OFF);
        let notify_addr = notify_base + (notify_off as u64 * notify_multiplier as u64);
        
        write_common_u16(common_cfg, OFF_QUEUE_ENABLE, 1);
        
        VIRT_QUEUES[index as usize] = Some(VirtQueue {
            desc_phys: desc_addr,
            avail_phys: avail_addr,
            used_phys: used_addr,
            queue_index: index,
            num: size,
            free_head: 0,
            last_used_idx: 0,
            notify_addr,
        });
        println!("VirtIO GPU: Queue {} setup at phys {:#x}. Notify: {:#x}", index, frame, notify_addr);
    }
}

pub unsafe fn send_command_simple(req_phys: u64, req_len: u32, resp_phys: u64, resp_len: u32) {
    let vq = match &mut VIRT_QUEUES[0] { 
        Some(v) => v,
        None => return,
    };

    let head_idx = vq.free_head % vq.num;
    let next_idx = (vq.free_head + 1) % vq.num;
    
    vq.free_head = vq.free_head.wrapping_add(2);

    let desc_ptr = vq.desc_phys as *mut VirtqDesc;
    
    (*desc_ptr.add(head_idx as usize)) = VirtqDesc {
        addr: req_phys,
        len: req_len,
        flags: 1, // NEXT
        next: next_idx,
    };

    (*desc_ptr.add(next_idx as usize)) = VirtqDesc {
        addr: resp_phys,
        len: resp_len,
        flags: 2, // WRITE
        next: 0,
    };

    let avail_ptr = vq.avail_phys as *mut VirtqAvail;
    let idx = (*avail_ptr).idx;
    (*avail_ptr).ring[(idx % vq.num) as usize] = head_idx;
    
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
    (*avail_ptr).idx = idx.wrapping_add(1);

    write_volatile(vq.notify_addr as *mut u16, vq.queue_index);

    let used_ptr = vq.used_phys as *mut VirtqUsed;
    loop {
        let used_idx = read_volatile(core::ptr::addr_of!((*used_ptr).idx));
        if used_idx != vq.last_used_idx {
            vq.last_used_idx = vq.last_used_idx.wrapping_add(1);
            break;
        }
        core::hint::spin_loop();
    }
}
