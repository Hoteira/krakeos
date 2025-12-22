use crate::drivers::port::Port;

const CMOS_ADDR: u16 = 0x70;
const CMOS_DATA: u16 = 0x71;

pub fn read_rtc(reg: u8) -> u8 {
    unsafe {
        Port::outb(CMOS_ADDR, reg);
        Port::inb(CMOS_DATA)
    }
}

pub fn get_time() -> (u8, u8, u8) {
    let mut second = read_rtc(0x00);
    let mut minute = read_rtc(0x02);
    let mut hour = read_rtc(0x04);
    let register_b = read_rtc(0x0B);

    // Convert BCD to binary if necessary
    if (register_b & 0x04) == 0 {
        second = (second & 0x0F) + ((second / 16) * 10);
        minute = (minute & 0x0F) + ((minute / 16) * 10);
        hour = (hour & 0x0F) + ((hour / 16) * 10) | (hour & 0x80);
    }
    
    // Convert 12h to 24h if necessary and remove AM/PM bit
    if (register_b & 0x02) == 0 && (hour & 0x80) != 0 {
        hour = ((hour & 0x7F) + 12) % 24;
    }

    (hour, minute, second)
}
