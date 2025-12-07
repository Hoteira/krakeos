use crate::drivers::port::{inb, outb};
use crate::drivers::periferics::keyboard::KEYBOARD_BUFFER; // Import KEYBOARD_BUFFER

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct StackFrame {
    pub instruction_pointer: u64,
    pub code_segment: u64,
    pub cpu_flags: u64,
    pub stack_pointer: u64,
    pub stack_segment: u64,
}

fn serial_print(s: &str) {
    for b in s.bytes() {
        while (inb(0x3F8 + 5) & 0x20) == 0 {}
        outb(0x3F8, b);
    }
    let newline = b"\r\n";
    for b in newline {
        while (inb(0x3F8 + 5) & 0x20) == 0 {}
        outb(0x3F8, *b);
    }
}

pub extern "x86-interrupt" fn div_error(_info: &mut StackFrame) {
    serial_print("EXCEPTION: DIV ERROR");
    loop {}
}

pub extern "x86-interrupt" fn bounds(_info: &mut StackFrame) {
    serial_print("EXCEPTION: BOUNDS");
    loop {}
}

pub extern "x86-interrupt" fn invalid_opcode(info: &mut StackFrame) {
    use core::fmt::Write;
    let mut writer = crate::debug::SerialDebug::new();
    let _ = write!(writer, "\n=== INVALID OPCODE ===\n");
    let _ = write!(writer, "RIP: {:#x}\n", info.instruction_pointer);
    let _ = write!(writer, "CS: {:#x}\n", info.code_segment);
    let _ = write!(writer, "RFLAGS: {:#x}\n", info.cpu_flags);
    let _ = write!(writer, "RSP: {:#x}\n", info.stack_pointer);
    let _ = write!(writer, "SS: {:#x}\n", info.stack_segment);
    loop {}
}

pub extern "x86-interrupt" fn double_fault(_info: &mut StackFrame, _error_code: u64) -> ! {
    serial_print("EXCEPTION: DOUBLE FAULT");
    loop {}
}

pub extern "x86-interrupt" fn general_protection_fault(info: &mut StackFrame, error_code: u64) {
    serial_print("=== GENERAL PROTECTION FAULT ===");

    // Decode error code
    let external = (error_code & 0x1) != 0;
    let table = (error_code >> 1) & 0x3; // 0=GDT, 1=IDT, 2=LDT, 3=IDT
    let index = (error_code >> 3) & 0x1FFF;

    use core::fmt::Write;
    let mut writer = crate::debug::SerialDebug::new();
    let _ = write!(writer, "Error Code: {:#x}\n", error_code);
    let _ = write!(writer, "  External: {}\n", external);
    let _ = write!(writer, "  Table: {} (0=GDT, 1=IDT, 2/3=LDT/IDT)\n", table);
    let _ = write!(writer, "  Index: {:#x}\n", index);
    let _ = write!(writer, "RIP: {:#x}\n", info.instruction_pointer);
    let _ = write!(writer, "CS: {:#x}\n", info.code_segment);
    let _ = write!(writer, "RFLAGS: {:#x}\n", info.cpu_flags);
    let _ = write!(writer, "RSP: {:#x}\n", info.stack_pointer);
    let _ = write!(writer, "SS: {:#x}\n", info.stack_segment);

    loop {}
}

pub extern "x86-interrupt" fn page_fault(info: &mut StackFrame, error_code: u64) {
    let cr2: u64;
    unsafe {
        core::arch::asm!("mov {}, cr2", out(reg) cr2);
    }

    use core::fmt::Write;
    let mut writer = crate::debug::SerialDebug::new();
    let _ = write!(writer, "\n=== PAGE FAULT ===\n");
    let _ = write!(writer, "Address (CR2): {:#x}\n", cr2);
    let _ = write!(writer, "Error Code: {:#x}\n", error_code);
    let _ = write!(writer, "RIP: {:#x}\n", info.instruction_pointer);
    let _ = write!(writer, "CS: {:#x}\n", info.code_segment);
    let _ = write!(writer, "RFLAGS: {:#x}\n", info.cpu_flags);
    let _ = write!(writer, "RSP: {:#x}\n", info.stack_pointer);
    let _ = write!(writer, "SS: {:#x}\n", info.stack_segment);
    
    // Analyze Error Code
    let present = (error_code & 1) != 0;
    let write = (error_code & 2) != 0;
    let user = (error_code & 4) != 0;
    let reserved = (error_code & 8) != 0;
    let instruction = (error_code & 16) != 0;

    let _ = write!(writer, "Flags: P:{} W:{} U:{} R:{} I:{}\n", present, write, user, reserved, instruction);

    loop {}
}

pub extern "x86-interrupt" fn generic_handler(_info: &mut StackFrame) {
    serial_print("EXCEPTION: GENERIC");
    loop {}
}

/* SPECIFIC STUFF */

#[allow(dead_code)]
pub const NET_INT: u8 = 43;

pub const TIMER_INT: u8 = 32;

pub const KEYBOARD_INT: u8 = 33;

pub extern "x86-interrupt" fn keyboard_handler(_info: &mut StackFrame) {

    let scancode: u8 = inb(0x60);

    if let Some(character) = crate::drivers::periferics::keyboard::handle_scancode(scancode) {
        KEYBOARD_BUFFER.lock().push_back(character); // Push to buffer instead of printing
    }

    unsafe {
        (*(&raw const crate::interrupts::pic::PICS)).end_interrupt(KEYBOARD_INT);
    }

}

pub const MOUSE_INT: u8 = 44;
#[allow(dead_code)]
pub static mut MOUSE_PACKET: [u8; 4] = [0; 4];
#[allow(dead_code)]
pub static mut MOUSE_IDX: usize = 0;

pub extern "x86-interrupt" fn mouse_handler(_info: &mut StackFrame) {

    let _data = inb(0x60);

    unsafe {
        (*(&raw const crate::interrupts::pic::PICS)).end_interrupt(MOUSE_INT);
    }
}


#[allow(dead_code)]
pub extern "x86-interrupt" fn timer_handler(_info: &mut StackFrame) {
    unsafe {
        (*(&raw const crate::interrupts::pic::PICS)).end_interrupt(TIMER_INT);
    }
}
