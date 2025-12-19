use super::window::{Window, Items, NULL_WINDOW};
use crate::window_manager::display::DISPLAY_SERVER;

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
                match self.windows[i].w_type {
                    Items::Null => {}
                    _ => unsafe {
                        (*(&raw mut DISPLAY_SERVER)).copy_to_db(
                            self.windows[i].width as u32,
                            self.windows[i].height as u32,
                            self.windows[i].buffer,
                            self.windows[i].x as i32,
                            self.windows[i].y as i32,
                        )
                    },
                }
            }
        }
    }

    pub fn copy_window_fb(&mut self, id: usize) {
        for i in 0..self.windows.len() {
            if id == self.windows[i].id {
                match self.windows[i].w_type {
                    Items::Null => {}
                    _ => unsafe {
                        (*(&raw mut DISPLAY_SERVER)).copy_to_fb_a(
                            self.windows[i].width as u32,
                            self.windows[i].height as u32,
                            self.windows[i].buffer,
                            self.windows[i].x as i32,
                            self.windows[i].y as i32,
                        )
                    },
                }
            }
        }
    }

    pub fn find_window(&mut self, x: usize, y: usize) -> Option<&mut Window> {
        let mx = x as isize;
        let my = y as isize;
        
        for i in (0..self.windows.len()).rev() {
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
        } else if wtype == Items::Bar {
            w.z = 0;
        } else if wtype == Items::Popup {
            w.z = 0;
        }

        w.id = self.check_id(w.buffer as u64);

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
            if self.windows[i].id != w.id {
                self.windows[i].z = self.windows[i].z.saturating_add(1);
            }
        }

        self.windows.sort_by_key(|w| w.z);
        w.id
    }

    pub fn resize_window(&mut self, w: Window) {
        for i in 0..self.windows.len() {
            if w.id == self.windows[i].id {
                self.windows[i].width = w.width;
                self.windows[i].height = w.height;
                self.windows[i].buffer = w.buffer;
                self.windows[i].can_move = w.can_move;
            }
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
                            display_server.copy_to_db(
                                self.windows[i].width as u32,
                                self.windows[i].height as u32,
                                self.windows[i].buffer,
                                self.windows[i].x as i32,
                                self.windows[i].y as i32,
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
                        let start_ptr = db_ptr.add(row_offset + start_x as usize);
                        let count = (end_x - start_x) as usize;
                        core::ptr::write_bytes(start_ptr, 0, count * 4);
                    }
                }
            }
            
            for i in (0..self.windows.len()).rev() {
                match self.windows[i].w_type {
                    Items::Null => {}
                    _ => {
                        let w = &self.windows[i];
                        display_server.copy_to_db_clipped(
                            w.width as u32,
                            w.height as u32,
                            w.buffer,
                            w.x as i32,
                            w.y as i32,
                            dirty_x, dirty_y, dirty_w, dirty_h
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
                        (*(&raw mut DISPLAY_SERVER)).copy_to_db(
                            self.windows[j].width as u32,
                            self.windows[j].height as u32,
                            self.windows[j].buffer,
                            self.windows[j].x as i32,
                            self.windows[j].y as i32,
                        );
                    }
                }
            }

            (*(&raw mut DISPLAY_SERVER)).copy();
        }
    }
}