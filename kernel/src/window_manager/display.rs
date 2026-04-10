use crate::drivers::video::virtio;
use crate::{debugln, println};
use core::arch::x86_64::*;

pub const DEPTH: u8 = 32;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

pub struct DisplayServer {
    pub width: u64,
    pub pitch: u64,
    pub height: u64,
    pub depth: usize,

    pub framebuffer: u64,
    pub double_buffer: u64,

    pub buffer1_phys: u64,
    pub buffer2_phys: u64,
    pub buffer1_virt: u64,
    pub buffer2_virt: u64,
    pub active_resource_id: u32,

    pub vbe_framebuffer: u64,

    // Damage tracking
    pub damage_rects: [Option<Rect>; 8],
    pub has_damage: bool,
}

use crate::sync::Mutex;

pub static DISPLAY_SERVER: Mutex<DisplayServer> = Mutex::new(DisplayServer {
    width: 0,
    height: 0,
    pitch: 0,
    depth: 32,
    framebuffer: 0,
    double_buffer: 0,
    buffer1_phys: 0,
    buffer2_phys: 0,
    buffer1_virt: 0,
    buffer2_virt: 0,
    active_resource_id: 1,
    vbe_framebuffer: 0,
    damage_rects: [None; 8],
    has_damage: false,
});

use core::sync::atomic::{AtomicBool, Ordering};
pub static VIRTIO_ACTIVE: AtomicBool = AtomicBool::new(false);
pub static HARDWARE_CURSOR_ACTIVE: AtomicBool = AtomicBool::new(false);

impl DisplayServer {
    pub fn mark_dirty(&mut self, x: i32, y: i32, w: u32, h: u32) {
        if w == 0 || h == 0 { return; }
        
        // Try to merge with or add to damage list
        for i in 0..8 {
            if let Some(r) = self.damage_rects[i] {
                // Check for overlap or containment
                if x >= r.x && y >= r.y && (x + w as i32) <= (r.x + r.w as i32) && (y + h as i32) <= (r.y + r.h as i32) {
                    return; // Already covered
                }
            } else {
                self.damage_rects[i] = Some(Rect { x, y, w, h });
                self.has_damage = true;
                return;
            }
        }
        
        // If list is full, merge into a bounding box in the first slot
        let mut min_x = x;
        let mut min_y = y;
        let mut max_x = x + w as i32;
        let mut max_y = y + h as i32;
        
        for i in 0..8 {
            if let Some(r) = self.damage_rects[i] {
                if r.x < min_x { min_x = r.x; }
                if r.y < min_y { min_y = r.y; }
                if (r.x + r.w as i32) > max_x { max_x = r.x + r.w as i32; }
                if (r.y + r.h as i32) > max_y { max_y = r.y + r.h as i32; }
                self.damage_rects[i] = None;
            }
        }
        self.damage_rects[0] = Some(Rect { 
            x: min_x, 
            y: min_y, 
            w: (max_x - min_x) as u32, 
            h: (max_y - min_y) as u32 
        });
        self.has_damage = true;
    }

    pub fn reset_dirty(&mut self) {
        for i in 0..8 { self.damage_rects[i] = None; }
        self.has_damage = false;
    }

    pub fn init(&mut self) {
        let boot_info = unsafe { crate::boot::BOOT_INFO };
        let vbe = boot_info.mode;

        self.width = vbe.width as u64;
        self.height = vbe.height as u64;
        self.reset_dirty();

        unsafe {
            virtio::init();
            if virtio::queue::VIRT_QUEUES[0].is_some() {
                if let Some((w, h)) = virtio::get_display_info() {
                    debugln!("DisplayServer: VirtIO display info: {}x{}", w, h);
                    if w > 0 && h > 0 {
                        self.width = w as u64;
                        self.height = h as u64;
                    }
                }

                if self.width == 0 || self.height == 0 {
                    self.width = 1024;
                    self.height = 576;
                }

                debugln!("DisplayServer: Using resolution {}x{}", self.width, self.height);

                self.pitch = self.width * 4;
                self.depth = 32;

                let size_bytes = (self.pitch * self.height) as usize;
                let pages = (size_bytes + 4095) / 4096;

                let b1 = crate::memory::pmm::allocate_frames(pages).expect("Failed to allocate buffer 1");
                let b2 = crate::memory::pmm::allocate_frames(pages).expect("Failed to allocate buffer 2");

                // Map framebuffers as uncacheable MMIO so the GPU sees updates immediately
                let b1_virt = crate::memory::vmm::map_mmio(b1, size_bytes);
                
                // Use standard cached RAM for the double buffer to make alpha blending (read-modify-write) 100x faster
                let ram_db = crate::memory::pmm::allocate_frames(pages).expect("Failed to allocate RAM buffer");
                let ram_virt = ram_db + crate::memory::paging::HHDM_OFFSET;

                {
                    let b1_ptr = b1_virt as *mut u32;
                    let db_ptr = ram_virt as *mut u32;
                    for i in 0..(self.width * self.height) as usize {
                        *b1_ptr.add(i) = 0xFF333333; // Dark gray
                        *db_ptr.add(i) = 0xFF333333; 
                    }
                }

                self.buffer1_phys = b1;
                self.buffer2_phys = b2;
                self.buffer1_virt = b1_virt;
                self.buffer2_virt = 0; // Unused in single-buffer mode

                self.framebuffer = b1_virt;
                self.double_buffer = ram_virt;
                self.active_resource_id = 1;

                // Ensure all cached zeroes from PMM are flushed to physical RAM
                // before the GPU tries to read them via DMA.
                core::arch::asm!("wbinvd", options(nostack, preserves_flags));

                virtio::start_gpu(self.width as u32, self.height as u32, self.buffer1_phys, self.buffer2_phys);
                virtio::set_scanout(1, self.width as u32, self.height as u32);
                virtio::transfer_and_flush(1, self.width as u32, self.height as u32, true); // Blocking init

                use crate::drivers::peripherals::mouse::{CURSOR_BUFFER, CURSOR_HEIGHT, CURSOR_WIDTH};
                let cursor_size_bytes = 64 * 64 * 4;
                let cursor_pages = (cursor_size_bytes + 4095) / 4096;
                if let Some(cursor_phys) = crate::memory::pmm::allocate_frames(cursor_pages) {
                    let cursor_ptr = (cursor_phys + crate::memory::paging::HHDM_OFFSET) as *mut u32;
                    core::ptr::write_bytes(cursor_ptr as *mut u8, 0, cursor_size_bytes);
                    for row in 0..CURSOR_HEIGHT {
                        for col in 0..CURSOR_WIDTH {
                            *cursor_ptr.add(row * 64 + col) = CURSOR_BUFFER[row * CURSOR_WIDTH + col];
                        }
                    }
                    virtio::cursor::setup_cursor(cursor_phys, 64, 64, 0, 0);
                    HARDWARE_CURSOR_ACTIVE.store(true, Ordering::SeqCst);
                }

                VIRTIO_ACTIVE.store(true, Ordering::SeqCst);
                println!("DisplayServer: VirtIO GPU active at {}x{}", self.width, self.height);
                return;
            }
        }


        println!("DisplayServer: Using VBE fallback");
        if vbe.width == 0 || vbe.height == 0 {
            println!("DisplayServer: VBE reports 0x0 resolution. Graphics unavailable.");
            return;
        }
        self.width = vbe.width as u64;
        self.pitch = vbe.pitch as u64;
        self.height = vbe.height as u64;
        self.depth = 32;

        let size_bytes = self.pitch as usize * self.height as usize;

        // Map VBE framebuffer as uncacheable MMIO so writes reach the device
        self.framebuffer = crate::memory::vmm::map_mmio(vbe.framebuffer as u64, size_bytes);

        let pages = (size_bytes + 4095) / 4096;

        unsafe {
            if let Some(buffer) = crate::memory::pmm::allocate_frames(pages) {
                self.double_buffer = buffer + crate::memory::paging::HHDM_OFFSET;
                core::ptr::write_bytes(self.double_buffer as *mut u8, 0, size_bytes);
            } else {
                panic!("[DisplayServer] Failed to allocate double buffer!");
            }
        }
    }

    pub fn force_full_sync(&mut self, wait: bool) {
        let fb_len = (self.pitch * self.height) as usize;
        let src = self.double_buffer as *const u8;
        
        if self.vbe_framebuffer != 0 {
            unsafe { core::ptr::copy_nonoverlapping(src, self.vbe_framebuffer as *mut u8, fb_len); }
        }

        unsafe {
            if VIRTIO_ACTIVE.load(Ordering::SeqCst) {
                virtio::flush(0, 0, self.width as u32, self.height as u32, self.width as u32, self.active_resource_id, wait);
            }
        }
    }

    pub fn copy(&mut self) {
        if VIRTIO_ACTIVE.load(Ordering::SeqCst) {
            if self.has_damage {
                // 1. Calculate a single bounding box for all damage this frame
                let mut min_x = self.width as i32;
                let mut min_y = self.height as i32;
                let mut max_x = 0i32;
                let mut max_y = 0i32;
                
                for i in 0..8 {
                    if let Some(r) = self.damage_rects[i] {
                        if r.x < min_x { min_x = r.x; }
                        if r.y < min_y { min_y = r.y; }
                        if (r.x + r.w as i32) > max_x { max_x = r.x + r.w as i32; }
                        if (r.y + r.h as i32) > max_y { max_y = r.y + r.h as i32; }
                    }
                }

                let sx = min_x.max(0) as u32;
                let sy = min_y.max(0) as u32;
                let ex = max_x.min(self.width as i32).max(0) as u32;
                let ey = max_y.min(self.height as i32).max(0) as u32;
                
                let sw = ex.saturating_sub(sx);
                let sh = ey.saturating_sub(sy);

                if sw > 0 && sh > 0 {
                    let pitch = self.pitch as usize;
                    let src = self.double_buffer as *const u8;
                    let dst = self.framebuffer as *mut u8;
                    unsafe {
                        for row in 0..sh {
                            let offset = (sy + row) as usize * pitch + (sx as usize * 4);
                            core::ptr::copy_nonoverlapping(
                                src.add(offset),
                                dst.add(offset),
                                (sw * 4) as usize,
                            );
                        }

                        // NON-BLOCKING transfer and flush to the single active resource
                        virtio::flush(sx, sy, sw, sh, self.width as u32, self.active_resource_id, false);
                    }
                    self.sync_vbe_rect(sx, sy, sw, sh);
                }
                
                self.reset_dirty();
            }
        } else {
            let buffer_size = (self.pitch * self.height) as usize;
            unsafe {
                core::ptr::copy(
                    self.double_buffer as *const u8,
                    self.framebuffer as *mut u8,
                    buffer_size,
                );
            }
        }
    }

    pub fn copy_to_fb(&mut self, x: i32, y: i32, width: u32, height: u32) {
        let screen_w = self.width as i32;
        let screen_h = self.height as i32;

        let dst_x = x.max(0);
        let dst_y = y.max(0);
        let end_x = (x + width as i32).min(screen_w);
        let end_y = (y + height as i32).min(screen_h);

        if end_x <= dst_x || end_y <= dst_y { return; }

        let copy_width = (end_x - dst_x) as usize;
        let copy_height = (end_y - dst_y) as usize;

        self.mark_dirty(dst_x, dst_y, copy_width as u32, copy_height as u32);

        unsafe {
            let src_base = self.double_buffer as *const u32;
            let dst_base = self.framebuffer as *mut u32;
            let pitch_u32 = self.pitch as usize / 4;

            for row in 0..copy_height {
                let offset = (dst_y as usize + row) * pitch_u32 + dst_x as usize;
                core::ptr::copy_nonoverlapping(
                    src_base.add(offset),
                    dst_base.add(offset),
                    copy_width,
                );
            }
        }
    }

    pub fn copy_to_db(&mut self, width: u32, height: u32, buffer: usize, x: i32, y: i32, border_color: Option<u32>, treat_as_transparent: bool) {
        let screen_w = self.width as i32;
        let screen_h = self.height as i32;

        let dst_x = x.max(0);
        let dst_y = y.max(0);
        let end_x = (x + width as i32).min(screen_w);
        let end_y = (y + height as i32).min(screen_h);

        if buffer == 0 { return; }

        if end_x <= dst_x || end_y <= dst_y { return; }

        let copy_width = (end_x - dst_x) as usize;
        let copy_height = (end_y - dst_y) as usize;

        let src_off_x = (dst_x - x) as usize;
        let src_off_y = (dst_y - y) as usize;

        self.mark_dirty(dst_x, dst_y, copy_width as u32, copy_height as u32);

        unsafe {
            let src_base = buffer as *const u32;
            let dst_base = self.double_buffer as *mut u32;
            let dst_pitch = self.pitch as usize / 4;
            let src_pitch = width as usize;

            for row in 0..copy_height {
                let src_row_ptr = src_base.add((src_off_y + row) * src_pitch + src_off_x);
                let dst_row_ptr = dst_base.add((dst_y as usize + row) * dst_pitch + (dst_x as usize));
                let is_top_or_bottom = (src_off_y + row) == 0 || (src_off_y + row) == (height as usize - 1);

                if !treat_as_transparent && !is_top_or_bottom && border_color.is_none() {
                    core::ptr::copy_nonoverlapping(src_row_ptr, dst_row_ptr, copy_width);
                    continue;
                }

                if is_top_or_bottom {
                    for col in 0..copy_width {
                        let in_window_x = src_off_x + col;
                        let is_border = in_window_x == 0 || in_window_x == (width as usize - 1) || is_top_or_bottom;

                        if is_border {
                            if let Some(color) = border_color {
                                *dst_row_ptr.add(col) = color;
                                continue;
                            }
                        }


                        let src_pixel = *src_row_ptr.add(col);
                        let alpha = if treat_as_transparent { (src_pixel >> 24) & 0xFF } else { 255 };
                        if alpha == 255 {
                            *dst_row_ptr.add(col) = src_pixel;
                        } else if alpha != 0 {
                            let dst_pixel = *dst_row_ptr.add(col);
                            let inv_alpha = 255 - alpha;
                            let r = (((src_pixel >> 16) & 0xFF) * alpha + ((dst_pixel >> 16) & 0xFF) * inv_alpha) >> 8;
                            let g = (((src_pixel >> 8) & 0xFF) * alpha + ((dst_pixel >> 8) & 0xFF) * inv_alpha) >> 8;
                            let b = ((src_pixel & 0xFF) * alpha + (dst_pixel & 0xFF) * inv_alpha) >> 8;
                            *dst_row_ptr.add(col) = (0xFF << 24) | (r << 16) | (g << 8) | b;
                        }
                    }
                    continue;
                }


                let mut col = 0;


                if copy_width > 0 {
                    let in_window_x = src_off_x + col;
                    if in_window_x == 0 {
                        if let Some(color) = border_color {
                            *dst_row_ptr.add(col) = color;
                        } else {
                            let src_pixel = *src_row_ptr.add(col);
                            let alpha = if treat_as_transparent { (src_pixel >> 24) & 0xFF } else { 255 };
                            if alpha == 255 { *dst_row_ptr.add(col) = src_pixel; } else if alpha != 0 {
                                let dst_pixel = *dst_row_ptr.add(col);
                                let inv_alpha = 255 - alpha;
                                let r = (((src_pixel >> 16) & 0xFF) * alpha + ((dst_pixel >> 16) & 0xFF) * inv_alpha) >> 8;
                                let g = (((src_pixel >> 8) & 0xFF) * alpha + ((dst_pixel >> 8) & 0xFF) * inv_alpha) >> 8;
                                let b = ((src_pixel & 0xFF) * alpha + (dst_pixel & 0xFF) * inv_alpha) >> 8;
                                *dst_row_ptr.add(col) = (0xFF << 24) | (r << 16) | (g << 8) | b;
                            }
                        }
                        col += 1;
                    }
                }


                let simd_end = if copy_width >= 4 { copy_width - 3 } else { 0 };


                while col < simd_end {
                    if (src_off_x + col + 4) >= (width as usize) {
                        break;
                    }

                    let src_vec = _mm_loadu_si128(src_row_ptr.add(col) as *const __m128i);

                    if !treat_as_transparent {
                        _mm_storeu_si128(dst_row_ptr.add(col) as *mut __m128i, src_vec);
                        col += 4;
                        continue;
                    }

                    let alphas = _mm_srli_epi32(src_vec, 24);

                    let all_opaque_mask = _mm_cmpeq_epi32(alphas, _mm_set1_epi32(255));
                    let mask_bits = _mm_movemask_epi8(all_opaque_mask);

                    if mask_bits == 0xFFFF {
                        _mm_storeu_si128(dst_row_ptr.add(col) as *mut __m128i, src_vec);
                        col += 4;
                        continue;
                    }


                    let all_transp_mask = _mm_cmpeq_epi32(alphas, _mm_setzero_si128());
                    let t_mask_bits = _mm_movemask_epi8(all_transp_mask);
                    if t_mask_bits == 0xFFFF {
                        col += 4;
                        continue;
                    }


                    let zero = _mm_setzero_si128();
                    let src_lo = _mm_unpacklo_epi8(src_vec, zero);
                    let src_hi = _mm_unpackhi_epi8(src_vec, zero);


                    let alpha_lo_32 = _mm_unpacklo_epi32(alphas, alphas);
                    let alpha_lo_16 = _mm_or_si128(alpha_lo_32, _mm_slli_epi32(alpha_lo_32, 16));
                    let alpha_hi_32 = _mm_unpackhi_epi32(alphas, alphas);
                    let alpha_hi_16 = _mm_or_si128(alpha_hi_32, _mm_slli_epi32(alpha_hi_32, 16));


                    let dst_vec = _mm_loadu_si128(dst_row_ptr.add(col) as *const __m128i);
                    let dst_lo = _mm_unpacklo_epi8(dst_vec, zero);
                    let dst_hi = _mm_unpackhi_epi8(dst_vec, zero);


                    let const_255 = _mm_set1_epi16(255);
                    let inv_alpha_lo = _mm_sub_epi16(const_255, alpha_lo_16);
                    let inv_alpha_hi = _mm_sub_epi16(const_255, alpha_hi_16);


                    let src_lo_mul = _mm_mullo_epi16(src_lo, alpha_lo_16);
                    let src_hi_mul = _mm_mullo_epi16(src_hi, alpha_hi_16);
                    let dst_lo_mul = _mm_mullo_epi16(dst_lo, inv_alpha_lo);
                    let dst_hi_mul = _mm_mullo_epi16(dst_hi, inv_alpha_hi);


                    let res_lo = _mm_add_epi16(src_lo_mul, dst_lo_mul);
                    let res_hi = _mm_add_epi16(src_hi_mul, dst_hi_mul);


                    let res_lo_shifted = _mm_srli_epi16(res_lo, 8);
                    let res_hi_shifted = _mm_srli_epi16(res_hi, 8);


                    let result = _mm_packus_epi16(res_lo_shifted, res_hi_shifted);


                    let alpha_mask = _mm_set1_epi32(0xFF000000u32 as i32);
                    let final_res = _mm_or_si128(result, alpha_mask);

                    _mm_storeu_si128(dst_row_ptr.add(col) as *mut __m128i, final_res);
                    col += 4;
                }


                while col < copy_width {
                    let in_window_x = src_off_x + col;
                    let is_border = in_window_x == 0 || in_window_x == (width as usize - 1);

                    if is_border {
                        if let Some(color) = border_color {
                            *dst_row_ptr.add(col) = color;
                        } else {
                            let src_pixel = *src_row_ptr.add(col);
                            let alpha = if treat_as_transparent { (src_pixel >> 24) & 0xFF } else { 255 };
                            if alpha == 255 { *dst_row_ptr.add(col) = src_pixel; } else if alpha != 0 {
                                let dst_pixel = *dst_row_ptr.add(col);
                                let inv_alpha = 255 - alpha;
                                let r = (((src_pixel >> 16) & 0xFF) * alpha + ((dst_pixel >> 16) & 0xFF) * inv_alpha) >> 8;
                                let g = (((src_pixel >> 8) & 0xFF) * alpha + ((dst_pixel >> 8) & 0xFF) * inv_alpha) >> 8;
                                let b = ((src_pixel & 0xFF) * alpha + (dst_pixel & 0xFF) * inv_alpha) >> 8;
                                *dst_row_ptr.add(col) = (0xFF << 24) | (r << 16) | (g << 8) | b;
                            }
                        }
                    } else {
                        let src_pixel = *src_row_ptr.add(col);
                        let alpha = if treat_as_transparent { (src_pixel >> 24) & 0xFF } else { 255 };
                        if alpha == 255 {
                            *dst_row_ptr.add(col) = src_pixel;
                        } else if alpha != 0 {
                            let dst_pixel = *dst_row_ptr.add(col);
                            let inv_alpha = 255 - alpha;
                            let r = (((src_pixel >> 16) & 0xFF) * alpha + ((dst_pixel >> 16) & 0xFF) * inv_alpha) >> 8;
                            let g = (((src_pixel >> 8) & 0xFF) * alpha + ((dst_pixel >> 8) & 0xFF) * inv_alpha) >> 8;
                            let b = ((src_pixel & 0xFF) * alpha + (dst_pixel & 0xFF) * inv_alpha) >> 8;
                            *dst_row_ptr.add(col) = (0xFF << 24) | (r << 16) | (g << 8) | b;
                        }
                    }
                    col += 1;
                }
            }
        }
    }


    pub fn copy_to_db_clipped(&mut self, width: u32, height: u32, buffer: usize, x: i32, y: i32, clip_x: i32, clip_y: i32, clip_w: u32, clip_h: u32, border_color: Option<u32>, treat_as_transparent: bool) {
        let screen_w = self.width as i32;
        let screen_h = self.height as i32;


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

        if buffer == 0 { return; }

        if intersect_end_x <= intersect_x || intersect_end_y <= intersect_y {
            return;
        }

        let copy_width = (intersect_end_x - intersect_x) as usize;
        let copy_height = (intersect_end_y - intersect_y) as usize;

        self.mark_dirty(intersect_x, intersect_y, copy_width as u32, copy_height as u32);

        let src_off_x = (intersect_x - win_x) as usize;
        let src_off_y = (intersect_y - win_y) as usize;

        unsafe {
            let src_base = buffer as *const u32;
            let dst_base = self.double_buffer as *mut u32;
            let dst_pitch = self.pitch as usize / 4;
            let src_pitch = width as usize;

            for row in 0..copy_height {
                let src_row_ptr = src_base.add((src_off_y + row) * src_pitch + src_off_x);


                let dst_row_ptr = dst_base.add((intersect_y as usize + row) * dst_pitch + (intersect_x as usize));

                let is_top_or_bottom = (src_off_y + row) == 0 || (src_off_y + row) == (height as usize - 1);

                if !treat_as_transparent && !is_top_or_bottom && border_color.is_none() {
                    core::ptr::copy_nonoverlapping(src_row_ptr, dst_row_ptr, copy_width);
                    continue;
                }

                if is_top_or_bottom {
                    for col in 0..copy_width {
                        let in_window_x = src_off_x + col;
                        let is_border = in_window_x == 0 || in_window_x == (width as usize - 1) || is_top_or_bottom;

                        if is_border {
                            if let Some(color) = border_color {
                                *dst_row_ptr.add(col) = color;
                                continue;
                            }
                        }


                        let src_pixel = *src_row_ptr.add(col);
                        let alpha = if treat_as_transparent { (src_pixel >> 24) & 0xFF } else { 255 };
                        if alpha == 255 {
                            *dst_row_ptr.add(col) = src_pixel;
                        } else if alpha != 0 {
                            let dst_pixel = *dst_row_ptr.add(col);
                            let inv_alpha = 255 - alpha;
                            let r = (((src_pixel >> 16) & 0xFF) * alpha + ((dst_pixel >> 16) & 0xFF) * inv_alpha) >> 8;
                            let g = (((src_pixel >> 8) & 0xFF) * alpha + ((dst_pixel >> 8) & 0xFF) * inv_alpha) >> 8;
                            let b = ((src_pixel & 0xFF) * alpha + (dst_pixel & 0xFF) * inv_alpha) >> 8;
                            *dst_row_ptr.add(col) = (0xFF << 24) | (r << 16) | (g << 8) | b;
                        }
                    }
                    continue;
                }


                let mut col = 0;


                if copy_width > 0 {
                    let in_window_x = src_off_x + col;
                    if in_window_x == 0 {
                        if let Some(color) = border_color {
                            *dst_row_ptr.add(col) = color;
                        } else {
                            let src_pixel = *src_row_ptr.add(col);
                            let alpha = if treat_as_transparent { (src_pixel >> 24) & 0xFF } else { 255 };
                            if alpha == 255 { *dst_row_ptr.add(col) = src_pixel; } else if alpha != 0 {
                                let dst_pixel = *dst_row_ptr.add(col);
                                let inv_alpha = 255 - alpha;
                                let r = (((src_pixel >> 16) & 0xFF) * alpha + ((dst_pixel >> 16) & 0xFF) * inv_alpha) >> 8;
                                let g = (((src_pixel >> 8) & 0xFF) * alpha + ((dst_pixel >> 8) & 0xFF) * inv_alpha) >> 8;
                                let b = ((src_pixel & 0xFF) * alpha + (dst_pixel & 0xFF) * inv_alpha) >> 8;
                                *dst_row_ptr.add(col) = (0xFF << 24) | (r << 16) | (g << 8) | b;
                            }
                        }
                        col += 1;
                    }
                }


                let simd_end = if copy_width >= 4 { copy_width - 3 } else { 0 };


                while col < simd_end {
                    if (src_off_x + col + 4) >= (width as usize) {
                        break;
                    }

                    let src_vec = _mm_loadu_si128(src_row_ptr.add(col) as *const __m128i);

                    if !treat_as_transparent {
                        _mm_storeu_si128(dst_row_ptr.add(col) as *mut __m128i, src_vec);
                        col += 4;
                        continue;
                    }

                    let alphas = _mm_srli_epi32(src_vec, 24);


                    let all_opaque_mask = _mm_cmpeq_epi32(alphas, _mm_set1_epi32(255));
                    let mask_bits = _mm_movemask_epi8(all_opaque_mask);

                    if mask_bits == 0xFFFF {
                        _mm_storeu_si128(dst_row_ptr.add(col) as *mut __m128i, src_vec);
                        col += 4;
                        continue;
                    }


                    let all_transp_mask = _mm_cmpeq_epi32(alphas, _mm_setzero_si128());
                    let t_mask_bits = _mm_movemask_epi8(all_transp_mask);
                    if t_mask_bits == 0xFFFF {
                        col += 4;
                        continue;
                    }


                    let zero = _mm_setzero_si128();
                    let src_lo = _mm_unpacklo_epi8(src_vec, zero);
                    let src_hi = _mm_unpackhi_epi8(src_vec, zero);


                    let alpha_lo_32 = _mm_unpacklo_epi32(alphas, alphas);
                    let alpha_lo_16 = _mm_or_si128(alpha_lo_32, _mm_slli_epi32(alpha_lo_32, 16));
                    let alpha_hi_32 = _mm_unpackhi_epi32(alphas, alphas);
                    let alpha_hi_16 = _mm_or_si128(alpha_hi_32, _mm_slli_epi32(alpha_hi_32, 16));


                    let dst_vec = _mm_loadu_si128(dst_row_ptr.add(col) as *const __m128i);
                    let dst_lo = _mm_unpacklo_epi8(dst_vec, zero);
                    let dst_hi = _mm_unpackhi_epi8(dst_vec, zero);


                    let const_255 = _mm_set1_epi16(255);
                    let inv_alpha_lo = _mm_sub_epi16(const_255, alpha_lo_16);
                    let inv_alpha_hi = _mm_sub_epi16(const_255, alpha_hi_16);


                    let src_lo_mul = _mm_mullo_epi16(src_lo, alpha_lo_16);
                    let src_hi_mul = _mm_mullo_epi16(src_hi, alpha_hi_16);
                    let dst_lo_mul = _mm_mullo_epi16(dst_lo, inv_alpha_lo);
                    let dst_hi_mul = _mm_mullo_epi16(dst_hi, inv_alpha_hi);


                    let res_lo = _mm_add_epi16(src_lo_mul, dst_lo_mul);
                    let res_hi = _mm_add_epi16(src_hi_mul, dst_hi_mul);


                    let res_lo_shifted = _mm_srli_epi16(res_lo, 8);
                    let res_hi_shifted = _mm_srli_epi16(res_hi, 8);


                    let result = _mm_packus_epi16(res_lo_shifted, res_hi_shifted);


                    let alpha_mask = _mm_set1_epi32(0xFF000000u32 as i32);
                    let final_res = _mm_or_si128(result, alpha_mask);

                    _mm_storeu_si128(dst_row_ptr.add(col) as *mut __m128i, final_res);
                    col += 4;
                }


                while col < copy_width {
                    let in_window_x = src_off_x + col;
                    let is_border = in_window_x == 0 || in_window_x == (width as usize - 1);

                    if is_border {
                        if let Some(color) = border_color {
                            *dst_row_ptr.add(col) = color;
                        } else {
                            let src_pixel = *src_row_ptr.add(col);
                            let alpha = if treat_as_transparent { (src_pixel >> 24) & 0xFF } else { 255 };
                            if alpha == 255 { *dst_row_ptr.add(col) = src_pixel; } else if alpha != 0 {
                                let dst_pixel = *dst_row_ptr.add(col);
                                let inv_alpha = 255 - alpha;
                                let r = (((src_pixel >> 16) & 0xFF) * alpha + ((dst_pixel >> 16) & 0xFF) * inv_alpha) >> 8;
                                let g = (((src_pixel >> 8) & 0xFF) * alpha + ((dst_pixel >> 8) & 0xFF) * inv_alpha) >> 8;
                                let b = ((src_pixel & 0xFF) * alpha + (dst_pixel & 0xFF) * inv_alpha) >> 8;
                                *dst_row_ptr.add(col) = (0xFF << 24) | (r << 16) | (g << 8) | b;
                            }
                        }
                    } else {
                        let src_pixel = *src_row_ptr.add(col);
                        let alpha = if treat_as_transparent { (src_pixel >> 24) & 0xFF } else { 255 };
                        if alpha == 255 {
                            *dst_row_ptr.add(col) = src_pixel;
                        } else if alpha != 0 {
                            let dst_pixel = *dst_row_ptr.add(col);
                            let inv_alpha = 255 - alpha;
                            let r = (((src_pixel >> 16) & 0xFF) * alpha + ((dst_pixel >> 16) & 0xFF) * inv_alpha) >> 8;
                            let g = (((src_pixel >> 8) & 0xFF) * alpha + ((dst_pixel >> 8) & 0xFF) * inv_alpha) >> 8;
                            let b = ((src_pixel & 0xFF) * alpha + (dst_pixel & 0xFF) * inv_alpha) >> 8;
                            *dst_row_ptr.add(col) = (0xFF << 24) | (r << 16) | (g << 8) | b;
                        }
                    }
                    col += 1;
                }
            }
        }
    }

    pub fn copy_to_fb_clipped(&mut self, width: u32, height: u32, buffer: usize, x: i32, y: i32, clip_x: i32, clip_y: i32, clip_w: u32, clip_h: u32, border_color: Option<u32>, treat_as_transparent: bool) {
        let screen_w = self.width as i32;
        let screen_h = self.height as i32;

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

        if buffer == 0 { return; }

        if intersect_end_x <= intersect_x || intersect_end_y <= intersect_y {
            return;
        }

        let copy_width = (intersect_end_x - intersect_x) as usize;
        let copy_height = (intersect_end_y - intersect_y) as usize;

        self.mark_dirty(intersect_x, intersect_y, copy_width as u32, copy_height as u32);

        let src_off_x = (intersect_x - win_x) as usize;
        let src_off_y = (intersect_y - win_y) as usize;

        unsafe {
            let src_base = buffer as *const u32;
            let dst_base = self.framebuffer as *mut u32;
            let dst_pitch = self.pitch as usize / 4;
            let src_pitch = width as usize;

            for row in 0..copy_height {
                let src_row_ptr = src_base.add((src_off_y + row) * src_pitch + src_off_x);
                let dst_row_ptr = dst_base.add((intersect_y as usize + row) * dst_pitch + (intersect_x as usize));

                let is_top_or_bottom = (src_off_y + row) == 0 || (src_off_y + row) == (height as usize - 1);

                if !treat_as_transparent && !is_top_or_bottom && border_color.is_none() {
                    let mut start_col = 0;
                    let mut end_col = copy_width;


                    if src_off_x == 0 && copy_width > 0 {
                        if let Some(color) = border_color {
                            *dst_row_ptr.add(0) = color;
                        } else {
                            *dst_row_ptr.add(0) = *src_row_ptr.add(0);
                        }
                        start_col = 1;
                    }


                    if (src_off_x + copy_width) == (width as usize) && copy_width > start_col {
                        if let Some(color) = border_color {
                            *dst_row_ptr.add(copy_width - 1) = color;
                        } else {
                            *dst_row_ptr.add(copy_width - 1) = *src_row_ptr.add(copy_width - 1);
                        }
                        end_col = copy_width - 1;
                    }


                    if end_col > start_col {
                        core::ptr::copy_nonoverlapping(
                            src_row_ptr.add(start_col),
                            dst_row_ptr.add(start_col),
                            end_col - start_col,
                        );
                    }
                    continue;
                }

                for col in 0..copy_width {
                    let in_window_x = src_off_x + col;
                    let in_window_y = src_off_y + row;
                    let is_border = in_window_x == 0 || in_window_x == (width as usize - 1) ||
                        in_window_y == 0 || in_window_y == (height as usize - 1);

                    if is_border {
                        if let Some(color) = border_color {
                            *dst_row_ptr.add(col) = color;
                            continue;
                        }
                    }

                    let src_pixel = *src_row_ptr.add(col);
                    let alpha = if treat_as_transparent { (src_pixel >> 24) & 0xFF } else { 255 };

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

    pub fn copy_to_fb_a(&mut self, width: u32, height: u32, buffer: usize, x: i32, y: i32, border_color: Option<u32>, treat_as_transparent: bool) {
        let screen_w = self.width as i32;
        let screen_h = self.height as i32;

        let dst_x = x.max(0);
        let dst_y = y.max(0);
        let end_x = (x + width as i32).min(screen_w);
        let end_y = (y + height as i32).min(screen_h);

        if buffer == 0 { return; }

        if end_x <= dst_x || end_y <= dst_y { return; }

        let copy_width = (end_x - dst_x) as usize;
        let copy_height = (end_y - dst_y) as usize;

        self.mark_dirty(dst_x, dst_y, copy_width as u32, copy_height as u32);

        let src_off_x = (dst_x - x) as usize;
        let src_off_y = (dst_y - y) as usize;

        unsafe {
            let src_base = buffer as *const u32;
            let dst_base = self.framebuffer as *mut u32;
            let dst_pitch = self.pitch as usize / 4;
            let src_pitch = width as usize;

            for row in 0..copy_height {
                let src_row_ptr = src_base.add((src_off_y + row) * src_pitch + src_off_x);
                let dst_row_ptr = dst_base.add((dst_y as usize + row) * dst_pitch + (dst_x as usize));

                let is_top_or_bottom = (src_off_y + row) == 0 || (src_off_y + row) == (height as usize - 1);

                if !treat_as_transparent && !is_top_or_bottom && border_color.is_none() {
                    core::ptr::copy_nonoverlapping(src_row_ptr, dst_row_ptr, copy_width);
                    continue;
                }

                for col in 0..copy_width {
                    let in_window_x = src_off_x + col;
                    let in_window_y = src_off_y + row;
                    let is_border = in_window_x == 0 || in_window_x == (width as usize - 1) ||
                        in_window_y == 0 || in_window_y == (height as usize - 1);

                    if is_border {
                        if let Some(color) = border_color {
                            *dst_row_ptr.add(col) = color;
                            continue;
                        }
                    }

                    let src_pixel = *src_row_ptr.add(col);
                    let alpha = if treat_as_transparent { (src_pixel >> 24) & 0xFF } else { 255 };

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

    pub fn write_pixel_db(&self, row: u32, col: u32, color: Color) {
        if col < self.width as u32 && row < self.height as u32 {
            unsafe {
                let offset = (row as u64 * self.pitch + col as u64 * 4) as usize;
                *((self.double_buffer as *mut u8).add(offset) as *mut u32) = color.to_u32();
            }
        }
    }

    pub fn present_rect(&mut self, x: i32, y: i32, w: u32, h: u32) {
        let sx = x.max(0) as u32;
        let sy = y.max(0) as u32;
        let ex = (x + w as i32).min(self.width as i32).max(0) as u32;
        let ey = (y + h as i32).min(self.height as i32).max(0) as u32;
        
        let sw = ex.saturating_sub(sx);
        let sh = ey.saturating_sub(sy);

        if sw == 0 || sh == 0 { return; }

        self.mark_dirty(x, y, w, h);

        unsafe {
            if VIRTIO_ACTIVE.load(Ordering::SeqCst) {
                let bpp = 4;
                let pitch = self.pitch as usize;
                let src = self.double_buffer as *const u8;
                let dst = self.framebuffer as *mut u8;
                let fb_len = (self.pitch * self.height) as usize;

                if self.vbe_framebuffer != 0 {
                    core::ptr::copy_nonoverlapping(src, self.vbe_framebuffer as *mut u8, fb_len);
                }

                if sx == 0 && sw == self.width as u32 {
                    let offset = sy as usize * pitch;
                    let size = sh as usize * pitch;
                    if offset + size <= fb_len {
                        core::ptr::copy_nonoverlapping(src.add(offset), dst.add(offset), size);
                    }
                } else {
                    for row in 0..sh {
                        let offset = (sy + row) as usize * pitch + sx as usize * bpp;
                        let end_offset = offset + (sw * bpp as u32) as usize;

                        if end_offset <= fb_len {
                            core::ptr::copy_nonoverlapping(src.add(offset), dst.add(offset), (sw * bpp as u32) as usize);
                        }
                    }
                }

                virtio::flush(sx, sy, sw, sh, self.width as u32, self.active_resource_id, false);

                // Also copy to VBE framebuffer so QEMU's VGA scanout shows the content
                if self.vbe_framebuffer != 0 {
                    let bpp = 4;
                    let pitch = self.pitch as usize;
                    let src = self.double_buffer as *const u8;
                    let vbe = self.vbe_framebuffer as *mut u8;

                    if sx == 0 && sw == self.width as u32 {
                        let offset = sy as usize * pitch;
                        let size = sh as usize * pitch;
                        if offset + size <= fb_len {
                            core::ptr::copy_nonoverlapping(src.add(offset), vbe.add(offset), size);
                        }
                    } else {
                        for row in 0..sh {
                            let offset = (sy + row) as usize * pitch + sx as usize * bpp;
                            let end_offset = offset + (sw * bpp as u32) as usize;
                            if end_offset <= fb_len {
                                core::ptr::copy_nonoverlapping(src.add(offset), vbe.add(offset), (sw * bpp as u32) as usize);
                            }
                        }
                    }
                }
            } else {
                self.copy_to_fb(x, y, w, h);

                let (mx, my) = {
                    let mouse = crate::window_manager::input::MOUSE.lock();
                    (mouse.x, mouse.y)
                };
                use crate::drivers::peripherals::mouse::{CURSOR_HEIGHT, CURSOR_WIDTH};
                let mw = CURSOR_WIDTH as u32;
                let mh = CURSOR_HEIGHT as u32;
                let overlap_x = (mx as u32) < (sx + sw) && (mx as u32 + mw) > sx;
                let overlap_y = (my as u32) < (sy + sh) && (my as u32 + mh) > sy;
                if overlap_x && overlap_y {
                    self.draw_mouse(mx as u16, my as u16, false);
                }
            }
        }
    }

    /// Copy a rect from the active framebuffer to the VBE framebuffer.
    /// Call this after any direct framebuffer writes + virtio::flush() so
    /// QEMU's VGA scanout picks up the changes.
    pub fn sync_vbe_rect(&self, x: u32, y: u32, w: u32, h: u32) {
        if self.vbe_framebuffer == 0 || w == 0 || h == 0 { return; }
        unsafe {
            let pitch = self.pitch as usize;
            let src = self.double_buffer as *const u8;
            let dst = self.vbe_framebuffer as *mut u8;
            let fb_len = pitch * self.height as usize;
            for row in 0..h {
                let offset = (y + row) as usize * pitch + x as usize * 4;
                let size = w as usize * 4;
                if offset + size <= fb_len {
                    core::ptr::copy_nonoverlapping(src.add(offset), dst.add(offset), size);
                }
            }
        }
    }

    pub fn draw_mouse(&self, x: u16, y: u16, _dragging_window: bool) {
        use crate::drivers::peripherals::mouse::{CURSOR_BUFFER, CURSOR_HEIGHT, CURSOR_WIDTH};

        let pitch_bytes = self.pitch as usize;
        let fb_ptr = self.framebuffer as *mut u32;
        let db_ptr = self.double_buffer as *const u32;
        let width = self.width as usize;
        let height = self.height as usize;
        let mx = x as usize;
        let my = y as usize;

        // Always read from RAM double_buffer to avoid catastrophic MMIO read stalls
        let bg_src = db_ptr;

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
                    } else {
                        // ALWAYS restore from db_ptr so mouse erasing works perfectly
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

    pub fn to_24(&self) -> [u8; 3] {
        [self.b, self.g, self.r]
    }

    pub fn from_u16(rgb: u16) -> Self {
        let r5 = ((rgb >> 11) & 0x1F) as u8;
        let g6 = ((rgb >> 5) & 0x3F) as u8;
        let b5 = (rgb & 0x1F) as u8;
        let r = (r5 << 3) | (r5 >> 2);
        let g = (g6 << 2) | (g6 >> 4);
        let b = (b5 << 3) | (b5 >> 2);
        Color { r, g, b, a: 0xFF }
    }

    pub fn from_u32(rgba: u32) -> Self {
        let r = ((rgba >> 24) & 0xFF) as u8;
        let g = ((rgba >> 16) & 0xFF) as u8;
        let b = ((rgba >> 8) & 0xFF) as u8;
        let a = (rgba & 0xFF) as u8;

        Color { r, g, b, a }
    }

    pub fn from_u24(rgb24: u32) -> Self {
        let r = ((rgb24 >> 16) & 0xFF) as u8;
        let g = ((rgb24 >> 8) & 0xFF) as u8;
        let b = (rgb24 & 0xFF) as u8;
        Color { r, g, b, a: 0xFF }
    }
}
