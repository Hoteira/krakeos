use alloc::string::String;
use alloc::vec::Vec;
use core::arch::{asm, naked_asm};
use crate::interrupts::task::CPUState;
use crate::debugln;
use crate::drivers::periferics::keyboard::KEYBOARD_BUFFER; 
use crate::memory::paging::{self, PAGE_USER, PAGE_WRITABLE};
use crate::window_manager::display::DISPLAY_SERVER;
use crate::fs::vfs::FileSystem;
use crate::window_manager::composer::COMPOSER;
use crate::window_manager::input::MOUSE;
use crate::window_manager::window::{Items, Window};

// Syscall Numbers
pub const SYS_READ: u64 = 0;
pub const SYS_PRINT: u64 = 1;
pub const SYS_MALLOC: u64 = 5;
pub const SYS_FREE: u64 = 6;
pub const SYS_REALLOC: u64 = 9;
pub const SYS_COPY_TO_DB: u64 = 8;
pub const SYS_ADD_WINDOW: u64 = 22;
pub const SYS_REMOVE_WINDOW: u64 = 23;
pub const SYS_GET_WIDTH: u64 = 44;
pub const SYS_GET_HEIGHT: u64 = 45;
pub const SYS_UPDATE_WINDOW: u64 = 51;
pub const SYS_EXIT: u64 = 60;

#[unsafe(naked)]
#[unsafe(no_mangle)]
pub extern "C" fn syscall_entry() {
    unsafe {
        naked_asm!(
            "mov [{scratch}], r15",
            "mov r15, rsp",
            "mov rsp, [{kernel_stack_ptr}]",
            "push QWORD PTR 0x23", // SS: User Data (0x20) | 3
            "push r15",
            "push r11",
            "push QWORD PTR 0x33", // CS: User Code (0x30) | 3
            "push rcx",
            "mov r15, [{scratch}]",
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
            "cld", // Clear direction flag for Rust ABI compliance
            "mov rdi, rsp",
            "call syscall_dispatcher",
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
            "iretq",
            kernel_stack_ptr = sym crate::interrupts::task::KERNEL_STACK_PTR,
            scratch = sym crate::interrupts::task::SCRATCH,
        );
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn syscall_dispatcher(context: &mut CPUState) {
    let syscall_num = context.rax;
    context.rax = 0; // Default return value

    match syscall_num {
        SYS_READ => {
            let user_ptr = context.rdi as *mut u8; 
            let user_len = context.rsi as usize;
            let mut bytes_written_to_user = 0;

            if user_ptr.is_null() {
                context.rax = 0;
                return;
            }

            let mut keyboard_buffer = KEYBOARD_BUFFER.lock();
            
            while bytes_written_to_user < user_len {
                if let Some(keycode) = keyboard_buffer.pop_front() {
                    unsafe {
                        // Direct write since identity mapped. Cast u32 to u8 (ASCII mostly)
                        *user_ptr.add(bytes_written_to_user) = keycode as u8;
                    }
                    bytes_written_to_user += 1;
                } else {
                    break;
                }
            }
            context.rax = bytes_written_to_user as u64;
        }

        SYS_PRINT => {
            let ptr = context.rdi; 
            let len = context.rsi as usize;

            // Direct access
            let s = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
            let str_val = String::from_utf8_lossy(s);
            crate::debug_print!("{}", str_val);
            
            context.rax = len as u64;
        }

        SYS_MALLOC => {
            let size = context.rdi as usize;
            let pages = (size + 4095) / 4096;
            
            let pid = {
                let tm = crate::interrupts::task::TASK_MANAGER.lock();
                if tm.current_task >= 0 {
                    tm.current_task as u64
                } else {
                    0
                }
            };

            if let Some(addr) = crate::memory::pmm::allocate_frames(pages, pid) {
                context.rax = addr;
            } else {
                context.rax = 0;
            }
        }

        SYS_REALLOC => {
            let old_ptr = context.rdi;
            let old_size = context.rsi as usize;
            let new_size = context.rdx as usize;
            
            let pid = {
                let tm = crate::interrupts::task::TASK_MANAGER.lock();
                if tm.current_task >= 0 {
                    tm.current_task as u64
                } else {
                    0
                }
            };

            if let Some(addr) = unsafe { crate::memory::pmm::realloc_memory(old_ptr, old_size, new_size, pid) } {
                context.rax = addr;
            } else {
                context.rax = 0;
            }
        }

        SYS_FREE => {
             let ptr = context.rdi;
             crate::memory::pmm::free_frame(ptr);
             context.rax = 0;
        }

        SYS_ADD_WINDOW => {
            let window_ptr = context.rdi as *const Window;
            unsafe {
                let w = *window_ptr;
                context.rax = (*(&raw mut COMPOSER)).add_window(w) as u64;
            }
        }

        SYS_UPDATE_WINDOW => {
            let window_ptr = context.rdi as *const Window;
            unsafe {
                let w = *window_ptr;
                (*(&raw mut COMPOSER)).resize_window(w);
                
                // Optimized redraw: only recompose the window's dirty area
                (*(&raw mut COMPOSER)).update_window_area(w.id);
            }
            context.rax = 1;
        }

        // SYS_GET_EVENTS
        52 => {
            let wid = context.rdi as u32;
            let buf_ptr = context.rsi as *mut crate::window_manager::events::Event;
            let max_events = context.rdx as usize;
            
            unsafe {
                use crate::window_manager::events::GLOBAL_EVENT_QUEUE;
                let events = (*(&raw mut GLOBAL_EVENT_QUEUE)).get_and_remove_events(wid, max_events);
                
                if !events.is_empty() {
                    // crate::debugln!("Syscall: Get events for wid {}, count {}", wid, events.len());
                }

                let user_slice = core::slice::from_raw_parts_mut(buf_ptr, max_events);
                for (i, evt) in events.into_iter().enumerate() {
                    if i < max_events {
                        user_slice[i] = evt;
                    }
                }
            }
            context.rax = 1;
        }

        SYS_GET_WIDTH => {
            unsafe {
                context.rax = (*(&raw mut DISPLAY_SERVER)).width;
            }
        }


        SYS_GET_HEIGHT => {
            unsafe {
                context.rax = (*(&raw mut DISPLAY_SERVER)).height;
            }
        }
        
        61 => { // SYS_OPEN
             let ptr = context.rdi as *const u8;
             let len = context.rsi as usize;
             let s = unsafe { core::slice::from_raw_parts(ptr, len) };
             let path_str_full = String::from_utf8_lossy(s);
             
             let path_parts: Vec<&str> = path_str_full.split('/').collect();
             if path_parts.len() < 1 || !path_parts[0].starts_with('@') {
                 context.rax = u64::MAX; // Invalid path format
                 return;
             }

             let disk_part = &path_parts[0][1..];
             let disk_id = if disk_part.starts_with("0x") || disk_part.starts_with("0X") {
                 u8::from_str_radix(&disk_part[2..], 16).unwrap_or(0xFF)
             } else {
                 disk_part.parse::<u8>().unwrap_or(0xFF)
             };

             if disk_id == 0xFF {
                 context.rax = u64::MAX;
                 return;
             }
             
             let actual_path_str = if path_parts.len() > 1 { path_parts[1..].join("/") } else { String::from("") };

             match crate::fs::vfs::open_file(disk_id, &actual_path_str) {
                 Ok(fd) => context.rax = fd as u64,
                 Err(_) => context.rax = u64::MAX,
             }
        }

        62 => { // SYS_FILE_READ
             let fd = context.rdi as usize;
             let buf_ptr = context.rsi as *mut u8;
             let len = context.rdx as usize;
             let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, len) };
             
             if let Some(handle) = crate::fs::vfs::get_file(fd) {
                  debugln!("Syscall: Reading file fd {}, len {}", fd, len);
                  match handle.node.read(handle.offset, buf) {
                      Ok(n) => {
                          handle.offset += n as u64;
                          context.rax = n as u64;
                          debugln!("Syscall: Read done, returned {} bytes", n);
                      },
                      Err(_) => context.rax = u64::MAX,
                  }
             } else {
                 context.rax = u64::MAX;
             }
        }

        65 => { // SYS_FILE_SIZE
             let fd = context.rdi as usize;
             if let Some(handle) = crate::fs::vfs::get_file(fd) {
                  context.rax = handle.node.size();
             } else {
                 context.rax = u64::MAX;
             }
        }

        53 => { // SYS_GET_MOUSE_POS
            unsafe {
                let mouse = &*(&raw const MOUSE);
                context.rax = ((mouse.x as u64) << 32) | (mouse.y as u64);
            }
        }

        SYS_EXIT => {
            debugln!("[Syscall] Process exited with code {}", context.rdi + 0);
            loop { unsafe { asm!("hlt") } }
        }
        
        _ => {
            debugln!("[Syscall] Unknown syscall #{}", syscall_num);
            context.rax = u64::MAX;
        }
    }
}