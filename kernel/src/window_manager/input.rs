use core::sync::atomic::{AtomicU16, Ordering};
use crate::debugln;
use crate::drivers::video::virtio;
use crate::window_manager::display::{DISPLAY_SERVER, VIRTIO_ACTIVE, Color, State, Mouse as DisplayMouse}; 
use super::composer::{COMPOSER, Composer};
use super::events::{Event, MouseEvent, ResizeEvent, GLOBAL_EVENT_QUEUE};
use super::window::{Items, Window};

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
        // Store old position
        let old_x = self.x;
        let old_y = self.y;

        debugln!(".");

        // Calculate relative movement
        let mut x_rel = data[1] as i16;
        let mut y_rel = data[2] as i16;

        if (data[0] & 0x10) != 0 {
            x_rel |= 0xFF00u16 as i16;
        }

        if (data[0] & 0x20) != 0 {
            y_rel |= 0xFF00u16 as i16;
        }

        // Update mouse position
        self.x = self.clamp_mx(x_rel);
        self.y = self.clamp_my(-y_rel);

        // Store previous button state
        let prev_left = self.left;

        // Update button states
        self.left = (data[0] & 0b00000001) != 0;
        self.right = (data[0] & 0b00000010) != 0;
        self.center = (data[0] & 0b00000100) != 0;

        unsafe {
            LAST_INPUT = data[0];
        }

        let scroll_val = data[3] as i8;

        // Check for click start in titlebar
        if self.left && !prev_left {
            let w = unsafe { (*(&raw mut COMPOSER)).find_window(self.x as usize, self.y as usize) };
            if let Some(ws) = w {
                if ws.can_move && self.y as usize >= ws.y && self.y as usize <= ws.y + 25 {
                    unsafe { CLICK_STARTED_IN_TITLEBAR = true; }
                } else {
                    unsafe { CLICK_STARTED_IN_TITLEBAR = false; }
                }
            } else {
                unsafe { CLICK_STARTED_IN_TITLEBAR = false; }
            }
        } else if !self.left {
            unsafe { CLICK_STARTED_IN_TITLEBAR = false; }
        }

        // Handle drag state
        unsafe {
            if self.left {
                DRAGS = DRAGS.wrapping_add(1);
                if DRAGS > 2 {
                    DRAG = true;
                }
            } else {
                DRAGS = 0;
                DRAG = false;

                // Handle resize completion
                if (*(&raw mut RESIZING_WINDOW)).load(Ordering::Relaxed) != 0 {
                    let w = (*(&raw mut COMPOSER))
                        .find_window_id((*(&raw mut RESIZING_WINDOW)).load(Ordering::Relaxed) as usize)
                        .unwrap();

                    (*(&raw mut DRAGGING_WINDOW)).store(0, Ordering::Relaxed);
                    (*(&raw mut RESIZING_WINDOW)).store(0, Ordering::Relaxed);

                    if w.event_handler != 0 {
                        (*(&raw mut GLOBAL_EVENT_QUEUE)).add_event(Event::Resize(ResizeEvent {
                            wid: w.id as u32,
                            width: W_WIDTH,
                            height: W_HEIGHT,
                        }));
                    }
                    W_WIDTH = 0;
                    W_HEIGHT = 0;

                // Handle drag completion - window released
                } else if (*(&raw mut DRAGGING_WINDOW)).load(Ordering::Relaxed) != 0 {
                    let wid = (*(&raw mut DRAGGING_WINDOW)).load(Ordering::Relaxed) as usize;
                    let composer = &mut *(&raw mut COMPOSER);
                    let display_server = &mut *(&raw mut DISPLAY_SERVER);

                    // Get window info
                    let w = composer.find_window_id(wid).unwrap();
                    let win_x = w.x;
                    let win_y = w.y;
                    let win_width = w.width;
                    let win_height = w.height;

                    // Copy window to double buffer at its new position
                    composer.copy_window(wid);

                    // Clear and restore the old cursor area from DB
                    // Note: copy_to_fb now in display_server
                    display_server.copy_to_fb(old_x as u32, old_y as u32, 32, 32);

                    // Copy window from DB to FB
                    display_server.copy_to_fb(win_x as u32, win_y as u32, win_width as u32, win_height as u32);

                    // FLUSH THE WINDOW AREA TO VIRTIO
                    if VIRTIO_ACTIVE {
                        virtio::flush(win_x as u32, win_y as u32, win_width as u32, win_height as u32, display_server.width as u32);
                    }

                    // Draw cursor at new position
                    display_server.draw_mouse(self.x, self.y, false);

                    (*(&raw mut DRAGGING_WINDOW)).store(0, Ordering::Relaxed);
                    (*(&raw mut RESIZING_WINDOW)).store(0, Ordering::Relaxed);
                    W_WIDTH = 0;
                    W_HEIGHT = 0;

                    return;
                }
            }
        }

        let w = unsafe { (*(&raw mut COMPOSER)).find_window(self.x as usize, self.y as usize) };

        // Handle active resize
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
                W_WIDTH = cap(final_width, ((*(&raw mut DISPLAY_SERVER)).width as usize).saturating_sub(w.x));
                W_HEIGHT = cap(final_height, ((*(&raw mut DISPLAY_SERVER)).height as usize).saturating_sub(w.y));
            }

            self.draw_square_outline(
                w.y as u16,
                w.x as u16,
                unsafe { W_HEIGHT as u16 },
                unsafe { W_WIDTH as u16 },
                Color::rgb(245, 245, 247)
            );

            unsafe { (*(&raw mut DISPLAY_SERVER)).draw_mouse(self.x, self.y, false) };
            return;

        // Handle active drag - window being moved
        } else if unsafe { (*(&raw mut DRAGGING_WINDOW)).load(Ordering::Relaxed) != 0 } {
            let composer = unsafe { &mut *(&raw mut COMPOSER) };
            let display_server = unsafe { &mut *(&raw mut DISPLAY_SERVER) };
            let wid = unsafe { (*(&raw mut DRAGGING_WINDOW)).load(Ordering::Relaxed) as usize };

            let x_vec = x_rel;
            let y_vec = y_rel;

            // Get window info and calculate new position
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

            // Calculate new window position based on mouse delta
            let mut new_x = old_win_x as i32 + x_vec as i32;
            let mut new_y = old_win_y as i32 - y_vec as i32; // Note: y_vec is inverted

            // Only clamp the TOP edge to keep titlebar grabbable (keep 3px visible)
            // let screen_height = display_server.height as i32;
            let min_y = -(height as i32) + 3;

            if new_y < min_y { new_y = min_y; }

            // No other clamping - let window go anywhere else!

            // Update window position
            let new_x = new_x.abs() as usize;
            let new_y = new_y.abs() as usize;

            // Update the window coordinates in the composer
            for i in 0..composer.windows.len() {
                if composer.windows[i].id == id {
                    composer.windows[i].x = new_x;
                    composer.windows[i].y = new_y;
                    break;
                }
            }

            // Erase old window position by copying clean background from DB to FB
            let src = display_server.double_buffer as *const u8;
            let dst = display_server.framebuffer as *mut u8;
            let pitch = display_server.pitch as usize;

            // Fast path: window fully onscreen
            if old_win_x + width <= display_server.width as usize &&
                old_win_y + height <= display_server.height as usize {
                unsafe {
                    for row in 0..height {
                        let offset = ((old_win_y + row) * pitch + old_win_x * 4) as usize;
                        core::ptr::copy_nonoverlapping(
                            src.add(offset),
                            dst.add(offset),
                            width * 4
                        );
                    }
                }
            } else if old_win_x < display_server.width as usize && old_win_y < display_server.height as usize {
                // Slow path: window partially offscreen
                let visible_width = width.min(display_server.width as usize - old_win_x);
                let visible_height = height.min(display_server.height as usize - old_win_y);

                unsafe {
                    for row in 0..visible_height {
                        let offset = ((old_win_y + row) * pitch + old_win_x * 4) as usize;
                        core::ptr::copy_nonoverlapping(
                            src.add(offset),
                            dst.add(offset),
                            visible_width * 4
                        );
                    }
                }
            }

            // Draw window from its buffer at new position directly to FB
            let src_buffer = buffer as *const u8;
            let dst_fb = display_server.framebuffer as *mut u8;
            let src_pitch = width * 4;
            let dst_pitch = display_server.pitch as usize;

            // Fast path: window fully onscreen at new position
            if  new_x + width <= display_server.width as usize &&
                new_y + height <= display_server.height as usize {
                let new_x = new_x as usize;
                let new_y = new_y as usize;
                unsafe {
                    for row in 0..height {
                        let src_offset = row * src_pitch;
                        let dst_offset = (new_y + row) * dst_pitch + new_x * 4;
                        core::ptr::copy_nonoverlapping(
                            src_buffer.add(src_offset),
                            dst_fb.add(dst_offset),
                            width * 4
                        );
                    }
                }
            } else {
                // Slow path: window partially offscreen
                let draw_x = new_x.max(0) as usize;
                let draw_y = new_y.max(0) as usize;

                if draw_x < display_server.width as usize && draw_y < display_server.height as usize {
                    let src_x_offset = if new_x < 0 { (-(new_x as i32)) as usize } else { 0 };
                    let src_y_offset = if new_y < 0 { (-(new_y as i32)) as usize } else { 0 };

                    let visible_width = (width - src_x_offset).min(display_server.width as usize - draw_x);
                    let visible_height = (height - src_y_offset).min(display_server.height as usize - draw_y);

                    unsafe {
                        for row in 0..visible_height {
                            let src_offset = (src_y_offset + row) * src_pitch + src_x_offset * 4;
                            let dst_offset = (draw_y + row) * dst_pitch + draw_x * 4;
                            core::ptr::copy_nonoverlapping(
                                src_buffer.add(src_offset),
                                dst_fb.add(dst_offset),
                                visible_width * 4
                            );
                        }
                    }
                }
            }

            // Calculate union rect of old and new positions for efficient flush
            // Clamp to screen bounds for VirtIO flush
            let old_x_clamped = (old_win_x as i32).max(0) as u32;
            let old_y_clamped = (old_win_y as i32).max(0) as u32;
            let new_x_clamped = new_x.max(0) as u32;
            let new_y_clamped = new_y.max(0) as u32;
            let screen_w = unsafe { (*(&raw const DISPLAY_SERVER)).width as usize };
            let screen_h = unsafe { (*(&raw const DISPLAY_SERVER)).height as usize };

            let old_x_end = (old_win_x + width).max(0).min(screen_w) as u32;
            let old_y_end = (old_win_y + height).max(0).min(screen_h) as u32;
            let new_x_end = (new_x + width).max(0).min(screen_w) as u32;
            let new_y_end = (new_y + height).max(0).min(screen_h) as u32;

            let min_x = old_x_clamped.min(new_x_clamped);
            let min_y = old_y_clamped.min(new_y_clamped);
            let max_x = old_x_end.max(new_x_end);
            let max_y = old_y_end.max(new_y_end);

            let flush_x = min_x;
            let flush_y = min_y;
            let flush_w = max_x.saturating_sub(min_x);
            let flush_h = max_y.saturating_sub(min_y);

            // SINGLE FLUSH TO VIRTIO for both old and new positions
            unsafe {
                if VIRTIO_ACTIVE {
                    virtio::flush(flush_x, flush_y, flush_w, flush_h, display_server.width as u32);
                }
            }

            // Draw cursor at new position
            // PASSING TRUE because we are dragging a window!
            display_server.draw_mouse(self.x, self.y, true);
            return;
        }

        // Normal case (not dragging) - erase old cursor by copying from DB to FB
        unsafe {
            let display_server = &mut *(&raw mut DISPLAY_SERVER);
            display_server.copy_to_fb(old_x as u32, old_y as u32, 32, 32);
        };

        // Handle window interaction
        if let Some(ws) = w {
            // Bring window to front on click
            if ws.w_type == Items::Window && ws.z != 0 && self.left && !prev_left {
                let x = ws.x;
                let y = ws.y;
                let width = ws.width;
                let height = ws.height;
                let id = ws.id;

                unsafe {
                    for i in (*(&raw mut COMPOSER)).windows.iter_mut() {
                        if i.id != id {
                            i.z = i.z.wrapping_add(1);
                        } else {
                            i.z = 0;
                        }
                    }
                    (*(&raw mut COMPOSER)).windows.sort_by_key(|w| w.z);
                    (*(&raw mut COMPOSER)).copy_window(id);

                    (*(&raw mut DISPLAY_SERVER)).copy_to_fb(
                        x as u32,
                        y as u32,
                        width as u32,
                        height as u32,
                    );
                }
            }

            let mut handled_drag = false;
            if self.left {
                // Check if starting drag from titlebar
                if ws.can_move && self.y as usize >= ws.y && self.y as usize <= ws.y + 25 {
                    if unsafe { DRAG == true && CLICK_STARTED_IN_TITLEBAR } {
                        if unsafe { (*(&raw mut DRAGGING_WINDOW)).load(Ordering::Relaxed) == 0 } {
                            unsafe {
                                (*(&raw mut DRAGGING_WINDOW)).store(ws.id as u16, Ordering::Relaxed);
                                (*(&raw mut COMPOSER)).recompose_except(ws.id);
                            }
                        }
                        handled_drag = true;
                    }
                // Check if starting resize from bottom-right corner
                } else if ws.can_move && self.is_bottom_right(
                    ws.x as u16,
                    ws.y as u16,
                    ws.width as u16,
                    ws.height as u16,
                    self.x,
                    self.y
                ) {
                    if unsafe { DRAG == true } {
                        if unsafe { (*(&raw mut RESIZING_WINDOW)).load(Ordering::Relaxed) == 0 } {
                            unsafe {
                                W_WIDTH = ws.width;
                                W_HEIGHT = ws.height;
                                (*(&raw mut RESIZING_WINDOW)).store(ws.id as u16, Ordering::Relaxed);
                            }
                        }
                        handled_drag = true;
                    }
                }
            }

            if handled_drag {
                unsafe { (*(&raw mut DISPLAY_SERVER)).draw_mouse(self.x, self.y, false) };
                return;
            }

            // Send mouse event to window
            if ws.event_handler != 0 && unsafe { DRAG == false } {
                let xc = ws.x;
                let yc = ws.y;
                let id = ws.id;
                unsafe {
                    (*(&raw mut GLOBAL_EVENT_QUEUE)).add_event(Event::Mouse(MouseEvent {
                        wid: id as u32,
                        x: self.x as usize - xc,
                        y: self.y as usize - yc,
                        buttons: [self.left, self.center, self.right],
                        scroll: scroll_val,
                    }));
                }
            }
        }

        // Draw mouse cursor at new position
        unsafe {
            if VIRTIO_ACTIVE {
                // Use hardware cursor if available!
                virtio::move_cursor(self.x as u32, self.y as u32);
            } else {
                (*(&raw mut DISPLAY_SERVER)).draw_mouse(self.x, self.y, false);
            }
        }
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

        if n + mx_0 >= (sx as i16 - 2) {
            sx.wrapping_sub(2)
        } else if n + mx_0 <= 0 {
            0
        } else {
            (n + mx_0) as u16
        }
    }

    fn clamp_my(&self, n: i16) -> u16 {
        let my_0 = self.y as i16;
        let sy = unsafe { (*(&raw mut DISPLAY_SERVER)).height } as u16;

        if n + my_0 >= (sy as i16 - 2) {
            sy.wrapping_sub(2)
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
