use crate::arch::trap::TrapFrame;
use alloc::string::String;
use alloc::collections::VecDeque;
use spin::Mutex;

pub static mut USER_BRK: usize = 0;
pub static SPAWN_QUEUE: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

pub const SYS_EXIT: usize = 1;
pub const SYS_READ: usize = 2;
pub const SYS_WRITE: usize = 3;
pub const SYS_OPEN: usize = 4;
pub const SYS_SBRK: usize = 5;
pub const SYS_SPAWN: usize = 12;
pub const SYS_SLEEP: usize = 13;
pub const SYS_FSTAT: usize = 14;

pub fn dispatch(frame: &mut TrapFrame) -> Option<usize> {
    let id = frame.regs[17]; // a7
    let a0 = frame.regs[10];
    let a1 = frame.regs[11];
    let a2 = frame.regs[12];
    let a5 = frame.regs[15];
    
    // Enable Supervisor User Memory (SUM) access (bit 18 of sstatus)
    unsafe {
        core::arch::asm!("csrs sstatus, {}", in(reg) 1 << 18);
    }
    
    let mut ret_sp = None;

    match id {
        SYS_EXIT => {
            crate::println!("Process exited with code: {}", a0);

            // Mark the thread dead and switch to the next runnable one.
            // (Never wfi-loop here: interrupts are off inside the trap handler,
            // so waiting for the timer would hang the whole machine.)
            unsafe {
                let current_tid = crate::sys::scheduler::SCHEDULER.current;
                crate::sys::scheduler::SCHEDULER.threads[current_tid].state = crate::sys::scheduler::ThreadState::Empty;
                if crate::sys::scheduler::FPU_OWNER == Some(current_tid) {
                    crate::sys::scheduler::FPU_OWNER = None;
                }

                // Thread 0 is the kernel idle thread; shut down when no user threads remain
                let mut has_user = false;
                for i in 1..crate::sys::scheduler::MAX_THREADS {
                    if crate::sys::scheduler::SCHEDULER.threads[i].state != crate::sys::scheduler::ThreadState::Empty {
                        has_user = true;
                        break;
                    }
                }

                if !has_user {
                    crate::arch::sbi::shutdown();
                }

                let new_sp = crate::sys::scheduler::switch(frame as *mut _ as usize);
                ret_sp = Some(new_sp);
            }
        }
        SYS_READ => {
            let fd = a0;
            let ptr = a1 as *mut u8;
            let len = a2;
            let offset = a5;

            // NOTE: no upper-address check here — user thread stacks are
            // identity-mapped kernel-heap pages above 0x8000_0000.
            if ptr.is_null() {
                frame.regs[10] = 0; // Error / read 0
            } else {
                let buf = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
                
                let bytes_read = if fd == 100 || fd == 101 {
                    let mut state = crate::sys::input::INPUT_STATE.lock();
                    let queue = if fd == 100 { &mut state.keyboard_events } else { &mut state.mouse_events };
                    if let Some(ev) = queue.pop_front() {
                        if len >= 8 {
                            let ev_bytes: [u8; 8] = unsafe { core::mem::transmute(ev) };
                            buf[0..8].copy_from_slice(&ev_bytes);
                            8
                        } else { 0 }
                    } else { 0 }
                } else if fd == 102 {
                    if let Some(filename) = SPAWN_QUEUE.lock().pop_front() {
                        let bytes = filename.as_bytes();
                        let copy_len = core::cmp::min(len, bytes.len());
                        buf[0..copy_len].copy_from_slice(&bytes[0..copy_len]);
                        copy_len
                    } else { 0 }
                } else if fd == 105 {
                    let mut bytes_copied = 0;
                    unsafe {
                        let comp = &*core::ptr::addr_of!(crate::sys::compositor::COMPOSITOR);
                        for i in 0..16 {
                            let win = &comp.windows[i];
                            let state = [
                                win.active as u32,
                                win.x,
                                win.y,
                                win.width,
                                win.height,
                                win.z_order as u32,
                            ];
                            let state_bytes: [u8; 24] = core::mem::transmute(state);
                            if bytes_copied + 24 <= len {
                                buf[bytes_copied..bytes_copied+24].copy_from_slice(&state_bytes);
                                bytes_copied += 24;
                            }
                        }
                        // Trailing u32: content generation, for damage tracking
                        if bytes_copied + 4 <= len {
                            let gen = *core::ptr::addr_of!(crate::sys::compositor::CONTENT_GEN);
                            buf[bytes_copied..bytes_copied+4].copy_from_slice(&gen.to_le_bytes());
                            bytes_copied += 4;
                        }
                    }
                    bytes_copied
                } else if fd == 106 {
                    unsafe {
                        let win_id = offset as usize;
                        if win_id < 16 {
                            let win = &(*core::ptr::addr_of!(crate::sys::compositor::COMPOSITOR)).windows[win_id];
                            if win.active {
                                let pixels_bytes: &[u8] = core::slice::from_raw_parts(
                                    win.buffer.as_ptr() as *const u8,
                                    win.buffer.len() * 4
                                );
                                let copy_len = core::cmp::min(len, pixels_bytes.len());
                                buf[0..copy_len].copy_from_slice(&pixels_bytes[0..copy_len]);
                                copy_len
                            } else { 0 }
                        } else { 0 }
                    }
                } else if fd == 108 {
                    let fs_str = crate::fs::list_fs();
                    let bytes = fs_str.as_bytes();
                    if offset >= bytes.len() {
                        0
                    } else {
                        let copy_len = core::cmp::min(len, bytes.len() - offset);
                        buf[0..copy_len].copy_from_slice(&bytes[offset..offset+copy_len]);
                        copy_len
                    }
                } else if fd == 109 {
                    let time = crate::drivers::rtc::get_time_ns();
                    let bytes = time.to_le_bytes();
                    if offset >= bytes.len() {
                        0
                    } else {
                        let copy_len = core::cmp::min(len, bytes.len() - offset);
                        buf[0..copy_len].copy_from_slice(&bytes[offset..offset+copy_len]);
                        copy_len
                    }
                } else if fd >= crate::sys::compositor::COMPOSITOR_FD_BASE {
                    // Reading a window fd returns routed mouse events for that
                    // window (writing it draws pixels).
                    let win_id = fd - crate::sys::compositor::COMPOSITOR_FD_BASE;
                    crate::sys::compositor::read_window_mouse(win_id, buf)
                } else if fd > 0 && fd < 100 {
                    crate::fs::read_file(fd, offset, buf)
                } else if fd >= 1024 {
                    crate::fs::ramfs::read_file(fd, offset, buf)
                } else {
                    0
                };

                frame.regs[10] = bytes_read;
            }
        }
        SYS_WRITE => {
            let fd = a0;
            let ptr = a1 as *const u8;
            let len = a2;
            let offset = a5;

            if ptr.is_null() {
                frame.regs[10] = 0;
            } else {
                let buf = unsafe { core::slice::from_raw_parts(ptr, len) };
                
                if fd == 1 || fd == 2 {
                    let mut uart = crate::uart::Uart::new(0x1000_0000);
                    for &b in buf {
                        uart.put(b);
                    }
                    frame.regs[10] = len;
                } else if fd == 104 {
                    unsafe {
                        // Writes land in the compositor's base layer; the
                        // kernel compositor presents it (plus windows) from
                        // the timer tick.
                        let base_ptr = core::ptr::addr_of_mut!(crate::sys::compositor::BASE) as *mut u8;
                        let max_len = (crate::drivers::virtio_gpu::FB_WIDTH * crate::drivers::virtio_gpu::FB_HEIGHT * 4) as usize;
                        let copy_len = core::cmp::min(len, max_len);

                        // write at offset!
                        let end = core::cmp::min(offset + copy_len, max_len);
                        let real_copy = if end > offset { end - offset } else { 0 };

                        if real_copy > 0 {
                            core::ptr::copy_nonoverlapping(buf.as_ptr(), base_ptr.add(offset), real_copy);
                            crate::sys::compositor::BASE_DIRTY = true;
                            // A base write covers the whole screen (wallpaper +
                            // icons + taskbar); damage everything.
                            crate::sys::compositor::damage_all();
                        }
                        frame.regs[10] = real_copy;
                    }
                } else if fd == 105 {
                    if len == 16 * 24 {
                        unsafe {
                            let comp = &mut *core::ptr::addr_of_mut!(crate::sys::compositor::COMPOSITOR);
                            for i in 0..16 {
                                let mut state_bytes = [0u8; 24];
                                state_bytes.copy_from_slice(&buf[i*24..(i+1)*24]);
                                let state: [u32; 6] = core::mem::transmute(state_bytes);
                                // Damage the window's OLD footprint before moving
                                // it, so the area it vacated gets repainted.
                                if comp.windows[i].active {
                                    crate::sys::compositor::expand_damage(
                                        comp.windows[i].x as usize,
                                        comp.windows[i].y as usize,
                                        comp.windows[i].width as usize,
                                        comp.windows[i].height as usize + 24,
                                    );
                                }
                                // shell can deactivate a window but not activate one
                                if state[0] == 0 { comp.windows[i].active = false; }
                                comp.windows[i].x = state[1];
                                comp.windows[i].y = state[2];
                                // Keep the pixel buffer in sync with the window size so
                                // write_window/read never index past the buffer.
                                if comp.windows[i].active
                                    && (state[3] != comp.windows[i].width || state[4] != comp.windows[i].height)
                                {
                                    comp.windows[i].buffer.clear();
                                    comp.windows[i].buffer.resize(state[3] as usize * state[4] as usize, 0xFF000000);
                                }
                                comp.windows[i].width = state[3];
                                comp.windows[i].height = state[4];
                                comp.windows[i].z_order = state[5] as usize;
                                // Damage the NEW footprint too.
                                if comp.windows[i].active {
                                    crate::sys::compositor::expand_damage(
                                        state[1] as usize,
                                        state[2] as usize,
                                        state[3] as usize,
                                        state[4] as usize + 24,
                                    );
                                }
                            }
                            crate::sys::compositor::bump_gen();
                            frame.regs[10] = len;
                        }
                    } else {
                        frame.regs[10] = 0;
                    }
                } else if fd == 110 {
                    frame.regs[10] = crate::drivers::virtio_gpu::set_cursor_image(buf);
                } else if fd >= crate::sys::compositor::COMPOSITOR_FD_BASE {
                    let win_id = fd - crate::sys::compositor::COMPOSITOR_FD_BASE;
                    let bw = crate::sys::compositor::write_window(win_id, buf);
                    frame.regs[10] = bw;
                } else {
                    // write to file
                    let bw = if fd >= 1024 {
                        crate::fs::ramfs::write_file(fd, offset, buf)
                    } else {
                        crate::fs::write_file(fd, offset, buf)
                    };
                    frame.regs[10] = bw;
                }
            }
        }
        SYS_OPEN => {
            let ptr = a0 as *const u8;
            let len = a1;
            let flags = a2;

            if ptr.is_null() {
                frame.regs[10] = usize::MAX; // Error
            } else {
                let buf = unsafe { core::slice::from_raw_parts(ptr, len) };
                if let Ok(path) = core::str::from_utf8(buf) {
                    // Device paths arrive both with and without a leading '/'
                    // (WASI strips the preopen prefix); normalize once.
                    let path = path.strip_prefix('/').unwrap_or(path);

                    if path == "dev/gpu/window" {
                        if let Some(fd) = crate::sys::compositor::create_window() {
                            frame.regs[10] = crate::sys::compositor::COMPOSITOR_FD_BASE + fd;
                        } else {
                            frame.regs[10] = usize::MAX;
                        }
                    } else if path == "dev/input/keyboard" {
                        frame.regs[10] = 100;
                    } else if path == "dev/input/mouse" {
                        frame.regs[10] = 101;
                    } else if path == "dev/system/spawn_queue" {
                        frame.regs[10] = 102;
                    } else if path == "dev/gpu/fb" {
                        frame.regs[10] = 104;
                    } else if path == "dev/system/windows" {
                        frame.regs[10] = 105;
                    } else if path == "dev/system/window_fb" {
                        frame.regs[10] = 106;
                    } else if path == "dev/system/fs" {
                        frame.regs[10] = 108;
                    } else if path == "dev/system/time" {
                        frame.regs[10] = 109;
                    } else if path == "dev/gpu/cursor" {
                        // Only claim hardware-cursor support when the cursorq
                        // actually initialized; the shell falls back to its
                        // software cursor otherwise.
                        frame.regs[10] = if crate::drivers::virtio_gpu::cursor_available() { 110 } else { usize::MAX };
                    } else if let Some(filename) = path.strip_prefix("spawn:/") {
                        SPAWN_QUEUE.lock().push_back(String::from(filename));
                        frame.regs[10] = 103; // Dummy success FD
                    } else if let Some(desc_idx) = crate::fs::find_file(path) {
                        if (flags & 8) != 0 {
                            crate::fs::truncate_file(desc_idx, 0);
                        }
                        frame.regs[10] = desc_idx;
                    } else if (flags & 1) != 0 {
                        if let Some(desc_idx) = crate::fs::create_file(path) {
                            frame.regs[10] = desc_idx;
                        } else {
                            frame.regs[10] = usize::MAX;
                        }
                    } else {
                        frame.regs[10] = usize::MAX; // Error
                    }
                } else {
                    frame.regs[10] = usize::MAX; // Error
                }
            }
        }
        SYS_SBRK => {
            let increment = a0;
            unsafe {
                let old_brk = USER_BRK;
                if increment == 0 {
                    frame.regs[10] = old_brk;
                } else {
                    let mut mapped_vaddr = (old_brk + 4095) & !4095;
                    let required_vaddr = old_brk + increment;
                    let required_mapped = (required_vaddr + 4095) & !4095;
                    
                    let layout = core::alloc::Layout::from_size_align(4096, 4096).unwrap();
                    let mut oom = false;
                    while mapped_vaddr < required_mapped {
                        let phys_addr = alloc::alloc::alloc_zeroed(layout) as usize;
                        if phys_addr == 0 {
                            crate::println!("OUT OF MEMORY during sys_sbrk!");
                            oom = true;
                            break;
                        }
                        crate::arch::paging::map_page(
                            crate::ROOT_PAGE_TABLE, 
                            mapped_vaddr, 
                            phys_addr, 
                            crate::arch::paging::PTE_R | crate::arch::paging::PTE_W | crate::arch::paging::PTE_X | crate::arch::paging::PTE_U
                        );
                        mapped_vaddr += 4096;
                    }
                    if !oom {
                        USER_BRK = required_vaddr;
                        crate::csr::sfence_vma();
                        frame.regs[10] = old_brk;
                    } else {
                        frame.regs[10] = usize::MAX;
                    }
                }
            }
        }

        SYS_SPAWN => {
            let entry = a0;
            let arg = a1;
            crate::sys::scheduler::Scheduler::spawn_user_thread(entry, arg);
            frame.regs[10] = 0;
        }
        SYS_SLEEP => {
            let ms = a0;
            let ticks = ms as u64 * 10_000;
            let wakeup = crate::csr::read_time() + ticks;
            unsafe {
                let tid = crate::sys::scheduler::SCHEDULER.current;
                crate::sys::scheduler::SCHEDULER.threads[tid].state = crate::sys::scheduler::ThreadState::Sleeping(wakeup);
                let new_sp = crate::sys::scheduler::switch(frame as *mut _ as usize);
                frame.regs[10] = 0;
                ret_sp = Some(new_sp);
            }
        }
        SYS_FSTAT => {
            let fd = a0;
            frame.regs[10] = crate::fs::get_file_size(fd);
        }
        _ => {
            crate::println!("Unknown syscall: {}", id);
            frame.regs[10] = usize::MAX;
        }
    }
    
    // Disable SUM access
    unsafe {
        core::arch::asm!("csrc sstatus, {}", in(reg) 1 << 18);
    }

    ret_sp
}
