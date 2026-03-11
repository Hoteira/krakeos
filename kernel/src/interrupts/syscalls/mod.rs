use crate::debugln;
use crate::interrupts::task::{CPUState, Process, TASK_MANAGER};
use crate::memory::address_space::{CODE_SLOT_SIZE, LINEAR_MEMORY_SLOT_SIZE, STACK_SLOT_SIZE};
use alloc::sync::Arc;
use core::arch::naked_asm;

pub mod fs;
pub use process::spawn_process;

pub mod event;
pub mod memory;
pub mod misc;
pub mod network;
pub mod process;
pub mod window;

/// Returns the current process (cloned Arc) without holding the task manager lock.
pub fn get_current_process() -> Option<Arc<Process>> {
    let tm = TASK_MANAGER.int_lock();
    let idx = tm.current_task_idx()?;
    tm.tasks[idx].as_ref()?.process.clone()
}

/// Returns true if the current thread is a kernel thread (Ring 0).
/// Kernel threads run the WASM runtime (interpreter/AOT) and are trusted —
/// they legitimately pass kernel heap and image pointers to syscalls.
/// User threads (Ring 3) can only access their process's SAS regions.
pub fn is_kernel_thread() -> bool {
    let tm = TASK_MANAGER.int_lock();
    if let Some(idx) = tm.current_task_idx() {
        if let Some(thread) = tm.tasks[idx].as_ref() {
            let result = thread.user_stack == 0;
            if !result {
                debugln!("[is_kernel_thread] TID {} user_stack={:#x} -> NOT kernel thread", idx, thread.user_stack);
            }
            return result;
        }
    }
    true // default to trusted if we can't determine (boot context)
}

/// Validates that [ptr, ptr+len) falls within the calling process's SAS regions.
/// Only applied to Ring 3 (user) threads. Kernel threads are trusted.
pub fn validate_user_ptr(proc: &Process, ptr: u64, len: u64) -> bool {
    if len == 0 {
        return true;
    }
    let end = match ptr.checked_add(len) {
        Some(e) => e,
        None => return false,
    };

    // Linear memory (WASM heap): [base, base + slot_size)
    let lm_base = proc.linear_memory_base;
    let lm_end = lm_base + LINEAR_MEMORY_SLOT_SIZE;
    if ptr >= lm_base && end <= lm_end {
        return true;
    }

    // User stack: [stack_top - slot_size, stack_top)
    let stack_top = proc.stack_base;
    let stack_bottom = stack_top - STACK_SLOT_SIZE;
    if ptr >= stack_bottom && end <= stack_top {
        return true;
    }

    // Code region: [code_base, code_base + slot_size)
    let code_end = proc.code_base + CODE_SLOT_SIZE;
    if ptr >= proc.code_base && end <= code_end {
        return true;
    }

    false
}

/// Validates that [ptr, ptr+len) is within the current process's valid memory.
/// Kernel threads (WASM runtime, Ring 0) are trusted and always pass.
/// User threads (Ring 3) must have pointers within their SAS regions.
/// On failure, sets context.rax = u64::MAX and returns false.
pub fn validate_user_buf(context: &mut CPUState, ptr: u64, len: u64) -> bool {
    if len == 0 {
        return true;
    }
    // Kernel threads run the trusted WASM runtime — skip validation
    if is_kernel_thread() {
        return true;
    }
    if let Some(proc) = get_current_process() {
        if validate_user_ptr(&proc, ptr, len) {
            return true;
        }
    }
    debugln!("[Syscall] REJECTED: invalid user pointer {:#x} len={}", ptr, len);
    context.rax = u64::MAX;
    false
}

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
pub const SYS_PREAD64: u64 = 17;
pub const SYS_PWRITE64: u64 = 18;
pub const SYS_PIPE: u64 = 22;
pub const SYS_NANOSLEEP: u64 = 35;
pub const SYS_GETPID: u64 = 39;
pub const SYS_SOCKET: u64 = 41;
pub const SYS_CONNECT: u64 = 42;
pub const SYS_ACCEPT: u64 = 43;
pub const SYS_SENDTO: u64 = 44;
pub const SYS_RECVFROM: u64 = 45;
pub const SYS_BIND: u64 = 49;
pub const SYS_LISTEN: u64 = 51;
pub const SYS_SOCKET_CLOSE: u64 = 50;
pub const SYS_TCP_SEND: u64 = 52;
pub const SYS_TCP_RECV: u64 = 53;
pub const SYS_TCP_CONNECT_FINISH: u64 = 54;
pub const SYS_EXECVE: u64 = 59;
pub const SYS_EXIT: u64 = 60;
pub const SYS_WAIT4: u64 = 61;
pub const SYS_KILL: u64 = 62;
pub const SYS_FCNTL: u64 = 72;
pub const SYS_GETDENTS: u64 = 78;
pub const SYS_CHDIR: u64 = 80;
pub const SYS_RENAME: u64 = 82;
pub const SYS_MKDIR: u64 = 83;
pub const SYS_RMDIR: u64 = 84;
pub const SYS_CREATE: u64 = 85;
pub const SYS_UNLINK: u64 = 87;
pub const SYS_LINKAT: u64 = 265;
pub const SYS_SYMLINKAT: u64 = 266;
pub const SYS_READLINKAT: u64 = 267;
pub const SYS_UTIMENSAT: u64 = 280;

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
pub const SYS_SHM_GET: u64 = 120;
pub const SYS_MMAP_FILE: u64 = 121;
pub const SYS_SHM_MAP: u64 = 122;
pub const SYS_YIELD: u64 = 129;

pub const SYS_FTRUNCATE: u64 = 77;

pub const SYS_SPAWN_THREAD: u64 = 112;
pub const SYS_THREAD_EXIT: u64 = 113;

pub const SYS_SPAWN_EXT: u64 = 114;
pub const SYS_GET_DATE: u64 = 115;
pub const SYS_DEBUG_PRINT: u64 = 999;
pub const SYS_MOUNT: u64 = 165;

pub const SYS_WAIT_FOR_EVENT: u64 = 130;
pub const SYS_REGISTER_EVENT: u64 = 131;
pub const SYS_SIGNAL_EVENT: u64 = 132;
pub const SYS_REGISTER_EVENT_QUEUE: u64 = 138;
pub const SYS_DEREGISTER_EVENT_QUEUE: u64 = 139;
pub const SYS_SET_NONBLOCK: u64 = 133;
pub const SYS_GET_TOTAL_MEM: u64 = 134;
pub const SYS_GET_USED_MEM: u64 = 135;
pub const SYS_GET_VMA_DUMP: u64 = 136;
pub const SYS_GET_SLOT_INFO: u64 = 137;

#[unsafe(naked)]
#[unsafe(no_mangle)]
pub extern "C" fn syscall_entry() {
    unsafe {
        naked_asm!(
            // 1. Save R15 to scratch
            "mov [rip + {scratch}], r15",
            // 2. Switch to kernel stack
            "mov r15, rsp",
            "mov rsp, [rip + {kernel_stack_ptr}]",
            // 3. Build the IRETQ frame (pushed by CPU normally, we do it manually for SYSCALL)
            "push QWORD PTR 0x1B", // SS (user_data 0x18 | RPL 3)
            "push r15",            // RSP
            "push r11",            // RFLAGS
            "push QWORD PTR 0x23", // CS (user_code_64 0x20 | RPL 3)
            "push rcx",            // RIP
            // 4. Restore R15 and push the rest of CPUState (r15 down to rbp)
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
            // 5. Call dispatcher
            "cld",
            "mov rdi, rsp",
            "call syscall_dispatcher",
            // 6. Restore all registers
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
            // 7. Return via IRETQ
            "iretq",
            kernel_stack_ptr = sym crate::interrupts::task::KERNEL_STACK_PTR,
            scratch = sym crate::interrupts::task::SCRATCH,
        );
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn syscall_dispatcher(
    num: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> u64 {
    let mut fake_context = CPUState {
        rax: num,
        rdi: arg1,
        rsi: arg2,
        rdx: arg3,
        r10: arg4,
        r8: arg5,
        r9: arg6,
        rbp: 0, rbx: 0, r12: 0, r13: 0, r14: 0, r15: 0,
        rip: 0, cs: 0, rflags: 0, rsp: 0, ss: 0,
        r11: 0, rcx: 0,
    };

    dispatch_syscall(&mut fake_context);
    fake_context.rax
}

pub fn dispatch_syscall(context: &mut CPUState) {
    let syscall_num = context.rax;

    context.rax = 0;

    match syscall_num {
        SYS_READ => fs::handle_read_file(context),
        SYS_WRITE => fs::handle_write_file(context),
        SYS_OPEN => fs::handle_open(context),
        SYS_CLOSE => fs::handle_close(context),
        SYS_STAT => fs::handle_stat(context, false),
        SYS_FSTAT => fs::handle_stat(context, true),
        SYS_POLL => fs::handle_poll(context),
        SYS_LSEEK => fs::handle_seek(context),
        SYS_MMAP => memory::handle_mmap(context),
        SYS_MUNMAP => memory::handle_munmap(context),
        SYS_BRK => memory::handle_brk(context),
        SYS_IOCTL => fs::handle_ioctl(context),
        SYS_PREAD64 => fs::handle_pread64(context),
        SYS_PWRITE64 => fs::handle_pwrite64(context),
        SYS_PIPE => fs::handle_pipe(context),
        SYS_NANOSLEEP => process::handle_sleep(context),
        SYS_GETPID => process::handle_getpid(context),
        SYS_SOCKET => network::handle_socket(context),
        SYS_CONNECT => network::handle_connect(context),
        SYS_ACCEPT => network::handle_accept(context),
        SYS_SENDTO => network::handle_sendto(context),
        SYS_RECVFROM => network::handle_recvfrom(context),
        SYS_BIND => network::handle_bind(context),
        SYS_LISTEN => network::handle_listen(context),
        SYS_SOCKET_CLOSE => network::handle_close_socket(context),
        SYS_TCP_SEND => network::handle_tcp_send(context),
        SYS_TCP_RECV => network::handle_tcp_recv(context),
        SYS_TCP_CONNECT_FINISH => network::handle_connect_finish(context),
        SYS_EXECVE => process::handle_spawn(context),
        SYS_EXIT => process::handle_exit(context),
        SYS_WAIT4 => process::handle_wait_pid(context),
        SYS_KILL => process::handle_kill(context),
        SYS_FCNTL => fs::handle_fcntl(context),
        SYS_GETDENTS => fs::handle_read_dir(context),
        SYS_CHDIR => fs::handle_chdir(context),
        SYS_RENAME => fs::handle_rename(context),
        SYS_MKDIR => fs::handle_create(context, 83),
        SYS_CREATE => fs::handle_create(context, 85),
        SYS_RMDIR => fs::handle_remove(context),
        SYS_UNLINK => fs::handle_remove(context),
        SYS_LINKAT => fs::handle_linkat(context),
        SYS_SYMLINKAT => fs::handle_symlinkat(context),
        SYS_READLINKAT => fs::handle_readlinkat(context),
        SYS_UTIMENSAT => fs::handle_utimensat(context),

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
        SYS_SHM_GET => memory::handle_shm_get(context),
        SYS_MMAP_FILE => fs::handle_mmap_file(context),
        SYS_SHM_MAP => memory::handle_shm_map(context),
        SYS_FTRUNCATE => fs::handle_ftruncate(context),

        SYS_SPAWN_THREAD => process::handle_spawn_thread(context),
        SYS_THREAD_EXIT => process::handle_thread_exit(context),
        SYS_SPAWN_EXT => process::handle_spawn_ext(context),

        SYS_DEBUG_PRINT => misc::handle_debug_print(context),
        SYS_MOUNT => {
            context.rax = 0;
        }

        SYS_WAIT_FOR_EVENT => event::handle_wait_for_event(context),
        SYS_REGISTER_EVENT => event::handle_register_event(context),
        SYS_SIGNAL_EVENT => event::handle_signal_event(context),
        SYS_SET_NONBLOCK => fs::handle_set_nonblock(context),
        SYS_GET_TOTAL_MEM => misc::handle_get_total_mem(context),
        SYS_GET_USED_MEM => misc::handle_get_used_mem(context),
        SYS_GET_VMA_DUMP => misc::handle_get_vma_dump(context),
        SYS_GET_SLOT_INFO => process::handle_get_slot_info(context),
        SYS_REGISTER_EVENT_QUEUE => window::handle_register_event_queue(context),
        SYS_DEREGISTER_EVENT_QUEUE => window::handle_deregister_event_queue(context),
        SYS_YIELD => {
            // No-op here, the dispatcher will return and the naked_asm will handle iretq.
            // Cooperative yielding is handled by the int 0x81 which actually switches.
            // But we must allow this syscall to prevent 'Unknown syscall' noise.
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
