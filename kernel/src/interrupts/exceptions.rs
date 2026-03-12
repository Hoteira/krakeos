use crate::drivers::periferics::keyboard::KEYBOARD_BUFFER;
use crate::drivers::port::{inb, outb};
use crate::window_manager::input::{MOUSE, RESIZING_WINDOW, W_WIDTH, W_HEIGHT, CLICKED_WINDOW_ID};
use crate::window_manager::events::{Event, ResizeEvent, GLOBAL_EVENT_QUEUE};
use core::sync::atomic::Ordering;

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
        let mut timeout = 10000;
        while (inb(0x3F8 + 5) & 0x20) == 0 && timeout > 0 {
            core::hint::spin_loop();
            timeout -= 1;
        }
        if timeout > 0 {
            outb(0x3F8, b);
        }
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
            if let Some(thread) = tm.tasks.get(&(current)) {
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

    // Check for guard page hits
    use crate::memory::address_space::*;
    use crate::memory::address::{PhysAddr, VirtAddr};
    use crate::memory::paging::{PageTableFlags, active_level_4_table};
    use crate::memory::mapper::Mapper;
    use crate::memory::pmm;

    let mut is_guard = false;
    // We only check for guards if CR2 is in the SAS regions
    if cr2 >= CODE_REGION_BASE && cr2 < LINEAR_MEMORY_BASE + (MAX_SLOTS as u64 * LINEAR_MEMORY_SLOT_SIZE) {
        let slot_id = if cr2 < STACK_REGION_BASE {
            ((cr2 - CODE_REGION_BASE) / CODE_SLOT_SIZE) as u16
        } else if cr2 < LINEAR_MEMORY_BASE {
            ((cr2 - STACK_REGION_BASE) / STACK_SLOT_SIZE) as u16
        } else {
            ((cr2 - LINEAR_MEMORY_BASE) / LINEAR_MEMORY_SLOT_SIZE) as u16
        };

        if slot_id < MAX_SLOTS {
            let code_base = CODE_REGION_BASE + (slot_id as u64) * CODE_SLOT_SIZE;
            let stack_base = STACK_REGION_BASE + (slot_id as u64) * STACK_SLOT_SIZE;
            let lin_base = LINEAR_MEMORY_BASE + (slot_id as u64) * LINEAR_MEMORY_SLOT_SIZE;

            if cr2 >= code_base + CODE_SLOT_SIZE - 4096 && cr2 < code_base + CODE_SLOT_SIZE {
                is_guard = true;
            } else if (cr2 >= stack_base && cr2 < stack_base + 4096) || (cr2 >= stack_base + STACK_SLOT_SIZE - 4096 && cr2 < stack_base + STACK_SLOT_SIZE) {
                is_guard = true;
            } else if cr2 >= lin_base + LINEAR_MEMORY_SLOT_SIZE - 4096 && cr2 < lin_base + LINEAR_MEMORY_SLOT_SIZE {
                is_guard = true;
            }
        }
    }

    if is_guard {
        serial_print("\nGuard page fault at ");
        print_hex(cr2);
        serial_println(". Terminating task.");
        kill_current_task();
        return;
    }

    // Demand Paging Logic
    let mut vma = crate::memory::vma::GLOBAL_VMA.lock();
    if vma.is_mapped(cr2) {
        // Find PID for this region to attribute memory correctly
        let mut target_pid = 0;
        for region in vma.get_regions() {
            if cr2 >= region.start && cr2 < region.start + region.size {
                target_pid = region.pid;
                break;
            }
        }

        if let Some(frame) = pmm::allocate_frame() {
            let pml4 = active_level_4_table();
            let mut mapper = unsafe { Mapper::new(PhysAddr::new(crate::memory::paging::virt_to_phys(pml4 as *const _ as u64))) };
            
            let mut flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE | PageTableFlags::WRITABLE;
            
            // If it's NOT in the code region, set NX
            if cr2 < STACK_REGION_BASE {
                // Code region: allow execution (do not set NX)
            } else {
                flags |= PageTableFlags::NO_EXECUTE;
            }

            let virt = VirtAddr::new(cr2 & !0xFFF);
            let phys = PhysAddr::new(frame);

            if let Ok(_) = mapper.map(virt, phys, flags) {
                // Flush TLB
                unsafe {
                    core::arch::asm!("invlpg [{}]", in(reg) cr2, options(nostack, preserves_flags));
                }
                return;
            }
        }
    } else {
        serial_print("[PF] Address not in VMA: ");
        print_hex(cr2);
        serial_println("");
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

static mut IN_IRQ: bool = false;

pub extern "x86-interrupt" fn keyboard_handler(_info: &mut StackFrame) {
    unsafe {
        if IN_IRQ {
            (*(&raw const crate::interrupts::pic::PICS)).end_interrupt(KEYBOARD_INT);
            return;
        }
        IN_IRQ = true;
    }

    serial_print("K");
    let scancode: u8 = inb(0x60);

    if let Some((key, pressed)) = crate::drivers::periferics::keyboard::handle_scancode(scancode) {
        let is_super = crate::drivers::periferics::keyboard::is_super_active();
        let mut handled_globally = false;

        // Check for keyboard shortcuts
        if is_super && pressed {
            if key == 'p' as u32 {
                crate::memory::vma::GLOBAL_VMA.lock().dump();
                handled_globally = true;
            }
        }

        if !handled_globally {
            if pressed && !is_super {
                KEYBOARD_BUFFER.lock().push_back(key);
                // Signal readiness for stdin (FD 0)
                crate::interrupts::event_manager::signal_event(crate::interrupts::event_manager::AsyncEvent::Read(0));
            }

            // Dispatch to Window Manager
            unsafe {
                let active_window_id = crate::window_manager::input::CLICKED_WINDOW_ID;
                if active_window_id != 0 {
                    let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
                    let composer = &*(&raw const crate::window_manager::composer::COMPOSER);
                    let mut target_info = None;
                    for w in &composer.windows {
                        if w.id == active_window_id {
                            if w.event_handler != 0 {
                                target_info = Some(w.pid);
                            }
                            break;
                        }
                    }

                    if let Some(w_pid) = target_info {
                        use crate::window_manager::events::{Event, KeyboardEvent, GLOBAL_EVENT_QUEUE};

                        let tm_ref = &*tm;
                        let event = Event::Keyboard(KeyboardEvent {
                            wid: active_window_id as u32,
                            key,
                            pressed,
                            repeat: 1,
                        });

                        if !GLOBAL_EVENT_QUEUE.int_lock().push_to_process(tm_ref, w_pid, event) {
                            GLOBAL_EVENT_QUEUE.int_lock().add_event(event);
                        }
                    }
                }
            }
        }
    }

    unsafe {
        IN_IRQ = false;
        (*(&raw const crate::interrupts::pic::PICS)).end_interrupt(KEYBOARD_INT);
    }
}

pub const MOUSE_INT: u8 = 44;
#[allow(dead_code)]
pub static mut MOUSE_PACKET: [u8; 4] = [0; 4];
#[allow(dead_code)]
pub static mut MOUSE_IDX: usize = 0;

pub extern "x86-interrupt" fn mouse_handler(_info: &mut StackFrame) {
    unsafe {
        if IN_IRQ {
            (*(&raw const crate::interrupts::pic::PICS)).end_interrupt(MOUSE_INT);
            return;
        }
        IN_IRQ = true;
    }

    serial_print("M");
    use crate::drivers::periferics::mouse::{VMMOUSE_ACTIVE, vmport_in, VMPORT_CMD_VMMOUSE_STATUS, VMPORT_CMD_VMMOUSE_DATA, MOUSE_IDX, MOUSE_PACKET, MOUSE_PACKET_SIZE};

    unsafe {
        if VMMOUSE_ACTIVE {
            let (status, _, _, _, _, _) = vmport_in(VMPORT_CMD_VMMOUSE_STATUS, 0);
            let count = status & 0xFFFF;
            if count > 0 {
                serial_print("V");
                let num_packets = count / 4;
                for _ in 0..num_packets {
                    let (buttons, x, y, z, _, _) = vmport_in(VMPORT_CMD_VMMOUSE_DATA, 4);
                    crate::window_manager::input::handle_vmmouse(buttons, x, y, z);
                }
            }
            
            // Clear the 8042 PS/2 controller queue so it doesn't get stuck
            let mut limit = 5;
            while (inb(0x64) & 1) == 1 && limit > 0 {
                let _ = inb(0x60);
                limit -= 1;
            }

            IN_IRQ = false;
            (*(&raw const crate::interrupts::pic::PICS)).end_interrupt(MOUSE_INT);
            return;
        }
    }

    let data = inb(0x60);
    serial_print("m");

    unsafe {
        if MOUSE_IDX == 0 && ((data & 0x08) == 0 || data == 0xFF) {
            IN_IRQ = false;
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

            crate::window_manager::input::handle_mouse_update();
            MOUSE_IDX = 0;
        }

        IN_IRQ = false;
        (*(&raw const crate::interrupts::pic::PICS)).end_interrupt(MOUSE_INT);
    }
}

pub const YIELD_INT: u8 = 129;