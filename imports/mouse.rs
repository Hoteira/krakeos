use libk::port::{inb, outb};

pub const MOUSE_INT: u8 = 44;
pub static mut MOUSE_PACKET: [u8; 4] = [0; 4];
pub static mut MOUSE_IDX: usize = 0;


pub fn init_mouse() {
    // First enable the auxiliary device (the mouse)
    outb(0x64, 0xA8);
    wait();
    outb(0x64,0x20);
    wait_input();
    let mut status = inb(0x60);
    status |= 0b11;
    outb(0x64, 0x60);
    wait();
    outb(0x60, status);

    // Reset defaults
    mouse_write(0xF6);
    let _ack1 = mouse_read();

    // Magic sequence to enable IntelliMouse wheel support:
    mouse_write(0xF3); mouse_write(200); let _ = mouse_read();
    mouse_write(0xF3); mouse_write(100); let _ = mouse_read();
    mouse_write(0xF3); mouse_write(80);  let _ = mouse_read();

    // Now ask for device ID
    mouse_write(0xF2);
    wait_input();
    let id = inb(0x60);
    if id != 0x03 && id != 0x04 {
        // no wheel support
        // optional: log or panic
    }

    // Finally enable data reporting
    mouse_write(0xF4);
    let _ack2 = mouse_read();
}

fn mouse_write(value: u8) {
    wait();
    outb(0x64, 0xD4);
    wait();
    outb(0x60, value);
}

fn mouse_read() -> u8 {
    wait_input();
    let resp = inb(0x60);
    if resp != 0xFA {
        panic!("Expected ACK 0xFA but got {:#X}", resp);
    }
    resp
}

fn wait() {
    let mut time = 100_000;

    while time > 1 {
        if (inb(0x64) & 0b10) == 0 {
            return;
        }

        time -= 1;
    }
}

fn wait_input() {
    let mut time = 100_000;

    while time > 1 {
        if (inb(0x64) & 0b1) == 1 {
            return;
        }

        time -= 1;
    }
}