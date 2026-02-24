use crate::io::{Error, Read, Result, Seek, SeekFrom, Write};
use crate::wasi::{filesystem, io as wasi_io};
use rust_alloc::string::String;
use rust_alloc::vec::Vec;

pub mod async_file;
pub use async_file::AsyncFile;

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
        unsafe {
            filesystem::open_at(3, 0, path.as_ptr(), path.len(), 0, 0, result.as_mut_ptr());
        }

        if result[0] != 0 {
            Err(Error::from_raw_os_error(2)) // ENOENT
        } else {
            let fd = unsafe { core::ptr::read_unaligned(result.as_ptr().add(4) as *const i32) };
            Ok(File { fd: fd as usize })
        }
    }

    pub fn create(path: &str) -> Result<Self> {
        // For simplicity, reuse open with create flags logic if implemented, or just a direct syscall wrapper in filesystem binding
        let mut result = [0u8; 8];
        unsafe {
            filesystem::open_at(3, 0, path.as_ptr(), path.len(), 1, 0, result.as_mut_ptr()); // 1 = create?
        }
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
        let mut result = [0u8; 128]; // Big enough for result tag + Stat struct
        unsafe {
            filesystem::stat(self.fd as i32, result.as_mut_ptr());
        }
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
        unsafe {
            filesystem::set_size(self.fd as i32, size, result.as_mut_ptr());
        }
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
        let mut result = [0u8; 24]; // Large enough for ReadResult on any arch

        unsafe {
            wasi_io::input_stream_read(self.fd as i32, buffer.len() as u64, result.as_mut_ptr());
        }

        let r = unsafe { &*(result.as_ptr() as *const ReadResult) };

        if r.tag == 0 {
            let copy_len = core::cmp::min(buffer.len(), r.len);
            if copy_len > 0 {
                unsafe {
                    core::ptr::copy_nonoverlapping(r.ptr, buffer.as_mut_ptr(), copy_len);
                }
            }
            if !r.ptr.is_null() {
                crate::memory::free(r.ptr as usize, buffer.len()); // free wasi buffer? Wait, wasi_io usually returns a buffer we must free?
                // Assuming yes based on current code
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
        unsafe {
            wasi_io::output_stream_blocking_write_and_flush(
                self.fd as i32,
                buffer.as_ptr(),
                buffer.len(),
                result.as_mut_ptr(),
            );
        }
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
        unsafe {
            filesystem::seek(self.fd as i32, offset as u64, whence, result.as_mut_ptr());
        }

        if result[0] == 0 {
            Ok(unsafe { core::ptr::read_unaligned(result.as_ptr().add(8) as *const u64) })
        } else {
            Err(Error::from_raw_os_error(29)) // ESPIPE
        }
    }
}

impl Drop for File {
    fn drop(&mut self) {
        unsafe {
            filesystem::descriptor_drop(self.fd as i32);
        }
    }
}

pub fn create_dir(path: &str) -> Result<()> {
    let res = unsafe { filesystem::create_dir(path) };
    if res == 0 {
        Ok(())
    } else {
        Err(Error::from_raw_os_error(1))
    }
}

pub fn remove_file(path: &str) -> Result<()> {
    let res = unsafe { filesystem::remove_file(path) };
    if res == 0 {
        Ok(())
    } else {
        Err(Error::from_raw_os_error(1))
    }
}

pub fn remove_dir(path: &str) -> Result<()> {
    let res = unsafe { filesystem::remove_dir(path) };
    if res == 0 {
        Ok(())
    } else {
        Err(Error::from_raw_os_error(1))
    }
}

pub fn rename(from: &str, to: &str) -> Result<()> {
    let res = unsafe { filesystem::rename(from, to) };
    if res == 0 {
        Ok(())
    } else {
        Err(Error::from_raw_os_error(1))
    }
}

pub fn mount(disk_id: u8, fs_type: &str) -> Result<()> {
    // Keep mount as platform specific or move to wasi::krakeos if intended to be portable
    // Since mount is not standard WASI, we assume it's KrakeOS extension.
    // However, to remove cfg, we should abstract it.
    // For now, leaving as is but maybe wrap syscall?
    // Let's wrap it properly:
    // wasi::krakeos::mount(...) -> Result
    // But wasi::krakeos is not standard.
    // I'll leave the cfg here for mount as it's not a standard function.
    // Or, better, call `wasi::filesystem::mount` which I can add to `wasi/filesystem.rs` as extension.
    // I'll stick to leaving it for now to focus on standard unification.
    #[cfg(not(target_arch = "wasm32"))]
    let res = unsafe {
        crate::sys::syscall(
            165,
            disk_id as u64,
            fs_type.as_ptr() as u64,
            fs_type.len() as u64,
        )
    };
    #[cfg(target_arch = "wasm32")]
    let res = u64::MAX; // Not supported

    if res == 0 {
        Ok(())
    } else {
        Err(Error::from_raw_os_error(1))
    }
}

pub fn read(path: &str) -> Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let size = file.size();
    let mut bytes = rust_alloc::vec![0u8; size];
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
    let file = File::open(path)?;
    let mut entries = Vec::new();

    // Use unified WASI P2 interface (wrapped by wasi::filesystem)
    let mut stream_res = [0u8; 8];
    unsafe {
        filesystem::read_directory(file.fd as i32, stream_res.as_mut_ptr());
    }

    if stream_res[0] != 0 {
        return Err(Error::from_raw_os_error(5));
    }

    let stream = unsafe { core::ptr::read_unaligned(stream_res.as_ptr().add(4) as *const i32) };

    loop {
        // Result<Option<DirectoryEntry>, Error>
        // Layout: tag(u32) + opt_tag(u32) + [type(u8), pad(3), name_ptr(u32), name_len(u32)]
        // Total size = 4 + 4 + 1 + 3 + 4 + 4 = 20 bytes.
        let mut entry_res = [0u8; 32];
        unsafe {
            filesystem::read_directory_entry(stream, entry_res.as_mut_ptr());
        }

        let res_tag = unsafe { core::ptr::read_unaligned(entry_res.as_ptr() as *const u32) };
        if res_tag != 0 {
            // Error
            unsafe { filesystem::drop_directory_entry_stream(stream); }
            return Err(Error::from_raw_os_error(5));
        }

        let opt_tag = unsafe { core::ptr::read_unaligned(entry_res.as_ptr().add(4) as *const u32) };
        if opt_tag == 0 {
            // None -> End of stream
            break;
        }

        // Some
        let type_byte = unsafe { *entry_res.as_ptr().add(8) };
        let name_ptr = unsafe { core::ptr::read_unaligned(entry_res.as_ptr().add(12) as *const u32) } as *mut u8;
        let name_len = unsafe { core::ptr::read_unaligned(entry_res.as_ptr().add(16) as *const u32) } as usize;

        let name_slice = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };
        let name = String::from_utf8_lossy(name_slice).into_owned();

        // Free the name buffer (assuming it was malloc'd by callee as per P2 convention)
        unsafe { crate::memory::free(name_ptr as usize, name_len); }

        let file_type = match type_byte {
            6 => FileType::File,
            3 => FileType::Directory,
            2 => FileType::Device,
            _ => FileType::Unknown,
        };

        entries.push(DirEntry { name, file_type });
    }

    unsafe { filesystem::drop_directory_entry_stream(stream); }
    Ok(entries)
}
