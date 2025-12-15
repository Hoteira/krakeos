use core::sync::atomic::Ordering;
use crate::drivers::video::virtio;
use crate::println;

pub const DEPTH: u8 = 32;

pub struct DisplayServer {
    pub width: u64,
    pub pitch: u64,
    pub height: u64,
    pub depth: usize,

    pub framebuffer: u64,
    pub double_buffer: u64,
}

pub static mut DISPLAY_SERVER: DisplayServer = DisplayServer {
    width: 0,
    height: 0,
    pitch: 0,
    depth: 32,
    framebuffer: 0,
    double_buffer: 0,
};

pub static mut VIRTIO_ACTIVE: bool = false;

impl DisplayServer {
    pub fn init(&mut self) {
        let boot_info = unsafe { crate::boot::BOOT_INFO };
        let vbe = boot_info.mode;

        unsafe {
            virtio::init();
            if virtio::queue::VIRT_QUEUES[0].is_some() {
                // VirtIO is available - use requested resolution
                self.width = 1280;
                self.height = 720;
                self.pitch = self.width * 4; // VirtIO always uses width * 4
                self.depth = 32;

                let size_bytes = (self.pitch * self.height) as usize;
                let pages = (size_bytes + 4095) / 4096;

                let db = crate::memory::pmm::allocate_frames(pages, 0).expect("Failed to allocate double buffer");
                let fb = crate::memory::pmm::allocate_frames(pages, 0).expect("Failed to allocate framebuffer");

                core::ptr::write_bytes(db as *mut u8, 0, size_bytes);
                core::ptr::write_bytes(fb as *mut u8, 0, size_bytes);


                self.double_buffer = db;
                self.framebuffer = fb;

                virtio::start_gpu(self.width as u32, self.height as u32, self.framebuffer);

                use crate::drivers::periferics::mouse::{CURSOR_BUFFER, CURSOR_WIDTH, CURSOR_HEIGHT};

                // Allocate 4 pages (16KB) for 64x64 cursor to ensure compatibility and alignment
                // 64 * 64 * 4 bytes = 16384 bytes = 4 pages
                let cursor_pages = 4;
                let cursor_phys_addr = crate::memory::pmm::allocate_frames(cursor_pages, 0)
                    .expect("Failed to allocate cursor buffer");

                let cursor_ptr = cursor_phys_addr as *mut u32;
                
                // Zero out the entire 16KB buffer
                core::ptr::write_bytes(cursor_ptr as *mut u8, 0, cursor_pages * 4096);

                // Copy the 32x32 cursor to the top-left corner
                // Stride of the new buffer is 64 pixels
                let new_stride = 64;
                for y in 0..CURSOR_HEIGHT {
                    for x in 0..CURSOR_WIDTH {
                        let src_idx = y * CURSOR_WIDTH + x;
                        let dst_idx = y * new_stride + x;
                        *cursor_ptr.add(dst_idx) = CURSOR_BUFFER[src_idx];
                    }
                }

                virtio::setup_cursor(
                    64, // Width
                    64, // Height
                    cursor_phys_addr as u64,
                    0,  // hot_x
                    0   // hot_y
                );

                println!("DisplayServer: Hardware cursor initialized");

                VIRTIO_ACTIVE = true;

                println!("DisplayServer: VirtIO GPU active at {}x{}", self.width, self.height);
                return;
            }
        }

        println!("DisplayServer: Using VBE fallback");
        self.width = vbe.width as u64;
        self.pitch = vbe.pitch as u64;
        self.height = vbe.height as u64;
        self.depth = 32;

        // Map VBE framebuffer with Write-Combining
        let fb_phys = vbe.framebuffer as u64;
        let fb_size = (self.pitch * self.height) as usize;
        let fb_pages = (fb_size + 4095) / 4096;
        let fb_virt_base = 0xFFFF_8000_FD00_0000;

        for i in 0..fb_pages {
            let offset = (i * 4096) as u64;
            let phys = fb_phys + offset;
            let virt = fb_virt_base + offset;

            let flags = crate::memory::paging::PAGE_PRESENT |
                crate::memory::paging::PAGE_WRITABLE |
                crate::memory::paging::PAGE_PAT;

            crate::memory::vmm::map_page(virt, phys, flags, None);
        }

        self.framebuffer = fb_virt_base;

        let size_bytes = self.pitch as usize * self.height as usize;
        let pages = (size_bytes + 4095) / 4096;

        unsafe {
            if let Some(buffer) = crate::memory::pmm::allocate_frames(pages, 0) {
                self.double_buffer = buffer;
                core::ptr::write_bytes(buffer as *mut u8, 0, size_bytes);
            } else {
                panic!("[DisplayServer] Failed to allocate double buffer!");
            }
        }
    }

    pub fn copy(&self) {
        unsafe {
            let buffer_size = self.pitch as u64 * self.height as u64;
            core::ptr::copy(
                self.double_buffer as *const u8,
                self.framebuffer as *mut u8,
                buffer_size as usize,
            );

            if VIRTIO_ACTIVE {
                virtio::flush(0, 0, self.width as u32, self.height as u32, self.width as u32);
            }
        }
    }

    pub fn copy_to_fb(&self, x: u32, y: u32, width: u32, height: u32) {
        let bytes_per_pixel = 4;

        let max_x = (self.width as u32).min(x + width);
        let max_y = (self.height as u32).min(y + height);

        if x >= self.width as u32 || y >= self.height as u32 {
            return;
        }

        let copy_width = (max_x - x) as usize;
        let copy_height = (max_y - y) as usize;

        let src = self.double_buffer as *const u8;
        let dst = self.framebuffer as *mut u8;
        let pitch = self.pitch as usize;

        unsafe {
            for row in 0..copy_height {
                let src_offset = ((y as usize + row) * pitch + x as usize * bytes_per_pixel) as usize;
                let dst_offset = ((y as usize + row) * pitch + x as usize * bytes_per_pixel) as usize;

                core::ptr::copy_nonoverlapping(
                    src.add(src_offset),
                    dst.add(dst_offset),
                    copy_width * bytes_per_pixel
                );
            }

            if VIRTIO_ACTIVE {
                virtio::flush(x, y, width, height, self.width as u32);
            }
        }
    }

    pub fn copy_to_db(&self, width: u32, height: u32, buffer: usize, x: u32, y: u32) {
        let dst_pitch = self.pitch as usize;
        let src_pitch = (width as usize) * 4;

        let max_x = (self.width as u32).min(x + width);
        let max_y = (self.height as u32).min(y + height);

        if x >= self.width as u32 || y >= self.height as u32 {
            return;
        }

        let copy_width = (max_x - x) as usize;
        let copy_height = (max_y - y) as usize;

        unsafe {
            let src_base = buffer as *const u8;
            let dst_base = self.double_buffer as *mut u8;

            for row in 0..copy_height {
                let src_row_ptr = src_base.add(row * src_pitch);
                let dst_row_ptr = dst_base.add((y as usize + row) * dst_pitch + (x as usize) * 4);

                for col in 0..copy_width {
                    let offset = col * 4;
                    let src_ptr = src_row_ptr.add(offset);
                    let dst_ptr = dst_row_ptr.add(offset);

                    let src_a = *src_ptr.add(3);

                    if src_a == 255 {
                        *(dst_ptr as *mut u32) = *(src_ptr as *const u32);
                    } else if src_a == 0 {
                        continue;
                    } else {
                        let alpha = src_a as u32;
                        let inv_alpha = 255 - alpha;

                        let src_r = *src_ptr as u32;
                        let src_g = *src_ptr.add(1) as u32;
                        let src_b = *src_ptr.add(2) as u32;

                        let dst_r = *dst_ptr as u32;
                        let dst_g = *dst_ptr.add(1) as u32;
                        let dst_b = *dst_ptr.add(2) as u32;

                        *dst_ptr = ((src_r * alpha + dst_r * inv_alpha) / 255) as u8;
                        *dst_ptr.add(1) = ((src_g * alpha + dst_g * inv_alpha) / 255) as u8;
                        *dst_ptr.add(2) = ((src_b * alpha + dst_b * inv_alpha) / 255) as u8;
                        *dst_ptr.add(3) = 255;
                    }
                }
            }
        }
    }

    pub fn copy_to_fb_a(&self, width: u32, height: u32, buffer: usize, x: u32, y: u32) {
        const BYTES_PER_PIXEL: usize = 4;

        let dst_pitch = self.pitch as usize;
        let src_pitch = (width as usize) * BYTES_PER_PIXEL;

        let max_x = (self.width as u32).min(x + width);
        let max_y = (self.height as u32).min(y + height);

        if x >= self.width as u32 || y >= self.height as u32 {
            return;
        }

        let copy_width = (max_x - x) as usize;
        let copy_height = (max_y - y) as usize;

        unsafe {
            let src_base = buffer as *const u8;
            let dst_base = self.framebuffer as *mut u8;

            for row in 0..copy_height {
                let src_row_ptr = src_base.add(row * src_pitch);
                let dst_row_ptr = dst_base.add((y as usize + row) * dst_pitch + (x as usize) * 4);

                for col in 0..copy_width {
                    let offset = col * 4;
                    let src_ptr = src_row_ptr.add(offset);
                    let dst_ptr = dst_row_ptr.add(offset);

                    let src_a = *src_ptr.add(3);

                    if src_a == 255 {
                        *(dst_ptr as *mut u32) = *(src_ptr as *const u32);
                    } else if src_a == 0 {
                        continue;
                    } else {
                        let alpha = src_a as u32;
                        let inv_alpha = 255 - alpha;

                        let src_r = *src_ptr as u32;
                        let src_g = *src_ptr.add(1) as u32;
                        let src_b = *src_ptr.add(2) as u32;

                        let dst_r = *dst_ptr as u32;
                        let dst_g = *dst_ptr.add(1) as u32;
                        let dst_b = *dst_ptr.add(2) as u32;

                        *dst_ptr = ((src_r * alpha + dst_r * inv_alpha) / 255) as u8;
                        *dst_ptr.add(1) = ((src_g * alpha + dst_g * inv_alpha) / 255) as u8;
                        *dst_ptr.add(2) = ((src_b * alpha + dst_b * inv_alpha) / 255) as u8;
                        *dst_ptr.add(3) = 255;
                    }
                }
            }
        }
    }

    pub fn write_pixel(&self, row: u32, col: u32, color: Color) {
        if col < self.width as u32 && row < self.height as u32 {
            unsafe {
                let offset = (row as u64 * self.pitch + col as u64 * 4) as usize;
                *((self.framebuffer as *mut u8).add(offset) as *mut u32) = color.to_u32();
            }
        }
    }

    pub fn draw_mouse(&self, x: u16, y: u16, dragging_window: bool) {
        use crate::drivers::periferics::mouse::{CURSOR_BUFFER, CURSOR_WIDTH, CURSOR_HEIGHT};

        let mut temp_buf: [u32; 1024] = [0; 1024];
        let pitch_bytes = self.pitch as usize;
        let fb_ptr = self.framebuffer as *mut u8;
        let db_ptr = self.double_buffer as *const u8;
        let screen_w = self.width as usize;
        let screen_h = self.height as usize;
        let mx = x as usize;
        let my = y as usize;

        // If dragging, read from FB (contains window). Else read from DB (clean).
        let bg_src = if dragging_window { fb_ptr as *const u8 } else { db_ptr };

        unsafe {
            for row in 0..CURSOR_HEIGHT {
                let screen_y = my + row;
                if screen_y >= screen_h { break; }

                let row_byte_offset = screen_y * pitch_bytes;

                for col in 0..CURSOR_WIDTH {
                    let screen_x = mx + col;
                    if screen_x >= screen_w { break; }

                    let pixel_offset = row_byte_offset + screen_x * 4;

                    let bg_color = *(bg_src.add(pixel_offset) as *const u32);
                    let cursor_color = CURSOR_BUFFER[row * CURSOR_WIDTH + col];

                    if cursor_color != 0 {
                        temp_buf[row * CURSOR_WIDTH + col] = cursor_color;
                    } else {
                        temp_buf[row * CURSOR_WIDTH + col] = bg_color;
                    }
                }
            }

            for row in 0..CURSOR_HEIGHT {
                let screen_y = my + row;
                if screen_y >= screen_h { break; }

                let fb_offset = screen_y * pitch_bytes + mx * 4;

                let copy_w = if mx + CURSOR_WIDTH > screen_w {
                    screen_w - mx
                } else {
                    CURSOR_WIDTH
                };

                core::ptr::copy_nonoverlapping(
                    temp_buf.as_ptr().add(row * CURSOR_WIDTH) as *const u8,
                    fb_ptr.add(fb_offset),
                    copy_w * 4
                );
            }

            if VIRTIO_ACTIVE {
                virtio::flush(x as u32, y as u32, 32, 32, self.width as u32);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct Window {
    pub id: usize,
    pub size_i: (u64, u64),
    pub size_f: (u64, u64),
    pub mouse_handler: usize,
    pub draw_handler: usize,
    pub z_index: usize,
}

pub struct Mouse {
    pub x: u16,
    pub y: u16,

    pub left: bool,
    pub center: bool,
    pub right: bool,

    pub state: State,
}

pub enum State {
    Point,
    Write,
    Click,
}

pub enum EventType {
    Close,
    Resize,
    Minimize,
    Refresh,
    Clicked { buttons: [bool; 3], x: u64, y: u64 },
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct Event {
    pub id: usize,
    pub addr: usize,
    pub args: [usize; 4],
}


#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct Color {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl Color {

    pub const fn new() -> Self {
        Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        }
    }

    pub fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 255 }
    }

    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r, g, b, a }
    }

    pub fn to_u16(&self) -> u16 {
        let r = (self.r >> 3) as u16;
        let g = (self.g >> 2) as u16;
        let b = (self.b >> 3) as u16;
        (r << 11) | (g << 5) | b
    }

    pub fn to_u32(&self) -> u32 {
        ((self.a as u32) << 24) | ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }

    pub fn to_u24(&self) -> [u8; 3] {
        [self.b, self.g, self.r]
    }

    pub fn from_u16(rgb: u16) -> Self {
        let r5 = ((rgb >> 11) & 0x1F) as u8;
        let g6 = ((rgb >> 5 ) & 0x3F) as u8;
        let b5 = ( rgb & 0x1F) as u8;
        let r = (r5 << 3) | (r5 >> 2);
        let g = (g6 << 2) | (g6 >> 4);
        let b = (b5 << 3) | (b5 >> 2);
        Color { r, g, b, a: 0xFF }
    }

    pub fn from_u32(rgba: u32) -> Self {
        let r = ((rgba >> 24) & 0xFF) as u8;
        let g = ((rgba >> 16) & 0xFF) as u8;
        let b = ((rgba >>  8) & 0xFF) as u8;
        let a = ( rgba & 0xFF) as u8;

        Color { r, g, b, a }
    }

    pub fn from_u24(rgb24: u32) -> Self {
        let r = ((rgb24 >> 16) & 0xFF) as u8;
        let g = ((rgb24 >>  8) & 0xFF) as u8;
        let b = ( rgb24         & 0xFF) as u8;
        Color { r, g, b, a: 0xFF }
    }
}