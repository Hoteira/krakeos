use crate::arch::trap::TrapFrame;
use alloc::string::String;
use alloc::collections::VecDeque;
use spin::Mutex;

pub static mut USER_BRK: usize = 0;
pub static SPAWN_QUEUE: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
// The clipboard every app shares (/dev/clipboard).
pub static CLIPBOARD: Mutex<alloc::vec::Vec<u8>> = Mutex::new(alloc::vec::Vec::new());

pub const SYS_EXIT: usize = 1;
pub const SYS_READ: usize = 2;
pub const SYS_WRITE: usize = 3;
pub const SYS_OPEN: usize = 4;
pub const SYS_SBRK: usize = 5;
pub const SYS_SPAWN: usize = 12;
pub const SYS_SLEEP: usize = 13;
pub const SYS_FSTAT: usize = 14;
pub const SYS_CLOSE: usize = 15;
pub const SYS_READDIR: usize = 16;
pub const SYS_KILL: usize = 17;
pub const SYS_SETNAME: usize = 18;
pub const SYS_MEM_RESERVE: usize = 19;
pub const SYS_MEM_COMMIT: usize = 20;
pub const SYS_MEM_RELEASE: usize = 21;
// a0 = op (FSOP_*), a1/a2 = path, a3/a4 = second path.
pub const SYS_FSOP: usize = 22;

pub const FSOP_UNLINK: usize = 1;
pub const FSOP_RMDIR: usize = 2;
pub const FSOP_MKDIR: usize = 3;
pub const FSOP_RENAME: usize = 4;
// Returns size | is_dir << 32, or usize::MAX if the path doesn't exist.
pub const FSOP_STAT: usize = 5;

// Root directory is descriptor 0, but fd 0 means stdin to WASI apps.
pub const ROOT_DIR_FD: usize = 111;
pub const PROCS_FD: usize = 112;
// Translucent default background behind app text.
pub const GPU_GLASS_FD: usize = 113;
pub const GPU_FONT_FD: usize = 114;
pub const GPU_INFO_FD: usize = 115;
// Always the serial log (the runner's messages for piped apps).
pub const LOG_FD: usize = 117;
// Opening with truncate clears the clipboard; writes land at their offset.
pub const CLIPBOARD_FD: usize = 116;

pub fn dispatch(frame: &mut TrapFrame) -> Option<usize> {
    let id = frame.regs[17]; // a7
    let a0 = frame.regs[10];
    let a1 = frame.regs[11];
    let a2 = frame.regs[12];
    let a3 = frame.regs[13];
    let a4 = frame.regs[14];
    let a5 = frame.regs[15];
    
    // sstatus bit 18: SUM, lets the kernel touch user memory
    unsafe {
        core::arch::asm!("csrs sstatus, {}", in(reg) 1 << 18);
    }
    
    let mut ret_sp = None;

    match id {
        SYS_EXIT => {
            crate::println!("Process exited with code: {}", a0);

            // Never wfi-loop here: interrupts are off in the trap handler.
            // The thread's stacks are freed on a later switch.
            unsafe {
                crate::sys::scheduler::Scheduler::exit_current();

                // Thread 0 is the kernel idle thread; shut down when no user threads remain.
                let mut has_user = false;
                for i in 1..crate::sys::scheduler::MAX_THREADS {
                    if !matches!(
                        crate::sys::scheduler::SCHEDULER.threads[i].state,
                        crate::sys::scheduler::ThreadState::Empty | crate::sys::scheduler::ThreadState::Dead
                    ) {
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

            // No upper-address check: user thread stacks are identity-mapped
            // kernel-heap pages above 0x8000_0000.
            if ptr.is_null() {
                frame.regs[10] = 0;
            } else {
                let buf = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
                
                let bytes_read = if fd == 0 {
                    crate::sys::pipe::app_read(buf).unwrap_or(0)
                } else if crate::sys::pipe::is_pipe_fd(fd) {
                    crate::sys::pipe::read(fd, buf)
                } else if fd >= crate::sys::desktop::TTY_FD_BASE {
                    crate::sys::desktop::read_tty(fd, buf)
                } else if fd == 102 {
                    if let Some(filename) = SPAWN_QUEUE.lock().pop_front() {
                        let bytes = filename.as_bytes();
                        let copy_len = core::cmp::min(len, bytes.len());
                        buf[0..copy_len].copy_from_slice(&bytes[0..copy_len]);
                        copy_len
                    } else { 0 }
                } else if fd == GPU_INFO_FD {
                    let mut bytes = [0u8; 28];
                    for (i, v) in crate::sys::desktop::info().iter().enumerate() {
                        bytes[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
                    }
                    let copy_len = len.min(bytes.len());
                    buf[..copy_len].copy_from_slice(&bytes[..copy_len]);
                    copy_len
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
                } else if fd == PROCS_FD {
                    let list = crate::sys::scheduler::Scheduler::list();
                    let bytes = list.as_bytes();
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
                } else if fd == CLIPBOARD_FD {
                    let clip = CLIPBOARD.lock();
                    if offset >= clip.len() {
                        0
                    } else {
                        let copy_len = core::cmp::min(len, clip.len() - offset);
                        buf[..copy_len].copy_from_slice(&clip[offset..offset + copy_len]);
                        copy_len
                    }
                } else if fd > 0 && fd < crate::fs::MAX_DESCRIPTORS {
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
                
                let piped = if fd == 1 || fd == 2 { crate::sys::pipe::app_write(buf) } else { None };
                if let Some(n) = piped {
                    frame.regs[10] = n;
                } else if crate::sys::pipe::is_pipe_fd(fd) {
                    frame.regs[10] = crate::sys::pipe::write(fd, buf);
                } else if fd == 1 || fd == 2 || fd == LOG_FD {
                    let mut uart = crate::uart::Uart::new(0x1000_0000);
                    for &b in buf {
                        uart.put(b);
                    }
                    frame.regs[10] = len;
                } else if fd >= crate::sys::desktop::TTY_FD_BASE {
                    frame.regs[10] = crate::sys::desktop::write_tty(fd, buf);
                } else if fd == 104 || fd == GPU_GLASS_FD {
                    // Wallpaper or glass layer, screen-sized BGRA at offset.
                    frame.regs[10] = crate::sys::desktop::write_layer(fd == GPU_GLASS_FD, offset, buf);
                } else if fd == GPU_FONT_FD {
                    frame.regs[10] = crate::sys::desktop::write_font(buf);
                } else if fd == CLIPBOARD_FD {
                    let mut clip = CLIPBOARD.lock();
                    if offset <= clip.len() && offset + len <= 1 << 20 {
                        clip.truncate(offset);
                        clip.extend_from_slice(buf);
                        frame.regs[10] = len;
                    } else {
                        frame.regs[10] = 0;
                    }
                } else {
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
                frame.regs[10] = usize::MAX;
            } else {
                let buf = unsafe { core::slice::from_raw_parts(ptr, len) };
                if let Ok(path) = core::str::from_utf8(buf) {
                    // WASI strips the preopen prefix, so paths may lack the leading '/'.
                    let path = path.strip_prefix('/').unwrap_or(path);

                    if path == "dev/pipe" {
                        frame.regs[10] = crate::sys::pipe::open().unwrap_or(usize::MAX);
                    } else if let Some(id) = path.strip_prefix("dev/pipe/attach/") {
                        let ok = id.parse().map_or(false, crate::sys::pipe::attach);
                        frame.regs[10] = if ok { 0 } else { usize::MAX };
                    } else if path == "dev/tty" {
                        frame.regs[10] = crate::sys::desktop::open_tty().unwrap_or(usize::MAX);
                    } else if path == "dev/system/spawn_queue" {
                        frame.regs[10] = 102;
                    } else if path == "dev/gpu/fb" {
                        frame.regs[10] = 104;
                    } else if path == "dev/gpu/glass" {
                        frame.regs[10] = GPU_GLASS_FD;
                    } else if path == "dev/gpu/font" {
                        frame.regs[10] = GPU_FONT_FD;
                    } else if path == "dev/gpu/info" {
                        frame.regs[10] = GPU_INFO_FD;
                    } else if path == "dev/system/fs" {
                        frame.regs[10] = 108;
                    } else if path == "dev/clipboard" {
                        if (flags & 8) != 0 {
                            CLIPBOARD.lock().clear();
                        }
                        frame.regs[10] = CLIPBOARD_FD;
                    } else if path == "dev/system/procs" {
                        frame.regs[10] = PROCS_FD;
                    } else if path.is_empty() || path == "." {
                        frame.regs[10] = ROOT_DIR_FD;
                    } else if path == "dev/system/time" {
                        frame.regs[10] = 109;
                    } else if let Some(filename) = path.strip_prefix("spawn:/") {
                        SPAWN_QUEUE.lock().push_back(String::from(filename));
                        frame.regs[10] = 103; // Dummy success fd
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
                        frame.regs[10] = usize::MAX;
                    }
                } else {
                    frame.regs[10] = usize::MAX;
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
            frame.regs[10] = crate::sys::scheduler::Scheduler::spawn_user_thread(entry, arg)
                .unwrap_or(usize::MAX);
        }
        SYS_SLEEP => {
            let ms = a0;
            unsafe {
                let tid = crate::sys::scheduler::current_tid();
                if crate::sys::scheduler::SCHEDULER.threads[tid].kill_pending {
                    // Tell the runner to unwind the app instead of sleeping.
                    frame.regs[10] = 1;
                    return finish(ret_sp);
                }
            }
            let ticks = ms as u64 * 10_000;
            let wakeup = crate::csr::read_time() + ticks;
            unsafe {
                let tid = crate::sys::scheduler::current_tid();
                crate::sys::scheduler::SCHEDULER.threads[tid].state = crate::sys::scheduler::ThreadState::Sleeping(wakeup);
                let new_sp = crate::sys::scheduler::switch(frame as *mut _ as usize);
                frame.regs[10] = 0;
                ret_sp = Some(new_sp);
            }
        }
        SYS_FSTAT => {
            let fd = a0;
            frame.regs[10] = if fd == CLIPBOARD_FD {
                CLIPBOARD.lock().len()
            } else if crate::sys::pipe::is_pipe_fd(fd) {
                crate::sys::pipe::status(fd)
            } else if fd > 0 && fd < crate::fs::MAX_DESCRIPTORS || fd >= crate::fs::ramfs::RAMFS_DESC_OFFSET && fd < crate::sys::desktop::TTY_FD_BASE {
                crate::fs::get_file_size(fd)
            } else {
                0
            };
        }
        SYS_FSOP => {
            let path = |p: usize, n: usize| -> Option<&str> {
                if p == 0 {
                    return None;
                }
                core::str::from_utf8(unsafe { core::slice::from_raw_parts(p as *const u8, n) }).ok()
            };
            let err = |r: Result<(), usize>| r.err().unwrap_or(0);
            frame.regs[10] = match (a0, path(a1, a2)) {
                (_, None) => crate::fs::E_INVAL,
                (FSOP_UNLINK, Some(p)) => err(crate::fs::remove(p, false)),
                (FSOP_RMDIR, Some(p)) => err(crate::fs::remove(p, true)),
                (FSOP_MKDIR, Some(p)) => err(crate::fs::make_dir(p).map(|_| ())),
                (FSOP_RENAME, Some(p)) => match path(a3, a4) {
                    Some(to) => err(crate::fs::rename(p, to)),
                    None => crate::fs::E_INVAL,
                },
                (FSOP_STAT, Some(p)) => match crate::fs::stat(p) {
                    Some((size, dir)) => size | (dir as usize) << 32,
                    None => usize::MAX,
                },
                _ => crate::fs::E_INVAL,
            };
        }
        SYS_CLOSE => {
            // File and device fds have nothing to release; a tty fd closes the desktop.
            let fd = a0;
            frame.regs[10] = if crate::sys::pipe::is_pipe_fd(fd) {
                crate::sys::pipe::close(fd);
                0
            } else if fd >= crate::sys::desktop::TTY_FD_BASE {
                if crate::sys::desktop::close_tty(fd) { 0 } else { usize::MAX }
            } else {
                0
            };
        }
        SYS_READDIR => {
            // a0 = dir fd, a1 = cookie (0 to start), a2 = buf, a3 = buf len.
            let fd = a0;
            let cookie = a1;
            let ptr = a2 as *mut u8;
            let len = a3;
            let dir_idx = if fd == ROOT_DIR_FD {
                Some(0)
            } else if fd > 0 && fd < crate::fs::MAX_DESCRIPTORS {
                Some(fd)
            } else {
                None
            };
            frame.regs[10] = match dir_idx {
                Some(idx) if !ptr.is_null() => {
                    let buf = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
                    crate::fs::read_dir_entry(idx, cookie, buf).unwrap_or(usize::MAX)
                }
                _ => usize::MAX,
            };
        }
        SYS_KILL => {
            frame.regs[10] = if crate::sys::scheduler::Scheduler::kill(a0) { 0 } else { usize::MAX };
        }
        SYS_MEM_RESERVE => {
            frame.regs[10] = crate::sys::appmem::reserve(crate::sys::scheduler::current_tid())
                .unwrap_or(usize::MAX);
        }
        SYS_MEM_COMMIT => {
            // a0 = slot base, a1 = bytes that must be backed from the base.
            let ok = crate::sys::appmem::commit(a0, a1, crate::sys::scheduler::current_tid());
            frame.regs[10] = if ok { 0 } else { usize::MAX };
        }
        SYS_MEM_RELEASE => {
            let ok = crate::sys::appmem::release(a0, crate::sys::scheduler::current_tid());
            frame.regs[10] = if ok { 0 } else { usize::MAX };
        }
        SYS_SETNAME => {
            // a0 = name ptr, a1 = name len, a2 = killable
            let ptr = a0 as *const u8;
            if !ptr.is_null() {
                let name = unsafe { core::slice::from_raw_parts(ptr, a1) };
                let tid = crate::sys::scheduler::current_tid();
                crate::sys::scheduler::Scheduler::set_name(tid, name, a2 != 0);
            }
            frame.regs[10] = 0;
        }
        _ => {
            crate::println!("Unknown syscall: {}", id);
            frame.regs[10] = usize::MAX;
        }
    }
    
    finish(ret_sp)
}

fn finish(ret_sp: Option<usize>) -> Option<usize> {
    unsafe {
        core::arch::asm!("csrc sstatus, {}", in(reg) 1 << 18);
    }

    ret_sp
}
