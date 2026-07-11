use core::sync::atomic::{compiler_fence, Ordering};

const VIRTIO_STATUS_ACKNOWLEDGE: u32 = 1;
const VIRTIO_STATUS_DRIVER: u32 = 2;
const VIRTIO_STATUS_DRIVER_OK: u32 = 4;
const VIRTIO_STATUS_FEATURES_OK: u32 = 8;

const QUEUE_SIZE: usize = 16;

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct VirtioInputEvent {
    pub type_: u16,
    pub code: u16,
    pub value: u32,
}

#[repr(C, align(4096))]
struct VirtQueueMem {
    desc: [crate::drivers::virtio::VirtqDesc; QUEUE_SIZE],
    avail: crate::drivers::virtio::VirtqAvail,
    pad: [u8; 4096 - (16 * QUEUE_SIZE + 6 + 2 * QUEUE_SIZE)],
    used: crate::drivers::virtio::VirtqUsed,
}

struct VirtioInputDevice {
    base: usize,
    vq: *mut VirtQueueMem,
    // Heap-allocated so the address handed to the device stays stable
    events: *mut VirtioInputEvent,
    last_used_idx: u16,
}

static mut DEVICES: [Option<VirtioInputDevice>; 4] = [None, None, None, None];
static mut DEV_COUNT: usize = 0;

pub fn init(base: usize) {
    unsafe {
        let version = core::ptr::read_volatile((base + 0x004) as *const u32);
        if version != 1 {
            return; // Legacy devices only for now
        }
        
        // Reset device
        core::ptr::write_volatile((base + 0x070) as *mut u32, 0);
        
        let mut status = VIRTIO_STATUS_ACKNOWLEDGE;
        core::ptr::write_volatile((base + 0x070) as *mut u32, status);
        status |= VIRTIO_STATUS_DRIVER;
        core::ptr::write_volatile((base + 0x070) as *mut u32, status);
        
        // Features
        core::ptr::write_volatile((base + 0x020) as *mut u32, 0);
        status |= VIRTIO_STATUS_FEATURES_OK;
        core::ptr::write_volatile((base + 0x070) as *mut u32, status);
        
        let s = core::ptr::read_volatile((base + 0x070) as *const u32);
        if (s & VIRTIO_STATUS_FEATURES_OK) == 0 {
            return;
        }

        // Allocate memory for virtqueue
        let layout = core::alloc::Layout::new::<VirtQueueMem>();
        let mem = alloc::alloc::alloc_zeroed(layout) as *mut VirtQueueMem;
        
        // Setup Queue 0 (eventq)
        core::ptr::write_volatile((base + 0x030) as *mut u32, 0); // QueueSel = 0
        let max_size = core::ptr::read_volatile((base + 0x034) as *const u32);
        if max_size < QUEUE_SIZE as u32 {
            return;
        }
        
        let pfn = (mem as u32) / 4096;
        core::ptr::write_volatile((base + 0x028) as *mut u32, 4096);
        core::ptr::write_volatile((base + 0x038) as *mut u32, QUEUE_SIZE as u32);
        core::ptr::write_volatile((base + 0x03C) as *mut u32, 4096);
        core::ptr::write_volatile((base + 0x040) as *mut u32, pfn);
        
        let ev_layout = core::alloc::Layout::array::<VirtioInputEvent>(QUEUE_SIZE).unwrap();
        let events = alloc::alloc::alloc_zeroed(ev_layout) as *mut VirtioInputEvent;

        let dev = VirtioInputDevice {
            base,
            vq: mem,
            events,
            last_used_idx: 0,
        };

        // Populate eventq with buffers
        for i in 0..QUEUE_SIZE {
            (*mem).desc[i].addr = events.add(i) as u64;
            (*mem).desc[i].len = 8;
            (*mem).desc[i].flags = 2; // VIRTQ_DESC_F_WRITE
            (*mem).desc[i].next = 0;
            (*mem).avail.ring[i] = i as u16;
        }
        compiler_fence(Ordering::SeqCst);
        (*mem).avail.idx = QUEUE_SIZE as u16;
        compiler_fence(Ordering::SeqCst);

        status |= VIRTIO_STATUS_DRIVER_OK;
        core::ptr::write_volatile((base + 0x070) as *mut u32, status);

        // Notify device that buffers are available
        core::ptr::write_volatile((base + 0x050) as *mut u32, 0); // QueueNotify = 0
        
        crate::println!("VirtIO Input initialized at {:#x}", base);
        
        DEVICES[DEV_COUNT] = Some(dev);
        DEV_COUNT += 1;
    }
}

pub fn poll() {
    unsafe {
        for i in 0..DEV_COUNT {
            if let Some(dev) = &mut DEVICES[i] {
                let vq = dev.vq;
                let used_idx = core::ptr::read_volatile(core::ptr::addr_of!((*vq).used.idx));
                while dev.last_used_idx != used_idx {
                    compiler_fence(Ordering::SeqCst);
                    let ring_idx = (dev.last_used_idx % QUEUE_SIZE as u16) as usize;
                    let desc_idx = (*vq).used.ring[ring_idx].id as usize;
                    let len = (*vq).used.ring[ring_idx].len;
                    
                    if desc_idx < QUEUE_SIZE && len == 8 {
                        let ev = core::ptr::read(dev.events.add(desc_idx));
                        crate::sys::input::push_event(ev.type_, ev.code, ev.value);
                    }
                    
                    // Requeue buffer
                    let avail_idx = (*vq).avail.idx;
                    (*vq).avail.ring[(avail_idx % QUEUE_SIZE as u16) as usize] = desc_idx as u16;
                    compiler_fence(Ordering::SeqCst);
                    (*vq).avail.idx = avail_idx.wrapping_add(1);
                    compiler_fence(Ordering::SeqCst);
                    core::ptr::write_volatile((dev.base + 0x050) as *mut u32, 0);
                    
                    dev.last_used_idx = dev.last_used_idx.wrapping_add(1);
                }
            }
        }

        // One coalesced hardware-cursor move per poll, after draining events
        if let Some((x, y)) = crate::sys::input::take_cursor_move() {
            crate::drivers::virtio_gpu::move_cursor(x, y);
        }
    }
}
