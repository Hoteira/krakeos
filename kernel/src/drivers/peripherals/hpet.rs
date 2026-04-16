use crate::memory::vmm;
use crate::arch::x86_64::acpi::HpetTable;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU64, Ordering};
use crate::debugln;

pub static mut HPET_ADDR: u64 = 0;
/// HPET counter period in femtoseconds, set during init. Zero if HPET not available.
pub static HPET_PERIOD_FS: AtomicU64 = AtomicU64::new(0);

/// Read the HPET main counter. Returns 0 if HPET was not initialized.
pub fn read_counter() -> u64 {
    unsafe {
        let hpet_base = *(&raw const HPET_ADDR);
        if hpet_base == 0 {
            return 0;
        }
        read_volatile((hpet_base + 0xF0) as *const u64)
    }
}

pub fn init(hpet_ptr: u64) -> bool {
    let hpet_phys = unsafe { 
        let table = hpet_ptr as *const HpetTable;
        core::ptr::read_unaligned(core::ptr::addr_of!((*table).base_address.address))
    };

    unsafe {
        HPET_ADDR = vmm::map_mmio(hpet_phys, 4096);
        debugln!("HPET: HPET at {:#x} (mapped to {:#x})", hpet_phys, *(&raw const HPET_ADDR));
    }

    unsafe {
        let hpet_base = *(&raw const HPET_ADDR);
        let caps = read_volatile(hpet_base as *const u64);
        let period_fs = (caps >> 32) as u32;
        HPET_PERIOD_FS.store(period_fs as u64, Ordering::Release);
        debugln!("HPET: Period: {} fs", period_fs);

        // Enable main counter AND Legacy Replacement Route (bit 1)
        let config_reg = (hpet_base + 0x10) as *mut u64;
        write_volatile(config_reg, read_volatile(config_reg) | 3);

        // Setup Timer 0 for periodic interrupts at 100Hz (10ms)
        let timer0_config_reg = (hpet_base + 0x100) as *mut u64;
        let timer0_comp_reg = (hpet_base + 0x108) as *mut u64;

        // Set to periodic, enable interrupts
        // Bit 2: Interrupt Enable, Bit 3: Periodic, Bit 6: Set Value
        let t0_config = (1 << 3) | (1 << 2) | (1 << 6);
        write_volatile(timer0_config_reg, t0_config);

        let increment = 10_000_000_000_000u64 / period_fs as u64;
        debugln!("HPET: Comparator increment for 10ms: {}", increment);
        
        write_volatile(timer0_comp_reg, increment);
        write_volatile(timer0_comp_reg, increment);
    }

    true
}
