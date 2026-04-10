use crate::memory::paging::HHDM_OFFSET;
use crate::memory::vmm;
use crate::memory::address::PhysAddr;
use crate::memory::paging;
use core::ptr::{read_volatile, write_volatile};
use crate::arch::x86_64::acpi::{Madt, AcpiHeader};
use crate::debugln;

pub static mut LOCAL_APIC_ADDR: u64 = 0;
pub static mut IO_APIC_ADDR: u64 = 0;

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
