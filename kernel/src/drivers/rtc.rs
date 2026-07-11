// QEMU virt machine: goldfish-rtc MMIO (see hw/riscv/virt.c memmap, VIRT_RTC)
pub const GOLDFISH_RTC_BASE: usize = 0x0010_1000;

pub fn get_time_ns() -> u64 {
    unsafe {
        let low = core::ptr::read_volatile((GOLDFISH_RTC_BASE + 0x00) as *const u32) as u64;
        let high = core::ptr::read_volatile((GOLDFISH_RTC_BASE + 0x04) as *const u32) as u64;
        (high << 32) | low
    }
}
