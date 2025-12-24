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
pub const SYS_COPY_TO_DB: u64 = 8;
pub const SYS_ADD_WINDOW: u64 = 22;
pub const SYS_REMOVE_WINDOW: u64 = 23;
pub const SYS_GET_WIDTH: u64 = 44;
pub const SYS_GET_HEIGHT: u64 = 45;
pub const SYS_UPDATE_WINDOW: u64 = 51;
pub const SYS_EXIT: u64 = 60;

pub const SYS_POLL: u64 = 70;
pub const SYS_CREATE_FILE: u64 = 71;
pub const SYS_CREATE_DIR: u64 = 72;
pub const SYS_REMOVE: u64 = 73;
pub const SYS_RENAME: u64 = 74;

static mut NEXT_LOAD_BASE: u64 = 0x08000000; // Start second process at 128MB

pub fn spawn_process(path: &str, fd_inheritance: Option<&[(u8, u8)]>) -> Result<u64, String> {
    // 1. Parse Path
    let path_parts: Vec<&str> = path.split('/').collect();
    if path_parts.len() < 1 || !path_parts[0].starts_with('@') {
        return Err(String::from("Invalid path format"));
    }

    let disk_part = &path_parts[0][1..];
    let disk_id = if disk_part.starts_with("0x") || disk_part.starts_with("0X") {
        u8::from_str_radix(&disk_part[2..], 16).unwrap_or(0xFF)
    } else {
        disk_part.parse::<u8>().unwrap_or(0xFF)
    };

    let actual_path = if path_parts.len() > 1 { path_parts[1..].join("/") } else { String::from("") };

    // 2. Read File
    let mut file_buf = Vec::new();
    if let Ok(mut node) = crate::fs::vfs::open(disk_id, &actual_path) {
        let size = node.size();
        if size > 0 {
            file_buf.resize(size as usize, 0);
            if let Err(_) = node.read(0, &mut file_buf) {
                return Err(String::from("Failed to read file"));
            }
        } else {
            return Err(String::from("File empty"));
        }
    } else {
        return Err(String::from("File not found"));
    }

    // 3. Determine Load Base
    let load_base = unsafe {
        let base = NEXT_LOAD_BASE;
        NEXT_LOAD_BASE += 0x01000000; // Increment by 16MB
        base
    };
    
    // 4. Get PML4 (Shared for now)
    let pml4_phys = unsafe { (*(&raw const crate::boot::BOOT_INFO)).pml4 };

    // 5. Load ELF
    match crate::fs::elf::load_elf(&file_buf, pml4_phys, load_base) {
        Ok(entry_point) => {
            // 6. Prepare FD Table
            let mut new_fd_table = [-1i16; 16];
            
            let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            if tm.current_task >= 0 {
                let current_fds = tm.tasks[tm.current_task as usize].fd_table;
                
                if let Some(map) = fd_inheritance {
                    for &(child_fd, parent_fd) in map {
                        if (parent_fd as usize) < 16 && (child_fd as usize) < 16 {
                            new_fd_table[child_fd as usize] = current_fds[parent_fd as usize];
                        }
                    }
                } else {
                    // Inherit all if no map provided
                    new_fd_table = current_fds;
                }
            }
            drop(tm); 

            // INCREMENT REF COUNTS FOR INHERITED FDS
            for &g_fd in new_fd_table.iter() {
                if g_fd != -1 {
                    crate::fs::vfs::increment_ref(g_fd as usize);
                }
            }

            // 7. Create Task
            let pid = {
                let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
                tm.add_user_task(entry_point, pml4_phys, None, Some(new_fd_table))
            };
            
            Ok(pid as u64)
        },
        Err(e) => Err(e),
    }
}

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
                let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
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

             // 1. Open globally
             match crate::fs::vfs::open_file(disk_id, &actual_path_str) {
                 Ok(global_fd) => {
                     // 2. Assign locally
                     let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
                     let current = tm.current_task;
                     if current >= 0 {
                         let task = &mut tm.tasks[current as usize];
                         // Find free local slot
                         let mut local_fd = -1;
                         for i in 0..16 { // Reserve 0,1,2? No, let's just use all for now since 0,1 are Syscalls 0,1
                             if task.fd_table[i] == -1 {
                                 local_fd = i as i32;
                                 break;
                             }
                         }

                         if local_fd != -1 {
                             task.fd_table[local_fd as usize] = global_fd as i16;
                             context.rax = local_fd as u64;
                         } else {
                             // No local slots, close global (TODO: Implement close)
                             context.rax = u64::MAX; 
                         }
                     } else {
                         context.rax = global_fd as u64; // Fallback for kernel task?
                     }
                 },
                 Err(_) => context.rax = u64::MAX,
             }
        }

        62 => { // SYS_FILE_READ
             let local_fd = context.rdi as usize;
             let buf_ptr = context.rsi as *mut u8;
             let len = context.rdx as usize;
             
             if buf_ptr.is_null() { context.rax = u64::MAX; return; }
             
             // 1. Resolve Local -> Global
             let global_fd_opt = {
                 let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
                 let current = tm.current_task;
                 if current >= 0 && local_fd < 16 {
                     let g = tm.tasks[current as usize].fd_table[local_fd];
                     if g != -1 { Some(g as usize) } else { None }
                 } else {
                     None
                 }
             };

             if let Some(fd) = global_fd_opt {
                 let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, len) };
                 
                 if let Some(handle) = crate::fs::vfs::get_file(fd) {
                      use crate::fs::vfs::FileHandle;
                      match handle {
                          FileHandle::File { node, offset } => {
                              match node.read(*offset, buf) {
                                  Ok(n) => {
                                      *offset += n as u64;
                                      context.rax = n as u64;
                                  },
                                  Err(_) => context.rax = u64::MAX,
                              }
                          },
                          FileHandle::Pipe { pipe } => {
                              let n = pipe.read(buf);
                              context.rax = n as u64;
                          }
                      }
                 } else {
                     context.rax = u64::MAX;
                 }
             } else {
                 context.rax = u64::MAX;
             }
        }

        63 => { // SYS_FILE_WRITE
             let local_fd = context.rdi as usize;
             let buf_ptr = context.rsi as *const u8;
             let len = context.rdx as usize;

             if buf_ptr.is_null() { context.rax = u64::MAX; return; }

             // 1. Resolve Local -> Global
             let global_fd_opt = {
                 let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
                 let current = tm.current_task;
                 if current >= 0 && local_fd < 16 {
                     let g = tm.tasks[current as usize].fd_table[local_fd];
                     if g != -1 { Some(g as usize) } else { None }
                 } else {
                     None
                 }
             };

             if let Some(fd) = global_fd_opt {
                 let buf = unsafe { core::slice::from_raw_parts(buf_ptr, len) };
                 
                 if let Some(handle) = crate::fs::vfs::get_file(fd) {
                      use crate::fs::vfs::FileHandle;
                      match handle {
                          FileHandle::File { node, offset } => {
                              match node.write(*offset, buf) {
                                  Ok(n) => {
                                      *offset += n as u64;
                                      context.rax = n as u64;
                                  },
                                  Err(_) => context.rax = u64::MAX,
                              }
                          },
                          FileHandle::Pipe { pipe } => {
                              let n = pipe.write(buf);
                              context.rax = n as u64;
                          }
                      }
                 } else {
                     context.rax = u64::MAX;
                 }
             } else {
                 context.rax = u64::MAX;
             }
        }

        64 => { // SYS_GETDENTS
             let local_fd = context.rdi as usize;
             let buf_ptr = context.rsi as *mut u8;
             let len = context.rdx as usize;
             
             if buf_ptr.is_null() { context.rax = u64::MAX; return; }

             let global_fd_opt = {
                 let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
                 let current = tm.current_task;
                 if current >= 0 && local_fd < 16 {
                     let g = tm.tasks[current as usize].fd_table[local_fd];
                     if g != -1 { Some(g as usize) } else { None }
                 } else {
                     None
                 }
             };

             if let Some(fd) = global_fd_opt {
                 if let Some(handle) = crate::fs::vfs::get_file(fd) {
                      use crate::fs::vfs::{FileHandle, FileType};
                      match handle {
                          FileHandle::File { node, offset } => {
                              match node.children() {
                                  Ok(children) => {
                                      let start_idx = *offset as usize;
                                      if start_idx >= children.len() {
                                          context.rax = 0; // EOF
                                      } else {
                                          let mut bytes_written = 0;
                                          let mut count = 0;
                                          let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, len) };
                                          
                                          for child in children.iter().skip(start_idx) {
                                              let name = child.name();
                                              let name_bytes = name.as_bytes();
                                              let name_len = name_bytes.len();
                                              
                                              // Check buffer space: 1 (type) + 1 (len) + name_len
                                              if bytes_written + 2 + name_len > len {
                                                  break;
                                              }
                                              
                                              let type_byte = match child.kind() {
                                                  FileType::File => 1,
                                                  FileType::Directory => 2,
                                                  FileType::Device => 3,
                                                  _ => 0,
                                              };
                                              
                                              buf[bytes_written] = type_byte;
                                              buf[bytes_written + 1] = name_len as u8;
                                              buf[bytes_written + 2 .. bytes_written + 2 + name_len].copy_from_slice(name_bytes);
                                              
                                              bytes_written += 2 + name_len;
                                              count += 1;
                                          }
                                          
                                          *offset += count as u64;
                                          context.rax = bytes_written as u64;
                                      }
                                  },
                                  Err(_) => context.rax = u64::MAX,
                              }
                          },
                          FileHandle::Pipe { .. } => context.rax = u64::MAX, // Not a directory
                      }
                 } else {
                     context.rax = u64::MAX;
                 }
             } else {
                 context.rax = u64::MAX;
             }
        }

        65 => { // SYS_FILE_SIZE
             let local_fd = context.rdi as usize;
             
             let global_fd_opt = {
                 let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
                 let current = tm.current_task;
                 if current >= 0 && local_fd < 16 {
                     let g = tm.tasks[current as usize].fd_table[local_fd];
                     if g != -1 { Some(g as usize) } else { None }
                 } else {
                     None
                 }
             };

             if let Some(fd) = global_fd_opt {
                 if let Some(handle) = crate::fs::vfs::get_file(fd) {
                      use crate::fs::vfs::FileHandle;
                      match handle {
                          FileHandle::File { node, .. } => context.rax = node.size(),
                          FileHandle::Pipe { .. } => context.rax = 0,
                      }
                 } else {
                     context.rax = u64::MAX;
                 }
             } else {
                 context.rax = u64::MAX;
             }
        }
        
        42 => { // SYS_PIPE
            let fds_ptr = context.rdi as *mut i32;
            if fds_ptr.is_null() { context.rax = u64::MAX; return; }
            
            unsafe {
                use crate::fs::vfs::{OPEN_FILES, GLOBAL_FILE_REFCOUNT, FileHandle};
                use crate::fs::pipe::Pipe;
                
                let mut g1 = -1;
                let mut g2 = -1;
                
                // 1. Find Global Slots
                for i in 3..256 {
                    if OPEN_FILES[i].is_none() {
                        if g1 == -1 {
                            g1 = i as i32;
                        } else {
                            g2 = i as i32;
                            break;
                        }
                    }
                }
                
                if g1 != -1 && g2 != -1 {
                    // 2. Find Local Slots
                    let mut l1 = -1;
                    let mut l2 = -1;
                    
                    let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
                    let current = tm.current_task;
                    if current >= 0 {
                        let task = &mut tm.tasks[current as usize];
                        for i in 0..16 {
                            if task.fd_table[i] == -1 {
                                if l1 == -1 {
                                    l1 = i as i32;
                                } else {
                                    l2 = i as i32;
                                    break;
                                }
                            }
                        }
                    }
                    
                    if l1 != -1 && l2 != -1 {
                        // Success!
                        let pipe = Pipe::new();
                        OPEN_FILES[g1 as usize] = Some(FileHandle::Pipe { pipe: pipe.clone() });
                        OPEN_FILES[g2 as usize] = Some(FileHandle::Pipe { pipe });
                        GLOBAL_FILE_REFCOUNT[g1 as usize] = 1;
                        GLOBAL_FILE_REFCOUNT[g2 as usize] = 1;
                        
                        let task = &mut tm.tasks[current as usize];
                        task.fd_table[l1 as usize] = g1 as i16;
                        task.fd_table[l2 as usize] = g2 as i16;
                        
                        *fds_ptr.add(0) = l1;
                        *fds_ptr.add(1) = l2;
                        context.rax = 0;
                    } else {
                        // Rollback global (not needed since we didn't write to OPEN_FILES yet? 
                        // Wait, we didn't write to OPEN_FILES yet. Good.)
                         context.rax = u64::MAX; // EMFILE (Process)
                    }
                } else {
                    context.rax = u64::MAX; // ENFILE (System)
                }
            }
        }

        53 => { // SYS_GET_MOUSE_POS
            unsafe {
                let mouse = &*(&raw const MOUSE);
                context.rax = ((mouse.x as u64) << 32) | (mouse.y as u64);
            }
        }

        54 => { // SYS_GET_TIME
            let (h, m, s) = crate::drivers::rtc::get_time();
            context.rax = ((h as u64) << 16) | ((m as u64) << 8) | (s as u64);
        }

        SYS_EXIT => {
            let exit_code = context.rdi;
            debugln!("[Syscall] Process exited with code {}", exit_code);
            {
                // Clean up FDs for this task
                let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();                
                let current = tm.current_task;
                if current >= 0 {
                    let task = &mut tm.tasks[current as usize];
                    task.state = crate::interrupts::task::TaskState::Zombie;
                    
                    // Close all open FDs
                    for i in 0..16 {
                        let global = task.fd_table[i];
                        if global != -1 {
                             crate::fs::vfs::close_file(global as usize);
                             task.fd_table[i] = -1;
                        }
                    }
                }
            }
            // Yield to schedule the next task
            unsafe { core::arch::asm!("int 0x20"); }
            loop { unsafe { core::arch::asm!("hlt"); } }
        }


        66 => { // SYS_SPAWN
            let path_ptr = context.rdi as *const u8;
            let path_len = context.rsi as usize;
            let fd_map_ptr = context.rdx as *const (u8, u8); // child_fd, parent_fd
            let fd_map_len = context.r10 as usize;

            if path_ptr.is_null() || path_len == 0 {
                context.rax = u64::MAX;
                return;
            }

            let path_slice = unsafe { core::slice::from_raw_parts(path_ptr, path_len) };
            let path_str = String::from_utf8_lossy(path_slice);

            let fd_map = if !fd_map_ptr.is_null() && fd_map_len > 0 {
                unsafe { Some(core::slice::from_raw_parts(fd_map_ptr, fd_map_len)) }
            } else {
                None
            };

            match spawn_process(&path_str, fd_map) {
                Ok(pid) => context.rax = pid,
                Err(e) => {
                    crate::debugln!("Spawn Error: {}", e);
                    context.rax = u64::MAX;
                }
            }
        }

        67 => { // SYS_CLOSE
            let local_fd = context.rdi as usize;
            
            let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            let current = tm.current_task;
            if current >= 0 {
                let task = &mut tm.tasks[current as usize];
                if local_fd < 16 {
                    let global = task.fd_table[local_fd];
                    if global != -1 {
                        crate::fs::vfs::close_file(global as usize);
                        task.fd_table[local_fd] = -1;
                        context.rax = 0;
                    } else {
                        context.rax = u64::MAX; // EBADF
                    }
                } else {
                    context.rax = u64::MAX;
                }
            } else {
                context.rax = u64::MAX;
            }
        }
        
        75 => { // SYS_SEEK
             let local_fd = context.rdi as usize;
             let offset = context.rsi as i64;
             let whence = context.rdx as usize; // 0: SET, 1: CUR, 2: END
             
             let global_fd_opt = {
                 let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
                 let current = tm.current_task;
                 if current >= 0 && local_fd < 16 {
                     let g = tm.tasks[current as usize].fd_table[local_fd];
                     if g != -1 { Some(g as usize) } else { None }
                 } else {
                     None
                 }
             };

             if let Some(fd) = global_fd_opt {
                 if let Some(handle) = crate::fs::vfs::get_file(fd) {
                      use crate::fs::vfs::FileHandle;
                      match handle {
                          FileHandle::File { node, offset: current_offset } => {
                              let size = node.size() as i64;
                              let new_offset = match whence {
                                  0 => offset, // SEEK_SET
                                  1 => (*current_offset as i64) + offset, // SEEK_CUR
                                  2 => size + offset, // SEEK_END
                                  _ => -1,
                              };
                              
                              if new_offset >= 0 {
                                  *current_offset = new_offset as u64;
                                  context.rax = new_offset as u64;
                              } else {
                                  context.rax = u64::MAX; // EINVAL
                              }
                          },
                          FileHandle::Pipe { .. } => context.rax = u64::MAX, // ESPIPE
                      }
                 } else {
                     context.rax = u64::MAX;
                 }
             } else {
                 context.rax = u64::MAX;
             }
        }

        68 => { //SYS_WAIT_PID
            let target_pid = context.rdi as usize;
            let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            if target_pid < crate::interrupts::task::MAX_TASKS {
                let state = tm.tasks[target_pid].state;
                if state == crate::interrupts::task::TaskState::Ready {
                    context.rax = 1; // Still running
                } else {
                    context.rax = 0; // Finished
                }
            } else {
                context.rax = 0;
            }
        }

        70 => { // SYS_POLL
            let fds_ptr = context.rdi as *const PollFd;
            let nfds = context.rsi as usize;
            let timeout = context.rdx as i32; // Ignored for now (always non-blocking or simple yield)

            if fds_ptr.is_null() || nfds == 0 {
                context.rax = 0;
                return;
            }

            let mut poll_fds = unsafe { core::slice::from_raw_parts(fds_ptr, nfds).to_vec() }; // Clone to avoid mutation issues/races if userspace
            // Actually, we need to write back to revents.
            // Let's iterate and write back directly.
            
            let mut ready_count = 0;
            
            {
                let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
                let current = tm.current_task;
                
                if current >= 0 {
                    let task = &tm.tasks[current as usize];
                    
                    for i in 0..nfds {
                        unsafe {
                            let pfd = &mut *(fds_ptr.add(i) as *mut PollFd);
                            pfd.revents = 0;
                            
                            let fd = pfd.fd;
                            if fd >= 0 && (fd as usize) < 16 {
                                let global_fd = task.fd_table[fd as usize];
                                if global_fd != -1 {
                                    if let Some(handle) = crate::fs::vfs::get_file(global_fd as usize) {
                                        use crate::fs::vfs::FileHandle;
                                        match handle {
                                            FileHandle::Pipe { pipe } => {
                                                if (pfd.events & POLLIN) != 0 {
                                                    if pipe.available() > 0 {
                                                        pfd.revents |= POLLIN;
                                                    }
                                                }
                                                if (pfd.events & POLLOUT) != 0 {
                                                    // Always ready to write for now unless full?
                                                    pfd.revents |= POLLOUT;
                                                }
                                            },
                                            FileHandle::File { .. } => {
                                                // Files always ready?
                                                if (pfd.events & POLLIN) != 0 { pfd.revents |= POLLIN; }
                                                if (pfd.events & POLLOUT) != 0 { pfd.revents |= POLLOUT; }
                                            }
                                        }
                                    } else {
                                        pfd.revents = POLLERR;
                                    }
                                } else {
                                    pfd.revents = POLLNVAL;
                                }
                            } else {
                                pfd.revents = POLLNVAL;
                            }
                            
                            if pfd.revents != 0 {
                                ready_count += 1;
                            }
                        }
                    }
                }
            }
            
            context.rax = ready_count as u64;
        }

        71 | 72 => { // SYS_CREATE_FILE | SYS_CREATE_DIR
             let ptr = context.rdi as *const u8;
             let len = context.rsi as usize;
             let s = unsafe { core::slice::from_raw_parts(ptr, len) };
             let path_str_full = String::from_utf8_lossy(s);
             
             let path_parts: Vec<&str> = path_str_full.split('/').collect();
             if path_parts.len() < 1 || !path_parts[0].starts_with('@') {
                 context.rax = u64::MAX; 
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
             
             let actual_path = if path_parts.len() > 1 { path_parts[1..].join("/") } else { String::from("") };
             
             // Split parent and new name
             if let Some(last_slash) = actual_path.rfind('/') {
                 let parent_path = &actual_path[..last_slash];
                 let new_name = &actual_path[last_slash+1..];
                 
                 if let Ok(mut parent) = crate::fs::vfs::open(disk_id, parent_path) {
                     let res = if syscall_num == 71 {
                         parent.create_file(new_name)
                     } else {
                         parent.create_dir(new_name)
                     };
                     
                     match res {
                         Ok(_) => context.rax = 0,
                         Err(_) => context.rax = u64::MAX,
                     }
                 } else {
                     context.rax = u64::MAX;
                 }
             } else {
                 // Root level? "file.txt" -> parent is root
                 if let Ok(mut root) = crate::fs::vfs::open(disk_id, "") {
                     let res = if syscall_num == 71 {
                         root.create_file(&actual_path)
                     } else {
                         root.create_dir(&actual_path)
                     };
                     
                     match res {
                         Ok(_) => context.rax = 0,
                         Err(_) => context.rax = u64::MAX,
                     }
                 } else {
                     context.rax = u64::MAX;
                 }
             }
        }

        73 => { // SYS_REMOVE
             let ptr = context.rdi as *const u8;
             let len = context.rsi as usize;
             let s = unsafe { core::slice::from_raw_parts(ptr, len) };
             let path_str_full = String::from_utf8_lossy(s);
             
             let path_parts: Vec<&str> = path_str_full.split('/').collect();
             if path_parts.len() < 1 || !path_parts[0].starts_with('@') { context.rax = u64::MAX; return; }

             let disk_part = &path_parts[0][1..];
             let disk_id = if disk_part.starts_with("0x") || disk_part.starts_with("0X") {
                 u8::from_str_radix(&disk_part[2..], 16).unwrap_or(0xFF)
             } else {
                 disk_part.parse::<u8>().unwrap_or(0xFF)
             };

             let actual_path = if path_parts.len() > 1 { path_parts[1..].join("/") } else { String::from("") };
             
             if let Some(last_slash) = actual_path.rfind('/') {
                 let parent_path = &actual_path[..last_slash];
                 let name = &actual_path[last_slash+1..];
                 
                 if let Ok(mut parent) = crate::fs::vfs::open(disk_id, parent_path) {
                     match parent.remove(name) {
                         Ok(_) => context.rax = 0,
                         Err(_) => context.rax = u64::MAX,
                     }
                 } else { context.rax = u64::MAX; }
             } else {
                 if let Ok(mut root) = crate::fs::vfs::open(disk_id, "") {
                     match root.remove(&actual_path) {
                         Ok(_) => context.rax = 0,
                         Err(_) => context.rax = u64::MAX,
                     }
                 } else { context.rax = u64::MAX; }
             }
        }

        74 => { // SYS_RENAME
             let old_ptr = context.rdi as *const u8;
             let old_len = context.rsi as usize;
             let new_ptr = context.rdx as *const u8;
             let new_len = context.r10 as usize;
             
             let s_old = unsafe { core::slice::from_raw_parts(old_ptr, old_len) };
             let s_new = unsafe { core::slice::from_raw_parts(new_ptr, new_len) };
             let path_old = String::from_utf8_lossy(s_old);
             let path_new = String::from_utf8_lossy(s_new);
             
             // Simplification: Assume same disk and same parent for now
             let parts_old: Vec<&str> = path_old.split('/').collect();
             if parts_old.len() < 1 || !parts_old[0].starts_with('@') { context.rax = u64::MAX; return; }
             
             let disk_part = &parts_old[0][1..];
             let disk_id = if disk_part.starts_with("0x") || disk_part.starts_with("0X") {
                 u8::from_str_radix(&disk_part[2..], 16).unwrap_or(0xFF)
             } else {
                 disk_part.parse::<u8>().unwrap_or(0xFF)
             };
             
             let actual_old = if parts_old.len() > 1 { parts_old[1..].join("/") } else { String::from("") };
             
             // Parse New (assuming absolute for now)
             let parts_new: Vec<&str> = path_new.split('/').collect();
             let actual_new = if parts_new.len() > 1 { parts_new[1..].join("/") } else { String::from("") };

             // Extract parents
             let (parent_old, name_old) = if let Some(idx) = actual_old.rfind('/') {
                 (&actual_old[..idx], &actual_old[idx+1..])
             } else {
                 ("", actual_old.as_str())
             };

             let (parent_new, name_new) = if let Some(idx) = actual_new.rfind('/') {
                 (&actual_new[..idx], &actual_new[idx+1..])
             } else {
                 ("", actual_new.as_str())
             };
             
             if parent_old != parent_new {
                 debugln!("SYS_RENAME: Moving between directories not supported yet");
                 context.rax = u64::MAX; 
                 return;
             }
             
             if let Ok(mut parent) = crate::fs::vfs::open(disk_id, parent_old) {
                 match parent.rename(name_old, name_new) {
                     Ok(_) => context.rax = 0,
                     Err(_) => context.rax = u64::MAX,
                 }
             } else {
                 context.rax = u64::MAX;
             }
        }
        
        _ => {
            debugln!("[Syscall] Unknown syscall #{}", syscall_num);
            context.rax = u64::MAX;
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct PollFd {
    pub fd: i32,
    pub events: i16,
    pub revents: i16,
}

pub const POLLIN: i16 = 0x001;
pub const POLLOUT: i16 = 0x004;
pub const POLLERR: i16 = 0x008;
pub const POLLNVAL: i16 = 0x020;