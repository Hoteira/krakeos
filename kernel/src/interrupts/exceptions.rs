use crate::debugln;
use crate::drivers::port::{inb, outb};
use crate::drivers::periferics::keyboard::KEYBOARD_BUFFER;
use crate::window_manager::input::MOUSE;

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
}

fn serial_println(s: &str) {
    serial_print(s);
    serial_print("\r\n");
}

fn print_hex(mut n: u64) {
    serial_print("0x");
    if n == 0 {
        serial_print("0");
        return;
    }
    let mut buffer = [0u8; 20];
    let mut i = 0;
    while n > 0 {
        let digit = (n % 16) as u8;
        buffer[i] = if digit < 10 { b'0' + digit } else { b'a' + (digit - 10) };
        n /= 16;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        let c = buffer[i];
        while (inb(0x3F8 + 5) & 0x20) == 0 {}
        outb(0x3F8, c);
    }
}

pub extern "x86-interrupt" fn div_error(_info: &mut StackFrame) {
    serial_println("EXCEPTION: DIV ERROR");
    loop {}
}

pub extern "x86-interrupt" fn bounds(_info: &mut StackFrame) {
    serial_println("EXCEPTION: BOUNDS");
    loop {}
}

pub extern "x86-interrupt" fn invalid_opcode(info: &mut StackFrame) {
    serial_println("\n=== INVALID OPCODE ===");
    serial_print("RIP: ");
    print_hex(info.instruction_pointer);
    serial_println("");

    if (info.code_segment & 3) == 3 {
        // User mode crash
        serial_println("User mode crash detected. Terminating task.");
        {
            let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            let current = tm.current_task;
            if current >= 0 {
                tm.tasks[current as usize].state = crate::interrupts::task::TaskState::Zombie;
            }
        }
        unsafe {
            core::arch::asm!("sti");
            loop { core::arch::asm!("hlt"); }
        }
    } else {
        loop {}
    }
}

pub extern "x86-interrupt" fn double_fault(_info: &mut StackFrame, _error_code: u64) -> ! {
    serial_println("EXCEPTION: DOUBLE FAULT");
    loop {}
}

pub extern "x86-interrupt" fn general_protection_fault(info: &mut StackFrame, error_code: u64) {
    serial_println("=== GENERAL PROTECTION FAULT===");
    serial_print("Error Code: ");
    print_hex(error_code);
    serial_print("\r\nRIP: ");
    print_hex(info.instruction_pointer);
    serial_println("");

    if (info.code_segment & 3) == 3 {
        serial_println("User mode GPF. Terminating task.");
        {
            let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            let current = tm.current_task;
            if current >= 0 {
                tm.tasks[current as usize].state = crate::interrupts::task::TaskState::Zombie;
            }
        }
        unsafe {
            core::arch::asm!("sti");
            loop { core::arch::asm!("hlt"); }
        }
    } else {
        loop {}
    }
}

pub extern "x86-interrupt" fn page_fault(info: &mut StackFrame, error_code: u64) {
    let cr2: u64;
    unsafe {
        core::arch::asm!("mov {}, cr2", out(reg) cr2);
    }

    serial_println("\n=== PAGE FAULT ===");
    serial_print("Address (CR2): ");
    print_hex(cr2);
    serial_print("\r\nError Code: ");
    print_hex(error_code);
    serial_print("\r\nRIP: ");
    print_hex(info.instruction_pointer);
    serial_println("");

    if (info.code_segment & 3) == 3 {
        serial_println("User mode Page Fault. Terminating task.");
        {
            let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            let current = tm.current_task;
            if current >= 0 {
                tm.tasks[current as usize].state = crate::interrupts::task::TaskState::Zombie;
            }
        }
        unsafe {
            core::arch::asm!("sti");
            loop { core::arch::asm!("hlt"); }
        }
    } else {
        loop {}
    }
}

pub extern "x86-interrupt" fn generic_handler(_info: &mut StackFrame) {
    serial_println("EXCEPTION: GENERIC");
}


#[allow(dead_code)]
pub const NET_INT: u8 = 43;

pub const TIMER_INT: u8 = 32;

pub const KEYBOARD_INT: u8 = 33;

pub extern "x86-interrupt" fn keyboard_handler(_info: &mut StackFrame) {
    let scancode: u8 = inb(0x60);

    if let Some(key) = crate::drivers::periferics::keyboard::handle_scancode(scancode) {
        if crate::drivers::periferics::keyboard::is_super_active() {
            // Global Shortcut Interception
            crate::debugln!("Global Shortcut: Super + {}", key);
            
            if key == 't' as u32 { 
                crate::debugln!("Spawning terminal...");
                match crate::interrupts::syscalls::spawn_process("@0xE0/sys/bin/term.elf", None) {
                    Ok(pid) => crate::debugln!("Terminal spawned with PID: {}", pid),
                    Err(e) => crate::debugln!("Failed to spawn terminal: {}", e),
                }
            }
            
            // Do NOT forward to userland
        } else {
            crate::debugln!("Key pressed: {}", key);
            
            // 1. CLI Buffer
            KEYBOARD_BUFFER.lock().push_back(key);

            // 2. GUI Event Dispatch
            unsafe {
                let active_window_id = crate::window_manager::input::CLICKED_WINDOW_ID;
                if active_window_id != 0 {
                    use crate::window_manager::events::{GLOBAL_EVENT_QUEUE, Event, KeyboardEvent};
                    
                    let event = Event::Keyboard(KeyboardEvent {
                        wid: active_window_id as u32,
                        key,
                        repeat: 1,
                    });

                    (*(&raw mut GLOBAL_EVENT_QUEUE)).add_event(event);
                }
            }
        }
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
    use crate::drivers::periferics::mouse::{MOUSE_PACKET, MOUSE_IDX, MOUSE_PACKET_SIZE};

    let data = inb(0x60);

    unsafe {
        if MOUSE_IDX == 0 && ((data & 0x08) == 0 || data == 0xFF) {
            (*(&raw const crate::interrupts::pic::PICS)).end_interrupt(MOUSE_INT);
            return;
        }

        MOUSE_PACKET[MOUSE_IDX] = data;
        MOUSE_IDX += 1;

        if MOUSE_IDX >= MOUSE_PACKET_SIZE {
            if MOUSE_PACKET_SIZE == 3 {
                MOUSE_PACKET[3] = 0;
            }
            
            (*(&raw mut MOUSE)).cursor(MOUSE_PACKET);
            MOUSE_IDX = 0;
        }

        (*(&raw const crate::interrupts::pic::PICS)).end_interrupt(MOUSE_INT);
    }
}


#[allow(dead_code)]
pub extern "x86-interrupt" fn timer_handler(_info: &mut StackFrame) {
    unsafe {
        (*(&raw const crate::interrupts::pic::PICS)).end_interrupt(TIMER_INT);
    }
}