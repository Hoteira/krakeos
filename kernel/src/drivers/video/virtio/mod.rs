pub mod consts;
pub mod structs;
pub mod queue;
pub mod util;

use alloc::vec::Vec;
use crate::drivers::pci::{PciCapability, PciDevice};
use crate::println;
use self::consts::*;
use self::structs::*;
use self::queue::*;
use self::util::*;

pub static mut COMMON_CFG_ADDR: u64 = 0;

pub fn init() {
    let virtio_opt = crate::drivers::pci::find_device(0x1AF4, 0x1050);

    if virtio_opt.is_none() {
        println!("VirtIO GPU: Device not found.");
        return;
    }

    let virtio = virtio_opt.unwrap();
    println!("VirtIO GPU: Found device at Bus {}, Device {}, Func {}", virtio.bus, virtio.device, virtio.function);

    if virtio.enable_bus_mastering() {
        println!("VirtIO GPU: Bus mastering enabled.");
    } else {
        println!("VirtIO GPU: Failed to enable bus mastering.");
    }

    let caps = virtio.list_capabilities();
    let virtio_caps = parse_virtio_caps(&virtio, &caps);

    let mut common_cfg_ptr: *mut u8 = core::ptr::null_mut();
    let mut notify_base: u64 = 0;
    let mut notify_multiplier: u32 = 0;

    for cap in virtio_caps {
        if cap.cfg_type == VIRTIO_CAP_COMMON {
            if let Some(bar_base) = virtio.get_bar(cap.bar) {
                let addr = (bar_base as u64) + (cap.offset as u64);
                common_cfg_ptr = addr as *mut u8;
                unsafe { COMMON_CFG_ADDR = addr; }
                println!("VirtIO GPU: Common Config found at BAR {} offset {:#x} -> Phys {:#x}", cap.bar, cap.offset, addr);
            }
        } else if cap.cfg_type == VIRTIO_CAP_NOTIFY {
             if let Some(bar_base) = virtio.get_bar(cap.bar) {
                 notify_base = (bar_base as u64) + (cap.offset as u64);
                 notify_multiplier = virtio.read_capability_data(cap.offset as u8, 16);
             }
        }
    }

    if common_cfg_ptr.is_null() {
        println!("VirtIO GPU: Could not find Common Config capability.");
        return;
    }

    unsafe {
        write_common_u8(common_cfg_ptr, OFF_DEVICE_STATUS, 0);

        let mut status = read_common_u8(common_cfg_ptr, OFF_DEVICE_STATUS);
        status |= STATUS_ACKNOWLEDGE;
        write_common_u8(common_cfg_ptr, OFF_DEVICE_STATUS, status);

        status |= STATUS_DRIVER;
        write_common_u8(common_cfg_ptr, OFF_DEVICE_STATUS, status);

        status |= STATUS_FEATURES_OK;
        write_common_u8(common_cfg_ptr, OFF_DEVICE_STATUS, status);

        let final_status = read_common_u8(common_cfg_ptr, OFF_DEVICE_STATUS);
        if (final_status & STATUS_FEATURES_OK) == 0 {
            println!("VirtIO GPU: Features negotiation failed.");
            return;
        }

        setup_queue(common_cfg_ptr, 0, notify_base, notify_multiplier);
        setup_queue(common_cfg_ptr, 1, notify_base, notify_multiplier);

        status |= STATUS_DRIVER_OK;
        write_common_u8(common_cfg_ptr, OFF_DEVICE_STATUS, status);

        println!("VirtIO GPU: Initialization complete (Driver OK). Status: {:#x}", read_common_u8(common_cfg_ptr, OFF_DEVICE_STATUS));
    }
}

pub fn parse_virtio_caps(pci_device: &PciDevice, caps: &[PciCapability]) -> Vec<VirtioPciCap> {
    let mut virtio_caps = Vec::new();

    for cap in caps.iter() {
        if cap.id != 0x09 {
            continue;
        }

        let cfg_type = pci_device.read_u8(cap.offset as u32 + 3);
        let bar      = pci_device.read_u8(cap.offset as u32 + 4);
        let offset   = pci_device.read_u32(cap.offset as u32 + 8);
        let length   = pci_device.read_u32(cap.offset as u32 + 12);

        virtio_caps.push(VirtioPciCap { cfg_type, bar, offset, length });
    }

    virtio_caps
}

pub unsafe fn start_gpu(width: u32, height: u32, phys_buffer: u64) {
    let req_info = VirtioGpuCtrlHeader {
        type_: VIRTIO_GPU_CMD_GET_DISPLAY_INFO,
        flags: 0,
        fence_id: 0,
        ctx_id: 0,
        ring_idx: 0,
        padding: [0; 3],
    };
    let mut resp_info: VirtioGpuRespDisplayInfo = core::mem::zeroed();

    send_command(
        0, // Control Queue
        &req_info as *const _ as u64,
        core::mem::size_of_val(&req_info) as u32,
        &resp_info as *const _ as u64,
        core::mem::size_of_val(&resp_info) as u32,
    );

    println!("VirtIO GPU: Display Info Type: {:#x}", resp_info.hdr.type_);

    let req_create = VirtioGpuResourceCreate2d {
        hdr: VirtioGpuCtrlHeader {
            type_: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_idx: 0,
            padding: [0; 3],
        },
        resource_id: 1,
        format: 1,
        width,
        height,
    };
    let mut resp_create: VirtioGpuCtrlHeader = core::mem::zeroed();

    send_command(
        0, // Control Queue
        &req_create as *const _ as u64,
        core::mem::size_of_val(&req_create) as u32,
        &resp_create as *const _ as u64,
        core::mem::size_of_val(&resp_create) as u32,
    );
    println!("VirtIO GPU: Create 2D Resp: {:#x}", resp_create.type_);

    // 3. Attach Backing
    // We need a contiguous struct of (AttachBacking + MemEntry)
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
            resource_id: 1,
            nr_entries: 1,
        },
        entry: VirtioGpuMemEntry {
            addr: phys_buffer,
            length: width * height * 4,
            padding: 0,
        },
    };
    let mut resp_attach: VirtioGpuCtrlHeader = core::mem::zeroed();

    send_command(
        0, // Control Queue
        &req_attach as *const _ as u64,
        core::mem::size_of_val(&req_attach) as u32,
        &resp_attach as *const _ as u64,
        core::mem::size_of_val(&resp_attach) as u32,
    );
    println!("VirtIO GPU: Attach Backing Resp: {:#x}", resp_attach.type_);

    // 4. Set Scanout
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
    let mut resp_scanout: VirtioGpuCtrlHeader = core::mem::zeroed();

    send_command(
        0, // Control Queue
        &req_scanout as *const _ as u64,
        core::mem::size_of_val(&req_scanout) as u32,
        &resp_scanout as *const _ as u64,
        core::mem::size_of_val(&resp_scanout) as u32,
    );
    println!("VirtIO GPU: Set Scanout Resp: {:#x}", resp_scanout.type_);

    println!("VirtIO GPU: Started. Scanout set to Resource 1.");
}

pub unsafe fn flush(x: u32, y: u32, width: u32, height: u32, screen_width: u32) {
    let offset = (y as u64 * screen_width as u64 + x as u64) * 4;
    
    let req_transfer = VirtioGpuTransferToHost2d {
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
        resource_id: 1,
        padding: 0,
    };
    let mut resp_transfer: VirtioGpuCtrlHeader = core::mem::zeroed();

    send_command(
        0, // Control Queue
        &req_transfer as *const _ as u64,
        core::mem::size_of_val(&req_transfer) as u32,
        &resp_transfer as *const _ as u64,
        core::mem::size_of_val(&resp_transfer) as u32,
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
        r: VirtioGpuRect { x, y, width, height },
        resource_id: 1,
        padding: 0,
    };
    let mut resp_flush: VirtioGpuCtrlHeader = core::mem::zeroed();

    send_command(
        0, // Control Queue
        &req_flush as *const _ as u64,
        core::mem::size_of_val(&req_flush) as u32,
        &resp_flush as *const _ as u64,
        core::mem::size_of_val(&resp_flush) as u32,
    );
}

pub unsafe fn setup_cursor(width: u32, height: u32, phys_buffer: u64, hot_x: u32, hot_y: u32) {
    println!("VirtIO Debug: setup_cursor called with w={} h={} phys={:#x} hot={},{}", width, height, phys_buffer, hot_x, hot_y);
    
    let cursor_w = 64;
    let cursor_h = 64;
    println!("VirtIO Debug: Forcing cursor size to {}x{}", cursor_w, cursor_h);

    // 1. Create Cursor Resource (ID 2)
    println!("VirtIO Debug: Sending RESOURCE_CREATE_2D (ID 2)...");
    let req_create = VirtioGpuResourceCreate2d {
        hdr: VirtioGpuCtrlHeader {
            type_: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_idx: 0,
            padding: [0; 3],
        },
        resource_id: 2, // ID 2 for Cursor
        format: 1,      // B8G8R8A8
        width: cursor_w,
        height: cursor_h,
    };
    let mut resp_create: VirtioGpuCtrlHeader = core::mem::zeroed();
    
    send_command(
        0, // Control Queue
        &req_create as *const _ as u64,
        core::mem::size_of_val(&req_create) as u32,
        &resp_create as *const _ as u64,
        core::mem::size_of_val(&resp_create) as u32,
    );
    println!("VirtIO Debug: Create 2D Resp Type: {:#x}", resp_create.type_);

    if resp_create.type_ != VIRTIO_GPU_RESP_OK_NODATA {
        println!("VirtIO Debug: Create 2D Failed!");
        return;
    }

    // 2. Attach Backing
    println!("VirtIO Debug: Sending RESOURCE_ATTACH_BACKING (ID 2, Addr {:#x})...", phys_buffer);
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
            resource_id: 2,
            nr_entries: 1,
        },
        entry: VirtioGpuMemEntry {
            addr: phys_buffer,
            length: cursor_w * cursor_h * 4, // 16KB
            padding: 0,
        },
    };
    let mut resp_attach: VirtioGpuCtrlHeader = core::mem::zeroed();

    send_command(
        0, // Control Queue
        &req_attach as *const _ as u64,
        core::mem::size_of_val(&req_attach) as u32,
        &resp_attach as *const _ as u64,
        core::mem::size_of_val(&resp_attach) as u32,
    );
    println!("VirtIO Debug: Attach Backing Resp Type: {:#x}", resp_attach.type_);

    // 3. Transfer Data (Upload)
    println!("VirtIO Debug: Sending TRANSFER_TO_HOST_2D (ID 2)...");
    let req_transfer = VirtioGpuTransferToHost2d {
        hdr: VirtioGpuCtrlHeader {
            type_: VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_idx: 0,
            padding: [0; 3],
        },
        r: VirtioGpuRect { x: 0, y: 0, width: cursor_w, height: cursor_h },
        offset: 0,
        resource_id: 2,
        padding: 0,
    };
    let mut resp_transfer: VirtioGpuCtrlHeader = core::mem::zeroed();

    send_command(
        0, // Control Queue
        &req_transfer as *const _ as u64,
        core::mem::size_of_val(&req_transfer) as u32,
        &resp_transfer as *const _ as u64,
        core::mem::size_of_val(&resp_transfer) as u32,
    );
    println!("VirtIO Debug: Transfer Resp Type: {:#x}", resp_transfer.type_);

    // 3.5. Flush Cursor Resource
    println!("VirtIO Debug: Sending RESOURCE_FLUSH (ID 2)...");
    let req_flush = VirtioGpuResourceFlush {
        hdr: VirtioGpuCtrlHeader {
            type_: VIRTIO_GPU_CMD_RESOURCE_FLUSH,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_idx: 0,
            padding: [0; 3],
        },
        r: VirtioGpuRect { x: 0, y: 0, width: cursor_w, height: cursor_h },
        resource_id: 2,
        padding: 0,
    };
    let mut resp_flush: VirtioGpuCtrlHeader = core::mem::zeroed();

    send_command(
        0, // Control Queue
        &req_flush as *const _ as u64,
        core::mem::size_of_val(&req_flush) as u32,
        &resp_flush as *const _ as u64,
        core::mem::size_of_val(&resp_flush) as u32,
    );
    println!("VirtIO Debug: Flush Resp Type: {:#x}", resp_flush.type_);

    // 3.6. Test Disable Cursor (Verify Command Struct)
    println!("VirtIO Debug: Testing UPDATE_CURSOR (Disable ID 0)...");
    let req_disable = VirtioGpuUpdateCursor {
        hdr: VirtioGpuCtrlHeader {
            type_: VIRTIO_GPU_CMD_UPDATE_CURSOR,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_idx: 0,
            padding: [0; 3],
        },
        pos: VirtioGpuCursorPos {
            scanout_id: 0,
            x: 0,
            y: 0,
            padding: 0,
        },
        resource_id: 0, // DISABLE
        hot_x: 0,
        hot_y: 0,
        padding: 0,
    };
    let mut resp_disable: VirtioGpuCtrlHeader = core::mem::zeroed();
    send_command(
        0, // REVERTED TO QUEUE 0
        &req_disable as *const _ as u64,
        core::mem::size_of_val(&req_disable) as u32,
        &resp_disable as *const _ as u64,
        core::mem::size_of_val(&resp_disable) as u32,
    );
    println!("VirtIO Debug: Disable Cursor Resp: {:#x} (Struct Size: {})", resp_disable.type_, core::mem::size_of_val(&req_disable));

    // 4. Update Cursor (Enable it)
    println!("VirtIO Debug: Sending UPDATE_CURSOR (ID 2, Pos 500,500)...");
    let req_update = VirtioGpuUpdateCursor {
        hdr: VirtioGpuCtrlHeader {
            type_: VIRTIO_GPU_CMD_UPDATE_CURSOR,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_idx: 0,
            padding: [0; 3],
        },
        pos: VirtioGpuCursorPos {
            scanout_id: 0,
            x: 500,
            y: 500,
            padding: 0,
        },
        resource_id: 2,
        hot_x,
        hot_y,
        padding: 0,
    };
    let mut resp_update: VirtioGpuCtrlHeader = core::mem::zeroed();

    send_command(
        0, // REVERTED TO QUEUE 0
        &req_update as *const _ as u64,
        core::mem::size_of_val(&req_update) as u32,
        &resp_update as *const _ as u64,
        core::mem::size_of_val(&resp_update) as u32,
    );
    
    println!("VirtIO Debug: Update Resp Type: {:#x}", resp_update.type_);
    match resp_update.type_ {
        VIRTIO_GPU_RESP_OK_NODATA => println!("VirtIO Cursor Update: OK"),
        VIRTIO_GPU_RESP_ERR_UNSPEC => println!("VirtIO Cursor Update: ERR_UNSPEC (0x1200) - Generic Error (Check memory/size/alignment)"),
        VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY => println!("VirtIO Cursor Update: ERR_OUT_OF_MEMORY"),
        VIRTIO_GPU_RESP_ERR_INVALID_SCANOUT_ID => println!("VirtIO Cursor Update: ERR_INVALID_SCANOUT_ID"),
        VIRTIO_GPU_RESP_ERR_INVALID_RESOURCE_ID => println!("VirtIO Cursor Update: ERR_INVALID_RESOURCE_ID"),
        VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER => println!("VirtIO Cursor Update: ERR_INVALID_PARAMETER"),
        _ => println!("VirtIO Cursor Update: Unknown Response {:#x}", resp_update.type_),
    }
    
    println!("VirtIO GPU: Hardware Cursor setup complete.");
}

pub unsafe fn test_cursor(width: u32, height: u32, phys_buffer: u64) {
    println!("VirtIO Debug: Starting Cursor Test...");

    // 1. Create
    let req_create = VirtioGpuResourceCreate2d {
        hdr: VirtioGpuCtrlHeader {
            type_: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_idx: 0,
            padding: [0; 3],
        },
        resource_id: 2,
        format: 1, // B8G8R8A8
        width,
        height,
    };
    let mut resp_create: VirtioGpuCtrlHeader = core::mem::zeroed();
    
    send_command(
        0, // Control Queue
        &req_create as *const _ as u64,
        core::mem::size_of_val(&req_create) as u32,
        &resp_create as *const _ as u64,
        core::mem::size_of_val(&resp_create) as u32,
    );
    println!("VirtIO Debug: Create(ID=2) -> {:#x}", resp_create.type_);

    // 2. Attach
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
            resource_id: 2,
            nr_entries: 1,
        },
        entry: VirtioGpuMemEntry {
            addr: phys_buffer,
            length: width * height * 4,
            padding: 0,
        },
    };
    let mut resp_attach: VirtioGpuCtrlHeader = core::mem::zeroed();

    send_command(
        0, // Control Queue
        &req_attach as *const _ as u64,
        core::mem::size_of_val(&req_attach) as u32,
        &resp_attach as *const _ as u64,
        core::mem::size_of_val(&resp_attach) as u32,
    );
    println!("VirtIO Debug: Attach(Addr={:#x}) -> {:#x}", phys_buffer, resp_attach.type_);

    // 3. Transfer (Upload)
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
        resource_id: 2,
        padding: 0,
    };
    let mut resp_transfer: VirtioGpuCtrlHeader = core::mem::zeroed();

    send_command(
        0, // Control Queue
        &req_transfer as *const _ as u64,
        core::mem::size_of_val(&req_transfer) as u32,
        &resp_transfer as *const _ as u64,
        core::mem::size_of_val(&resp_transfer) as u32,
    );
    println!("VirtIO Debug: Transfer -> {:#x}", resp_transfer.type_);
    
    println!("VirtIO Debug: UpdateCursor Struct Size: {}", core::mem::size_of::<VirtioGpuUpdateCursor>());

    // TEST 0: Move Cursor (0x0301) - Should verify struct layout
    let req_move = VirtioGpuUpdateCursor {
        hdr: VirtioGpuCtrlHeader {
            type_: VIRTIO_GPU_CMD_MOVE_CURSOR,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_idx: 0,
            padding: [0; 3],
        },
        pos: VirtioGpuCursorPos {
            scanout_id: 0,
            x: 100,
            y: 100,
            padding: 0,
        },
        resource_id: 0,
        hot_x: 0,
        hot_y: 0,
        padding: 0,
    };
    let mut resp_move: VirtioGpuCtrlHeader = core::mem::zeroed();
    send_command(
        0, // REVERTED TO QUEUE 0
        &req_move as *const _ as u64,
        core::mem::size_of_val(&req_move) as u32,
        &resp_move as *const _ as u64,
        core::mem::size_of_val(&resp_move) as u32,
    );
    println!("VirtIO Debug: Move(0x0301) -> {:#x}", resp_move.type_);

    // TEST 1: Disable Cursor (Resource ID 0)
    let req_disable = VirtioGpuUpdateCursor {
        hdr: VirtioGpuCtrlHeader {
            type_: VIRTIO_GPU_CMD_UPDATE_CURSOR,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_idx: 0,
            padding: [0; 3],
        },
        pos: VirtioGpuCursorPos {
            scanout_id: 0,
            x: 0,
            y: 0,
            padding: 0,
        },
        resource_id: 0, // Disable
        hot_x: 0,
        hot_y: 0,
        padding: 0,
    };
    let mut resp_disable: VirtioGpuCtrlHeader = core::mem::zeroed();
    send_command(
        0, // REVERTED TO QUEUE 0
        &req_disable as *const _ as u64,
        core::mem::size_of_val(&req_disable) as u32,
        &resp_disable as *const _ as u64,
        core::mem::size_of_val(&resp_disable) as u32,
    );
    println!("VirtIO Debug: Disable(ID=0) -> {:#x}", resp_disable.type_);

    // TEST 2: Enable Cursor (Resource ID 2)
    let req_update = VirtioGpuUpdateCursor {
        hdr: VirtioGpuCtrlHeader {
            type_: VIRTIO_GPU_CMD_UPDATE_CURSOR,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_idx: 0,
            padding: [0; 3],
        },
        pos: VirtioGpuCursorPos {
            scanout_id: 0,
            x: 500,
            y: 500,
            padding: 0,
        },
        resource_id: 2,
        hot_x: 0,
        hot_y: 0,
        padding: 0,
    };
    let mut resp_update: VirtioGpuCtrlHeader = core::mem::zeroed();

    send_command(
        0, // REVERTED TO QUEUE 0
        &req_update as *const _ as u64,
        core::mem::size_of_val(&req_update) as u32,
        &resp_update as *const _ as u64,
        core::mem::size_of_val(&resp_update) as u32,
    );
    println!("VirtIO Debug: Update(ID=2, Pos=500,500) -> {:#x}", resp_update.type_);
}

pub unsafe fn move_cursor(x: u32, y: u32) {
    let req_move = VirtioGpuUpdateCursor {
        hdr: VirtioGpuCtrlHeader {
            type_: VIRTIO_GPU_CMD_MOVE_CURSOR,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_idx: 0,
            padding: [0; 3],
        },
        pos: VirtioGpuCursorPos {
            scanout_id: 0,
            x,
            y,
            padding: 0,
        },
        resource_id: 0, // Not used for move
        hot_x: 0,
        hot_y: 0,
        padding: 0,
    };
    let mut resp_move: VirtioGpuCtrlHeader = core::mem::zeroed();

    send_command(
        0, // REVERTED TO QUEUE 0
        &req_move as *const _ as u64,
        core::mem::size_of_val(&req_move) as u32,
        &resp_move as *const _ as u64,
        core::mem::size_of_val(&resp_move) as u32,
    );
    
    if resp_move.type_ != VIRTIO_GPU_RESP_OK_NODATA {
        println!("VirtIO Debug: Move Cursor Failed! Type: {:#x}", resp_move.type_);
    }
}