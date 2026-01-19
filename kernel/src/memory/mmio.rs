pub unsafe fn read_8(addr: *mut u8) -> u8 {
    core::ptr::read_volatile(addr)
}

pub unsafe fn write_8(addr: *mut u8, val: u8) {
    core::ptr::write_volatile(addr, val);
}

pub unsafe fn read_16(addr: *mut u8) -> u16 {
    core::ptr::read_volatile(addr as *mut u16)
}

pub unsafe fn write_16(addr: *mut u8, val: u16) {
    core::ptr::write_volatile(addr as *mut u16, val);
}

pub unsafe fn read_32(addr: *mut u8) -> u32 {
    core::ptr::read_volatile(addr as *mut u32)
}

pub unsafe fn write_32(addr: *mut u8, val: u32) {
    core::ptr::write_volatile(addr as *mut u32, val);
}

pub unsafe fn write_64(addr: *mut u8, val: u64) {
    core::ptr::write_volatile(addr as *mut u64, val);
}
