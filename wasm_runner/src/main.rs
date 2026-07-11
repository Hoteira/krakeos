#![no_std]
#![no_main]

extern crate alloc;

use core::panic::PanicInfo;
use linked_list_allocator::LockedHeap;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

// Each app eagerly gets a 32MB wasm memory (see run.bat RUSTFLAGS); 288MB
// fits the shell + ~7 more apps with runner overhead.
const HEAP_SIZE: usize = 288 * 1024 * 1024;

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

pub fn sys_sleep(ms: usize) {
    unsafe { core::arch::asm!("ecall", in("a7") 13, in("a0") ms); }
}

pub fn sys_fstat(fd: usize) -> usize {
    let mut ret: usize;
    unsafe { core::arch::asm!("ecall", in("a7") 14, inout("a0") fd => ret); }
    ret
}

// Lazily opened fd for dev/system/time; the kernel always hands out the same
// fd for it, so sharing one across threads is fine.
static TIME_FD: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(usize::MAX);

fn time_ns() -> u64 {
    use core::sync::atomic::Ordering;
    let mut fd = TIME_FD.load(Ordering::Relaxed);
    if fd == usize::MAX {
        fd = sys_open("dev/system/time", 0);
        TIME_FD.store(fd, Ordering::Relaxed);
    }
    if fd == usize::MAX { return 0; }
    let mut buf = [0u8; 8];
    if sys_read(fd, 0, &mut buf) == 8 { u64::from_le_bytes(buf) } else { 0 }
}

fn run_wasm(path: &str) {
    let msg = b"wasm_runner: opening module...\n";
    sys_write(1, 0, msg);

    let fd = sys_open(path, 0);
    if fd == usize::MAX {
        sys_write(1, 0, b"wasm_runner: failed to open module!\n");
        sys_exit(1);
    }

    let size = sys_fstat(fd);
    if size == 0 || size == usize::MAX {
        sys_write(1, 0, b"wasm_runner: module is empty!\n");
        sys_exit(1);
    }
    let mut wasm_bytes = alloc::vec![0u8; size];
    let bytes_read = sys_read(fd, 0, &mut wasm_bytes);
    wasm_bytes.truncate(bytes_read);

    if bytes_read == 0 {
        sys_write(1, 0, b"wasm_runner: read 0 bytes!\n");
        sys_exit(1);
    }
    
    use wasmi::*;
    // Lazy compilation: translate functions on first call instead of the
    // whole module up front — big app-startup win under interpretation.
    let mut config = Config::default();
    config.compilation_mode(CompilationMode::Lazy);
    let engine = Engine::new(&config);
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
            
            let out_fd_idx = out_fd as usize;
            let offset = caller.data().fd_offsets.get(out_fd_idx).copied().unwrap_or(0);
            
            let mem_data = memory.data(&caller);
            let Some(data_slice) = mem_data.get(buf_ptr .. buf_ptr + buf_len) else { return 21; };

            let bw = sys_write(out_fd_idx, offset, data_slice);
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
            
            let in_fd_idx = in_fd as usize;
            let offset = caller.data().fd_offsets.get(in_fd_idx).copied().unwrap_or(0);
            
            let br = {
                let mem_data = memory.data_mut(&mut caller);
                let Some(data_slice) = mem_data.get_mut(buf_ptr .. buf_ptr + buf_len) else { return 21; };
                sys_read(in_fd_idx, offset, data_slice)
            };
            
            if br > 0 {
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
    
    let fd_filestat_get = Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, fd: i32, buf_ptr: i32| -> i32 { 
        let size = sys_fstat(fd as usize) as u64;
        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
        
        let mut stat = [0u8; 64];
        stat[16] = 4; // regular file
        stat[24] = 1; // nlink
        stat[32..40].copy_from_slice(&size.to_le_bytes()); // size
        
        let _ = memory.write(&mut caller, buf_ptr as usize, &stat);
        0 
    });
    let _ = linker.define("wasi_snapshot_preview1", "fd_filestat_get", fd_filestat_get);

    let fd_prestat_get = Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, fd: i32, buf_ptr: i32| -> i32 { 
        if fd == 3 {
            let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
            let mut prestat = [0u8; 8];
            prestat[0] = 0; // tag = dir
            prestat[4] = 1; // name_len = 1 ("/")
            let _ = memory.write(&mut caller, buf_ptr as usize, &prestat);
            0 // SUCCESS
        } else {
            8 // EBADF
        }
    });
    let _ = linker.define("wasi_snapshot_preview1", "fd_prestat_get", fd_prestat_get);

    let fd_prestat_dir_name = Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, fd: i32, path_ptr: i32, path_len: i32| -> i32 { 
        if fd == 3 && path_len > 0 {
            let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
            let _ = memory.write(&mut caller, path_ptr as usize, b"/");
            0
        } else {
            28 // EINVAL
        }
    });
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
            } else { // END
                let size = sys_fstat(fd_idx) as i64;
                (size + offset) as usize
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
        let time_bytes = time_ns().to_le_bytes();
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
            let msg = alloc::format!("wasm_runner: path_open '{}'\n", path_str);
            sys_write(1, 0, msg.as_bytes());
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

    let poll_oneoff = Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, in_ptr: i32, out_ptr: i32, nsubscriptions: i32, ret_events_ptr: i32| -> i32 {
        if nsubscriptions == 0 { return 28; /* EINVAL */ }
        
        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
        
        let mut sub_buf = alloc::vec![0u8; 48];
        if memory.read(&caller, in_ptr as usize, &mut sub_buf).is_err() { return 21; }
        
        let sub_type = sub_buf[8];
        if sub_type == 0 { // Clock
            let timeout_ns = u64::from_le_bytes([
                sub_buf[24], sub_buf[25], sub_buf[26], sub_buf[27],
                sub_buf[28], sub_buf[29], sub_buf[30], sub_buf[31]
            ]);
            let timeout_ms = (timeout_ns / 1_000_000) as usize;
            
            sys_sleep(timeout_ms);
            
            // Write Event
            let mut event_buf = alloc::vec![0u8; 32];
            // userdata
            event_buf[0..8].copy_from_slice(&sub_buf[0..8]);
            // error = 0, type = 0
            event_buf[8] = 0; event_buf[9] = 0; event_buf[10] = 0;
            
            let _ = memory.write(&mut caller, out_ptr as usize, &event_buf);
            let _ = memory.write(&mut caller, ret_events_ptr as usize, &1u32.to_le_bytes());
            
            0
        } else {
            // Unsupported subscription type for now
            58 // ENOTSUP
        }
    });
    let _ = linker.define("wasi_snapshot_preview1", "poll_oneoff", poll_oneoff);

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
    
    if let Err(e) = start_func.call(&mut store, &[], &mut []) {
        let msg = alloc::format!("wasm_runner: app '{}' trapped: {:?}\n", path, e);
        sys_write(1, 0, msg.as_bytes());
    }
}

fn spawn_listener(_arg: usize) {
    let fd = sys_open("dev/system/spawn_queue", 0);
    if fd == usize::MAX { return; }
    let mut buf = [0u8; 256];
    loop {
        let bytes_read = sys_read(fd, 0, &mut buf);
        if bytes_read > 0 {
            if let Ok(path_str) = core::str::from_utf8(&buf[..bytes_read]) {
                let path_box = alloc::boxed::Box::new(alloc::string::String::from(path_str));
                let arg_ptr = alloc::boxed::Box::into_raw(path_box) as usize;
                sys_spawn(app_runner as *const () as usize, arg_ptr);
            }
        } else {
            sys_sleep(10);
        }
    }
}

fn app_runner(arg: usize) {
    let path_ptr = arg as *mut alloc::string::String;
    let path_box = unsafe { alloc::boxed::Box::from_raw(path_ptr) };
    run_wasm(&path_box);
    sys_exit(0);
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe {
        let heap_start = sys_sbrk(0);
        sys_sbrk(HEAP_SIZE);
        ALLOCATOR.lock().init(heap_start as *mut u8, HEAP_SIZE);
    }
    
    sys_write(1, 0, b"wasm_runner: started in U-mode! Starting shell.wasm...\n");
    
    sys_spawn(spawn_listener as *const () as usize, 0);
    
    run_wasm("/apps/shell.wasm");
    sys_exit(0);
    loop {}
}
