use crate::io::{Error, Read, Result, Seek, SeekFrom, Write};
use crate::os::syscall;
use rust_alloc::string::String;
use rust_alloc::vec::Vec;

pub struct File {
    fd: usize,
}

impl File {
    pub fn open(path: &str) -> Result<Self> {
        let res = unsafe {
            syscall(2, path.as_ptr() as u64, path.len() as u64, 0)
        };

        if res == u64::MAX {
            Err(Error::from_raw_os_error(2)) // ENOENT-ish
        } else {
            Ok(File { fd: res as usize })
        }
    }

    pub fn create(path: &str) -> Result<Self> {
        let res = unsafe {
            syscall(85, path.as_ptr() as u64, path.len() as u64, 0)
        };
        if res == 0 {
            File::open(path)
        } else {
            Err(Error::from_raw_os_error(1)) // EPERM-ish
        }
    }

    pub fn size(&self) -> usize {
        unsafe {
            let res = syscall(5, self.fd as u64, 0, 0);
            if res == u64::MAX {
                0
            } else {
                res as usize
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
        let res = crate::os::file_truncate(self.fd, size);
        if res == 0 {
            Ok(())
        } else {
            Err(Error::from_raw_os_error(5))
        }
    }
}

impl Read for File {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        let res = unsafe {
            syscall(0, self.fd as u64, buffer.as_mut_ptr() as u64, buffer.len() as u64)
        };

        if res == u64::MAX {
            Err(Error::from_raw_os_error(5)) // EIO
        } else {
            Ok(res as usize)
        }
    }
}

impl Write for File {
    fn write(&mut self, buffer: &[u8]) -> Result<usize> {
        let res = unsafe {
            syscall(1, self.fd as u64, buffer.as_ptr() as u64, buffer.len() as u64)
        };

        if res == u64::MAX {
            Err(Error::from_raw_os_error(5)) // EIO
        } else {
            Ok(res as usize)
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

        let res = unsafe {
            crate::os::syscall(8, self.fd as u64, offset as u64, whence as u64)
        };

        if res == u64::MAX {
            Err(Error::from_raw_os_error(29)) // ESPIPE or similar
        } else {
            Ok(res)
        }
    }
}

impl Drop for File {
    fn drop(&mut self) {
        crate::os::file_close(self.fd);
    }
}

pub fn create_dir(path: &str) -> Result<()> {
    let res = unsafe {
        syscall(83, path.as_ptr() as u64, path.len() as u64, 0)
    };
    if res == 0 { Ok(()) } else { Err(Error::from_raw_os_error(1)) }
}

pub fn remove_file(path: &str) -> Result<()> {
    let res = unsafe {
        syscall(87, path.as_ptr() as u64, path.len() as u64, 0)
    };
    if res == 0 { Ok(()) } else { Err(Error::from_raw_os_error(1)) }
}

pub fn remove_dir(path: &str) -> Result<()> {
    remove_file(path)
}

pub fn rename(from: &str, to: &str) -> Result<()> {
    let res = unsafe {
        crate::os::syscall4(82, from.as_ptr() as u64, from.len() as u64, to.as_ptr() as u64, to.len() as u64)
    };
    if res == 0 { Ok(()) } else { Err(Error::from_raw_os_error(1)) }
}

pub fn mount(disk_id: u8, fs_type: &str) -> Result<()> {
    let res = unsafe {
        syscall(165, disk_id as u64, fs_type.as_ptr() as u64, fs_type.len() as u64)
    };

    if res == 0 {
        Ok(())
    } else {
        Err(Error::from_raw_os_error(1))
    }
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
    let mut actual_path = String::from(path);
    if actual_path.starts_with('/') {
        actual_path = String::from("@0xE0") + &actual_path;
    } else if !actual_path.starts_with('@') {
        actual_path = String::from("@0xE0/") + &actual_path;
    }

    let mut file = File::open(&actual_path)?;

    let mut entries = Vec::new();
    let mut buffer = [0u8; 1024];

    loop {
        let res = unsafe {
            syscall(78, file.fd as u64, buffer.as_mut_ptr() as u64, buffer.len() as u64)
        };

        if res == u64::MAX {
            return Err(Error::from_raw_os_error(5));
        }

        let bytes_read = res as usize;
        if bytes_read == 0 {
            break;
        }

        let mut offset = 0;
        while offset < bytes_read {
            if offset + 2 > bytes_read { break; }

            let type_byte = buffer[offset];
            let name_len = buffer[offset + 1] as usize;

            if offset + 2 + name_len > bytes_read { break; }

            let name_bytes = &buffer[offset + 2..offset + 2 + name_len];
            let name = String::from_utf8_lossy(name_bytes).into_owned();

            let file_type = match type_byte {
                1 => FileType::File,
                2 => FileType::Directory,
                3 => FileType::Device,
                _ => FileType::Unknown,
            };

            entries.push(DirEntry { name, file_type });

            offset += 2 + name_len;
        }
    }

    Ok(entries)
}