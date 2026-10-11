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

const VIRTIO_GPU_CMD_RESOURCE_CREATE_2D: u32 = 0x0101;
const VIRTIO_GPU_CMD_SET_SCANOUT: u32 = 0x0103;
const VIRTIO_GPU_CMD_RESOURCE_FLUSH: u32 = 0x0104;
const VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D: u32 = 0x0105;
const VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING: u32 = 0x0106;
const VIRTIO_GPU_CMD_GET_DISPLAY_INFO: u32 = 0x0100;
const VIRTIO_GPU_CMD_RESOURCE_UNREF: u32 = 0x0102;
const VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING: u32 = 0x0107;
const VIRTIO_GPU_RESP_OK_DISPLAY_INFO: u32 = 0x1101;
// Config-space event bit: the host display changed size.
const VIRTIO_GPU_EVENT_DISPLAY: u32 = 1;

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
struct VirtioGpuResourceUnref {
    hdr: VirtioGpuCtrlHdr,
    resource_id: u32,
    padding: u32,
}

#[repr(C)]
struct VirtioGpuDisplayOne {
    r_x: u32,
    r_y: u32,
    r_width: u32,
    r_height: u32,
    enabled: u32,
    flags: u32,
}

#[repr(C)]
struct VirtioGpuRespDisplayInfo {
    hdr: VirtioGpuCtrlHdr,
    pmodes: [VirtioGpuDisplayOne; 16],
}

const fn hdr(type_: u32) -> VirtioGpuCtrlHdr {
    VirtioGpuCtrlHdr { type_, flags: 0, fence_id: 0, ctx_id: 0, padding: 0 }
}

static mut VIRTIO_GPU_BASE: usize = 0;
static mut VIRTQ_MEM: [u8; 16384] = [0; 16384];
static mut VIRTQ_PTR: *mut VirtQueue = core::ptr::null_mut();
static mut LAST_USED_IDX: u16 = 0;

// Responses land here; GET_DISPLAY_INFO is the largest (408 bytes).
#[repr(C, align(8))]
struct RespBuf([u8; 512]);
static mut RESP_BUF: RespBuf = RespBuf([0; 512]);

// Bigger host windows show the image unscaled (QEMU runs with zoom-to-fit off).
pub const MAX_WIDTH: u32 = 2560;
pub const MAX_HEIGHT: u32 = 1600;
const DEFAULT_WIDTH: u32 = 1280;
const DEFAULT_HEIGHT: u32 = 800;
const MIN_WIDTH: u32 = 640;
const MIN_HEIGHT: u32 = 400;

// Backing is physically contiguous since the kernel heap is identity mapped.
// The resource id alternates 1/3 so a resize creates the new one before dropping the old.
static mut WIDTH: u32 = 0;
static mut HEIGHT: u32 = 0;
static mut FB_PTR: *mut u32 = core::ptr::null_mut();
static mut FB_BYTES: usize = 0;
static mut RESOURCE_ID: u32 = 0;
// Bumped on every mode change, so readers of /dev/gpu/info can tell.
static mut MODE_GEN: u32 = 0;

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

        status |= VIRTIO_STATUS_DRIVER_OK;
        core::ptr::write_volatile((base + 0x070) as *mut u32, status);
        
        crate::println!("VirtIO GPU initialized at 0x{:X}", base);
    }
    
    let (w, h) = display_size().unwrap_or((DEFAULT_WIDTH, DEFAULT_HEIGHT));
    set_mode(w, h);
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
        
        (*VIRTQ_PTR).desc[p_resp].addr = core::ptr::addr_of_mut!(RESP_BUF) as u64;
        (*VIRTQ_PTR).desc[p_resp].len = resp_size;
        (*VIRTQ_PTR).desc[p_resp].flags = 2;
        (*VIRTQ_PTR).desc[p_resp].next = 0;
        (*VIRTQ_PTR).avail.ring[((*VIRTQ_PTR).avail.idx % QUEUE_SIZE as u16) as usize] = p_req as u16;
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

// Scanout 0's preferred size (the QEMU window size).
fn display_size() -> Option<(u32, u32)> {
    let cmd = hdr(VIRTIO_GPU_CMD_GET_DISPLAY_INFO);
    send_cmd(&cmd, core::mem::size_of::<VirtioGpuRespDisplayInfo>() as u32);
    unsafe {
        let resp = &*(core::ptr::addr_of!(RESP_BUF) as *const VirtioGpuRespDisplayInfo);
        if resp.hdr.type_ != VIRTIO_GPU_RESP_OK_DISPLAY_INFO {
            return None;
        }
        let m = &resp.pmodes[0];
        if m.enabled == 0 || m.r_width == 0 || m.r_height == 0 {
            return None;
        }
        Some((m.r_width, m.r_height))
    }
}

// The old resource is released only after scanout 0 points at the new one.
fn set_mode(w: u32, h: u32) {
    let w = w.clamp(MIN_WIDTH, MAX_WIDTH) & !7;
    let h = h.clamp(MIN_HEIGHT, MAX_HEIGHT) & !1;
    unsafe {
        if w == WIDTH && h == HEIGHT {
            return;
        }
        let bytes = ((w * h * 4) as usize + 4095) & !4095;
        let layout = core::alloc::Layout::from_size_align(bytes, 4096).unwrap();
        let ptr = alloc::alloc::alloc_zeroed(layout) as *mut u32;
        if ptr.is_null() {
            crate::println!("virtio_gpu: no memory for a {}x{} framebuffer", w, h);
            return;
        }
        for i in 0..(w * h) as usize {
            *ptr.add(i) = 0xFF00_0000;
        }

        let old_id = RESOURCE_ID;
        let new_id = if old_id == 1 { 3 } else { 1 };

        let res_create = VirtioGpuResourceCreate2d {
            hdr: hdr(VIRTIO_GPU_CMD_RESOURCE_CREATE_2D),
            resource_id: new_id,
            format: VIRTIO_GPU_FORMAT_B8G8R8A8_UNORM,
            width: w,
            height: h,
        };
        send_cmd(&res_create, 24);

        #[repr(C)]
        struct AttachBackingCmd {
            hdr: VirtioGpuResourceAttachBacking,
            entry: VirtioGpuMemEntry,
        }
        let attach = AttachBackingCmd {
            hdr: VirtioGpuResourceAttachBacking {
                hdr: hdr(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING),
                resource_id: new_id,
                nr_entries: 1,
            },
            entry: VirtioGpuMemEntry { addr: ptr as u64, length: w * h * 4, padding: 0 },
        };
        send_cmd(&attach, 24);

        let scanout = VirtioGpuSetScanout {
            hdr: hdr(VIRTIO_GPU_CMD_SET_SCANOUT),
            r_x: 0,
            r_y: 0,
            r_width: w,
            r_height: h,
            scanout_id: 0,
            resource_id: new_id,
        };
        send_cmd(&scanout, 24);

        let old_ptr = FB_PTR;
        let old_bytes = FB_BYTES;
        WIDTH = w;
        HEIGHT = h;
        FB_PTR = ptr;
        FB_BYTES = bytes;
        RESOURCE_ID = new_id;
        MODE_GEN = MODE_GEN.wrapping_add(1);

        if old_id != 0 {
            let detach = VirtioGpuResourceUnref { hdr: hdr(VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING), resource_id: old_id, padding: 0 };
            send_cmd(&detach, 24);
            let unref = VirtioGpuResourceUnref { hdr: hdr(VIRTIO_GPU_CMD_RESOURCE_UNREF), resource_id: old_id, padding: 0 };
            send_cmd(&unref, 24);
            alloc::alloc::dealloc(old_ptr as *mut u8, core::alloc::Layout::from_size_align(old_bytes, 4096).unwrap());
        }
        crate::println!("virtio_gpu: mode {}x{}", w, h);
    }
}

// Returns true if the mode changed.
pub fn poll_resize() -> bool {
    unsafe {
        if !is_ready() {
            return false;
        }
        let base = VIRTIO_GPU_BASE;
        let events = core::ptr::read_volatile((base + 0x100) as *const u32);
        if events & VIRTIO_GPU_EVENT_DISPLAY == 0 {
            return false;
        }
        core::ptr::write_volatile((base + 0x104) as *mut u32, VIRTIO_GPU_EVENT_DISPLAY);
        // Ack the config-change interrupt too, or the line stays raised.
        core::ptr::write_volatile((base + 0x064) as *mut u32, 2);
        let gen = MODE_GEN;
        if let Some((w, h)) = display_size() {
            set_mode(w, h);
        }
        gen != MODE_GEN
    }
}

pub fn flush_rect(x: u32, y: u32, w: u32, h: u32) {
    unsafe {
        let transfer = VirtioGpuTransferToHost2d {
            hdr: hdr(VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D),
            r_x: x,
            r_y: y,
            r_width: w,
            r_height: h,
            offset: ((y * WIDTH + x) * 4) as u64,
            resource_id: RESOURCE_ID,
            padding: 0,
        };
        send_cmd(&transfer, 24);
        
        let flush = VirtioGpuResourceFlush {
            hdr: hdr(VIRTIO_GPU_CMD_RESOURCE_FLUSH),
            r_x: x,
            r_y: y,
            r_width: w,
            r_height: h,
            resource_id: RESOURCE_ID,
            padding: 0,
        };
        send_cmd(&flush, 24);
    }
}

pub fn width() -> u32 {
    unsafe { WIDTH }
}

pub fn height() -> u32 {
    unsafe { HEIGHT }
}

pub fn mode_gen() -> u32 {
    unsafe { MODE_GEN }
}

pub fn fb_ptr() -> *mut u32 {
    unsafe { FB_PTR }
}

pub fn is_ready() -> bool {
    unsafe { VIRTIO_GPU_BASE != 0 && !VIRTQ_PTR.is_null() && !FB_PTR.is_null() }
}
