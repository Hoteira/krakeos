pub mod consts;
pub mod structs;
pub mod queue;
pub mod cursor;

use self::consts::*;
use self::queue::*;
use self::structs::*;
use crate::debugln;
use crate::drivers::pci::{PciCapability, PciDevice};
use crate::memory::mmio::{read_16, read_32, read_8, write_32, write_8};
use crate::memory::paging::virt_to_phys;
use crate::memory::pmm;
use crate::memory::vmm;
use alloc::vec::Vec;

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub static mut COMMON_CFG_ADDR: u64 = 0;


static mut GPU_CMD_VIRT: u64 = 0;
static mut GPU_CMD_PHYS: u64 = 0;
static GPU_CMD_LOCK: AtomicBool = AtomicBool::new(false);
static REQ_IDX: AtomicUsize = AtomicUsize::new(0);
static FENCE_COUNTER: AtomicUsize = AtomicUsize::new(0);

static mut TRANSFER_REQUESTS_VIRT: *mut VirtioGpuTransferToHost2d = core::ptr::null_mut();
static mut TRANSFER_REQUESTS_PHYS: u64 = 0;
static mut TRANSFER_RESPONSES_VIRT: *mut VirtioGpuCtrlHeader = core::ptr::null_mut();
static mut TRANSFER_RESPONSES_PHYS: u64 = 0;
static mut FLUSH_REQUESTS_VIRT: *mut VirtioGpuResourceFlush = core::ptr::null_mut();
static mut FLUSH_REQUESTS_PHYS: u64 = 0;
static mut FLUSH_RESPONSES_VIRT: *mut VirtioGpuCtrlHeader = core::ptr::null_mut();
static mut FLUSH_RESPONSES_PHYS: u64 = 0;

pub fn init() {
    let virtio_opt = crate::drivers::pci::find_device(0x1AF4, 0x1050);

    if virtio_opt.is_none() {
        debugln!("VirtIO GPU: Device not found.");
        return;
    }

    let virtio = virtio_opt.unwrap();
    debugln!("VirtIO GPU: Found device at Bus {}, Device {}, Func {}", virtio.bus, virtio.device, virtio.function);

    unsafe {
        if let Some(frame) = pmm::allocate_frame() {
            GPU_CMD_PHYS = frame;
            // Map command buffer as uncacheable MMIO
            GPU_CMD_VIRT = crate::memory::vmm::map_mmio(frame, 4096);
            core::ptr::write_bytes(GPU_CMD_VIRT as *mut u8, 0, 4096);
        } else {
            panic!("VirtIO GPU: Failed to allocate command buffer");
        }
        
        // Allocate 4 frames for the asynchronous request/response slots (mapped NO_CACHE)
        // This prevents the previous overlap where 2 frames were insufficient.
        if let Some(f1) = pmm::allocate_frames(4) {
            let virt = vmm::map_mmio(f1, 16384);
            core::ptr::write_bytes(virt as *mut u8, 0, 16384);
            
            TRANSFER_REQUESTS_VIRT = virt as *mut VirtioGpuTransferToHost2d;
            TRANSFER_REQUESTS_PHYS = f1;
            
            TRANSFER_RESPONSES_VIRT = (virt + 4096) as *mut VirtioGpuCtrlHeader;
            TRANSFER_RESPONSES_PHYS = f1 + 4096;
            
            FLUSH_REQUESTS_VIRT = (virt + 8192) as *mut VirtioGpuResourceFlush;
            FLUSH_REQUESTS_PHYS = f1 + 8192;
            
            FLUSH_RESPONSES_VIRT = (virt + 12288) as *mut VirtioGpuCtrlHeader;
            FLUSH_RESPONSES_PHYS = f1 + 12288;
        } else {
            panic!("VirtIO GPU: Failed to allocate async buffers");
        }
    }

    let caps = virtio.list_capabilities();
    let virtio_caps = parse_virtio_caps(&virtio, &caps);

    let mut common_cfg_ptr: *mut u8 = core::ptr::null_mut();
    let mut notify_base: u64 = 0;
    let mut notify_multiplier: u32 = 0;

    for cap in &virtio_caps {
        if cap.cfg_type == VIRTIO_CAP_COMMON {
            let mut bar_base_opt = virtio.get_bar(cap.bar);
            if bar_base_opt.is_none() || bar_base_opt.unwrap() < 0xC0000000 {
                let remapped_addr = crate::drivers::pci::allocate_bar_address(0x1000000); // 16MB for caps
                virtio.write_bar(cap.bar, remapped_addr);
                debugln!("VirtIO GPU: Remapped BAR {} to {:#x}", cap.bar, remapped_addr);
                bar_base_opt = virtio.get_bar(cap.bar);
            }

            if let Some(bar_base) = bar_base_opt {
                let addr = (bar_base as u64) + (cap.offset as u64);
                let virt_addr = vmm::map_mmio(addr, cap.length as usize);
                common_cfg_ptr = virt_addr as *mut u8;
                unsafe { COMMON_CFG_ADDR = virt_addr; }
            }
        } else if cap.cfg_type == VIRTIO_CAP_NOTIFY {
            let mut bar_base_opt = virtio.get_bar(cap.bar);
            if bar_base_opt.is_none() || bar_base_opt.unwrap() < 0xC0000000 {
                // Determine size based on multiplier if possible, otherwise assume large enough for safety
                let remapped_addr = crate::drivers::pci::allocate_bar_address(0x1000000); // 16MB for notify
                virtio.write_bar(cap.bar, remapped_addr);
                debugln!("VirtIO GPU: Remapped BAR {} to {:#x} (Notify Area)", cap.bar, remapped_addr);
                bar_base_opt = virtio.get_bar(cap.bar);
            }

            if let Some(bar_base) = bar_base_opt {
                let addr = (bar_base as u64) + (cap.offset as u64);
                notify_base = vmm::map_mmio(addr, cap.length as usize);
                notify_multiplier = virtio.read_capability_data(cap.cap_offset, 16);
                if notify_multiplier == 0 { notify_multiplier = 4; }
            }
        }
    }

    if common_cfg_ptr.is_null() {
        debugln!("VirtIO GPU: Could not find Common Config capability.");
        return;
    }

    check_features(common_cfg_ptr);

    if virtio.enable_bus_mastering() {
        debugln!("VirtIO GPU: Bus mastering enabled.");
    } else {
        debugln!("VirtIO GPU: Failed to enable bus mastering.");
    }

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
        
        // Mandatory check: did the device accept our features?
        let check_status = read_8(common_cfg_ptr.add(OFF_DEVICE_STATUS));
        if (check_status & STATUS_FEATURES_OK) == 0 {
            debugln!("VirtIO GPU: Feature negotiation failed.");
            return;
        }

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
        virtio_caps.push(VirtioPciCap { cfg_type, bar, offset, length, cap_offset: cap.offset });
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
    while GPU_CMD_LOCK.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    unsafe {
        let req_ptr = GPU_CMD_VIRT as *mut VirtioGpuCtrlHeader;
        let resp_ptr = (GPU_CMD_VIRT + 1024) as *mut VirtioGpuRespDisplayInfo;

        core::ptr::write(req_ptr, VirtioGpuCtrlHeader {
            type_: VIRTIO_GPU_CMD_GET_DISPLAY_INFO,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_idx: 0,
            padding: [0; 3],
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
        let result = if resp.hdr.type_ == VIRTIO_GPU_RESP_OK_DISPLAY_INFO {
            let pmode = resp.pmodes[0];
            if pmode.r.width > 0 && pmode.r.height > 0 {
                Some((pmode.r.width, pmode.r.height))
            } else {
                let mut found = None;
                for i in 1..16 {
                    let pmode = resp.pmodes[i];
                    if pmode.enabled != 0 {
                        if pmode.r.width > 0 && pmode.r.height > 0 {
                            found = Some((pmode.r.width, pmode.r.height));
                            break;
                        }
                    }
                }
                found
            }
        } else {
            None
        };
        GPU_CMD_LOCK.store(false, Ordering::Release);
        result
    }
}

pub fn start_gpu(width: u32, height: u32, phys_buf1: u64, phys_buf2: u64) {
    while GPU_CMD_LOCK.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    unsafe {
        // get_display_info already called externally or we'd deadlock here
        
        let req_create_ptr = GPU_CMD_VIRT as *mut VirtioGpuResourceCreate2d;
        let resp_ptr = (GPU_CMD_VIRT + 1024) as *mut VirtioGpuCtrlHeader;

        let mut create_resource = |id: u32, phys: u64| {
            core::ptr::write(req_create_ptr, VirtioGpuResourceCreate2d {
                hdr: VirtioGpuCtrlHeader {
                    type_: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
                    flags: 0,
                    fence_id: 0,
                    ctx_id: 0,
                    ring_idx: 0,
                    padding: [0; 3],
                },
                resource_id: id,
                format: 2, // A8R8G8B8_UNORM
                width,
                height,
                });

            crate::debugln!("VirtIO GPU: Creating resource {} ({}x{})", id, width, height);
            send_command_queue(0, &[GPU_CMD_PHYS], &[core::mem::size_of::<VirtioGpuResourceCreate2d>() as u32],
                               &[GPU_CMD_PHYS + 1024], &[24], true);
            
            if (*resp_ptr).type_ != VIRTIO_GPU_RESP_OK_NODATA {
                crate::debugln!("VirtIO GPU: Resource creation failed: {:#x}", (*resp_ptr).type_);
            }

            // Use TWO descriptors for ATTACH_BACKING: one for header, one for entries
            let req_attach_hdr_ptr = GPU_CMD_VIRT as *mut VirtioGpuResourceAttachBacking;
            core::ptr::write(req_attach_hdr_ptr, VirtioGpuResourceAttachBacking {
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
            });

            let entry_ptr = (GPU_CMD_VIRT + 512) as *mut VirtioGpuMemEntry;
            core::ptr::write(entry_ptr, VirtioGpuMemEntry {
                addr: phys,
                length: width * height * 4,
                padding: 0,
            });

            crate::debugln!("VirtIO GPU: Attaching backing memory to resource {} at {:#x}", id, phys);
            send_command_queue(0, 
                &[GPU_CMD_PHYS, GPU_CMD_PHYS + 512], 
                &[core::mem::size_of::<VirtioGpuResourceAttachBacking>() as u32, core::mem::size_of::<VirtioGpuMemEntry>() as u32],
                &[GPU_CMD_PHYS + 1024], &[24], true);

            if (*resp_ptr).type_ != VIRTIO_GPU_RESP_OK_NODATA {
                crate::debugln!("VirtIO GPU: Resource attachment failed: {:#x}", (*resp_ptr).type_);
            }
        };

        create_resource(1, phys_buf1);
        create_resource(2, phys_buf2);

        let req_scanout_ptr = GPU_CMD_VIRT as *mut VirtioGpuSetScanout;
        core::ptr::write(req_scanout_ptr, VirtioGpuSetScanout {
            hdr: VirtioGpuCtrlHeader {
                type_: VIRTIO_GPU_CMD_SET_SCANOUT,
                flags: VIRTIO_GPU_FLAG_FENCE, // Use FENCE to ensure device is ready
                fence_id: 1,
                ctx_id: 0,
                ring_idx: 0,
                padding: [0; 3],
            },
            r: VirtioGpuRect { x: 0, y: 0, width, height },
            scanout_id: 0,
            resource_id: 1,
        });

        crate::debugln!("VirtIO GPU: Setting scanout to resource 1 (Fenced)");
        send_command_queue(0, &[GPU_CMD_PHYS], &[core::mem::size_of::<VirtioGpuSetScanout>() as u32],
                           &[GPU_CMD_PHYS + 1024], &[24], true);
        
        if (*resp_ptr).type_ != VIRTIO_GPU_RESP_OK_NODATA {
            crate::debugln!("VirtIO GPU: Setting scanout failed: {:#x}", (*resp_ptr).type_);
        }
    }
    GPU_CMD_LOCK.store(false, Ordering::Release);
}

pub fn transfer_and_flush(resource_id: u32, width: u32, height: u32, wait: bool) {
    unsafe {
        let idx = REQ_IDX.fetch_add(1, Ordering::SeqCst) % 64;

        let req_transfer = TRANSFER_REQUESTS_VIRT.add(idx);
        core::ptr::write(req_transfer, VirtioGpuTransferToHost2d {
            hdr: VirtioGpuCtrlHeader { type_: VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D, flags: 0, fence_id: 0, ctx_id: 0, ring_idx: 0, padding: [0; 3] },
            r: VirtioGpuRect { x: 0, y: 0, width, height },
            offset: 0,
            resource_id,
            padding: 0,
        });
        
        let req_transfer_phys = TRANSFER_REQUESTS_PHYS + (idx * core::mem::size_of::<VirtioGpuTransferToHost2d>()) as u64;
        let resp_transfer_phys = TRANSFER_RESPONSES_PHYS + (idx * core::mem::size_of::<VirtioGpuCtrlHeader>()) as u64;
        let resp_transfer_virt = TRANSFER_RESPONSES_VIRT.add(idx);
        
        send_command_queue(0, &[req_transfer_phys], &[core::mem::size_of::<VirtioGpuTransferToHost2d>() as u32], &[resp_transfer_phys], &[24], wait);
        
        if wait && (*resp_transfer_virt).type_ != VIRTIO_GPU_RESP_OK_NODATA {
            crate::debugln!("VirtIO GPU: Transfer failed: {:#x}", (*resp_transfer_virt).type_);
        }

        let req_flush = FLUSH_REQUESTS_VIRT.add(idx);
        core::ptr::write(req_flush, VirtioGpuResourceFlush {
            hdr: VirtioGpuCtrlHeader { type_: VIRTIO_GPU_CMD_RESOURCE_FLUSH, flags: 0, fence_id: 0, ctx_id: 0, ring_idx: 0, padding: [0; 3] },
            r: VirtioGpuRect { x: 0, y: 0, width, height },
            resource_id,
            padding: 0,
        });
        
        let req_flush_phys = FLUSH_REQUESTS_PHYS + (idx * core::mem::size_of::<VirtioGpuResourceFlush>()) as u64;
        let resp_flush_phys = FLUSH_RESPONSES_PHYS + (idx * core::mem::size_of::<VirtioGpuCtrlHeader>()) as u64;
        let resp_flush_virt = FLUSH_RESPONSES_VIRT.add(idx);
        
        send_command_queue(0, &[req_flush_phys], &[core::mem::size_of::<VirtioGpuResourceFlush>() as u32], &[resp_flush_phys], &[24], wait);

        if wait && (*resp_flush_virt).type_ != VIRTIO_GPU_RESP_OK_NODATA {
            crate::debugln!("VirtIO GPU: Flush failed: {:#x}", (*resp_flush_virt).type_);
        }
    }
}

pub fn flush(x: u32, y: u32, width: u32, height: u32, screen_width: u32, resource_id: u32, wait: bool) {
    unsafe {
        let offset = (y * screen_width + x) * 4;
        let idx = REQ_IDX.fetch_add(1, Ordering::SeqCst) % 64;

        let req_transfer = TRANSFER_REQUESTS_VIRT.add(idx);
        core::ptr::write(req_transfer, VirtioGpuTransferToHost2d {
            hdr: VirtioGpuCtrlHeader { type_: VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D, flags: 0, fence_id: 0, ctx_id: 0, ring_idx: 0, padding: [0; 3] },
            r: VirtioGpuRect { x, y, width, height },
            offset: offset as u64,
            resource_id,
            padding: 0,
        });
        
        let req_transfer_phys = TRANSFER_REQUESTS_PHYS + (idx * core::mem::size_of::<VirtioGpuTransferToHost2d>()) as u64;
        let resp_transfer_phys = TRANSFER_RESPONSES_PHYS + (idx * core::mem::size_of::<VirtioGpuCtrlHeader>()) as u64;
        let resp_transfer_virt = TRANSFER_RESPONSES_VIRT.add(idx);
        
        send_command_queue(0, &[req_transfer_phys], &[core::mem::size_of::<VirtioGpuTransferToHost2d>() as u32], &[resp_transfer_phys], &[24], wait);
        
        if wait && (*resp_transfer_virt).type_ != VIRTIO_GPU_RESP_OK_NODATA {
            crate::debugln!("VirtIO GPU: Transfer failed: {:#x}", (*resp_transfer_virt).type_);
        }

        let req_flush = FLUSH_REQUESTS_VIRT.add(idx);
        core::ptr::write(req_flush, VirtioGpuResourceFlush {
            hdr: VirtioGpuCtrlHeader { type_: VIRTIO_GPU_CMD_RESOURCE_FLUSH, flags: 0, fence_id: 0, ctx_id: 0, ring_idx: 0, padding: [0; 3] },
            r: VirtioGpuRect { x, y, width, height },
            resource_id,
            padding: 0,
        });
        
        let req_flush_phys = FLUSH_REQUESTS_PHYS + (idx * core::mem::size_of::<VirtioGpuResourceFlush>()) as u64;
        let resp_flush_phys = FLUSH_RESPONSES_PHYS + (idx * core::mem::size_of::<VirtioGpuCtrlHeader>()) as u64;
        let resp_flush_virt = FLUSH_RESPONSES_VIRT.add(idx);
        
        send_command_queue(0, &[req_flush_phys], &[core::mem::size_of::<VirtioGpuResourceFlush>() as u32], &[resp_flush_phys], &[24], wait);

        if wait && (*resp_flush_virt).type_ != VIRTIO_GPU_RESP_OK_NODATA {
            crate::debugln!("VirtIO GPU: Flush failed: {:#x}", (*resp_flush_virt).type_);
        }
    }
}
pub fn set_scanout(resource_id: u32, width: u32, height: u32) {
    while GPU_CMD_LOCK.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    unsafe {
        let req_scanout_ptr = GPU_CMD_VIRT as *mut VirtioGpuSetScanout;
        let resp_ptr = (GPU_CMD_VIRT + 1024) as *mut VirtioGpuCtrlHeader;
        core::ptr::write(req_scanout_ptr, VirtioGpuSetScanout {
            hdr: VirtioGpuCtrlHeader { type_: VIRTIO_GPU_CMD_SET_SCANOUT, flags: 0, fence_id: 0, ctx_id: 0, ring_idx: 0, padding: [0; 3] },
            r: VirtioGpuRect { x: 0, y: 0, width, height },
            scanout_id: 0,
            resource_id,
        });
        send_command_queue(0, &[GPU_CMD_PHYS], &[core::mem::size_of::<VirtioGpuSetScanout>() as u32], &[GPU_CMD_PHYS + 1024], &[24], true);
        if (*resp_ptr).type_ != VIRTIO_GPU_RESP_OK_NODATA {
            crate::debugln!("VirtIO GPU: set_scanout failed: {:#x}", (*resp_ptr).type_);
        }
    }
    GPU_CMD_LOCK.store(false, Ordering::Release);
}

/// Double-buffer flip: TRANSFER(back_id, FENCE) → SET_SCANOUT(back_id) → RESOURCE_FLUSH(back_id).
/// Ensures the GPU has finished reading the host buffer (via FENCE) before flipping the scanout,
/// then flushes to the display. The caller is responsible for updating active_resource_id after return.
pub fn flip_and_flush(x: u32, y: u32, w: u32, h: u32, screen_w: u32, screen_h: u32, back_id: u32) {
    unsafe {
        let offset = (y * screen_w + x) * 4;
        let idx = REQ_IDX.fetch_add(1, Ordering::SeqCst) % 64;
        let fence_id = FENCE_COUNTER.fetch_add(1, Ordering::SeqCst) as u64 + 1;

        // Step 1: TRANSFER_TO_HOST_2D with FENCE, blocking — guarantees DMA complete before flip
        let req_transfer = TRANSFER_REQUESTS_VIRT.add(idx);
        core::ptr::write(req_transfer, VirtioGpuTransferToHost2d {
            hdr: VirtioGpuCtrlHeader {
                type_: VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D,
                flags: VIRTIO_GPU_FLAG_FENCE,
                fence_id,
                ctx_id: 0,
                ring_idx: 0,
                padding: [0; 3],
            },
            r: VirtioGpuRect { x, y, width: w, height: h },
            offset: offset as u64,
            resource_id: back_id,
            padding: 0,
        });
        let req_phys = TRANSFER_REQUESTS_PHYS + (idx * core::mem::size_of::<VirtioGpuTransferToHost2d>()) as u64;
        let resp_phys = TRANSFER_RESPONSES_PHYS + (idx * core::mem::size_of::<VirtioGpuCtrlHeader>()) as u64;
        let resp_virt = TRANSFER_RESPONSES_VIRT.add(idx);
        send_command_queue(0, &[req_phys], &[core::mem::size_of::<VirtioGpuTransferToHost2d>() as u32],
                           &[resp_phys], &[24], true);
        if (*resp_virt).type_ != VIRTIO_GPU_RESP_OK_NODATA {
            crate::debugln!("VirtIO GPU: flip TRANSFER failed: {:#x}", (*resp_virt).type_);
        }
    }

    // Step 2: SET_SCANOUT — flip display to back_id now that data is ready
    set_scanout(back_id, screen_w, screen_h);

    unsafe {
        let idx = REQ_IDX.fetch_add(1, Ordering::SeqCst) % 64;
        // Step 3: RESOURCE_FLUSH — present the newly scanned-out resource (non-blocking)
        let req_flush = FLUSH_REQUESTS_VIRT.add(idx);
        core::ptr::write(req_flush, VirtioGpuResourceFlush {
            hdr: VirtioGpuCtrlHeader {
                type_: VIRTIO_GPU_CMD_RESOURCE_FLUSH,
                flags: 0,
                fence_id: 0,
                ctx_id: 0,
                ring_idx: 0,
                padding: [0; 3],
            },
            r: VirtioGpuRect { x, y, width: w, height: h },
            resource_id: back_id,
            padding: 0,
        });
        let req_phys = FLUSH_REQUESTS_PHYS + (idx * core::mem::size_of::<VirtioGpuResourceFlush>()) as u64;
        let resp_phys = FLUSH_RESPONSES_PHYS + (idx * core::mem::size_of::<VirtioGpuCtrlHeader>()) as u64;
        send_command_queue(0, &[req_phys], &[core::mem::size_of::<VirtioGpuResourceFlush>() as u32],
                           &[resp_phys], &[24], false);
    }
}