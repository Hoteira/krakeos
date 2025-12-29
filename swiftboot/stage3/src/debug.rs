use core::arch::asm;

pub fn print_hex(mut num: u64) {
    debug("0x");
    if num == 0 {
        debug("0");
        return;
    }
    
    let mut buffer = [0u8; 16]; 
    let mut idx = 0;
    
    while num > 0 {
        let digit = (num % 16) as u8;
        buffer[idx] = if digit < 10 { b'0' + digit } else { b'a' + (digit - 10) };
        num /= 16;
        idx += 1;
    }
    
    while idx > 0 {
        idx -= 1;
        write_byte(buffer[idx]);
    }
}

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