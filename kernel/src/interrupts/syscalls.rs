use alloc::string::String;
use alloc::vec::Vec;
use core::arch::naked_asm;
use crate::interrupts::task::CPUState;
use crate::debugln;
use crate::drivers::periferics::keyboard::KEYBOARD_BUFFER; 
use crate::window_manager::display::DISPLAY_SERVER;
use crate::window_manager::composer::COMPOSER;
use crate::window_manager::input::MOUSE;
use crate::window_manager::window::Window;


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
pub const SYS_SLEEP: u64 = 76;

pub fn spawn_process(path: &str, fd_inheritance: Option<&[(u8, u8)]>) -> Result<u64, String> {
    
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

    let pml4_phys = unsafe { (*(&raw const crate::boot::BOOT_INFO)).pml4 };

    let pid_idx = crate::interrupts::task::TASK_MANAGER.lock().reserve_pid().map_err(|_| String::from("No free process slots"))?;
    let pid = pid_idx as u64;

    
    match crate::fs::elf::load_elf(&file_buf, pml4_phys, pid) {
        Ok(entry_point) => {
            
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
                    
                    new_fd_table = current_fds;
                }
            }
            drop(tm); 

            
            for &g_fd in new_fd_table.iter() {
                if g_fd != -1 {
                    crate::fs::vfs::increment_ref(g_fd as usize);
                }
            }

            
            let init_res = {
                let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
                tm.init_user_task(pid_idx, entry_point, pml4_phys, None, Some(new_fd_table))
            };
            
            match init_res {
                Ok(_) => Ok(pid),
                Err(_) => Err(String::from("Failed to init task")),
            }
        },
        Err(e) => {
            
            Err(e)
        },
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
            "push QWORD PTR 0x23", 
            "push r15",
            "push r11",
            "push QWORD PTR 0x33", 
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
            "cld", 
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
    
    context.rax = 0; 

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
                let mut w = *window_ptr;
                if let Some(current) = crate::interrupts::task::TASK_MANAGER.int_lock().current_task_idx() {
                    w.pid = current as u64;
                }
                context.rax = (*(&raw mut COMPOSER)).add_window(w) as u64;
            }
        }

        SYS_UPDATE_WINDOW => {
            let window_ptr = context.rdi as *const Window;
            unsafe {
                let w = *window_ptr;
                (*(&raw mut COMPOSER)).resize_window(w);
            }
            context.rax = 1;
        }

        
        52 => {
            let wid = context.rdi as u32;
            let buf_ptr = context.rsi as *mut crate::window_manager::events::Event;
            let max_events = context.rdx as usize;
            
            unsafe {
                use crate::window_manager::events::GLOBAL_EVENT_QUEUE;
                let events = GLOBAL_EVENT_QUEUE.lock().get_and_remove_events(wid, max_events);
                
                if !events.is_empty() {
                    
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
        
        61 => { 
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
             
             let actual_path_str = if path_parts.len() > 1 { path_parts[1..].join("/") } else { String::from("") };

             
             match crate::fs::vfs::open_file(disk_id, &actual_path_str) {
                 Ok(global_fd) => {
                     
                     let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
                     let current = tm.current_task;
                     if current >= 0 {
                         let task = &mut tm.tasks[current as usize];
                         
                         let mut local_fd = -1;
                         for i in 0..16 { 
                             if task.fd_table[i] == -1 {
                                 local_fd = i as i32;
                                 break;
                             }
                         }

                         if local_fd != -1 {
                             task.fd_table[local_fd as usize] = global_fd as i16;
                             context.rax = local_fd as u64;
                         } else {
                             
                             context.rax = u64::MAX; 
                         }
                     } else {
                         context.rax = global_fd as u64; 
                     }
                 },
                 Err(_) => context.rax = u64::MAX,
             }
        }

        62 => { 
             let local_fd = context.rdi as usize;
             let buf_ptr = context.rsi as *mut u8;
             let len = context.rdx as usize;
             
             if buf_ptr.is_null() { context.rax = u64::MAX; return; }
             
             
             let global_fd_opt = {
                 let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
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

        63 => { 
             let local_fd = context.rdi as usize;
             let buf_ptr = context.rsi as *const u8;
             let len = context.rdx as usize;

             if buf_ptr.is_null() { context.rax = u64::MAX; return; }

             
             let global_fd_opt = {
                 let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
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

        64 => { 
             let local_fd = context.rdi as usize;
             let buf_ptr = context.rsi as *mut u8;
             let len = context.rdx as usize;
             
             if buf_ptr.is_null() { context.rax = u64::MAX; return; }

             let global_fd_opt = {
                 let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
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
                                          context.rax = 0; 
                                      } else {
                                          let mut bytes_written = 0;
                                          let mut count = 0;
                                          let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, len) };
                                          
                                          for child in children.iter().skip(start_idx) {
                                              let name = child.name();
                                              let name_bytes = name.as_bytes();
                                              let name_len = name_bytes.len();
                                              
                                              
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
                          FileHandle::Pipe { .. } => context.rax = u64::MAX, 
                      }
                 } else {
                     context.rax = u64::MAX;
                 }
             } else {
                 context.rax = u64::MAX;
             }
        }

        65 => { 
             let local_fd = context.rdi as usize;
             
             let global_fd_opt = {
                 let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
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
        
        42 => { 
            let fds_ptr = context.rdi as *mut i32;
            if fds_ptr.is_null() { context.rax = u64::MAX; return; }
            
            unsafe {
                use crate::fs::vfs::{OPEN_FILES, GLOBAL_FILE_REFCOUNT, FileHandle};
                use crate::fs::pipe::Pipe;
                
                let mut g1 = -1;
                let mut g2 = -1;
                
                
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
                        
                        
                         context.rax = u64::MAX; 
                    }
                } else {
                    context.rax = u64::MAX; 
                }
            }
        }

        53 => { 
            unsafe {
                let mouse = &*(&raw const MOUSE);
                context.rax = ((mouse.x as u64) << 32) | (mouse.y as u64);
            }
        }

        54 => { 
            let (h, m, s) = crate::drivers::rtc::get_time();
            context.rax = ((h as u64) << 16) | ((m as u64) << 8) | (s as u64);
        }

        55 => { 
            unsafe {
                context.rax = crate::interrupts::task::SYSTEM_TICKS;
            }
        }

        SYS_EXIT => {
            let exit_code = context.rdi;
            debugln!("[Syscall] Process exited with code {}", exit_code);
            {

                let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
                let current = tm.current_task;
                if current >= 0 {
                    let task = &mut tm.tasks[current as usize];
                    task.exit_code = exit_code;
                    task.state = crate::interrupts::task::TaskState::Zombie;


                    unsafe {
                        (*(&raw mut COMPOSER)).remove_windows_by_pid(current as u64);
                    }


                    for i in 0..16 {
                        let global = task.fd_table[i];
                        if global != -1 {
                            crate::fs::vfs::close_file(global as usize);
                            task.fd_table[i] = -1;
                        }
                    }
                }
            }


            unsafe {
                core::arch::asm!("sti");
                loop { core::arch::asm!("hlt"); }
            }
        }


        66 => { 
            let path_ptr = context.rdi as *const u8;
            let path_len = context.rsi as usize;
            let fd_map_ptr = context.rdx as *const (u8, u8); 
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

        67 => { 
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
                        context.rax = u64::MAX; 
                    }
                } else {
                    context.rax = u64::MAX;
                }
            } else {
                context.rax = u64::MAX;
            }
        }
        
        75 => { 
             let local_fd = context.rdi as usize;
             let offset = context.rsi as i64;
             let whence = context.rdx as usize; 
             
             let global_fd_opt = {
                 let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
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
                                  0 => offset, 
                                  1 => (*current_offset as i64) + offset, 
                                  2 => size + offset, 
                                  _ => -1,
                              };
                              
                              if new_offset >= 0 {
                                  *current_offset = new_offset as u64;
                                  context.rax = new_offset as u64;
                              } else {
                                  context.rax = u64::MAX; 
                              }
                          },
                          FileHandle::Pipe { .. } => context.rax = u64::MAX, 
                      }
                 } else {
                     context.rax = u64::MAX;
                 }
             } else {
                 context.rax = u64::MAX;
             }
        }

        68 => { 
            let target_pid = context.rdi as usize;
            let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            if target_pid < crate::interrupts::task::MAX_TASKS {
                match tm.tasks[target_pid].state {
                    crate::interrupts::task::TaskState::Ready | crate::interrupts::task::TaskState::Reserved => {
                        context.rax = u64::MAX; 
                    }
                    crate::interrupts::task::TaskState::Zombie => {
                        let exit_code = tm.tasks[target_pid].exit_code;
                        context.rax = exit_code;
                        
                        
                        let pid = target_pid as u64;
                        let k_stack_top = tm.tasks[target_pid].kernel_stack;
                        
                        crate::memory::pmm::free_frames_by_pid(pid);
                        
                        
                        if k_stack_top != 0 {
                             let k_stack_start = k_stack_top - (4096 * 16);
                             crate::memory::pmm::free_frame(k_stack_start);
                        }

                        tm.tasks[target_pid] = crate::interrupts::task::NULL_TASK;
                    }
                    _ => {
                        context.rax = 0; 
                    }
                }
            } else {
                context.rax = 0;
            }
        }

        70 => { 
            let fds_ptr = context.rdi as *const PollFd;
            let nfds = context.rsi as usize;
            let _timeout = context.rdx as i32; 

            if fds_ptr.is_null() || nfds == 0 {
                context.rax = 0;
                return;
            }

            let _poll_fds = unsafe { core::slice::from_raw_parts(fds_ptr, nfds).to_vec() }; 
            
            
            
            let mut ready_count = 0;
            
            {
                let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
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
                                                    
                                                    pfd.revents |= POLLOUT;
                                                }
                                            },
                                            FileHandle::File { .. } => {
                                                
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

        71 | 72 => { 
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

        73 => { 
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

        74 => { 
             let old_ptr = context.rdi as *const u8;
             let old_len = context.rsi as usize;
             let new_ptr = context.rdx as *const u8;
             let new_len = context.r10 as usize;
             
             let s_old = unsafe { core::slice::from_raw_parts(old_ptr, old_len) };
             let s_new = unsafe { core::slice::from_raw_parts(new_ptr, new_len) };
             let path_old = String::from_utf8_lossy(s_old);
             let path_new = String::from_utf8_lossy(s_new);
             
             
             let parts_old: Vec<&str> = path_old.split('/').collect();
             if parts_old.len() < 1 || !parts_old[0].starts_with('@') { context.rax = u64::MAX; return; }
             
             let disk_part = &parts_old[0][1..];
             let disk_id = if disk_part.starts_with("0x") || disk_part.starts_with("0X") {
                 u8::from_str_radix(&disk_part[2..], 16).unwrap_or(0xFF)
             } else {
                 disk_part.parse::<u8>().unwrap_or(0xFF)
             };
             
             let actual_old = if parts_old.len() > 1 { parts_old[1..].join("/") } else { String::from("") };
             
             
             let parts_new: Vec<&str> = path_new.split('/').collect();
             let actual_new = if parts_new.len() > 1 { parts_new[1..].join("/") } else { String::from("") };

             
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
        
        76 => {
            let duration = context.rdi;
            let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            let current = tm.current_task;

            if current >= 0 {
                let task = &mut tm.tasks[current as usize];
                task.wake_ticks = unsafe { crate::interrupts::task::SYSTEM_TICKS } + duration;
                task.state = crate::interrupts::task::TaskState::Sleeping;
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