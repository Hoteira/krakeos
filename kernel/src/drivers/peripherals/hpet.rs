use crate::memory::vmm;
use crate::arch::x86_64::acpi::HpetTable;
use core::ptr::{read_volatile, write_volatile};
use crate::debugln;

pub static mut HPET_ADDR: u64 = 0;

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
        debugln!("HPET: Period: {} fs", period_fs);

        // Enable main counter AND Legacy Replacement Route (bit 1)
        let config_reg = (hpet_base + 0x10) as *mut u64;
        write_volatile(config_reg, read_volatile(config_reg) | 3);

        // Setup Timer 0 for periodic interrupts at 1000Hz (1ms)
        let timer0_config_reg = (hpet_base + 0x100) as *mut u64;
        let timer0_comp_reg = (hpet_base + 0x108) as *mut u64;

        // Set to periodic, enable interrupts
        // Bit 2: Interrupt Enable, Bit 3: Periodic, Bit 6: Set Value
        let t0_config = (1 << 3) | (1 << 2) | (1 << 6);
        write_volatile(timer0_config_reg, t0_config);

        let increment = 1_000_000_000_000u64 / period_fs as u64;
        debugln!("HPET: Comparator increment for 1ms: {}", increment);
        
        write_volatile(timer0_comp_reg, increment);
        write_volatile(timer0_comp_reg, increment);
    }

    true
}
