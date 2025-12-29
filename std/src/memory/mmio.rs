use core::ptr::{read_volatile, write_volatile};

pub fn write_64(addr:*mut u8, data: u64) {
    unsafe {
        write_volatile(addr as *mut u64, data);
    }
}

pub fn read_64(addr: *mut u8) -> u64 {
    unsafe { read_volatile(addr as *mut u64) }
}

pub fn write_32(addr:*mut u8, data: u32) {
    unsafe {
        write_volatile(addr as *mut u32, data);
    }
}

pub fn read_32(addr: *mut u8) -> u32 {
    unsafe { read_volatile(addr as *mut u32) }
}

pub fn write_16(addr:*mut u8, data: u16) {
    unsafe {
        write_volatile(addr as *mut u16, data);
    }
}

pub fn read_16(addr: *mut u8) -> u16 {
    unsafe { read_volatile(addr as *mut u16) }
}

pub fn write_8(addr: *mut u8, data: u8) {
    unsafe {
        write_volatile(addr as *mut u8, data);
    }
}

pub fn read_8(addr: *mut u8) -> u8 {
    unsafe { read_volatile(addr) }
}
