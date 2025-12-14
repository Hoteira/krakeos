use core::ptr::{read_volatile, write_volatile};

pub unsafe fn write_common_u8(base: *mut u8, offset: usize, value: u8) {
    write_volatile(base.add(offset), value);
}

pub unsafe fn read_common_u8(base: *mut u8, offset: usize) -> u8 {
    read_volatile(base.add(offset))
}

pub unsafe fn write_common_u16(base: *mut u8, offset: usize, value: u16) {
    write_volatile(base.add(offset) as *mut u16, value);
}

pub unsafe fn read_common_u16(base: *mut u8, offset: usize) -> u16 {
    read_volatile(base.add(offset) as *mut u16)
}

pub unsafe fn write_common_u32(base: *mut u8, offset: usize, value: u32) {
    write_volatile(base.add(offset) as *mut u32, value);
}

pub unsafe fn read_common_u32(base: *mut u8, offset: usize) -> u32 {
    read_volatile(base.add(offset) as *mut u32)
}

pub unsafe fn write_common_u64(base: *mut u8, offset: usize, value: u64) {
    write_volatile(base.add(offset) as *mut u64, value);
}
