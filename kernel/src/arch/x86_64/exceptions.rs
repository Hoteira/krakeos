use crate::arch::x86_64::io::{inb, outb};
use crate::window_manager::input::MOUSE;
use crate::window_manager::events::{Event, ResizeEvent, GLOBAL_EVENT_QUEUE, KeyboardEvent};
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
        let tm = crate::task::TASK_MANAGER.int_lock();
        if let Some(current) = tm.current_task_idx() {
            if let Some(thread) = tm.tasks.get(&(current)) {
                pid_to_kill = thread.process.as_ref().expect("Thread has no process").pid as i32;
            }
        }
    }

    if pid_to_kill != -1 {
        crate::task::manager::kill_process(pid_to_kill as u64);

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

/// Naked asm wrapper for #PF — bypasses extern "x86-interrupt" ABI to
/// guarantee correct exception-frame offsets.
#[unsafe(naked)]
pub extern "C" fn page_fault() {
    unsafe {
        core::arch::naked_asm!(
            // CPU pushed: SS, RSP, RFLAGS, CS, RIP, error_code
            // Save all GP registers (same order as timer_handler / CPUState layout)
            "push rbp",
            "push rax",
            "push rbx",
            "push rcx",
            "push rdx",
            "push rsi",
            "push rdi",
            "push r8",
            "push r9",
            "push r10",
            "push r11",
            "push r12",
            "push r13",
            "push r14",
            "push r15",
            // 15 pushes (120 bytes)
            // Layout from RSP:
            //   [RSP+0]   r15  ... [RSP+112] rbp
            //   [RSP+120] error_code
            //   [RSP+128] RIP  [RSP+136] CS  [RSP+144] RFLAGS
            //   [RSP+152] RSP  [RSP+160] SS
            "mov rdi, rsp",         // arg1 = saved-regs pointer
            "mov r15, rsp",         // callee-save backup of RSP
            "and rsp, -16",         // 16-byte align for C call
            "call page_fault_inner",
            "mov rsp, r15",         // restore exact RSP
            // Only reached when demand paging succeeded (inner returned)
            "pop r15",
            "pop r14",
            "pop r13",
            "pop r12",
            "pop r11",
            "pop r10",
            "pop r9",
            "pop r8",
            "pop rdi",
            "pop rsi",
            "pop rdx",
            "pop rcx",
            "pop rbx",
            "pop rax",
            "pop rbp",
            "add rsp, 8",          // skip error code
            "iretq",
        );
    }
}

/// Inner handler called from naked page_fault wrapper.
/// Returns normally ONLY when demand paging succeeded.
/// All fatal paths call kill_current_task() or halt (never return).
#[unsafe(no_mangle)]
pub extern "C" fn page_fault_inner(saved_regs: u64) {
    let base = saved_regs as *const u64;

    let cr2: u64;
    unsafe { core::arch::asm!("mov {}, cr2", out(reg) cr2); }

    // Read raw exception frame from known stack offsets
    let error_code = unsafe { *base.add(15) };
    let rip        = unsafe { *base.add(16) };
    let cs         = unsafe { *base.add(17) };
    let rflags     = unsafe { *base.add(18) };
    let fault_rsp  = unsafe { *base.add(19) };
    let ss         = unsafe { *base.add(20) };

    // Read saved GP registers for diagnostics
    let saved_r14 = unsafe { *base.add(1) };   // r14 = memory_base in AOT
    let saved_rdi = unsafe { *base.add(8) };    // rdi = ctx_ptr in AOT
    let saved_rbp = unsafe { *base.add(14) };   // rbp = frame pointer

    // Only print diagnostic for fatal/unexpected faults (not routine demand paging)
    let is_demand_pageable = (error_code & 1) == 0; // bit 0 clear = not-present (normal demand page)
    if !is_demand_pageable {
        serial_print("[PF] CR2=");
        print_hex(cr2);
        serial_print(" RIP=");
        print_hex(rip);
        serial_print(" CS=");
        print_hex(cs);
        serial_print(" RSP=");
        print_hex(fault_rsp);
        serial_print(" ERR=");
        print_hex(error_code);
        serial_print(" R14=");
        print_hex(saved_r14);
        serial_print(" RDI=");
        print_hex(saved_rdi);
        serial_print(" RBP=");
        print_hex(saved_rbp);
        serial_println("");
    }

    // --- Guard page check ---
    use crate::memory::address_space::*;
    use crate::memory::address::{PhysAddr, VirtAddr};
    use crate::memory::paging::{PageTableFlags, active_level_4_table};
    use crate::memory::mapper::Mapper;
    use crate::memory::pmm;

    let mut is_guard = false;
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
            } else if (cr2 >= stack_base && cr2 < stack_base + 4096)
                || (cr2 >= stack_base + STACK_SLOT_SIZE - 4096 && cr2 < stack_base + STACK_SLOT_SIZE)
            {
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
        // never returns
    }

    // --- Demand Paging ---
    {
        let mut vma = crate::memory::vma::GLOBAL_VMA.lock();
        if vma.is_mapped(cr2) {
            let pml4 = active_level_4_table();
            let mut mapper = unsafe {
                Mapper::new(PhysAddr::new(crate::memory::paging::virt_to_phys(
                    pml4 as *const _ as u64,
                )))
            };

            let mut flags = PageTableFlags::PRESENT
                | PageTableFlags::USER_ACCESSIBLE
                | PageTableFlags::WRITABLE;

            if cr2 >= STACK_REGION_BASE {
                flags |= PageTableFlags::NO_EXECUTE;
            }

            let virt = VirtAddr::new(cr2 & !0xFFF);

            if let Some(frame) = pmm::allocate_frame() {
                let phys = PhysAddr::new(frame);

                if let Ok(_) = mapper.map(virt, phys, flags) {
                    unsafe {
                        core::arch::asm!("invlpg [{}]", in(reg) cr2, options(nostack, preserves_flags));
                    }
                    return; // demand paging success → naked wrapper will iretq
                } else {
                    // mapper.map failed — page already exists.  Free the
                    // unused frame.  The Mapper's traversal already upgraded
                    // USER_ACCESSIBLE on all intermediate entries (including
                    // shattered huge pages), so just flush and retry.
                    pmm::free_frame(frame);
                    unsafe {
                        core::arch::asm!("invlpg [{}]", in(reg) cr2, options(nostack, preserves_flags));
                    }
                    return;
                }
            }
            // fall through to fatal if frame allocation failed
        } else {
            serial_print("[PF] Address not in VMA: ");
            print_hex(cr2);
            serial_println("");
        }
    }

    // --- Fatal page fault ---
    serial_println("\n=== PAGE FAULT ===");
    serial_print("CR2: ");
    print_hex(cr2);
    serial_print(" RIP: ");
    print_hex(rip);
    serial_print(" RSP: ");
    print_hex(fault_rsp);
    serial_print(" Err: ");
    print_hex(error_code);
    serial_print(" CS: ");
    print_hex(cs);
    {
        let tm = crate::task::TASK_MANAGER.int_lock();
        let pid = crate::task::cpu::get_current_task_idx();
        serial_print(" PID: ");
        print_hex(pid as u64);
        if pid >= 0 {
            if let Some(thread) = tm.tasks.get(&(pid as usize)) {
                if let Some(proc) = &thread.process {
                    serial_print(" code_base: ");
                    print_hex(proc.code_base);
                    serial_print(" mem_base: ");
                    print_hex(proc.linear_memory_base);
                }
            }
        }
    }
    serial_println("");

    if (cs & 3) == 3 {
        serial_println("User mode Page Fault. Terminating task.");
        kill_current_task();
        // never returns
    } else {
        serial_println("Kernel Panic: Page Fault in Kernel Mode");
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

use core::sync::atomic::{AtomicBool, AtomicU32};
static IN_IRQ: AtomicBool = AtomicBool::new(false);
static LAST_KEY_GLOBAL: AtomicU32 = AtomicU32::new(0);
static LAST_NORMAL_KEY: AtomicU32 = AtomicU32::new(0);

pub fn end_interrupt(int: u8) {
    unsafe {
        if *(&raw const crate::arch::x86_64::USING_APIC) {
            crate::arch::x86_64::apic::eoi();
        } else {
            (*(&raw const crate::arch::x86_64::pic::PICS)).end_interrupt(int);
        }
    }
}

pub extern "x86-interrupt" fn keyboard_handler(_info: &mut StackFrame) {
    if IN_IRQ.swap(true, Ordering::SeqCst) {
        end_interrupt(KEYBOARD_INT);
        return;
    }

    let scancode: u8 = inb(0x60);

    if let Some((key, pressed)) = crate::drivers::peripherals::keyboard::handle_scancode(scancode) {
        let is_super = crate::drivers::peripherals::keyboard::is_super_active();
        let mut handled_globally = false;
        
        if !is_super && key != crate::drivers::peripherals::keyboard::KEY_SUPER {
            if pressed {
                LAST_NORMAL_KEY.store(key, Ordering::SeqCst);
            } else if LAST_NORMAL_KEY.load(Ordering::SeqCst) == key {
                LAST_NORMAL_KEY.store(0, Ordering::SeqCst);
            }
        }

        // Reset shortcut debounce when super is released
        if !is_super {
            LAST_KEY_GLOBAL.store(0, Ordering::SeqCst);
        }

        // Swallow all key releases while super is held — they belong to
        // shortcuts, not the app (also prevents QEMU down/up repeat leak)
        if is_super && !pressed {
            handled_globally = true;
            if LAST_KEY_GLOBAL.load(Ordering::SeqCst) == key {
                LAST_KEY_GLOBAL.store(0, Ordering::SeqCst);
            }
        }

        // Check for keyboard shortcuts
        if is_super && pressed {
            let mut eval_key = key;
            if key == crate::drivers::peripherals::keyboard::KEY_SUPER {
                let lnk = LAST_NORMAL_KEY.load(Ordering::SeqCst);
                if lnk != 0 {
                    eval_key = lnk;
                }
            }

            crate::debugln!("[Shortcut] Super={} Key={:#x} Eval={:#x}", is_super, key, eval_key);

            let is_new_press = LAST_KEY_GLOBAL.load(Ordering::SeqCst) != eval_key;

            if is_new_press {
                if eval_key == 'p' as u32 {
                    crate::memory::vma::GLOBAL_VMA.lock().dump();
                    handled_globally = true;
                    LAST_KEY_GLOBAL.store(eval_key, Ordering::SeqCst);
                } else if eval_key == 'x' as u32 || eval_key == 'w' as u32 {
                    let active_id = crate::window_manager::composer::CLICKED_WINDOW_ID.load(Ordering::SeqCst) as u64;
                    let needs_recompose = if active_id != 0 {
                        let mut composer = crate::window_manager::composer::COMPOSER.write();
                        if active_id != composer.wallpaper.id && active_id != composer.taskbar.id {
                            composer.remove_window_data(active_id)
                        } else { false }
                    } else { false }; // COMPOSER.write() dropped
                    if needs_recompose {
                        crate::window_manager::render_worker::mark_all_dirty();
                    }
                    handled_globally = true;
                    LAST_KEY_GLOBAL.store(eval_key, Ordering::SeqCst);
                } else if eval_key == 'z' as u32 || eval_key == 'f' as u32 {
                    let active_id = crate::window_manager::composer::CLICKED_WINDOW_ID.load(Ordering::SeqCst);
                    // Phase 1: mutate window state and compute resize event.
                    // COMPOSER.write() is dropped before acquiring TASK_MANAGER to preserve
                    // lock ordering (Tasks before Composer) and avoid AB-BA deadlock.
                    let event_and_pid: Option<(crate::window_manager::events::Event, u64)> = if active_id != 0 {
                        let mut composer = crate::window_manager::composer::COMPOSER.write();
                        if let Some(w) = composer.find_window_id(active_id as u64) {
                            if w.can_resize {
                                let (screen_w, screen_h) = {
                                    let ds = crate::window_manager::display::DISPLAY_SERVER.lock();
                                    (ds.width as u64, ds.height as u64)
                                };
                                let (target_x, target_y, target_w, target_h, is_transparent) = if w.is_maximized {
                                    w.is_maximized = false;
                                    (w.prev_x, w.prev_y, w.prev_width.max(100), w.prev_height.max(100), true)
                                } else {
                                    w.is_maximized = true;
                                    w.prev_width = w.width; w.prev_height = w.height;
                                    w.prev_x = w.x; w.prev_y = w.y;
                                    (0, 0, screen_w, screen_h, false)
                                };
                                w.transparent = is_transparent;
                                w.treat_as_transparent = is_transparent;
                                let event = crate::window_manager::events::Event::Resize(
                                    crate::window_manager::events::ResizeEvent {
                                        wid: w.id as u32,
                                        width: target_w as u32,
                                        height: target_h as u32,
                                        x: target_x as i32,
                                        y: target_y as i32,
                                    }
                                );
                                Some((event, w.pid))
                            } else { None }
                        } else { None }
                    } else { None }; // COMPOSER.write() dropped here
                    // Phase 2: deliver event — TASK_MANAGER acquired AFTER COMPOSER is released.
                    if let Some((event, pid)) = event_and_pid {
                        let mut tm = crate::task::TASK_MANAGER.int_lock();
                        if !crate::window_manager::events::GLOBAL_EVENT_QUEUE.int_lock().push_to_process(&mut *tm, pid, event) {
                            drop(tm);
                            crate::window_manager::events::GLOBAL_EVENT_QUEUE.int_lock().add_event(event);
                        }
                    }
                    handled_globally = true;
                    LAST_KEY_GLOBAL.store(eval_key, Ordering::SeqCst);
                } else if eval_key == 't' as u32 {
                    crate::debugln!("[Shortcut] eval_key='t' detected");
                    crate::debugln!("[Shortcut] calling request_spawn(/apps/term.wasm)");
                    crate::task::aot_worker::request_spawn("/apps/term.wasm", true);
                    crate::debugln!("[Shortcut] request_spawn returned");
                    handled_globally = true;
                    crate::debugln!("[Shortcut] handled_globally=true");
                    LAST_KEY_GLOBAL.store(eval_key, Ordering::SeqCst);
                    crate::debugln!("[Shortcut] LAST_KEY_GLOBAL stored");
                } else if eval_key == 0x0D {
                    crate::debugln!("[Shortcut] eval_key=0x0D (Enter) detected");
                    crate::debugln!("[Shortcut] calling request_spawn(/apps/term.wasm)");
                    crate::task::aot_worker::request_spawn("/apps/term.wasm", true);
                    crate::debugln!("[Shortcut] request_spawn returned");
                    handled_globally = true;
                    crate::debugln!("[Shortcut] handled_globally=true");
                    LAST_KEY_GLOBAL.store(eval_key, Ordering::SeqCst);
                    crate::debugln!("[Shortcut] LAST_KEY_GLOBAL stored");
                } else if eval_key >= '1' as u32 && eval_key <= '5' as u32 {
                    let workspace_idx = (eval_key - '1' as u32) as usize;
                    // Data mutation under brief write lock, render under read lock after.
                    let changed = crate::window_manager::composer::COMPOSER.write().switch_workspace_data(workspace_idx);
                    if changed {
                        crate::window_manager::render_worker::mark_all_dirty();
                    }
                    handled_globally = true;
                    LAST_KEY_GLOBAL.store(eval_key, Ordering::SeqCst);
                } else if key == crate::drivers::peripherals::keyboard::KEY_SUPER {
                    handled_globally = true;
                }
            } else {
                handled_globally = true;
            }
        }

        if !handled_globally {
            let active_window_id = crate::window_manager::composer::CLICKED_WINDOW_ID.load(Ordering::SeqCst);
            if active_window_id != 0 {
                let mut target_pid = None;
                {
                    let composer = crate::window_manager::composer::COMPOSER.read();
                    for ws in 0..5 {
                        for w in &composer.workspaces[ws].windows {
                            if w.id == active_window_id as u64 {
                                target_pid = Some(w.pid);
                                break;
                            }
                        }
                        if target_pid.is_some() { break; }
                    }
                }

                if let Some(pid) = target_pid {
                    if pressed && !is_super {
                        let mut tm = crate::task::TASK_MANAGER.int_lock();
                        if let Some(thread) = tm.tasks.get(&(pid as usize)) {
                            if let Some(proc) = &thread.process {
                                proc.stdin_buffer.lock().push_back(key);
                                crate::task::event_manager::signal_event_internal(&mut tm, crate::task::event_manager::AsyncEvent::Read(pid as i32));
                            }
                        }
                    }

                    // Also send as window event if handler is registered
                    let event = Event::Keyboard(KeyboardEvent {
                        wid: active_window_id as u32,
                        key,
                        pressed,
                        repeat: 1,
                    });

                    let mut pushed = false;
                    if let Some(mut tm) = crate::task::TASK_MANAGER.try_lock() {
                        if GLOBAL_EVENT_QUEUE.int_lock().push_to_process(&mut tm, pid, event) {
                            pushed = true;
                        }
                    }

                    if !pushed {
                        GLOBAL_EVENT_QUEUE.lock().add_event(event);
                    }
                }
            }
        }
    }

    IN_IRQ.store(false, Ordering::SeqCst);
    end_interrupt(KEYBOARD_INT);
}

pub const MOUSE_INT: u8 = 44;
pub static mut MOUSE_PACKET: [u8; 4] = [0; 4];
pub static mut MOUSE_IDX: usize = 0;

pub extern "x86-interrupt" fn mouse_handler(_info: &mut StackFrame) {
    if IN_IRQ.swap(true, Ordering::SeqCst) {
        end_interrupt(MOUSE_INT);
        return;
    }

    use crate::drivers::peripherals::mouse::{VMMOUSE_ACTIVE, vmport_in, VMPORT_CMD_VMMOUSE_STATUS, VMPORT_CMD_VMMOUSE_DATA, MOUSE_IDX, MOUSE_PACKET, MOUSE_PACKET_SIZE};

    unsafe {
        if VMMOUSE_ACTIVE {
            let (status, _, _, _, _, _) = vmport_in(VMPORT_CMD_VMMOUSE_STATUS, 0);
            let count = status & 0xFFFF;
            if count > 0 {
                let num_packets = count / 4;
                for _ in 0..num_packets {
                    let (buttons, x, y, z, _, _) = vmport_in(VMPORT_CMD_VMMOUSE_DATA, 4);
                    crate::window_manager::input::handle_vmmouse(buttons, x, y, z);
                }
            }
            
            let mut limit = 5;
            while (inb(0x64) & 1) == 1 && limit > 0 {
                let _ = inb(0x60);
                limit -= 1;
            }

            IN_IRQ.store(false, Ordering::SeqCst);
            end_interrupt(MOUSE_INT);
            return;
        }
    }

    let data = inb(0x60);

    unsafe {
        if MOUSE_IDX == 0 && ((data & 0x08) == 0 || data == 0xFF) {
            IN_IRQ.store(false, Ordering::SeqCst);
            end_interrupt(MOUSE_INT);
            return;
        }

        if MOUSE_IDX < 4 {
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

        IN_IRQ.store(false, Ordering::SeqCst);
        end_interrupt(MOUSE_INT);
    }
}

pub const YIELD_INT: u8 = 129;

/// VirtIO Block completion interrupt handler (IRQ 10 → vector 42).
/// Reads ISR register (clears the PCI interrupt), sets COMPLETION_FLAG, sends EOI.
pub extern "x86-interrupt" fn blk_interrupt_handler(_frame: &mut StackFrame) {
    crate::fs::virtio::on_disk_irq();
    end_interrupt(crate::fs::virtio::BLK_INT_VEC);
}

/// VirtIO Net RX interrupt handler (IRQ 11 → vector 43).
/// Reads and clears the VirtIO ISR status register (mandatory acknowledgement),
/// drains the RX used ring, then sends EOI.
pub extern "x86-interrupt" fn net_interrupt_handler(_frame: &mut StackFrame) {
    // Must read ISR to acknowledge the interrupt to the device.
    let isr = crate::drivers::network::virtio::read_isr();
    if isr & 1 != 0 {
        // Queue interrupt — drain the RX used ring.
        crate::drivers::network::virtio::poll_rx();
    }
    end_interrupt(NET_INT);
}

/// TLB shootdown IPI handler (vector 0x50 = 80).
/// Flushes the local TLB by reloading CR3, then sends EOI.
pub extern "x86-interrupt" fn tlb_shootdown_handler(_frame: &mut StackFrame) {
    crate::memory::vmm::tlb_shootdown_handler();
}
