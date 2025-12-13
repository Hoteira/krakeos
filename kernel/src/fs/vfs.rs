use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::string::String;
use alloc::string::ToString;

// Using const { None } for array initialization of non-Copy types (Option<Box<...>>)
// This assumes a recent Rust compiler supporting inline const expressions or relaxed array initialization.
pub static mut FILESYSTEMS: [Option<Box<dyn FileSystem>>; 256] = [const { None }; 256];
pub static mut OPEN_FILES: [Option<FileHandle>; 256] = [const { None }; 256];

pub struct FileHandle {
    pub node: Box<dyn VfsNode>,
    pub offset: u64,
}

pub fn init() {
    unsafe {
        crate::debugln!("FILESYSTEMS at {:p}", core::ptr::addr_of!(FILESYSTEMS));
        // Zero out FILESYSTEMS because .bss is not zeroed by bootloader
        // If we don't do this, 'mount' will try to drop garbage "previous" values.
        core::ptr::write_bytes(core::ptr::addr_of_mut!(FILESYSTEMS), 0, 1);
        core::ptr::write_bytes(core::ptr::addr_of_mut!(OPEN_FILES), 0, 1);
    }
}

pub fn mount(disk_id: u8, fs: Box<dyn FileSystem>) {
    crate::debugln!("Mounting at index {}, fs box: {:p}", disk_id, fs);
    unsafe {
        FILESYSTEMS[disk_id as usize] = Some(fs);
    }
}

pub fn open_file(disk_id: u8, path_str: &str) -> Result<usize, String> {
    let node = open(disk_id, path_str)?;
    unsafe {
        for i in 3..256 {
            if OPEN_FILES[i].is_none() {
                OPEN_FILES[i] = Some(FileHandle { node, offset: 0 });
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
         if fd > 2 && fd < 256 {
             OPEN_FILES[fd] = None;
         }
    }
}

pub fn open(disk_id: u8, path_str: &str) -> Result<Box<dyn VfsNode>, String> {
    crate::debug::serial_print_str("vfs::open: start\r\n");
    // let disk_id = 0xE0;
    // let path_str = "user";
    crate::debugln!("vfs::open: disk_id check...");
    
    // Hardcode path parsing logic for now
    // let path_str_literal = "user";
    let components: Vec<String> = path_str.split('/').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();

    unsafe {
        crate::debugln!("Accessing FILESYSTEMS at index {}", disk_id);
        if let Some(fs) = &mut FILESYSTEMS[disk_id as usize] {
             crate::debugln!("FS box ptr: {:p}", fs);
             crate::debugln!("vfs::open: found fs, calling root()");
             let mut node = fs.root()?;
             for component in components.iter() {
                 crate::debugln!("vfs::open: finding component {}", component);
                 node = node.find(&component)?;
             }
             crate::debugln!("vfs::open: done");
             Ok(node)
        } else {
            Err(String::from("Disk ID not mounted"))
        }
    }
}

pub fn read(disk_id: u8, path_str: &str, offset: u64, size: u64, buffer: *mut u8) -> Result<usize, String> {
    let components: Vec<String> = path_str
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    unsafe {
        if let Some(fs) = &mut FILESYSTEMS[disk_id as usize] {
             let mut node = fs.root()?;
             for component in components {
                 node = node.find(&component)?;
             }
             let slice = core::slice::from_raw_parts_mut(buffer, size as usize);
             node.read(offset, slice)
        } else {
            Err(String::from("Disk ID not mounted"))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    File,
    Directory,
    Device,
    Unknown,
}

pub trait FileSystem: Send + Sync {
    fn root(&mut self) -> Result<Box<dyn VfsNode>, String>;
}

pub trait VfsNode {
    fn name(&self) -> String;
    fn size(&self) -> u64;
    fn kind(&self) -> FileType;
    
    fn read(&mut self, offset: u64, buffer: &mut [u8]) -> Result<usize, String>;
    fn write(&mut self, offset: u64, buffer: &[u8]) -> Result<usize, String>;
    
    fn children(&mut self) -> Result<Vec<Box<dyn VfsNode>>, String>;
    fn find(&mut self, name: &str) -> Result<Box<dyn VfsNode>, String>;
}