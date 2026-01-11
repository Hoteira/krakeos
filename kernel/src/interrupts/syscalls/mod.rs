use crate::debugln;
use crate::interrupts::task::CPUState;
use core::arch::naked_asm;

pub mod fs;
pub use process::spawn_process;

pub mod process;
pub mod memory;
pub mod window;
pub mod misc;

pub const SYS_READ: u64 = 0;
pub const SYS_WRITE: u64 = 1;
pub const SYS_OPEN: u64 = 2;
pub const SYS_CLOSE: u64 = 3;
pub const SYS_STAT: u64 = 4;
pub const SYS_FSTAT: u64 = 5;
pub const SYS_POLL: u64 = 7;
pub const SYS_LSEEK: u64 = 8;
pub const SYS_MMAP: u64 = 9;
pub const SYS_MUNMAP: u64 = 11;
pub const SYS_BRK: u64 = 12;
pub const SYS_IOCTL: u64 = 16;
pub const SYS_PIPE: u64 = 22;
pub const SYS_NANOSLEEP: u64 = 35;
pub const SYS_GETPID: u64 = 39;
pub const SYS_EXECVE: u64 = 59;
pub const SYS_EXIT: u64 = 60;
pub const SYS_WAIT4: u64 = 61;
pub const SYS_KILL: u64 = 62;
pub const SYS_GETDENTS: u64 = 78;
pub const SYS_CHDIR: u64 = 80;
pub const SYS_RENAME: u64 = 82;
pub const SYS_MKDIR: u64 = 83;
pub const SYS_RMDIR: u64 = 84;
pub const SYS_CREATE: u64 = 85;
pub const SYS_UNLINK: u64 = 87;


pub const SYS_ADD_WINDOW: u64 = 100;
pub const SYS_REMOVE_WINDOW: u64 = 101;
pub const SYS_UPDATE_WINDOW: u64 = 102;
pub const SYS_UPDATE_WINDOW_AREA: u64 = 103;
pub const SYS_GET_EVENTS: u64 = 104;
pub const SYS_GET_MOUSE: u64 = 105;
pub const SYS_GET_SCREEN_WIDTH: u64 = 106;
pub const SYS_GET_SCREEN_HEIGHT: u64 = 107;
pub const SYS_GET_TIME: u64 = 108;
pub const SYS_GET_TICKS: u64 = 109;
pub const SYS_GET_PROCESS_LIST: u64 = 110;
pub const SYS_GET_PROCESS_MEM: u64 = 111;

pub const SYS_FTRUNCATE: u64 = 77;

pub const SYS_SPAWN_THREAD: u64 = 112;
pub const SYS_THREAD_EXIT: u64 = 113;

pub const SYS_SPAWN_EXT: u64 = 114;
pub const SYS_GET_DATE: u64 = 115;
pub const SYS_DEBUG_PRINT: u64 = 999;
pub const SYS_MOUNT: u64 = 165;

#[unsafe(naked)]
#[unsafe(no_mangle)]
pub extern "C" fn syscall_entry() {
    unsafe {
        naked_asm!(
            "mov [rip + {scratch}], r15",
            "mov r15, rsp",
            "mov rsp, [rip + {kernel_stack_ptr}]",
            "push QWORD PTR 0x23", 
            "push r15",
            "push r11",
            "push QWORD PTR 0x33", 
            "push rcx",
            "mov r15, [rip + {scratch}]",
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
        SYS_READ => fs::handle_read_file(context),
        SYS_WRITE => fs::handle_write_file(context),
        SYS_OPEN => fs::handle_open(context),
        SYS_CLOSE => fs::handle_close(context),
        SYS_STAT => fs::handle_file_size(context),
        SYS_FSTAT => fs::handle_file_size(context),
        SYS_POLL => fs::handle_poll(context),
        SYS_LSEEK => fs::handle_seek(context),
        SYS_MMAP => memory::handle_mmap(context),
        SYS_MUNMAP => memory::handle_munmap(context),
        SYS_BRK => memory::handle_brk(context),
        SYS_IOCTL => fs::handle_ioctl(context),
        SYS_PIPE => fs::handle_pipe(context),
        SYS_NANOSLEEP => process::handle_sleep(context),
        SYS_EXECVE => process::handle_spawn(context),
        SYS_EXIT => process::handle_exit(context),
        SYS_WAIT4 => process::handle_wait_pid(context),
        SYS_KILL => process::handle_kill(context),
        SYS_GETDENTS => fs::handle_read_dir(context),
        SYS_CHDIR => fs::handle_chdir(context),
        SYS_RENAME => fs::handle_rename(context),
        SYS_MKDIR => fs::handle_create(context, 83),
        SYS_CREATE => fs::handle_create(context, 85),
        SYS_RMDIR => fs::handle_remove(context),
        SYS_UNLINK => fs::handle_remove(context),

        SYS_ADD_WINDOW => window::handle_add_window(context),
        SYS_UPDATE_WINDOW => window::handle_update_window(context),
        SYS_UPDATE_WINDOW_AREA => window::handle_update_window_area(context),
        SYS_GET_EVENTS => window::handle_get_events(context),
        SYS_GET_SCREEN_WIDTH => window::handle_get_width(context),
        SYS_GET_SCREEN_HEIGHT => window::handle_get_height(context),
        SYS_GET_MOUSE => window::handle_get_mouse(context),
        SYS_GET_TIME => misc::handle_time(context),
        SYS_GET_DATE => misc::handle_date(context),
        SYS_GET_TICKS => misc::handle_ticks(context),
        SYS_GET_PROCESS_LIST => process::handle_get_process_list(context),
        SYS_GET_PROCESS_MEM => memory::handle_get_process_mem(context),
        SYS_FTRUNCATE => fs::handle_ftruncate(context),

        SYS_SPAWN_THREAD => process::handle_spawn_thread(context),
        SYS_THREAD_EXIT => process::handle_thread_exit(context),

        SYS_DEBUG_PRINT => misc::handle_debug_print(context),
        SYS_MOUNT => {
            context.rax = 0;
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
