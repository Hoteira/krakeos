use core::sync::atomic::Ordering;
use crate::drivers::video::virtio;
use crate::{debugln, println};

pub const DEPTH: u8 = 32;

pub struct DisplayServer {
    pub width: u64,
    pub pitch: u64,
    pub height: u64,
    pub depth: usize,

    pub framebuffer: u64,
    pub double_buffer: u64,
    
    pub buffer1_phys: u64,
    pub buffer2_phys: u64,
    pub active_resource_id: u32,
}

pub static mut DISPLAY_SERVER: DisplayServer = DisplayServer {
    width: 0,
    height: 0,
    pitch: 0,
    depth: 32,
    framebuffer: 0,
    double_buffer: 0,
    buffer1_phys: 0,
    buffer2_phys: 0,
    active_resource_id: 1,
};

pub static mut VIRTIO_ACTIVE: bool = false;

impl DisplayServer {
    pub fn init(&mut self) {
        let boot_info = unsafe { crate::boot::BOOT_INFO };
        let vbe = boot_info.mode;

        unsafe {
            virtio::init();
            if virtio::queue::VIRT_QUEUES[0].is_some() {
                
                if let Some((w, h)) = virtio::get_display_info() {
                    self.width = w as u64;
                    self.height = h as u64;
                    debugln!("DisplayServer: Detected resolution {}x{}", w, h);
                } else {
                    self.width = 1280;
                    self.height = 720;
                    debugln!("DisplayServer: Could not detect resolution, defaulting to 1280x720");
                }

                self.pitch = self.width * 4;
                self.depth = 32;

                let size_bytes = (self.pitch * self.height) as usize;
                let pages = (size_bytes + 4095) / 4096;

                let b1 = crate::memory::pmm::allocate_frames(pages, 0).expect("Failed to allocate buffer 1");
                let b2 = crate::memory::pmm::allocate_frames(pages, 0).expect("Failed to allocate buffer 2");

                core::ptr::write_bytes(b1 as *mut u8, 0, size_bytes);
                core::ptr::write_bytes(b2 as *mut u8, 0, size_bytes);

                self.buffer1_phys = b1;
                self.buffer2_phys = b2;
                
                // Active (Front) is Buffer 1
                self.framebuffer = b1;
                self.active_resource_id = 1;
                
                // Double (Back) is Buffer 2
                self.double_buffer = b2;

                virtio::start_gpu(self.width as u32, self.height as u32, self.buffer1_phys, self.buffer2_phys);

                // Initialize Host Resources with zeroed data
                virtio::transfer_and_flush(1, self.width as u32, self.height as u32);
                virtio::transfer_and_flush(2, self.width as u32, self.height as u32);

                // Setup Hardware Cursor
                use crate::drivers::periferics::mouse::{CURSOR_BUFFER, CURSOR_WIDTH, CURSOR_HEIGHT};
                let cursor_size_bytes = (CURSOR_WIDTH * CURSOR_HEIGHT * 4) as usize;
                let cursor_pages = (cursor_size_bytes + 4095) / 4096;
                if let Some(cursor_phys) = crate::memory::pmm::allocate_frames(cursor_pages, 0) {
                    let cursor_ptr = cursor_phys as *mut u32;
                    // Copy cursor buffer
                    for i in 0..CURSOR_BUFFER.len() {
                        *cursor_ptr.add(i) = CURSOR_BUFFER[i];
                    }
                    
                    // Temporarily disabled hardware cursor setup
                    // virtio::cursor::setup_cursor(cursor_phys, CURSOR_WIDTH as u32, CURSOR_HEIGHT as u32, self.width as u32 / 2, self.height as u32 / 2);
                    debugln!("DisplayServer: Hardware cursor is DISABLED by request.");
                } else {
                    println!("DisplayServer: Failed to allocate hardware cursor buffer!");
                    debugln!("DisplayServer: Hardware cursor is NOT ACTIVE (buffer alloc failed).");
                }

                VIRTIO_ACTIVE = true;

                println!("DisplayServer: VirtIO GPU active at {}x{}", self.width, self.height);
                return;
            } else {
                debugln!("DisplayServer: Hardware cursor is NOT ACTIVE (VirtIO GPU not found or setup failed).");
            }
        }

        println!("DisplayServer: Using VBE fallback");
        self.width = vbe.width as u64;
        self.pitch = vbe.pitch as u64;
        self.height = vbe.height as u64;
        self.depth = 32;

        // Map VBE framebuffer with Write-Combining

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

    pub fn copy(&mut self) {
        unsafe {
            if VIRTIO_ACTIVE {
                // PAGE FLIP LOGIC
                let next_resource = if self.active_resource_id == 1 { 2 } else { 1 };
                let next_buffer = if self.active_resource_id == 1 { self.buffer2_phys } else { self.buffer1_phys };
                let current_buffer = if self.active_resource_id == 1 { self.buffer1_phys } else { self.buffer2_phys };

                // 1. Transfer BACK buffer (which we just drew to) to host and flush it
                virtio::transfer_and_flush(next_resource, self.width as u32, self.height as u32);

                // 2. Set scanout to BACK buffer (Flip)
                virtio::set_scanout(next_resource, self.width as u32, self.height as u32);

                // 3. Update state
                self.active_resource_id = next_resource;
                
                // Swap pointers:
                // framebuffer becomes the NEW active buffer (what was back)
                // double_buffer becomes the NEW back buffer (what was front, to be drawn over)
                self.framebuffer = next_buffer;
                self.double_buffer = current_buffer;

                // Sync the new back buffer with the current front buffer (which contains the clean scene)
                // This ensures the back buffer is a valid source for background restoration.
                let size_bytes = (self.pitch * self.height) as usize;
                core::ptr::copy_nonoverlapping(
                    self.framebuffer as *const u8,
                    self.double_buffer as *mut u8,
                    size_bytes
                );

            } else {
                let buffer_size = self.pitch as u64 * self.height as u64;
                core::ptr::copy(
                    self.double_buffer as *const u8,
                    self.framebuffer as *mut u8,
                    buffer_size as usize,
                );
            }
        }
    }

    pub fn copy_to_fb(&self, x: i32, y: i32, width: u32, height: u32) {
        let bytes_per_pixel = 4;
        let screen_w = self.width as i32;
        let screen_h = self.height as i32;

        let dst_x = x.max(0);
        let dst_y = y.max(0);
        let end_x = (x + width as i32).min(screen_w);
        let end_y = (y + height as i32).min(screen_h);

        if end_x <= dst_x || end_y <= dst_y { return; }

        let copy_width = (end_x - dst_x) as usize;
        let copy_height = (end_y - dst_y) as usize;

        let src_off_x = (dst_x - x) as usize;
        let src_off_y = (dst_y - y) as usize;

        let src = self.double_buffer as *const u8;
        let dst = self.framebuffer as *mut u8;
        let pitch = self.pitch as usize;

        unsafe {
            for row in 0..copy_height {
                // SRC: pitch * (original_y + src_off_y + row) + (original_x + src_off_x) * 4
                // But wait, source is double_buffer (screen sized).
                // So source coords are SAME as dest coords for copy_to_fb (restoring background).
                
                let offset = ((dst_y as usize + row) * pitch + dst_x as usize * bytes_per_pixel) as usize;

                core::ptr::copy_nonoverlapping(
                    src.add(offset),
                    dst.add(offset),
                    copy_width * bytes_per_pixel
                );
            }
        }
    }

    pub fn copy_to_db(&self, width: u32, height: u32, buffer: usize, x: i32, y: i32) {
        let dst_pitch = self.pitch as usize / 4; // Pitch in u32
        let src_pitch = width as usize;          // Pitch in u32
        let screen_w = self.width as i32;
        let screen_h = self.height as i32;

        let dst_x = x.max(0);
        let dst_y = y.max(0);
        let end_x = (x + width as i32).min(screen_w);
        let end_y = (y + height as i32).min(screen_h);

        if end_x <= dst_x || end_y <= dst_y { return; }

        let copy_width = (end_x - dst_x) as usize;
        let copy_height = (end_y - dst_y) as usize;
        
        let src_off_x = (dst_x - x) as usize;
        let src_off_y = (dst_y - y) as usize;

        unsafe {
            let src_base = buffer as *const u32;
            let dst_base = self.double_buffer as *mut u32;

            for row in 0..copy_height {
                let src_row_ptr = src_base.add((src_off_y + row) * src_pitch + src_off_x);
                let dst_row_ptr = dst_base.add((dst_y as usize + row) * dst_pitch + (dst_x as usize));

                for col in 0..copy_width {
                    let src_pixel = *src_row_ptr.add(col);
                    let alpha = (src_pixel >> 24) & 0xFF;

                    if alpha == 255 {
                        *dst_row_ptr.add(col) = src_pixel;
                    } else if alpha == 0 {
                        continue;
                    } else {
                        let dst_pixel = *dst_row_ptr.add(col);
                        
                        let inv_alpha = 255 - alpha;

                        let src_r = (src_pixel >> 16) & 0xFF;
                        let src_g = (src_pixel >> 8) & 0xFF;
                        let src_b = src_pixel & 0xFF;

                        let dst_r = (dst_pixel >> 16) & 0xFF;
                        let dst_g = (dst_pixel >> 8) & 0xFF;
                        let dst_b = dst_pixel & 0xFF;

                        let r = (src_r * alpha + dst_r * inv_alpha) >> 8;
                        let g = (src_g * alpha + dst_g * inv_alpha) >> 8;
                        let b = (src_b * alpha + dst_b * inv_alpha) >> 8;

                        *dst_row_ptr.add(col) = (0xFF << 24) | (r << 16) | (g << 8) | b;
                    }
                }
            }
        }
    }

    pub fn copy_to_db_clipped(&self, width: u32, height: u32, buffer: usize, x: i32, y: i32, clip_x: i32, clip_y: i32, clip_w: u32, clip_h: u32) {
        let dst_pitch = self.pitch as usize / 4; // Pitch in u32
        let src_pitch = width as usize;          // Pitch in u32
        let screen_w = self.width as i32;
        let screen_h = self.height as i32;

        // 1. Calculate Intersection of Window and Clip Rect
        let win_x = x;
        let win_y = y;
        let win_w = width as i32;
        let win_h = height as i32;

        let cx = clip_x;
        let cy = clip_y;
        let cw = clip_w as i32;
        let ch = clip_h as i32;

        let intersect_x = win_x.max(cx).max(0);
        let intersect_y = win_y.max(cy).max(0);
        let intersect_end_x = (win_x + win_w).min(cx + cw).min(screen_w);
        let intersect_end_y = (win_y + win_h).min(cy + ch).min(screen_h);

        if intersect_end_x <= intersect_x || intersect_end_y <= intersect_y {
            return;
        }

        let copy_width = (intersect_end_x - intersect_x) as usize;
        let copy_height = (intersect_end_y - intersect_y) as usize;

        // 2. Calculate Offsets
        let src_off_x = (intersect_x - win_x) as usize;
        let src_off_y = (intersect_y - win_y) as usize;

        unsafe {
            let src_base = buffer as *const u32;
            let dst_base = self.double_buffer as *mut u32;

            for row in 0..copy_height {
                // Source pointer: window_buffer[ (src_y + row) * width + src_x ]
                let src_row_ptr = src_base.add((src_off_y + row) * src_pitch + src_off_x);
                
                // Dest pointer:   db[ (dst_y + row) * pitch + dst_x ]
                // intersect_y is absolute screen Y
                let dst_row_ptr = dst_base.add((intersect_y as usize + row) * dst_pitch + (intersect_x as usize));

                for col in 0..copy_width {
                    let src_pixel = *src_row_ptr.add(col);
                    let alpha = (src_pixel >> 24) & 0xFF;

                    if alpha == 255 {
                        *dst_row_ptr.add(col) = src_pixel;
                    } else if alpha == 0 {
                        continue;
                    } else {
                        let dst_pixel = *dst_row_ptr.add(col);
                        
                        let inv_alpha = 255 - alpha;

                        let src_r = (src_pixel >> 16) & 0xFF;
                        let src_g = (src_pixel >> 8) & 0xFF;
                        let src_b = src_pixel & 0xFF;

                        let dst_r = (dst_pixel >> 16) & 0xFF;
                        let dst_g = (dst_pixel >> 8) & 0xFF;
                        let dst_b = dst_pixel & 0xFF;

                        let r = (src_r * alpha + dst_r * inv_alpha) >> 8;
                        let g = (src_g * alpha + dst_g * inv_alpha) >> 8;
                        let b = (src_b * alpha + dst_b * inv_alpha) >> 8;

                        *dst_row_ptr.add(col) = (0xFF << 24) | (r << 16) | (g << 8) | b;
                    }
                }
            }
        }
    }

    pub fn copy_to_fb_a(&self, width: u32, height: u32, buffer: usize, x: i32, y: i32) {
        let dst_pitch = self.pitch as usize / 4;
        let src_pitch = width as usize;
        let screen_w = self.width as i32;
        let screen_h = self.height as i32;

        let dst_x = x.max(0);
        let dst_y = y.max(0);
        let end_x = (x + width as i32).min(screen_w);
        let end_y = (y + height as i32).min(screen_h);

        if end_x <= dst_x || end_y <= dst_y { return; }

        let copy_width = (end_x - dst_x) as usize;
        let copy_height = (end_y - dst_y) as usize;
        
        let src_off_x = (dst_x - x) as usize;
        let src_off_y = (dst_y - y) as usize;

        unsafe {
            let src_base = buffer as *const u32;
            let dst_base = self.framebuffer as *mut u32; 

            for row in 0..copy_height {
                let src_row_ptr = src_base.add((src_off_y + row) * src_pitch + src_off_x);
                let dst_row_ptr = dst_base.add((dst_y as usize + row) * dst_pitch + (dst_x as usize));

                for col in 0..copy_width {
                    let src_pixel = *src_row_ptr.add(col);
                    let alpha = (src_pixel >> 24) & 0xFF;

                    if alpha == 255 {
                        *dst_row_ptr.add(col) = src_pixel;
                    } else if alpha == 0 {
                        continue;
                    } else {
                        let dst_pixel = *dst_row_ptr.add(col);
                        
                        let inv_alpha = 255 - alpha;

                        let src_r = (src_pixel >> 16) & 0xFF;
                        let src_g = (src_pixel >> 8) & 0xFF;
                        let src_b = src_pixel & 0xFF;

                        let dst_r = (dst_pixel >> 16) & 0xFF;
                        let dst_g = (dst_pixel >> 8) & 0xFF;
                        let dst_b = dst_pixel & 0xFF;

                        let r = (src_r * alpha + dst_r * inv_alpha) >> 8;
                        let g = (src_g * alpha + dst_g * inv_alpha) >> 8;
                        let b = (src_b * alpha + dst_b * inv_alpha) >> 8;

                        *dst_row_ptr.add(col) = (0xFF << 24) | (r << 16) | (g << 8) | b;
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

    pub fn present_rect(&self, x: i32, y: i32, w: u32, h: u32) {
        let sx = x.max(0) as u32;
        let sy = y.max(0) as u32;
        let sw = w.min(self.width as u32 - sx);
        let sh = h.min(self.height as u32 - sy);

        unsafe {
            if VIRTIO_ACTIVE {
                // For partial updates, we flush directly to the active resource
                // This assumes we are drawing into the backing memory of active_resource_id
                // Wait, copy_to_db writes to self.double_buffer.
                // If double_buffer != framebuffer (active), we are writing to the back buffer.
                // If we flush the BACK buffer, it won't show up if scanout is on FRONT buffer.
                
                // FORCE: For dirty rects, we must copy DB -> FB (RAM copy) then Flush FB -> GPU.
                // Or, simple mode: Just copy DB -> FB and flush.
                
                let bpp = 4;
                let pitch = self.pitch as usize;
                let src = self.double_buffer as *const u8;
                let dst = self.framebuffer as *mut u8; // Active buffer
                
                // 1. Sync RAM (DB -> FB) for this rect
                for row in 0..sh {
                    let offset = ((sy + row) as usize * pitch + sx as usize * bpp);
                    core::ptr::copy_nonoverlapping(src.add(offset), dst.add(offset), (sw * bpp as u32) as usize);
                }
                
                // 2. Flush to GPU
                virtio::flush(sx, sy, sw, sh, self.width as u32, self.active_resource_id);
            } else {
                // VBE: Copy DB -> FB
                self.copy_to_fb(x, y, w, h);
            }
        }
    }

    pub fn draw_mouse(&self, x: u16, y: u16, dragging_window: bool) {
        use crate::drivers::periferics::mouse::{CURSOR_BUFFER, CURSOR_WIDTH, CURSOR_HEIGHT};

        /* Hardware cursor disabled by request
        unsafe {
            if VIRTIO_ACTIVE {
                let cx = (x as u32).min(self.width as u32 - 1);
                let cy = (y as u32).min(self.height as u32 - 1);
                virtio::cursor::move_cursor(cx, cy);
                return;
            }
        }
        */

        let pitch_bytes = self.pitch as usize;
        let fb_ptr = self.framebuffer as *mut u32; 
        let db_ptr = self.double_buffer as *const u32; 
        let width = self.width as usize;
        let height = self.height as usize;
        let mx = x as usize;
        let my = y as usize;

        let bg_src = if dragging_window { fb_ptr as *const u32 } else { db_ptr };

        unsafe {
            let fb_pitch_u32 = pitch_bytes / 4;

            for row in 0..CURSOR_HEIGHT {
                let screen_y = my + row;
                if screen_y >= height { break; }

                let fb_row_start = screen_y * fb_pitch_u32 + mx;
                let cursor_row_start = row * CURSOR_WIDTH;

                for col in 0..CURSOR_WIDTH {
                    let screen_x = mx + col;
                    if screen_x >= width { break; }

                    let cursor_color = CURSOR_BUFFER[cursor_row_start + col];
                    
                    if cursor_color != 0 {
                        *fb_ptr.add(fb_row_start + col) = cursor_color;
                    } else if !dragging_window {
                        let bg_color = *bg_src.add(fb_row_start + col);
                        *fb_ptr.add(fb_row_start + col) = bg_color;
                    }
                }
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