use crate::memory::paging::HHDM_OFFSET;
use crate::memory::vmm;
use crate::memory::address::PhysAddr;
use crate::memory::paging;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU32, Ordering};
use crate::arch::x86_64::acpi::{Madt, AcpiHeader};
use crate::debugln;

pub static mut LOCAL_APIC_ADDR: u64 = 0;
pub static mut IO_APIC_ADDR: u64 = 0;

// LAPIC timer register offsets (relative to LAPIC base)
const LAPIC_LVT_TIMER: u64        = 0x320;
const LAPIC_TIMER_INITIAL_COUNT: u64 = 0x380;
const LAPIC_TIMER_CURRENT_COUNT: u64 = 0x390;
const LAPIC_TIMER_DIVIDE_CONFIG: u64 = 0x3E0;

/// Calibrated LAPIC timer ticks per 10ms. Written once by BSP, read by all APs.
pub static LAPIC_TIMER_COUNT: AtomicU32 = AtomicU32::new(0);

/// Calibrate the LAPIC timer against the HPET.
/// Must be called on the BSP after both APIC and HPET have been initialized.
pub fn calibrate_lapic_timer() {
    unsafe {
        let lapic_base = *(&raw const LOCAL_APIC_ADDR);
        if lapic_base == 0 {
            return;
        }

        // Divide-by-16 (bits[3:1,0] = 0b0011 → divisor 16)
        write_volatile((lapic_base + LAPIC_TIMER_DIVIDE_CONFIG) as *mut u32, 0x3);
        // One-shot mode, vector 0xFF (masked); just counting
        write_volatile((lapic_base + LAPIC_LVT_TIMER) as *mut u32, 0x000100FF);
        write_volatile((lapic_base + LAPIC_TIMER_INITIAL_COUNT) as *mut u32, 0xFFFF_FFFF);

        let period_fs = crate::drivers::peripherals::hpet::HPET_PERIOD_FS.load(Ordering::Acquire);
        let ticks_10ms = if period_fs > 0 {
            // Busy-wait for exactly 10ms using HPET main counter
            let hpet_ticks = 10_000_000_000_000u64 / period_fs; // HPET ticks in 10ms
            let t0 = crate::drivers::peripherals::hpet::read_counter();
            loop {
                if crate::drivers::peripherals::hpet::read_counter().wrapping_sub(t0) >= hpet_ticks {
                    break;
                }
                core::hint::spin_loop();
            }
            let lapic_remaining = read_volatile((lapic_base + LAPIC_TIMER_CURRENT_COUNT) as *const u32);
            0xFFFF_FFFFu32.wrapping_sub(lapic_remaining)
        } else {
            // No HPET: conservative default (≈5ms at 3 GHz / divide-by-16)
            1_000_000u32
        };

        debugln!("APIC: LAPIC timer calibrated: {} ticks/10ms", ticks_10ms);
        LAPIC_TIMER_COUNT.store(ticks_10ms, Ordering::Release);

        // Disarm one-shot timer used for calibration
        write_volatile((lapic_base + LAPIC_TIMER_INITIAL_COUNT) as *mut u32, 0);
    }
}

/// Program the current CPU's LAPIC timer to fire periodically at `vector` every 10ms.
/// Must be called after `enable_local_apic()` and after `calibrate_lapic_timer()`.
pub fn init_lapic_timer(vector: u8) {
    let count = LAPIC_TIMER_COUNT.load(Ordering::Acquire);
    if count == 0 {
        return;
    }
    unsafe {
        let lapic_base = *(&raw const LOCAL_APIC_ADDR);
        if lapic_base == 0 {
            return;
        }
        // Divide-by-16
        write_volatile((lapic_base + LAPIC_TIMER_DIVIDE_CONFIG) as *mut u32, 0x3);
        // Periodic mode (bit 17 set), vector
        write_volatile((lapic_base + LAPIC_LVT_TIMER) as *mut u32, (1u32 << 17) | (vector as u32));
        // Start counting
        write_volatile((lapic_base + LAPIC_TIMER_INITIAL_COUNT) as *mut u32, count);
    }
}

pub fn init(madt_ptr: u64) {
    let madt = unsafe { &*(madt_ptr as *const Madt) };
    let lapic_phys = madt.lapic_addr as u64;
    
    unsafe {
        LOCAL_APIC_ADDR = vmm::map_mmio(lapic_phys, 4096);
        debugln!("APIC: Local APIC at {:#x} (mapped to {:#x})", lapic_phys, *(&raw const LOCAL_APIC_ADDR));
    }

    // Enable Local APIC
    unsafe {
        let lapic_base = *(&raw const LOCAL_APIC_ADDR);
        let spurious_vector_reg = (lapic_base + 0xF0) as *mut u32;
        write_volatile(spurious_vector_reg, read_volatile(spurious_vector_reg) | 0x100 | 0xFF);
    }

    // Parse MADT entries
    let mut offset = core::mem::size_of::<Madt>();
    while offset < madt.header.length as usize {
        let entry_ptr = (madt_ptr + offset as u64) as *const u8;
        let entry_type = unsafe { *entry_ptr };
        let entry_len = unsafe { *entry_ptr.add(1) };

        match entry_type {
            1 => { // I/O APIC
                let ioapic_phys = unsafe { core::ptr::read_unaligned(entry_ptr.add(4) as *const u32) } as u64;
                unsafe {
                    IO_APIC_ADDR = vmm::map_mmio(ioapic_phys, 4096);
                    debugln!("APIC: I/O APIC at {:#x} (mapped to {:#x})", ioapic_phys, *(&raw const IO_APIC_ADDR));
                }
            }
            2 => { // Interrupt Source Override
                let bus_source = unsafe { *entry_ptr.add(2) };
                let irq_source = unsafe { *entry_ptr.add(3) };
                let global_system_interrupt = unsafe { core::ptr::read_unaligned(entry_ptr.add(4) as *const u32) };
                debugln!("APIC: IRQ Override: Bus {} IRQ {} -> GSI {}", bus_source, irq_source, global_system_interrupt);
            }
            _ => {}
        }
        offset += entry_len as usize;
    }
}

pub fn eoi() {
    unsafe {
        let lapic_base = *(&raw const LOCAL_APIC_ADDR);
        if lapic_base != 0 {
            let eoi_reg = (lapic_base + 0x0B0) as *mut u32;
            write_volatile(eoi_reg, 0);
        }
    }
}

pub fn ioapic_write(reg: u32, value: u32) {
    unsafe {
        let ioapic_base = *(&raw const IO_APIC_ADDR);
        let ioregsel = ioapic_base as *mut u32;
        let iowin = (ioapic_base + 0x10) as *mut u32;
        write_volatile(ioregsel, reg);
        write_volatile(iowin, value);
    }
}

pub fn enable_local_apic() {
    unsafe {
        let lapic_base = *(&raw const LOCAL_APIC_ADDR);
        let spurious_vector_reg = (lapic_base + 0xF0) as *mut u32;
        write_volatile(spurious_vector_reg, read_volatile(spurious_vector_reg) | 0x100 | 0xFF);
    }
}

pub fn get_id() -> u8 {
    unsafe {
        let lapic_base = *(&raw const LOCAL_APIC_ADDR);
        if lapic_base == 0 { return 0; }
        let id_reg = (lapic_base + 0x20) as *const u32;
        (read_volatile(id_reg) >> 24) as u8
    }
}

pub fn send_ipi(lapic_id: u8, command: u32) {
    unsafe {
        let lapic_base = *(&raw const LOCAL_APIC_ADDR);
        let icr_high = (lapic_base + 0x310) as *mut u32;
        let icr_low = (lapic_base + 0x300) as *mut u32;
        
        while (read_volatile(icr_low) & (1 << 12)) != 0 {}
        write_volatile(icr_high, (lapic_id as u32) << 24);
        write_volatile(icr_low, command);
    }
}

pub fn set_irq(irq: u8, vector: u8) {
    let low_index = 0x10 + (irq as u32) * 2;
    let high_index = low_index + 1;
    ioapic_write(low_index, vector as u32);
    ioapic_write(high_index, 0);
}

/// Send a fixed IPI to a specific LAPIC ID (e.g., cross-CPU wakeup).
/// `vector` is the IDT vector the remote CPU will receive.
pub fn send_ipi_to(lapic_id: u8, vector: u8) {
    // Delivery mode: Fixed (000), level: Assert, destination: Physical
    send_ipi(lapic_id, 0x00004000 | (vector as u32));
}

/// Send a broadcast IPI to all CPUs except self.
/// Used for TLB shootdown: all CPUs flush their TLBs after a PTE change.
pub fn broadcast_ipi_except_self(vector: u8) {
    unsafe {
        let lapic_base = *(&raw const LOCAL_APIC_ADDR);
        if lapic_base == 0 { return; }
        let icr_high = (lapic_base + 0x310) as *mut u32;
        let icr_low  = (lapic_base + 0x300) as *mut u32;
        // Wait for any previous IPI to be dispatched
        while (read_volatile(icr_low) & (1 << 12)) != 0 {}
        write_volatile(icr_high, 0); // ignored for broadcast
        // Destination shorthand = 11b (All Excluding Self), delivery = Fixed, level = Assert
        write_volatile(icr_low, 0x000C4000 | (vector as u32));
    }
}
