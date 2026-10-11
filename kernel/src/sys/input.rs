// Keyboard routing: Win shortcuts go to the desktop manager, other keys to the visible app.
// Fn and Right Ctrl act as Win, for remote sessions where the host swallows Win.

pub const EV_KEY: u16 = 1;

pub const MOD_SHIFT: u32 = 1;
pub const MOD_CTRL: u32 = 2;
pub const MOD_ALT: u32 = 4;
pub const MOD_SUPER: u32 = 8;
pub const MOD_CAPS: u32 = 16;
// Right Alt, kept apart from Alt so apps can read it as AltGr.
pub const MOD_ALTGR: u32 = 32;

const KEY_LEFTSHIFT: u16 = 42;
const KEY_RIGHTSHIFT: u16 = 54;
const KEY_LEFTCTRL: u16 = 29;
const KEY_RIGHTCTRL: u16 = 97;
const KEY_LEFTALT: u16 = 56;
const KEY_RIGHTALT: u16 = 100;
const KEY_LEFTMETA: u16 = 125;
const KEY_RIGHTMETA: u16 = 126;
// Fn, for keyboards that report it.
const KEY_FN: u16 = 0x1d0;
const KEY_CAPSLOCK: u16 = 58;
const KEY_ENTER: u16 = 28;
const KEY_Q: u16 = 16;
const KEY_LEFT: u16 = 105;
const KEY_RIGHT: u16 = 106;
const KEY_UP: u16 = 103;
const KEY_DOWN: u16 = 108;

pub const TERMINAL_PATH: &str = "apps/terminal.wasm";

static mut MODS: u32 = 0;
// Win is down and nothing else has been pressed since.
static mut SUPER_ALONE: bool = false;

fn spawn_terminal() {
    crate::sys::syscall::SPAWN_QUEUE
        .lock()
        .push_back(alloc::string::String::from(TERMINAL_PATH));
}

pub fn push_event(type_: u16, code: u16, value: u32) {
    if type_ != EV_KEY || (code >= 0x100 && code != KEY_FN) {
        return; // pointer motion, buttons, sync
    }
    let down = value != 0;
    let bit = match code {
        KEY_LEFTSHIFT | KEY_RIGHTSHIFT => MOD_SHIFT,
        KEY_LEFTCTRL => MOD_CTRL,
        KEY_LEFTALT => MOD_ALT,
        KEY_RIGHTALT => MOD_ALTGR,
        KEY_LEFTMETA | KEY_RIGHTMETA | KEY_FN | KEY_RIGHTCTRL => MOD_SUPER,
        _ => 0,
    };
    unsafe {
        if bit == MOD_SUPER {
            if value == 1 {
                if MODS & MOD_SUPER == 0 {
                    SUPER_ALONE = true;
                }
                MODS |= MOD_SUPER;
            } else if value == 0 {
                MODS &= !MOD_SUPER;
                if SUPER_ALONE {
                    spawn_terminal();
                }
                SUPER_ALONE = false;
            }
            return;
        }
        if bit != 0 {
            if down {
                MODS |= bit;
            } else {
                MODS &= !bit;
            }
        }
        if code == KEY_CAPSLOCK && value == 1 {
            MODS ^= MOD_CAPS;
        }

        if MODS & MOD_SUPER != 0 {
            if code == KEY_UP || code == KEY_DOWN {
                SUPER_ALONE = false;
                crate::sys::desktop::key(code, value, MODS);
                return;
            }
            // Win+Up/Down go to the app for scrolling; anything else is a desktop shortcut.
            if value != 0 {
                SUPER_ALONE = false;
                match code {
                    KEY_LEFT if value == 1 => crate::sys::desktop::switch_left(),
                    KEY_RIGHT if value == 1 => crate::sys::desktop::switch_right(),
                    KEY_Q if value == 1 => crate::sys::desktop::close_current(),
                    KEY_ENTER if value == 1 => spawn_terminal(),
                    _ => {}
                }
            }
            return;
        }
        crate::sys::desktop::key(code, value, MODS);
    }
}
