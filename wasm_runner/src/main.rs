#![no_std]
#![no_main]

extern crate alloc;

mod aot_host;

use core::alloc::{GlobalAlloc, Layout};
use core::panic::PanicInfo;
use linked_list_allocator::LockedHeap;

// Runner heap (module bytes, compiled code, app stacks), grown via sys_sbrk.
// App linear memory is not here: the kernel backs it on demand (APP_MEMORY).
struct GrowableHeap(LockedHeap);

const HEAP_INITIAL: usize = 16 * 1024 * 1024;
const HEAP_GROW_STEP: usize = 8 * 1024 * 1024;
// The kernel maps the runner's main stack at 0x7000_0000; stay below it.
const HEAP_LIMIT: usize = 0x6000_0000;

unsafe impl GlobalAlloc for GrowableHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut heap = self.0.lock();
        loop {
            if let Ok(p) = heap.allocate_first_fit(layout) {
                return p.as_ptr();
            }
            let need = layout.size() + layout.align() + 4096;
            let grow = (need.max(HEAP_GROW_STEP) + 0xFFFFF) & !0xFFFFF;
            let top = heap.top() as usize;
            if top + grow > HEAP_LIMIT || sys_sbrk(grow) != top {
                return core::ptr::null_mut();
            }
            heap.extend(grow);
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if let Some(p) = core::ptr::NonNull::new(ptr) {
            self.0.lock().deallocate(p, layout);
        }
    }
}

#[global_allocator]
static ALLOCATOR: GrowableHeap = GrowableHeap(LockedHeap::empty());

// On-demand linear memory for AOT apps, backed by kernel syscalls.
static APP_MEMORY: wasmaot::aot::MemoryOps = wasmaot::aot::MemoryOps {
    reserve: |_max| {
        let base = sys_mem_reserve();
        if base == usize::MAX { core::ptr::null_mut() } else { base as *mut u8 }
    },
    commit: |base, size| sys_mem_commit(base as usize, size) == 0,
    release: |base| {
        sys_mem_release(base as usize);
    },
};

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let msg = b"wasm_runner: PANIC!\n";
    sys_write(LOG_FD, 0, msg);
    
    if let Some(loc) = info.location() {
        let file = loc.file().as_bytes();
        sys_write(LOG_FD, 0, file);
        sys_write(LOG_FD, 0, b":");
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

// Returns the new thread id, or usize::MAX if it couldn't be created.
pub fn sys_spawn(entry: usize, arg: usize) -> usize {
    let mut ret: usize;
    unsafe { core::arch::asm!("ecall", in("a7") 12, inout("a0") entry => ret, in("a1") arg); }
    ret
}

// Returns 1 instead of sleeping if this thread has been killed.
pub fn sys_sleep(ms: usize) -> usize {
    let mut ret: usize;
    unsafe { core::arch::asm!("ecall", in("a7") 13, inout("a0") ms => ret); }
    ret
}

pub fn sys_close(fd: usize) -> usize {
    let mut ret: usize;
    unsafe { core::arch::asm!("ecall", in("a7") 15, inout("a0") fd => ret); }
    ret
}

// One kernel directory record (see fs::read_dir_entry); 0 = end,
// usize::MAX = not a directory.
pub fn sys_readdir(fd: usize, cookie: usize, buf: &mut [u8]) -> usize {
    let mut ret: usize;
    unsafe {
        core::arch::asm!("ecall", in("a7") 16, inout("a0") fd => ret, in("a1") cookie, in("a2") buf.as_mut_ptr() as usize, in("a3") buf.len());
    }
    ret
}

// Reserve a 1 GiB app-memory slot; returns its base or usize::MAX.
pub fn sys_mem_reserve() -> usize {
    let mut ret: usize;
    unsafe { core::arch::asm!("ecall", in("a7") 19, lateout("a0") ret); }
    ret
}

// Back [base, base + size) of a reserved slot with zeroed pages.
pub fn sys_mem_commit(base: usize, size: usize) -> usize {
    let mut ret: usize;
    unsafe { core::arch::asm!("ecall", in("a7") 20, inout("a0") base => ret, in("a1") size); }
    ret
}

pub fn sys_mem_release(base: usize) -> usize {
    let mut ret: usize;
    unsafe { core::arch::asm!("ecall", in("a7") 21, inout("a0") base => ret); }
    ret
}

pub fn sys_kill(tid: usize) -> usize {
    let mut ret: usize;
    unsafe { core::arch::asm!("ecall", in("a7") 17, inout("a0") tid => ret); }
    ret
}

// SYS_FSOP operations (see the kernel's syscall.rs).
pub const FSOP_UNLINK: usize = 1;
pub const FSOP_RMDIR: usize = 2;
pub const FSOP_MKDIR: usize = 3;
pub const FSOP_RENAME: usize = 4;
pub const FSOP_STAT: usize = 5;

pub fn sys_fsop(op: usize, a: &str, b: &str) -> usize {
    let mut ret: usize;
    unsafe {
        core::arch::asm!("ecall", in("a7") 22, inout("a0") op => ret, in("a1") a.as_ptr() as usize, in("a2") a.len(),
            in("a3") b.as_ptr() as usize, in("a4") b.len());
    }
    ret
}

// WASI filestat for path, or ENOENT.
pub fn wasi_filestat(path: &str) -> Result<[u8; 64], u32> {
    let r = sys_fsop(FSOP_STAT, path, "");
    if r == usize::MAX {
        return Err(44); // NOENT
    }
    let mut stat = [0u8; 64];
    let ino = path.bytes().fold(1u64, |h, c| h.wrapping_mul(31).wrapping_add(c as u64));
    stat[8..16].copy_from_slice(&ino.to_le_bytes());
    stat[16] = if r >> 32 & 1 != 0 { 3 } else { 4 }; // DIRECTORY / REGULAR_FILE
    stat[24] = 1; // nlink
    stat[32..40].copy_from_slice(&((r & 0xFFFF_FFFF) as u64).to_le_bytes());
    Ok(stat)
}

pub fn sys_setname(name: &str, killable: bool) {
    unsafe { core::arch::asm!("ecall", in("a7") 18, in("a0") name.as_ptr() as usize, in("a1") name.len(), in("a2") killable as usize); }
}

// Exit status reported for an app stopped by SYS_KILL.
pub const KILLED_STATUS: u32 = 137;

// The runner's own messages: always the serial log, even for an app
// whose stdout goes to the terminal.
pub const LOG_FD: usize = 117;
// sys_read result for stdin with nothing typed yet (see kernel pipe.rs).
pub const WOULD_BLOCK: usize = usize::MAX - 1;
// Separates a spawn request from the id of the pipe the app's stdio
// should use (sent by the terminal for console apps).
const PIPE_MARK: char = '\x1d';

const WASI_ENOTDIR: u32 = 54;

// Fills out with WASI fd_readdir dirents from cookie. Returns the bytes used
// (out.len() when more entries may follow) or a WASI errno.
pub fn wasi_readdir(fd: usize, mut cookie: u64, out: &mut [u8]) -> Result<usize, u32> {
    let mut used = 0;
    let mut rec = [0u8; 80];
    loop {
        let r = sys_readdir(fd, cookie as usize, &mut rec);
        if r == usize::MAX {
            return if used == 0 { Err(WASI_ENOTDIR) } else { Ok(used) };
        }
        if r == 0 {
            return Ok(used);
        }
        let next = u32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]) as u64;
        let ino = u32::from_le_bytes([rec[8], rec[9], rec[10], rec[11]]) as u64;
        let is_dir = rec[12] != 0;
        let name_len = rec[13] as usize;

        // dirent: d_next u64, d_ino u64, d_namlen u32, d_type u8, 3 pad, name.
        // d_ino must be nonzero or wasi-libc stats every entry to get one.
        let mut ent = [0u8; 24 + 64];
        ent[0..8].copy_from_slice(&next.to_le_bytes());
        ent[8..16].copy_from_slice(&ino.to_le_bytes());
        ent[16..20].copy_from_slice(&(name_len as u32).to_le_bytes());
        ent[20] = if is_dir { 3 } else { 4 }; // DIRECTORY / REGULAR_FILE
        ent[24..24 + name_len].copy_from_slice(&rec[14..14 + name_len]);
        let ent = &ent[..24 + name_len];

        // A truncated last entry is allowed: the caller sees bufused == len
        // and retries from that entry's cookie with a bigger buffer.
        let n = ent.len().min(out.len() - used);
        out[used..used + n].copy_from_slice(&ent[..n]);
        used += n;
        if used == out.len() {
            return Ok(used);
        }
        cookie = next;
    }
}

pub fn sys_fstat(fd: usize) -> usize {
    let mut ret: usize;
    unsafe { core::arch::asm!("ecall", in("a7") 14, inout("a0") fd => ret); }
    ret
}

// Lazily opened fd for dev/system/time; the kernel always hands out the same
// fd for it, so sharing one across threads is fine.
static TIME_FD: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(usize::MAX);

pub fn time_ns() -> u64 {
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

// Returns true if the module ran (cleanly or trapped), false if the AOT
// compiler can't handle it and the caller should fall back to wasmi.
fn try_run_aot(wasm: &[u8], args: &[&str]) -> bool {
    use aot_host::{dispatch, resolve, AotHostState, TRAP_PROC_EXIT_BASE};
    use wasmaot::aot::AotModule;

    let mut module = match AotModule::instantiate_with_memory(wasm, dispatch, 0, Some(&APP_MEMORY)) {
        Ok(m) => m,
        Err(wasmaot::aot::AotError::OutOfMemory) => {
            sys_write(LOG_FD, 0, b"wasm_runner: not enough memory to start app\n");
            return true;
        }
        Err(e) => {
            let msg = alloc::format!("wasm_runner: AOT cannot compile ({:?}), using wasmi\n", e);
            sys_write(LOG_FD, 0, msg.as_bytes());
            return false;
        }
    };

    // The dispatcher finds this state through the context's user_data slot.
    let mut state = alloc::boxed::Box::new(AotHostState::new());
    state.args = Args::new(args);
    for (m, n) in module.func_imports() {
        state.imports.push(resolve(m, n));
    }
    module.set_user_data(state.as_mut() as *mut AotHostState as usize);

    let start = match module.find_export("_start") {
        Some(a) => a,
        None => {
            sys_write(LOG_FD, 0, b"wasm_runner: AOT: module has no _start\n");
            return false;
        }
    };

    sys_write(LOG_FD, 0, b"wasm_runner: running via AOT engine\n");
    match module.call_void(start) {
        Ok(()) => {}
        Err(code) if code == TRAP_PROC_EXIT_BASE + KILLED_STATUS => {
            sys_write(LOG_FD, 0, b"wasm_runner: app killed\n");
        }
        Err(code) if code >= TRAP_PROC_EXIT_BASE => {} // clean proc_exit
        Err(code) => {
            let msg = alloc::format!(
                "wasm_runner: AOT trap: {} (code {})\n",
                AotModule::trap_name(code),
                code
            );
            sys_write(LOG_FD, 0, msg.as_bytes());
        }
    }
    drop(state);
    true
}

// Command-line arguments as WASI hands them out: a NUL-terminated blob and
// each argument's offset into it.
#[derive(Default)]
pub struct Args {
    pub blob: alloc::vec::Vec<u8>,
    pub offsets: alloc::vec::Vec<u32>,
}

impl Args {
    pub fn new(args: &[&str]) -> Args {
        let mut a = Args::default();
        for s in args {
            a.offsets.push(a.blob.len() as u32);
            a.blob.extend_from_slice(s.as_bytes());
            a.blob.push(0);
        }
        a
    }
}

// args[0] is the module path; spawn requests carry the remaining
// arguments tab-separated after it.
fn run_wasm(args: &[&str]) {
    let path = args[0];
    let msg = b"wasm_runner: opening module...\n";
    sys_write(LOG_FD, 0, msg);

    let fd = sys_open(path, 0);
    if fd == usize::MAX {
        sys_write(LOG_FD, 0, b"wasm_runner: failed to open module!\n");
        sys_exit(1);
    }

    let size = sys_fstat(fd);
    if size == 0 || size == usize::MAX {
        sys_write(LOG_FD, 0, b"wasm_runner: module is empty!\n");
        sys_exit(1);
    }
    let mut wasm_bytes = alloc::vec![0u8; size];
    let bytes_read = sys_read(fd, 0, &mut wasm_bytes);
    wasm_bytes.truncate(bytes_read);

    if bytes_read == 0 {
        sys_write(LOG_FD, 0, b"wasm_runner: read 0 bytes!\n");
        sys_exit(1);
    }

    if try_run_aot(&wasm_bytes, args) {
        return;
    }

    use wasmi::*;
    // Lazy compilation translates functions on first call: much faster startup.
    let mut config = Config::default();
    config.compilation_mode(CompilationMode::Lazy);
    let engine = Engine::new(&config);
    let module = match Module::new(&engine, &wasm_bytes[..]) {
        Ok(m) => m,
        Err(_) => {
            sys_write(LOG_FD, 0, b"wasm_runner: Failed to parse module!\n");
            sys_exit(1);
            loop {}
        }
    };
    
    struct HostState {
        fd_offsets: [usize; 2048],
        args: Args,
    }
    let mut store = Store::new(&engine, HostState { fd_offsets: [0; 2048], args: Args::new(args) });
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
        stat[32..40].copy_from_slice(&size.to_le_bytes());
        
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
            0
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

    fn guest_str(caller: &Caller<'_, HostState>, ptr: i32, len: i32) -> Option<alloc::string::String> {
        let memory = caller.get_export("memory")?.into_memory()?;
        let mut buf = alloc::vec![0u8; len as u32 as usize];
        memory.read(caller, ptr as u32 as usize, &mut buf).ok()?;
        alloc::string::String::from_utf8(buf).ok()
    }

    let path_filestat_get = Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, _dirfd: i32, _flags: i32, path_ptr: i32, path_len: i32, buf_ptr: i32| -> i32 {
        let Some(path) = guest_str(&caller, path_ptr, path_len) else { return 21 };
        match wasi_filestat(&path) {
            Ok(stat) => {
                let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
                if memory.write(&mut caller, buf_ptr as u32 as usize, &stat).is_err() { return 21; }
                0
            }
            Err(e) => e as i32,
        }
    });
    let _ = linker.define("wasi_snapshot_preview1", "path_filestat_get", path_filestat_get);

    for (name, op) in [("path_unlink_file", FSOP_UNLINK), ("path_remove_directory", FSOP_RMDIR), ("path_create_directory", FSOP_MKDIR)] {
        let f = Func::wrap(&mut store, move |caller: Caller<'_, HostState>, _dirfd: i32, path_ptr: i32, path_len: i32| -> i32 {
            match guest_str(&caller, path_ptr, path_len) {
                Some(path) => sys_fsop(op, &path, "") as i32,
                None => 21,
            }
        });
        let _ = linker.define("wasi_snapshot_preview1", name, f);
    }
    let path_rename = Func::wrap(&mut store, |caller: Caller<'_, HostState>, _fd: i32, old_ptr: i32, old_len: i32, _new_fd: i32, new_ptr: i32, new_len: i32| -> i32 {
        match (guest_str(&caller, old_ptr, old_len), guest_str(&caller, new_ptr, new_len)) {
            (Some(a), Some(b)) => sys_fsop(FSOP_RENAME, &a, &b) as i32,
            _ => 21,
        }
    });
    let _ = linker.define("wasi_snapshot_preview1", "path_rename", path_rename);

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
        if fd_idx > 2 {
            sys_close(fd_idx);
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
    let wasi_proc_exit = Func::wrap(&mut store, |mut _caller: Caller<'_, HostState>, r: i32| -> Result<(), Error> { Err(Error::i32_exit(r)) });
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
        let count = caller.data().args.offsets.len() as u32;
        let size = caller.data().args.blob.len() as u32;
        let _ = memory.write(&mut caller, count_ptr as usize, &count.to_le_bytes());
        let _ = memory.write(&mut caller, size_ptr as usize, &size.to_le_bytes());
        0
    });
    let _ = linker.define("wasi_snapshot_preview1", "args_sizes_get", args_sizes_get);
    
    let args_get = Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, argv_ptr: i32, argv_buf_ptr: i32| -> i32 {
        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
        let blob = caller.data().args.blob.clone();
        let offsets = caller.data().args.offsets.clone();
        for (i, off) in offsets.iter().enumerate() {
            let p = (argv_buf_ptr as u32).wrapping_add(*off);
            if memory.write(&mut caller, argv_ptr as usize + i * 4, &p.to_le_bytes()).is_err() { return 21; }
        }
        if memory.write(&mut caller, argv_buf_ptr as usize, &blob).is_err() { return 21; }
        0
    });
    let _ = linker.define("wasi_snapshot_preview1", "args_get", args_get);

    let proc_exit = Func::wrap(&mut store, |mut _caller: Caller<'_, HostState>, code: i32| -> Result<(), Error> { Err(Error::i32_exit(code)) });
    let _ = linker.define("wasi_snapshot_preview1", "proc_exit", proc_exit);
    
    let path_open = Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, _dirfd: i32, _dirflags: i32, path_ptr: i32, path_len: i32, oflags: i32, _rights_base: i64, _rights_inheriting: i64, _fdflags: i32, fd_out_ptr: i32| -> i32 { 
        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
        
        let mut path_buf = alloc::vec![0u8; path_len as usize];
        if memory.read(&caller, path_ptr as usize, &mut path_buf).is_err() { return 21; }
        
        if let Ok(path_str) = core::str::from_utf8(&path_buf) {
            let msg = alloc::format!("wasm_runner: path_open '{}'\n", path_str);
            sys_write(LOG_FD, 0, msg.as_bytes());
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

    let poll_oneoff = Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, in_ptr: i32, out_ptr: i32, nsubscriptions: i32, ret_events_ptr: i32| -> Result<i32, Error> {
        if nsubscriptions == 0 { return Ok(28); } // EINVAL
        
        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
        
        let mut sub_buf = alloc::vec![0u8; 48];
        if memory.read(&caller, in_ptr as usize, &mut sub_buf).is_err() { return Ok(21); }
        
        let sub_type = sub_buf[8];
        if sub_type == 0 { // Clock
            let mut timeout_ns = u64::from_le_bytes([
                sub_buf[24], sub_buf[25], sub_buf[26], sub_buf[27],
                sub_buf[28], sub_buf[29], sub_buf[30], sub_buf[31]
            ]);
            if sub_buf[40] & 1 != 0 { // ABSTIME: a deadline, not a delay
                timeout_ns = timeout_ns.saturating_sub(time_ns());
            }
            let timeout_ms = (timeout_ns / 1_000_000) as usize;
            
            if sys_sleep(timeout_ms) != 0 {
                return Err(Error::i32_exit(KILLED_STATUS as i32));
            }
            
            let mut event_buf = alloc::vec![0u8; 32];
            // userdata
            event_buf[0..8].copy_from_slice(&sub_buf[0..8]);
            // error = 0, type = 0
            event_buf[8] = 0; event_buf[9] = 0; event_buf[10] = 0;
            
            let _ = memory.write(&mut caller, out_ptr as usize, &event_buf);
            let _ = memory.write(&mut caller, ret_events_ptr as usize, &1u32.to_le_bytes());
            
            Ok(0)
        } else {
            Ok(58) // ENOTSUP
        }
    });
    let _ = linker.define("wasi_snapshot_preview1", "poll_oneoff", poll_oneoff);

    let fd_readdir = Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, fd: i32, buf_ptr: i32, buf_len: i32, cookie: i64, bufused_ptr: i32| -> i32 {
        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
        let mut out = alloc::vec![0u8; buf_len as u32 as usize];
        match wasi_readdir(fd as u32 as usize, cookie as u64, &mut out) {
            Ok(used) => {
                if memory.write(&mut caller, buf_ptr as u32 as usize, &out[..used]).is_err() { return 21; }
                let _ = memory.write(&mut caller, bufused_ptr as u32 as usize, &(used as u32).to_le_bytes());
                0
            }
            Err(errno) => errno as i32,
        }
    });
    let _ = linker.define("wasi_snapshot_preview1", "fd_readdir", fd_readdir);

    // KrakeOS-specific: stop another app by thread id (0 on success).
    let krake_kill = Func::wrap(&mut store, |_caller: Caller<'_, HostState>, tid: i32| -> i32 {
        if sys_kill(tid as u32 as usize) == usize::MAX { -1 } else { 0 }
    });
    let _ = linker.define("krake", "kill", krake_kill);

    let instance = match linker.instantiate_and_start(&mut store, &module) {
        Ok(inst) => inst,
        Err(e) => {
            sys_write(LOG_FD, 0, b"wasm_runner: Failed to instantiate module!\n");
            let msg = alloc::format!("Error: {:?}\n", e);
            sys_write(LOG_FD, 0, msg.as_bytes());
            sys_exit(1);
            loop {}
        }
    };
    
    let start_func = match instance.get_export(&store, "_start").and_then(|e| e.into_func()) {
        Some(f) => f,
        None => {
            sys_write(LOG_FD, 0, b"wasm_runner: _start not found!\n");
            sys_exit(1);
            loop {}
        }
    };
    
    if let Err(e) = start_func.call(&mut store, &[], &mut []) {
        match e.i32_exit_status() {
            Some(s) if s == KILLED_STATUS as i32 => {
                sys_write(LOG_FD, 0, b"wasm_runner: app killed\n");
                return;
            }
            Some(_) => return, // proc_exit
            None => {}
        }
        let msg = alloc::format!("wasm_runner: app '{}' trapped: {:?}\n", path, e);
        sys_write(LOG_FD, 0, msg.as_bytes());
    }
}

fn spawn_listener(_arg: usize) {
    sys_setname("spawner", false);
    let fd = sys_open("dev/system/spawn_queue", 0);
    if fd == usize::MAX { return; }
    let mut buf = [0u8; 1024];
    loop {
        let bytes_read = sys_read(fd, 0, &mut buf);
        if bytes_read > 0 {
            if let Ok(path_str) = core::str::from_utf8(&buf[..bytes_read]) {
                // "path\targ\targ...", see run_wasm.
                let path_box = alloc::boxed::Box::new(alloc::string::String::from(path_str));
                let arg_ptr = alloc::boxed::Box::into_raw(path_box) as usize;
                if sys_spawn(app_runner as *const () as usize, arg_ptr) == usize::MAX {
                    // No thread was created, so app_runner won't free it.
                    drop(unsafe { alloc::boxed::Box::from_raw(arg_ptr as *mut alloc::string::String) });
                    sys_write(LOG_FD, 0, b"wasm_runner: too many running apps\n");
                }
            }
        } else {
            sys_sleep(10);
        }
    }
}

fn app_runner(arg: usize) {
    let path_ptr = arg as *mut alloc::string::String;
    let cmd = unsafe { alloc::boxed::Box::from_raw(path_ptr) };
    let (cmd, pipe) = match cmd.split_once(PIPE_MARK) {
        Some((c, id)) => (c, Some(id)),
        None => (cmd.as_str(), None),
    };
    if let Some(id) = pipe {
        if sys_open(&alloc::format!("dev/pipe/attach/{}", id), 0) == usize::MAX {
            sys_exit(1);
        }
    }
    let args: alloc::vec::Vec<&str> = cmd.split('\t').collect();
    if args[0].starts_with('/') {
        sys_setname(args[0], true);
    } else {
        sys_setname(&alloc::format!("/{}", args[0]), true);
    }
    run_wasm(&args);
    sys_exit(0);
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe {
        let heap_start = sys_sbrk(0);
        sys_sbrk(HEAP_INITIAL);
        ALLOCATOR.0.lock().init(heap_start as *mut u8, HEAP_INITIAL);
    }
    
    sys_write(LOG_FD, 0, b"wasm_runner: started in U-mode! Starting shell.wasm...\n");
    
    sys_setname("/apps/shell.wasm", false);
    sys_spawn(spawn_listener as *const () as usize, 0);
    
    run_wasm(&["/apps/shell.wasm"]);
    sys_exit(0);
    loop {}
}
