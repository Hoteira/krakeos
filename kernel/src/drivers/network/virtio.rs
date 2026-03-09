#![allow(static_mut_refs)]
use crate::debugln;
use crate::drivers::pci::{PciCapability, PciDevice};
use crate::memory::mmio::{read_8, read_16, read_32, write_8, write_16, write_32, write_64};
use crate::memory::pmm;
use crate::memory::vmm;
use crate::sync::Mutex;
use alloc::collections::VecDeque;
use alloc::string::String;
use core::sync::atomic::{AtomicBool, Ordering};

static LOCK: AtomicBool = AtomicBool::new(false);

const VIRTIO_CAP_COMMON: u8 = 1;
const VIRTIO_CAP_NOTIFY: u8 = 2;
const VIRTIO_CAP_ISR: u8 = 3;

const OFF_DEVICE_FEATURE_SELECT: usize = 0x00;
const OFF_DEVICE_FEATURE: usize = 0x04;
const OFF_DRIVER_FEATURE_SELECT: usize = 0x08;
const OFF_DRIVER_FEATURE: usize = 0x0C;
const OFF_MSIX_CONFIG: usize = 0x10;
const OFF_NUM_QUEUES: usize = 0x12;
const OFF_DEVICE_STATUS: usize = 0x14;
const OFF_CONFIG_GENERATION: usize = 0x15;
const OFF_QUEUE_SELECT: usize = 0x16;
const OFF_QUEUE_SIZE: usize = 0x18;
const OFF_QUEUE_MSIX_VECTOR: usize = 0x1A;
const OFF_QUEUE_ENABLE: usize = 0x1C;
const OFF_QUEUE_NOTIFY_OFF: usize = 0x1E;
const OFF_QUEUE_DESC: usize = 0x20;
const OFF_QUEUE_DRIVER: usize = 0x28;
const OFF_QUEUE_DEVICE: usize = 0x30;

const STATUS_ACKNOWLEDGE: u8 = 1;
const STATUS_DRIVER: u8 = 2;
const STATUS_DRIVER_OK: u8 = 4;
const STATUS_FEATURES_OK: u8 = 8;

const VIRTIO_NET_F_MAC: u32 = 1 << 5;
const VIRTIO_NET_F_STATUS: u32 = 1 << 16;

#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C, align(2))]
#[derive(Debug, Clone, Copy)]
struct VirtqAvail {
    flags: u16,
    idx: u16,
    ring: [u16; 256],
    used_event: u16,
}

#[repr(C, align(4))]
#[derive(Debug, Clone, Copy)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

#[repr(C, align(4))]
#[derive(Debug, Clone, Copy)]
struct VirtqUsed {
    flags: u16,
    idx: u16,
    ring: [VirtqUsedElem; 256],
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

// VirtIO Net Header (Legacy/Modern merge)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct VirtioNetHdr {
    flags: u8,
    gso_type: u8,
    hdr_len: u16,
    gso_size: u16,
    csum_start: u16,
    csum_offset: u16,
}

pub struct VirtioNetDevice {
    mac: [u8; 6],
    rx_queue: Option<VirtQueue>,
    tx_queue: Option<VirtQueue>,
    rx_buffers: VecDeque<(u64, u64, u32)>, // phys, virt, len
    rx_packet_queue: Mutex<VecDeque<alloc::vec::Vec<u8>>>,
}

pub static mut NET_DEVICE: Option<VirtioNetDevice> = None;

pub fn init() -> Result<(), String> {
    let device_opt = crate::drivers::pci::find_device(0x1AF4, 0x1000); // Legacy
    let device = if let Some(d) = device_opt {
        d
    } else {
        if let Some(d) = crate::drivers::pci::find_device(0x1AF4, 0x1041) {
            // Modern
            d
        } else {
            return Err(String::from("VirtIO Net: Device not found."));
        }
    };

    debugln!(
        "VirtIO Net: Found device at Bus {}, Device {}, Func {}",
        device.bus,
        device.device,
        device.function
    );
    device.enable_bus_mastering();

    let caps = device.list_capabilities();
    let mut common_cfg_ptr: *mut u8 = core::ptr::null_mut();
    let mut notify_base: u64 = 0;
    let mut notify_multiplier: u32 = 0;
    let mut device_cfg_ptr: *mut u8 = core::ptr::null_mut();

    let mut next_bar_addr = 0xF2000000;

    for cap in caps {
        if cap.id != 0x09 {
            continue;
        }

        let cfg_type = device.read_u8(cap.offset as u32 + 3);
        let bar = device.read_u8(cap.offset as u32 + 4);
        let offset = device.read_u32(cap.offset as u32 + 8);

        let mut bar_base_opt = device.get_bar(bar);
        if bar_base_opt.is_none() || bar_base_opt.unwrap() < 0xC0000000 {
            let raw_bar = device.read_bar_raw(bar);
            if (raw_bar & 0xFFFFFFF0) < 0xC0000000 {
                debugln!(
                    "VirtIO Net: BAR {} is unmapped or low ({:#x}). Remapping to {:#x}",
                    bar,
                    raw_bar,
                    next_bar_addr
                );
                device.write_bar(bar, next_bar_addr);
                next_bar_addr += 0x100000;
                bar_base_opt = device.get_bar(bar);
            }
        }

        if let Some(bar_base) = bar_base_opt {
            let addr = (bar_base as u64) + (offset as u64);

            if cfg_type == VIRTIO_CAP_COMMON {
                let virt_addr = vmm::map_mmio(addr, 4096);
                common_cfg_ptr = virt_addr as *mut u8;
            } else if cfg_type == VIRTIO_CAP_NOTIFY {
                notify_base = vmm::map_mmio(addr, 4096);
                notify_multiplier = device.read_capability_data(cap.offset as u8, 16);
            } else if cfg_type == 4 {
                // Device specific (MAC)
                let virt_addr = vmm::map_mmio(addr, 4096);
                device_cfg_ptr = virt_addr as *mut u8;
            }
        }
    }

    if common_cfg_ptr.is_null() {
        return Err(String::from("VirtIO Net: Common config not found."));
    }

    unsafe {
        // Reset
        write_8(common_cfg_ptr.add(OFF_DEVICE_STATUS), 0);

        // Acknowledge
        let mut status = read_8(common_cfg_ptr.add(OFF_DEVICE_STATUS));
        status |= STATUS_ACKNOWLEDGE;
        write_8(common_cfg_ptr.add(OFF_DEVICE_STATUS), status);

        // Driver
        status |= STATUS_DRIVER;
        write_8(common_cfg_ptr.add(OFF_DEVICE_STATUS), status);

        // Features
        write_32(common_cfg_ptr.add(OFF_DEVICE_FEATURE_SELECT), 0);
        let features = read_32(common_cfg_ptr.add(OFF_DEVICE_FEATURE));

        let mut driver_features = 0;
        if (features & VIRTIO_NET_F_MAC) != 0 {
            driver_features |= VIRTIO_NET_F_MAC;
        }
        if (features & VIRTIO_NET_F_STATUS) != 0 {
            driver_features |= VIRTIO_NET_F_STATUS;
        }

        write_32(common_cfg_ptr.add(OFF_DRIVER_FEATURE_SELECT), 0);
        write_32(common_cfg_ptr.add(OFF_DRIVER_FEATURE), driver_features);

        status |= STATUS_FEATURES_OK;
        write_8(common_cfg_ptr.add(OFF_DEVICE_STATUS), status);

        let final_status = read_8(common_cfg_ptr.add(OFF_DEVICE_STATUS));
        if (final_status & STATUS_FEATURES_OK) == 0 {
            return Err(String::from("VirtIO Net: Feature negotiation failed."));
        }

        // Get MAC
        let mut mac = [0u8; 6];
        if !device_cfg_ptr.is_null() {
            for i in 0..6 {
                mac[i] = read_8(device_cfg_ptr.add(i));
            }
            debugln!(
                "VirtIO Net: MAC Address: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
            );
        }

        // Queues: 0=RX, 1=TX
        let rx_queue = setup_queue(common_cfg_ptr, 0, notify_base, notify_multiplier);
        let tx_queue = setup_queue(common_cfg_ptr, 1, notify_base, notify_multiplier);

        if rx_queue.is_none() || tx_queue.is_none() {
            return Err(String::from("VirtIO Net: Failed to setup queues."));
        }

        let mut net_dev = VirtioNetDevice {
            mac,
            rx_queue,
            tx_queue,
            rx_buffers: VecDeque::new(),
            rx_packet_queue: Mutex::new(VecDeque::new()),
        };

        // Populate RX queue
        fill_rx_queue(&mut net_dev);

        NET_DEVICE = Some(net_dev);

        status |= STATUS_DRIVER_OK;
        write_8(common_cfg_ptr.add(OFF_DEVICE_STATUS), status);

        debugln!("VirtIO Net: Initialized.");
    }
    Ok(())
}

unsafe fn setup_queue(
    common_cfg: *mut u8,
    index: u16,
    notify_base: u64,
    notify_multiplier: u32,
) -> Option<VirtQueue> {
    write_16(common_cfg.add(OFF_QUEUE_SELECT), index);

    let max_size = read_16(common_cfg.add(OFF_QUEUE_SIZE));
    if max_size == 0 {
        return None;
    }

    let size = 256;
    write_16(common_cfg.add(OFF_QUEUE_SIZE), size);

    let size_bytes = 16 * size as usize + 6 + 2 * size as usize + 2 + 6 + 8 * size as usize + 2;
    let pages = (size_bytes + 4095) / 4096;

    let frame = pmm::allocate_frames(pages, 0)?;
    let virt_frame = (frame + crate::memory::paging::HHDM_OFFSET) as *mut u8;
    core::ptr::write_bytes(virt_frame, 0, pages * 4096);

    let desc_addr = frame;
    let avail_addr = desc_addr + (16 * size as u64);
    let used_addr = (avail_addr + 6 + 2 * size as u64 + 2 + 3) & !3; // Align 4

    let desc_ptr = (desc_addr + crate::memory::paging::HHDM_OFFSET) as *mut VirtqDesc;
    for i in 0..size {
        (*desc_ptr.add(i as usize)).next = (i + 1);
        (*desc_ptr.add(i as usize)).flags = 0;
    }
    (*desc_ptr.add((size - 1) as usize)).next = 0xFFFF; // End of list

    write_64(common_cfg.add(OFF_QUEUE_DESC), desc_addr);
    write_64(common_cfg.add(OFF_QUEUE_DRIVER), avail_addr);
    write_64(common_cfg.add(OFF_QUEUE_DEVICE), used_addr);

    let notify_off = read_16(common_cfg.add(OFF_QUEUE_NOTIFY_OFF));
    let notify_addr = notify_base + (notify_off as u64 * notify_multiplier as u64);

    write_16(common_cfg.add(OFF_QUEUE_ENABLE), 1);

    Some(VirtQueue {
        desc_phys: desc_addr,
        avail_phys: avail_addr,
        used_phys: used_addr,
        queue_index: index,
        num: size,
        free_head: 0,
        last_used_idx: 0,
        notify_addr,
    })
}

unsafe fn fill_rx_queue(dev: &mut VirtioNetDevice) {
    if let Some(vq) = &mut dev.rx_queue {
        let num_bufs = vq.num as usize;
        for _ in 0..num_bufs {
            if vq.free_head == 0xFFFF {
                break;
            }

            if let Some(frame) = pmm::allocate_frame(0) {
                let virt = frame + crate::memory::paging::HHDM_OFFSET;
                dev.rx_buffers.push_back((frame, virt, 2048));

                let desc_idx = vq.free_head;
                let desc_ptr = (vq.desc_phys + crate::memory::paging::HHDM_OFFSET) as *mut VirtqDesc;
                let next_idx = (*desc_ptr.add(desc_idx as usize)).next;

                (*desc_ptr.add(desc_idx as usize)) = VirtqDesc {
                    addr: frame,
                    len: 2048,
                    flags: 2, // WRITABLE
                    next: 0,
                };

                vq.free_head = next_idx;

                let avail_ptr = (vq.avail_phys + crate::memory::paging::HHDM_OFFSET) as *mut VirtqAvail;
                let idx = (*avail_ptr).idx;
                (*avail_ptr).ring[(idx % vq.num) as usize] = desc_idx;

                core::sync::atomic::fence(Ordering::SeqCst);
                (*avail_ptr).idx = idx.wrapping_add(1);
            }
        }
        write_16(vq.notify_addr as *mut u8, vq.queue_index);
    }
}

unsafe fn recycle_tx_descriptors(dev: &mut VirtioNetDevice) {
    if let Some(vq) = &mut dev.tx_queue {
        let used_ptr = (vq.used_phys + crate::memory::paging::HHDM_OFFSET) as *mut VirtqUsed;
        let current_used_idx = (*used_ptr).idx;

        while vq.last_used_idx != current_used_idx {
            let elem = (*used_ptr).ring[(vq.last_used_idx % vq.num) as usize];
            let id = elem.id as usize;
            let desc_ptr = (vq.desc_phys + crate::memory::paging::HHDM_OFFSET) as *mut VirtqDesc;
            let addr = (*desc_ptr.add(id)).addr;

            if addr != 0 {
                pmm::free_frame(addr);
            }

            (*desc_ptr.add(id)).flags = 0;
            (*desc_ptr.add(id)).next = vq.free_head;
            vq.free_head = id as u16;

            vq.last_used_idx = vq.last_used_idx.wrapping_add(1);
        }
    }
}

pub fn poll_rx() {
    unsafe {
        let dev_ptr = core::ptr::addr_of_mut!(NET_DEVICE);
        if let Some(dev) = (*dev_ptr).as_mut() {
            if let Some(vq) = &mut dev.rx_queue {
                let used_ptr = (vq.used_phys + crate::memory::paging::HHDM_OFFSET) as *mut VirtqUsed;
                let used_idx = (*used_ptr).idx;

                while vq.last_used_idx != used_idx {
                    let elem = (*used_ptr).ring[(vq.last_used_idx % vq.num) as usize];
                    let id = elem.id;
                    let len = elem.len;

                    let desc_ptr = (vq.desc_phys + crate::memory::paging::HHDM_OFFSET) as *mut VirtqDesc;
                    let desc = *desc_ptr.add(id as usize);

                    let virt_addr = desc.addr + crate::memory::paging::HHDM_OFFSET;
                    let hdr_size = core::mem::size_of::<VirtioNetHdr>();

                    if len as usize > hdr_size {
                        let packet_len = len as usize - hdr_size;
                        let mut packet = alloc::vec![0u8; packet_len];
                        core::ptr::copy_nonoverlapping(
                            (virt_addr as *const u8).add(hdr_size),
                            packet.as_mut_ptr(),
                            packet_len,
                        );

                        crate::net::on_receive(&packet);
                        dev.rx_packet_queue.lock().push_back(packet);
                    }

                    let avail_ptr = (vq.avail_phys + crate::memory::paging::HHDM_OFFSET) as *mut VirtqAvail;
                    let idx = (*avail_ptr).idx;
                    (*avail_ptr).ring[(idx % vq.num) as usize] = id as u16;
                    core::sync::atomic::fence(Ordering::SeqCst);
                    (*avail_ptr).idx = idx.wrapping_add(1);

                    vq.last_used_idx = vq.last_used_idx.wrapping_add(1);
                }

                if vq.last_used_idx != used_idx {
                    write_16(vq.notify_addr as *mut u8, vq.queue_index);
                }
            }
        }
    }
    crate::net::poll_loopback();
}

pub fn recv_packet() -> Option<alloc::vec::Vec<u8>> {
    unsafe {
        let dev_ptr = core::ptr::addr_of_mut!(NET_DEVICE);
        if let Some(dev) = (*dev_ptr).as_mut() {
            poll_rx();
            dev.rx_packet_queue.lock().pop_front()
        } else {
            crate::net::poll_loopback();
            None
        }
    }
}

pub fn send_packet(data: &[u8]) -> usize {
    unsafe {
        let dev = if let Some(d) = NET_DEVICE.as_mut() {
            d
        } else {
            return 1;
        };

        recycle_tx_descriptors(dev);

        let vq = if let Some(q) = &mut dev.tx_queue {
            q
        } else {
            return 2;
        };

        let head_idx = vq.free_head;
        if head_idx == 0xFFFF { return 3; }

        let desc_ptr = (vq.desc_phys + crate::memory::paging::HHDM_OFFSET) as *mut VirtqDesc;
        let next_idx = (*desc_ptr.add(head_idx as usize)).next;

        let hdr_frame = match pmm::allocate_frame(0) {
            Some(f) => f,
            None => return 5,
        };
        let hdr_virt = hdr_frame + crate::memory::paging::HHDM_OFFSET;
        let hdr = VirtioNetHdr::default();
        *(hdr_virt as *mut VirtioNetHdr) = hdr;

        let total_len = core::mem::size_of::<VirtioNetHdr>() + data.len();
        if total_len > 4096 { return 4; }

        let ptr = hdr_virt as *mut u8;
        core::ptr::copy_nonoverlapping(
            data.as_ptr(),
            ptr.add(core::mem::size_of::<VirtioNetHdr>()),
            data.len(),
        );

        (*desc_ptr.add(head_idx as usize)) = VirtqDesc {
            addr: hdr_frame,
            len: total_len as u32,
            flags: 0,
            next: 0,
        };

        vq.free_head = next_idx;

        let avail_ptr = (vq.avail_phys + crate::memory::paging::HHDM_OFFSET) as *mut VirtqAvail;
        let idx = (*avail_ptr).idx;
        (*avail_ptr).ring[(idx % vq.num) as usize] = head_idx;

        core::sync::atomic::fence(Ordering::SeqCst);
        (*avail_ptr).idx = idx.wrapping_add(1);

        write_16(vq.notify_addr as *mut u8, vq.queue_index);
        0
    }
}
