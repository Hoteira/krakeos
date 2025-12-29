use core::arch::asm;

pub fn write_byte(byte: u8) {
    match byte {
        b'\n' => outb(0x3F8, '\n' as u8),
        byte => {
            outb(0x3F8, byte);
        }
    }
}

pub fn debug(s: &str) {
    for byte in s.bytes() {
        match byte {
            0x20..=0x7e | b'\n' => write_byte(byte),
            _ => write_byte(0xfe),
        }
    }
}


pub fn outb(port: u16, value: u8) {
    unsafe {
        asm!(
        "out dx, al",
        in("dx") port,
        in("al") value,
        options(nomem, nostack, preserves_flags));
    }
}
