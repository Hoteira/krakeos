use crate::fs;
use crate::rust_alloc::collections::BTreeMap;
use crate::rust_alloc::format;
use crate::rust_alloc::string::String;
use crate::rust_alloc::string::ToString;
use crate::rust_alloc::vec::Vec;
use crate::sys::{syscall, syscall4, syscall5, syscall6};
use crate::wasm::wasi::env::{FdStat, FileStat, WasiEnv};

pub struct KrakeosWasiEnv {
    pub fd_table: BTreeMap<i32, WasiFile>,
    pub next_fd: i32,
    pub random_state: u64,
}

pub struct WasiFile {
    pub file: fs::File,
    pub path: String,
}

impl Default for KrakeosWasiEnv {
    fn default() -> Self {
        Self {
            fd_table: BTreeMap::new(),
            next_fd: 4, // 0,1,2 are reserved, 3 is preopened root
            random_state: 0,
        }
    }
}

impl KrakeosWasiEnv {
    fn resolve_path(&self, dirfd: i32, path: &str) -> Result<String, i32> {
        if path.contains("..") { return Err(76); } // ENOTCAPABLE
        
        let base = if dirfd == 3 {
            "@0xE0"
        } else if let Some(wf) = self.fd_table.get(&dirfd) {
            &wf.path
        } else {
            return Err(76); // ENOTCAPABLE
        };
        
        let clean = path.trim_start_matches('.').trim_start_matches('/');
        if base.ends_with('/') {
             Ok(format!("{}{}", base, clean))
        } else {
             Ok(format!("{}/{}", base, clean))
        }
    }
}

impl WasiEnv for KrakeosWasiEnv {
    fn args_get(&self) -> Result<Vec<String>, i32> {
        Ok(crate::env::args().collect())
    }

    fn environ_get(&self) -> Result<Vec<(String, String)>, i32> {
        Ok(crate::env::vars().collect())
    }

    fn clock_res_get(&self, id: u32) -> Result<u64, i32> {
        match id {
            0 | 1 => Ok(1_000_000), // 1ms
            _ => Err(28), // EINVAL
        }
    }

    fn clock_time_get(&self, id: u32, _precision: u64) -> Result<u64, i32> {
        match id {
            1 => Ok(crate::os::get_system_ticks() * 1_000_000), // Monotonic
            0 => { // Realtime
                let (d, m, y) = crate::os::get_date();
                let (h, min, s) = crate::os::get_time();
                let yrs = if y >= 1970 { (y - 1970) as u64 } else { 0 };
                let mut secs = yrs * 31_536_000 + (m as u64).saturating_sub(1) * 2_592_000 + (d as u64).saturating_sub(1) * 86_400 + (h as u64) * 3600 + (min as u64) * 60 + s as u64;
                Ok((secs * 1_000_000_000) + (crate::os::get_system_ticks() % 1000) * 1_000_000)
            }
            _ => Err(28)
        }
    }

    fn fd_close(&mut self, fd: i32) -> Result<(), i32> {
        if fd < 3 { return Ok(()); } // Don't close stdin/out/err for now
        if self.fd_table.remove(&fd).is_some() {
            Ok(())
        } else {
            Err(8) // EBADF
        }
    }

    fn fd_fdstat_get(&self, fd: i32) -> Result<FdStat, i32> {
        if fd >= 0 && fd <= 2 {
            Ok(FdStat { filetype: 2, rights_base: 0, rights_inheriting: 0, flags: 0 }) // Character device
        } else if self.fd_table.contains_key(&fd) || fd == 3 {
            Ok(FdStat { filetype: 4, rights_base: 0, rights_inheriting: 0, flags: 0 }) // Regular file or Dir
        } else {
            Err(8) // EBADF
        }
    }

    fn fd_fdstat_set_flags(&mut self, _fd: i32, _flags: u16) -> Result<(), i32> {
        // Stub: Assume flags set successfully
        Ok(())
    }

    fn fd_filestat_get(&self, fd: i32) -> Result<FileStat, i32> {
        if let Some(wf) = self.fd_table.get(&fd) {
            if let Ok(s) = wf.file.stat() {
                let filetype = if (s.mode & 0xF000) == 0x4000 { 3 } else { 4 };
                Ok(FileStat {
                    dev: s.dev,
                    ino: s.ino,
                    filetype,
                    nlink: s.nlink as u64,
                    size: s.size,
                    atime: s.atime * 1_000_000_000,
                    mtime: s.mtime * 1_000_000_000,
                    ctime: s.ctime * 1_000_000_000,
                })
            } else {
                Err(5) // EIO
            }
        } else if fd >= 0 && fd <= 2 {
             // Fake stat for stdio
             Ok(FileStat { dev: 0, ino: 0, filetype: 2, nlink: 1, size: 0, atime: 0, mtime: 0, ctime: 0 })
        } else if fd == 3 {
             // Fake stat for root
             Ok(FileStat { dev: 0, ino: 0, filetype: 3, nlink: 1, size: 0, atime: 0, mtime: 0, ctime: 0 })
        } else {
            Err(8)
        }
    }

    fn fd_filestat_set_size(&mut self, fd: i32, size: u64) -> Result<(), i32> {
        if let Some(wf) = self.fd_table.get_mut(&fd) {
            match wf.file.set_len(size) {
                Ok(_) => Ok(()),
                Err(_) => Err(28) // EINVAL or EIO
            }
        } else {
            Err(8)
        }
    }

    fn fd_filestat_set_times(&mut self, _fd: i32, _atime: u64, _mtime: u64, _fst_flags: u16) -> Result<(), i32> {
        // Stub: pretend success
        Ok(())
    }

    fn fd_prestat_get(&self, fd: i32) -> Result<u32, i32> {
        if fd == 3 { Ok(0) } // Preopen type dir (0)
        else { Err(8) }
    }

    fn fd_prestat_dir_name(&self, fd: i32) -> Result<String, i32> {
        if fd == 3 { Ok(String::from("/")) }
        else { Err(8) }
    }

    fn fd_read(&mut self, fd: i32, iovs: &mut [(&mut [u8])]) -> Result<usize, i32> {
        if fd == 0 {
            // Stdin unimplemented for now
            return Ok(0);
        }
        
        let wf = self.fd_table.get_mut(&fd).ok_or(8)?;
        use crate::io::Read;
        let mut total = 0;
        for buf in iovs {
            if let Ok(n) = wf.file.read(buf) {
                total += n;
                if n < buf.len() { break; }
            } else {
                return Err(5);
            }
        }
        Ok(total)
    }

    fn fd_pread(&mut self, fd: i32, iovs: &mut [(&mut [u8])], offset: u64) -> Result<usize, i32> {
        if let Some(wf) = self.fd_table.get(&fd) {
            let mut total = 0;
            let mut curr_offset = offset;
            for buf in iovs {
                let len = buf.len();
                let res = unsafe { syscall4(17, wf.file.as_raw_fd() as u64, buf.as_mut_ptr() as u64, len as u64, curr_offset) };
                if res == u64::MAX { return Err(5); }
                total += res as usize;
                curr_offset += res;
                if (res as usize) < len { break; }
            }
            Ok(total)
        } else {
            Err(8)
        }
    }

    fn fd_write(&mut self, fd: i32, iovs: &[&[u8]]) -> Result<usize, i32> {
        let mut total = 0;
        if fd == 1 || fd == 2 {
            for buf in iovs {
                // crate::debug_print! handles formatted string logic
                if let Ok(s) = core::str::from_utf8(buf) {
                    crate::debug_print!("{}", s);
                } else {
                    crate::debug_print!("{:?}", buf);
                }
                total += buf.len();
            }
            return Ok(total);
        }

        let wf = self.fd_table.get_mut(&fd).ok_or(8)?;
        use crate::io::Write;
        for buf in iovs {
            if let Ok(n) = wf.file.write(buf) {
                total += n;
            } else {
                return Err(5);
            }
        }
        Ok(total)
    }

    fn fd_pwrite(&mut self, fd: i32, iovs: &[&[u8]], offset: u64) -> Result<usize, i32> {
        if let Some(wf) = self.fd_table.get(&fd) {
            let mut total = 0;
            let mut curr_offset = offset;
            for buf in iovs {
                let len = buf.len();
                let res = unsafe { syscall4(18, wf.file.as_raw_fd() as u64, buf.as_ptr() as u64, len as u64, curr_offset) };
                if res == u64::MAX { return Err(5); }
                total += res as usize;
                curr_offset += res;
            }
            Ok(total)
        } else {
            Err(8)
        }
    }

    fn fd_seek(&mut self, fd: i32, offset: i64, whence: u8) -> Result<u64, i32> {
        let wf = self.fd_table.get_mut(&fd).ok_or(8)?;
        use crate::io::{Seek, SeekFrom};
        let p = match whence {
            0 => SeekFrom::Start(offset as u64),
            1 => SeekFrom::Current(offset),
            2 => SeekFrom::End(offset),
            _ => return Err(28),
        };
        wf.file.seek(p).map_err(|_| 28)
    }

    fn fd_tell(&mut self, fd: i32) -> Result<u64, i32> {
        let wf = self.fd_table.get_mut(&fd).ok_or(8)?;
        use crate::io::{Seek, SeekFrom};
        wf.file.seek(SeekFrom::Current(0)).map_err(|_| 28)
    }

    fn fd_sync(&mut self, _fd: i32) -> Result<(), i32> {
        Ok(())
    }

    fn fd_datasync(&mut self, _fd: i32) -> Result<(), i32> {
        Ok(())
    }

    fn fd_advise(&mut self, _fd: i32, _offset: u64, _len: u64, _advice: u8) -> Result<(), i32> {
        Ok(())
    }

    fn path_open(&mut self, dirfd: i32, _dirflags: u32, path: &str, oflags: u32, _fs_rights_base: u64, _fs_rights_inheriting: u64, _fdflags: u16) -> Result<i32, i32> {
        let full_path = self.resolve_path(dirfd, path)?;
        
        let res = if (oflags & 0x1) != 0 { fs::File::create(&full_path) } else { fs::File::open(&full_path) };
        match res {
            Ok(mut f) => {
                if (oflags & 0x8) != 0 { let _ = f.set_len(0); }
                let fd = self.next_fd;
                self.next_fd += 1;
                self.fd_table.insert(fd, WasiFile { file: f, path: full_path });
                Ok(fd)
            }
            Err(_) => Err(44) // ENOENT
        }
    }

    fn path_create_directory(&mut self, dirfd: i32, path: &str) -> Result<(), i32> {
        let full_path = self.resolve_path(dirfd, path)?;
        match fs::create_dir(&full_path) {
            Ok(_) => Ok(()),
            Err(_) => Err(28)
        }
    }

    fn path_remove_directory(&mut self, dirfd: i32, path: &str) -> Result<(), i32> {
        let full_path = self.resolve_path(dirfd, path)?;
        match fs::remove_dir(&full_path) {
            Ok(_) => Ok(()),
            Err(_) => Err(28)
        }
    }

    fn path_unlink_file(&mut self, dirfd: i32, path: &str) -> Result<(), i32> {
        let full_path = self.resolve_path(dirfd, path)?;
        match fs::remove_file(&full_path) {
            Ok(_) => Ok(()),
            Err(_) => Err(28)
        }
    }

    fn path_rename(&mut self, old_fd: i32, old_path: &str, new_fd: i32, new_path: &str) -> Result<(), i32> {
        let op = self.resolve_path(old_fd, old_path)?;
        let np = self.resolve_path(new_fd, new_path)?;
        match fs::rename(&op, &np) {
            Ok(_) => Ok(()),
            Err(_) => Err(28)
        }
    }

    fn path_readlink(&mut self, dirfd: i32, path: &str, buf: &mut [u8]) -> Result<usize, i32> {
        let res = unsafe { syscall5(267, dirfd as u64, path.as_ptr() as u64, path.len() as u64, buf.as_mut_ptr() as u64, buf.len() as u64) };
        if res == u64::MAX { Err(5) } else { Ok(res as usize) }
    }

    fn path_link(&mut self, old_fd: i32, _old_flags: u32, old_path: &str, new_fd: i32, new_path: &str) -> Result<(), i32> {
        let op = self.resolve_path(old_fd, old_path)?;
        let np = self.resolve_path(new_fd, new_path)?;
        // syscall6(265, 0, old_ptr, old_len, 0, new_ptr, new_len)
        let res = unsafe { syscall6(265, 0, op.as_ptr() as u64, op.len() as u64, 0, np.as_ptr() as u64, np.len() as u64) };
        if res == 0 { Ok(()) } else { Err(28) }
    }

    fn path_symlink(&mut self, old_path: &str, fd: i32, new_path: &str) -> Result<(), i32> {
        let np = self.resolve_path(fd, new_path)?;
        // syscall6(266, target_ptr, target_len, 0, new_ptr, new_len, 0)
        let res = unsafe { syscall6(266, old_path.as_ptr() as u64, old_path.len() as u64, 0, np.as_ptr() as u64, np.len() as u64, 0) };
        if res == 0 { Ok(()) } else { Err(28) }
    }

    fn path_filestat_get(&mut self, dirfd: i32, _flags: u32, path: &str) -> Result<FileStat, i32> {
        let full_path = self.resolve_path(dirfd, path)?;
        if let Ok(f) = fs::File::open(&full_path) {
            if let Ok(s) = f.stat() {
                let filetype = if (s.mode & 0xF000) == 0x4000 { 3 } else { 4 };
                return Ok(FileStat {
                    dev: s.dev,
                    ino: s.ino,
                    filetype,
                    nlink: s.nlink as u64,
                    size: s.size,
                    atime: s.atime * 1_000_000_000,
                    mtime: s.mtime * 1_000_000_000,
                    ctime: s.ctime * 1_000_000_000,
                });
            }
        }
        Err(44)
    }

    fn path_filestat_set_times(&mut self, dirfd: i32, _flags: u32, path: &str, atime: u64, mtime: u64, _fst_flags: u16) -> Result<(), i32> {
        let full_path = self.resolve_path(dirfd, path)?;
        // syscall5(280, 0, path_ptr, path_len, atime, mtime)
        let res = unsafe { syscall5(280, 0, full_path.as_ptr() as u64, full_path.len() as u64, atime, mtime) };
        if res == 0 { Ok(()) } else { Err(28) }
    }

    fn random_get(&mut self, buf: &mut [u8]) -> Result<(), i32> {
        if self.random_state == 0 { self.random_state = crate::os::get_system_ticks().wrapping_add(0xACE1BADE); }
        for i in 0..buf.len() {
            self.random_state ^= self.random_state << 13;
            self.random_state ^= self.random_state >> 17;
            self.random_state ^= self.random_state << 5;
            buf[i] = (self.random_state & 0xFF) as u8;
        }
        Ok(())
    }

    fn sched_yield(&mut self) -> Result<(), i32> {
        crate::os::yield_task();
        Ok(())
    }

    fn poll_oneoff(&mut self, in_events: &[u8], out_events: &mut [u8], nsubscriptions: u32) -> Result<u32, i32> {
        let mut events_written = 0;
        let mut min_delay_ns = u64::MAX;
        let mut has_delay = false;

        // 1. Scan for Clock Subscriptions to determine sleep time
        for i in 0..nsubscriptions as usize {
            let offset = i * 48;
            if offset + 48 > in_events.len() { break; }
            
            let tag = in_events[offset + 8];
            if tag == 0 { // Clock
                let timeout_bytes: [u8; 8] = in_events[offset+24..offset+32].try_into().unwrap();
                let timeout = u64::from_le_bytes(timeout_bytes);
                
                let flags_bytes: [u8; 2] = in_events[offset+40..offset+42].try_into().unwrap();
                let flags = u16::from_le_bytes(flags_bytes);
                
                let is_abs = (flags & 1) != 0;
                
                let delay = if is_abs {
                    let now = crate::os::get_system_ticks() * 1_000_000;
                    timeout.saturating_sub(now)
                } else {
                    timeout
                };
                
                if delay < min_delay_ns {
                    min_delay_ns = delay;
                    has_delay = true;
                }
            }
        }

        // 2. Sleep if needed
        if has_delay && min_delay_ns > 0 {
            // Round up to nearest ms
            let ms = (min_delay_ns + 999_999) / 1_000_000;
            if ms > 0 {
                crate::os::sleep(ms);
            }
        }

        // 3. Generate Events
        let now = crate::os::get_system_ticks() * 1_000_000;
        
        for i in 0..nsubscriptions as usize {
            let in_off = i * 48;
            if in_off + 48 > in_events.len() { break; }
            
            let userdata_bytes: [u8; 8] = in_events[in_off..in_off+8].try_into().unwrap();
            let tag = in_events[in_off + 8];
            
            let mut triggered = false;
            
            if tag == 0 { // Clock
                let timeout_bytes: [u8; 8] = in_events[in_off+24..in_off+32].try_into().unwrap();
                let timeout = u64::from_le_bytes(timeout_bytes);
                let flags_bytes: [u8; 2] = in_events[in_off+40..in_off+42].try_into().unwrap();
                let flags = u16::from_le_bytes(flags_bytes);
                let is_abs = (flags & 1) != 0;
                
                if is_abs {
                    if now >= timeout { triggered = true; }
                } else {
                    triggered = true; // Relative always triggers after sleep
                }
            } else if tag == 2 { // FdWrite
                // For now assume writes are always ready
                triggered = true; 
            } else if tag == 1 { // FdRead
                // Stub: assume ready? Or not? 
                // If we assume ready, `read` might block if we don't have non-blocking I/O.
                // But for tests, we usually want to proceed.
                triggered = true;
            }
            
            if triggered {
                let out_off = events_written * 32;
                if out_off + 32 > out_events.len() { break; }
                
                // Write Event
                out_events[out_off..out_off+8].copy_from_slice(&userdata_bytes);
                out_events[out_off+8] = 0; // Error
                out_events[out_off+9] = 0;
                out_events[out_off+10] = tag; // Type
                
                // Zero rest
                for j in 11..32 { out_events[out_off+j] = 0; }
                
                events_written += 1;
            }
        }

        Ok(events_written as u32)
    }

    fn proc_exit(&mut self, code: i32) -> ! {
        crate::debugln!("WASI: proc_exit({})", code);
        crate::os::exit(code as u64)
    }

    // Socket Stubs
    fn sock_accept(&mut self, _fd: i32, _flags: u16) -> Result<i32, i32> { Err(76) } // ENOTSUP
    fn sock_recv(&mut self, _fd: i32, _ri_data: &mut [(&mut [u8])], _ri_flags: u16) -> Result<(usize, u16), i32> { Err(76) }
    fn sock_send(&mut self, _fd: i32, _si_data: &[&[u8]], _si_flags: u16) -> Result<usize, i32> { Err(76) }
    fn sock_shutdown(&mut self, _fd: i32, _how: u8) -> Result<(), i32> { Err(76) }

    fn fd_readdir(&mut self, fd: i32, cookie: u64) -> Result<Vec<(String, u8, u64)>, i32> {
        let p = if fd == 3 {
            "@0xE0"
        } else if let Some(wf) = self.fd_table.get(&fd) {
            &wf.path
        } else {
            return Err(8); // EBADF
        };

        match crate::fs::read_dir(p) {
            Ok(re) => {
                let mut entries = Vec::new();
                for (i, e) in re.iter().enumerate() {
                    let wt = match e.file_type {
                        crate::fs::FileType::File => 4,
                        crate::fs::FileType::Directory => 3,
                        crate::fs::FileType::Device => 2,
                        _ => 0
                    };
                    entries.push((e.name.clone(), wt, (i + 1) as u64));
                }
                
                if cookie >= entries.len() as u64 {
                    return Ok(Vec::new());
                }
                
                Ok(entries.into_iter().skip(cookie as usize).collect())
            }
            Err(_) => Err(28),
        }
    }
}
