use crate::drivers::port::Port;

const CMOS_ADDR: u16 = 0x70;
const CMOS_DATA: u16 = 0x71;

pub fn read_rtc(reg: u8) -> u8 {
    unsafe {
        Port::new(CMOS_ADDR).outb(reg);
        Port::new(CMOS_DATA).inb()
    }
}

pub fn get_time() -> (u8, u8, u8) {
    let mut second = read_rtc(0x00);
    let mut minute = read_rtc(0x02);
    let mut hour = read_rtc(0x04);
    let register_b = read_rtc(0x0B);


    if (register_b & 0x04) == 0 {
        second = (second & 0x0F) + ((second / 16) * 10);
        minute = (minute & 0x0F) + ((minute / 16) * 10);
        hour = (hour & 0x0F) + ((hour / 16) * 10) | (hour & 0x80);
    }


    if (register_b & 0x02) == 0 && (hour & 0x80) != 0 {
        hour = ((hour & 0x7F) + 12) % 24;
    }

    (hour, minute, second)
}

pub fn get_date() -> (u8, u8, u16) {
    let mut day = read_rtc(0x07);
    let mut month = read_rtc(0x08);
    let mut year = read_rtc(0x09);
    let register_b = read_rtc(0x0B);

    if (register_b & 0x04) == 0 {
        day = (day & 0x0F) + ((day / 16) * 10);
        month = (month & 0x0F) + ((month / 16) * 10);
        year = (year & 0x0F) + ((year / 16) * 10);
    }

    let full_year = 2000 + year as u16;
    (day, month, full_year)
}
