use crate::BOOT;
use core::arch::asm;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MemoryMapEntry {
    pub base: u64,
    pub length: u64,
    pub memory_type: u32,
    pub reserved_acpi: u32,
}

pub const MAX_ENTRIES: usize = 32;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MemoryMap {
    pub entries: [MemoryMapEntry; MAX_ENTRIES],
}


#[inline(never)]
pub fn get_mmap() {
    let mut cont_id: u32 = 0;
    let mut entries: usize = 0;
    let mut _signature: u32 = 0;
    let mut _bytes: u32 = 0;

    // Bound the index before each call so the BIOS is never handed a pointer
    // past the end of the fixed-size array (the previous code wrote one entry
    // out of bounds when the firmware reported more than MAX_ENTRIES regions).
    while entries < MAX_ENTRIES {
        unsafe {
            asm!(
            "int 0x15",
            inout("eax") 0xE820 => _signature,
            inout("ecx") 24 => _bytes,
            inout("ebx") cont_id,
            in("edx") 0x534D4150,
            in("edi") &mut BOOT.mmap.entries[entries] as *mut MemoryMapEntry,
            );
        }

        entries += 1;

        // ebx == 0 means the entry just read was the final one.
        if cont_id == 0 {
            break;
        }
    }
}

