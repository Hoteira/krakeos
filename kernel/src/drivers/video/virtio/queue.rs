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
}

pub static mut VIRT_QUEUES: [Option<VirtQueue>; 2] = [None, None];
// Split locks to prevent deadlock between Screen Flush (Q0) and Mouse Move (Q1)
static QUEUE_LOCKS: [AtomicBool; 2] = [AtomicBool::new(false), AtomicBool::new(false)];

pub fn setup_queue(common_cfg: *mut u8, index: u16, notify_base: u64, notify_multiplier: u32) {
    unsafe {
        write_16(common_cfg.add(OFF_QUEUE_SELECT), index);
        let max_size = read_16(common_cfg.add(OFF_QUEUE_SIZE));
        if max_size == 0 { return; }
        let size: u16 = 128;
        write_16(common_cfg.add(OFF_QUEUE_SIZE), size);
        if let Some(frame) = pmm::allocate_frame(0) {
            let virt_frame = (frame + crate::memory::paging::HHDM_OFFSET) as *mut u8;
            core::ptr::write_bytes(virt_frame, 0, 4096);
            let desc_addr = frame;
            let avail_addr = desc_addr + 2048;
            let used_addr = (avail_addr + 262 + 3) & !3;
            let avail_ptr = (avail_addr + crate::memory::paging::HHDM_OFFSET) as *mut VirtqAvail;
            (*avail_ptr).flags = 1;
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
            });
        }
    }
}

pub fn send_command_queue(queue_idx: usize, out_phys: &[u64], out_lens: &[u32], in_phys: &[u64], in_lens: &[u32], wait: bool) -> bool {
    if queue_idx >= 2 { return false; }
    while QUEUE_LOCKS[queue_idx].compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    let result = unsafe { send_command_queue_unlocked(queue_idx, out_phys, out_lens, in_phys, in_lens, wait) };
    QUEUE_LOCKS[queue_idx].store(false, Ordering::Release);
    result
}

unsafe fn send_command_queue_unlocked(queue_idx: usize, out_phys: &[u64], out_lens: &[u32], in_phys: &[u64], in_lens: &[u32], wait: bool) -> bool {
    let int_enabled = crate::interrupts::idt::interrupts();
    if int_enabled { core::arch::asm!("cli"); }
    let vq = match &mut VIRT_QUEUES[queue_idx] {
        Some(v) => v,
        None => { if int_enabled { core::arch::asm!("sti"); } return false; }
    };
    let total_descs = out_phys.len() + in_phys.len();
    let mut timeout = 10_000_000;
    while vq.last_avail_idx.wrapping_sub(vq.last_used_idx) + total_descs as u16 > vq.num {
        vq.last_used_idx = read_volatile(core::ptr::addr_of!((*( (vq.used_phys + crate::memory::paging::HHDM_OFFSET) as *const VirtqUsed )).idx));
        timeout -= 1;
        if timeout == 0 { if int_enabled { core::arch::asm!("sti"); } return false; }
    }
    let virt_desc_base = (vq.desc_phys + crate::memory::paging::HHDM_OFFSET) as *mut VirtqDesc;
    let mut curr = vq.free_head as usize;
    for i in 0..out_phys.len() {
        *(virt_desc_base).add(curr) = VirtqDesc {
            addr: out_phys[i], len: out_lens[i],
            flags: if i == out_phys.len() - 1 && in_phys.len() == 0 { 0 } else { 1 },
            next: ((curr + 1) % vq.num as usize) as u16,
        };
        curr = (curr + 1) % vq.num as usize;
    }
    for i in 0..in_phys.len() {
        *(virt_desc_base).add(curr) = VirtqDesc {
            addr: in_phys[i], len: in_lens[i],
            flags: 2 | (if i == in_phys.len() - 1 { 0 } else { 1 }),
            next: ((curr + 1) % vq.num as usize) as u16,
        };
        curr = (curr + 1) % vq.num as usize;
    }
    let last_desc = (virt_desc_base).add((vq.free_head as usize + total_descs - 1) % vq.num as usize);
    (*last_desc).next = 0; (*last_desc).flags &= !1;
    let avail_ptr = (vq.avail_phys + crate::memory::paging::HHDM_OFFSET) as *mut VirtqAvail;
    (*avail_ptr).ring[((*avail_ptr).idx % vq.num) as usize] = vq.free_head;
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    (*avail_ptr).idx = (*avail_ptr).idx.wrapping_add(1);
    vq.last_avail_idx = vq.last_avail_idx.wrapping_add(1);
    write_volatile(vq.notify_addr as *mut u16, vq.queue_index);
    vq.free_head = curr as u16;
    if !wait { if int_enabled { core::arch::asm!("sti"); } return true; }
    let used_ptr = (vq.used_phys + crate::memory::paging::HHDM_OFFSET) as *mut VirtqUsed;
    let mut success = false; timeout = 10_000_000;
    loop {
        if read_volatile(core::ptr::addr_of!((*used_ptr).idx)) != vq.last_used_idx {
            vq.last_used_idx = read_volatile(core::ptr::addr_of!((*used_ptr).idx));
            if vq.last_used_idx == vq.last_avail_idx { success = true; break; }
        }
        core::hint::spin_loop(); timeout -= 1;
        if timeout == 0 { break; }
    }
    if int_enabled { core::arch::asm!("sti"); }
    success
}
