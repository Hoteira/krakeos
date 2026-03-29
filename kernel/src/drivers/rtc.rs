use crate::arch::x86_64::io::Port;

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

/// Returns seconds since Unix epoch (1970-01-01 00:00:00 UTC).
pub fn unix_timestamp() -> u32 {
    let (h, m, s) = get_time();
    let (d, mo, y) = get_date();

    // Days per month (non-leap)
    const DAYS: [u16; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    let y = y as u32;
    let mut days: u32 = 0;

    // Full years since 1970
    for yr in 1970..y {
        days += if yr % 4 == 0 && (yr % 100 != 0 || yr % 400 == 0) { 366 } else { 365 };
    }

    // Full months this year
    for i in 0..(mo as usize).saturating_sub(1) {
        days += DAYS[i] as u32;
        if i == 1 && y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            days += 1; // leap feb
        }
    }

    days += (d as u32).saturating_sub(1);

    days * 86400 + (h as u32) * 3600 + (m as u32) * 60 + (s as u32)
}
