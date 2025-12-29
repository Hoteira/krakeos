use core::arch::asm;
use crate::BOOT;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MemoryMapEntry {
    pub base: u64,
    pub length: u64,
    pub memory_type: u32,
    pub reserved_acpi: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MemoryMap {
    pub entries: [MemoryMapEntry; 32],
}


#[inline(never)]
pub fn get_mmap() {
    let mut cont_id: u32 = 0;
    let mut entries: u32 = 0;
    let mut _signature: u32 = 0;
    let mut _bytes: u32 = 0;

    loop {
        unsafe {
            asm!(
                "int 0x15",
                inout("eax") 0xE820 => _signature,
                inout("ecx") 24 => _bytes,
                inout("ebx") cont_id,

                in("edx") 0x534D4150,
                in("edi") &mut BOOT.mmap.entries[entries as usize] as *mut MemoryMapEntry,
            );
        }

        if entries >= 32 {
            break;
        } else {
            entries += 1;
        }

        if cont_id == 0 {
            break;
        }
    }
}

