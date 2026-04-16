use crate::context::Ring3Context;
use crate::syscall::{syscall1, syscall2, syscall3, syscall6};

// Kernel syscall numbers
const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_OPEN: u64 = 2;
const SYS_CLOSE: u64 = 3;
const SYS_FSTAT: u64 = 5;
const SYS_POLL: u64 = 7;
const SYS_IOCTL: u64 = 16;
const SYS_PIPE: u64 = 22;
const SYS_GETPID: u64 = 39;
const SYS_SPAWN: u64 = 59;
const SYS_EXIT: u64 = 60;
const SYS_WAITPID: u64 = 61;
const SYS_KILL: u64 = 62;
const SYS_CHDIR: u64 = 80;
const SYS_ADD_WINDOW: u64 = 100;
const SYS_UPDATE_WINDOW: u64 = 102;
const SYS_UPDATE_WINDOW_AREA: u64 = 103;
const SYS_GET_EVENTS: u64 = 104;
const SYS_GET_SCREEN_WIDTH: u64 = 106;
const SYS_GET_SCREEN_HEIGHT: u64 = 107;
const SYS_GET_TICKS: u64 = 109;
const SYS_GET_LIST: u64 = 110;
const SYS_SPAWN_THREAD: u64 = 112;
const SYS_SPAWN_EXT: u64 = 114;
const SYS_YIELD: u64 = 129;
const SYS_SET_NONBLOCK: u64 = 133;
const SYS_GET_SLOT_INFO: u64 = 137;
const SYS_REGISTER_EVENT_QUEUE: u64 = 138;
const SYS_DEREGISTER_EVENT_QUEUE: u64 = 139;
const SYS_DEBUG_PRINT: u64 = 999;

// =============================================================================
// KrakeOS Graphics/Screen
// =============================================================================

/// get-width() -> u32
#[no_mangle]
pub extern "C" fn krakeos_get_screen_width(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let w = syscall1(SYS_GET_SCREEN_WIDTH, 0);
        let result_sp = sp.sub(1);
        *result_sp = w as u128;
        result_sp
    }
}

/// get-height() -> u32
#[no_mangle]
pub extern "C" fn krakeos_get_screen_height(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let h = syscall1(SYS_GET_SCREEN_HEIGHT, 0);
        let result_sp = sp.sub(1);
        *result_sp = h as u128;
        result_sp
    }
}

// =============================================================================
// KrakeOS Window
// =============================================================================

/// create(attributes_ptr: u32) -> u64
/// Window struct has buffer/back_buffer/flipped pointers at offsets 8/16/24
/// that are WASM-relative and must be converted to absolute for the kernel.
#[no_mangle]
pub extern "C" fn krakeos_window_create(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let attr_offset = (*sp.add(0)) as u32;
        let abs_ptr = ctx.memory_base.add(attr_offset as usize);
        let base = ctx.memory_base as u64;

        // Fix buffer/back_buffer/flipped: convert WASM offsets to absolute addresses
        let buf_ptr = abs_ptr.add(8) as *mut u64;
        let bb_ptr = abs_ptr.add(16) as *mut u64;
        let fl_ptr = abs_ptr.add(24) as *mut u64;

        let orig_buf = core::ptr::read_unaligned(buf_ptr);
        let orig_bb = core::ptr::read_unaligned(bb_ptr);
        let orig_fl = core::ptr::read_unaligned(fl_ptr);

        if orig_buf != 0 { core::ptr::write_unaligned(buf_ptr, orig_buf + base); }
        if orig_bb != 0 { core::ptr::write_unaligned(bb_ptr, orig_bb + base); }
        if orig_fl != 0 { core::ptr::write_unaligned(fl_ptr, orig_fl + base); }

        let id = syscall1(SYS_ADD_WINDOW, abs_ptr as u64);

        // Restore original values so WASM memory is not corrupted
        core::ptr::write_unaligned(buf_ptr, orig_buf);
        core::ptr::write_unaligned(bb_ptr, orig_bb);
        core::ptr::write_unaligned(fl_ptr, orig_fl);

        let result_sp = sp.add(1).sub(1);
        *result_sp = id as u128;
        result_sp
    }
}

/// update(handle: u64, attributes_ptr: u32)
#[no_mangle]
pub extern "C" fn krakeos_window_update(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let attr_offset = (*sp.add(0)) as u32;
        let _handle = (*sp.add(1)) as u64;
        let abs_ptr = ctx.memory_base.add(attr_offset as usize);
        let base = ctx.memory_base as u64;

        // Fix buffer/back_buffer/flipped pointers
        let buf_ptr = abs_ptr.add(8) as *mut u64;
        let bb_ptr = abs_ptr.add(16) as *mut u64;
        let fl_ptr = abs_ptr.add(24) as *mut u64;

        let orig_buf = core::ptr::read_unaligned(buf_ptr);
        let orig_bb = core::ptr::read_unaligned(bb_ptr);
        let orig_fl = core::ptr::read_unaligned(fl_ptr);

        if orig_buf != 0 { core::ptr::write_unaligned(buf_ptr, orig_buf + base); }
        if orig_bb != 0 { core::ptr::write_unaligned(bb_ptr, orig_bb + base); }
        if orig_fl != 0 { core::ptr::write_unaligned(fl_ptr, orig_fl + base); }

        syscall1(SYS_UPDATE_WINDOW, abs_ptr as u64);

        // Restore
        core::ptr::write_unaligned(buf_ptr, orig_buf);
        core::ptr::write_unaligned(bb_ptr, orig_bb);
        core::ptr::write_unaligned(fl_ptr, orig_fl);

        sp.add(2)
    }
}


/// update-area(id: u64, x: u64, y: u64, w: u64, h: u64)
#[no_mangle]
pub extern "C" fn krakeos_window_update_area(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let h = (*sp.add(0)) as u64;
        let w = (*sp.add(1)) as u64;
        let y = (*sp.add(2)) as u64;
        let x = (*sp.add(3)) as u64;
        let id = (*sp.add(4)) as u64;
        crate::syscall::syscall5(SYS_UPDATE_WINDOW_AREA, id, x, y, w, h);
        sp.add(5)
    }
}

/// get-events(handle: u64, buf_ptr: u32, max: u32) -> i32
#[no_mangle]
pub extern "C" fn krakeos_window_get_events(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let max = (*sp.add(0)) as u32;
        let buf_offset = (*sp.add(1)) as u32;
        let _handle = (*sp.add(2)) as u64;
        let abs_buf = ctx.memory_base.add(buf_offset as usize);
        let ret = syscall3(SYS_GET_EVENTS, 0, abs_buf as u64, max as u64);
        let result_sp = sp.add(3).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// register-event-queue(header_ptr: u64, buf_ptr: u64, capacity: u64)
#[no_mangle]
pub extern "C" fn krakeos_register_event_queue(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let capacity = (*sp.add(0)) as u64;
        let buf_offset = (*sp.add(1)) as u64;
        let header_offset = (*sp.add(2)) as u64;
        let abs_header = ctx.memory_base.add(header_offset as usize);
        let abs_buf = ctx.memory_base.add(buf_offset as usize);
        syscall3(SYS_REGISTER_EVENT_QUEUE, abs_header as u64, abs_buf as u64, capacity);
        sp.add(3)
    }
}

/// deregister-event-queue()
#[no_mangle]
pub extern "C" fn krakeos_deregister_event_queue(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        syscall1(SYS_DEREGISTER_EVENT_QUEUE, 0);
        sp
    }
}

// =============================================================================
// KrakeOS Process
// =============================================================================

/// get-pid() -> u64
#[no_mangle]
pub extern "C" fn krakeos_get_pid(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let pid = syscall1(SYS_GETPID, 0);
        let result_sp = sp.sub(1);
        *result_sp = pid as u128;
        result_sp
    }
}

/// debug-print(s_ptr: u32, s_len: u64)
#[no_mangle]
pub extern "C" fn krakeos_debug_print(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let s_len = (*sp.add(0)) as u64;
        let s_ptr = (*sp.add(1)) as u32;
        let abs_ptr = ctx.memory_base.add(s_ptr as usize);
        syscall3(SYS_DEBUG_PRINT, abs_ptr as u64, s_len, 0);
        sp.add(2)
    }
}

/// yield()
#[no_mangle]
pub extern "C" fn krakeos_yield(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        syscall1(SYS_YIELD, 0);
        sp
    }
}

/// spawn(path_ptr: u32, path_len: u32, args_ptr: u32, args_len: u32, fds_ptr: u32, fds_len: u32) -> u64
#[no_mangle]
pub extern "C" fn krakeos_spawn(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let fds_len = (*sp.add(0)) as u64;
        let fds_ptr = (*sp.add(1)) as u32;
        let args_len = (*sp.add(2)) as u64;
        let args_ptr = (*sp.add(3)) as u32;
        let path_len = (*sp.add(4)) as u64;
        let path_ptr = (*sp.add(5)) as u32;
        let abs_path = ctx.memory_base.add(path_ptr as usize);
        let abs_args = ctx.memory_base.add(args_ptr as usize);
        let abs_fds = ctx.memory_base.add(fds_ptr as usize);
        let ret = syscall6(SYS_SPAWN, abs_path as u64, path_len, abs_args as u64, args_len, abs_fds as u64, fds_len);
        let result_sp = sp.add(6).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// waitpid(pid: u64) -> i32
#[no_mangle]
pub extern "C" fn krakeos_waitpid(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let pid = (*sp.add(0)) as u64;
        let ret = syscall1(SYS_WAITPID, pid);
        let result_sp = sp.add(1).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// pipe(fds_ptr: u32) -> i32
#[no_mangle]
pub extern "C" fn krakeos_pipe(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let fds_offset = (*sp.add(0)) as u32;
        let abs_ptr = ctx.memory_base.add(fds_offset as usize);
        let ret = syscall1(SYS_PIPE, abs_ptr as u64);
        let result_sp = sp.add(1).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// native-file-open(path_ptr: u32, path_len: u64, flags: u64) -> i64
#[no_mangle]
pub extern "C" fn krakeos_native_file_open(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let flags = (*sp.add(0)) as u64;
        let path_len = (*sp.add(1)) as u64;
        let path_ptr = (*sp.add(2)) as u32;
        let abs_path = ctx.memory_base.add(path_ptr as usize);
        let ret = syscall3(SYS_OPEN, abs_path as u64, path_len, flags);
        let result_sp = sp.add(3).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// native-file-stat(fd: u64, stat_ptr: u32) -> i32
#[no_mangle]
pub extern "C" fn krakeos_native_file_stat(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let stat_offset = (*sp.add(0)) as u32;
        let fd = (*sp.add(1)) as u64;
        let abs_stat = ctx.memory_base.add(stat_offset as usize);
        let ret = syscall3(SYS_FSTAT, fd, 0, abs_stat as u64);
        let result_sp = sp.add(2).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// file-read(fd: u64, buf_ptr: u32, len: u64) -> i64
#[no_mangle]
pub extern "C" fn krakeos_file_read(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let len = (*sp.add(0)) as u64;
        let buf_offset = (*sp.add(1)) as u32;
        let fd = (*sp.add(2)) as u64;
        let abs_buf = ctx.memory_base.add(buf_offset as usize);
        let ret = syscall3(SYS_READ, fd, abs_buf as u64, len);
        let result_sp = sp.add(3).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// file-write(fd: u64, buf_ptr: u32, len: u64) -> i64
#[no_mangle]
pub extern "C" fn krakeos_file_write(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let len = (*sp.add(0)) as u64;
        let buf_offset = (*sp.add(1)) as u32;
        let fd = (*sp.add(2)) as u64;
        let abs_buf = ctx.memory_base.add(buf_offset as usize);
        let ret = syscall3(SYS_WRITE, fd, abs_buf as u64, len);
        let result_sp = sp.add(3).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// kill(pid: u64, signal: u32) -> i32
#[no_mangle]
pub extern "C" fn krakeos_kill(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let signal = (*sp.add(0)) as u64;
        let pid = (*sp.add(1)) as u64;
        let ret = syscall2(SYS_KILL, pid, signal);
        let result_sp = sp.add(2).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// get-list(buf_ptr: u32, max_count: u64) -> u64
#[no_mangle]
pub extern "C" fn krakeos_get_list(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let max_count = (*sp.add(0)) as u64;
        let buf_offset = (*sp.add(1)) as u32;
        let abs_buf = ctx.memory_base.add(buf_offset as usize);
        let ret = syscall2(SYS_GET_LIST, abs_buf as u64, max_count);
        let result_sp = sp.add(2).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// chdir(path_ptr: u32, path_len: u64) -> i32
#[no_mangle]
pub extern "C" fn krakeos_chdir(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let path_len = (*sp.add(0)) as u64;
        let path_ptr = (*sp.add(1)) as u32;
        let abs_path = ctx.memory_base.add(path_ptr as usize);
        let ret = syscall2(SYS_CHDIR, abs_path as u64, path_len);
        let result_sp = sp.add(2).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// get-slot-info(buf_ptr: u32) -> i32
#[no_mangle]
pub extern "C" fn krakeos_get_slot_info(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let buf_offset = (*sp.add(0)) as u32;
        let abs_buf = ctx.memory_base.add(buf_offset as usize);
        let ret = syscall1(SYS_GET_SLOT_INFO, abs_buf as u64);
        let result_sp = sp.add(1).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// ioctl(fd: u64, request: u64, arg: u64) -> i32
#[no_mangle]
pub extern "C" fn krakeos_ioctl(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let arg = (*sp.add(0)) as u64;
        let request = (*sp.add(1)) as u64;
        let fd = (*sp.add(2)) as u64;
        let ret = syscall3(SYS_IOCTL, fd, request, arg);
        let result_sp = sp.add(3).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// set-nonblock(fd: u64, nonblock: u64) -> i32
#[no_mangle]
pub extern "C" fn krakeos_set_nonblock(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let nonblock = (*sp.add(0)) as u64;
        let fd = (*sp.add(1)) as u64;
        let ret = syscall2(SYS_SET_NONBLOCK, fd, nonblock);
        let result_sp = sp.add(2).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// poll(fds_ptr: u32, count: u64, timeout: u64) -> i32
#[no_mangle]
pub extern "C" fn krakeos_poll(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let timeout = (*sp.add(0)) as u64;
        let count = (*sp.add(1)) as u64;
        let fds_offset = (*sp.add(2)) as u32;
        let abs_fds = ctx.memory_base.add(fds_offset as usize);
        let ret = syscall3(SYS_POLL, abs_fds as u64, count, timeout);
        let result_sp = sp.add(3).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// get-current-user(ret_ptr: u32)
#[no_mangle]
pub extern "C" fn krakeos_get_current_user(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let ret_offset = (*sp.add(0)) as u32;
        let mem = ctx.memory_base;
        let user = b"racap";
        let dest = mem.add(ret_offset as usize);
        
        let str_offset = ret_offset + 8;
        let str_abs = mem.add(str_offset as usize);
        core::ptr::copy_nonoverlapping(user.as_ptr(), str_abs, user.len());
        
        core::ptr::write_unaligned(dest as *mut u32, str_offset);
        core::ptr::write_unaligned(dest.add(4) as *mut u32, user.len() as u32);
        sp.add(1)
    }
}

/// spawn-ext(name_ptr: u32, name_len: u32, state_ptr: u32) -> u64
#[no_mangle]
pub extern "C" fn krakeos_spawn_ext(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let state_offset = (*sp.add(0)) as u32;
        let name_len = (*sp.add(1)) as u64;
        let name_ptr = (*sp.add(2)) as u32;
        let abs_name = ctx.memory_base.add(name_ptr as usize);
        let abs_state = ctx.memory_base.add(state_offset as usize);
        let ret = syscall3(SYS_SPAWN_EXT, abs_name as u64, name_len, abs_state as u64);
        let result_sp = sp.add(3).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// spawn-thread(entry: u64, stack: u64) -> u64
#[no_mangle]
pub extern "C" fn krakeos_spawn_thread(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let stack = (*sp.add(0)) as u64;
        let entry = (*sp.add(1)) as u64;
        let ret = syscall2(SYS_SPAWN_THREAD, entry, stack);
        let result_sp = sp.add(2).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// thread-exit()
#[no_mangle]
pub extern "C" fn krakeos_thread_exit(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        syscall1(SYS_EXIT, 0);
    }
    loop {}
}

/// syscall(num: u64, arg1: u64, arg2: u64, arg3: u64) -> u64
#[no_mangle]
pub extern "C" fn krakeos_syscall(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let arg3 = (*sp.add(0)) as u64;
        let arg2 = (*sp.add(1)) as u64;
        let arg1 = (*sp.add(2)) as u64;
        let num = (*sp.add(3)) as u64;
        let ret = syscall3(num, arg1, arg2, arg3);
        let result_sp = sp.add(4).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

// =============================================================================
// KrakeOS Memory
// =============================================================================

/// shm-get(name_ptr: u32, name_len: u32, size: u32) -> u64
#[no_mangle]
pub extern "C" fn krakeos_shm_get(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let size = (*sp.add(0)) as u64;
        let name_len = (*sp.add(1)) as u64;
        let name_ptr = (*sp.add(2)) as u32;
        let abs_name = ctx.memory_base.add(name_ptr as usize);
        let ret = syscall3(120, abs_name as u64, name_len, size);
        let result_sp = sp.add(3).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// brk(addr: u64) -> u64
#[no_mangle]
pub extern "C" fn krakeos_brk(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let addr = (*sp.add(0)) as u64;
        let ret = syscall1(12, addr);
        let result_sp = sp.add(1).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// get-total-mem() -> u64
#[no_mangle]
pub extern "C" fn krakeos_get_total_mem(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let ret = syscall1(134, 0);
        let result_sp = sp.sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// get-used-mem() -> u64
#[no_mangle]
pub extern "C" fn krakeos_get_used_mem(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let ret = syscall1(135, 0);
        let result_sp = sp.sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

/// get-vma-dump(buf_ptr: u32, len: u64) -> u64
#[no_mangle]
pub extern "C" fn krakeos_get_vma_dump(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let len = (*sp.add(0)) as u64;
        let buf_offset = (*sp.add(1)) as u32;
        let abs_buf = ctx.memory_base.add(buf_offset as usize);
        let ret = syscall2(136, abs_buf as u64, len);
        let result_sp = sp.add(2).sub(1);
        *result_sp = ret as u128;
        result_sp
    }
}

// =============================================================================
// KrakeOS Terminal
// =============================================================================

/// set-window-size(handle: i32, w: i32, h: i32, flags: i32)
#[no_mangle]
pub extern "C" fn krakeos_terminal_set_window_size(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let flags = (*sp.add(0)) as u64;
        let h = (*sp.add(1)) as u64;
        let w = (*sp.add(2)) as u64;
        let handle = (*sp.add(3)) as u64;
        // SYS_IOCTL or a specific terminal syscall? 
        // Based on wasi/preview2/mod.rs, it maps to crate::os::krakeos::wasi::terminal_set_window_size
        // which likely uses a specific internal mechanism. 
        // For now, let's use a dummy syscall or the one identified in the plan if available.
        // Actually, looking at mod.rs, it seems these are host-only for now.
        // We'll just consume the args to keep stack integrity.
        sp.add(4)
    }
}

/// get-window-size(handle: i32, result_ptr: i32)
#[no_mangle]
pub extern "C" fn krakeos_terminal_get_window_size(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_offset = (*sp.add(0)) as u32;
        let _handle = (*sp.add(1)) as i32;
        let mem = ctx.memory_base;
        // Write dummy 80x25
        core::ptr::write_unaligned(mem.add(result_offset as usize) as *mut u32, 80);
        core::ptr::write_unaligned(mem.add(result_offset as usize + 4) as *mut u32, 25);
        sp.add(2)
    }
}

// =============================================================================
// KrakeOS Debug
// =============================================================================

/// get-process-list(result_ptr: i32)
#[no_mangle]
pub extern "C" fn krakeos_debug_get_process_list(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_offset = (*sp.add(0)) as u32;
        let abs_ptr = ctx.memory_base.add(result_offset as usize);
        let count = syscall2(SYS_GET_LIST, abs_ptr as u64, 64); // max 64
        // Result is usually a list (ptr, len)
        // For now, just consume the arg
        sp.add(1)
    }
}

/// kill(pid: u64, result_ptr: u32)
#[no_mangle]
pub extern "C" fn krakeos_debug_kill(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        sp.add(2)
    }
}

/// dump-vma(result_ptr: u32)
#[no_mangle]
pub extern "C" fn krakeos_debug_dump_vma(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        sp.add(1)
    }
}

/// get-memory-usage() -> (u64, u64)
#[no_mangle]
pub extern "C" fn krakeos_debug_get_memory_usage(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_sp = sp.sub(2);
        *result_sp = 0;
        *result_sp.add(1) = 0;
        result_sp
    }
}

// =============================================================================
// KrakeOS Container
// =============================================================================

/// plant(wasm_ptr: u32, wasm_len: u32, offset: u32, size: u32, fds_ptr: u32, fds_len: u32) -> u64
#[no_mangle]
pub extern "C" fn krakeos_container_plant(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        // consumes 6 args, returns 1
        let result_sp = sp.add(6).sub(1);
        *result_sp = 0; // Fails
        result_sp
    }
}

/// plant-from-path(path_ptr: u32, path_len: u32, offset: u32, size: u32, fds_ptr: u32, fds_len: u32) -> u64
#[no_mangle]
pub extern "C" fn krakeos_container_plant_from_path(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let result_sp = sp.add(6).sub(1);
        *result_sp = 0;
        result_sp
    }
}

/// harvest(child_id: u64, result_ptr: u32)
#[no_mangle]
pub extern "C" fn krakeos_container_harvest(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        sp.add(2)
    }
}

/// list-children(result_ptr: u32)
#[no_mangle]
pub extern "C" fn krakeos_container_list_children(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        sp.add(1)
    }
}

/// kill-child(id: i64, result_ptr: i32)
#[no_mangle]
pub extern "C" fn krakeos_container_kill_child(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let _result_ptr = (*sp.add(0)) as u32;
        let id = (*sp.add(1)) as u64;
        syscall2(SYS_KILL, id, 9); // SIGKILL
        sp.add(2)
    }
}

// =============================================================================
// Generic no-op (for __wasi_init_tp, __wasm_call_dtors, etc.)
// =============================================================================

/// No-op stub that consumes 0 args and returns nothing.
#[no_mangle]
pub extern "C" fn krakeos_noop(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    sp
}

/// No-op stub that consumes 1 arg and returns nothing.
#[no_mangle]
pub extern "C" fn krakeos_noop1(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe { sp.add(1) }
}

/// No-op stub that consumes 2 args and returns nothing.
#[no_mangle]
pub extern "C" fn krakeos_noop2(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe { sp.add(2) }
}
