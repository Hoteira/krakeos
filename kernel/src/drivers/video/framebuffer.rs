use crate::boot::BOOT_INFO;

pub struct Framebuffer {
    pub base: *mut u32,
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
    pub bpp: usize,
}

// Global framebuffer instance (unsafe to access directly)
pub static mut FRAMEBUFFER: Framebuffer = Framebuffer {
    base: 0 as *mut u32,
    width: 0,
    height: 0,
    pitch: 0,
    bpp: 0,
};

pub fn init() {
    unsafe {
        // Retrieve VBE info from BootInfo
        let mode = BOOT_INFO.mode; // This is VbeModeInfoBlock (packed)
        
        // Convert physical framebuffer address to a virtual one?
        // Wait, paging is enabled. Is this address identity mapped?
        // Usually bootloaders identity map the first few GBs or provide a map.
        // 'swiftboot' likely provides the physical address in `mode.framebuffer`.
        // We need to ensure this memory is mapped in the kernel page table.
        
        // For now, we assume it's mapped or we map it.
        // Let's grab the physical address.
        let phys_addr = mode.framebuffer as u64;
        // let fb_size = (mode.height as usize * mode.pitch as usize) as u64;
        
        // Map the framebuffer! 
        // We need to map enough pages to cover fb_size.
        // We map it 1:1 (virt = phys) for simplicity in the kernel.
        // let pages_needed = (fb_size + 4095) / 4096;
        
        // crate::debugln!("[VIDEO] Mapping framebuffer: phys={:#x}, pages={}", phys_addr, pages_needed);
        
        // NOTE: Commented out because we hit a Huge Page Collision.
        // This means the framebuffer (0xFD000000) is already mapped by the bootloader
        // as part of a large (2MB or 1GB) identity map.
        /*
        for i in 0..pages_needed {
            let offset = i * 4096;
            let page_phys = phys_addr + offset;
            // Map as Present | Writable. (No User flag, kernel only for now)
            crate::memory::vmm::map_page(
                page_phys, 
                page_phys, 
                crate::memory::paging::PAGE_PRESENT | crate::memory::paging::PAGE_WRITABLE | crate::memory::paging::PAGE_WRITE_THROUGH
            );
        }
        */
        
        FRAMEBUFFER.base = phys_addr as *mut u32;
        FRAMEBUFFER.width = mode.width as usize;
        FRAMEBUFFER.height = mode.height as usize;
        FRAMEBUFFER.pitch = mode.pitch as usize;
        FRAMEBUFFER.bpp = mode.bpp as usize;
        
        let fb_width = FRAMEBUFFER.width;
        let fb_height = FRAMEBUFFER.height;
        let fb_bpp = FRAMEBUFFER.bpp;
        
        crate::debugln!("[VIDEO] Framebuffer at {:#x} ({}x{} @ {}bpp)", phys_addr, fb_width, fb_height, fb_bpp);
    }
}

pub fn put_pixel(x: usize, y: usize, color: u32) {
    unsafe {
        if x >= FRAMEBUFFER.width || y >= FRAMEBUFFER.height {
            return;
        }
        
        // Pitch is usually in bytes. 
        // If BPP is 32 (4 bytes), pitch / 4 = pixels per line stride.
        // Standard formula: base + y * pitch + x * Bpp
        
        let offset = y * FRAMEBUFFER.pitch + x * (FRAMEBUFFER.bpp / 8);
        
        // We are writing u32 (0xAARRGGBB), so we cast to u32 ptr
        // NOTE: This assumes 32-bit BPP. If 24-bit, we need 3 writes.
        let ptr = (FRAMEBUFFER.base as *mut u8).add(offset) as *mut u32;
        
        *ptr = color;
    }
}

pub fn clear_screen(color: u32) {
    unsafe {
        // Fill memory logic
        // Naive loop for now
        for y in 0..FRAMEBUFFER.height {
            for x in 0..FRAMEBUFFER.width {
                put_pixel(x, y, color);
            }
        }
    }
}
