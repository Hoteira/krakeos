//! `Hal` implementation backing the `virtio-drivers` crate.
//!
//! Bridges the crate's DMA / MMIO requirements onto the kernel's physical memory
//! manager and HHDM (higher-half direct map). All kernel DMA buffers live in the
//! HHDM and come from `pmm::allocate_frames` (physically contiguous), so phys<->virt
//! translation is a simple HHDM offset.

use core::ptr::NonNull;
use virtio_drivers::{BufferDirection, Hal, PhysAddr};

use crate::memory::paging::{virt_to_phys, HHDM_OFFSET};
use crate::memory::{pmm, vmm};

/// Zero-sized HAL handle; all state lives in the kernel's global allocators.
pub struct KrakenHal;

unsafe impl Hal for KrakenHal {
    fn dma_alloc(pages: usize, _direction: BufferDirection) -> (PhysAddr, NonNull<u8>) {
        let phys = pmm::allocate_frames(pages).expect("virtio HAL: DMA allocation failed");
        let virt = (phys + HHDM_OFFSET) as *mut u8;
        // Virtqueues must start zeroed.
        unsafe { core::ptr::write_bytes(virt, 0, pages * 4096) };
        (phys as PhysAddr, NonNull::new(virt).unwrap())
    }

    unsafe fn dma_dealloc(paddr: PhysAddr, _vaddr: NonNull<u8>, pages: usize) -> i32 {
        for i in 0..pages {
            pmm::free_frame(paddr as u64 + (i * 4096) as u64);
        }
        0
    }

    unsafe fn mmio_phys_to_virt(paddr: PhysAddr, size: usize) -> NonNull<u8> {
        let virt = vmm::map_mmio(paddr as u64, size);
        NonNull::new(virt as *mut u8).unwrap()
    }

    unsafe fn share(buffer: NonNull<[u8]>, _direction: BufferDirection) -> PhysAddr {
        // Buffers handed to the device are kernel memory; resolve their physical
        // address via the page tables. They are physically contiguous (HHDM / heap).
        let vaddr = buffer.as_ptr() as *mut u8 as u64;
        virt_to_phys(vaddr) as PhysAddr
    }

    unsafe fn unshare(_paddr: PhysAddr, _buffer: NonNull<[u8]>, _direction: BufferDirection) {
        // No bounce buffer: the device DMA'd directly to/from kernel memory.
    }
}
