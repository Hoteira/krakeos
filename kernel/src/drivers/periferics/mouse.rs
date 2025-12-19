use crate::drivers::port::{inb, outb};
use crate::println;

pub const MOUSE_INT: u8 = 44;
pub static mut MOUSE_PACKET: [u8; 4] = [0; 4];
pub static mut MOUSE_IDX: usize = 0;
pub static mut MOUSE_PACKET_SIZE: usize = 3; 

const CMD_ENABLE_AUX: u8 = 0xA8;
const CMD_GET_COMPAQ_STATUS: u8 = 0x20;
const CMD_SET_COMPAQ_STATUS: u8 = 0x60;
const CMD_WRITE_AUX: u8 = 0xD4;

const MOUSE_RESET: u8 = 0xFF;
const MOUSE_SET_DEFAULTS: u8 = 0xF6;
const MOUSE_ENABLE_STREAMING: u8 = 0xF4;
const MOUSE_GET_ID: u8 = 0xF2;
const MOUSE_SET_SAMPLE_RATE: u8 = 0xF3;

pub fn init_mouse() {
    println!("Mouse: Initializing PS/2 Mouse...");

    wait_write();
    outb(0x64, CMD_ENABLE_AUX);

    wait_write();
    outb(0x64, CMD_GET_COMPAQ_STATUS);
    wait_read();
    let mut status = inb(0x60);
    status |= 2; // Enable IRQ12
    status &= !0x20; // Clear "Disable Mouse" bit
    wait_write();
    outb(0x64, CMD_SET_COMPAQ_STATUS);
    wait_write();
    outb(0x60, status);

    mouse_write(MOUSE_RESET);
    let _r1 = mouse_read();
    let _r2 = mouse_read(); 
    
    mouse_write(MOUSE_SET_DEFAULTS);
    let _ack = mouse_read();

    mouse_write(MOUSE_SET_SAMPLE_RATE); let _ = mouse_read(); mouse_write(200); let _ = mouse_read();
    mouse_write(MOUSE_SET_SAMPLE_RATE); let _ = mouse_read(); mouse_write(100); let _ = mouse_read();
    mouse_write(MOUSE_SET_SAMPLE_RATE); let _ = mouse_read(); mouse_write(80);  let _ = mouse_read();

    mouse_write(MOUSE_GET_ID);
    let _ack = mouse_read();
    let id = mouse_read();
    
    unsafe {
        MOUSE_PACKET_SIZE = 4;
        println!("Mouse: ID: {}. Forcing 4-byte packet mode (Scroll Enabled).", id);
    }

    mouse_write(MOUSE_ENABLE_STREAMING);
    let _ack = mouse_read();

    println!("Mouse: Initialized.");
}

fn mouse_write(value: u8) {
    wait_write();
    outb(0x64, CMD_WRITE_AUX);
    wait_write();
    outb(0x60, value);
}

fn mouse_read() -> u8 {
    wait_read();
    inb(0x60)
}

fn wait_write() {
    let mut timeout = 100_000;
    while timeout > 0 {
        if (inb(0x64) & 2) == 0 { return; }
        timeout -= 1;
    }
}

fn wait_read() {
    let mut timeout = 100_000;
    while timeout > 0 {
        if (inb(0x64) & 1) == 1 { return; }
        timeout -= 1;
    }
}

const O: u32 = 0x0000_0000;
const B: u32 = 0xFF00_0000;
const T: u32 = 0xFFFF_FFFF;

pub const CURSOR_BUFFER: [u32; 1024] = [
    B, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, T, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, T, T, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, T, T, T, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, T, T, T, T, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, T, T, T, T, T, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, T, T, T, T, T, T, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, T, T, T, T, T, T, T, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, T, T, T, T, T, T, T, T, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, T, T, T, T, T, T, T, T, T, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, T, T, T, T, T, T, T, T, T, T, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, T, T, T, T, T, T, T, T, T, T, T, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, T, T, T, T, T, T, T, T, T, T, T, T, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, T, T, T, T, T, T, T, T, T, T, T, T, T, T, B, B, B, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, T, T, T, T, T, B, B, B, B, B, B, B, B, B, B, B, B, B, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, T, T, T, T, B, B, B, B, B, B, B, B, B, B, B, B, B, B, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, T, T, T, B, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, T, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, T, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
    B, B, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O, O,
];

pub const CURSOR_WIDTH: usize = 32;
pub const CURSOR_HEIGHT: usize = 32;