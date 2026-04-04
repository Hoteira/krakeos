use crate::boot::BOOT_INFO;
use crate::memory::paging::HHDM_OFFSET;
use core::mem::size_of;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct AcpiHeader {
    pub signature: [u8; 4],
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_revision: u32,
    pub creator_id: u32,
    pub creator_revision: u32,
}

#[repr(C, packed)]
pub struct Madt {
    pub header: AcpiHeader,
    pub lapic_addr: u32,
    pub flags: u32,
    // entries follow
}

#[repr(C, packed)]
pub struct HpetTable {
    pub header: AcpiHeader,
    pub event_timer_block_id: u32,
    pub base_address: GenericAddressStructure,
    pub hpet_number: u8,
    pub main_counter_minimum_clock_tick_in_periodic_mode: u16,
    pub page_protection_and_oem_attribute: u8,
}

#[repr(C, packed)]
pub struct GenericAddressStructure {
    pub address_space_id: u8,
    pub register_bit_width: u8,
    pub register_bit_offset: u8,
    pub access_size: u8,
    pub address: u64,
}

pub fn find_table(signature: &[u8; 4]) -> Option<u64> {
    let rsdp = unsafe { &*(&raw const BOOT_INFO.rsdp) };
    let rsdt_phys = rsdp.rsdt_address as u64;
    if rsdt_phys == 0 { return None; }

    let rsdt = unsafe { &*((rsdt_phys + HHDM_OFFSET) as *const AcpiHeader) };
    let entries_count = (rsdt.length - size_of::<AcpiHeader>() as u32) / 4;
    let entries_ptr = (rsdt_phys + HHDM_OFFSET + size_of::<AcpiHeader>() as u64) as *const u32;

    for i in 0..entries_count {
        let table_phys = unsafe { core::ptr::read_unaligned(entries_ptr.add(i as usize)) } as u64;
        let table_header = unsafe { &*((table_phys + HHDM_OFFSET) as *const AcpiHeader) };
        if &table_header.signature == signature {
            return Some(table_phys + HHDM_OFFSET);
        }
    }
    None
}

pub fn get_madt() -> Option<u64> {
    find_table(b"APIC")
}

pub fn get_hpet() -> Option<u64> {
    find_table(b"HPET")
}
