use wasmi::*;
use alloc::vec::Vec;
use crate::uart::Uart;

pub fn run_wasm() {
    crate::println!("run_wasm: start");
    
    let desc_idx = match crate::fs::find_file("app.wasm") {
        Some(idx) => idx,
        None => {
            crate::println!("run_wasm: app.wasm not found on disk!");
            return;
        }
    };
    
    let size = crate::fs::get_file_size(desc_idx);
    crate::println!("Found app.wasm on disk, size: {} bytes", size);
    
    let mut wasm_bytes = alloc::vec![0; size];
    let bytes_read = crate::fs::read_file(desc_idx, 0, &mut wasm_bytes);
    if bytes_read != size {
        crate::println!("run_wasm: Failed to read full app.wasm");
        return;
    }

    crate::println!("run_wasm: init engine");
    let engine = Engine::default();
    crate::println!("run_wasm: init module");
    let module = Module::new(&engine, &wasm_bytes[..]).unwrap();

    crate::println!("run_wasm: init store");
    type HostState = ();
    let mut store = Store::new(&engine, ());

    #[derive(Copy, Clone)]
    struct FileDesc {
        desc_idx: usize,
        offset: usize,
        in_use: bool,
    }

    crate::println!("run_wasm: init linker");
    let mut linker = <Linker<HostState>>::new(&engine);
    static mut FDS: [FileDesc; 16] = [FileDesc { desc_idx: 0, offset: 0, in_use: false }; 16];

    let path_open = Func::wrap(
        &mut store,
        |mut caller: Caller<'_, HostState>, _dirfd: i32, _dirflags: i32, path_ptr: i32, path_len: i32, oflags: i32, _rights_base: i64, _rights_inheriting: i64, _fdflags: i32, fd_out_ptr: i32| -> i32 {
            let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
            let mut path_buf = alloc::vec![0u8; path_len as usize];
            if memory.read(&caller, path_ptr as usize, &mut path_buf).is_err() { return 21; }
            
            let path_str = core::str::from_utf8(&path_buf).unwrap_or("");
            
            let desc_idx = if (oflags & 1) != 0 {
                // O_CREAT
                crate::fs::find_file(path_str).or_else(|| crate::fs::create_file(path_str))
            } else {
                crate::fs::find_file(path_str)
            };
            
            if let Some(idx) = desc_idx {
                unsafe {
                    for i in 3..16 {
                        if !FDS[i].in_use {
                            FDS[i].in_use = true;
                            FDS[i].desc_idx = idx;
                            FDS[i].offset = 0;
                            if (oflags & 8) != 0 { // O_TRUNC
                                // Not implemented trunc, but reset offset
                            }
                            let fd_out = (i as u32).to_le_bytes();
                            let _ = memory.write(&mut caller, fd_out_ptr as usize, &fd_out);
                            return 0; // SUCCESS
                        }
                    }
                }
                return 24; // EMFILE
            }
            44 // ENOENT
        }
    );
    linker.define("wasi_snapshot_preview1", "path_open", path_open).unwrap();

    let fd_read = Func::wrap(
        &mut store,
        |mut caller: Caller<'_, HostState>, fd: i32, iovs_ptr: i32, iovs_len: i32, nread_ptr: i32| -> i32 {
            if fd < 3 || fd >= 16 { return 8; } // EBADF
            let desc_idx;
            let mut offset;
            unsafe {
                if !FDS[fd as usize].in_use { return 8; }
                desc_idx = FDS[fd as usize].desc_idx;
                offset = FDS[fd as usize].offset;
            }
            
            let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
            let mut total_read = 0;
            let mut iovs_buf = [0u8; 8];
            
            for i in 0..iovs_len {
                let iov_addr = (iovs_ptr + i * 8) as usize;
                if memory.read(&caller, iov_addr, &mut iovs_buf).is_err() { return 21; }
                let buf_ptr = u32::from_le_bytes([iovs_buf[0], iovs_buf[1], iovs_buf[2], iovs_buf[3]]) as usize;
                let buf_len = u32::from_le_bytes([iovs_buf[4], iovs_buf[5], iovs_buf[6], iovs_buf[7]]) as usize;
                
                let mut data = alloc::vec![0u8; buf_len];
                let bytes_read = crate::fs::read_file(desc_idx, offset, &mut data);
                if bytes_read > 0 {
                    let _ = memory.write(&mut caller, buf_ptr, &data[..bytes_read]);
                    total_read += bytes_read;
                    offset += bytes_read;
                }
                if bytes_read < buf_len { break; }
            }
            
            unsafe { FDS[fd as usize].offset = offset; }
            let nread_bytes = (total_read as u32).to_le_bytes();
            let _ = memory.write(&mut caller, nread_ptr as usize, &nread_bytes);
            0
        }
    );
    linker.define("wasi_snapshot_preview1", "fd_read", fd_read).unwrap();

    let fd_close = Func::wrap(
        &mut store,
        |mut _caller: Caller<'_, HostState>, fd: i32| -> i32 {
            if fd < 3 || fd >= 16 { return 8; }
            unsafe {
                if !FDS[fd as usize].in_use { return 8; }
                FDS[fd as usize].in_use = false;
            }
            0
        }
    );
    linker.define("wasi_snapshot_preview1", "fd_close", fd_close).unwrap();

    let fd_seek = Func::wrap(
        &mut store,
        |mut caller: Caller<'_, HostState>, fd: i32, offset: i64, whence: i32, newoffset_ptr: i32| -> i32 {
            if fd < 3 || fd >= 16 { return 8; }
            unsafe {
                if !FDS[fd as usize].in_use { return 8; }
                let mut new_off = FDS[fd as usize].offset as i64;
                match whence {
                    0 => new_off = offset, // SET
                    1 => new_off += offset, // CUR
                    2 => new_off = 1024 * 1024, // END (approx, actually needs file size)
                    _ => return 28, // EINVAL
                }
                if new_off < 0 { return 28; }
                FDS[fd as usize].offset = new_off as usize;
                
                let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
                let bytes = (new_off as u64).to_le_bytes();
                let _ = memory.write(&mut caller, newoffset_ptr as usize, &bytes);
            }
            0
        }
    );
    linker.define("wasi_snapshot_preview1", "fd_seek", fd_seek).unwrap();
    
    // Also need to handle fd_write for files
    let fd_write = Func::wrap(
        &mut store,
        |mut caller: Caller<'_, HostState>, fd: i32, iovs_ptr: i32, iovs_len: i32, nwritten_ptr: i32| -> i32 {
            let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
            let mut total_written = 0;
            let mut iovs_buf = [0u8; 8];

            if fd == 1 || fd == 2 {
                for i in 0..iovs_len {
                    let iov_addr = (iovs_ptr + i * 8) as usize;
                    if memory.read(&caller, iov_addr, &mut iovs_buf).is_err() { return 21; }

                    let buf_ptr = u32::from_le_bytes([iovs_buf[0], iovs_buf[1], iovs_buf[2], iovs_buf[3]]) as usize;
                    let buf_len = u32::from_le_bytes([iovs_buf[4], iovs_buf[5], iovs_buf[6], iovs_buf[7]]) as usize;

                    let mut data = alloc::vec![0u8; buf_len];
                    if memory.read(&caller, buf_ptr, &mut data).is_err() { return 21; }

                    let mut uart = Uart::new(0x1000_0000);
                    for &b in &data { uart.put(b); }
                    total_written += buf_len;
                }
            } else if fd >= 3 && fd < 16 {
                let desc_idx;
                let mut offset;
                unsafe {
                    if !FDS[fd as usize].in_use { return 8; }
                    desc_idx = FDS[fd as usize].desc_idx;
                    offset = FDS[fd as usize].offset;
                }
                
                for i in 0..iovs_len {
                    let iov_addr = (iovs_ptr + i * 8) as usize;
                    if memory.read(&caller, iov_addr, &mut iovs_buf).is_err() { return 21; }

                    let buf_ptr = u32::from_le_bytes([iovs_buf[0], iovs_buf[1], iovs_buf[2], iovs_buf[3]]) as usize;
                    let buf_len = u32::from_le_bytes([iovs_buf[4], iovs_buf[5], iovs_buf[6], iovs_buf[7]]) as usize;

                    let mut data = alloc::vec![0u8; buf_len];
                    if memory.read(&caller, buf_ptr, &mut data).is_err() { return 21; }

                    let bw = crate::fs::write_file(desc_idx, offset, &data);
                    total_written += bw;
                    offset += bw;
                    if bw < buf_len { break; }
                }
                unsafe { FDS[fd as usize].offset = offset; }
            } else {
                return 8; // EBADF
            }

            let nwritten_bytes = (total_written as u32).to_le_bytes();
            let _ = memory.write(&mut caller, nwritten_ptr as usize, &nwritten_bytes);
            0
        }
    );
    linker.define("wasi_snapshot_preview1", "fd_write", fd_write).unwrap();

    let fd_fdstat_get = Func::wrap(&mut store, |mut _caller: Caller<'_, HostState>, _fd: i32, _stat_ptr: i32| -> i32 { 0 });
    linker.define("wasi_snapshot_preview1", "fd_fdstat_get", fd_fdstat_get).unwrap();

    let clock_time_get = Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, _id: i32, _precision: i64, time_ptr: i32| -> i32 {
        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
        // Return 0 for now as we don't have a real RTC wired up to WASI yet
        let time_bytes = (0u64).to_le_bytes();
        let _ = memory.write(&mut caller, time_ptr as usize, &time_bytes);
        0
    });
    linker.define("wasi_snapshot_preview1", "clock_time_get", clock_time_get).unwrap();

    // WASI-libc requires __wasi_init_tp for thread-local storage initialization
    let wasi_init_tp = Func::wrap(&mut store, |mut _caller: Caller<'_, HostState>| {});
    linker.define("env", "__wasi_init_tp", wasi_init_tp).unwrap();

    let wasm_call_dtors = Func::wrap(&mut store, |mut _caller: Caller<'_, HostState>| {});
    linker.define("env", "__wasm_call_dtors", wasm_call_dtors).unwrap();

    let wasi_proc_exit = Func::wrap(&mut store, |mut _caller: Caller<'_, HostState>, _r: i32| {});
    linker.define("env", "__wasi_proc_exit", wasi_proc_exit).unwrap();

    crate::println!("run_wasm: instantiating...");
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .unwrap();

    crate::println!("run_wasm: getting _start");
    let start_func = instance
        .get_export(&store, "_start")
        .and_then(|e| e.into_func())
        .unwrap();

    let _ = start_func.call(&mut store, &[], &mut []);
    
    // Halt thread to avoid crash
    loop { core::hint::spin_loop(); }
}
