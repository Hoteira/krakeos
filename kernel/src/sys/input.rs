use spin::Mutex;
use alloc::collections::VecDeque;

// Wire format for /dev/input: apps parse [type_: u16][code: u16][value: u32]
// little-endian, so the layout must be C-stable (repr(Rust) reorders fields).
#[repr(C)]
pub struct InputEvent {
    pub type_: u16,
    pub code: u16,
    pub value: u32,
}

pub struct InputState {
    pub keyboard_events: VecDeque<InputEvent>,
    pub mouse_events: VecDeque<InputEvent>,
}

pub static INPUT_STATE: Mutex<InputState> = Mutex::new(InputState {
    keyboard_events: VecDeque::new(),
    mouse_events: VecDeque::new(),
});

pub const EV_KEY: u16 = 1;
pub const EV_REL: u16 = 2;
pub const EV_ABS: u16 = 3;

// Hardware-cursor tracking: the kernel scales tablet EV_ABS to screen
// coordinates and moves the virtio-gpu cursor directly from the timer tick,
// so pointer motion doesn't wait for the shell's (slow, interpreted) redraw.
static mut HW_CURSOR_X: u32 = 0;
static mut HW_CURSOR_Y: u32 = 0;
static mut HW_CURSOR_MOVED: bool = false;

pub fn take_cursor_move() -> Option<(u32, u32)> {
    unsafe {
        if HW_CURSOR_MOVED {
            HW_CURSOR_MOVED = false;
            Some((HW_CURSOR_X, HW_CURSOR_Y))
        } else {
            None
        }
    }
}

pub fn push_event(type_: u16, code: u16, value: u32) {
    if type_ == EV_REL || type_ == EV_ABS || (type_ == EV_KEY && (code == 0x110 || code == 0x111)) {
        if type_ == EV_ABS {
            unsafe {
                // Same 0..32767 -> pixel scaling the shell uses for hit tests
                if code == 0 {
                    HW_CURSOR_X = (value * crate::drivers::virtio_gpu::FB_WIDTH / 32767)
                        .min(crate::drivers::virtio_gpu::FB_WIDTH - 1);
                    HW_CURSOR_MOVED = true;
                } else if code == 1 {
                    HW_CURSOR_Y = (value * crate::drivers::virtio_gpu::FB_HEIGHT / 32767)
                        .min(crate::drivers::virtio_gpu::FB_HEIGHT - 1);
                    HW_CURSOR_MOVED = true;
                }
            }
        }
        // Route button events to the window under the cursor (content-area
        // clicks reach apps; the shell still sees the raw event for its own
        // chrome via the queue below).
        if type_ == EV_KEY && (code == 0x110 || code == 0x111) {
            unsafe {
                let btn = if code == 0x110 { 0 } else { 1 };
                crate::sys::compositor::route_mouse(HW_CURSOR_X, HW_CURSOR_Y, btn, value as u8);
            }
        }
        let mut state = INPUT_STATE.lock();
        if state.mouse_events.len() < 256 {
            state.mouse_events.push_back(InputEvent { type_, code, value });
        }
    }
    // EV_KEY (Keyboard events)
    else if type_ == EV_KEY {
        let mut state = INPUT_STATE.lock();
        if state.keyboard_events.len() < 256 {
            state.keyboard_events.push_back(InputEvent { type_, code, value });
        }
    }
}
