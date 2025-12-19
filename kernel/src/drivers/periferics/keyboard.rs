// kernel/src/drivers/periferics/keyboard.rs

use crate::drivers::port::{inb, outb};
use alloc::collections::VecDeque;
use std::sync::Mutex;

#[allow(dead_code)]
pub static KEYBOARD_BUFFER: Mutex<VecDeque<u32>> = Mutex::new(VecDeque::new());

// Keycodes for special keys
pub const KEY_LEFT: u32 = 0x110001;
pub const KEY_RIGHT: u32 = 0x110002;
pub const KEY_UP: u32 = 0x110003;
pub const KEY_DOWN: u32 = 0x110004;
pub const KEY_BACKSPACE: u32 = 0x08;
pub const KEY_ENTER: u32 = 0x0A;

// PS/2 Controller Ports
#[allow(dead_code)]
const DATA_PORT: u16 = 0x60;
#[allow(dead_code)]
const STATUS_PORT: u16 = 0x64;
#[allow(dead_code)]
const COMMAND_PORT: u16 = 0x64;

// PS/2 Controller Commands
#[allow(dead_code)]
const PS2_CMD_READ_CONFIG: u8 = 0x20;
#[allow(dead_code)]
const PS2_CMD_WRITE_CONFIG: u8 = 0x60;
#[allow(dead_code)]
const PS2_CMD_DISABLE_PORT1: u8 = 0xAD;
#[allow(dead_code)]
const PS2_CMD_ENABLE_PORT1: u8 = 0xAE;
#[allow(dead_code)]
const PS2_CMD_DISABLE_PORT2: u8 = 0xA7; // Not always present
#[allow(dead_code)]
const PS2_CMD_ENABLE_PORT2: u8 = 0xA8; // Not always present
#[allow(dead_code)]
const PS2_CMD_TEST_PORT1: u8 = 0xAB;
#[allow(dead_code)]
const PS2_CMD_TEST_PORT2: u8 = 0xA9; // Not always present
#[allow(dead_code)]
const PS2_CMD_TEST_CONTROLLER: u8 = 0xAA;
#[allow(dead_code)]
const PS2_CMD_RESET_DEVICE: u8 = 0xFF;

// Keyboard device commands
#[allow(dead_code)]
const KEYBOARD_CMD_ENABLE_SCANNING: u8 = 0xF4;

// Scancode set 1 (Standard XT/AT)
// This is a very simplified mapping and only covers basic keys.
// More complete mappings would require state tracking (Shift, Ctrl, Alt).
// For now, only printable ASCII characters are handled.
const SCANCODE_MAP_LOWERCASE: [char; 128] = [
    '\0', '\x1B', '1', '2', '3', '4', '5', '6', '7', '8', '9', '0', '-', '=', '\x08', '\t',
    'q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p', '[', ']', '\n', '\0', 'a', 's',
    'd', 'f', 'g', 'h', 'j', 'k', 'l', ';', '\'', '`', '\0', '\\', 'z', 'x', 'c', 'v',
    'b', 'n', 'm', ',', '.', '/', '\0', '\0', '\0', ' ', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
];

const SCANCODE_MAP_UPPERCASE: [char; 128] = [
    '\0', '\x1B', '!', '@', '#', '$', '%', '^', '&', '*', '(', ')', '_', '+', '\x08', '\t',
    'Q', 'W', 'E', 'R', 'T', 'Y', 'U', 'I', 'O', 'P', '{', '}', '\n', '\0', 'A', 'S',
    'D', 'F', 'G', 'H', 'J', 'K', 'L', ':', '"', '~', '\0', '|', 'Z', 'X', 'C', 'V',
    'B', 'N', 'M', '<', '>', '?', '\0', '\0', '\0', ' ', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
];

// State to track shift key status
static mut SHIFT_ACTIVE: bool = false;
static mut E0_ACTIVE: bool = false;

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
pub fn handle_scancode(scancode: u8) -> Option<u32> {
    unsafe {
        if scancode == 0xE0 {
            E0_ACTIVE = true;
            return None;
        }

        let is_e0 = E0_ACTIVE;
        E0_ACTIVE = false;

        match scancode {
            // Shift keys (make codes)
            0x2A | 0x36 => { // Left Shift, Right Shift
                SHIFT_ACTIVE = true;
                None
            },
            // Shift keys (break codes)
            0xAA | 0xB6 => { // Left Shift, Right Shift
                SHIFT_ACTIVE = false;
                None
            },
            // Special keys (make codes)
            0x0E => Some(KEY_BACKSPACE), // Backspace
            0x1C => Some(KEY_ENTER), // Enter
            0x39 => Some(' ' as u32), // Space
            0x01 => Some('\x1B' as u32), // ESC (Escape)
            0x0F => Some('\t' as u32), // Tab
            
            // Arrow Keys (Extended)
            0x4B if is_e0 => Some(KEY_LEFT),
            0x4D if is_e0 => Some(KEY_RIGHT),
            0x48 if is_e0 => Some(KEY_UP),
            0x50 if is_e0 => Some(KEY_DOWN),

            // Regular keys (make codes)
            0x02..=0x0D | // 1-0, -, =
            0x10..=0x19 | // Q-P, [, ]
            0x1E..=0x26 | // A-L, ;, '
            0x2C..=0x35 | // Z-M, ,, ., /
            0x3A => {
                if scancode < 128 {
                    if SHIFT_ACTIVE {
                        let c = SCANCODE_MAP_UPPERCASE[scancode as usize];
                        if c != '\0' { Some(c as u32) } else { None }
                    } else {
                        let c = SCANCODE_MAP_LOWERCASE[scancode as usize];
                        if c != '\0' { Some(c as u32) } else { None }
                    }
                } else {
                    None
                }
            },
            _ => None,
        }
    }
}
