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

pub extern "x86-interrupt" fn div_error(info: &mut StackFrame) {
    serial_print("EXCEPTION: DIV ERROR");
    loop {}
}

pub extern "x86-interrupt" fn bounds(info: &mut StackFrame) {
    serial_print("EXCEPTION: BOUNDS");
    loop {}
}

pub extern "x86-interrupt" fn invalid_opcode(info: &mut StackFrame) {
    serial_print("EXCEPTION: INVALID OPCODE");
    loop {}
}

pub extern "x86-interrupt" fn double_fault(info: &mut StackFrame, _error_code: u64) -> ! {
    serial_print("EXCEPTION: DOUBLE FAULT");
    loop {}
}

pub extern "x86-interrupt" fn general_protection_fault(info: &mut StackFrame, _error_code: u64) {
    serial_print("EXCEPTION: GPF");
    loop {}
}

pub extern "x86-interrupt" fn page_fault(info: &mut StackFrame, _error_code: u64) {
    serial_print("EXCEPTION: PAGE FAULT");
    loop {}
}

pub extern "x86-interrupt" fn generic_handler(_info: &mut StackFrame) {
    serial_print("EXCEPTION: GENERIC");
    loop {}
}

/* SPECIFIC STUFF */

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
pub static mut MOUSE_PACKET: [u8; 4] = [0; 4];
pub static mut MOUSE_IDX: usize = 0;

pub extern "x86-interrupt" fn mouse_handler(_info: &mut StackFrame) {

    let _data = inb(0x60);

    unsafe {
        (*(&raw const crate::interrupts::pic::PICS)).end_interrupt(MOUSE_INT);
    }
}


pub extern "x86-interrupt" fn timer_handler(_info: &mut StackFrame) {
    unsafe {
        (*(&raw const crate::interrupts::pic::PICS)).end_interrupt(TIMER_INT);
    }
}
