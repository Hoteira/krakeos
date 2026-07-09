use core::sync::atomic::{compiler_fence, Ordering};

const VIRTIO_MAGIC: u32 = 0x74726976;
const VIRTIO_DEV_BLOCK: u32 = 2;

const VIRTIO_STATUS_ACKNOWLEDGE: u32 = 1;
const VIRTIO_STATUS_DRIVER: u32 = 2;
const VIRTIO_STATUS_DRIVER_OK: u32 = 4;
const VIRTIO_STATUS_FEATURES_OK: u32 = 8;

const QUEUE_SIZE: usize = 16; // Keep it small and simple

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

#[repr(C)]
struct BlkReq {
    type_: u32,
    reserved: u32,
    sector: u64,
}

static mut VIRTIO_BASE: usize = 0;
#[repr(C, align(4096))]
struct VirtQueue {
    desc: [VirtqDesc; QUEUE_SIZE],
    avail: VirtqAvail,
    _pad: [u8; 3802],
    used: VirtqUsed,
}

static mut VIRTQ_MEM: [u8; 16384] = [0; 16384];
static mut VIRTQ_PTR: *mut VirtQueue = core::ptr::null_mut();

static mut REQ: BlkReq = BlkReq { type_: 0, reserved: 0, sector: 0 };
static mut BLK_STATUS: u8 = 0;

static mut QUEUE_IDX: u16 = 0;
static mut LAST_USED_IDX: u16 = 0;

pub fn init() -> bool {
    // Scan QEMU virtio-mmio addresses
    let mut base = 0x10001000;
    let mut found = false;
    for _ in 0..8 {
        unsafe {
            let magic = core::ptr::read_volatile(base as *const u32);
            let dev_id = core::ptr::read_volatile((base + 0x008) as *const u32);
            if magic == VIRTIO_MAGIC && dev_id == VIRTIO_DEV_BLOCK {
                found = true;
                VIRTIO_BASE = base;
                break;
            }
        }
        base += 0x1000;
    }

    if !found {
        crate::println!("VirtIO Block device not found!");
        return false;
    }

    unsafe {
        let base = VIRTIO_BASE;
        let version = core::ptr::read_volatile((base + 0x004) as *const u32);
        crate::println!("VirtIO Version: {}", version);
        
        // Reset device
        core::ptr::write_volatile((base + 0x070) as *mut u32, 0);
        
        // Acknowledge & Driver
        let mut status = VIRTIO_STATUS_ACKNOWLEDGE;
        core::ptr::write_volatile((base + 0x070) as *mut u32, status);
        status |= VIRTIO_STATUS_DRIVER;
        core::ptr::write_volatile((base + 0x070) as *mut u32, status);
        
        // Features (Legacy)
        let features = core::ptr::read_volatile((base + 0x010) as *const u32);
        core::ptr::write_volatile((base + 0x020) as *mut u32, features & !(1 << 5)); // Clear RO feature
        
        status |= VIRTIO_STATUS_FEATURES_OK;
        core::ptr::write_volatile((base + 0x070) as *mut u32, status);
        let s = core::ptr::read_volatile((base + 0x070) as *const u32);
        if (s & VIRTIO_STATUS_FEATURES_OK) == 0 {
            crate::println!("VirtIO features not accepted!");
            return false;
        }

        // Setup Queue 0
        core::ptr::write_volatile((base + 0x030) as *mut u32, 0); // QueueSel
        let max_size = core::ptr::read_volatile((base + 0x034) as *const u32);
        if max_size == 0 || max_size < QUEUE_SIZE as u32 {
            crate::println!("VirtIO queue size insufficient!");
            return false;
        }
        
        let mem_addr = core::ptr::addr_of_mut!(VIRTQ_MEM) as usize;
        let aligned_addr = (mem_addr + 4095) & !4095;
        VIRTQ_PTR = aligned_addr as *mut VirtQueue;
        core::ptr::write_bytes(VIRTQ_PTR as *mut u8, 0, 4096);

        core::ptr::write_volatile((base + 0x028) as *mut u32, 4096); // GuestPageSize (Legacy)
        core::ptr::write_volatile((base + 0x038) as *mut u32, QUEUE_SIZE as u32); // QueueNum
        core::ptr::write_volatile((base + 0x03C) as *mut u32, 4096); // QueueAlign
        let pfn = (aligned_addr as u32) / 4096;
        crate::println!("VIRTQ Addr: {:#x}, PFN: {:#x}", aligned_addr, pfn);
        core::ptr::write_volatile((base + 0x040) as *mut u32, pfn); // QueuePFN (Legacy)

        // Driver OK
        status |= VIRTIO_STATUS_DRIVER_OK;
        core::ptr::write_volatile((base + 0x070) as *mut u32, status);
        
        crate::println!("VirtIO Block driver initialized at 0x{:X}", base);
        true
    }
}

// Synchronous block read/write. Sector is 512 bytes.
pub fn block_op(sector: u64, buf: *mut u8, len: u32, write: bool) {
    unsafe {
        let base = VIRTIO_BASE;
        if base == 0 { return; }

        let head = 0;
        let p_req = 1;
        let p_buf = 2;
        let p_stat = 3;

        REQ.type_ = if write { 1 } else { 0 }; // 0 = Read, 1 = Write
        REQ.sector = sector;
        BLK_STATUS = 0;

        // Descriptor 0: BlkReq (Device reads this)
        (*VIRTQ_PTR).desc[p_req].addr = core::ptr::addr_of!(REQ) as u64;
        (*VIRTQ_PTR).desc[p_req].len = core::mem::size_of::<BlkReq>() as u32;
        (*VIRTQ_PTR).desc[p_req].flags = 1; // NEXT
        (*VIRTQ_PTR).desc[p_req].next = p_buf as u16;

        // Descriptor 1: Buffer (Device reads or writes this)
        (*VIRTQ_PTR).desc[p_buf].addr = buf as u64;
        (*VIRTQ_PTR).desc[p_buf].len = len;
        (*VIRTQ_PTR).desc[p_buf].flags = 1 | (if write { 0 } else { 2 }); // NEXT, (2 = DEVICE_WRITE for reads)
        (*VIRTQ_PTR).desc[p_buf].next = p_stat as u16;

        // Descriptor 2: Status (Device writes this)
        (*VIRTQ_PTR).desc[p_stat].addr = core::ptr::addr_of!(BLK_STATUS) as u64;
        (*VIRTQ_PTR).desc[p_stat].len = 1;
        (*VIRTQ_PTR).desc[p_stat].flags = 2; // WRITE
        (*VIRTQ_PTR).desc[p_stat].next = 0;

        // Put in available ring
        (*VIRTQ_PTR).avail.ring[((*VIRTQ_PTR).avail.idx % QUEUE_SIZE as u16) as usize] = p_req as u16;
        core::arch::asm!("fence rw, rw");
        (*VIRTQ_PTR).avail.idx = (*VIRTQ_PTR).avail.idx.wrapping_add(1);
        core::arch::asm!("fence rw, rw");

        // Notify device
        core::ptr::write_volatile((base + 0x050) as *mut u32, 0); // Notify Queue 0

        // Wait for used ring to update
        while LAST_USED_IDX == core::ptr::read_volatile(core::ptr::addr_of!((*VIRTQ_PTR).used.idx)) {
            core::arch::asm!("nop");
            core::arch::asm!("fence r, rw");
        }
        
        LAST_USED_IDX = LAST_USED_IDX.wrapping_add(1);
    }
}

pub fn read_sector(sector: u64, buf: *mut u8, count: usize) {
    // VirtIO block device operates in 512-byte sectors
    block_op(sector, buf, (count * 512) as u32, false);
}

pub fn write_sector(sector: u64, buf: *const u8, count: usize) {
    block_op(sector, buf as *mut u8, (count * 512) as u32, true);
}
