use crate::interrupts::{exceptions, task};
use core::arch::asm;
use core::mem::size_of;

pub static mut IDT: Idt = Idt {
    entries: unsafe { core::mem::zeroed() },
};

#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
pub struct Entry {
    pointer_low: u16,
    gdt_selector: u16,
    options: u16,
    pointer_middle: u16,
    pointer_high: u32,
    reserved: u32,
}

impl Entry {
    pub fn set(&mut self, offset: u64) {
        self.gdt_selector = 0x28;
        self.pointer_low = (offset & 0xFFFF) as u16;
        self.pointer_middle = ((offset >> 16) & 0xFFFF) as u16;
        self.pointer_high = (offset >> 32) as u32;
        self.options = 0x8E00;
        self.reserved = 0;
    }

    pub fn set_ist(&mut self, ist: u8) {
        self.options = (self.options & 0xFFF8) | (ist as u16 & 0x7);
    }

    #[allow(dead_code)]
    pub fn set_ring_3(&mut self, offset: u64) {
        self.gdt_selector = 0x28;
        self.pointer_low = (offset & 0xFFFF) as u16;
        self.pointer_middle = ((offset >> 16) & 0xFFFF) as u16;
        self.pointer_high = (offset >> 32) as u32;
        self.options = 0xEE00;
        self.reserved = 0;
    }
}

#[repr(C, packed)]
pub struct Idt {
    entries: [Entry; 256],
}

#[repr(C, packed)]
pub struct Descriptor {
    size: u16,
    offset: *const Idt,
}

impl Idt {
    pub fn init(&mut self) {
        for i in 0..self.entries.len() {
            self.add(i, exceptions::generic_handler as u64);
        }
    }

    pub fn add(&mut self, int: usize, handler: u64) {
        self.entries[int].set(handler);
    }

    #[allow(dead_code)]
    pub fn add_ring_3(&mut self, int: usize, handler: u64) {
        self.entries[int].set_ring_3(handler);
    }

    pub fn load(&self) {
        let idt_descriptor = Descriptor {
            size: (256 * size_of::<Entry>() - 1) as u16,
            offset: self,
        };

        unsafe {
            asm!("lidt [{}]", in(reg) &idt_descriptor, options(readonly, nostack, preserves_flags));
        }
    }

    pub fn processor_exceptions(&mut self) {
        self.entries[0].set(crate::interrupts::exceptions::div_error as u64);
        self.entries[5].set(crate::interrupts::exceptions::bounds as u64);
        self.entries[6].set(crate::interrupts::exceptions::invalid_opcode as u64);
        
        self.entries[8].set(crate::interrupts::exceptions::double_fault as u64);
        self.entries[8].set_ist(1); 

        self.entries[13].set(crate::interrupts::exceptions::general_protection_fault as u64);
        self.entries[13].set_ist(3);

        self.entries[14].set(crate::interrupts::exceptions::page_fault as u64);
        self.entries[14].set_ist(2);
    }

    pub fn hardware_interrupts(&mut self) {
        self.add_ring_3(exceptions::TIMER_INT as usize, task::timer_handler as u64);
        self.add(exceptions::KEYBOARD_INT as usize, exceptions::keyboard_handler as u64);
        self.add(exceptions::MOUSE_INT as usize, exceptions::mouse_handler as u64);
    }
}

#[allow(dead_code)]
pub fn interrupts() -> bool {
    let flags: u64;

    unsafe {
        asm!(
            "pushfq",
            "pop {}",
            out(reg) flags,
            options(nomem, nostack, preserves_flags)
        );
    }
    (flags & (1 << 9)) != 0
}