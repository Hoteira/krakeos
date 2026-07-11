const VIRTIO_MAGIC: u32 = 0x74726976;
const VIRTIO_DEV_GPU: u32 = 16;

const VIRTIO_STATUS_ACKNOWLEDGE: u32 = 1;
const VIRTIO_STATUS_DRIVER: u32 = 2;
const VIRTIO_STATUS_DRIVER_OK: u32 = 4;
const VIRTIO_STATUS_FEATURES_OK: u32 = 8;

const QUEUE_SIZE: usize = 16;

#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C, align(2))]
#[derive(Clone, Copy)]
struct VirtqAvail {
    flags: u16,
    idx: u16,
    ring: [u16; QUEUE_SIZE],
    used_event: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

#[repr(C, align(4))]
struct VirtqUsed {
    flags: u16,
    idx: u16,
    ring: [VirtqUsedElem; QUEUE_SIZE],
    avail_event: u16,
}

#[repr(C, align(4096))]
struct VirtQueue {
    desc: [VirtqDesc; QUEUE_SIZE],
    avail: VirtqAvail,
    _pad: [u8; 3802],
    used: VirtqUsed,
}

// GPU Command Types
const VIRTIO_GPU_CMD_RESOURCE_CREATE_2D: u32 = 0x0101;
const VIRTIO_GPU_CMD_SET_SCANOUT: u32 = 0x0103;
const VIRTIO_GPU_CMD_RESOURCE_FLUSH: u32 = 0x0104;
const VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D: u32 = 0x0105;
const VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING: u32 = 0x0106;
const VIRTIO_GPU_CMD_UPDATE_CURSOR: u32 = 0x0300;
const VIRTIO_GPU_CMD_MOVE_CURSOR: u32 = 0x0301;

const VIRTIO_GPU_FORMAT_B8G8R8A8_UNORM: u32 = 1;

#[repr(C)]
struct VirtioGpuCtrlHdr {
    type_: u32,
    flags: u32,
    fence_id: u64,
    ctx_id: u32,
    padding: u32,
}

#[repr(C)]
struct VirtioGpuResourceCreate2d {
    hdr: VirtioGpuCtrlHdr,
    resource_id: u32,
    format: u32,
    width: u32,
    height: u32,
}

#[repr(C)]
struct VirtioGpuResourceAttachBacking {
    hdr: VirtioGpuCtrlHdr,
    resource_id: u32,
    nr_entries: u32,
}

#[repr(C)]
struct VirtioGpuMemEntry {
    addr: u64,
    length: u32,
    padding: u32,
}

#[repr(C)]
struct VirtioGpuSetScanout {
    hdr: VirtioGpuCtrlHdr,
    r_x: u32,
    r_y: u32,
    r_width: u32,
    r_height: u32,
    scanout_id: u32,
    resource_id: u32,
}

#[repr(C)]
struct VirtioGpuTransferToHost2d {
    hdr: VirtioGpuCtrlHdr,
    r_x: u32,
    r_y: u32,
    r_width: u32,
    r_height: u32,
    offset: u64,
    resource_id: u32,
    padding: u32,
}

#[repr(C)]
struct VirtioGpuResourceFlush {
    hdr: VirtioGpuCtrlHdr,
    r_x: u32,
    r_y: u32,
    r_width: u32,
    r_height: u32,
    resource_id: u32,
    padding: u32,
}

#[repr(C)]
struct VirtioGpuCursorPos {
    scanout_id: u32,
    x: u32,
    y: u32,
    padding: u32,
}

#[repr(C)]
struct VirtioGpuUpdateCursor {
    hdr: VirtioGpuCtrlHdr,
    pos: VirtioGpuCursorPos,
    resource_id: u32,
    hot_x: u32,
    hot_y: u32,
    padding: u32,
}

static mut VIRTIO_GPU_BASE: usize = 0;
static mut VIRTQ_MEM: [u8; 16384] = [0; 16384];
static mut VIRTQ_PTR: *mut VirtQueue = core::ptr::null_mut();

pub const FB_WIDTH: u32 = 1024;
pub const FB_HEIGHT: u32 = 576;
// We align it to 4096 bytes so mapping it is easy
#[repr(align(4096))]
pub struct Framebuffer(pub [u32; (FB_WIDTH * FB_HEIGHT) as usize]);
pub static mut FB_MEM: Framebuffer = Framebuffer([0xFF000000; (FB_WIDTH * FB_HEIGHT) as usize]); 

static mut LAST_USED_IDX: u16 = 0;

// --- Hardware cursor (cursorq = queue 1, resource id 2) ---
pub const CURSOR_SIZE: u32 = 64;
const CURSOR_RESOURCE_ID: u32 = 2;

#[repr(align(4096))]
struct CursorBuf(#[allow(dead_code)] [u32; (CURSOR_SIZE * CURSOR_SIZE) as usize]); // accessed via addr_of (DMA)
static mut CURSOR_MEM: CursorBuf = CursorBuf([0; (CURSOR_SIZE * CURSOR_SIZE) as usize]);
static mut CURSOR_VQ_MEM: [u8; 16384] = [0; 16384];
static mut CURSOR_VQ_PTR: *mut VirtQueue = core::ptr::null_mut();
static mut CURSOR_LAST_USED: u16 = 0;
static mut CURSOR_QUEUE_OK: bool = false; // cursorq initialized
static mut CURSOR_RES_OK: bool = false;   // resource created + image uploaded
static mut CURSOR_X: u32 = 0;
static mut CURSOR_Y: u32 = 0;

pub fn init() -> bool {
    let mut base = 0x10001000;
    let mut found = false;
    for _ in 0..8 {
        unsafe {
            let magic = core::ptr::read_volatile(base as *const u32);
            let dev_id = core::ptr::read_volatile((base + 0x008) as *const u32);
            if magic == VIRTIO_MAGIC && dev_id == VIRTIO_DEV_GPU {
                found = true;
                VIRTIO_GPU_BASE = base;
                break;
            }
        }
        base += 0x1000;
    }

    if !found {
        return false;
    }

    unsafe {
        let base = VIRTIO_GPU_BASE;
        core::ptr::write_volatile((base + 0x070) as *mut u32, 0);
        let mut status = VIRTIO_STATUS_ACKNOWLEDGE;
        core::ptr::write_volatile((base + 0x070) as *mut u32, status);
        status |= VIRTIO_STATUS_DRIVER;
        core::ptr::write_volatile((base + 0x070) as *mut u32, status);
        
        core::ptr::write_volatile((base + 0x020) as *mut u32, 0);
        
        status |= VIRTIO_STATUS_FEATURES_OK;
        core::ptr::write_volatile((base + 0x070) as *mut u32, status);
        
        core::ptr::write_volatile((base + 0x030) as *mut u32, 0);
        
        let mem_addr = core::ptr::addr_of_mut!(VIRTQ_MEM) as usize;
        let aligned_addr = (mem_addr + 4095) & !4095;
        VIRTQ_PTR = aligned_addr as *mut VirtQueue;
        core::ptr::write_bytes(VIRTQ_PTR as *mut u8, 0, 4096);
        
        core::ptr::write_volatile((base + 0x028) as *mut u32, 4096);
        core::ptr::write_volatile((base + 0x038) as *mut u32, QUEUE_SIZE as u32);
        core::ptr::write_volatile((base + 0x03C) as *mut u32, 4096);
        let pfn = (aligned_addr as u32) / 4096;
        core::ptr::write_volatile((base + 0x040) as *mut u32, pfn);

        // Queue 1 (cursorq) for the hardware cursor
        core::ptr::write_volatile((base + 0x030) as *mut u32, 1);
        let cursor_max = core::ptr::read_volatile((base + 0x034) as *const u32);
        if cursor_max >= QUEUE_SIZE as u32 {
            let cmem = core::ptr::addr_of_mut!(CURSOR_VQ_MEM) as usize;
            let caligned = (cmem + 4095) & !4095;
            CURSOR_VQ_PTR = caligned as *mut VirtQueue;
            core::ptr::write_bytes(CURSOR_VQ_PTR as *mut u8, 0, 4096);
            core::ptr::write_volatile((base + 0x038) as *mut u32, QUEUE_SIZE as u32);
            core::ptr::write_volatile((base + 0x03C) as *mut u32, 4096);
            core::ptr::write_volatile((base + 0x040) as *mut u32, (caligned as u32) / 4096);
            CURSOR_QUEUE_OK = true;
        }

        status |= VIRTIO_STATUS_DRIVER_OK;
        core::ptr::write_volatile((base + 0x070) as *mut u32, status);
        
        crate::println!("VirtIO GPU initialized at 0x{:X}", base);
    }
    
    setup_fb();
    true
}

fn send_cmd<T>(cmd: &T, resp_size: u32) {
    unsafe {
        let base = VIRTIO_GPU_BASE;
        
        let p_req = 0;
        let p_resp = 1;
        
        (*VIRTQ_PTR).desc[p_req].addr = cmd as *const T as u64;
        (*VIRTQ_PTR).desc[p_req].len = core::mem::size_of::<T>() as u32;
        (*VIRTQ_PTR).desc[p_req].flags = 1;
        (*VIRTQ_PTR).desc[p_req].next = p_resp as u16;
        
        static mut RESP_BUF: [u8; 256] = [0; 256];
        (*VIRTQ_PTR).desc[p_resp].addr = core::ptr::addr_of_mut!(RESP_BUF) as u64;
        (*VIRTQ_PTR).desc[p_resp].len = resp_size;
        (*VIRTQ_PTR).desc[p_resp].flags = 2;
        (*VIRTQ_PTR).desc[p_resp].next = 0;
        (*VIRTQ_PTR).avail.ring[((*VIRTQ_PTR).avail.idx % QUEUE_SIZE as u16) as usize] = p_req as u16;
        // crate::println!("virtio_gpu: send_cmd: VIRTQ_PTR = {:p}, idx = {}", VIRTQ_PTR, (*VIRTQ_PTR).avail.idx);
        core::arch::asm!("fence rw, rw");
        (*VIRTQ_PTR).avail.idx = (*VIRTQ_PTR).avail.idx.wrapping_add(1);
        core::arch::asm!("fence rw, rw");
        
        core::ptr::write_volatile((base + 0x050) as *mut u32, 0);
        
        while LAST_USED_IDX == core::ptr::read_volatile(core::ptr::addr_of!((*VIRTQ_PTR).used.idx)) {
            core::arch::asm!("nop");
            core::arch::asm!("fence r, rw");
        }
        LAST_USED_IDX = LAST_USED_IDX.wrapping_add(1);
    }
}

fn setup_fb() {
    let res_create = VirtioGpuResourceCreate2d {
        hdr: VirtioGpuCtrlHdr { type_: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D, flags: 0, fence_id: 0, ctx_id: 0, padding: 0 },
        resource_id: 1,
        format: VIRTIO_GPU_FORMAT_B8G8R8A8_UNORM,
        width: FB_WIDTH,
        height: FB_HEIGHT,
    };
    send_cmd(&res_create, 24);
    
    #[repr(C)]
    struct AttachBackingCmd {
        hdr: VirtioGpuResourceAttachBacking,
        entry: VirtioGpuMemEntry,
    }
    
    let attach = AttachBackingCmd {
        hdr: VirtioGpuResourceAttachBacking {
            hdr: VirtioGpuCtrlHdr { type_: VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING, flags: 0, fence_id: 0, ctx_id: 0, padding: 0 },
            resource_id: 1,
            nr_entries: 1,
        },
        entry: VirtioGpuMemEntry {
            addr: core::ptr::addr_of!(FB_MEM) as u64,
            length: (FB_WIDTH * FB_HEIGHT * 4) as u32,
            padding: 0,
        }
    };
    send_cmd(&attach, 24);
    
    let scanout = VirtioGpuSetScanout {
        hdr: VirtioGpuCtrlHdr { type_: VIRTIO_GPU_CMD_SET_SCANOUT, flags: 0, fence_id: 0, ctx_id: 0, padding: 0 },
        r_x: 0,
        r_y: 0,
        r_width: FB_WIDTH,
        r_height: FB_HEIGHT,
        scanout_id: 0,
        resource_id: 1,
    };
    send_cmd(&scanout, 24);
}

pub fn flush_rect(x: u32, y: u32, w: u32, h: u32) {
    let transfer = VirtioGpuTransferToHost2d {
        hdr: VirtioGpuCtrlHdr { type_: VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D, flags: 0, fence_id: 0, ctx_id: 0, padding: 0 },
        r_x: x,
        r_y: y,
        r_width: w,
        r_height: h,
        offset: ((y * FB_WIDTH + x) * 4) as u64,
        resource_id: 1,
        padding: 0,
    };
    send_cmd(&transfer, 24);
    
    let flush = VirtioGpuResourceFlush {
        hdr: VirtioGpuCtrlHdr { type_: VIRTIO_GPU_CMD_RESOURCE_FLUSH, flags: 0, fence_id: 0, ctx_id: 0, padding: 0 },
        r_x: x,
        r_y: y,
        r_width: w,
        r_height: h,
        resource_id: 1,
        padding: 0,
    };
    send_cmd(&flush, 24);
}

pub fn get_fb_ptr() -> usize {
    core::ptr::addr_of!(FB_MEM) as usize
}

// Cursorq commands have no response payload; the device just consumes the
// buffer and pushes it to the used ring.
fn send_cursor_cmd(cmd: &VirtioGpuUpdateCursor) {
    unsafe {
        if !CURSOR_QUEUE_OK { return; }
        let base = VIRTIO_GPU_BASE;

        (*CURSOR_VQ_PTR).desc[0].addr = cmd as *const _ as u64;
        (*CURSOR_VQ_PTR).desc[0].len = core::mem::size_of::<VirtioGpuUpdateCursor>() as u32;
        (*CURSOR_VQ_PTR).desc[0].flags = 0;
        (*CURSOR_VQ_PTR).desc[0].next = 0;

        let avail_idx = (*CURSOR_VQ_PTR).avail.idx;
        (*CURSOR_VQ_PTR).avail.ring[(avail_idx % QUEUE_SIZE as u16) as usize] = 0;
        core::arch::asm!("fence rw, rw");
        (*CURSOR_VQ_PTR).avail.idx = avail_idx.wrapping_add(1);
        core::arch::asm!("fence rw, rw");

        core::ptr::write_volatile((base + 0x050) as *mut u32, 1);

        while CURSOR_LAST_USED == core::ptr::read_volatile(core::ptr::addr_of!((*CURSOR_VQ_PTR).used.idx)) {
            core::arch::asm!("fence r, rw");
        }
        CURSOR_LAST_USED = CURSOR_LAST_USED.wrapping_add(1);
    }
}

pub fn cursor_available() -> bool {
    unsafe { CURSOR_QUEUE_OK }
}

pub fn is_ready() -> bool {
    unsafe { VIRTIO_GPU_BASE != 0 && !VIRTQ_PTR.is_null() }
}

/// Upload a CURSOR_SIZE x CURSOR_SIZE BGRA image and show the hardware
/// cursor. Returns the number of bytes consumed (0 if no cursorq).
pub fn set_cursor_image(data: &[u8]) -> usize {
    unsafe {
        if !CURSOR_QUEUE_OK { return 0; }

        let max = (CURSOR_SIZE * CURSOR_SIZE * 4) as usize;
        let n = data.len().min(max);
        core::ptr::copy_nonoverlapping(
            data.as_ptr(),
            core::ptr::addr_of_mut!(CURSOR_MEM) as *mut u8,
            n,
        );

        if !CURSOR_RES_OK {
            let res_create = VirtioGpuResourceCreate2d {
                hdr: VirtioGpuCtrlHdr { type_: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D, flags: 0, fence_id: 0, ctx_id: 0, padding: 0 },
                resource_id: CURSOR_RESOURCE_ID,
                format: VIRTIO_GPU_FORMAT_B8G8R8A8_UNORM,
                width: CURSOR_SIZE,
                height: CURSOR_SIZE,
            };
            send_cmd(&res_create, 24);

            #[repr(C)]
            struct AttachBackingCmd {
                hdr: VirtioGpuResourceAttachBacking,
                entry: VirtioGpuMemEntry,
            }
            let attach = AttachBackingCmd {
                hdr: VirtioGpuResourceAttachBacking {
                    hdr: VirtioGpuCtrlHdr { type_: VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING, flags: 0, fence_id: 0, ctx_id: 0, padding: 0 },
                    resource_id: CURSOR_RESOURCE_ID,
                    nr_entries: 1,
                },
                entry: VirtioGpuMemEntry {
                    addr: core::ptr::addr_of!(CURSOR_MEM) as u64,
                    length: (CURSOR_SIZE * CURSOR_SIZE * 4) as u32,
                    padding: 0,
                },
            };
            send_cmd(&attach, 24);
            CURSOR_RES_OK = true;
        }

        let transfer = VirtioGpuTransferToHost2d {
            hdr: VirtioGpuCtrlHdr { type_: VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D, flags: 0, fence_id: 0, ctx_id: 0, padding: 0 },
            r_x: 0,
            r_y: 0,
            r_width: CURSOR_SIZE,
            r_height: CURSOR_SIZE,
            offset: 0,
            resource_id: CURSOR_RESOURCE_ID,
            padding: 0,
        };
        send_cmd(&transfer, 24);

        let update = VirtioGpuUpdateCursor {
            hdr: VirtioGpuCtrlHdr { type_: VIRTIO_GPU_CMD_UPDATE_CURSOR, flags: 0, fence_id: 0, ctx_id: 0, padding: 0 },
            pos: VirtioGpuCursorPos { scanout_id: 0, x: CURSOR_X, y: CURSOR_Y, padding: 0 },
            resource_id: CURSOR_RESOURCE_ID,
            hot_x: 0,
            hot_y: 0,
            padding: 0,
        };
        send_cursor_cmd(&update);
        n
    }
}

/// Move the hardware cursor. Cheap enough to call from the timer tick.
pub fn move_cursor(x: u32, y: u32) {
    unsafe {
        CURSOR_X = x;
        CURSOR_Y = y;
        if !CURSOR_RES_OK { return; }
        let cmd = VirtioGpuUpdateCursor {
            hdr: VirtioGpuCtrlHdr { type_: VIRTIO_GPU_CMD_MOVE_CURSOR, flags: 0, fence_id: 0, ctx_id: 0, padding: 0 },
            pos: VirtioGpuCursorPos { scanout_id: 0, x, y, padding: 0 },
            resource_id: CURSOR_RESOURCE_ID,
            hot_x: 0,
            hot_y: 0,
            padding: 0,
        };
        send_cursor_cmd(&cmd);
    }
}
