
use crate::debugln;

use super::consts::*;
use super::structs::*;
use super::queue::send_command_queue;

pub fn setup_cursor(phys_ptr: u64, width: u32, height: u32, x: u32, y: u32) {
    let cursor_id = 3;

    
    let req_create = VirtioGpuResourceCreate2d {
        hdr: VirtioGpuCtrlHeader {
            type_: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_idx: 0,
            padding: [0; 3],
        },
        resource_id: cursor_id,
        format: VIRTIO_GPU_FORMAT_B8G8R8A8_UNORM,
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
    );

    
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
            resource_id: cursor_id,
            nr_entries: 1,
        },
        entry: VirtioGpuMemEntry {
            addr: phys_ptr,
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
    );

    
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
        resource_id: cursor_id,
        padding: 0,
    };
    let resp_transfer: VirtioGpuCtrlHeader = unsafe { core::mem::zeroed() };

    send_command_queue(
        0,
        &[&req_transfer as *const _ as u64],
        &[core::mem::size_of_val(&req_transfer) as u32],
        &[&resp_transfer as *const _ as u64],
        &[core::mem::size_of_val(&resp_transfer) as u32],
    );

    
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
            x,
            y,
            padding: 0,
        },
        resource_id: cursor_id,
        hot_x: 0,
        hot_y: 0,
        padding: 0,
    };

    
    send_command_queue(
        1,
        &[&req_update as *const _ as u64],
        &[core::mem::size_of_val(&req_update) as u32],
        &[], 
        &[],
    );
    debugln!("VirtIO GPU: Hardware Cursor Setup & Update Sent (Queue 1).");
}

pub fn move_cursor(x: u32, y: u32) {
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
        resource_id: 0, 
        hot_x: 0,
        hot_y: 0,
        padding: 0,
    };

    
    send_command_queue(
        1,
        &[&req_move as *const _ as u64],
        &[core::mem::size_of_val(&req_move) as u32],
        &[], 
        &[],
    );
}
