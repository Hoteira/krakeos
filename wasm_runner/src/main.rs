#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use core::panic::PanicInfo;
use linked_list_allocator::LockedHeap;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

const HEAP_SIZE: usize = 48 * 1024 * 1024; // 48 MB heap

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let msg = b"wasm_runner: PANIC!\n";
    sys_write(1, 0, msg);
    
    // Convert location to string manually (limited)
    if let Some(loc) = info.location() {
        let file = loc.file().as_bytes();
        sys_write(1, 0, file);
        sys_write(1, 0, b":");
        // We could print the line number, but just file is enough to find the unwrap
    }
    
    sys_exit(1);
    loop {}
}

// System calls via ecall

pub fn sys_exit(code: usize) {
    unsafe { core::arch::asm!("ecall", in("a7") 1, in("a0") code); }
}

pub fn sys_read(fd: usize, offset: usize, buf: &mut [u8]) -> usize {
    let mut ret: usize;
    unsafe {
        core::arch::asm!("ecall", in("a7") 2, inout("a0") fd => ret, in("a1") buf.as_mut_ptr() as usize, in("a2") buf.len(), in("a5") offset);
    }
    ret
}

pub fn sys_write(fd: usize, offset: usize, buf: &[u8]) -> usize {
    let mut ret: usize;
    unsafe {
        core::arch::asm!("ecall", in("a7") 3, inout("a0") fd => ret, in("a1") buf.as_ptr() as usize, in("a2") buf.len(), in("a5") offset);
    }
    ret
}

pub fn sys_open(path: &str, flags: usize) -> usize {
    let mut ret: usize;
    unsafe {
        core::arch::asm!("ecall", in("a7") 4, inout("a0") path.as_ptr() as usize => ret, in("a1") path.len(), in("a2") flags);
    }
    ret
}

pub fn sys_sbrk(size: usize) -> usize {
    let mut ret: usize;
    unsafe {
        core::arch::asm!("ecall", in("a7") 5, inout("a0") size => ret);
    }
    ret
}

pub fn sys_spawn(entry: usize, arg: usize) {
    unsafe { core::arch::asm!("ecall", in("a7") 12, in("a0") entry, in("a1") arg); }
}

pub fn sys_wait_fs_event(fd: usize) -> usize {
    let mut ret: usize;
    unsafe {
        core::arch::asm!("ecall", in("a7") 11, inout("a0") fd => ret);
    }
    ret
}

fn run_wasm(path: &str) {
    let msg = b"wasm_runner: opening module...\n";
    sys_write(1, 0, msg);
    
    let fd = sys_open(path, 0);
    if fd == usize::MAX {
        sys_write(1, 0, b"wasm_runner: failed to open module!\n");
        sys_exit(1);
    }
    
    let mut wasm_bytes = alloc::vec![0u8; 1024 * 1024 * 10]; 
    let bytes_read = sys_read(fd, 0, &mut wasm_bytes);
    wasm_bytes.truncate(bytes_read);
    
    if bytes_read == 0 {
        sys_write(1, 0, b"wasm_runner: read 0 bytes!\n");
        sys_exit(1);
    }
    
    use wasmi::*;
    let engine = Engine::default();
    let module = match Module::new(&engine, &wasm_bytes[..]) {
        Ok(m) => m,
        Err(_) => {
            sys_write(1, 0, b"wasm_runner: Failed to parse module!\n");
            sys_exit(1);
            loop {}
        }
    };
    
    struct HostState {
        fd_offsets: [usize; 2048],
    }
    let mut store = Store::new(&engine, HostState { fd_offsets: [0; 2048] });
    let mut linker = <Linker<HostState>>::new(&engine);
    
    let fd_write = Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, out_fd: i32, iovs_ptr: i32, iovs_len: i32, nwritten_ptr: i32| -> i32 {
        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
        let mut total_written = 0;
        let mut iovs_buf = [0u8; 8];
        for i in 0..iovs_len {
            let iov_addr = (iovs_ptr + i * 8) as usize;
            if memory.read(&caller, iov_addr, &mut iovs_buf).is_err() { return 21; }
            let buf_ptr = u32::from_le_bytes([iovs_buf[0], iovs_buf[1], iovs_buf[2], iovs_buf[3]]) as usize;
            let buf_len = u32::from_le_bytes([iovs_buf[4], iovs_buf[5], iovs_buf[6], iovs_buf[7]]) as usize;
            
            let mut data = alloc::vec![0u8; buf_len];
            if memory.read(&caller, buf_ptr, &mut data).is_err() { return 21; }
            
            let out_fd_idx = out_fd as usize;
            let offset = caller.data().fd_offsets.get(out_fd_idx).copied().unwrap_or(0);
            
            // let msg = alloc::format!("wasm_runner: write to {} at offset {}\n", out_fd_idx, offset);
            // sys_write(1, 0, msg.as_bytes());
            
            let bw = sys_write(out_fd_idx, offset, &data);
            total_written += bw;
            
            if out_fd_idx < 2048 {
                caller.data_mut().fd_offsets[out_fd_idx] = offset + bw;
            }
            
            if bw < buf_len { break; }
        }
        let nwritten_bytes = (total_written as u32).to_le_bytes();
        let _ = memory.write(&mut caller, nwritten_ptr as usize, &nwritten_bytes);
        0
    });
    let _ = linker.define("wasi_snapshot_preview1", "fd_write", fd_write);

    let fd_read = Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, in_fd: i32, iovs_ptr: i32, iovs_len: i32, nread_ptr: i32| -> i32 {
        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
        let mut total_read = 0;
        let mut iovs_buf = [0u8; 8];
        for i in 0..iovs_len {
            let iov_addr = (iovs_ptr + i * 8) as usize;
            if memory.read(&caller, iov_addr, &mut iovs_buf).is_err() { return 21; }
            let buf_ptr = u32::from_le_bytes([iovs_buf[0], iovs_buf[1], iovs_buf[2], iovs_buf[3]]) as usize;
            let buf_len = u32::from_le_bytes([iovs_buf[4], iovs_buf[5], iovs_buf[6], iovs_buf[7]]) as usize;
            
            let mut data = alloc::vec![0u8; buf_len];
            
            let in_fd_idx = in_fd as usize;
            let offset = caller.data().fd_offsets.get(in_fd_idx).copied().unwrap_or(0);
            
            let br = sys_read(in_fd_idx, offset, &mut data);
            if br > 0 {
                let _ = memory.write(&mut caller, buf_ptr, &data[..br]);
                total_read += br;
                
                if in_fd_idx < 2048 {
                    caller.data_mut().fd_offsets[in_fd_idx] = offset + br;
                }
            }
            if br < buf_len { break; }
        }
        let nread_bytes = (total_read as u32).to_le_bytes();
        let _ = memory.write(&mut caller, nread_ptr as usize, &nread_bytes);
        0
    });
    let _ = linker.define("wasi_snapshot_preview1", "fd_read", fd_read);
    let fd_fdstat_get = Func::wrap(&mut store, |mut _caller: Caller<'_, HostState>, _fd: i32, _stat_ptr: i32| -> i32 { 0 });
    let _ = linker.define("wasi_snapshot_preview1", "fd_fdstat_get", fd_fdstat_get);
    
    let fd_filestat_get = Func::wrap(&mut store, |mut _caller: Caller<'_, HostState>, _fd: i32, _buf_ptr: i32| -> i32 { 0 });
    let _ = linker.define("wasi_snapshot_preview1", "fd_filestat_get", fd_filestat_get);

    let fd_prestat_get = Func::wrap(&mut store, |mut _caller: Caller<'_, HostState>, _fd: i32, _buf_ptr: i32| -> i32 { 8 });
    let _ = linker.define("wasi_snapshot_preview1", "fd_prestat_get", fd_prestat_get);

    let fd_prestat_dir_name = Func::wrap(&mut store, |mut _caller: Caller<'_, HostState>, _fd: i32, _path_ptr: i32, _path_len: i32| -> i32 { 8 });
    let _ = linker.define("wasi_snapshot_preview1", "fd_prestat_dir_name", fd_prestat_dir_name);

    let path_filestat_get = Func::wrap(&mut store, |mut _caller: Caller<'_, HostState>, _dirfd: i32, _flags: i32, _path_ptr: i32, _path_len: i32, _buf_ptr: i32| -> i32 { 8 });
    let _ = linker.define("wasi_snapshot_preview1", "path_filestat_get", path_filestat_get);

    let fd_seek = Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, fd: i32, offset: i64, whence: i32, newoffset_ptr: i32| -> i32 { 
        let fd_idx = fd as usize;
        if fd_idx < 2048 {
            let current = caller.data().fd_offsets[fd_idx];
            let new_offset = if whence == 0 { // SET
                offset as usize
            } else if whence == 1 { // CUR
                (current as i64 + offset) as usize
            } else { // END (not properly supported without stat)
                offset as usize
            };
            caller.data_mut().fd_offsets[fd_idx] = new_offset;
            
            // let msg = alloc::format!("wasm_runner: seek fd {} offset {} whence {} -> new_offset {}\n", fd_idx, offset, whence, new_offset);
            // sys_write(1, 0, msg.as_bytes());
            
            let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
            let _ = memory.write(&mut caller, newoffset_ptr as usize, &(new_offset as u64).to_le_bytes());
            0
        } else {
            8 // EBADF
        }
    });
    let _ = linker.define("wasi_snapshot_preview1", "fd_seek", fd_seek);
    let fd_close = Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, fd: i32| -> i32 {
        let fd_idx = fd as usize;
        if fd_idx < 2048 {
            caller.data_mut().fd_offsets[fd_idx] = 0;
        }
        0
    });
    let _ = linker.define("wasi_snapshot_preview1", "fd_close", fd_close);
    let clock_time_get = Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, _id: i32, _precision: i64, time_ptr: i32| -> i32 {
        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
        let time_bytes = (0u64).to_le_bytes();
        let _ = memory.write(&mut caller, time_ptr as usize, &time_bytes);
        0
    });
    let _ = linker.define("wasi_snapshot_preview1", "clock_time_get", clock_time_get);
    let wasi_init_tp = Func::wrap(&mut store, |mut _caller: Caller<'_, HostState>| {});
    let _ = linker.define("env", "__wasi_init_tp", wasi_init_tp);
    let wasm_call_dtors = Func::wrap(&mut store, |mut _caller: Caller<'_, HostState>| {});
    let _ = linker.define("env", "__wasm_call_dtors", wasm_call_dtors);
    let wasi_proc_exit = Func::wrap(&mut store, |mut _caller: Caller<'_, HostState>, _r: i32| {});
    let _ = linker.define("env", "__wasi_proc_exit", wasi_proc_exit);

    let random_get = Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, buf_ptr: i32, buf_len: i32| -> i32 {
        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
        let zeroes = alloc::vec![0u8; buf_len as usize];
        let _ = memory.write(&mut caller, buf_ptr as usize, &zeroes);
        0
    });
    let _ = linker.define("wasi_snapshot_preview1", "random_get", random_get);

    let environ_sizes_get = Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, count_ptr: i32, size_ptr: i32| -> i32 {
        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
        let _ = memory.write(&mut caller, count_ptr as usize, &0u32.to_le_bytes());
        let _ = memory.write(&mut caller, size_ptr as usize, &0u32.to_le_bytes());
        0
    });
    let _ = linker.define("wasi_snapshot_preview1", "environ_sizes_get", environ_sizes_get);

    let environ_get = Func::wrap(&mut store, |mut _caller: Caller<'_, HostState>, _environ_ptr: i32, _environ_buf_ptr: i32| -> i32 { 0 });
    let _ = linker.define("wasi_snapshot_preview1", "environ_get", environ_get);

    let args_sizes_get = Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, count_ptr: i32, size_ptr: i32| -> i32 {
        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
        let _ = memory.write(&mut caller, count_ptr as usize, &0u32.to_le_bytes());
        let _ = memory.write(&mut caller, size_ptr as usize, &0u32.to_le_bytes());
        0
    });
    let _ = linker.define("wasi_snapshot_preview1", "args_sizes_get", args_sizes_get);
    
    let args_get = Func::wrap(&mut store, |mut _caller: Caller<'_, HostState>, _argv_ptr: i32, _argv_buf_ptr: i32| -> i32 { 0 });
    let _ = linker.define("wasi_snapshot_preview1", "args_get", args_get);

    let proc_exit = Func::wrap(&mut store, |mut _caller: Caller<'_, HostState>, _code: i32| {});
    let _ = linker.define("wasi_snapshot_preview1", "proc_exit", proc_exit);
    
    let path_open = Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, _dirfd: i32, _dirflags: i32, path_ptr: i32, path_len: i32, oflags: i32, _rights_base: i64, _rights_inheriting: i64, _fdflags: i32, fd_out_ptr: i32| -> i32 { 
        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
        
        let mut path_buf = alloc::vec![0u8; path_len as usize];
        if memory.read(&caller, path_ptr as usize, &mut path_buf).is_err() { return 21; }
        
        if let Ok(path_str) = core::str::from_utf8(&path_buf) {
            let fd = sys_open(path_str, oflags as usize);
            if fd != usize::MAX {
                let fd_bytes = (fd as u32).to_le_bytes();
                let _ = memory.write(&mut caller, fd_out_ptr as usize, &fd_bytes);
                0
            } else {
                44 // ENOENT
            }
        } else {
            28 // EINVAL
        }
    });
    let _ = linker.define("wasi_snapshot_preview1", "path_open", path_open);

    linker.define("krakeos", "fb_flush", Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, ptr: u32, len: u32| -> u32 {
        let mem = caller.get_export("memory").unwrap().into_memory().unwrap();
        let data = mem.data(&caller);
        
        let byte_len = (len as usize) * 4;
        
        if (ptr as usize) + byte_len > data.len() {
            return 1;
        }
        
        let host_ptr = &data[ptr as usize] as *const u8;
        unsafe {
            let mut ret: usize;
            core::arch::asm!(
                "ecall",
                in("a7") 10,
                in("a0") host_ptr,
                in("a1") len,
                lateout("a0") ret,
            );
            ret as u32
        }
    })).unwrap();

    linker.define("krakeos", "wait_fs_event", Func::wrap(&mut store, |mut _caller: Caller<'_, HostState>, fd: u32| -> u32 {
        sys_wait_fs_event(fd as usize) as u32
    })).unwrap();

    let instance = match linker.instantiate_and_start(&mut store, &module) {
        Ok(inst) => inst,
        Err(e) => {
            sys_write(1, 0, b"wasm_runner: Failed to instantiate module!\n");
            let msg = alloc::format!("Error: {:?}\n", e);
            sys_write(1, 0, msg.as_bytes());
            sys_exit(1);
            loop {}
        }
    };
    
    let start_func = match instance.get_export(&store, "_start").and_then(|e| e.into_func()) {
        Some(f) => f,
        None => {
            sys_write(1, 0, b"wasm_runner: _start not found!\n");
            sys_exit(1);
            loop {}
        }
    };
    
    let _ = start_func.call(&mut store, &[], &mut []);
}

extern "C" fn wm_entry(_arg: usize) {
    run_wasm("wm.wasm");
    sys_exit(0);
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe {
        let heap_start = sys_sbrk(0);
        sys_sbrk(HEAP_SIZE);
        ALLOCATOR.lock().init(heap_start as *mut u8, HEAP_SIZE);
    }
    
    sys_write(1, 0, b"wasm_runner: started in U-mode! Spawning WM thread...\n");
    sys_spawn(wm_entry as usize, 0);
    
    run_wasm("app.wasm");
    sys_exit(0);
    loop {}
}
