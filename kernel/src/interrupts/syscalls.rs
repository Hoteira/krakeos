use alloc::string::String;
use core::arch::{asm, naked_asm};
use crate::interrupts::task::CPUState;
use crate::debugln;
use crate::drivers::periferics::keyboard::KEYBOARD_BUFFER; 
use crate::memory::paging::{self, PAGE_USER, PAGE_WRITABLE};
use crate::composer::Window;
use crate::fs::vfs::FileSystem;

// Syscall Numbers
pub const SYS_READ: u64 = 0;
pub const SYS_PRINT: u64 = 1;
pub const SYS_MALLOC: u64 = 5;
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
                if let Some(char_code) = keyboard_buffer.pop_front() {
                    unsafe {
                        // Direct write since identity mapped
                        *user_ptr.add(bytes_written_to_user) = char_code as u8;
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
            if let Some(addr) = crate::memory::pmm::allocate_frames(pages) {
                context.rax = addr;
            } else {
                context.rax = 0;
            }
        }

        SYS_ADD_WINDOW => {
            let window_ptr = context.rdi as *const Window;
            unsafe {
                let w = *window_ptr;
                context.rax = (*(&raw mut crate::composer::COMPOSER)).add_window(w) as u64;
            }
        }

        SYS_UPDATE_WINDOW => {
            let window_ptr = context.rdi as *const Window;
            unsafe {
                let w = *window_ptr;
                (*(&raw mut crate::composer::COMPOSER)).resize_window(w);
                
                // Force redraw
                for j in (0..(*(&raw mut crate::composer::COMPOSER)).windows.len()).rev() {
                    match (*(&raw mut crate::composer::COMPOSER)).windows[j].w_type {
                        crate::composer::Items::Null => {}
                        _ => {
                            (*(&raw mut crate::composer::DISPLAY_SERVER)).copy_to_db(
                                (*(&raw mut crate::composer::COMPOSER)).windows[j].width as u32,
                                (*(&raw mut crate::composer::COMPOSER)).windows[j].height as u32,
                                (*(&raw mut crate::composer::COMPOSER)).windows[j].buffer,
                                (*(&raw mut crate::composer::COMPOSER)).windows[j].x as u32,
                                (*(&raw mut crate::composer::COMPOSER)).windows[j].y as u32,
                            );
                        }
                    }
                }
                (*(&raw mut crate::composer::DISPLAY_SERVER)).copy();
            }
            context.rax = 1;
        }

        SYS_GET_WIDTH => {
            unsafe {
                context.rax = (*(&raw mut crate::composer::DISPLAY_SERVER)).width;
            }
        }

        SYS_GET_HEIGHT => {
            unsafe {
                context.rax = (*(&raw mut crate::composer::DISPLAY_SERVER)).height;
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