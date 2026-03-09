use super::composer::COMPOSER;
use super::events::{Event, ResizeEvent, GLOBAL_EVENT_QUEUE};
use super::window::Items;
use crate::debugln;
use crate::drivers::video::virtio;
use crate::window_manager::display::{Color, State, DISPLAY_SERVER, HARDWARE_CURSOR_ACTIVE, VIRTIO_ACTIVE};
use core::sync::atomic::{AtomicU16, Ordering};

pub static mut MOUSE: Mouse = Mouse {
    x: 0,
    y: 0,
    left: false,
    center: false,
    right: false,
    state: State::Point,
};

pub struct Mouse {
    pub x: u16,
    pub y: u16,
    pub left: bool,
    pub center: bool,
    pub right: bool,
    pub state: State,
}


pub static mut LAST_INPUT: u8 = 0;
pub static mut DRAGS: u8 = 0;
pub static mut DRAG: bool = false;
pub static DRAGGING_WINDOW: AtomicU16 = AtomicU16::new(0);
pub static RESIZING_WINDOW: AtomicU16 = AtomicU16::new(0);
pub static mut CLICK_STARTED_IN_TITLEBAR: bool = false;
pub static mut CLICKED_WINDOW_ID: usize = 0;
pub static mut W_WIDTH: usize = 0;
pub static mut W_HEIGHT: usize = 0;
pub static mut MOUSE_PENDING: bool = false;

pub fn handle_mouse_update() {
    unsafe {
        use crate::drivers::periferics::mouse::MOUSE_PACKET;
        (*(&raw mut MOUSE)).cursor(MOUSE_PACKET);
    }
}

impl Mouse {
    pub fn cursor(&mut self, data: [u8; 4]) {
        let old_x = self.x;
        let old_y = self.y;

        // Check for overflow bits (6 and 7 of first byte)
        if (data[0] & 0x40) != 0 || (data[0] & 0x80) != 0 {
            return;
        }

        let mut x_rel = data[1] as i16;
        let mut y_rel = data[2] as i16;

        if (data[0] & 0x10) != 0 {
            x_rel |= 0xFF00u16 as i16;
        }

        if (data[0] & 0x20) != 0 {
            y_rel |= 0xFF00u16 as i16;
        }

        self.x = self.clamp_mx(x_rel);
        self.y = self.clamp_my(-y_rel);

        unsafe {
            if VIRTIO_ACTIVE && HARDWARE_CURSOR_ACTIVE {
                virtio::cursor::move_cursor(self.x as u32, self.y as u32);
            }
        }

        let prev_left = self.left;

        self.left = (data[0] & 0b00000001) != 0;
        self.right = (data[0] & 0b00000010) != 0;
        self.center = (data[0] & 0b00000100) != 0;

        // Check for resize termination via Left Click
        let resizing_id = RESIZING_WINDOW.load(Ordering::Relaxed);
        if resizing_id != 0 && self.left && !prev_left {
            let final_w = unsafe { W_WIDTH };
            let final_h = unsafe { W_HEIGHT };

            unsafe {
                let composer = &mut *(&raw mut COMPOSER);
                if let Some(w) = composer.find_window_id(resizing_id as usize) {
                    let event = Event::Resize(ResizeEvent {
                        wid: resizing_id as u32,
                        width: final_w as u32,
                        height: final_h as u32,
                    });

                    let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
                    if !GLOBAL_EVENT_QUEUE.int_lock().push_to_process(&*tm, w.pid, event) {
                        GLOBAL_EVENT_QUEUE.int_lock().add_event(event);
                    }
                }
            }

            RESIZING_WINDOW.store(0, Ordering::Relaxed);
            unsafe {
                W_WIDTH = 0;
                W_HEIGHT = 0;
            }
            crate::debugln!("Resize Mode: STOPPED (Mouse Click)");
            return;
        }

        unsafe {
            LAST_INPUT = data[0];
        }

        // Scroll value is a 4-bit signed value in Explorer IntelliMouse mode (ID 4)
        // Bit 3 is the sign bit.
        let mut scroll_val = (data[3] & 0x0F) as i8;
        if (scroll_val & 0x08) != 0 {
            scroll_val |= !0x0F; // Sign extend to 8-bit
        }

        if scroll_val != 0 {
            // debugln!("Mouse Scroll: {}", scroll_val);
        }

        if self.left && !prev_left {
            let w = unsafe { (*(&raw mut COMPOSER)).find_window(self.x as usize, self.y as usize) };
            if let Some(ws) = w {
                let is_super = crate::drivers::periferics::keyboard::is_super_active();

                unsafe {
                    let old_id = CLICKED_WINDOW_ID;
                    let new_id = ws.id;


                    if old_id != new_id {
                        CLICKED_WINDOW_ID = new_id;
                        (*(&raw mut COMPOSER)).focus_window(new_id);
                    }
                }


                if ws.can_move && is_super {
                    unsafe {
                        CLICK_STARTED_IN_TITLEBAR = true;
                    }
                } else {
                    unsafe { CLICK_STARTED_IN_TITLEBAR = false; }
                }
            } else {
                unsafe { CLICK_STARTED_IN_TITLEBAR = false; }
            }
        } else if !self.left {
            unsafe { CLICK_STARTED_IN_TITLEBAR = false; }
        }

        unsafe {
            if self.left {
                DRAGS = DRAGS.wrapping_add(1);
                if DRAGS > 2 {
                    DRAG = true;
                }
            } else {
                DRAGS = 0;
                DRAG = false;

                if DRAGGING_WINDOW.load(Ordering::Relaxed) != 0 {
                    let wid = DRAGGING_WINDOW.load(Ordering::Relaxed) as usize;
                    let composer = &mut *(&raw mut COMPOSER);
                    let display_server = &mut *(&raw mut DISPLAY_SERVER);

                    let w = composer.find_window_id(wid);
                    if w.is_none() { return; }

                    let w = w.unwrap();
                    let win_x = w.x;
                    let win_y = w.y;
                    let win_width = w.width;
                    let win_height = w.height;

                    composer.copy_window(wid);

                    display_server.copy_to_fb(old_x as i32, old_y as i32, 32, 32);

                    display_server.copy_to_fb(win_x as i32, win_y as i32, win_width as u32, win_height as u32);

                    if !HARDWARE_CURSOR_ACTIVE {
                        display_server.draw_mouse(self.x, self.y, false);
                    }

                    DRAGGING_WINDOW.store(0, Ordering::Relaxed);
                    RESIZING_WINDOW.store(0, Ordering::Relaxed);
                    W_WIDTH = 0;
                    W_HEIGHT = 0;

                    return;
                }
            }
        }

        unsafe {
            if DRAG && CLICK_STARTED_IN_TITLEBAR {
                if DRAGGING_WINDOW.load(Ordering::Relaxed) == 0 {
                    let wid = CLICKED_WINDOW_ID;
                    DRAGGING_WINDOW.store(wid as u16, Ordering::Relaxed);
                    (*(&raw mut COMPOSER)).recompose_except(wid);
                }
            }
        }

        let _w = unsafe { (*(&raw mut COMPOSER)).find_window(self.x as usize, self.y as usize) };

        if RESIZING_WINDOW.load(Ordering::Relaxed) != 0 {
            let (w_id, w_x, w_y, min_w, min_h) = unsafe {
                let composer = &mut *(&raw mut COMPOSER);
                let w = composer.find_window_id(RESIZING_WINDOW.load(Ordering::Relaxed) as usize).unwrap();
                (w.id, w.x, w.y, w.min_width.max(50), w.min_height.max(50))
            };

            let old_w_val = unsafe { W_WIDTH };
            let old_h_val = unsafe { W_HEIGHT };

            let cur_w: usize;
            let cur_h: usize;

            unsafe {
                // Calculate dimensions from window top-left to mouse tip
                let new_w = (self.x as isize - w_x).max(min_w as isize);
                let new_h = (self.y as isize - w_y).max(min_h as isize);
                
                // Limit to screen size relative to window position
                let max_w = ((*(&raw mut DISPLAY_SERVER)).width as isize).saturating_sub(w_x);
                let max_h = ((*(&raw mut DISPLAY_SERVER)).height as isize).saturating_sub(w_y);

                W_WIDTH = new_w.min(max_w) as usize;
                W_HEIGHT = new_h.min(max_h) as usize;
                cur_w = W_WIDTH;
                cur_h = W_HEIGHT;
            }

            // Union of old and new area + mouse margin (32px)
            let dirty_w = old_w_val.max(cur_w) + 32;
            let dirty_h = old_h_val.max(cur_h) + 32;

            unsafe {
                let composer = &mut *(&raw mut COMPOSER);
                let ds = &mut *(&raw mut DISPLAY_SERVER);

                // 1. Recompose background AND static window into Double Buffer
                composer.recompose_area(w_x as i32, w_y as i32, dirty_w as u32, dirty_h as u32);

                // 2. Draw new white wireframe border into Double Buffer
                (*(&raw mut MOUSE)).draw_resize_border(
                    w_x as u16,
                    w_y as u16,
                    cur_w as u16,
                    cur_h as u16,
                    Color::rgb(255, 255, 255), // White border
                    3 // Thickness
                );

                // 3. Flush Double Buffer to Front Buffer
                ds.present_rect(w_x as i32, w_y as i32, dirty_w as u32, dirty_h as u32);

                // 4. Draw Mouse directly to Front Buffer (on top of everything)
                if !HARDWARE_CURSOR_ACTIVE {
                    ds.draw_mouse(self.x, self.y, false);
                }

                // 5. Final flush for mouse cursor (VirtIO)
                if VIRTIO_ACTIVE {
                    let mx = self.x as u32;
                    let my = self.y as u32;
                    let sw = ds.width as u32;
                    let sh = ds.height as u32;
                    let fw = (32 as u32).min(sw.saturating_sub(mx));
                    let fh = (32 as u32).min(sh.saturating_sub(my));
                    if fw > 0 && fh > 0 {
                        virtio::flush(mx, my, fw, fh, sw, ds.active_resource_id);
                    }
                }
            }
            return;
        } else if DRAGGING_WINDOW.load(Ordering::Relaxed) != 0 {
            let composer = unsafe { &mut *(&raw mut COMPOSER) };
            let display_server = unsafe { &mut *(&raw mut DISPLAY_SERVER) };
            let wid = DRAGGING_WINDOW.load(Ordering::Relaxed) as usize;

            let window_opt = composer.find_window_id(wid);
            let w = match window_opt {
                Some(w) => w,
                None => return,
            };

            let old_win_x = w.x;
            let old_win_y = w.y;
            let width = w.width;
            let height = w.height;
            let buffer = w.get_active_buffer();

            let mouse_dx = self.x as i32 - old_x as i32;
            let mouse_dy = self.y as i32 - old_y as i32;

            let target_win_x = old_win_x as i32 + mouse_dx;
            let target_win_y = old_win_y as i32 + mouse_dy;

            let screen_w = display_server.width as i32;
            let screen_h = display_server.height as i32;

            let margin = 3;

            let min_visible_x = -(width as i32) + margin;
            let max_visible_x = screen_w - margin;
            let min_visible_y = -(height as i32) + margin;
            let max_visible_y = screen_h - margin;

            let clamped_win_x = target_win_x.max(min_visible_x).min(max_visible_x);
            let clamped_win_y = target_win_y.max(min_visible_y).min(max_visible_y);

            let new_x = clamped_win_x as isize;
            let new_y = clamped_win_y as isize;

            w.x = new_x;
            w.y = new_y;

            display_server.copy_to_fb(old_win_x as i32, old_win_y as i32, width as u32, height as u32);


            display_server.copy_to_fb_a(width as u32, height as u32, buffer, new_x as i32, new_y as i32, Some(0xFFFFFFFF), w.treat_as_transparent);


            for i in 0..composer.windows.len() {
                let w = &composer.windows[i];
                match w.w_type {
                    Items::Bar | Items::Popup => {
                        display_server.copy_to_fb_clipped(
                            w.width as u32,
                            w.height as u32,
                            w.get_active_buffer(),
                            w.x as i32,
                            w.y as i32,
                            new_x as i32, new_y as i32, width as u32, height as u32,
                            None,
                            w.treat_as_transparent,
                        );
                    }
                    _ => {}
                }
            }

            let old_x_clamped = (old_win_x as i32).max(0) as u32;
            let old_y_clamped = (old_win_y as i32).max(0) as u32;
            let new_x_clamped = (new_x as i32).max(0) as u32;
            let new_y_clamped = (new_y as i32).max(0) as u32;
            let mouse_x = self.x as u32;
            let mouse_y = self.y as u32;

            let screen_w_u32 = screen_w as u32;
            let screen_h_u32 = screen_h as u32;

            let old_x_end = (old_win_x as i32 + width as i32).max(0).min(screen_w).max(0) as u32;
            let old_y_end = (old_win_y as i32 + height as i32).max(0).min(screen_h).max(0) as u32;
            let new_x_end = (new_x as i32 + width as i32).max(0).min(screen_w).max(0) as u32;
            let new_y_end = (new_y as i32 + height as i32).max(0).min(screen_h).max(0) as u32;
            let mouse_x_end = (mouse_x + 32).min(screen_w_u32);
            let mouse_y_end = (mouse_y + 32).min(screen_h_u32);

            let min_x = old_x_clamped.min(new_x_clamped).min(mouse_x);
            let min_y = old_y_clamped.min(new_y_clamped).min(mouse_y);
            let max_x = old_x_end.max(new_x_end).max(mouse_x_end);
            let max_y = old_y_end.max(new_y_end).max(mouse_y_end);

            let flush_x = min_x;
            let flush_y = min_y;
            let flush_w = max_x.saturating_sub(min_x);
            let flush_h = max_y.saturating_sub(min_y);

            unsafe {
                if !HARDWARE_CURSOR_ACTIVE {
                    display_server.draw_mouse(self.x, self.y, true);
                }
            }

            unsafe {
                if VIRTIO_ACTIVE && flush_w > 0 && flush_h > 0 {
                    virtio::flush(flush_x, flush_y, flush_w, flush_h, display_server.width as u32, display_server.active_resource_id);
                }
            }
            return;
        }

        unsafe {
            let display_server = &mut *(&raw mut DISPLAY_SERVER);
            if !HARDWARE_CURSOR_ACTIVE {
                display_server.copy_to_fb(old_x as i32, old_y as i32, 32, 32);
                display_server.draw_mouse(self.x, self.y, false);
            }

            if VIRTIO_ACTIVE {
                let u_old_x = old_x as u32;
                let u_old_y = old_y as u32;
                let u_new_x = self.x as u32;
                let u_new_y = self.y as u32;

                let min_x = u_old_x.min(u_new_x);
                let min_y = u_old_y.min(u_new_y);
                let max_x = (u_old_x + 32).max(u_new_x + 32);
                let max_y = (u_old_y + 32).max(u_new_y + 32);

                let screen_w = display_server.width as u32;
                let screen_h = display_server.height as u32;

                let flush_x = min_x.min(screen_w);
                let flush_y = min_y.min(screen_h);
                let flush_w = (max_x.min(screen_w)).saturating_sub(flush_x);
                let flush_h = (max_y.min(screen_h)).saturating_sub(flush_y);

                if !HARDWARE_CURSOR_ACTIVE && flush_w > 0 && flush_h > 0 {
                    virtio::flush(flush_x, flush_y, flush_w, flush_h, screen_w, display_server.active_resource_id);
                }
            }

            if self.left {
                crate::debugln!("Input: Click at {},{}", self.x, self.y);
            }

            if let Some(w) = (*(&raw mut COMPOSER)).find_window(self.x as usize, self.y as usize) {
                if self.left {
                    crate::debugln!("Input: Found window ID {} at {},{}", w.id, w.x, w.y);
                }

                if w.event_handler != 0 {
                    let local_x = (self.x as isize - w.x).max(0) as usize;
                    let local_y = (self.y as isize - w.y).max(0) as usize;

                    use crate::window_manager::events::{Event, MouseEvent, GLOBAL_EVENT_QUEUE};

                    static mut LAST_X: usize = 9999;
                    static mut LAST_Y: usize = 9999;
                    static mut LAST_BTNS: [bool; 3] = [false; 3];

                    let btns = [self.left, self.right, self.center];
                    if local_x != unsafe { LAST_X } || local_y != unsafe { LAST_Y } || btns != unsafe { LAST_BTNS } || scroll_val != 0 {
                        unsafe {
                            LAST_X = local_x;
                            LAST_Y = local_y;
                            LAST_BTNS = btns;
                        }

                        let event = Event::Mouse(MouseEvent {
                            wid: w.id as u32,
                            x: local_x as u32,
                            y: local_y as u32,
                            buttons: btns,
                            scroll: scroll_val,
                        });

                        {
                            let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
                            if !GLOBAL_EVENT_QUEUE.int_lock().push_to_process(&*tm, w.pid, event) {
                                GLOBAL_EVENT_QUEUE.int_lock().add_event(event);
                            }
                        }

                        if self.left {
                            crate::debugln!("Input: Dispatching Mouse Event to {}", w.id);
                        }
                    }
                }
            }
        };
    }

    fn is_bottom_right(
        &self,
        w_x: u16,
        w_y: u16,
        w_width: u16,
        w_height: u16,
        mouse_x: u16,
        mouse_y: u16,
    ) -> bool {
        let x_min = w_x.wrapping_add(w_width.wrapping_sub(8));
        let x_max = w_x.wrapping_add(w_width.wrapping_sub(0));
        let y_min = w_y.wrapping_add(w_height.wrapping_sub(8));
        let y_max = w_y.wrapping_add(w_height.wrapping_sub(0));

        (mouse_x >= x_min && mouse_x <= x_max) && (mouse_y >= y_min && mouse_y <= y_max)
    }

    pub fn draw_resize_border(&self, x: u16, y: u16, width: u16, height: u16, color: Color, thickness: u16) {
        let start_x = x as u32;
        let start_y = y as u32;
        let end_x = start_x + width as u32;
        let end_y = start_y + height as u32;
        let t = thickness as u32;

        unsafe {
            // Top
            for row in start_y..start_y + t {
                for col in start_x..end_x {
                    (*(&raw mut DISPLAY_SERVER)).write_pixel_db(row, col, color);
                }
            }
            // Bottom
            for row in end_y.saturating_sub(t)..end_y {
                for col in start_x..end_x {
                    (*(&raw mut DISPLAY_SERVER)).write_pixel_db(row, col, color);
                }
            }
            // Left
            for row in start_y..end_y {
                for col in start_x..start_x + t {
                    (*(&raw mut DISPLAY_SERVER)).write_pixel_db(row, col, color);
                }
            }
            // Right
            for row in start_y..end_y {
                for col in end_x.saturating_sub(t)..end_x {
                    (*(&raw mut DISPLAY_SERVER)).write_pixel_db(row, col, color);
                }
            }
        }
    }

    fn clamp_mx(&self, n: i16) -> u16 {
        let sx = unsafe { (*(&raw mut DISPLAY_SERVER)).width } as i32;
        if sx <= 0 { return 0; }

        let limit = if DRAGGING_WINDOW.load(Ordering::Relaxed) != 0 {
            sx + 50
        } else {
            sx - 3
        };

        let next_x = (self.x as i32) + (n as i32);
        if next_x < 0 {
            0
        } else if next_x >= limit {
            limit.saturating_sub(1) as u16
        } else {
            next_x as u16
        }
    }

    fn clamp_my(&self, n: i16) -> u16 {
        let sy = unsafe { (*(&raw mut DISPLAY_SERVER)).height } as i32;
        if sy <= 0 { return 0; }

        let limit = if DRAGGING_WINDOW.load(Ordering::Relaxed) != 0 {
            sy + 50
        } else {
            sy - 3
        };

        let next_y = (self.y as i32) + (n as i32);
        if next_y < 0 {
            0
        } else if next_y >= limit {
            limit.saturating_sub(1) as u16
        } else {
            next_y as u16
        }
    }
}

fn cap(n: usize, value: usize) -> usize {
    if n > value { value } else { n }
}