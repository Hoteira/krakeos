use core::sync::atomic::{AtomicU16, Ordering};
use crate::debugln;
use crate::window_manager::composer::{COMPOSER, CLICKED_WINDOW_ID};
use crate::window_manager::display::{DISPLAY_SERVER, HARDWARE_CURSOR_ACTIVE, VIRTIO_ACTIVE};
use crate::drivers::video::virtio;
use crate::window_manager::window::Items;

pub static mut DRAGS: u8 = 0;
pub static mut DRAG: bool = false;
pub static DRAGGING_WINDOW: AtomicU16 = AtomicU16::new(0);
pub static RESIZING_WINDOW: AtomicU16 = AtomicU16::new(0);
pub static mut CLICK_STARTED_IN_TITLEBAR: bool = false;
pub static mut CLICK_X_OFFSET: i32 = 0;
pub static mut CLICK_Y_OFFSET: i32 = 0;
pub static mut W_WIDTH: usize = 0;
pub static mut W_HEIGHT: usize = 0;
pub static mut MOUSE_PENDING: bool = false;

pub struct Mouse {
    pub x: isize,
    pub y: isize,
    pub left: bool,
    pub right: bool,
    pub center: bool,
}

pub static mut MOUSE: Mouse = Mouse {
    x: 0,
    y: 0,
    left: false,
    right: false,
    center: false,
};

impl Mouse {
    pub fn update(&mut self, dx: isize, dy: isize, left: bool, right: bool, center: bool, is_super: bool) {
        let old_x = self.x;
        let old_y = self.y;
        let btns_changed = self.left != left || self.right != right || self.center != center;

        self.x = (self.x + dx).max(0).min(unsafe { (*(&raw mut DISPLAY_SERVER)).width as isize - 1 });
        self.y = (self.y + dy).max(0).min(unsafe { (*(&raw mut DISPLAY_SERVER)).height as isize - 1 });

        let moved = old_x != self.x || old_y != self.y;
        let clicked = !self.left && left;
        self.left = left;
        self.right = right;
        self.center = center;

        if clicked {
            let ws_opt = unsafe { (*(&raw mut COMPOSER)).find_window(self.x as usize, self.y as usize) };
            if let Some(ws) = ws_opt {
                unsafe {
                    let old_id = CLICKED_WINDOW_ID;
                    let new_id = ws.id;

                    if old_id != new_id as usize {
                        CLICKED_WINDOW_ID = new_id as usize;
                        (*(&raw mut COMPOSER)).focus_window(new_id);
                    }

                    // Capture offset for absolute dragging
                    CLICK_X_OFFSET = self.x as i32 - ws.x as i32;
                    CLICK_Y_OFFSET = self.y as i32 - ws.y as i32;
                }

                if ws.can_move && is_super {
                    unsafe {
                        CLICK_STARTED_IN_TITLEBAR = true;
                    }
                } else {
                    unsafe {
                        CLICK_STARTED_IN_TITLEBAR = false;
                    }
                }
            } else {
                unsafe {
                    CLICKED_WINDOW_ID = 0;
                    CLICK_STARTED_IN_TITLEBAR = false;
                }
            }
        } else if !self.left {
            unsafe {
                CLICK_STARTED_IN_TITLEBAR = false;
            }
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
                    let wid = DRAGGING_WINDOW.load(Ordering::Relaxed) as u64;
                    let composer = &mut *(&raw mut COMPOSER);
                    let display_server = &mut *(&raw mut DISPLAY_SERVER);

                    let w_opt = composer.find_window_id(wid);
                    if w_opt.is_none() {
                        return;
                    }

                    let w = w_opt.unwrap();
                    let win_x = w.x;
                    let win_y = w.y;
                    let win_width = w.width;
                    let win_height = w.height;

                    composer.copy_window(wid);

                    display_server.copy_to_fb(old_x as i32, old_y as i32, 32, 32);

                    display_server.copy_to_fb(
                        win_x as i32,
                        win_y as i32,
                        win_width as u32,
                        win_height as u32,
                    );

                    if !HARDWARE_CURSOR_ACTIVE {
                        display_server.draw_mouse(self.x as u16, self.y as u16, false);
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
                    let wid = CLICKED_WINDOW_ID as u64;
                    DRAGGING_WINDOW.store(wid as u16, Ordering::Relaxed);
                    (*(&raw mut COMPOSER)).recompose_except(wid);
                }
            }
        }

        if DRAGGING_WINDOW.load(Ordering::Relaxed) != 0 {
            let composer = unsafe { &mut *(&raw mut COMPOSER) };
            let display_server = unsafe { &mut *(&raw mut DISPLAY_SERVER) };
            let wid = DRAGGING_WINDOW.load(Ordering::Relaxed) as u64;

            if !moved && !btns_changed {
                return;
            }

            let (old_x_pos, old_y_pos, width, height) = {
                let w = match composer.find_window_id(wid) {
                    Some(w) => w,
                    None => return,
                };

                let old_x = w.x;
                let old_y = w.y;
                let width = w.width;
                let height = w.height;

                // ABSOLUTE MATH: New position is CurrentMouse - ClickOffset
                let target_win_x = self.x as i32 - unsafe { CLICK_X_OFFSET };
                let target_win_y = self.y as i32 - unsafe { CLICK_Y_OFFSET };

                let screen_w = display_server.width as i32;
                let screen_h = display_server.height as i32;

                // Clamp to ensure window doesn't disappear and handles edge limits correctly
                let new_x = target_win_x.max(-((width as i32) - 20)).min(screen_w - 20);
                let new_y = target_win_y.max(0).min(screen_h - 20);

                w.x = new_x as i64;
                w.y = new_y as i64;

                (old_x, old_y, width, height)
            };

            let (new_x_pos, new_y_pos) = {
                let w = composer.find_window_id(wid).unwrap();
                (w.x, w.y)
            };

            // Only update if the position actually changed after clamping
            if old_x_pos != new_x_pos || old_y_pos != new_y_pos {
                // Calculate the total area that needs updating (Union of old and new)
                let min_x = old_x_pos.min(new_x_pos) as i32;
                let min_y = old_y_pos.min(new_y_pos) as i32;
                let max_x = (old_x_pos + width as i64).max(new_x_pos + width as i64) as i32;
                let max_y = (old_y_pos + height as i64).max(new_y_pos + height as i64) as i32;

                let update_w = (max_x - min_x) as u32;
                let update_h = (max_y - min_y) as u32;

                // Perform ONE unified recomposition into the back-buffer
                composer.update_window_area_rect(min_x, min_y, update_w, update_h);
            }
        }

        unsafe {
            let display_server = &mut *(&raw mut DISPLAY_SERVER);
            if !HARDWARE_CURSOR_ACTIVE {
                display_server.copy_to_fb(old_x as i32, old_y as i32, 32, 32);
                display_server.draw_mouse(self.x as u16, self.y as u16, false);
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
                    virtio::flush(
                        flush_x,
                        flush_y,
                        flush_w,
                        flush_h,
                        screen_w,
                        display_server.active_resource_id,
                        false,
                    );
                    display_server.sync_vbe_rect(flush_x, flush_y, flush_w, flush_h);
                }
            }

            if let Some(w) = (*(&raw mut COMPOSER)).find_window(self.x as usize, self.y as usize) {
                if w.event_handler != 0 {
                    let local_x = (self.x as i64 - w.x).max(0) as usize;
                    let local_y = (self.y as i64 - w.y).max(0) as usize;

                    use crate::window_manager::events::{Event, GLOBAL_EVENT_QUEUE, MouseEvent};

                    static mut LAST_X: usize = 9999;
                    static mut LAST_Y: usize = 9999;
                    static mut LAST_BTNS: [bool; 3] = [false; 3];

                    let btns = [self.left, self.right, self.center];
                    if local_x != unsafe { LAST_X }
                        || local_y != unsafe { LAST_Y }
                        || btns != unsafe { LAST_BTNS }
                    {
                        let mut event_queue = GLOBAL_EVENT_QUEUE.lock();
                        event_queue.add_event(Event::Mouse(MouseEvent {
                            wid: w.id as u32,
                            x: local_x as u32,
                            y: local_y as u32,
                            buttons: btns,
                            scroll: 0,
                        }));
                        unsafe {
                            LAST_X = local_x;
                            LAST_Y = local_y;
                            LAST_BTNS = btns;
                        }
                    }
                }
            }
        }
    }
}

pub fn handle_vmmouse(buttons: u32, x: u32, y: u32, z: u32) {
    unsafe {
        let screen_w = (*(&raw mut DISPLAY_SERVER)).width as isize;
        let screen_h = (*(&raw mut DISPLAY_SERVER)).height as isize;
        
        let abs_x = (x as isize * screen_w) / 0xFFFF;
        let abs_y = (y as isize * screen_h) / 0xFFFF;
        
        let dx = abs_x - MOUSE.x;
        let dy = abs_y - MOUSE.y;
        
        let left = (buttons & 0x20) != 0;
        let right = (buttons & 0x10) != 0;
        let center = (buttons & 0x08) != 0;
        
        (*(&raw mut MOUSE)).update(dx, dy, left, right, center, false);
    }
}

pub fn handle_mouse_update() {
    unsafe {
        let packet = crate::drivers::peripherals::mouse::MOUSE_PACKET;
        let dx = packet[1] as i8 as isize;
        let dy = -(packet[2] as i8 as isize);
        let left = (packet[0] & 1) != 0;
        let right = (packet[0] & 2) != 0;
        let center = (packet[0] & 4) != 0;
        
        (*(&raw mut MOUSE)).update(dx, dy, left, right, center, false);
    }
}
