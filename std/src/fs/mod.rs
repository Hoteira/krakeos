use crate::io::{Error, Read, Result, Seek, SeekFrom, Write};
use alloc::string::String;
use alloc::vec::Vec;

pub mod host;
pub mod async_file;
#[cfg(any(feature = "userland", target_arch = "x86_64"))]
pub mod wasi;
pub use async_file::AsyncFile;

// Re-export key bindings for callers that use crate::fs::descriptor_drop etc.
pub use host::{open_at, stat, set_size, seek, descriptor_drop};
pub use host::{create_directory_at, unlink_file_at, remove_directory_at, rename_at};

// --- Types ---

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Stat {
    pub dev: u64,
    pub ino: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub nlink: u32,
    pub size: u64,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
}

#[derive(Debug)]
pub struct File {
    fd: usize,
}

impl Drop for File {
    fn drop(&mut self) {
        if self.fd > 2 {
            descriptor_drop(self.fd as i32);
        }
    }
}

impl File {
    pub fn open(path: &str) -> Result<Self> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut result = [0u8; 8];
            open_at(3, 0, path.as_ptr(), path.len(), 0, 0, result.as_mut_ptr());

            if result[0] != 0 {
                Err(Error::from_raw_os_error(2)) // ENOENT
            } else {
                let fd = unsafe { core::ptr::read_unaligned(result.as_ptr().add(4) as *const i32) };
                Ok(File { fd: fd as usize })
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            let res = crate::os::native_file_open(path.as_ptr(), path.len() as u64, 0);
            if res < 0 {
                Err(Error::from_raw_os_error(2))
            } else {
                Ok(File { fd: res as usize })
            }
        }
    }

    pub fn create(path: &str) -> Result<Self> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut result = [0u8; 8];
            open_at(3, 0, path.as_ptr(), path.len(), 1, 0, result.as_mut_ptr());
            if result[0] != 0 {
                Err(Error::from_raw_os_error(1))
            } else {
                let fd = unsafe { core::ptr::read_unaligned(result.as_ptr().add(4) as *const i32) };
                Ok(File { fd: fd as usize })
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            let res = crate::os::native_file_open(path.as_ptr(), path.len() as u64, 1);
            if res < 0 {
                Err(Error::from_raw_os_error(5))
            } else {
                Ok(File { fd: res as usize })
            }
        }
    }

    pub fn size(&self) -> usize {
        self.stat().map(|s| s.size as usize).unwrap_or(0)
    }

    pub fn stat(&self) -> Result<Stat> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut result = [0u8; 128];
            stat(self.fd as i32, result.as_mut_ptr());
            if result[0] != 0 {
                Err(Error::from_raw_os_error(5))
            } else {
                let s = unsafe { core::ptr::read_unaligned(result.as_ptr().add(8) as *const Stat) };
                Ok(s)
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            let mut s = unsafe { core::mem::zeroed::<Stat>() };
            let res = crate::os::native_file_stat(self.fd as u64, &mut s as *mut _ as *mut u8);
            if res != 0 {
                Err(Error::from_raw_os_error(5))
            } else {
                Ok(s)
            }
        }
    }

    pub fn as_raw_fd(&self) -> usize {
        self.fd
    }

    pub fn from_raw_fd(fd: usize) -> Self {
        File { fd }
    }

    pub fn set_len(&self, size: u64) -> Result<()> {
        let mut result = [0u8; 8];
        set_size(self.fd as i32, size, result.as_mut_ptr());
        if result[0] == 0 {
            Ok(())
        } else {
            Err(Error::from_raw_os_error(5))
        }
    }

    pub fn sync_all(&self) -> Result<()> {
        Ok(())
    }
}

impl Read for File {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let res = unsafe { crate::sys::syscall(0, self.fd as u64, buffer.as_mut_ptr() as u64, buffer.len() as u64) };
            if res == u64::MAX {
                return Err(Error::from_raw_os_error(5));
            }
            if res == u64::MAX - 1 {
                return Ok(0);
            }
            return Ok(res as usize);
        }

        #[cfg(target_arch = "wasm32")]
        {
            let n = crate::os::file_read(self.fd, buffer);
            if n == usize::MAX {
                return Err(Error::from_raw_os_error(5));
            }
            if n == usize::MAX - 1 {
                return Ok(0);
            }
            Ok(n)
        }
    }
}

impl Write for File {
    fn write(&mut self, buffer: &[u8]) -> Result<usize> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let res = unsafe { crate::sys::syscall(1, self.fd as u64, buffer.as_ptr() as u64, buffer.len() as u64) };
            if res == u64::MAX {
                return Err(Error::from_raw_os_error(5));
            }
            return Ok(res as usize);
        }

        #[cfg(target_arch = "wasm32")]
        {
            let n = crate::os::file_write(self.fd, buffer);
            if n == usize::MAX {
                return Err(Error::from_raw_os_error(5));
            }
            Ok(n)
        }
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl Seek for File {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        let (offset, whence) = match pos {
            SeekFrom::Start(off) => (off as i64, 0),
            SeekFrom::Current(off) => (off, 1),
            SeekFrom::End(off) => (off, 2),
        };

        let mut result = [0u8; 16];
        host::seek(self.fd as i32, offset as u64, whence, result.as_mut_ptr());

        if result[0] == 0 {
            Ok(unsafe { core::ptr::read_unaligned(result.as_ptr().add(8) as *const u64) })
        } else {
            Err(Error::from_raw_os_error(29)) // ESPIPE
        }
    }
}

pub fn create_dir(path: &str) -> Result<()> {
    let res = host::create_dir(path);
    if res == 0 {
        Ok(())
    } else {
        Err(Error::from_raw_os_error(1))
    }
}

pub fn remove_file(path: &str) -> Result<()> {
    let res = host::remove_file(path);
    if res == 0 {
        Ok(())
    } else {
        Err(Error::from_raw_os_error(1))
    }
}

pub fn remove_dir(path: &str) -> Result<()> {
    let res = host::remove_dir(path);
    if res == 0 {
        Ok(())
    } else {
        Err(Error::from_raw_os_error(1))
    }
}

pub fn rename(from: &str, to: &str) -> Result<()> {
    let res = host::rename(from, to);
    if res == 0 {
        Ok(())
    } else {
        Err(Error::from_raw_os_error(1))
    }
}

pub fn mount(disk_id: u8, fs_type: &str) -> Result<()> {
    crate::debugln!("CALLING TCP FN mount WITH ARGS: disk_id={}, fs_type={}", disk_id, fs_type);
    let res = host::mount_host(disk_id as u64, fs_type.as_ptr(), fs_type.len());
    crate::debugln!("TCP RESULT: mount RESULT: {}", res);
    if res == 0 {
        Ok(())
    } else {
        Err(Error::from_raw_os_error(1))
    }
}

pub fn read(path: &str) -> Result<Vec<u8>> {
    crate::debugln!("[std::fs::read] Opening '{}'...", path);
    let mut file = File::open(path)?;
    let size = file.size();
    crate::debugln!("[std::fs::read] File size: {} bytes", size);
    let mut bytes = alloc::vec![0u8; size];
    let mut total_read = 0;
    while total_read < size {
        crate::debugln!("[std::fs::read] Reading at offset {} (remaining {})...", total_read, size - total_read);
        let n = file.read(&mut bytes[total_read..])?;
        if n == 0 {
            crate::debugln!("[std::fs::read] EOF reached early at {}", total_read);
            break;
        }
        total_read += n;
    }
    bytes.truncate(total_read);
    crate::debugln!("[std::fs::read] Finished reading {} bytes", total_read);
    Ok(bytes)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Unknown = 0,
    File = 1,
    Directory = 2,
    Device = 3,
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub file_type: FileType,
}

pub fn read_dir(path: &str) -> Result<Vec<DirEntry>> {
    crate::debugln!("CALLING read_dir WITH ARGS: path={}", path);

    let mut open_buf = [0u8; 8];
    host::open_at(3, 0, path.as_ptr(), path.len(), 0, 0, open_buf.as_mut_ptr());
    if open_buf[0] != 0 {
        return Err(Error::from_raw_os_error(2));
    }
    let dir_handle = unsafe { core::ptr::read_unaligned(open_buf.as_ptr().add(4) as *const i32) };

    let mut result_buf = [0u8; 8];
    host::read_directory(dir_handle, result_buf.as_mut_ptr());
    if result_buf[0] != 0 {
        host::descriptor_drop(dir_handle);
        return Err(Error::from_raw_os_error(5));
    }
    let stream_handle = unsafe { core::ptr::read_unaligned(result_buf.as_ptr().add(4) as *const i32) };

    let mut entries = Vec::new();
    loop {
        let mut entry_buf = [0u8; 32];
        host::read_directory_entry(stream_handle, entry_buf.as_mut_ptr());
        if entry_buf[0] != 0 { break; }
        let has_value = unsafe { core::ptr::read_unaligned(entry_buf.as_ptr().add(4) as *const u32) };
        if has_value == 0 { break; }

        let type_byte = entry_buf[8];
        // On wasm32: name_ptr is 4 bytes at offset 12, name_len at offset 16
        let name_ptr = unsafe { core::ptr::read_unaligned(entry_buf.as_ptr().add(12) as *const u32) } as *const u8;
        let name_len = unsafe { core::ptr::read_unaligned(entry_buf.as_ptr().add(16) as *const u32) } as usize;

        let name = unsafe {
            let slice = core::slice::from_raw_parts(name_ptr, name_len);
            String::from(String::from_utf8_lossy(slice).trim_end_matches(char::from(0)))
        };

        let file_type = match type_byte {
            6 => FileType::File,
            3 => FileType::Directory,
            2 => FileType::Device,
            _ => FileType::Unknown,
        };

        crate::debugln!("[std::fs::read_dir] Entry: '{}' type={}", name, type_byte);
        entries.push(DirEntry { name, file_type });
    }

    host::drop_directory_entry_stream(stream_handle);
    host::descriptor_drop(dir_handle);

    crate::debugln!("TCP RESULT: read_dir RESULT: {} entries", entries.len());

    return Ok(entries);
}
