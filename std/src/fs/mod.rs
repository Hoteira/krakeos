use crate::os::syscall;
use alloc::string::String;
use alloc::vec::Vec;

pub struct File {
    fd: usize,
}

impl File {
    pub fn open(path: &str) -> Result<Self, String> {
        let res = unsafe {
            syscall(61, path.as_ptr() as u64, path.len() as u64, 0)
        };
        
        if res == u64::MAX {
            Err(String::from("Failed to open file"))
        } else {
            Ok(File { fd: res as usize })
        }
    }

    pub fn create(path: &str) -> Result<Self, String> {
        let res = unsafe {
            syscall(71, path.as_ptr() as u64, path.len() as u64, 0)
        };
        if res == 0 {
            File::open(path)
        } else {
            Err(String::from("Failed to create file"))
        }
    }

    pub fn read(&mut self, buffer: &mut [u8]) -> Result<usize, String> {
        let res = unsafe {
            syscall(62, self.fd as u64, buffer.as_mut_ptr() as u64, buffer.len() as u64)
        };
        
        if res == u64::MAX {
            Err(String::from("Read error"))
        } else {
            Ok(res as usize)
        }
    }

    pub fn write(&mut self, buffer: &[u8]) -> Result<usize, String> {
        let res = unsafe {
            syscall(63, self.fd as u64, buffer.as_ptr() as u64, buffer.len() as u64)
        };
        
        if res == u64::MAX {
            Err(String::from("Write error"))
        } else {
            Ok(res as usize)
        }
    }

    pub fn size(&self) -> usize {
        unsafe {
            let res = syscall(65, self.fd as u64, 0, 0);
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
}

impl Drop for File {
    fn drop(&mut self) {
        crate::os::file_close(self.fd);
    }
}

pub fn create_dir(path: &str) -> Result<(), String> {
    let res = unsafe {
        syscall(72, path.as_ptr() as u64, path.len() as u64, 0)
    };
    if res == 0 { Ok(()) } else { Err(String::from("Failed to create directory")) }
}

pub fn remove_file(path: &str) -> Result<(), String> {
    let res = unsafe {
        syscall(73, path.as_ptr() as u64, path.len() as u64, 0)
    };
    if res == 0 { Ok(()) } else { Err(String::from("Failed to remove file")) }
}

pub fn remove_dir(path: &str) -> Result<(), String> {
    remove_file(path) 
}

pub fn rename(from: &str, to: &str) -> Result<(), String> {
    let res = unsafe {
        crate::os::syscall4(74, from.as_ptr() as u64, from.len() as u64, to.as_ptr() as u64, to.len() as u64)
    };
    if res == 0 { Ok(()) } else { Err(String::from("Failed to rename")) }
}

pub fn mount(disk_id: u8, fs_type: &str) -> Result<(), String> {
    let res = unsafe {
        syscall(63, disk_id as u64, fs_type.as_ptr() as u64, fs_type.len() as u64)
    };
    
    if res == 0 {
        Ok(())
    } else {
        Err(String::from("Mount failed"))
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

pub fn read_dir(path: &str) -> Result<Vec<DirEntry>, String> {
    let file = File::open(path)?; 
    
    let mut entries = Vec::new();
    let mut buffer = [0u8; 1024]; 
    
    loop {
        
        let res = unsafe {
            syscall(64, file.fd as u64, buffer.as_mut_ptr() as u64, buffer.len() as u64)
        };
        
        if res == u64::MAX {
            return Err(String::from("read_dir failed"));
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
            
            let name_bytes = &buffer[offset + 2 .. offset + 2 + name_len];
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
