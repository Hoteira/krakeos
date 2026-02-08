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
        let is_super = crate::drivers::periferics::keyboard::is_super_active();
        let mut handled_globally = false;

        // Check for keyboard shortcuts
        if is_super && pressed {
            if key == 'p' as u32 {
                crate::memory::pmm::print_allocations();
                handled_globally = true;
            } else if key == 't' as u32 {
                crate::debugln!("Spawning terminal...");
                match crate::interrupts::syscalls::spawn_process("@0xE0/sys/bin/term.elf", None, None) {
                    Ok(pid) => crate::debugln!("Terminal spawned with PID: {}", pid),
                    Err(e) => crate::debugln!("Failed to spawn terminal: {}", e),
                }
                handled_globally = true;
            } else if key == 'x' as u32 || key == 'X' as u32 {
                unsafe {
                    let active_window_id = CLICKED_WINDOW_ID;
                    crate::debugln!("Global Shortcut: Win + X detected. Active window: {}", active_window_id);
                    if active_window_id != 0 {
                        let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
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
                            tm.kill_process(pid);
                        } else {
                            crate::debugln!("Global Shortcut: No PID found for Window {}", active_window_id);
                        }
                    }
                }
                handled_globally = true;
            } else if key == 122 || key == 90 { // 'z' or 'Z'
                // Toggle Maximize (Win + Z)
                unsafe {
                    let active_id = CLICKED_WINDOW_ID;
                    if active_id != 0 {
                        let composer = &mut *(&raw mut crate::window_manager::composer::COMPOSER);
                        let ds = &*(&raw const crate::window_manager::display::DISPLAY_SERVER);
                        
                        let mut window_update = None;
                        
                        if let Some(w) = composer.find_window_id(active_id) {
                            if w.w_type == crate::window_manager::window::Items::Window {
                                let screen_w = ds.width as usize;
                                let screen_h = ds.height as usize;
                                let taskbar_h = (screen_h * 4) / 100;

                                let (new_w, new_h);

                                if w.prev_width == 0 {
                                    // Maximize
                                    w.prev_x = w.x;
                                    w.prev_y = w.y;
                                    w.prev_width = w.width;
                                    w.prev_height = w.height;

                                    w.x = 0;
                                    w.y = taskbar_h as isize;
                                    w.width = screen_w;
                                    w.height = screen_h - taskbar_h;
                                    
                                    w.can_move = false;
                                    w.can_resize = false;
                                    
                                    new_w = w.width;
                                    new_h = w.height;
                                    
                                    crate::debugln!("Maximize: Window {} to {}x{}", w.id, new_w, new_h);
                                } else {
                                    // Restore
                                    w.x = w.prev_x;
                                    w.y = w.prev_y;
                                    w.width = w.prev_width;
                                    w.height = w.prev_height;
                                    
                                    w.prev_width = 0; // Reset
                                    
                                    w.can_move = true;
                                    w.can_resize = true;
                                    
                                    new_w = w.width;
                                    new_h = w.height;
                                    
                                    crate::debugln!("Restore: Window {} to {}x{}", w.id, new_w, new_h);
                                }
                                
                                window_update = Some((*w, w.pid));
                            }
                        }
                        
                        if let Some((updated_w, pid)) = window_update {
                            // Trigger resize event and update screen
                            composer.resize_window(updated_w); 
                            
                            // Dispatch ResizeEvent so app reallocates buffers
                            let event = Event::Resize(ResizeEvent {
                                wid: updated_w.id as u32,
                                width: updated_w.width as u32,
                                height: updated_w.height as u32,
                            });
                            
                            let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
                            if !GLOBAL_EVENT_QUEUE.int_lock().push_to_process(&*tm, pid, event) {
                                GLOBAL_EVENT_QUEUE.int_lock().add_event(event);
                            }
                        }
                    }
                }
                handled_globally = true;
            } else if key == 99 || key == 67 { // 'c' or 'C'
                // Start Resize Mode
                let current_resize = RESIZING_WINDOW.load(Ordering::Relaxed);
                
                if current_resize == 0 {
                    unsafe {
                        let active_id = CLICKED_WINDOW_ID;
                        if active_id != 0 {
                            crate::debugln!("Resize Mode: STARTED for Window {}", active_id);
                            let composer = &mut *(&raw mut crate::window_manager::composer::COMPOSER);
                            let ds = &mut *(&raw mut crate::window_manager::display::DISPLAY_SERVER);

                            let window_data = if let Some(w) = composer.find_window_id(active_id) {
                                if w.w_type == crate::window_manager::window::Items::Window {
                                    Some((w.id, w.x, w.y, w.width, w.height))
                                } else { None }
                            } else { None };

                            if let Some((w_id, w_x, w_y, w_width, w_height)) = window_data {
                                // 1. Clear old mouse position
                                ds.copy_to_fb(MOUSE.x as i32, MOUSE.y as i32, 32, 32);

                                // 2. Set State
                                RESIZING_WINDOW.store(active_id as u16, Ordering::Relaxed);
                                W_WIDTH = w_width;
                                W_HEIGHT = w_height;

                                // 3. Warp Mouse to bottom-right corner
                                let target_x = (w_x + w_width as isize).max(0) as u16;
                                let target_y = (w_y + w_height as isize).max(0) as u16;
                                MOUSE.x = target_x;
                                MOUSE.y = target_y;

                                // 4. Initial Draw of Resize Wireframe
                                composer.recompose_area(w_x as i32, w_y as i32, w_width as u32, w_height as u32);
                                (*(&raw mut MOUSE)).draw_resize_border(
                                    w_x as u16, 
                                    w_y as u16, 
                                    w_width as u16, 
                                    w_height as u16, 
                                    crate::window_manager::display::Color::rgb(255, 255, 255),
                                    2
                                );
                                
                                // 5. Redraw mouse and flush combined area
                                use crate::drivers::video::virtio;
                                use crate::window_manager::display::{VIRTIO_ACTIVE, HARDWARE_CURSOR_ACTIVE};
                                if VIRTIO_ACTIVE && HARDWARE_CURSOR_ACTIVE {
                                    virtio::cursor::move_cursor(target_x as u32, target_y as u32);
                                } else {
                                    ds.draw_mouse(target_x, target_y, false);
                                }
                                
                                // Flush the whole window area + mouse margin
                                // present_rect handles screen boundary clipping safely
                                ds.present_rect(w_x as i32, w_y as i32, w_width as u32 + 32, w_height as u32 + 32);
                            }
                        }
                    }
                }
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