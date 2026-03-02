use crate::io::{Error, Read, Result, Seek, SeekFrom, Write};
use alloc::string::String;
use alloc::vec::Vec;

pub mod host;
pub mod async_file;
#[cfg(feature = "userland")]
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
    pub nlink: u32,
    pub size: u64,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
    pub _reserved: [u64; 1],
}

#[derive(Debug)]
pub struct File {
    fd: usize,
}

impl File {
    pub fn open(path: &str) -> Result<Self> {
        let mut result = [0u8; 8];
        open_at(3, 0, path.as_ptr(), path.len(), 0, 0, result.as_mut_ptr());

        if result[0] != 0 {
            Err(Error::from_raw_os_error(2)) // ENOENT
        } else {
            let fd = unsafe { core::ptr::read_unaligned(result.as_ptr().add(4) as *const i32) };
            Ok(File { fd: fd as usize })
        }
    }

    pub fn create(path: &str) -> Result<Self> {
        let mut result = [0u8; 8];
        open_at(3, 0, path.as_ptr(), path.len(), 1, 0, result.as_mut_ptr());
        if result[0] != 0 {
            Err(Error::from_raw_os_error(1))
        } else {
            let fd = unsafe { core::ptr::read_unaligned(result.as_ptr().add(4) as *const i32) };
            Ok(File { fd: fd as usize })
        }
    }

    pub fn size(&self) -> usize {
        self.stat().map(|s| s.size as usize).unwrap_or(0)
    }

    pub fn stat(&self) -> Result<Stat> {
        let mut result = [0u8; 128];
        stat(self.fd as i32, result.as_mut_ptr());
        if result[0] != 0 {
            Err(Error::from_raw_os_error(5))
        } else {
            let s = unsafe { core::ptr::read_unaligned(result.as_ptr().add(8) as *const Stat) };
            Ok(s)
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
        #[repr(C)]
        struct ReadResult {
            tag: u32,
            ptr: *mut u8,
            len: usize,
        }
        let mut result = [0u8; 24];

        crate::io::host::input_stream_read(self.fd as i32, buffer.len() as u64, result.as_mut_ptr());

        let r = unsafe { &*(result.as_ptr() as *const ReadResult) };

        if r.tag == 0 {
            let copy_len = core::cmp::min(buffer.len(), r.len);
            if copy_len > 0 {
                unsafe {
                    core::ptr::copy_nonoverlapping(r.ptr, buffer.as_mut_ptr(), copy_len);
                }
            }
            if !r.ptr.is_null() {
                crate::memory::free(r.ptr as usize, buffer.len());
            }
            Ok(copy_len)
        } else {
            Err(Error::from_raw_os_error(5))
        }
    }
}

impl Write for File {
    fn write(&mut self, buffer: &[u8]) -> Result<usize> {
        let mut result = [0u8; 8];
        crate::io::host::output_stream_blocking_write_and_flush(
            self.fd as i32,
            buffer.as_ptr(),
            buffer.len(),
            result.as_mut_ptr(),
        );
        if result[0] == 0 {
            Ok(buffer.len())
        } else {
            Err(Error::from_raw_os_error(5))
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

impl Drop for File {
    fn drop(&mut self) {
        descriptor_drop(self.fd as i32);
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
    let mut file = File::open(path)?;
    let size = file.size();
    let mut bytes = alloc::vec![0u8; size];
    let mut total_read = 0;
    while total_read < size {
        let n = file.read(&mut bytes[total_read..])?;
        if n == 0 {
            break;
        }
        total_read += n;
    }
    bytes.truncate(total_read);
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
    crate::debugln!("CALLING TCP FN read_dir WITH ARGS: path={}", path);
    let file = File::open(path)?;
    let mut entries = Vec::new();

    let mut result_buf = [0u8; 8];
    host::read_directory(file.fd as i32, result_buf.as_mut_ptr());
    
    if result_buf[0] != 0 {
        return Err(Error::from_raw_os_error(5));
    }
    
    let stream_handle = unsafe { core::ptr::read_unaligned(result_buf.as_ptr().add(4) as *const i32) };
    
    loop {
        let mut entry_buf = [0u8; 32];
        host::read_directory_entry(stream_handle, entry_buf.as_mut_ptr());
        
        if entry_buf[0] != 0 { break; } // Err or end
        let has_value = unsafe { core::ptr::read_unaligned(entry_buf.as_ptr().add(4) as *const u32) };
        if has_value == 0 { break; } // None
        
        let type_byte = entry_buf[8];
        let name_ptr = unsafe { core::ptr::read_unaligned(entry_buf.as_ptr().add(12) as *const *mut u8) };
        let name_len = unsafe { core::ptr::read_unaligned(entry_buf.as_ptr().add(16) as *const u32) } as usize;
        
        let name = unsafe {
            let slice = core::slice::from_raw_parts(name_ptr, name_len);
            let s = String::from_utf8_lossy(slice).into_owned();
            crate::memory::free(name_ptr as usize, name_len);
            s
        };
        
        let file_type = match type_byte {
            6 => FileType::File,
            3 => FileType::Directory,
            2 => FileType::Device,
            _ => FileType::Unknown,
        };
        
        entries.push(DirEntry { name, file_type });
    }
    
    host::drop_directory_entry_stream(stream_handle);
    
    crate::debugln!("TCP RESULT: read_dir RESULT: {} entries", entries.len());
    Ok(entries)
}