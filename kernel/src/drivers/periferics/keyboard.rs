

use crate::drivers::port::{inb, outb};
use alloc::collections::VecDeque;
use std::sync::Mutex;

#[allow(dead_code)]
pub static KEYBOARD_BUFFER: Mutex<VecDeque<u32>> = Mutex::new(VecDeque::new());


pub const KEY_LEFT: u32 = 0x110001;
pub const KEY_RIGHT: u32 = 0x110002;
pub const KEY_UP: u32 = 0x110003;
pub const KEY_DOWN: u32 = 0x110004;
pub const KEY_BACKSPACE: u32 = 0x08;
pub const KEY_ENTER: u32 = 0x0D;
pub const KEY_CTRL: u32 = 0x110005;
pub const KEY_ALT: u32 = 0x110006;
pub const KEY_SHIFT: u32 = 0x110007;


#[allow(dead_code)]
const DATA_PORT: u16 = 0x60;
#[allow(dead_code)]
const STATUS_PORT: u16 = 0x64;
#[allow(dead_code)]
const COMMAND_PORT: u16 = 0x64;


#[allow(dead_code)]
const PS2_CMD_READ_CONFIG: u8 = 0x20;
#[allow(dead_code)]
const PS2_CMD_WRITE_CONFIG: u8 = 0x60;
#[allow(dead_code)]
const PS2_CMD_DISABLE_PORT1: u8 = 0xAD;
#[allow(dead_code)]
const PS2_CMD_ENABLE_PORT1: u8 = 0xAE;
#[allow(dead_code)]
const PS2_CMD_DISABLE_PORT2: u8 = 0xA7; 
#[allow(dead_code)]
const PS2_CMD_ENABLE_PORT2: u8 = 0xA8; 
#[allow(dead_code)]
const PS2_CMD_TEST_PORT1: u8 = 0xAB;
#[allow(dead_code)]
const PS2_CMD_TEST_PORT2: u8 = 0xA9; 
#[allow(dead_code)]
const PS2_CMD_TEST_CONTROLLER: u8 = 0xAA;
#[allow(dead_code)]
const PS2_CMD_RESET_DEVICE: u8 = 0xFF;


#[allow(dead_code)]
const KEYBOARD_CMD_ENABLE_SCANNING: u8 = 0xF4;





const SCANCODE_MAP_LOWERCASE: [char; 128] = [
    '\0', '\x1B', '1', '2', '3', '4', '5', '6', '7', '8', '9', '0', '\'', 'ì', '\x08', '\t',
    'q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p', 'è', '+', '\n', '\0', 'a', 's',
    'd', 'f', 'g', 'h', 'j', 'k', 'l', 'ò', 'à', '\\', '\0', 'ù', 'z', 'x', 'c', 'v',
    'b', 'n', 'm', ',', '.', '-', '\0', '\0', '\0', ' ', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '<', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
];

const SCANCODE_MAP_UPPERCASE: [char; 128] = [
    '\0', '\x1B', '!', '"', '£', '$', '%', '&', '/', '(', ')', '=', '?', '^', '\x08', '\t',
    'Q', 'W', 'E', 'R', 'T', 'Y', 'U', 'I', 'O', 'P', 'é', '*', '\n', '\0', 'A', 'S',
    'D', 'F', 'G', 'H', 'J', 'K', 'L', 'ç', '°', '|', '\0', '§', 'Z', 'X', 'C', 'V',
    'B', 'N', 'M', ';', ':', '_', '\0', '\0', '\0', ' ', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '>', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
];

// State to track shift key status
static mut SHIFT_ACTIVE: bool = false;
static mut E0_ACTIVE: bool = false;
static mut SUPER_ACTIVE: bool = false;
static mut ALT_ACTIVE: bool = false;

pub fn is_super_active() -> bool {
    unsafe { SUPER_ACTIVE }
}

const SCANCODE_MAP_ALT: [char; 128] = [
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '{', '[', ']', '}', '\0', '\0', '\0', '\0',
    '@', '\0', '€', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '[', ']', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '@', '#', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
];

fn wait_for_read() -> bool {
    let mut timeout = 100000;
    while (inb(STATUS_PORT) & 0x01) == 0 {
        timeout -= 1;
        if timeout == 0 {
            return false;
        }
    }
    true
}

fn wait_for_write() -> bool {
    let mut timeout = 100000;
    while (inb(STATUS_PORT) & 0x02) != 0 {
        timeout -= 1;
        if timeout == 0 {
            return false;
        }
    }
    true
}

#[allow(dead_code)]
pub fn init() {
    // Disable PS/2 Port 1 first to prevent interference
    if !wait_for_write() { return; }
    outb(COMMAND_PORT, PS2_CMD_DISABLE_PORT1);

    // Flush the Output Buffer (discard pending data)
    while (inb(STATUS_PORT) & 0x01) != 0 {
        inb(DATA_PORT);
    }

    // Read Controller Configuration Byte
    if !wait_for_write() { return; }
    outb(COMMAND_PORT, PS2_CMD_READ_CONFIG);
    if !wait_for_read() { return; }
    let mut config = inb(DATA_PORT);

    // Configure Controller:
    // Bit 0: Enable IRQ1 (Keyboard)
    // Bit 6: Enable Translation (Convert Set 2 to Set 1)
    config |= 0x01;
    config |= 0x40;

    // Write Controller Configuration Byte
    if !wait_for_write() { return; }
    outb(COMMAND_PORT, PS2_CMD_WRITE_CONFIG);
    if !wait_for_write() { return; }
    outb(DATA_PORT, config);

    // Enable PS/2 Port 1
    if !wait_for_write() { return; }
    outb(COMMAND_PORT, PS2_CMD_ENABLE_PORT1);

    // Reset Device (Optional, can be slow, skipping for speed unless needed)
    // Instead, just Enable Scanning
    if !wait_for_write() { return; }
    outb(DATA_PORT, KEYBOARD_CMD_ENABLE_SCANNING);

    // Wait for ACK (0xFA)
    if wait_for_read() {
        let _ack = inb(DATA_PORT);
    }
}

#[allow(dead_code)]
pub fn handle_scancode(scancode: u8) -> Option<(u32, bool)> {
    unsafe {
        if scancode == 0xE0 {
            E0_ACTIVE = true;
            return None;
        }

        let is_e0 = E0_ACTIVE;
        E0_ACTIVE = false;

        let is_release = (scancode & 0x80) != 0;
        let scancode_val = if is_release { scancode & 0x7F } else { scancode };
        let pressed = !is_release;

        match scancode_val {
            // Windows Key
            0x5B | 0x5C if is_e0 => {
                SUPER_ACTIVE = pressed;
                if pressed { crate::debugln!("Global Shortcut: Super Key Pressed"); }
                None
            },

            
            0x38 => {
                ALT_ACTIVE = pressed;
                Some((KEY_ALT, pressed))
            },

            
            0x2A | 0x36 => { 
                SHIFT_ACTIVE = pressed;
                Some((KEY_SHIFT, pressed))
            },

            
            0x1D => {
                Some((KEY_CTRL, pressed))
            },
            
            
            0x0E => Some((KEY_BACKSPACE, pressed)), 
            0x1C => Some((KEY_ENTER, pressed)), 
            0x39 => Some((' ' as u32, pressed)), 
            0x01 => Some(('\x1B' as u32, pressed)), 
            0x0F => Some(('\t' as u32, pressed)), 
            
            
            0x4B if is_e0 => Some((KEY_LEFT, pressed)),
            0x4D if is_e0 => Some((KEY_RIGHT, pressed)),
            0x48 if is_e0 => Some((KEY_UP, pressed)),
            0x50 if is_e0 => Some((KEY_DOWN, pressed)),

            
            0x56 => {
                if SHIFT_ACTIVE {
                    Some(('>' as u32, pressed))
                } else {
                    Some(('<' as u32, pressed))
                }
            },

            
            0x02..=0x0D | 
            0x10..=0x1B | 
            0x1E..=0x28 | 
            0x2B..=0x35 | 
            0x3A => {
                if scancode_val < 128 {
                    let c = if ALT_ACTIVE {
                        SCANCODE_MAP_ALT[scancode_val as usize]
                    } else if SHIFT_ACTIVE {
                        SCANCODE_MAP_UPPERCASE[scancode_val as usize]
                    } else {
                        SCANCODE_MAP_LOWERCASE[scancode_val as usize]
                    };
                    if c != '\0' { Some((c as u32, pressed)) } else { None }
                } else {
                    None
                }
            },
            _ => None,
        }
    }
}
