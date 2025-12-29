use core::sync::atomic::{AtomicU16, Ordering};
use crate::debugln;
use crate::drivers::video::virtio;
use crate::window_manager::display::{DISPLAY_SERVER, VIRTIO_ACTIVE, Color, State};
use super::composer::COMPOSER;
use super::events::{Event, ResizeEvent, GLOBAL_EVENT_QUEUE};
use super::window::Items;

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
pub static mut DRAGGING_WINDOW: AtomicU16 = AtomicU16::new(0);
pub static mut RESIZING_WINDOW: AtomicU16 = AtomicU16::new(0);
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

        let prev_left = self.left;

        self.left = (data[0] & 0b00000001) != 0;
        self.right = (data[0] & 0b00000010) != 0;
        self.center = (data[0] & 0b00000100) != 0;

        unsafe {
            LAST_INPUT = data[0];
        }

        let scroll_val = data[3] as i8;

        if scroll_val != 0 {
            debugln!("Mouse Scroll: {}", scroll_val);
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

                if (*(&raw mut RESIZING_WINDOW)).load(Ordering::Relaxed) != 0 {
                    let w = (*(&raw mut COMPOSER))
                        .find_window_id((*(&raw mut RESIZING_WINDOW)).load(Ordering::Relaxed) as usize)
                        .unwrap();

                    (*(&raw mut DRAGGING_WINDOW)).store(0, Ordering::Relaxed);
                    (*(&raw mut RESIZING_WINDOW)).store(0, Ordering::Relaxed);

                    if w.event_handler != 0 {
                        GLOBAL_EVENT_QUEUE.int_lock().add_event(Event::Resize(ResizeEvent {
                            wid: w.id as u32,
                            width: W_WIDTH,
                            height: W_HEIGHT,
                        }));
                    }
                    W_WIDTH = 0;
                    W_HEIGHT = 0;

                } else if (*(&raw mut DRAGGING_WINDOW)).load(Ordering::Relaxed) != 0 {
                    let wid = (*(&raw mut DRAGGING_WINDOW)).load(Ordering::Relaxed) as usize;
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

                    display_server.draw_mouse(self.x, self.y, false);

                    (*(&raw mut DRAGGING_WINDOW)).store(0, Ordering::Relaxed);
                    (*(&raw mut RESIZING_WINDOW)).store(0, Ordering::Relaxed);
                    W_WIDTH = 0;
                    W_HEIGHT = 0;

                    return;
                }
            }
        }

        unsafe {
            if DRAG && CLICK_STARTED_IN_TITLEBAR {
                if (*(&raw mut DRAGGING_WINDOW)).load(Ordering::Relaxed) == 0 {
                    let wid = CLICKED_WINDOW_ID;
                    (*(&raw mut DRAGGING_WINDOW)).store(wid as u16, Ordering::Relaxed);
                    (*(&raw mut COMPOSER)).recompose_except(wid);
                }
            }
        }

        let _w = unsafe { (*(&raw mut COMPOSER)).find_window(self.x as usize, self.y as usize) };

        if unsafe { (*(&raw mut RESIZING_WINDOW)).load(Ordering::Relaxed) != 0 } {
            let x_vec = x_rel;
            let y_vec = y_rel;
            let dx = x_vec;
            let dy = y_vec * -1;

            let w = unsafe {
                (*(&raw mut COMPOSER))
                    .find_window_id((*(&raw mut RESIZING_WINDOW)).load(Ordering::Relaxed) as usize)
                    .unwrap()
            };

            let final_width = self.rem_sign(unsafe { W_WIDTH } as i16 + dx) as usize;
            let final_height = self.rem_sign(unsafe { W_HEIGHT } as i16 + dy) as usize;

            unsafe {
                let wx = w.x.max(0) as usize;
                let wy = w.y.max(0) as usize;
                W_WIDTH = cap(final_width, ((*(&raw mut DISPLAY_SERVER)).width as usize).saturating_sub(wx));
                W_HEIGHT = cap(final_height, ((*(&raw mut DISPLAY_SERVER)).height as usize).saturating_sub(wy));
            }

            self.draw_square_outline(
                w.y as u16,
                w.x as u16,
                unsafe { W_HEIGHT as u16 },
                unsafe { W_WIDTH as u16 },
                Color::rgb(245, 245, 247)
            );

            unsafe {
                if VIRTIO_ACTIVE {
                    virtio::flush(w.x as u32, w.y as u32, W_WIDTH as u32, W_HEIGHT as u32, (*(&raw mut DISPLAY_SERVER)).width as u32, (*(&raw mut DISPLAY_SERVER)).active_resource_id);
                }

                (*(&raw mut DISPLAY_SERVER)).draw_mouse(self.x, self.y, false);

                if VIRTIO_ACTIVE {
                    let mx = self.x as u32;
                    let my = self.y as u32;
                    let sw = (*(&raw mut DISPLAY_SERVER)).width as u32;
                    let sh = (*(&raw mut DISPLAY_SERVER)).height as u32;
                    let fw = (32 as u32).min(sw.saturating_sub(mx));
                    let fh = (32 as u32).min(sh.saturating_sub(my));

                    if fw > 0 && fh > 0 {
                        virtio::flush(mx, my, fw, fh, sw, (*(&raw mut DISPLAY_SERVER)).active_resource_id);
                    }
                }
            }
            return;

        } else if unsafe { (*(&raw mut DRAGGING_WINDOW)).load(Ordering::Relaxed) != 0 } {
            let composer = unsafe { &mut *(&raw mut COMPOSER) };
            let display_server = unsafe { &mut *(&raw mut DISPLAY_SERVER) };
            let wid = unsafe { (*(&raw mut DRAGGING_WINDOW)).load(Ordering::Relaxed) as usize };

            let x_vec = x_rel;
            let y_vec = y_rel;

            let window_opt = composer.find_window_id(wid);
            let w = match window_opt {
                Some(w) => w,
                None => return,
            };

            let old_win_x = w.x;
            let old_win_y = w.y;
            let width = w.width;
            let height = w.height;
            let id = w.id;
            let buffer = w.buffer;

            let target_mx = old_x as i32 + x_vec as i32;
            let target_my = old_y as i32 - y_vec as i32;

            let screen_w = display_server.width as i32;
            let screen_h = display_server.height as i32;

            let mouse_limit_w = screen_w + 50;
            let mouse_limit_h = screen_h + 50;

            let clamped_mx = target_mx.max(0).min(mouse_limit_w - 1);
            let clamped_my = target_my.max(0).min(mouse_limit_h - 1);

            let mouse_dx = clamped_mx - old_x as i32;
            let mouse_dy = clamped_my - old_y as i32;

            let target_win_x = old_win_x as i32 + mouse_dx;
            let target_win_y = old_win_y as i32 + mouse_dy;

            let margin = 3;

            let min_visible_x = -(width as i32) + margin;
            let max_visible_x = screen_w - margin;
            let min_visible_y = -(height as i32) + margin;
            let max_visible_y = screen_h - margin;

            let clamped_win_x = target_win_x.max(min_visible_x).min(max_visible_x);
            let clamped_win_y = target_win_y.max(min_visible_y).min(max_visible_y);

            let allowed_dx = clamped_win_x - old_win_x as i32;
            let allowed_dy = clamped_win_y - old_win_y as i32;

            self.x = (old_x as i32 + allowed_dx).max(0).min(mouse_limit_w - 1) as u16;
            self.y = (old_y as i32 + allowed_dy).max(0).min(mouse_limit_h - 1) as u16;

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
                            w.buffer,
                            w.x as i32,
                            w.y as i32,
                            new_x as i32, new_y as i32, width as u32, height as u32, 
                            None,
                            w.treat_as_transparent
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
                display_server.draw_mouse(self.x, self.y, true);
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
            display_server.copy_to_fb(old_x as i32, old_y as i32, 32, 32);

            display_server.draw_mouse(self.x, self.y, false);

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

                if flush_w > 0 && flush_h > 0 {
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

                    use crate::window_manager::events::{GLOBAL_EVENT_QUEUE, Event, MouseEvent};
                    let event = Event::Mouse(MouseEvent {
                        wid: w.id as u32,
                        x: local_x,
                        y: local_y,
                        buttons: [self.left, self.right, self.center],
                        scroll: scroll_val,
                    });

                    GLOBAL_EVENT_QUEUE.int_lock().add_event(event);

                    if self.left {
                        crate::debugln!("Input: Dispatching Mouse Event to {}", w.id);
                    }
                }
            }
        };
    }

    fn rem_sign(&self, n: i16) -> u16 {
        if n < 0 { (n * -1) as u16 } else { n as u16 }
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

    pub fn draw_square_outline(&self, x: u16, y: u16, width: u16, height: u16, color: Color) {
        let max_x = x + width - 1;
        let max_y = y + height - 1;
        unsafe {
            for i in x..=max_x {
                (*(&raw mut DISPLAY_SERVER)).write_pixel(i as u32, y as u32, color);
                (*(&raw mut DISPLAY_SERVER)).write_pixel(i as u32, max_y as u32, color);
            }
            for i in y..=max_y {
                (*(&raw mut DISPLAY_SERVER)).write_pixel(x as u32, i as u32, color);
                (*(&raw mut DISPLAY_SERVER)).write_pixel(max_x as u32, i as u32, color);
            }
        }
    }

    fn clamp_mx(&self, n: i16) -> u16 {
        let mx_0 = self.x as i16;
        let sx = unsafe { (*(&raw mut DISPLAY_SERVER)).width } as u16;

        let limit = unsafe {
            if (*(&raw mut DRAGGING_WINDOW)).load(Ordering::Relaxed) != 0 {
                sx + 50
            } else {
                sx.saturating_sub(3)
            }
        };

        if n + mx_0 >= (limit as i16) {
            limit.saturating_sub(1)
        } else if n + mx_0 <= 0 {
            0
        } else {
            (n + mx_0) as u16
        }
    }

    fn clamp_my(&self, n: i16) -> u16 {
        let my_0 = self.y as i16;
        let sy = unsafe { (*(&raw mut DISPLAY_SERVER)).height } as u16;

        let limit = unsafe {
            if (*(&raw mut DRAGGING_WINDOW)).load(Ordering::Relaxed) != 0 {
                sy + 50
            } else {
                sy.saturating_sub(3)
            }
        };

        if n + my_0 >= (limit as i16) {
            limit.saturating_sub(1)
        } else if n + my_0 <= 0 {
            return 0;
        } else {
            (n + my_0) as u16
        }
    }
}

fn cap(n: usize, value: usize) -> usize {
    if n > value { value } else { n }
}