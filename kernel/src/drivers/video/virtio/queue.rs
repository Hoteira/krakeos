use super::structs::*;
use super::consts::*;
use crate::memory::pmm;
use core::ptr::{read_volatile, write_volatile};
use std::memory::mmio::{read_16, write_16, write_64};
use crate::{debugln};

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

pub fn setup_queue(common_cfg: *mut u8, index: u16, notify_base: u64, notify_multiplier: u32) {
    unsafe {
        write_16(common_cfg.add(OFF_QUEUE_SELECT), index);

        let max_size = read_16(common_cfg.add(OFF_QUEUE_SIZE));
        if max_size == 0 { return; }

        let size: u16 = 32;
        write_16(common_cfg.add(OFF_QUEUE_SIZE), size);

        if let Some(frame) = pmm::allocate_frame(0) {
            core::ptr::write_bytes(frame as *mut u8, 0, 4096);

            let desc_addr = frame;
            let avail_addr = desc_addr + 512;
            let used_addr = (avail_addr + 70 + 3) & !3;

            let avail_ptr = avail_addr as *mut VirtqAvail;
            (*avail_ptr).flags = 1;

            write_64(common_cfg.add(OFF_QUEUE_DESC), desc_addr);
            write_64(common_cfg.add(OFF_QUEUE_DRIVER), avail_addr);
            write_64(common_cfg.add(OFF_QUEUE_DEVICE), used_addr);

            let notify_off = read_16(common_cfg.add(OFF_QUEUE_NOTIFY_OFF));
            let notify_addr = notify_base + (notify_off as u64 * notify_multiplier as u64);

            write_16(common_cfg.add(OFF_QUEUE_ENABLE), 1);
            
            
            let enabled = read_16(common_cfg.add(OFF_QUEUE_ENABLE));
            if enabled != 1 {
                debugln!("VirtIO GPU: WARNING - Queue {} failed to enable! Read back: {}", index, enabled);
            }

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
            
            debugln!("VirtIO GPU: Queue {} setup at phys {:#x}. Notify Off: {}, Addr: {:#x}", index, frame, notify_off, notify_addr);
        }
    }
}

pub fn send_command_queue(queue_idx: usize, out_phys: &[u64], out_lens: &[u32], in_phys: &[u64], in_lens: &[u32]) -> bool {
    unsafe {
        let int_enabled = crate::interrupts::idt::interrupts();
        if int_enabled { core::arch::asm!("cli"); }

        let vq = match &mut VIRT_QUEUES[queue_idx] {
            Some(v) => v,
            None => {
                if int_enabled { core::arch::asm!("sti"); }
                return false;
            }
        };

        let total_descs = out_phys.len() + in_phys.len();
        if total_descs == 0 {
            if int_enabled { core::arch::asm!("sti"); }
            return false;
        }

        let free_head_usize = vq.free_head as usize;
        let num_usize = vq.num as usize;
        let mut current_desc_idx = free_head_usize;


        
        for i in 0..out_phys.len() {
            *(vq.desc_phys as *mut VirtqDesc).add(current_desc_idx) = VirtqDesc {
                addr: out_phys[i],
                len: out_lens[i],
                flags: if i == out_phys.len() - 1 && in_phys.len() == 0 { 0 } else { 1 }, 
                next: ((current_desc_idx + 1) % num_usize) as u16,
            };
            current_desc_idx = (current_desc_idx + 1) % num_usize;
        }

        
        for i in 0..in_phys.len() {
            *(vq.desc_phys as *mut VirtqDesc).add(current_desc_idx) = VirtqDesc {
                addr: in_phys[i],
                len: in_lens[i],
                flags: 2 | (if i == in_phys.len() - 1 { 0 } else { 1 }), 
                next: ((current_desc_idx + 1) % num_usize) as u16,
            };
            current_desc_idx = (current_desc_idx + 1) % num_usize;
        }
        
        
        let last_desc_idx = (free_head_usize + total_descs - 1) % num_usize;
        (*(vq.desc_phys as *mut VirtqDesc).add(last_desc_idx)).next = 0; 

        let avail_ptr = vq.avail_phys as *mut VirtqAvail;
        let idx = (*avail_ptr).idx;
        (*avail_ptr).ring[(idx % vq.num) as usize] = vq.free_head;

        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        (*avail_ptr).idx = idx.wrapping_add(1);

        write_volatile(vq.notify_addr as *mut u16, vq.queue_index);
        vq.free_head = ((free_head_usize + total_descs) % num_usize) as u16;

        let used_ptr = vq.used_phys as *mut VirtqUsed;
        let mut timeout = 10_000_000;
        let mut success = false;

        loop {
            let used_idx = read_volatile(core::ptr::addr_of!((*used_ptr).idx));
            if used_idx != vq.last_used_idx {
                vq.last_used_idx = vq.last_used_idx.wrapping_add(1);
                success = true;
                break;
            }
            core::hint::spin_loop();
            timeout -= 1;
            if timeout == 0 {
                break;
            }
        }

        if int_enabled { core::arch::asm!("sti"); }

        if !success {
            debugln!("VirtIO GPU: Queue {} Timed Out!", queue_idx);
        }
        success
    }
}