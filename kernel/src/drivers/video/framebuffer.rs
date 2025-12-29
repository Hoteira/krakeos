use crate::boot::BOOT_INFO;

pub struct Framebuffer {
    pub base: *mut u32,
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
    pub bpp: usize,
}

pub static mut FRAMEBUFFER: Framebuffer = Framebuffer {
    base: 0 as *mut u32,
    width: 0,
    height: 0,
    pitch: 0,
    bpp: 0,
};

pub fn init() {
    unsafe {
        let mode = BOOT_INFO.mode;
        
        let phys_addr = mode.framebuffer as u64;
        
        FRAMEBUFFER.base = phys_addr as *mut u32;
        FRAMEBUFFER.width = mode.width as usize;
        FRAMEBUFFER.height = mode.height as usize;
        FRAMEBUFFER.pitch = mode.pitch as usize;
        FRAMEBUFFER.bpp = mode.bpp as usize;
        
        let _fb_width = FRAMEBUFFER.width;
        let _fb_height = FRAMEBUFFER.height;
        let _fb_bpp = FRAMEBUFFER.bpp;
        

    }
}

pub fn put_pixel(x: usize, y: usize, color: u32) {
    unsafe {
        if x >= FRAMEBUFFER.width || y >= FRAMEBUFFER.height {
            return;
        }
        
        let offset = y * FRAMEBUFFER.pitch + x * (FRAMEBUFFER.bpp / 8);
        
        let ptr = (FRAMEBUFFER.base as *mut u8).add(offset) as *mut u32;
        
        *ptr = color;
    }
}

pub fn clear_screen(color: u32) {
    unsafe {
        for y in 0..FRAMEBUFFER.height {
            for x in 0..FRAMEBUFFER.width {
                put_pixel(x, y, color);
            }
        }
    }
}