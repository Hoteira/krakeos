use core::sync::atomic::{AtomicU16, Ordering};
use crate::debugln;
use crate::window_manager::composer::{COMPOSER, CLICKED_WINDOW_ID};
use crate::window_manager::display::{DISPLAY_SERVER, HARDWARE_CURSOR_ACTIVE, VIRTIO_ACTIVE};
use crate::drivers::video::virtio;
use crate::window_manager::window::Items;

use core::sync::atomic::AtomicBool;
pub static MOUSE_PENDING: AtomicBool = AtomicBool::new(false);

use crate::sync::Mutex;

pub struct Mouse {
    pub x: isize,
    pub y: isize,
    pub left: bool,
    pub right: bool,
    pub center: bool,
}

pub static MOUSE: Mutex<Mouse> = Mutex::new(Mouse {
    x: 0,
    y: 0,
    left: false,
    right: false,
    center: false,
});

impl Mouse {
    pub fn update(&mut self, dx: isize, dy: isize, left: bool, right: bool, center: bool, scroll: i8, _is_super: bool) {
        let old_x = self.x;
        let old_y = self.y;
        let _btns_changed = self.left != left || self.right != right || self.center != center;

        self.x = (self.x + dx).max(0).min(DISPLAY_SERVER.lock().width as isize - 1);
        self.y = (self.y + dy).max(0).min(DISPLAY_SERVER.lock().height as isize - 1);

        let moved = old_x != self.x || old_y != self.y;
        self.left = left;
        self.right = right;
        self.center = center;

        if moved {
            let mut composer = COMPOSER.write();
            let ws_opt = composer.find_window(self.x as usize, self.y as usize);
            if let Some(ws) = ws_opt {
                let old_id = CLICKED_WINDOW_ID.load(core::sync::atomic::Ordering::SeqCst);
                let new_id = ws.id;

                if old_id != new_id as usize {
                    CLICKED_WINDOW_ID.store(new_id as usize, core::sync::atomic::Ordering::SeqCst);
                    composer.focus_window(new_id);
                }
            }
        }

        let mut display_server = DISPLAY_SERVER.lock();
        if !HARDWARE_CURSOR_ACTIVE.load(core::sync::atomic::Ordering::SeqCst) {
            display_server.copy_to_fb(old_x as i32, old_y as i32, 32, 32);
            display_server.draw_mouse(self.x as u16, self.y as u16, false);
        }

        if VIRTIO_ACTIVE.load(core::sync::atomic::Ordering::SeqCst) {
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

            if !HARDWARE_CURSOR_ACTIVE.load(core::sync::atomic::Ordering::SeqCst) && flush_w > 0 && flush_h > 0 {
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

        // Release DISPLAY_SERVER lock before acquiring COMPOSER lock to prevent AB-BA deadlock
        drop(display_server);

        let composer = COMPOSER.read();
        if let Some(w) = composer.find_window_immut(self.x as usize, self.y as usize) {
            if w.event_handler != 0 {
                let local_x = (self.x as i64 - w.x).max(0) as usize;
                let local_y = (self.y as i64 - w.y).max(0) as usize;

                use crate::window_manager::events::{Event, GLOBAL_EVENT_QUEUE, MouseEvent};

                static LAST_X: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(9999);
                static LAST_Y: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(9999);
                static LAST_BTNS: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

                let btns = [self.left, self.right, self.center];
                let btns_byte = (if self.left { 1 } else { 0 }) | (if self.right { 2 } else { 0 }) | (if self.center { 4 } else { 0 });
                
                if local_x != LAST_X.load(core::sync::atomic::Ordering::Relaxed)
                    || local_y != LAST_Y.load(core::sync::atomic::Ordering::Relaxed)
                    || btns_byte != LAST_BTNS.load(core::sync::atomic::Ordering::Relaxed)
                    || scroll != 0
                {
                    let event = Event::Mouse(MouseEvent {
                        wid: w.id as u32,
                        x: local_x as u32,
                        y: local_y as u32,
                        buttons: btns,
                        scroll,
                    });

                    let mut pushed = false;
                    if let Some(mut tm) = crate::task::TASK_MANAGER.try_lock() {
                        if GLOBAL_EVENT_QUEUE.int_lock().push_to_process(&mut tm, w.pid, event) {
                            pushed = true;
                        }
                    }

                    if !pushed {
                        GLOBAL_EVENT_QUEUE.lock().add_event(event);
                    }

                    LAST_X.store(local_x, core::sync::atomic::Ordering::Relaxed);
                    LAST_Y.store(local_y, core::sync::atomic::Ordering::Relaxed);
                    LAST_BTNS.store(btns_byte, core::sync::atomic::Ordering::Relaxed);
                }
            }
        }
    }
}

pub fn handle_vmmouse(buttons: u32, x: u32, y: u32, z: u32) {
    let (screen_w, screen_h) = {
        let ds = DISPLAY_SERVER.lock();
        (ds.width as isize, ds.height as isize)
    };
    
    let abs_x = (x as isize * screen_w) / 0xFFFF;
    let abs_y = (y as isize * screen_h) / 0xFFFF;
    
    let mut mouse = MOUSE.lock();
    let dx = abs_x - mouse.x;
    let dy = abs_y - mouse.y;
    
    let left = (buttons & 0x20) != 0;
    let right = (buttons & 0x10) != 0;
    let center = (buttons & 0x08) != 0;
    let scroll = z as i8;
    
    mouse.update(dx, dy, left, right, center, scroll, false);
}

pub fn handle_mouse_update() {
    let packet = unsafe { crate::drivers::peripherals::mouse::MOUSE_PACKET };
    let dx = packet[1] as i8 as isize;
    let dy = -(packet[2] as i8 as isize);
    let left = (packet[0] & 1) != 0;
    let right = (packet[0] & 2) != 0;
    let center = (packet[0] & 4) != 0;
    let scroll = packet[3] as i8;
    
    MOUSE.lock().update(dx, dy, left, right, center, scroll, false);
}
