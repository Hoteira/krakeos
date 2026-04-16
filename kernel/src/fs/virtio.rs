use crate::debugln;
use crate::memory::pmm;
use core::ptr::{read_volatile, write_volatile};
use crate::sync::Mutex;
use core::sync::atomic::{AtomicBool, Ordering};

static LOCK: AtomicBool = AtomicBool::new(false);

/// Set by the VirtIO Block ISR on each queue completion; cleared before every request.
static COMPLETION_FLAG: AtomicBool = AtomicBool::new(false);

/// MMIO address of the VirtIO Block ISR status register (must be read to clear the interrupt).
pub static mut BLK_ISR_ADDR: u64 = 0;

/// IDT vector used for the VirtIO Block IRQ (IRQ10 → 0x20 + 10 = 42).
pub const BLK_INT_VEC: u8 = 42;

const VIRTIO_BLK_T_IN:    u32 = 0;
const VIRTIO_BLK_T_OUT:   u32 = 1;
const VIRTIO_BLK_T_FLUSH: u32 = 4; // Write barrier: flush volatile write cache

const VIRTIO_CAP_COMMON: u8 = 1;
const VIRTIO_CAP_NOTIFY: u8 = 2;
const VIRTIO_CAP_ISR:    u8 = 3;

const OFF_DEVICE_FEATURE_SELECT: usize = 0x00;
const OFF_DEVICE_FEATURE: usize = 0x04;
const OFF_DRIVER_FEATURE_SELECT: usize = 0x08;
const OFF_DRIVER_FEATURE: usize = 0x0C;
const OFF_DEVICE_STATUS: usize = 0x14;
const OFF_QUEUE_SELECT: usize = 0x16;
const OFF_QUEUE_SIZE: usize = 0x18;
const OFF_QUEUE_ENABLE: usize = 0x1C;
const OFF_QUEUE_NOTIFY_OFF: usize = 0x1E;
const OFF_QUEUE_DESC: usize = 0x20;
const OFF_QUEUE_DRIVER: usize = 0x28;
const OFF_QUEUE_DEVICE: usize = 0x30;

const STATUS_ACKNOWLEDGE: u8 = 1;
const STATUS_DRIVER: u8 = 2;
const STATUS_DRIVER_OK: u8 = 4;
const STATUS_FEATURES_OK: u8 = 8;


unsafe fn read_16(addr: *mut u8) -> u16 {
    core::ptr::read_volatile(addr as *mut u16)
}
unsafe fn read_32(addr: *mut u8) -> u32 {
    core::ptr::read_volatile(addr as *mut u32)
}
unsafe fn read_8(addr: *mut u8) -> u8 {
    core::ptr::read_volatile(addr)
}
unsafe fn write_8(addr: *mut u8, val: u8) {
    core::ptr::write_volatile(addr, val);
}
unsafe fn write_16(addr: *mut u8, val: u16) {
    core::ptr::write_volatile(addr as *mut u16, val);
}
unsafe fn write_32(addr: *mut u8, val: u32) {
    core::ptr::write_volatile(addr as *mut u32, val);
}
unsafe fn write_64(addr: *mut u8, val: u64) {
    core::ptr::write_volatile(addr as *mut u64, val);
}


#[repr(C, align(16))]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C, align(2))]
struct VirtqAvail {
    flags: u16,
    idx: u16,
    ring: [u16; 128],
    used_event: u16,
}

#[repr(C, align(4))]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

#[repr(C, align(4))]
struct VirtqUsed {
    flags: u16,
    idx: u16,
    ring: [VirtqUsedElem; 128],
    avail_event: u16,
}

struct VirtQueue {
    desc_phys: u64,
    avail_phys: u64,
    used_phys: u64,
    queue_index: u16,
    num: u16,
    free_head: u16,
    last_used_idx: u16,
    notify_addr: u64,
}


static BLK_QUEUE: Mutex<Option<VirtQueue>> = Mutex::new(None);
static IS_ACTIVE: AtomicBool = AtomicBool::new(false);


#[repr(C)]
struct VirtioBlkReqHeader {
    type_: u32,
    reserved: u32,
    sector: u64,
}

pub fn init() {
    let mut device = crate::drivers::pci::find_device(0x1AF4, 0x1042);
    if device.is_none() {
        device = crate::drivers::pci::find_device(0x1AF4, 0x1001);
    }

    if device.is_none() {
        debugln!("VirtIO Block: Device not found.");
        return;
    }

    let virtio = device.unwrap();
    debugln!("VirtIO Block: Found device at Bus {}, Device {}, Func {}", virtio.bus, virtio.device, virtio.function);

    if virtio.enable_bus_mastering() {
        debugln!("VirtIO Block: Bus mastering enabled.");
    } else {
        debugln!("VirtIO Block: Failed to enable bus mastering.");
    }

    let caps = virtio.list_capabilities();


    let mut common_cfg_ptr: *mut u8 = core::ptr::null_mut();
    let mut notify_base: u64 = 0;
    let mut notify_multiplier: u32 = 0;

    for cap in caps {
        if cap.id != 0x09 { continue; }

        let cfg_type = virtio.read_u8(cap.offset as u32 + 3);
        let bar = virtio.read_u8(cap.offset as u32 + 4);
        let offset = virtio.read_u32(cap.offset as u32 + 8);
        let length = virtio.read_u32(cap.offset as u32 + 12);

        let mut bar_base_opt = virtio.get_bar(bar);


        if bar_base_opt.is_none() || bar_base_opt.unwrap() < 0xC0000000 {
            let remapped_addr = crate::drivers::pci::allocate_bar_address(0x1000000); // 16MB
            virtio.write_bar(bar, remapped_addr);
            debugln!("VirtIO Block: Remapped BAR {} to {:#x}", bar, remapped_addr);
            bar_base_opt = virtio.get_bar(bar);
        }

        if cfg_type == VIRTIO_CAP_COMMON {
            if let Some(bar_base) = bar_base_opt {
                let addr = (bar_base as u64) + (offset as u64);
                let virt_addr = crate::memory::vmm::map_mmio(addr, length as usize);
                common_cfg_ptr = virt_addr as *mut u8;
                debugln!("VirtIO Block: Common Config mapped at {:#x} -> Phys {:#x}", virt_addr, addr);
            }
        } else if cfg_type == VIRTIO_CAP_NOTIFY {
            if let Some(bar_base) = bar_base_opt {
                let addr = (bar_base as u64) + (offset as u64);
                notify_base = crate::memory::vmm::map_mmio(addr, length as usize);
                notify_multiplier = virtio.read_capability_data(cap.offset as u8, 16);
                debugln!("VirtIO Block: Notify mapped at {:#x} -> Phys {:#x}", notify_base, addr);
            }
        } else if cfg_type == VIRTIO_CAP_ISR {
            if let Some(bar_base) = bar_base_opt {
                let addr = (bar_base as u64) + (offset as u64);
                unsafe { BLK_ISR_ADDR = crate::memory::vmm::map_mmio(addr, length as usize); }
                debugln!("VirtIO Block: ISR register mapped at {:#x}", unsafe { BLK_ISR_ADDR });
            }
        }
    }

    if common_cfg_ptr.is_null() {
        debugln!("VirtIO Block: Could not find Common Config. Legacy mode not fully implemented.");
        return;
    }

    unsafe {
        debugln!("VirtIO Block: Negotiating features...");
        write_8(common_cfg_ptr.add(OFF_DEVICE_STATUS), 0);


        let mut status = read_8(common_cfg_ptr.add(OFF_DEVICE_STATUS));
        status |= STATUS_ACKNOWLEDGE;
        write_8(common_cfg_ptr.add(OFF_DEVICE_STATUS), status);


        status |= STATUS_DRIVER;
        write_8(common_cfg_ptr.add(OFF_DEVICE_STATUS), status);


        write_32(common_cfg_ptr.add(OFF_DEVICE_FEATURE_SELECT), 1);
        let features_high = read_32(common_cfg_ptr.add(OFF_DEVICE_FEATURE));

        let mut driver_features_high = 0;
        if (features_high & 1) != 0 {
            driver_features_high |= 1;
        }

        write_32(common_cfg_ptr.add(OFF_DRIVER_FEATURE_SELECT), 1);
        write_32(common_cfg_ptr.add(OFF_DRIVER_FEATURE), driver_features_high);

        write_32(common_cfg_ptr.add(OFF_DRIVER_FEATURE_SELECT), 0);
        write_32(common_cfg_ptr.add(OFF_DRIVER_FEATURE), 0);


        status |= STATUS_FEATURES_OK;
        write_8(common_cfg_ptr.add(OFF_DEVICE_STATUS), status);

        let final_status = read_8(common_cfg_ptr.add(OFF_DEVICE_STATUS));
        if (final_status & STATUS_FEATURES_OK) == 0 {
            debugln!("VirtIO Block: Feature negotiation failed.");
            return;
        }


        setup_queue(common_cfg_ptr, 0, notify_base, notify_multiplier);


        status |= STATUS_DRIVER_OK;
        write_8(common_cfg_ptr.add(OFF_DEVICE_STATUS), status);

        if BLK_QUEUE.lock().is_some() {
            IS_ACTIVE.store(true, Ordering::SeqCst);
            debugln!("VirtIO Block: Initialized successfully.");
        }
    }
}

pub fn is_active() -> bool {
    IS_ACTIVE.load(Ordering::SeqCst)
}

unsafe fn setup_queue(common_cfg: *mut u8, index: u16, notify_base: u64, notify_multiplier: u32) {
    write_16(common_cfg.add(OFF_QUEUE_SELECT), index);

    let max_size = read_16(common_cfg.add(OFF_QUEUE_SIZE));
    if max_size == 0 { return; }

    let size: u16 = 128;
    write_16(common_cfg.add(OFF_QUEUE_SIZE), size);

    if let Some(frame) = pmm::allocate_frame() {
        let virt_frame = (frame + crate::memory::paging::HHDM_OFFSET) as *mut u8;
        core::ptr::write_bytes(virt_frame, 0, 4096);


        let desc_addr = frame;
        let avail_addr = desc_addr + 2048;
        let used_addr = desc_addr + 2312;

        let avail_ptr = (avail_addr + crate::memory::paging::HHDM_OFFSET) as *mut VirtqAvail;
        (*avail_ptr).flags = 0; // Enable queue interrupts (was VIRTQ_AVAIL_F_NO_INTERRUPT)

        write_64(common_cfg.add(OFF_QUEUE_DESC), desc_addr);
        write_64(common_cfg.add(OFF_QUEUE_DRIVER), avail_addr);
        write_64(common_cfg.add(OFF_QUEUE_DEVICE), used_addr);

        let notify_off = read_16(common_cfg.add(OFF_QUEUE_NOTIFY_OFF));
        let notify_addr = notify_base + (notify_off as u64 * notify_multiplier as u64);

        write_16(common_cfg.add(OFF_QUEUE_ENABLE), 1);

        *BLK_QUEUE.lock() = Some(VirtQueue {
            desc_phys: desc_addr,
            avail_phys: avail_addr,
            used_phys: used_addr,
            queue_index: index,
            num: size,
            free_head: 0,
            last_used_idx: 0,
            notify_addr,
        });
    }
}

/// Called from the VirtIO Block ISR (IDT vector BLK_INT_VEC).
/// Reads the ISR status register (mandatory to clear the PCI interrupt), then
/// sets COMPLETION_FLAG to wake the waiting `send_command` caller.
pub fn on_disk_irq() {
    if unsafe { BLK_ISR_ADDR } != 0 {
        let _ = unsafe { crate::memory::mmio::read_8(BLK_ISR_ADDR as *mut u8) };
    }
    COMPLETION_FLAG.store(true, Ordering::Release);
}

pub fn read(lba: u64, _disk: u8, target: &mut [u8]) {
    let mut total_processed = 0;
    while total_processed < target.len() {
        let remaining = target.len() - total_processed;


        let chunk_limit = 64 * 4096;
        let current_len = core::cmp::min(remaining, chunk_limit);

        let slice = &mut target[total_processed..total_processed + current_len];
        let current_lba = lba + (total_processed as u64 / 512);

        read_chunk(current_lba, slice);

        total_processed += current_len;
    }
}

fn read_chunk(lba: u64, target: &mut [u8]) {
    let header = VirtioBlkReqHeader {
        type_: VIRTIO_BLK_T_IN,
        reserved: 0,
        sector: lba,
    };

    let status: u8 = 255;

    let req_phys = crate::memory::paging::virt_to_phys(&header as *const _ as u64);
    let req_len = core::mem::size_of::<VirtioBlkReqHeader>() as u32;

    let status_phys = crate::memory::paging::virt_to_phys(&status as *const _ as u64);
    let status_len = 1u32;

    let num_pages = (target.len() + 4095) / 4096 + 1; // +1 for status
    let mut in_phys = alloc::vec::Vec::with_capacity(num_pages + 1);
    let mut in_lens = alloc::vec::Vec::with_capacity(num_pages + 1);

    let mut current_offset = 0;
    while current_offset < target.len() {
        let virt_addr = target.as_ptr() as u64 + current_offset as u64;
        let page_offset = virt_addr & 0xFFF;
        let bytes_left_in_page = 4096 - page_offset;
        let bytes_left_total = (target.len() - current_offset) as u64;

        let chunk_size = core::cmp::min(bytes_left_in_page, bytes_left_total);

        let phys_addr = crate::memory::paging::virt_to_phys(virt_addr);

        in_phys.push(phys_addr);
        in_lens.push(chunk_size as u32);

        current_offset += chunk_size as usize;
    }

    in_phys.push(status_phys);
    in_lens.push(status_len);

    unsafe {
        send_command(&[req_phys], &[req_len], &in_phys, &in_lens);
    }
}

pub fn write(lba: u64, _disk: u8, buffer: &[u8]) {
    let mut total_processed = 0;
    while total_processed < buffer.len() {
        let remaining = buffer.len() - total_processed;
        let chunk_limit = 64 * 4096;
        let current_len = core::cmp::min(remaining, chunk_limit);

        let slice = &buffer[total_processed..total_processed + current_len];
        let current_lba = lba + (total_processed as u64 / 512);

        write_chunk(current_lba, slice);

        total_processed += current_len;
    }
}

/// Issue a `VIRTIO_BLK_T_FLUSH` to ensure all preceding writes have reached
/// stable storage.  Must be called after any sequence of writes that should
/// survive a power failure (e.g., after updating an Ext2 inode or bitmap).
pub fn flush_write_cache(_disk: u8) {
    let header = VirtioBlkReqHeader {
        type_: VIRTIO_BLK_T_FLUSH,
        reserved: 0,
        sector: 0,
    };
    let status: u8 = 255;
    let req_phys    = crate::memory::paging::virt_to_phys(&header as *const _ as u64);
    let status_phys = crate::memory::paging::virt_to_phys(&status  as *const _ as u64);
    unsafe {
        send_command(
            &[req_phys],  &[core::mem::size_of::<VirtioBlkReqHeader>() as u32],
            &[status_phys], &[1],
        );
    }
}

fn write_chunk(lba: u64, buffer: &[u8]) {
    let header = VirtioBlkReqHeader {
        type_: VIRTIO_BLK_T_OUT,
        reserved: 0,
        sector: lba,
    };

    let status: u8 = 255;

    let req_phys = crate::memory::paging::virt_to_phys(&header as *const _ as u64);
    let req_len = core::mem::size_of::<VirtioBlkReqHeader>() as u32;

    let status_phys = crate::memory::paging::virt_to_phys(&status as *const _ as u64);
    let status_len = 1u32;

    let num_pages = (buffer.len() + 4095) / 4096 + 1; // +1 for header
    let mut out_phys = alloc::vec::Vec::with_capacity(num_pages + 1);
    let mut out_lens = alloc::vec::Vec::with_capacity(num_pages + 1);

    out_phys.push(req_phys);
    out_lens.push(req_len);

    let mut current_offset = 0;
    while current_offset < buffer.len() {
        let virt_addr = buffer.as_ptr() as u64 + current_offset as u64;
        let page_offset = virt_addr & 0xFFF;
        let bytes_left_in_page = 4096 - page_offset;
        let bytes_left_total = (buffer.len() - current_offset) as u64;

        let chunk_size = core::cmp::min(bytes_left_in_page, bytes_left_total);
        let phys_addr = crate::memory::paging::virt_to_phys(virt_addr);

        out_phys.push(phys_addr);
        out_lens.push(chunk_size as u32);

        current_offset += chunk_size as usize;
    }

    unsafe {
        send_command(&out_phys, &out_lens, &[status_phys], &[status_len]);
    }
}

unsafe fn send_command(out_phys: &[u64], out_lens: &[u32], in_phys: &[u64], in_lens: &[u32]) {
    // Per-request serialization: only one request in flight at a time.
    // Spin rather than yield: we may be called from boot context (IF=0),
    // from an interrupt handler, or from inside a lock-critical section.
    // The disk ISR fires on CPU 0 via IOAPIC and sets COMPLETION_FLAG
    // asynchronously; we do not need to explicitly invoke the scheduler.
    while LOCK.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }

    // Clear per-request completion flag before submitting (SeqCst: visible to ISR).
    COMPLETION_FLAG.store(false, Ordering::SeqCst);

    // Phase 1: submit request to virtqueue.
    // BLK_QUEUE Mutex (Spinlock) disables interrupts for the duration of the submit.
    // Guard drop re-enables them so the ISR can fire while we wait in Phase 2.
    {
        let mut guard = BLK_QUEUE.lock();
        match guard.as_mut() {
            None => {
                LOCK.store(false, Ordering::Release);
                return;
            }
            Some(vq) => {
                let total_descs = out_phys.len() + in_phys.len();
                let num_usize = vq.num as usize;
                let mut current_desc_idx = vq.free_head as usize;
                let virt_desc_base = (vq.desc_phys + crate::memory::paging::HHDM_OFFSET) as *mut VirtqDesc;

                for i in 0..out_phys.len() {
                    *(virt_desc_base.add(current_desc_idx)) = VirtqDesc {
                        addr: out_phys[i], len: out_lens[i],
                        flags: 1, next: ((current_desc_idx + 1) % num_usize) as u16,
                    };
                    current_desc_idx = (current_desc_idx + 1) % num_usize;
                }
                for i in 0..in_phys.len() {
                    let flags = if i == in_phys.len() - 1 { 2 } else { 2 | 1 };
                    *(virt_desc_base.add(current_desc_idx)) = VirtqDesc {
                        addr: in_phys[i], len: in_lens[i],
                        flags, next: ((current_desc_idx + 1) % num_usize) as u16,
                    };
                    current_desc_idx = (current_desc_idx + 1) % num_usize;
                }

                let last_idx = (vq.free_head as usize + total_descs - 1) % num_usize;
                let last_desc_ptr = virt_desc_base.add(last_idx);
                (*last_desc_ptr).flags &= !1;
                (*last_desc_ptr).next = 0;

                let avail_ptr = (vq.avail_phys + crate::memory::paging::HHDM_OFFSET) as *mut VirtqAvail;
                let idx = (*avail_ptr).idx;
                (*avail_ptr).ring[(idx % vq.num) as usize] = vq.free_head;
                core::sync::atomic::fence(Ordering::SeqCst);
                (*avail_ptr).idx = idx.wrapping_add(1);
                write_volatile(vq.notify_addr as *mut u16, vq.queue_index);

                vq.free_head = ((vq.free_head as usize + total_descs) % num_usize) as u16;
            }
        }
    } // BLK_QUEUE guard dropped here; ISR may now fire if interrupts were enabled

    // Phase 2: per-request completion wait.
    // Fast path: ISR calls on_disk_irq() → sets COMPLETION_FLAG.
    // Fallback: poll used ring directly (works if IRQ not routed via IOAPIC).
    let mut timeout: u64 = 1_000_000_000;
    loop {
        if COMPLETION_FLAG.load(Ordering::Acquire) {
            COMPLETION_FLAG.store(false, Ordering::Release);
            break;
        }
        // Polled fallback: check used ring under BLK_QUEUE lock.
        {
            let guard = BLK_QUEUE.lock();
            if let Some(vq) = guard.as_ref() {
                let used_ptr = (vq.used_phys + crate::memory::paging::HHDM_OFFSET) as *const VirtqUsed;
                let used_idx = read_volatile(core::ptr::addr_of!((*used_ptr).idx));
                if used_idx != vq.last_used_idx { break; }
            }
        }
        // Spin: on a multi-CPU system the disk ISR fires on CPU 0 via IOAPIC
        // and sets COMPLETION_FLAG via a Release store; we will see it on the
        // next Acquire load above without needing an explicit scheduler yield.
        // Timer preemption continues to work normally in the background.
        core::hint::spin_loop();
        timeout -= 1;
        if timeout == 0 { break; }
    }

    // Phase 3: advance the used-ring consumer index.
    {
        let mut guard = BLK_QUEUE.lock();
        if let Some(vq) = guard.as_mut() {
            vq.last_used_idx = vq.last_used_idx.wrapping_add(1);
        }
    }

    LOCK.store(false, Ordering::Release);
}
