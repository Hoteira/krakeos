use crate::debugln;
use crate::memory::pmm;
use core::ptr::{read_volatile, write_volatile};


const VIRTIO_BLK_T_IN: u32 = 0;
const VIRTIO_BLK_T_OUT: u32 = 1;


const VIRTIO_CAP_COMMON: u8 = 1;
const VIRTIO_CAP_NOTIFY: u8 = 2;

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
    unsafe {
        core::ptr::read_volatile(addr as *mut u16)
    }
}
unsafe fn read_32(addr: *mut u8) -> u32 {
    unsafe {
        core::ptr::read_volatile(addr as *mut u32)
    }
}
unsafe fn read_8(addr: *mut u8) -> u8 {
    unsafe {
        core::ptr::read_volatile(addr)
    }
}
unsafe fn write_8(addr: *mut u8, val: u8) {
    unsafe {
        core::ptr::write_volatile(addr, val);
    }
}
unsafe fn write_16(addr: *mut u8, val: u16) {
    unsafe {
        core::ptr::write_volatile(addr as *mut u16, val);
    }
}
unsafe fn write_32(addr: *mut u8, val: u32) {
    unsafe {
        core::ptr::write_volatile(addr as *mut u32, val);
    }
}
unsafe fn write_64(addr: *mut u8, val: u64) {
    unsafe {
        core::ptr::write_volatile(addr as *mut u64, val);
    }
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
    ring: [u16; 32],
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
    ring: [VirtqUsedElem; 32],
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


static mut BLK_QUEUE: Option<VirtQueue> = None;
static mut IS_ACTIVE: bool = false;


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

    let mut next_bar_addr = 0xF1000000; // Use a different range than GPU

    for cap in caps {
        if cap.id != 0x09 { continue; }

        let cfg_type = virtio.read_u8(cap.offset as u32 + 3);
        let bar = virtio.read_u8(cap.offset as u32 + 4);
        let offset = virtio.read_u32(cap.offset as u32 + 8);

        let mut bar_base_opt = virtio.get_bar(bar);
        
        // If BAR is 0 or suspiciously low (below 1MB), remap it.
        if bar_base_opt.is_none() || bar_base_opt.unwrap() < 0x100000 {
            let raw_bar = virtio.read_bar_raw(bar);
            if (raw_bar & 0xFFFFFFF0) < 0x100000 {
                debugln!("VirtIO Block: BAR {} is unmapped or low ({:#x}). Remapping to {:#x}", bar, raw_bar, next_bar_addr);
                virtio.write_bar(bar, next_bar_addr);
                next_bar_addr += 0x100000;
                bar_base_opt = virtio.get_bar(bar);
            }
        }

        if cfg_type == VIRTIO_CAP_COMMON {
            if let Some(bar_base) = bar_base_opt {
                let addr = (bar_base as u64) + (offset as u64);
                common_cfg_ptr = addr as *mut u8;
                debugln!("VirtIO Block: Common Config found at BAR {} offset {:#x} -> Phys {:#x}", bar, offset, addr);
            }
        } else if cfg_type == VIRTIO_CAP_NOTIFY {
            if let Some(bar_base) = bar_base_opt {
                notify_base = (bar_base as u64) + (offset as u64);
                notify_multiplier = virtio.read_capability_data(cap.offset as u8, 16);
                debugln!("VirtIO Block: Notify found at BAR {} offset {:#x} -> Phys {:#x}", bar, offset, notify_base);
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

        if (*(&raw mut BLK_QUEUE)).is_some() {
            IS_ACTIVE = true;
            debugln!("VirtIO Block: Initialized successfully.");
        }
    }
}

pub fn is_active() -> bool {
    unsafe { IS_ACTIVE }
}

unsafe fn setup_queue(common_cfg: *mut u8, index: u16, notify_base: u64, notify_multiplier: u32) {
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
            let _used_addr = (avail_addr + 4 + (2 * 32) + 2 + 4095) & !4095;


            let used_addr = desc_addr + 2048;

            write_64(common_cfg.add(OFF_QUEUE_DESC), desc_addr);
            write_64(common_cfg.add(OFF_QUEUE_DRIVER), avail_addr);
            write_64(common_cfg.add(OFF_QUEUE_DEVICE), used_addr);

            let notify_off = read_16(common_cfg.add(OFF_QUEUE_NOTIFY_OFF));
            let notify_addr = notify_base + (notify_off as u64 * notify_multiplier as u64);

            write_16(common_cfg.add(OFF_QUEUE_ENABLE), 1);

            BLK_QUEUE = Some(VirtQueue {
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
}

pub fn read(lba: u64, _disk: u8, target: &mut [u8]) {
    let sectors = (target.len() + 511) / 512;


    let header = VirtioBlkReqHeader {
        type_: VIRTIO_BLK_T_IN,
        reserved: 0,
        sector: lba,
    };

    let status: u8 = 255;


    let req_phys = &header as *const _ as u64;
    let req_len = core::mem::size_of::<VirtioBlkReqHeader>() as u32;

    let buf_phys = target.as_mut_ptr() as u64;

    let buf_len = (sectors * 512) as u32;

    let status_phys = &status as *const _ as u64;
    let status_len = 1u32;

    unsafe {
        send_command(&[req_phys], &[req_len], &[buf_phys, status_phys], &[buf_len, status_len]);
    }
}

pub fn write(lba: u64, _disk: u8, buffer: &[u8]) {
    let sectors = (buffer.len() + 511) / 512;

    let header = VirtioBlkReqHeader {
        type_: VIRTIO_BLK_T_OUT,
        reserved: 0,
        sector: lba,
    };

    let status: u8 = 255;


    let req_phys = &header as *const _ as u64;
    let req_len = core::mem::size_of::<VirtioBlkReqHeader>() as u32;

    let buf_phys = buffer.as_ptr() as u64;
    let buf_len = (sectors * 512) as u32;

    let status_phys = &status as *const _ as u64;
    let status_len = 1u32;

    unsafe {
        send_command(&[req_phys, buf_phys], &[req_len, buf_len], &[status_phys], &[status_len]);
    }
}

unsafe fn send_command(out_phys: &[u64], out_lens: &[u32], in_phys: &[u64], in_lens: &[u32]) {
    unsafe {
        let int_enabled = crate::interrupts::idt::interrupts();
        if int_enabled { core::arch::asm!("cli"); }

        let vq = match (*(&raw mut BLK_QUEUE)).as_mut() {
            Some(q) => q,
            None => {
                if int_enabled { core::arch::asm!("sti"); }
                return;
            }
        };

        let total_descs = out_phys.len() + in_phys.len();
        let num_usize = vq.num as usize;
        let mut current_desc_idx = vq.free_head as usize;


        for i in 0..out_phys.len() {
            let desc = VirtqDesc {
                addr: out_phys[i],
                len: out_lens[i],
                flags: 1,
                next: ((current_desc_idx + 1) % num_usize) as u16,
            };
            *(vq.desc_phys as *mut VirtqDesc).add(current_desc_idx) = desc;
            current_desc_idx = (current_desc_idx + 1) % num_usize;
        }


        for i in 0..in_phys.len() {
            let flags = if i == in_phys.len() - 1 { 2 } else { 2 | 1 };
            let desc = VirtqDesc {
                addr: in_phys[i],
                len: in_lens[i],
                flags,
                next: ((current_desc_idx + 1) % num_usize) as u16,
            };
            *(vq.desc_phys as *mut VirtqDesc).add(current_desc_idx) = desc;
            current_desc_idx = (current_desc_idx + 1) % num_usize;
        }


        let last_idx = (vq.free_head as usize + total_descs - 1) % num_usize;
        let last_desc_ptr = (vq.desc_phys as *mut VirtqDesc).add(last_idx);
        (*last_desc_ptr).flags &= !1;


        let avail_ptr = vq.avail_phys as *mut VirtqAvail;
        let idx = (*avail_ptr).idx;
        (*avail_ptr).ring[(idx % vq.num) as usize] = vq.free_head;

        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        (*avail_ptr).idx = idx.wrapping_add(1);


        write_volatile(vq.notify_addr as *mut u16, vq.queue_index);


        vq.free_head = ((vq.free_head as usize + total_descs) % num_usize) as u16;


        let used_ptr = vq.used_phys as *mut VirtqUsed;
        loop {
            let used_idx = read_volatile(core::ptr::addr_of!((*used_ptr).idx));
            if used_idx != vq.last_used_idx {
                vq.last_used_idx = vq.last_used_idx.wrapping_add(1);
                break;
            }
            core::hint::spin_loop();
        }

        if int_enabled { core::arch::asm!("sti"); }
    }
}
