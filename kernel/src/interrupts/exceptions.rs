use crate::drivers::periferics::keyboard::KEYBOARD_BUFFER;
use crate::drivers::port::{inb, outb};
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

fn print_hex(n: u64) {
    serial_print("0x");
    if n == 0 {
        serial_print("0");
        return;
    }


    let mut leading = true;
    for i in (0..16).rev() {
        let shift = i * 4;
        let nibble = (n >> shift) & 0xF;

        if nibble != 0 || !leading || i == 0 {
            leading = false;
            let c = if nibble < 10 { b'0' + nibble as u8 } else { b'a' + (nibble as u8 - 10) };
            while (inb(0x3F8 + 5) & 0x20) == 0 {}
            outb(0x3F8, c);
        }
    }
}

fn kill_current_task() {
    let mut pid_to_kill = -1;
    {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        if let Some(current) = tm.current_task_idx() {
            if let Some(thread) = tm.tasks[current].as_ref() {
                pid_to_kill = thread.process.as_ref().expect("Thread has no process").pid as i32;
            }
        }
    }

    if pid_to_kill != -1 {
        crate::interrupts::task::TASK_MANAGER.int_lock().kill_process(pid_to_kill as u64);

        unsafe {
            core::arch::asm!("sti");
            loop { core::arch::asm!("hlt"); }
        }
    } else {
        serial_println("Kernel Panic: Exception in Kernel Mode with no valid task.");
        unsafe {
            core::arch::asm!("cli");
            loop { core::arch::asm!("hlt"); }
        }
    }
}

pub extern "x86-interrupt" fn div_error(info: &mut StackFrame) {
    serial_println("EXCEPTION: DIV ERROR");
    if (info.code_segment & 3) == 3 {
        serial_println("User mode exception. Terminating task.");
        kill_current_task();
    } else {
        loop {}
    }
}

pub extern "x86-interrupt" fn bounds(info: &mut StackFrame) {
    serial_println("EXCEPTION: BOUNDS");
    if (info.code_segment & 3) == 3 {
        serial_println("User mode exception. Terminating task.");
        kill_current_task();
    } else {
        loop {}
    }
}

pub extern "x86-interrupt" fn invalid_opcode(info: &mut StackFrame) {
    serial_println("EXCEPTION: INVALID OPCODE");
    serial_print("RIP: ");
    print_hex(info.instruction_pointer);
    serial_print("\r\n");

    if (info.code_segment & 3) == 3 {
        serial_println("User mode exception. Terminating task.");
        kill_current_task();
    } else {
        loop {}
    }
}

pub extern "x86-interrupt" fn double_fault(_info: &mut StackFrame, _error_code: u64) -> ! {
    serial_println("EXCEPTION: DOUBLE FAULT");
    loop {}
}

pub extern "x86-interrupt" fn general_protection_fault(info: &mut StackFrame, error_code: u64) {
    serial_print("\r\n=== GENERAL PROTECTION FAULT ===\r\n");
    serial_print("Error Code: ");
    print_hex(error_code);
    serial_print("\r\nRIP: ");
    print_hex(info.instruction_pointer);
    serial_print("\r\nRSP: ");
    print_hex(info.stack_pointer);
    serial_print("\r\n");

    if (info.code_segment & 3) == 3 {
        serial_println("User mode GPF. Terminating task.");
        kill_current_task();
    } else {
        unsafe {
            core::arch::asm!("cli");
            loop { core::arch::asm!("hlt"); }
        }
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
        kill_current_task();
    } else {
        unsafe {
            core::arch::asm!("cli");
            loop { core::arch::asm!("hlt"); }
        }
    }
}

pub extern "x86-interrupt" fn generic_handler(_info: &mut StackFrame) {
    serial_println("EXCEPTION: GENERIC");
}

pub extern "x86-interrupt" fn device_not_available(info: &mut StackFrame) {
    serial_println("EXCEPTION: DEVICE NOT AVAILABLE (#NM)");
    if (info.code_segment & 3) == 3 {
        kill_current_task();
    } else {
        loop {}
    }
}

pub extern "x86-interrupt" fn fpu_error(info: &mut StackFrame) {
    serial_println("EXCEPTION: x87 FPU ERROR (#MF)");
    if (info.code_segment & 3) == 3 {
        kill_current_task();
    } else {
        loop {}
    }
}

pub extern "x86-interrupt" fn simd_error(info: &mut StackFrame) {
    serial_println("EXCEPTION: SIMD FP ERROR (#XM)");
    if (info.code_segment & 3) == 3 {
        kill_current_task();
    } else {
        loop {}
    }
}


#[allow(dead_code)]
pub const NET_INT: u8 = 43;

pub const TIMER_INT: u8 = 32;

pub const KEYBOARD_INT: u8 = 33;

pub extern "x86-interrupt" fn keyboard_handler(_info: &mut StackFrame) {
    let scancode: u8 = inb(0x60);

    if let Some((key, pressed)) = crate::drivers::periferics::keyboard::handle_scancode(scancode) {
        if crate::drivers::periferics::keyboard::is_super_active() {
            if pressed {
                crate::debugln!("Global Shortcut: Super + {}", key);

                if key == 'p' as u32 {
                    crate::memory::pmm::print_allocations();
                }

                if key == 't' as u32 {
                    crate::debugln!("Spawning terminal...");
                    match crate::interrupts::syscalls::spawn_process("@0xE0/sys/bin/term.elf", None, None) {
                        Ok(pid) => crate::debugln!("Terminal spawned with PID: {}", pid),
                        Err(e) => crate::debugln!("Failed to spawn terminal: {}", e),
                    }
                }

                if key == 'x' as u32 || key == 'X' as u32 {
                    unsafe {
                        let active_window_id = crate::window_manager::input::CLICKED_WINDOW_ID;
                        crate::debugln!("Global Shortcut: Win + X detected. Active window: {}", active_window_id);
                        if active_window_id != 0 {
                            let composer = &*(&raw const crate::window_manager::composer::COMPOSER);
                            let mut pid_to_kill = None;

                            for w in &composer.windows {
                                if w.id == active_window_id {
                                    pid_to_kill = Some(w.pid);
                                    break;
                                }
                            }

                            if let Some(pid) = pid_to_kill {
                                crate::debugln!("Global Shortcut: Killing Process {} associated with Window {}", pid, active_window_id);
                                crate::interrupts::task::TASK_MANAGER.int_lock().kill_process(pid);
                            } else {
                                crate::debugln!("Global Shortcut: No PID found for Window {}", active_window_id);
                            }
                        }
                    }
                }
            }
        } else {
            if pressed {
                KEYBOARD_BUFFER.lock().push_back(key);
            } else {}


            unsafe {
                let active_window_id = crate::window_manager::input::CLICKED_WINDOW_ID;
                let repeat = 1;
                if active_window_id != 0 {
                    let composer = &*(&raw const crate::window_manager::composer::COMPOSER);
                    let mut found = false;
                    for w in &composer.windows {
                        if w.id == active_window_id {
                            if w.event_handler != 0 {
                                found = true;
                            }
                            break;
                        }
                    }

                    if found {
                        use crate::window_manager::events::{Event, KeyboardEvent, GLOBAL_EVENT_QUEUE};

                        for _ in 0..repeat {
                            let event = Event::Keyboard(KeyboardEvent {
                                wid: active_window_id as u32,
                                key,
                                pressed,
                                repeat: 1,
                            });

                            GLOBAL_EVENT_QUEUE.int_lock().add_event(event);
                        }
                    }
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
    use crate::drivers::periferics::mouse::{MOUSE_IDX, MOUSE_PACKET, MOUSE_PACKET_SIZE};

    let data = inb(0x60);

    unsafe {
        if MOUSE_IDX == 0 && ((data & 0x08) == 0 || data == 0xFF) {
            (*(&raw const crate::interrupts::pic::PICS)).end_interrupt(MOUSE_INT);
            return;
        }

        if MOUSE_IDX < (*(&raw const MOUSE_PACKET)).len() {
            MOUSE_PACKET[MOUSE_IDX] = data;
            MOUSE_IDX += 1;
        } else {
            MOUSE_IDX = 0;
        }

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

pub const YIELD_INT: u8 = 129;