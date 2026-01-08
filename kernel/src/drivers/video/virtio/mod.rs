pub mod consts;
pub mod structs;
pub mod queue;
pub mod cursor;

use self::consts::*;
use self::queue::*;
use self::structs::*;
use crate::debugln;
use crate::drivers::pci::{PciCapability, PciDevice};
use alloc::vec::Vec;
use crate::memory::mmio::{read_16, read_32, read_8, write_32, write_8};
use crate::memory::vmm;
use crate::memory::pmm;
use crate::memory::paging::virt_to_phys;

pub static mut COMMON_CFG_ADDR: u64 = 0;


static mut GPU_CMD_VIRT: u64 = 0;
static mut GPU_CMD_PHYS: u64 = 0;
static mut REQ_IDX: usize = 0;

pub static mut TRANSFER_REQUESTS: [VirtioGpuTransferToHost2d; 128] = [VirtioGpuTransferToHost2d {
    hdr: VirtioGpuCtrlHeader { type_: 0, flags: 0, fence_id: 0, ctx_id: 0, ring_idx: 0, padding: [0; 3] },
    r: VirtioGpuRect { x: 0, y: 0, width: 0, height: 0 },
    offset: 0,
    resource_id: 0,
    padding: 0,
}; 128];

pub static mut FLUSH_REQUESTS: [VirtioGpuResourceFlush; 128] = [VirtioGpuResourceFlush {
    hdr: VirtioGpuCtrlHeader { type_: 0, flags: 0, fence_id: 0, ctx_id: 0, ring_idx: 0, padding: [0; 3] },
    r: VirtioGpuRect { x: 0, y: 0, width: 0, height: 0 },
    resource_id: 0,
    padding: 0,
}; 128];

pub fn init() {
    let virtio_opt = crate::drivers::pci::find_device(0x1AF4, 0x1050);

    if virtio_opt.is_none() {
        debugln!("VirtIO GPU: Device not found.");
        return;
    }

    let virtio = virtio_opt.unwrap();
    debugln!("VirtIO GPU: Found device at Bus {}, Device {}, Func {}", virtio.bus, virtio.device, virtio.function);

    if virtio.enable_bus_mastering() {
        debugln!("VirtIO GPU: Bus mastering enabled.");
    } else {
        debugln!("VirtIO GPU: Failed to enable bus mastering.");
    }

    
    unsafe {
        if let Some(frame) = pmm::allocate_frame(0) {
            GPU_CMD_PHYS = frame;
            GPU_CMD_VIRT = frame + crate::memory::paging::HHDM_OFFSET;
            core::ptr::write_bytes(GPU_CMD_VIRT as *mut u8, 0, 4096);
        } else {
            panic!("VirtIO GPU: Failed to allocate command buffer");
        }
    }

    let caps = virtio.list_capabilities();
    let virtio_caps = parse_virtio_caps(&virtio, &caps);

    let mut common_cfg_ptr: *mut u8 = core::ptr::null_mut();
    let mut notify_base: u64 = 0;
    let mut notify_multiplier: u32 = 0;
    
    let mut next_bar_addr = 0xF0000000;

    for cap in virtio_caps {
        if cap.cfg_type == VIRTIO_CAP_COMMON {
            let mut bar_base_opt = virtio.get_bar(cap.bar);
            if bar_base_opt.is_none() || bar_base_opt == Some(0) {
                let raw_bar = virtio.read_bar_raw(cap.bar);
                if (raw_bar & 0xFFFFFFF0) == 0 {
                    virtio.write_bar(cap.bar, next_bar_addr);
                    next_bar_addr += 0x100000; 
                    bar_base_opt = virtio.get_bar(cap.bar);
                }
            }

            if let Some(bar_base) = bar_base_opt {
                let addr = (bar_base as u64) + (cap.offset as u64);
                let virt_addr = vmm::map_mmio(addr, 4096);
                common_cfg_ptr = virt_addr as *mut u8;
                unsafe { COMMON_CFG_ADDR = virt_addr; }
            }
        } else if cap.cfg_type == VIRTIO_CAP_NOTIFY {
            let mut bar_base_opt = virtio.get_bar(cap.bar);
            if bar_base_opt.is_none() || bar_base_opt == Some(0) {
                let raw_bar = virtio.read_bar_raw(cap.bar);
                if (raw_bar & 0xFFFFFFF0) == 0 {
                    virtio.write_bar(cap.bar, next_bar_addr);
                    next_bar_addr += 0x100000;
                    bar_base_opt = virtio.get_bar(cap.bar);
                }
            }

            if let Some(bar_base) = bar_base_opt {
                let addr = (bar_base as u64) + (cap.offset as u64);
                notify_base = vmm::map_mmio(addr, 4096);
                notify_multiplier = virtio.read_capability_data(cap.offset as u8, 16);
                if notify_multiplier == 0 { notify_multiplier = 4; }
            }
        }
    }

    if common_cfg_ptr.is_null() {
        debugln!("VirtIO GPU: Could not find Common Config capability.");
        return;
    }

    check_features(common_cfg_ptr);

    unsafe {
        write_8(common_cfg_ptr.add(OFF_DEVICE_STATUS), 0);
        let mut status = read_8(common_cfg_ptr.add(OFF_DEVICE_STATUS));
        status |= STATUS_ACKNOWLEDGE;
        write_8(common_cfg_ptr.add(OFF_DEVICE_STATUS), status);
        status |= STATUS_DRIVER;
        write_8(common_cfg_ptr.add(OFF_DEVICE_STATUS), status);

        write_32(common_cfg_ptr.add(OFF_DEVICE_FEATURE_SELECT), 0);
        let device_features_low = read_32(common_cfg_ptr.add(OFF_DEVICE_FEATURE));
        write_32(common_cfg_ptr.add(OFF_DEVICE_FEATURE_SELECT), 1);
        let device_features_high = read_32(common_cfg_ptr.add(OFF_DEVICE_FEATURE));

        let mut driver_features_low = 0;
        if (device_features_low & (1 << VIRTIO_GPU_F_EDID)) != 0 {
            driver_features_low |= 1 << VIRTIO_GPU_F_EDID;
        }
        let mut driver_features_high = 0;
        if (device_features_high & (1 << 0)) != 0 {
            driver_features_high |= 1 << 0;
        }

        write_32(common_cfg_ptr.add(OFF_DRIVER_FEATURE_SELECT), 0);
        write_32(common_cfg_ptr.add(OFF_DRIVER_FEATURE), driver_features_low);
        write_32(common_cfg_ptr.add(OFF_DRIVER_FEATURE_SELECT), 1);
        write_32(common_cfg_ptr.add(OFF_DRIVER_FEATURE), driver_features_high);

        status |= STATUS_FEATURES_OK;
        write_8(common_cfg_ptr.add(OFF_DEVICE_STATUS), status);

        let num_queues = read_16(common_cfg_ptr.add(OFF_NUM_QUEUES));
        setup_queue(common_cfg_ptr, 0, notify_base, notify_multiplier);
        if num_queues > 1 { setup_queue(common_cfg_ptr, 1, notify_base, notify_multiplier); }

        status |= STATUS_DRIVER_OK;
        write_8(common_cfg_ptr.add(OFF_DEVICE_STATUS), status);
        debugln!("VirtIO GPU: Initialized successfully.");
    }
}

pub fn parse_virtio_caps(pci_device: &PciDevice, caps: &[PciCapability]) -> Vec<VirtioPciCap> {
    let mut virtio_caps = Vec::new();
    for cap in caps.iter() {
        if cap.id != 0x09 { continue; }
        let cfg_type = pci_device.read_u8(cap.offset as u32 + 3);
        let bar = pci_device.read_u8(cap.offset as u32 + 4);
        let offset = pci_device.read_u32(cap.offset as u32 + 8);
        let length = pci_device.read_u32(cap.offset as u32 + 12);
        virtio_caps.push(VirtioPciCap { cfg_type, bar, offset, length });
    }
    virtio_caps
}

fn check_features(common_cfg: *mut u8) {
    unsafe {
        write_32(common_cfg.add(OFF_DEVICE_FEATURE_SELECT), 0);
        let features = read_32(common_cfg.add(OFF_DEVICE_FEATURE));
        let has_virgl = (features & (1 << VIRTIO_GPU_F_VIRGL)) != 0;
        let num_queues = read_16(common_cfg.add(OFF_NUM_QUEUES));
        let has_cursor = num_queues > 1;
        debugln!("VirtIO GPU: features virGL: {}, Cursor: {}", has_virgl, has_cursor);
    }
}

pub fn get_display_info() -> Option<(u32, u32)> {
    unsafe {
        let req_ptr = GPU_CMD_VIRT as *mut VirtioGpuCtrlHeader;
        let resp_ptr = (GPU_CMD_VIRT + 1024) as *mut VirtioGpuRespDisplayInfo;

        core::ptr::write(req_ptr, VirtioGpuCtrlHeader {
            type_: VIRTIO_GPU_CMD_GET_DISPLAY_INFO,
            flags: 0, fence_id: 0, ctx_id: 0, ring_idx: 0, padding: [0; 3],
        });
        core::ptr::write_bytes(resp_ptr as *mut u8, 0, 512);

        send_command_queue(
            0,
            &[GPU_CMD_PHYS],
            &[core::mem::size_of::<VirtioGpuCtrlHeader>() as u32],
            &[GPU_CMD_PHYS + 1024],
            &[core::mem::size_of::<VirtioGpuRespDisplayInfo>() as u32],
            true,
        );

        let resp = &*resp_ptr;
        if resp.hdr.type_ == VIRTIO_GPU_RESP_OK_DISPLAY_INFO {
            for i in 0..16 {
                let pmode = resp.pmodes[i];
                if pmode.enabled != 0 {
                    if pmode.r.width > 0 && pmode.r.height > 0 {
                        return Some((pmode.r.width, pmode.r.height));
                    }
                }
            }
            
            let pmode = resp.pmodes[0];
            if pmode.r.width > 0 && pmode.r.height > 0 {
                return Some((pmode.r.width, pmode.r.height));
            }
        }
    }
    None
}

pub fn start_gpu(width: u32, height: u32, phys_buf1: u64, phys_buf2: u64) {
    unsafe {
        get_display_info();

        let req_create_ptr = GPU_CMD_VIRT as *mut VirtioGpuResourceCreate2d;
        let resp_ptr = (GPU_CMD_VIRT + 1024) as *mut VirtioGpuCtrlHeader;

        let mut create_resource = |id: u32, phys: u64| {
            core::ptr::write(req_create_ptr, VirtioGpuResourceCreate2d {
                hdr: VirtioGpuCtrlHeader {
                    type_: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
                    flags: 0, fence_id: 0, ctx_id: 0, ring_idx: 0, padding: [0; 3],
                },
                resource_id: id,
                format: 1,
                width,
                height,
            });

            send_command_queue(0, &[GPU_CMD_PHYS], &[core::mem::size_of::<VirtioGpuResourceCreate2d>() as u32],
                               &[GPU_CMD_PHYS + 1024], &[24], true);

            let req_attach_ptr = GPU_CMD_VIRT as *mut AttachRequest;
            core::ptr::write(req_attach_ptr, AttachRequest {
                hdr: VirtioGpuResourceAttachBacking {
                    hdr: VirtioGpuCtrlHeader {
                        type_: VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING,
                        flags: 0, fence_id: 0, ctx_id: 0, ring_idx: 0, padding: [0; 3],
                    },
                    resource_id: id,
                    nr_entries: 1,
                },
                entry: VirtioGpuMemEntry { addr: phys, length: width * height * 4, padding: 0 },
            });

            send_command_queue(0, &[GPU_CMD_PHYS], &[core::mem::size_of::<AttachRequest>() as u32],
                               &[GPU_CMD_PHYS + 1024], &[24], true);
        };

        create_resource(1, phys_buf1);
        create_resource(2, phys_buf2);

        let req_scanout_ptr = GPU_CMD_VIRT as *mut VirtioGpuSetScanout;
        core::ptr::write(req_scanout_ptr, VirtioGpuSetScanout {
            hdr: VirtioGpuCtrlHeader {
                type_: VIRTIO_GPU_CMD_SET_SCANOUT,
                flags: 0, fence_id: 0, ctx_id: 0, ring_idx: 0, padding: [0; 3],
            },
            r: VirtioGpuRect { x: 0, y: 0, width, height },
            scanout_id: 0,
            resource_id: 1,
        });

        send_command_queue(0, &[GPU_CMD_PHYS], &[core::mem::size_of::<VirtioGpuSetScanout>() as u32],
                           &[GPU_CMD_PHYS + 1024], &[24], true);
    }
}

pub fn transfer_and_flush(resource_id: u32, width: u32, height: u32) {
    unsafe {
        let idx = REQ_IDX % 128;
        REQ_IDX += 1;

        let req_transfer = &mut TRANSFER_REQUESTS[idx];
        *req_transfer = VirtioGpuTransferToHost2d {
            hdr: VirtioGpuCtrlHeader { type_: VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D, flags: 0, fence_id: 0, ctx_id: 0, ring_idx: 0, padding: [0; 3] },
            r: VirtioGpuRect { x: 0, y: 0, width, height },
            offset: 0,
            resource_id,
            padding: 0,
        };
        let req_transfer_phys = virt_to_phys(req_transfer as *const _ as u64);
        send_command_queue(0, &[req_transfer_phys], &[core::mem::size_of::<VirtioGpuTransferToHost2d>() as u32], &[], &[], false);

        let req_flush = &mut FLUSH_REQUESTS[idx];
        *req_flush = VirtioGpuResourceFlush {
            hdr: VirtioGpuCtrlHeader { type_: VIRTIO_GPU_CMD_RESOURCE_FLUSH, flags: 0, fence_id: 0, ctx_id: 0, ring_idx: 0, padding: [0; 3] },
            r: VirtioGpuRect { x: 0, y: 0, width, height },
            resource_id, padding: 0,
        };
        let req_flush_phys = virt_to_phys(req_flush as *const _ as u64);
        send_command_queue(0, &[req_flush_phys], &[core::mem::size_of::<VirtioGpuResourceFlush>() as u32], &[], &[], false);
    }
}

pub fn flush(x: u32, y: u32, width: u32, height: u32, screen_width: u32, resource_id: u32) {
    let offset = (y as u64 * screen_width as u64 + x as u64) * 4;
    unsafe {
        let idx = REQ_IDX % 128;
        REQ_IDX += 1;

        let req_transfer = &mut TRANSFER_REQUESTS[idx];
        *req_transfer = VirtioGpuTransferToHost2d {
            hdr: VirtioGpuCtrlHeader { type_: VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D, flags: 0, fence_id: 0, ctx_id: 0, ring_idx: 0, padding: [0; 3] },
            r: VirtioGpuRect { x, y, width, height },
            offset, resource_id, padding: 0,
        };
        let req_transfer_phys = virt_to_phys(req_transfer as *const _ as u64);
        send_command_queue(0, &[req_transfer_phys], &[core::mem::size_of::<VirtioGpuTransferToHost2d>() as u32], &[], &[], false);

        let req_flush = &mut FLUSH_REQUESTS[idx];
        *req_flush = VirtioGpuResourceFlush {
            hdr: VirtioGpuCtrlHeader { type_: VIRTIO_GPU_CMD_RESOURCE_FLUSH, flags: 0, fence_id: 0, ctx_id: 0, ring_idx: 0, padding: [0; 3] },
            r: VirtioGpuRect { x, y, width, height },
            resource_id, padding: 0,
        };
        let req_flush_phys = virt_to_phys(req_flush as *const _ as u64);
        send_command_queue(0, &[req_flush_phys], &[core::mem::size_of::<VirtioGpuResourceFlush>() as u32], &[], &[], false);
    }
}
pub fn set_scanout(resource_id: u32, width: u32, height: u32) {
    unsafe {
        let req_scanout_ptr = GPU_CMD_VIRT as *mut VirtioGpuSetScanout;
        core::ptr::write(req_scanout_ptr, VirtioGpuSetScanout {
            hdr: VirtioGpuCtrlHeader { type_: VIRTIO_GPU_CMD_SET_SCANOUT, flags: 0, fence_id: 0, ctx_id: 0, ring_idx: 0, padding: [0; 3] },
            r: VirtioGpuRect { x: 0, y: 0, width, height },
            scanout_id: 0, resource_id,
        });
        send_command_queue(0, &[GPU_CMD_PHYS], &[core::mem::size_of::<VirtioGpuSetScanout>() as u32], &[GPU_CMD_PHYS + 1024], &[24], false);
    }
}