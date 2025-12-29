use core::arch::asm;
use core::mem::size_of;
use core::ptr::addr_of;

pub static mut GDT: Gdt = {
    let limit = (0xFFFF << 0) | (0xF << 48);
    let base = (0x0000 << 16) | (0x00 << 56);
    let zero = Entry { entry: 0 };
    let kernel_code_32 = Entry { entry: limit | base | (0x9A << 40) | (0xC << 52) };
    let kernel_data = Entry { entry: limit | base | (0x92 << 40) | (0xC << 52) };
    let user_code_32 = Entry { entry: limit | base | (0xFA << 40) | (0xC << 52) };
    let user_data = Entry { entry: limit | base | (0xF2 << 40) | (0xC << 52) };
    let kernel_code_64 = Entry { entry: limit | base | (0x9A << 40) | (0xA << 52) };
    let user_code_64 = Entry { entry: limit | base | (0xFA << 40) | (0xA << 52) };
    let tss_low = Entry { entry: 0 };
    let tss_high = Entry { entry: 0 };

    Gdt {
        entries: [
            zero,
            kernel_code_32,
            kernel_data,
            user_code_32,
            user_data,
            kernel_code_64,
            user_code_64,
            tss_low,
            tss_high,
        ],
    }
};

#[repr(C, packed)]
pub struct Gdt {
    entries: [Entry; 9],
}

#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
pub struct Entry {
    entry: u64,
}

#[repr(C, packed)]
pub struct Descriptor {
    size: u16,
    offset: *const Gdt,
}

impl Gdt {
    pub fn load(&self) {
        let gdt_descriptor = Descriptor {
            size: (9 * size_of::<Entry>() - 1) as u16,
            offset: self,
        };
        unsafe {
            asm!("lgdt [{0:e}]", in(reg) &gdt_descriptor);
        }
    }

    pub fn load_tss(&self) {
        unsafe {
            asm!("ltr {0:x}", in(reg) 0x38u16, options(nostack, preserves_flags));
        }
    }

    pub fn write_tss(&mut self) {
        let tss_limit = (size_of::<crate::tss::TaskStateSegment>() - 1) as u64;
        let tss_base = addr_of!(crate::tss::BASE_TSS) as u64;

        let low_desc = (tss_limit & 0xFFFF)
            | ((tss_base & 0xFFFF) << 16)
            | (((tss_base >> 16) & 0xFF) << 32)
            | (0x89 << 40) 
            | (((tss_limit >> 16) & 0xF) << 48)
            | (((tss_base >> 24) & 0xFF) << 56);

        let high_desc = tss_base >> 32;

        self.entries[7] = Entry { entry: low_desc };
        self.entries[8] = Entry { entry: high_desc };
    }
}