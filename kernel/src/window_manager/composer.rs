use super::window::{Window, Items, NULL_WINDOW};
use crate::window_manager::display::DISPLAY_SERVER;
use crate::window_manager::events::{Event, ResizeEvent, GLOBAL_EVENT_QUEUE};
use crate::window_manager::input::CLICKED_WINDOW_ID;
use crate::debugln;

#[derive(Debug, Clone)]
pub struct Composer {
    pub windows: [Window; 16],
}

pub static mut COMPOSER: Composer = Composer {
    windows: [NULL_WINDOW; 16],
};

impl Composer {
    pub fn copy_window(&mut self, id: usize) {
        for i in 0..self.windows.len() {
            if id == self.windows[i].id {
                let border_color = if self.windows[i].w_type == Items::Window {
                    unsafe {
                        if self.windows[i].id == CLICKED_WINDOW_ID {
                            Some(0xFFFFFFFF)
                        } else {
                            Some(0xFF9070FF)
                        }
                    }
                } else {
                    None
                };

                match self.windows[i].w_type {
                    Items::Null => {}
                    _ => unsafe {
                        (*(&raw mut DISPLAY_SERVER)).copy_to_db(
                            self.windows[i].width as u32,
                            self.windows[i].height as u32,
                            self.windows[i].buffer,
                            self.windows[i].x as i32,
                            self.windows[i].y as i32,
                            border_color,
                            self.windows[i].treat_as_transparent,
                        )
                    },
                }
            }
        }
    }

    pub fn copy_window_fb(&mut self, id: usize) {
        for i in 0..self.windows.len() {
            if id == self.windows[i].id {
                let border_color = if self.windows[i].w_type == Items::Window {
                    unsafe {
                        if self.windows[i].id == CLICKED_WINDOW_ID {
                            Some(0xFFFFFFFF)
                        } else {
                            Some(0xFF9070FF)
                        }
                    }
                } else {
                    None
                };

                match self.windows[i].w_type {
                    Items::Null => {}
                    _ => unsafe {
                        (*(&raw mut DISPLAY_SERVER)).copy_to_fb_a(
                            self.windows[i].width as u32,
                            self.windows[i].height as u32,
                            self.windows[i].buffer,
                            self.windows[i].x as i32,
                            self.windows[i].y as i32,
                            border_color,
                            self.windows[i].treat_as_transparent,
                        )
                    },
                }
            }
        }
    }

    pub fn find_window(&mut self, x: usize, y: usize) -> Option<&mut Window> {
        let mx = x as isize;
        let my = y as isize;

        for i in 0..self.windows.len() {
            if mx >= self.windows[i].x
                && mx <= (self.windows[i].x + self.windows[i].width as isize)
                && my >= self.windows[i].y
                && my <= (self.windows[i].y + self.windows[i].height as isize)
            {
                match self.windows[i].w_type {
                    Items::Null => {}
                    _ => return Some(&mut self.windows[i]),
                }
            }
        }
        None
    }

    pub fn find_window_id(&mut self, id: usize) -> Option<&mut Window> {
        for i in 0..self.windows.len() {
            if self.windows[i].id == id {
                let h = self.windows[i].w_type;
                if h != Items::Null {
                    return Some(&mut self.windows[i]);
                }
            }
        }
        None
    }

    pub fn focus_window(&mut self, id: usize) {
        let mut target_idx = None;
        for i in 0..self.windows.len() {
            if self.windows[i].id == id && self.windows[i].w_type != Items::Null {
                target_idx = Some(i);
                break;
            }
        }

        if let Some(idx) = target_idx {
            let wtype = self.windows[idx].w_type;
            
            if wtype == Items::Bar || wtype == Items::Popup || wtype == Items::Wallpaper {
                return;
            }

            
            self.windows[idx].z = 1;

            for i in 0..self.windows.len() {
                if i == idx { continue; }
                match self.windows[i].w_type {
                    Items::Bar | Items::Popup | Items::Null | Items::Wallpaper => {}
                    _ => {
                        
                        self.windows[i].z = self.windows[i].z.saturating_add(1);
                    }
                }
            }

            self.windows.sort_by_key(|w| w.z);
            
            
            let (sw, sh) = unsafe {
                ((*(&raw mut DISPLAY_SERVER)).width as u32, (*(&raw mut DISPLAY_SERVER)).height as u32)
            };
            self.update_window_area_rect(0, 0, sw, sh);
        }
    }

    pub fn update_tiling(&mut self) {
        
        

        let (screen_w, screen_h) = unsafe {
            ((*(&raw mut DISPLAY_SERVER)).width as usize, (*(&raw mut DISPLAY_SERVER)).height as usize)
        };

        for i in 0..self.windows.len() {
            if self.windows[i].w_type == Items::Window {
                self.windows[i].can_move = true;
                self.windows[i].can_resize = true;
            }
        }

        
        self.update_window_area_rect(0, 0, screen_w as u32, screen_h as u32);
    }

    pub fn check_id(&self, _rng_seed: u64) -> usize {
        static mut NEXT_ID: usize = 1;
        unsafe {
            let id = NEXT_ID;
            NEXT_ID += 1;
            id
        }
    }

    pub fn add_window(&mut self, mut w: Window) -> usize {
        let wtype = w.w_type;

        if wtype == Items::Wallpaper {
            w.z = 255;
            w.transparent = false;
            w.treat_as_transparent = false;
        } else if wtype == Items::Bar || wtype == Items::Popup {
            w.z = 0;
        } else {
            w.z = 1;
        }

        w.id = self.check_id(w.buffer as u64);

        
        if wtype == Items::Window {
            let mut count = 0;
            for i in 0..self.windows.len() {
                if self.windows[i].w_type == Items::Window {
                    count += 1;
                }
            }
            
            let offset = 30;
            let start_x = 50;
            let start_y = 50;

            w.x = (start_x + (count * offset)) as isize;
            w.y = (start_y + (count * offset)) as isize;

            
            w.can_move = true;
            w.can_resize = true;
        }

        for i in 0..self.windows.len() {
            match self.windows[i].w_type {
                Items::Null => {
                    self.windows[i] = w;
                    break;
                }
                _ => {}
            }
        }

        for i in 0..self.windows.len() {
            if self.windows[i].id == w.id { continue; }

            match self.windows[i].w_type {
                Items::Bar | Items::Popup => {
                    self.windows[i].z = 0;
                },
                Items::Null => {},
                _ => {
                    if wtype == Items::Bar || wtype == Items::Popup {
                        if self.windows[i].z == 0 { self.windows[i].z = 1; }
                    } else {
                        self.windows[i].z = self.windows[i].z.saturating_add(1);
                    }
                }
            }
        }

        self.windows.sort_by_key(|w| w.z);
        debugln!("add_window: sorted, updating tiling...");
        self.update_tiling();
        debugln!("add_window: tiling updated, returning ID.");
        w.id
    }

    pub fn resize_window(&mut self, w: Window) {
        for i in 0..self.windows.len() {
            if w.id == self.windows[i].id {
                
                self.windows[i].buffer = w.buffer;

                
                let old_x = self.windows[i].x;
                let old_y = self.windows[i].y;
                let old_w = self.windows[i].width;
                let old_h = self.windows[i].height;

                
                self.windows[i].width = w.width;
                self.windows[i].height = w.height;

                
                if self.windows[i].w_type != Items::Window {
                    self.windows[i].x = w.x;
                    self.windows[i].y = w.y;
                    self.windows[i].can_move = w.can_move;
                }

                self.windows[i].transparent = w.transparent;
                self.windows[i].treat_as_transparent = w.treat_as_transparent;

                
                let current_x = self.windows[i].x;
                let current_y = self.windows[i].y;

                let min_x = old_x.min(current_x);
                let min_y = old_y.min(current_y);
                let max_x = (old_x + old_w as isize).max(current_x + w.width as isize);
                let max_y = (old_y + old_h as isize).max(current_y + w.height as isize);

                let dirty_w = (max_x - min_x).max(0) as u32;
                let dirty_h = (max_y - min_y).max(0) as u32;

                if dirty_w > 0 && dirty_h > 0 {
                    self.update_window_area_rect(min_x as i32, min_y as i32, dirty_w, dirty_h);
                }

                if self.windows[i].w_type == Items::Window {
                    
                    self.update_tiling();
                }

                break;
            }
        }
    }

    pub fn update_window_area_rect(&mut self, dirty_x: i32, dirty_y: i32, dirty_w: u32, dirty_h: u32) {
        unsafe {
            let display_server = &mut *(&raw mut DISPLAY_SERVER);

            if display_server.double_buffer != 0 {
                let db_ptr = display_server.double_buffer as *mut u32;
                let pitch_u32 = (display_server.pitch / 4) as usize;
                let height = display_server.height as i32;
                let width = display_server.width as i32;

                let start_x = dirty_x.max(0);
                let start_y = dirty_y.max(0);
                let end_x = (dirty_x + dirty_w as i32).min(width);
                let end_y = (dirty_y + dirty_h as i32).min(height);

                if end_x > start_x && end_y > start_y {
                    
                }
            }

            // Occlusion Culling: Find the front-most window that is opaque and covers the dirty rect.
            // We only need to draw from that window forward.
            let mut start_index = self.windows.len().saturating_sub(1);
            for i in 0..self.windows.len() {
                let w = &self.windows[i];
                if w.w_type == Items::Null { continue; }
                
                // If window covers the dirty rect and is opaque
                if !w.treat_as_transparent && 
                   w.x as i32 <= dirty_x && 
                   w.y as i32 <= dirty_y && 
                   (w.x as i32 + w.width as i32) >= (dirty_x + dirty_w as i32) &&
                   (w.y as i32 + w.height as i32) >= (dirty_y + dirty_h as i32) {
                    start_index = i;
                    break;
                }
            }

            for i in (0..=start_index).rev() {
                match self.windows[i].w_type {
                    Items::Null => {}
                    _ => {
                        let w = &self.windows[i];
                        let border_color = if w.w_type == Items::Window {
                            if w.id == CLICKED_WINDOW_ID {
                                Some(0xFFFFFFFF)
                            } else {
                                Some(0xFF9070FF)
                            }
                        } else {
                            None
                        };

                        display_server.copy_to_db_clipped(
                            w.width as u32,
                            w.height as u32,
                            w.buffer,
                            w.x as i32,
                            w.y as i32,
                            dirty_x, dirty_y, dirty_w, dirty_h,
                            border_color,
                            w.treat_as_transparent,
                        );
                    }
                }
            }

            display_server.present_rect(dirty_x, dirty_y, dirty_w, dirty_h);
        }
    }

    pub fn recompose_except(&mut self, except_id: usize) {
        unsafe {
            let display_server = &mut *(&raw mut DISPLAY_SERVER);
            if display_server.double_buffer != 0 {
                core::ptr::write_bytes(
                    display_server.double_buffer as *mut u8,
                    0,
                    (display_server.pitch * display_server.height) as usize,
                );
            }

            for i in (0..self.windows.len()).rev() {
                if self.windows[i].id != except_id {
                    match self.windows[i].w_type {
                        Items::Null => {}
                        _ => {
                            let border_color = if self.windows[i].w_type == Items::Window {
                                if self.windows[i].id == CLICKED_WINDOW_ID {
                                    Some(0xFFFFFFFF)
                                } else {
                                    Some(0xFF9070FF)
                                }
                            } else {
                                None
                            };

                            display_server.copy_to_db(
                                self.windows[i].width as u32,
                                self.windows[i].height as u32,
                                self.windows[i].buffer,
                                self.windows[i].x as i32,
                                self.windows[i].y as i32,
                                border_color,
                                self.windows[i].treat_as_transparent,
                            );
                        }
                    }
                }
            }
        }
    }

    pub fn update_window_area(&mut self, id: usize) {
        let (dirty_x, dirty_y, dirty_w, dirty_h) = {
            let mut found = None;
            for i in 0..self.windows.len() {
                if self.windows[i].id == id {
                    let w = &self.windows[i];
                    found = Some((w.x as i32, w.y as i32, w.width as u32, w.height as u32));
                    break;
                }
            }
            match found {
                Some(rect) => rect,
                None => return,
            }
        };

        unsafe {
            let display_server = &mut *(&raw mut DISPLAY_SERVER);

            if display_server.double_buffer != 0 {
                let db_ptr = display_server.double_buffer as *mut u32;
                let pitch_u32 = (display_server.pitch / 4) as usize;
                let height = display_server.height as i32;
                let width = display_server.width as i32;

                let start_x = dirty_x.max(0);
                let start_y = dirty_y.max(0);
                let end_x = (dirty_x + dirty_w as i32).min(width);
                let end_y = (dirty_y + dirty_h as i32).min(height);

                if end_x > start_x && end_y > start_y {
                    for y in start_y..end_y {
                        let row_offset = y as usize * pitch_u32;
                        let _start_ptr = db_ptr.add(row_offset + start_x as usize);
                        let _count = (end_x - start_x) as usize;
                    }
                }
            }

            for i in (0..self.windows.len()).rev() {
                match self.windows[i].w_type {
                    Items::Null => {}
                    _ => {
                        let w = &self.windows[i];
                        let border_color = if w.w_type == Items::Window {
                            if w.id == CLICKED_WINDOW_ID {
                                Some(0xFFFFFFFF)
                            } else {
                                Some(0xFF9070FF)
                            }
                        } else {
                            None
                        };

                        display_server.copy_to_db_clipped(
                            w.width as u32,
                            w.height as u32,
                            w.buffer,
                            w.x as i32,
                            w.y as i32,
                            dirty_x, dirty_y, dirty_w, dirty_h,
                            border_color,
                            w.treat_as_transparent,
                        );
                    }
                }
            }

            display_server.present_rect(dirty_x, dirty_y, dirty_w, dirty_h);
        }
    }

    pub fn remove_window(&mut self, wid: usize) {
        for i in 0..self.windows.len() {
            if self.windows[i].id == wid {
                self.windows[i].w_type = Items::Null;
                self.windows[i].z = 255;
            }
        }

        self.windows.sort_by_key(|w| w.z);

        unsafe {
            let display_server = &mut *(&raw mut DISPLAY_SERVER);
            if display_server.double_buffer != 0 {
                core::ptr::write_bytes(
                    display_server.double_buffer as *mut u8,
                    0,
                    (display_server.pitch * display_server.height) as usize,
                );
            }

            for j in (0..self.windows.len()).rev() {
                match self.windows[j].w_type {
                    Items::Null => {}
                    _ => {
                        let border_color = if self.windows[j].w_type == Items::Window {
                            if self.windows[j].id == CLICKED_WINDOW_ID {
                                Some(0xFFFFFFFF)
                            } else {
                                Some(0xFF9070FF)
                            }
                        } else {
                            None
                        };

                        (*(&raw mut DISPLAY_SERVER)).copy_to_db(
                            self.windows[j].width as u32,
                            self.windows[j].height as u32,
                            self.windows[j].buffer,
                            self.windows[j].x as i32,
                            self.windows[j].y as i32,
                            border_color,
                            self.windows[j].treat_as_transparent,
                        );
                    }
                }
            }

            (*(&raw mut DISPLAY_SERVER)).copy();
        }
        self.update_tiling();
    }

    pub fn remove_windows_by_pid(&mut self, pid: u64) {
        let mut removed = false;
        for i in 0..self.windows.len() {
            if self.windows[i].pid == pid && self.windows[i].w_type != Items::Null {
                self.windows[i].w_type = Items::Null;
                self.windows[i].z = 255;
                removed = true;
            }
        }

        if removed {
            self.windows.sort_by_key(|w| w.z);
            unsafe {
                let display_server = &mut *(&raw mut DISPLAY_SERVER);
                if display_server.double_buffer != 0 {
                    core::ptr::write_bytes(
                        display_server.double_buffer as *mut u8,
                        0,
                        (display_server.pitch * display_server.height) as usize,
                    );
                }

                for j in (0..self.windows.len()).rev() {
                    if self.windows[j].w_type != Items::Null {
                        let border_color = if self.windows[j].w_type == Items::Window {
                            if self.windows[j].id == CLICKED_WINDOW_ID {
                                Some(0xFFFFFFFF)
                            } else {
                                Some(0xFF9070FF)
                            }
                        } else {
                            None
                        };

                        display_server.copy_to_db(
                            self.windows[j].width as u32,
                            self.windows[j].height as u32,
                            self.windows[j].buffer,
                            self.windows[j].x as i32,
                            self.windows[j].y as i32,
                            border_color,
                            self.windows[j].treat_as_transparent,
                        );
                    }
                }
                display_server.copy();
            }
            self.update_tiling();
        }
    }
}