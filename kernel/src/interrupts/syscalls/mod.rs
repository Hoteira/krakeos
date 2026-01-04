pub mod memory;
pub mod window;
pub mod fs;
pub mod process;
pub mod system;

use core::arch::naked_asm;
use crate::interrupts::task::CPUState;

pub const SYS_READ: u64 = 0;
pub const SYS_PRINT: u64 = 1;
pub const SYS_MALLOC: u64 = 5;
pub const SYS_FREE: u64 = 6;
pub const SYS_PIPE: u64 = 42;
pub const SYS_ADD_WINDOW: u64 = 22;
pub const SYS_REMOVE_WINDOW: u64 = 23;
pub const SYS_GET_WIDTH: u64 = 44;
pub const SYS_GET_HEIGHT: u64 = 45;
pub const SYS_UPDATE_WINDOW: u64 = 51;
pub const SYS_GET_EVENTS: u64 = 52;
pub const SYS_GET_MOUSE: u64 = 53;
pub const SYS_GET_TIME: u64 = 54;
pub const SYS_GET_TICKS: u64 = 55;
pub const SYS_UPDATE_WINDOW_AREA: u64 = 56;
pub const SYS_EXIT: u64 = 60;
pub const SYS_OPEN: u64 = 61;
pub const SYS_READ_FILE: u64 = 62;
pub const SYS_WRITE_FILE: u64 = 63;
pub const SYS_READDIR: u64 = 64;
pub const SYS_STAT: u64 = 65;
pub const SYS_SPAWN: u64 = 66;
pub const SYS_CLOSE: u64 = 67;
pub const SYS_WAITPID: u64 = 68;
pub const SYS_POLL: u64 = 70;
pub const SYS_CREATE_FILE: u64 = 71;
pub const SYS_CREATE_DIR: u64 = 72;
pub const SYS_REMOVE: u64 = 73;
pub const SYS_RENAME: u64 = 74;
pub const SYS_LSEEK: u64 = 75;
pub const SYS_SLEEP: u64 = 76;
pub const SYS_GET_PROCESS_LIST: u64 = 77;
pub const SYS_KILL: u64 = 78;
pub const SYS_GET_PROCESS_MEM: u64 = 79;

// Re-export spawn_process for internal kernel use if needed
pub use process::spawn_process;

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
        SYS_READ => system::sys_read(context),
        SYS_PRINT => system::sys_print(context),
        SYS_MALLOC => memory::sys_malloc(context),
        SYS_FREE => memory::sys_free(context),
        SYS_ADD_WINDOW => window::sys_add_window(context),
        SYS_UPDATE_WINDOW => window::sys_update_window(context),
        SYS_UPDATE_WINDOW_AREA => window::sys_update_window_area(context),
        SYS_GET_EVENTS => window::sys_get_events(context),
        SYS_GET_WIDTH => window::sys_get_width(context),
        SYS_GET_HEIGHT => window::sys_get_height(context),
        SYS_OPEN => fs::sys_open(context),
        SYS_READ_FILE => fs::sys_read_file(context),
        SYS_WRITE_FILE => fs::sys_write_file(context),
        SYS_READDIR => fs::sys_readdir(context),
        SYS_STAT => fs::sys_stat(context),
        SYS_PIPE => fs::sys_pipe(context),
        SYS_CLOSE => fs::sys_close(context),
        SYS_LSEEK => fs::sys_lseek(context),
        SYS_POLL => fs::sys_poll(context),
        SYS_CREATE_FILE => fs::sys_create_fs_obj(context, false),
        SYS_CREATE_DIR => fs::sys_create_fs_obj(context, true),
        SYS_REMOVE => fs::sys_remove(context),
        SYS_RENAME => fs::sys_rename(context),
        SYS_GET_MOUSE => system::sys_get_mouse(context),
        SYS_GET_TIME => system::sys_get_time(context),
        SYS_GET_TICKS => system::sys_get_ticks(context),
        SYS_EXIT => process::sys_exit(context),
        SYS_SPAWN => process::sys_spawn(context),
        SYS_WAITPID => process::sys_waitpid(context),
        SYS_SLEEP => system::sys_sleep(context),
        SYS_GET_PROCESS_LIST => process::sys_get_process_list(context),
        SYS_KILL => process::sys_kill(context),
        SYS_GET_PROCESS_MEM => memory::sys_get_process_mem(context),
        _ => {
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
