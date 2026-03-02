use crate::fs;
use crate::alloc::boxed::Box;
use crate::alloc::collections::BTreeMap;
use crate::alloc::format;
use crate::alloc::string::String;
use crate::alloc::vec::Vec;
#[cfg(not(target_arch = "wasm32"))]
use crate::sys::{syscall1, syscall4, syscall5, syscall6};
use crate::wasm::wasi::env::{FdStat, FileStat, WasiEnv};

const SYS_SOCKET: u64 = 41;
const SYS_BIND: u64 = 49;
const SYS_SENDTO: u64 = 44;
const SYS_RECVFROM: u64 = 45;

pub struct KrakeosWasiEnv {
    pub fd_table: BTreeMap<i32, Box<dyn WasiFileAny>>,
    pub stdio_map: [i32; 3],
    pub next_fd: i32,
    pub random_state: u64,
    pub args: Vec<String>,
    pub root_path: String,
    pub env_vars: Vec<(String, String)>,
}

pub trait WasiFile {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, i32>;
    fn write(&mut self, buf: &[u8]) -> Result<usize, i32>;
    fn seek(&mut self, pos: crate::io::SeekFrom) -> Result<u64, i32>;
    fn stat(&self) -> Result<crate::fs::Stat, i32>;
    fn set_len(&mut self, size: u64) -> Result<(), i32>;
    fn as_raw_fd(&self) -> usize;
}

pub struct WasiFsFile {
    pub file: fs::File,
    pub path: String,
}

impl WasiFile for WasiFsFile {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, i32> {
        use crate::io::Read;
        self.file.read(buf).map_err(|_| 5)
    }
    fn write(&mut self, buf: &[u8]) -> Result<usize, i32> {
        use crate::io::Write;
        self.file.write(buf).map_err(|_| 5)
    }
    fn seek(&mut self, pos: crate::io::SeekFrom) -> Result<u64, i32> {
        use crate::io::Seek;
        self.file.seek(pos).map_err(|_| 28)
    }
    fn stat(&self) -> Result<crate::fs::Stat, i32> {
        self.file.stat().map_err(|_| 5)
    }
    fn set_len(&mut self, size: u64) -> Result<(), i32> {
        self.file.set_len(size).map_err(|_| 5)
    }
    fn as_raw_fd(&self) -> usize {
        self.file.as_raw_fd()
    }
}

pub struct WasiSocket {
    pub fd: usize,
    pub dst_addr: Option<(u32, u16)>, // IPv4, Port
}

impl WasiFile for WasiSocket {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, i32> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            // SYS_RECVFROM(fd, buf, len, flags, src_addr, addr_len)
            let res = unsafe {
                syscall6(
                    SYS_RECVFROM,
                    self.fd as u64,
                    buf.as_mut_ptr() as u64,
                    buf.len() as u64,
                    0,
                    0,
                    0,
                )
            };
            if res == u64::MAX {
                Err(5)
            } else {
                Ok(res as usize)
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            // Stub or use crates::wasi::sockets
            Err(58) // ENOTSUP
        }
    }
    fn write(&mut self, buf: &[u8]) -> Result<usize, i32> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            // SYS_SENDTO(fd, buf, len, flags, dest_addr, addr_len)
            let (dst_ptr, dst_len) = if let Some((ip, port)) = self.dst_addr {
                let mut addr = [0u8; 16];
                addr[0] = 2; // AF_INET
                addr[1] = 0;
                addr[2] = (port >> 8) as u8;
                addr[3] = (port & 0xFF) as u8;
                addr[4] = (ip >> 24) as u8;
                addr[5] = (ip >> 16) as u8;
                addr[6] = (ip >> 8) as u8;
                addr[7] = (ip & 0xFF) as u8;
                (addr.as_ptr() as u64, 16)
            } else {
                (0, 0)
            };

            if dst_ptr == 0 {
                return Err(28);
            }

            let res = unsafe {
                syscall6(
                    SYS_SENDTO,
                    self.fd as u64,
                    buf.as_ptr() as u64,
                    buf.len() as u64,
                    0,
                    dst_ptr,
                    dst_len,
                )
            };
            if res == u64::MAX {
                Err(5)
            } else {
                Ok(res as usize)
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            Err(58)
        }
    }
    fn seek(&mut self, _pos: crate::io::SeekFrom) -> Result<u64, i32> {
        Err(29)
    } // ESPIPE
    fn stat(&self) -> Result<crate::fs::Stat, i32> {
        Ok(crate::fs::Stat {
            dev: 0,
            ino: 0,
            mode: 0,
            nlink: 1,
            size: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
            _reserved: [0; 1],
        })
    }
    fn set_len(&mut self, _size: u64) -> Result<(), i32> {
        Err(28)
    }
    fn as_raw_fd(&self) -> usize {
        self.fd
    }
}

impl Drop for WasiSocket {
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        unsafe {
            syscall1(50, self.fd as u64)
        };
        #[cfg(target_arch = "wasm32")]
        { /* Use socket drop binding */ }
    }
}

impl Default for KrakeosWasiEnv {
    fn default() -> Self {
        Self {
            fd_table: BTreeMap::new(),
            stdio_map: [0, 1, 2],
            next_fd: 4, // 0,1,2 are reserved, 3 is preopened root
            random_state: 0,
            args: Vec::new(),
            root_path: String::from("@0xE0"),
            env_vars: Vec::new(),
        }
    }
}

impl KrakeosWasiEnv {
    pub fn new(args: Vec<String>, root_path: String, fds: &[(u8, u8)]) -> Self {
        Self::new_with_env(args, root_path, fds, Vec::new())
    }

    pub fn new_with_env(
        args: Vec<String>,
        root_path: String,
        fds: &[(u8, u8)],
        env_vars: Vec<(String, String)>,
    ) -> Self {
        let mut stdio_map = [0, 1, 2];
        for &(guest, host) in fds {
            if guest < 3 {
                stdio_map[guest as usize] = host as i32;
            }
        }

        Self {
            fd_table: BTreeMap::new(),
            stdio_map,
            next_fd: 4,
            random_state: 0,
            args,
            root_path,
            env_vars,
        }
    }

    fn resolve_path(&self, dirfd: i32, path: &str) -> Result<String, i32> {
        if path.starts_with("/dev/udp") {
            return Ok(String::from(path));
        }
        if path.starts_with('@') {
            return Ok(String::from(path));
        }
        if path.contains("..") {
            return Err(76);
        } // ENOTCAPABLE

        let base = if dirfd == 3 {
            &self.root_path
        } else if let Some(wf) = self.fd_table.get(&dirfd) {
            if let Some(f) = wf.as_any().downcast_ref::<WasiFsFile>() {
                &f.path
            } else {
                return Err(54); // ENOTDIR
            }
        } else {
            return Err(76); // ENOTCAPABLE
        };

        let clean = path
            .trim_start_matches('.')
            .trim_start_matches('/')
            .trim_end_matches('/');
        if clean.is_empty() {
            return Ok(base.clone());
        }

        if base.ends_with('/') {
            Ok(format!("{}{}", base, clean))
        } else {
            Ok(format!("{}/{}", base, clean))
        }
    }
}

// Helper trait to allow downcasting
pub trait WasiFileAny: WasiFile + core::any::Any {
    fn as_any(&self) -> &dyn core::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn core::any::Any;
}
impl<T: WasiFile + core::any::Any> WasiFileAny for T {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}
// Shim to make Box<dyn WasiFile> work if we change definition
// Actually, let's change fd_table to Box<dyn WasiFileAny>
// Or just implement resolve_path differently.

impl WasiEnv for KrakeosWasiEnv {
    fn args_get(&self) -> Result<Vec<String>, i32> {
        Ok(self.args.clone())
    }

    fn environ_get(&self) -> Result<Vec<(String, String)>, i32> {
        let mut vars = self.env_vars.clone();
        // Auto-derive common env vars if not already set
        if !vars.iter().any(|(k, _)| k == "HOME") {
            vars.push((String::from("HOME"), String::from("/")));
        }
        if !vars.iter().any(|(k, _)| k == "PWD") {
            vars.push((String::from("PWD"), String::from("/")));
        }
        // Auto-detect Python and set PYTHONHOME/PYTHONPATH
        // IMPORTANT: These must be WASI-relative paths (relative to preopened dir "/")
        // because resolve_path(dirfd=3, path) prepends root_path.
        // Python will pass these paths through path_open(dirfd=3, ...).
        let is_python = self
            .args
            .first()
            .map(|a| a.contains("python"))
            .unwrap_or(false);
        if is_python {
            if !vars.iter().any(|(k, _)| k == "PYTHONHOME") {
                vars.push((String::from("PYTHONHOME"), String::from("/")));
            }
            if !vars.iter().any(|(k, _)| k == "PYTHONPATH") {
                vars.push((String::from("PYTHONPATH"), String::from("/lib/python3.13")));
            }
            if !vars.iter().any(|(k, _)| k == "PYTHONDONTWRITEBYTECODE") {
                vars.push((String::from("PYTHONDONTWRITEBYTECODE"), String::from("1")));
            }
            // Force UTF-8 mode - on WASI, libc locale detection (nl_langinfo)
            // doesn't work, so Python can't auto-detect the filesystem encoding.
            if !vars.iter().any(|(k, _)| k == "PYTHONUTF8") {
                vars.push((String::from("PYTHONUTF8"), String::from("1")));
            }
            if !vars.iter().any(|(k, _)| k == "PYTHONIOENCODING") {
                vars.push((String::from("PYTHONIOENCODING"), String::from("utf-8")));
            }
            if !vars.iter().any(|(k, _)| k == "LC_ALL") {
                vars.push((String::from("LC_ALL"), String::from("C.UTF-8")));
            }
        }
        Ok(vars)
    }

    fn clock_res_get(&self, _id: u32) -> Result<u64, i32> {
        Ok(1_000_000) // 1ms resolution
    }

    fn clock_time_get(&self, _id: u32, _precision: u64) -> Result<u64, i32> {
        let (d, m, y) = crate::os::get_date();
        let (h, min, s) = crate::os::get_time();
        let yrs = if y >= 1970 { (y - 1970) as u64 } else { 0 };
        let secs = yrs * 31_536_000
            + (m as u64).saturating_sub(1) * 2_592_000
            + (d as u64).saturating_sub(1) * 86_400
            + (h as u64) * 3600
            + (min as u64) * 60
            + s as u64;
        Ok(secs * 1_000_000_000)
    }

    fn fd_close(&mut self, fd: i32) -> Result<(), i32> {
        if fd < 3 {
            return Ok(());
        }
        if let Some(_) = self.fd_table.remove(&fd) {
            // Drop handles close
            Ok(())
        } else {
            Err(8)
        }
    }

    fn fd_fdstat_get(&self, fd: i32) -> Result<FdStat, i32> {
        if fd >= 0 && fd <= 2 {
            let rb = if fd == 0 { 0x2 } else { 0x40 };
            Ok(FdStat {
                filetype: 2,
                rights_base: rb,
                rights_inheriting: rb,
                flags: 0,
            })
        } else if let Some(wf) = self.fd_table.get(&fd) {
            let ft = match wf.stat() {
                Ok(s) => {
                    if (s.mode & 0xF000) == 0x4000 {
                        3
                    } else {
                        4
                    }
                }
                Err(_) => 4,
            };
            let (rb, ri) = if ft == 3 {
                (u64::MAX, u64::MAX)
            } else {
                (0x3F, 0x3F)
            };
            Ok(FdStat {
                filetype: ft,
                rights_base: rb,
                rights_inheriting: ri,
                flags: 0,
            })
        } else if fd == 3 {
            Ok(FdStat {
                filetype: 3,
                rights_base: u64::MAX,
                rights_inheriting: u64::MAX,
                flags: 0,
            })
        } else {
            Err(8)
        }
    }

    fn fd_filestat_get(&self, fd: i32) -> Result<FileStat, i32> {
        if let Some(wf) = self.fd_table.get(&fd) {
            if let Ok(s) = wf.stat() {
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
                Err(5)
            }
        } else if fd >= 0 && fd <= 2 {
            Ok(FileStat {
                dev: 0,
                ino: 0,
                filetype: 2,
                nlink: 1,
                size: 0,
                atime: 0,
                mtime: 0,
                ctime: 0,
            })
        } else if fd == 3 {
            Ok(FileStat {
                dev: 0,
                ino: 0,
                filetype: 3,
                nlink: 1,
                size: 0,
                atime: 0,
                mtime: 0,
                ctime: 0,
            })
        } else {
            Err(8)
        }
    }

    fn fd_filestat_set_size(&mut self, fd: i32, size: u64) -> Result<(), i32> {
        if let Some(wf) = self.fd_table.get_mut(&fd) {
            match wf.set_len(size) {
                Ok(_) => Ok(()),
                Err(_) => Err(28),
            }
        } else {
            Err(8)
        }
    }

    fn fd_read(&mut self, fd: i32, iovs: &mut [(&mut [u8])]) -> Result<usize, i32> {
        if fd == 0 {
            let host_fd = self.stdio_map[0] as usize;
            let host_stdout = self.stdio_map[1] as usize;
            loop {
                let mut total = 0;
                for buf in iovs.iter_mut() {
                    let n = crate::os::file_read(host_fd, buf);
                    if n > 0 && n <= buf.len() {
                        // Translate \r to \n
                        for i in 0..n {
                            if buf[i] == b'\r' {
                                buf[i] = b'\n';
                            }
                        }
                        // Echo stdin to stdout for interactive programs
                        crate::os::file_write(host_stdout, &buf[..n]);
                        total += n;
                        if n < buf.len() {
                            break;
                        }
                    } else if n == usize::MAX - 1 {
                        // EWOULDBLOCK, yield and retry if total == 0
                        break;
                    }
                }
                if total > 0 {
                    return Ok(total);
                }
                crate::os::yield_task();
            }
        }

        let wf = self.fd_table.get_mut(&fd).ok_or(8)?;
        let mut total = 0;
        for buf in iovs {
            match wf.read(buf) {
                Ok(n) => {
                    total += n;
                    if n < buf.len() {
                        break;
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Ok(total)
    }

    fn fd_pread(&mut self, fd: i32, iovs: &mut [(&mut [u8])], offset: u64) -> Result<usize, i32> {
        let wf = self.fd_table.get_mut(&fd).ok_or(8)?;
        // Save current position, seek to offset, read, restore
        use crate::io::{Seek, SeekFrom};
        let saved = wf.seek(SeekFrom::Current(0)).unwrap_or(0);
        wf.seek(SeekFrom::Start(offset)).map_err(|_| 29)?;
        let mut total = 0;
        for buf in iovs.iter_mut() {
            match wf.read(buf) {
                Ok(n) => {
                    total += n;
                    if n < buf.len() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = wf.seek(SeekFrom::Start(saved));
                    return Err(e);
                }
            }
        }
        let _ = wf.seek(SeekFrom::Start(saved));
        Ok(total)
    }

    fn fd_write(&mut self, fd: i32, iovs: &[&[u8]]) -> Result<usize, i32> {
        if fd == 1 || fd == 2 {
            let host_fd = self.stdio_map[fd as usize] as usize;
            // Batch all iovecs into one system call to avoid per-iov overhead
            let total_len: usize = iovs.iter().map(|b| b.len()).sum();
            if total_len == 0 {
                return Ok(0);
            }
            if iovs.len() == 1 {
                // Fast path: single iov, no copy needed
                return Ok(crate::os::file_write(host_fd, iovs[0]));
            }
            let mut batch = crate::alloc::vec::Vec::with_capacity(total_len);
            for buf in iovs {
                batch.extend_from_slice(buf);
            }
            return Ok(crate::os::file_write(host_fd, &batch));
        }

        let wf = self.fd_table.get_mut(&fd).ok_or(8)?;
        let mut total = 0;
        for buf in iovs {
            match wf.write(buf) {
                Ok(n) => total += n,
                Err(e) => return Err(e),
            }
        }
        Ok(total)
    }

    fn fd_pwrite(&mut self, fd: i32, iovs: &[&[u8]], offset: u64) -> Result<usize, i32> {
        let wf = self.fd_table.get_mut(&fd).ok_or(8)?;
        use crate::io::{Seek, SeekFrom};
        let saved = wf.seek(SeekFrom::Current(0)).unwrap_or(0);
        wf.seek(SeekFrom::Start(offset)).map_err(|_| 29)?;
        let mut total = 0;
        for buf in iovs {
            match wf.write(buf) {
                Ok(n) => total += n,
                Err(e) => {
                    let _ = wf.seek(SeekFrom::Start(saved));
                    return Err(e);
                }
            }
        }
        let _ = wf.seek(SeekFrom::Start(saved));
        Ok(total)
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
        wf.seek(p)
    }

    fn fd_tell(&mut self, fd: i32) -> Result<u64, i32> {
        let wf = self.fd_table.get_mut(&fd).ok_or(8)?;
        use crate::io::{Seek, SeekFrom};
        wf.seek(SeekFrom::Current(0))
    }

    fn fd_renumber(&mut self, from: i32, to: i32) -> Result<(), i32> {
        if !self.fd_table.contains_key(&from) {
            return Err(8);
        }
        if self.fd_table.contains_key(&to) {
            self.fd_close(to)?;
        }
        let f = self.fd_table.remove(&from).unwrap();
        self.fd_table.insert(to, f);
        Ok(())
    }

    fn fd_prestat_get(&self, fd: i32) -> Result<u32, i32> {
        if fd == 3 {
            Ok(0) // PrestatDir
        } else {
            Err(8)
        }
    }

    fn fd_prestat_dir_name(&self, fd: i32) -> Result<String, i32> {
        if fd == 3 {
            Ok(String::from("/"))
        } else {
            Err(8)
        }
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
    fn fd_fdstat_set_flags(&mut self, _fd: i32, _flags: u16) -> Result<(), i32> {
        Ok(())
    }
    fn fd_filestat_set_times(
        &mut self,
        _fd: i32,
        _atime: u64,
        _mtime: u64,
        _fst_flags: u16,
    ) -> Result<(), i32> {
        Ok(())
    }

    fn path_open(
        &mut self,
        dirfd: i32,
        _dirflags: u32,
        path: &str,
        oflags: u32,
        _fs_rights_base: u64,
        _fs_rights_inheriting: u64,
        _fdflags: u16,
    ) -> Result<i32, i32> {
        if path.starts_with("/dev/udp") {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let sock_fd = unsafe { syscall4(SYS_SOCKET, 2, 2, 0, 0) };
                if sock_fd == u64::MAX {
                    return Err(5);
                }

                let parts: Vec<&str> = path.split('/').collect();
                let mut dst_addr = None;

                if parts.len() >= 5 && parts[3] == "bind" {
                    if let Ok(port) = parts[4].parse::<u16>() {
                        let mut addr = [0u8; 16];
                        addr[0] = 2; // AF_INET
                        addr[2] = (port >> 8) as u8;
                        addr[3] = (port & 0xFF) as u8;
                        let res =
                            unsafe { syscall4(SYS_BIND, sock_fd, addr.as_ptr() as u64, 16, 0) };
                        if res != 0 {
                            return Err(48);
                        }
                    }
                } else if parts.len() >= 5 {
                    if let Ok(port) = parts[4].parse::<u16>() {
                        let ip_parts: Vec<&str> = parts[3].split('.').collect();
                        if ip_parts.len() == 4 {
                            let a = ip_parts[0].parse::<u8>().unwrap_or(0);
                            let b = ip_parts[1].parse::<u8>().unwrap_or(0);
                            let c = ip_parts[2].parse::<u8>().unwrap_or(0);
                            let d = ip_parts[3].parse::<u8>().unwrap_or(0);
                            let ip_u32 = ((a as u32) << 24)
                                | ((b as u32) << 16)
                                | ((c as u32) << 8)
                                | (d as u32);
                            dst_addr = Some((ip_u32, port));
                        }
                    }
                }

                let fd = self.next_fd;
                self.next_fd += 1;
                self.fd_table.insert(
                    fd,
                    Box::new(WasiSocket {
                        fd: sock_fd as usize,
                        dst_addr,
                    }),
                );
                return Ok(fd);
            }
            #[cfg(target_arch = "wasm32")]
            {
                return Err(58);
            }
        }

        let full_path = self.resolve_path(dirfd, path)?;
        crate::debugln!("[WASI] path_open: '{}' -> '{}'", path, full_path);

        let res = if (oflags & 0x1) != 0 {
            fs::File::create(&full_path)
        } else {
            fs::File::open(&full_path)
        };
        match res {
            Ok(mut f) => {
                if (oflags & 0x8) != 0 {
                    let _ = f.set_len(0);
                }
                let fd = self.next_fd;
                self.next_fd += 1;
                self.fd_table.insert(
                    fd,
                    Box::new(WasiFsFile {
                        file: f,
                        path: full_path,
                    }),
                );
                Ok(fd)
            }
            Err(_) => {
                crate::debugln!("[WASI] path_open FAILED: '{}'", full_path);
                Err(44)
            }
        }
    }

    fn path_create_directory(&mut self, dirfd: i32, path: &str) -> Result<(), i32> {
        let full_path = self.resolve_path(dirfd, path)?;
        if crate::fs::create_dir(&full_path).is_ok() {
            Ok(())
        } else {
            Err(5)
        }
    }

    fn path_remove_directory(&mut self, dirfd: i32, path: &str) -> Result<(), i32> {
        let full_path = self.resolve_path(dirfd, path)?;
        if crate::fs::remove_dir(&full_path).is_ok() {
            Ok(())
        } else {
            Err(5)
        }
    }

    fn path_unlink_file(&mut self, dirfd: i32, path: &str) -> Result<(), i32> {
        let full_path = self.resolve_path(dirfd, path)?;
        if crate::fs::remove_file(&full_path).is_ok() {
            Ok(())
        } else {
            Err(5)
        }
    }

    fn path_rename(
        &mut self,
        old_fd: i32,
        old_path: &str,
        new_fd: i32,
        new_path: &str,
    ) -> Result<(), i32> {
        let full_old = self.resolve_path(old_fd, old_path)?;
        let full_new = self.resolve_path(new_fd, new_path)?;
        crate::fs::rename(&full_old, &full_new).map_err(|_| 28)
    }

    fn path_readlink(&mut self, _dirfd: i32, _path: &str, _buf: &mut [u8]) -> Result<usize, i32> {
        Err(58)
    }
    fn path_link(
        &mut self,
        _old_fd: i32,
        _old_flags: u32,
        _old_path: &str,
        _new_fd: i32,
        _new_path: &str,
    ) -> Result<(), i32> {
        Err(58)
    }
    fn path_symlink(&mut self, _old_path: &str, _fd: i32, _new_path: &str) -> Result<(), i32> {
        Err(58)
    }
    fn path_filestat_get(&mut self, dirfd: i32, _flags: u32, path: &str) -> Result<FileStat, i32> {
        let full_path = self.resolve_path(dirfd, path)?;
        // Open to stat? Or just stat path? Krakeos fs might have stat path.
        // Assuming no symlinks for now.
        if let Ok(f) = fs::File::open(&full_path) {
            if let Ok(s) = f.stat() {
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
                Err(5)
            }
        } else {
            Err(44)
        }
    }
    fn path_filestat_set_times(
        &mut self,
        _dirfd: i32,
        _flags: u32,
        _path: &str,
        _atime: u64,
        _mtime: u64,
        _fst_flags: u16,
    ) -> Result<(), i32> {
        Ok(())
    }

    fn fd_readdir(&mut self, fd: i32, cookie: u64) -> Result<Vec<(String, u8, u64)>, i32> {
        let p = if fd == 3 {
            self.root_path.as_str()
        } else if let Some(wf) = self.fd_table.get(&fd) {
            if let Some(f) = wf.as_any().downcast_ref::<WasiFsFile>() {
                &f.path
            } else {
                crate::debugln!("[WASI] fd_readdir fd={}: not a WasiFsFile", fd);
                return Err(54);
            }
        } else {
            crate::debugln!("[WASI] fd_readdir fd={}: not found", fd);
            return Err(8);
        };
        crate::debugln!("[WASI] fd_readdir fd={} cookie={} path='{}'", fd, cookie, p);
        match crate::fs::read_dir(p) {
            Ok(re) => {
                let mut entries = Vec::new();
                for (i, e) in re.iter().enumerate() {
                    let wt = match e.file_type {
                        crate::fs::FileType::File => 4,
                        crate::fs::FileType::Directory => 3,
                        crate::fs::FileType::Device => 2,
                        _ => 0,
                    };
                    entries.push((e.name.clone(), wt, (i + 1) as u64));
                }
                crate::debugln!(
                    "[WASI] fd_readdir: {} total entries, returning {} (after cookie {})",
                    entries.len(),
                    entries.len().saturating_sub(cookie as usize),
                    cookie
                );
                if cookie >= entries.len() as u64 {
                    return Ok(Vec::new());
                }
                Ok(entries.into_iter().skip(cookie as usize).collect())
            }
            Err(_) => {
                crate::debugln!("[WASI] fd_readdir: read_dir FAILED for '{}'", p);
                Err(28)
            }
        }
    }

    fn random_get(&mut self, buf: &mut [u8]) -> Result<(), i32> {
        // Pseudo-random
        for b in buf.iter_mut() {
            self.random_state = self
                .random_state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1);
            *b = (self.random_state >> 33) as u8;
        }
        Ok(())
    }

    fn sched_yield(&mut self) -> Result<(), i32> {
        crate::os::yield_task();
        Ok(())
    }

    fn poll_oneoff(
        &mut self,
        in_events: &[u8],
        out_events: &mut [u8],
        nsubscriptions: u32,
    ) -> Result<u32, i32> {
        // Parse WASI subscriptions (each 48 bytes) and handle clock / fd_read / fd_write
        let mut events_written = 0u32;
        for i in 0..nsubscriptions as usize {
            let base = i * 48;
            if base + 48 > in_events.len() {
                break;
            }
            // Subscription layout:
            // [0..8] = userdata (u64)
            // [8] = type tag (0=clock, 1=fd_read, 2=fd_write)
            let userdata =
                u64::from_le_bytes(in_events[base..base + 8].try_into().unwrap_or([0; 8]));
            let event_type = in_events[base + 8];

            let out_base = events_written as usize * 32;
            if out_base + 32 > out_events.len() {
                break;
            }

            match event_type {
                0 => {
                    // Clock subscription
                    // [16..24] = clock_id (u32)
                    // [24..32] = timeout (u64 in ns)
                    // [32..40] = precision (u64)
                    // [40..42] = flags (u16) - bit 0 = absolute
                    let timeout_ns = u64::from_le_bytes(
                        in_events[base + 24..base + 32].try_into().unwrap_or([0; 8]),
                    );
                    let flags = u16::from_le_bytes(
                        in_events[base + 40..base + 42].try_into().unwrap_or([0; 2]),
                    );

                    if flags & 1 == 0 {
                        // Relative timeout: sleep
                        let ms = timeout_ns / 1_000_000;
                        if ms > 0 {
                            crate::os::sleep(ms);
                        } else {
                            crate::os::yield_task();
                        }
                    } else {
                        // Absolute timeout: just yield for now
                        crate::os::yield_task();
                    }

                    // Write event result
                    // [0..8] = userdata
                    out_events[out_base..out_base + 8].copy_from_slice(&userdata.to_le_bytes());
                    // [8..10] = errno (0 = success)
                    out_events[out_base + 8..out_base + 10].copy_from_slice(&0u16.to_le_bytes());
                    // [10] = type (0 = clock)
                    out_events[out_base + 10] = 0;
                    // [11..32] = padding/reserved
                    for j in 11..32 {
                        out_events[out_base + j] = 0;
                    }
                    events_written += 1;
                }
                1 => {
                    // FD Read subscription
                    // [16..20] = fd (i32)
                    // Just report ready immediately (non-blocking check not available)
                    out_events[out_base..out_base + 8].copy_from_slice(&userdata.to_le_bytes());
                    out_events[out_base + 8..out_base + 10].copy_from_slice(&0u16.to_le_bytes());
                    out_events[out_base + 10] = 1; // type = fd_read
                    for j in 11..32 {
                        out_events[out_base + j] = 0;
                    }
                    events_written += 1;
                }
                2 => {
                    // FD Write subscription
                    out_events[out_base..out_base + 8].copy_from_slice(&userdata.to_le_bytes());
                    out_events[out_base + 8..out_base + 10].copy_from_slice(&0u16.to_le_bytes());
                    out_events[out_base + 10] = 2; // type = fd_write
                    for j in 11..32 {
                        out_events[out_base + j] = 0;
                    }
                    events_written += 1;
                }
                _ => {
                    // Unknown type - report error
                    out_events[out_base..out_base + 8].copy_from_slice(&userdata.to_le_bytes());
                    out_events[out_base + 8..out_base + 10].copy_from_slice(&28u16.to_le_bytes()); // EINVAL
                    out_events[out_base + 10] = event_type;
                    for j in 11..32 {
                        out_events[out_base + j] = 0;
                    }
                    events_written += 1;
                }
            }
        }
        Ok(events_written)
    }

    fn proc_exit(&mut self, code: i32) -> Result<(), i32> {
        // Return Err(code) so the WASM interpreter catches it and halts the guest execution
        // instead of killing the host process.
        Err(code)
    }

    fn initial_cwd(&self) -> Result<String, i32> {
        Ok(self.root_path.clone())
    }

    fn sock_accept(&mut self, _fd: i32, _flags: u16) -> Result<i32, i32> {
        Err(58)
    }
    fn sock_recv(
        &mut self,
        fd: i32,
        ri_data: &mut [(&mut [u8])],
        _ri_flags: u16,
    ) -> Result<(usize, u16), i32> {
        let wf = self.fd_table.get_mut(&fd).ok_or(8)?;
        // Only if it's a socket
        if let Some(s) = wf.as_any_mut().downcast_mut::<WasiSocket>() {
            let mut total = 0;
            for buf in ri_data {
                match s.read(buf) {
                    Ok(n) => total += n,
                    Err(e) => return Err(e),
                }
            }
            Ok((total, 0))
        } else {
            Err(8)
        }
    }
    fn sock_send(&mut self, fd: i32, si_data: &[&[u8]], _si_flags: u16) -> Result<usize, i32> {
        let wf = self.fd_table.get_mut(&fd).ok_or(8)?;
        if let Some(s) = wf.as_any_mut().downcast_mut::<WasiSocket>() {
            let mut total = 0;
            for buf in si_data {
                match s.write(buf) {
                    Ok(n) => total += n,
                    Err(e) => return Err(e),
                }
            }
            Ok(total)
        } else {
            Err(8)
        }
    }
    fn sock_shutdown(&mut self, _fd: i32, _how: u8) -> Result<(), i32> {
        Ok(())
    }

    fn stdio_map(&self) -> [i32; 3] {
        self.stdio_map
    }
}
