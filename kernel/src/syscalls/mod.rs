use crate::memory::address_space::{CODE_SLOT_SIZE, LINEAR_MEMORY_SLOT_SIZE, STACK_SLOT_SIZE};
use crate::task::manager::TASK_MANAGER;
use crate::task::process::Process;
use crate::task::thread::CPUState;
use alloc::sync::Arc;
use core::arch::naked_asm;

pub mod event;
pub mod fs;
pub mod memory;
pub mod misc;
pub mod network;
pub mod process;
pub mod window;

pub use process::spawn_process;

pub fn get_current_process() -> Option<Arc<Process>> {
    let tm = TASK_MANAGER.int_lock();
    let idx = tm.current_task_idx()?;
    tm.tasks.get(&(idx))?.process.clone()
}

pub fn is_kernel_thread() -> bool {
    let idx = crate::task::cpu::get_current_task_idx();
    if idx < 0 { return true; } // Bootstrap/Initial
    
    let tm = TASK_MANAGER.int_lock();
    if let Some(thread) = tm.tasks.get(&(idx as usize)) {
        return thread.user_stack == 0;
    }
    true
}

pub fn validate_user_ptr(proc: &Process, ptr: u64, len: u64) -> bool {
    if len == 0 {
        return true;
    }
    let end = match ptr.checked_add(len) {
        Some(e) => e,
        None => return false,
    };

    let lm_base = proc.linear_memory_base;
    let lm_end = lm_base + LINEAR_MEMORY_SLOT_SIZE;
    if ptr >= lm_base && end <= lm_end {
        return true;
    }

    let stack_top = proc.stack_base;
    let stack_bottom = stack_top - STACK_SLOT_SIZE;
    if ptr >= stack_bottom && end <= stack_top {
        return true;
    }

    let code_end = proc.code_base + CODE_SLOT_SIZE;
    if ptr >= proc.code_base && end <= code_end {
        return true;
    }

    false
}

pub fn validate_user_buf(context: &mut CPUState, ptr: u64, len: u64) -> bool {
    if len == 0 {
        return true;
    }
    if is_kernel_thread() {
        return true;
    }
    // Allow kernel-space pointers (e.g. from host call dispatch via SYS_WASM_HOST_CALL)
    if ptr >= 0xFFFF_8000_0000_0000 {
        return true;
    }
    if let Some(proc) = get_current_process() {
        if validate_user_ptr(&proc, ptr, len) {
            return true;
        }
    }
    crate::debugln!(
        "[Syscall] REJECTED: invalid user pointer {:#x} len={}",
        ptr,
        len
    );
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
pub const SYS_GET_DMESG: u64 = 140;

pub const SYS_WASM_HOST_CALL: u64 = 300;
pub const SYS_WASM_MEMORY_INIT: u64 = 350;
pub const SYS_WASM_DATA_DROP: u64 = 351;
pub const SYS_WASM_TABLE_INIT: u64 = 352;
pub const SYS_WASM_ELEM_DROP: u64 = 353;
pub const SYS_WASM_TABLE_COPY: u64 = 354;
pub const SYS_WASM_TABLE_GROW: u64 = 355;
pub const SYS_WASM_TABLE_SIZE: u64 = 356;
pub const SYS_WASM_TABLE_FILL: u64 = 357;

pub const SYS_MEMORY_GROW: u64 = 200;
pub const SYS_MEMORY_SIZE: u64 = 201;
pub const SYS_ARGS_GET: u64 = 202;
pub const SYS_ARGS_SIZES_GET: u64 = 203;
pub const SYS_ENVIRON_GET: u64 = 204;
pub const SYS_ENVIRON_SIZES_GET: u64 = 205;
pub const SYS_CLOCK_RES_GET: u64 = 206;
pub const SYS_CLOCK_TIME_GET: u64 = 207;
pub const SYS_RANDOM_GET: u64 = 208;
pub const SYS_FD_PRESTAT_GET: u64 = 209;
pub const SYS_FD_PRESTAT_DIR_NAME: u64 = 210;
pub const SYS_PROC_RAISE: u64 = 211;

#[unsafe(naked)]
#[unsafe(no_mangle)]
pub extern "C" fn syscall_entry() {
    unsafe {
        naked_asm!(
            "mov gs:[16], r15",
            "mov r15, rsp",
            "mov rsp, gs:[8]",
            "push QWORD PTR 0x1B",
            "push r15",
            "push r11",
            "push QWORD PTR 0x23",
            "push rcx",
            "mov r15, gs:[16]",
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
            "and rsp, -16",
            "call dispatch_syscall",
            "mov rsp, rax",
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
        r11: 0,
        rcx: 0,
        rbp: 0,
        rbx: 0,
        r12: 0,
        r13: 0,
        r14: 0,
        r15: 0,
        rip: 0,
        cs: 0,
        rflags: 0,
        rsp: 0,
        ss: 0,
    };

    let _ = dispatch_syscall(&mut fake_context);
    fake_context.rax
}
#[unsafe(no_mangle)]
pub fn dispatch_syscall(context: &mut CPUState) -> u64 {
    let syscall_num = context.rax;
    let pid = crate::task::cpu::get_current_task_idx();

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
        SYS_GET_DMESG => misc::handle_get_dmesg(context),
        SYS_GET_SLOT_INFO => process::handle_get_slot_info(context),
        SYS_REGISTER_EVENT_QUEUE => window::handle_register_event_queue(context),
        SYS_DEREGISTER_EVENT_QUEUE => window::handle_deregister_event_queue(context),
        SYS_YIELD => {}

        SYS_WASM_MEMORY_INIT => {
            let ctx_ptr = context.rdi as *mut ::std::wasm::aot::runtime::Ring3Context;
            let d = context.rsi as i32;
            let s = context.rdx as i32;
            let n = context.r10 as u32;
            let data_idx = context.r8 as u32;
            ::std::wasm::aot::trampoline::aot_memory_init(unsafe { &*ctx_ptr }, d, s, n, data_idx);
        }
        SYS_WASM_DATA_DROP => {
            let ctx_ptr = context.rdi as *mut ::std::wasm::aot::runtime::Ring3Context;
            let data_idx = context.rsi as u32;
            ::std::wasm::aot::trampoline::aot_data_drop(unsafe { &*ctx_ptr }, data_idx);
        }
        SYS_WASM_TABLE_INIT => {
            let ctx_ptr = context.rdi as *mut ::std::wasm::aot::runtime::Ring3Context;
            let d = context.rsi as i32;
            let s = context.rdx as i32;
            let n = context.r10 as u32;
            let table_idx = context.r8 as u32;
            let elem_idx = context.r9 as u32;
            ::std::wasm::aot::trampoline::aot_table_init(
                unsafe { &*ctx_ptr },
                d,
                s,
                n,
                table_idx,
                elem_idx,
            );
        }
        SYS_WASM_ELEM_DROP => {
            let ctx_ptr = context.rdi as *mut ::std::wasm::aot::runtime::Ring3Context;
            let elem_idx = context.rsi as u32;
            ::std::wasm::aot::trampoline::aot_elem_drop(unsafe { &*ctx_ptr }, elem_idx);
        }
        SYS_WASM_TABLE_COPY => {
            let ctx_ptr = context.rdi as *mut ::std::wasm::aot::runtime::Ring3Context;
            let d = context.rsi as i32;
            let s = context.rdx as i32;
            let n = context.r10 as u32;
            let table_dst = context.r8 as u32;
            let table_src = context.r9 as u32;
            ::std::wasm::aot::trampoline::aot_table_copy(
                unsafe { &*ctx_ptr },
                d,
                s,
                n,
                table_dst,
                table_src,
            );
        }
        SYS_WASM_TABLE_GROW => {
            let ctx_ptr = context.rdi as *mut ::std::wasm::aot::runtime::Ring3Context;
            let val = context.rsi as usize;
            let n = context.rdx as u32;
            let table_idx = context.r10 as u32;
            context.rax = ::std::wasm::aot::trampoline::aot_table_grow(
                unsafe { &*ctx_ptr },
                val,
                n,
                table_idx,
            ) as u64;
        }
        SYS_WASM_TABLE_SIZE => {
            let ctx_ptr = context.rdi as *mut ::std::wasm::aot::runtime::Ring3Context;
            let table_idx = context.rsi as u32;
            context.rax =
                ::std::wasm::aot::trampoline::aot_table_size(unsafe { &*ctx_ptr }, table_idx)
                    as u64;
        }
        SYS_WASM_TABLE_FILL => {
            let ctx_ptr = context.rdi as *mut ::std::wasm::aot::runtime::Ring3Context;
            let d = context.rsi as i32;
            let val = context.rdx as usize;
            let n = context.r10 as u32;
            let table_idx = context.r8 as u32;
            ::std::wasm::aot::trampoline::aot_table_fill(
                unsafe { &*ctx_ptr },
                d,
                val,
                n,
                table_idx,
            );
        }

        SYS_MEMORY_GROW => memory::handle_memory_grow(context),
        SYS_MEMORY_SIZE => memory::handle_memory_size(context),
        SYS_ARGS_GET => misc::handle_args_get(context),
        SYS_ARGS_SIZES_GET => misc::handle_args_sizes_get(context),
        SYS_ENVIRON_GET => misc::handle_environ_get(context),
        SYS_ENVIRON_SIZES_GET => misc::handle_environ_sizes_get(context),
        SYS_CLOCK_RES_GET => misc::handle_clock_res_get(context),
        SYS_CLOCK_TIME_GET => misc::handle_clock_time_get(context),
        SYS_RANDOM_GET => misc::handle_random_get(context),
        SYS_FD_PRESTAT_GET => fs::handle_fd_prestat_get(context),
        SYS_FD_PRESTAT_DIR_NAME => fs::handle_fd_prestat_dir_name(context),
        SYS_PROC_RAISE => process::handle_exit(context), // Stub for now
        60 => process::handle_exit(context),             // Map to same handler

        999 => {
            let rdi = context.rdi;
            let rsi = context.rsi;
            let ptr = rdi as *const u8;
            let len = rsi as usize;
            let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
            let serial = crate::debug::SerialDebug::new();
            for &b in slice {
                serial.write_byte(b);
            }
            context.rax = len as u64;
        }

        _ => {
            crate::debugln!("[Syscall] Unknown syscall #{}", syscall_num);
            context.rax = u64::MAX;
        }
    }

    let ret_rax = context.rax;
    context as *mut CPUState as u64
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
