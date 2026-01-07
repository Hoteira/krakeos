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
use std::memory::mmio::{read_16, read_32, read_8, write_32, write_8};
pub static mut COMMON_CFG_ADDR: u64 = 0;


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

    let caps = virtio.list_capabilities();
    let virtio_caps = parse_virtio_caps(&virtio, &caps);

    let mut common_cfg_ptr: *mut u8 = core::ptr::null_mut();
    let mut notify_base: u64 = 0;
    let mut notify_multiplier: u32 = 0;
    
    // Simple bump allocator for remapping BARs (starts at 3.75GB)
    let mut next_bar_addr = 0xF0000000;

    for cap in virtio_caps {
        if cap.cfg_type == VIRTIO_CAP_COMMON {
            let mut bar_base_opt = virtio.get_bar(cap.bar);
            
            // If BAR is 0/unmapped (or just has flags but address 0), try to remap it
            if bar_base_opt.is_none() || bar_base_opt == Some(0) {
                let raw_bar = virtio.read_bar_raw(cap.bar);
                // Check if the address part (upper bits) is zero
                if (raw_bar & 0xFFFFFFF0) == 0 {
                    debugln!("VirtIO GPU: BAR {} is unmapped (0x0). Remapping to {:#x}", cap.bar, next_bar_addr);
                    virtio.write_bar(cap.bar, next_bar_addr);
                    
                    next_bar_addr += 0x100000; 
                    
                    bar_base_opt = virtio.get_bar(cap.bar);
                }
            }

            if let Some(bar_base) = bar_base_opt {
                let addr = (bar_base as u64) + (cap.offset as u64);
                common_cfg_ptr = addr as *mut u8;
                unsafe { COMMON_CFG_ADDR = addr; }
                debugln!("VirtIO GPU: Common Config found at BAR {} offset {:#x} -> Phys {:#x}", cap.bar, cap.offset, addr);
            }
        } else if cap.cfg_type == VIRTIO_CAP_NOTIFY {
            let mut bar_base_opt = virtio.get_bar(cap.bar);
            
            if bar_base_opt.is_none() || bar_base_opt == Some(0) {
                let raw_bar = virtio.read_bar_raw(cap.bar);
                if (raw_bar & 0xFFFFFFF0) == 0 {
                    debugln!("VirtIO GPU: BAR {} is unmapped (0x0). Remapping to {:#x}", cap.bar, next_bar_addr);
                    virtio.write_bar(cap.bar, next_bar_addr);
                    next_bar_addr += 0x100000;
                    bar_base_opt = virtio.get_bar(cap.bar);
                }
            }

            if let Some(bar_base) = bar_base_opt {
                notify_base = (bar_base as u64) + (cap.offset as u64);
                notify_multiplier = virtio.read_capability_data(cap.offset as u8, 16);

                if notify_multiplier == 0 {
                    notify_multiplier = 4;
                }

                debugln!("VirtIO GPU: Notify found at BAR {} offset {:#x} -> Phys {:#x}. Multiplier: {}", cap.bar, cap.offset, notify_base, notify_multiplier);
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

        debugln!("VirtIO GPU: Device Features: Lo={:#x}, Hi={:#x}", device_features_low, device_features_high);

        let mut driver_features_low = 0;
        if (device_features_low & (1 << VIRTIO_GPU_F_VIRGL)) != 0 {
            driver_features_low |= 1 << VIRTIO_GPU_F_VIRGL;
            debugln!("VirtIO GPU: Negotiating VIRGL");
        }

        if (device_features_low & (1 << VIRTIO_GPU_F_EDID)) != 0 {
            driver_features_low |= 1 << VIRTIO_GPU_F_EDID;
            debugln!("VirtIO GPU: Negotiating EDID");
        }

        let mut driver_features_high = 0;
        if (device_features_high & (1 << 0)) != 0 {
            driver_features_high |= 1 << 0;
            debugln!("VirtIO GPU: Negotiated VIRTIO_F_VERSION_1");
        }

        write_32(common_cfg_ptr.add(OFF_DRIVER_FEATURE_SELECT), 0);
        write_32(common_cfg_ptr.add(OFF_DRIVER_FEATURE), driver_features_low);

        write_32(common_cfg_ptr.add(OFF_DRIVER_FEATURE_SELECT), 1);
        write_32(common_cfg_ptr.add(OFF_DRIVER_FEATURE), driver_features_high);

        status |= STATUS_FEATURES_OK;
        write_8(common_cfg_ptr.add(OFF_DEVICE_STATUS), status);

        let final_status = read_8(common_cfg_ptr.add(OFF_DEVICE_STATUS));
        if (final_status & STATUS_FEATURES_OK) == 0 {
            debugln!("VirtIO GPU: Features negotiation failed.");
            return;
        }

        let num_queues = read_16(common_cfg_ptr.add(OFF_NUM_QUEUES));
        setup_queue(common_cfg_ptr, 0, notify_base, notify_multiplier);

        if num_queues > 1 {
            setup_queue(common_cfg_ptr, 1, notify_base, notify_multiplier);
        }

        status |= STATUS_DRIVER_OK;
        write_8(common_cfg_ptr.add(OFF_DEVICE_STATUS), status);

        debugln!("VirtIO GPU: Initialization complete (Driver OK). Status: {:#x}", read_8(common_cfg_ptr.add(OFF_DEVICE_STATUS)));
    }
}

pub fn parse_virtio_caps(pci_device: &PciDevice, caps: &[PciCapability]) -> Vec<VirtioPciCap> {
    let mut virtio_caps = Vec::new();

    for cap in caps.iter() {
        if cap.id != 0x09 {
            continue;
        }

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
        debugln!("VirtIO GPU: Checking features...");


        write_32(common_cfg.add(OFF_DEVICE_FEATURE_SELECT), 0);
        let features = read_32(common_cfg.add(OFF_DEVICE_FEATURE));
        let has_virgl = (features & (1 << VIRTIO_GPU_F_VIRGL)) != 0;


        let num_queues = read_16(common_cfg.add(OFF_NUM_QUEUES));
        let has_cursor = num_queues > 1;

        debugln!("  - virGL: {}", if has_virgl { "Available" } else { "Unavailable" });
        debugln!("  - Hardware Cursor: {}", if has_cursor { "Available" } else { "Unavailable" });
    }
}

pub fn get_display_info() -> Option<(u32, u32)> {
    let req_info = VirtioGpuCtrlHeader {
        type_: VIRTIO_GPU_CMD_GET_DISPLAY_INFO,
        flags: 0,
        fence_id: 0,
        ctx_id: 0,
        ring_idx: 0,
        padding: [0; 3],
    };
    let resp_info: VirtioGpuRespDisplayInfo = unsafe { core::mem::zeroed() };

    send_command_queue(
        0,
        &[&req_info as *const _ as u64],
        &[core::mem::size_of_val(&req_info) as u32],
        &[&resp_info as *const _ as u64],
        &[core::mem::size_of_val(&resp_info) as u32],
        true,
    );

    if resp_info.hdr.type_ == VIRTIO_GPU_RESP_OK_DISPLAY_INFO {
        let pmode = resp_info.pmodes[0];

        if pmode.r.width > 0 && pmode.r.height > 0 {
            return Some((pmode.r.width, pmode.r.height));
        }
    }
    None
}

pub fn start_gpu(width: u32, height: u32, phys_buf1: u64, phys_buf2: u64) {
    let req_info = VirtioGpuCtrlHeader {
        type_: VIRTIO_GPU_CMD_GET_DISPLAY_INFO,
        flags: 0,
        fence_id: 0,
        ctx_id: 0,
        ring_idx: 0,
        padding: [0; 3],
    };
    let resp_info: VirtioGpuRespDisplayInfo = unsafe { core::mem::zeroed() };

    send_command_queue(
        0,
        &[&req_info as *const _ as u64],
        &[core::mem::size_of_val(&req_info) as u32],
        &[&resp_info as *const _ as u64],
        &[core::mem::size_of_val(&resp_info) as u32],
        true,
    );

    debugln!("VirtIO GPU: Display Info - Enabled: {}, Flags: {}",
                    resp_info.pmodes[0].enabled, resp_info.pmodes[0].flags);
    debugln!("VirtIO GPU: Display Rect: {}x{} @ ({},{})",
                    resp_info.pmodes[0].r.width, resp_info.pmodes[0].r.height,
                    resp_info.pmodes[0].r.x, resp_info.pmodes[0].r.y);

    debugln!("VirtIO GPU: Display Info Type: {:#x}", resp_info.hdr.type_);


    let create_resource = |id: u32, phys: u64| {
        let req_create = VirtioGpuResourceCreate2d {
            hdr: VirtioGpuCtrlHeader {
                type_: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
                flags: 0,
                fence_id: 0,
                ctx_id: 0,
                ring_idx: 0,
                padding: [0; 3],
            },
            resource_id: id,
            format: 1,
            width,
            height,
        };
        let resp_create: VirtioGpuCtrlHeader = unsafe { core::mem::zeroed() };

        send_command_queue(
            0,
            &[&req_create as *const _ as u64],
            &[core::mem::size_of_val(&req_create) as u32],
            &[&resp_create as *const _ as u64],
            &[core::mem::size_of_val(&resp_create) as u32],
            true,
        );
        debugln!("VirtIO GPU: Create Resource {} Resp: {:#x}", id, resp_create.type_);

        #[repr(C)]
        struct AttachRequest {
            hdr: VirtioGpuResourceAttachBacking,
            entry: VirtioGpuMemEntry,
        }

        let req_attach = AttachRequest {
            hdr: VirtioGpuResourceAttachBacking {
                hdr: VirtioGpuCtrlHeader {
                    type_: VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING,
                    flags: 0,
                    fence_id: 0,
                    ctx_id: 0,
                    ring_idx: 0,
                    padding: [0; 3],
                },
                resource_id: id,
                nr_entries: 1,
            },
            entry: VirtioGpuMemEntry {
                addr: phys,
                length: width * height * 4,
                padding: 0,
            },
        };
        let resp_attach: VirtioGpuCtrlHeader = unsafe { core::mem::zeroed() };

        send_command_queue(
            0,
            &[&req_attach as *const _ as u64],
            &[core::mem::size_of_val(&req_attach) as u32],
            &[&resp_attach as *const _ as u64],
            &[core::mem::size_of_val(&resp_attach) as u32],
            true,
        );
        debugln!("VirtIO GPU: Attach Resource {} Resp: {:#x}", id, resp_attach.type_);
    };


    create_resource(1, phys_buf1);
    create_resource(2, phys_buf2);


    let req_scanout = VirtioGpuSetScanout {
        hdr: VirtioGpuCtrlHeader {
            type_: VIRTIO_GPU_CMD_SET_SCANOUT,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_idx: 0,
            padding: [0; 3],
        },
        r: VirtioGpuRect { x: 0, y: 0, width, height },
        scanout_id: 0,
        resource_id: 1,
    };
    let resp_scanout: VirtioGpuCtrlHeader = unsafe { core::mem::zeroed() };

    send_command_queue(
        0,
        &[&req_scanout as *const _ as u64],
        &[core::mem::size_of_val(&req_scanout) as u32],
        &[&resp_scanout as *const _ as u64],
        &[core::mem::size_of_val(&resp_scanout) as u32],
        true,
    );
    debugln!("VirtIO GPU: Set Scanout (Res 1) Resp: {:#x}", resp_scanout.type_);

    debugln!("VirtIO GPU: Started with Page Flipping (Res 1 & 2).");
}

pub fn transfer_and_flush(resource_id: u32, width: u32, height: u32) {
    let req_transfer = VirtioGpuTransferToHost2d {
        hdr: VirtioGpuCtrlHeader {
            type_: VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_idx: 0,
            padding: [0; 3],
        },
        r: VirtioGpuRect { x: 0, y: 0, width, height },
        offset: 0,
        resource_id,
        padding: 0,
    };
    let resp_transfer: VirtioGpuCtrlHeader = unsafe { core::mem::zeroed() };

    send_command_queue(
        0,
        &[&req_transfer as *const _ as u64],
        &[core::mem::size_of_val(&req_transfer) as u32],
        &[&resp_transfer as *const _ as u64],
        &[core::mem::size_of_val(&resp_transfer) as u32],
        false,
    );

    let req_flush = VirtioGpuResourceFlush {
        hdr: VirtioGpuCtrlHeader {
            type_: VIRTIO_GPU_CMD_RESOURCE_FLUSH,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_idx: 0,
            padding: [0; 3],
        },
        r: VirtioGpuRect { x: 0, y: 0, width, height },
        resource_id,
        padding: 0,
    };
    let resp_flush: VirtioGpuCtrlHeader = unsafe { core::mem::zeroed() };

    send_command_queue(
        0,
        &[&req_flush as *const _ as u64],
        &[core::mem::size_of_val(&req_flush) as u32],
        &[&resp_flush as *const _ as u64],
        &[core::mem::size_of_val(&resp_flush) as u32],
        true,
    );
}

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

pub static mut REQ_IDX: usize = 0;

pub fn flush(x: u32, y: u32, width: u32, height: u32, screen_width: u32, resource_id: u32) {
    let offset = (y as u64 * screen_width as u64 + x as u64) * 4;

    unsafe {
        let idx = REQ_IDX % 128;
        REQ_IDX += 1;

        TRANSFER_REQUESTS[idx] = VirtioGpuTransferToHost2d {
            hdr: VirtioGpuCtrlHeader {
                type_: VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D,
                flags: 0,
                fence_id: 0,
                ctx_id: 0,
                ring_idx: 0,
                padding: [0; 3],
            },
            r: VirtioGpuRect { x, y, width, height },
            offset,
            resource_id,
            padding: 0,
        };

        send_command_queue(
            0,
            &[&TRANSFER_REQUESTS[idx] as *const _ as u64],
            &[core::mem::size_of::<VirtioGpuTransferToHost2d>() as u32],
            &[],
            &[],
            false,
        );

        FLUSH_REQUESTS[idx] = VirtioGpuResourceFlush {
            hdr: VirtioGpuCtrlHeader {
                type_: VIRTIO_GPU_CMD_RESOURCE_FLUSH,
                flags: 0,
                fence_id: 0,
                ctx_id: 0,
                ring_idx: 0,
                padding: [0; 3],
            },
            r: VirtioGpuRect { x, y, width, height },
            resource_id,
            padding: 0,
        };

        send_command_queue(
            0,
            &[&FLUSH_REQUESTS[idx] as *const _ as u64],
            &[core::mem::size_of::<VirtioGpuResourceFlush>() as u32],
            &[],
            &[],
            true, // We still wait for flush to ensure frame is visible
        );
    }
}

pub fn set_scanout(resource_id: u32, width: u32, height: u32) {
    let req_scanout = VirtioGpuSetScanout {
        hdr: VirtioGpuCtrlHeader {
            type_: VIRTIO_GPU_CMD_SET_SCANOUT,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_idx: 0,
            padding: [0; 3],
        },
        r: VirtioGpuRect { x: 0, y: 0, width, height },
        scanout_id: 0,
        resource_id,
    };
    let resp_scanout: VirtioGpuCtrlHeader = unsafe { core::mem::zeroed() };

    send_command_queue(
        0,
        &[&req_scanout as *const _ as u64],
        &[core::mem::size_of_val(&req_scanout) as u32],
        &[&resp_scanout as *const _ as u64],
        &[core::mem::size_of_val(&resp_scanout) as u32],
        true,
    );
}

