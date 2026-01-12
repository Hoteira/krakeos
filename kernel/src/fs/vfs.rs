use alloc::boxed::Box;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;


pub static mut FILESYSTEMS: [Option<Box<dyn FileSystem>>; 256] = [const { None }; 256];
pub static mut OPEN_FILES: [Option<FileHandle>; 256] = [const { None }; 256];
pub static mut GLOBAL_FILE_REFCOUNT: [u16; 256] = [0; 256];

pub enum FileHandle {
    File { node: Box<dyn VfsNode>, offset: u64 },
    Pipe { pipe: crate::fs::pipe::Pipe },
}

pub fn init() {}

pub fn mount(disk_id: u8, fs: Box<dyn FileSystem>) {
    crate::debugln!("Mounting at index {}, fs box: {:p}", disk_id, fs);
    unsafe {
        FILESYSTEMS[disk_id as usize] = Some(fs);
    }
}

pub fn open_file(disk_id: u8, path_str: &str) -> Result<usize, String> {
    let mut actual_path = String::from(path_str);
    if !path_str.starts_with('@') && !path_str.starts_with('/') {}

    let node = open(disk_id, path_str)?;
    unsafe {
        for i in 3..256 {
            if OPEN_FILES[i].is_none() {
                OPEN_FILES[i] = Some(FileHandle::File { node, offset: 0 });
                GLOBAL_FILE_REFCOUNT[i] = 1;
                return Ok(i);
            }
        }
        Err(String::from("No free file descriptors"))
    }
}

pub fn get_file(fd: usize) -> Option<&'static mut FileHandle> {
    unsafe {
        if fd < 256 {
            OPEN_FILES[fd].as_mut()
        } else {
            None
        }
    }
}

pub fn close_file(fd: usize) {
    unsafe {
        if fd < 256 {
            if GLOBAL_FILE_REFCOUNT[fd] > 0 {
                GLOBAL_FILE_REFCOUNT[fd] -= 1;
                if GLOBAL_FILE_REFCOUNT[fd] == 0 {
                    if let Some(FileHandle::Pipe { pipe }) = &OPEN_FILES[fd] {
                        pipe.close();
                    }
                    OPEN_FILES[fd] = None;
                }
            }
        }
    }
}

pub fn increment_ref(fd: usize) {
    unsafe {
        if fd < 256 && OPEN_FILES[fd].is_some() {
            GLOBAL_FILE_REFCOUNT[fd] += 1;
        }
    }
}

pub fn open(disk_id: u8, path_str: &str) -> Result<Box<dyn VfsNode>, String> {
    let (actual_disk, actual_path) = if path_str.starts_with('@') {
        let mut parts = path_str.splitn(2, '/');
        let disk_part = parts.next().unwrap().trim_start_matches('@');
        let id = if disk_part.starts_with("0x") || disk_part.starts_with("0X") {
            u8::from_str_radix(&disk_part[2..], 16).unwrap_or(disk_id)
        } else {
            u8::from_str_radix(disk_part, 16).unwrap_or_else(|_| disk_part.parse::<u8>().unwrap_or(disk_id))
        };
        let p = parts.next().unwrap_or("");
        (id, p)
    } else {
        (disk_id, path_str)
    };

    let components: Vec<String> = actual_path.split('/').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();

    unsafe {
        if let Some(fs) = &mut FILESYSTEMS[actual_disk as usize] {
            let mut node = fs.root()?;
            for component in components.iter() {
                node = node.find(&component)?;
            }
            Ok(node)
        } else {
            Err(String::from("Disk ID not mounted"))
        }
    }
}

pub fn read(disk_id: u8, path_str: &str, offset: u64, size: u64, buffer: *mut u8) -> Result<usize, String> {
    let mut node = open(disk_id, path_str)?;
    let slice = unsafe { core::slice::from_raw_parts_mut(buffer, size as usize) };
    node.read(offset, slice)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    File,
    Directory,
    Device,
    Unknown,
}

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
    pub _reserved: [u64; 1], // Pad to 64 bytes
}

pub trait FileSystem: Send + Sync {
    fn root(&mut self) -> Result<Box<dyn VfsNode>, String>;
}


pub trait VfsNode: Send + Sync {
    fn name(&self) -> String;
    fn size(&self) -> u64;
    fn kind(&self) -> FileType;
    fn inode(&self) -> u64 { 0 }
    fn stat(&self) -> Stat {
        Stat {
            dev: 1,
            ino: self.inode(),
            mode: 0,
            nlink: 1,
            size: self.size(),
            atime: 0,
            mtime: 0,
            ctime: 0,
            _reserved: [0],
        }
    }
    fn read(&mut self, offset: u64, buffer: &mut [u8]) -> Result<usize, String>;
    fn write(&mut self, offset: u64, buffer: &[u8]) -> Result<usize, String>;
    fn children(&mut self) -> Result<Vec<Box<dyn VfsNode>>, String>;
    fn find(&mut self, name: &str) -> Result<Box<dyn VfsNode>, String>;

    fn read_dir(&mut self, _start_index: u64, _buffer: &mut [u8]) -> Result<(usize, usize), String> {
        Err(String::from("Not supported"))
    }


    fn create_file(&mut self, _name: &str) -> Result<Box<dyn VfsNode>, String> {
        Err(String::from("Not supported"))
    }


    fn create_dir(&mut self, _name: &str) -> Result<Box<dyn VfsNode>, String> {
        Err(String::from("Not supported"))
    }

    fn remove(&mut self, _name: &str) -> Result<(), String> {
        Err(String::from("Not supported"))
    }

    fn rename(&mut self, _old_name: &str, _new_name: &str) -> Result<(), String> {
        Err(String::from("Not supported"))
    }

    fn truncate(&mut self, _size: u64) -> Result<(), String> {
        Err(String::from("Not supported"))
    }
}
