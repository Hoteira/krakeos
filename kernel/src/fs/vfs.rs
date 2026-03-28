use alloc::boxed::Box;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;


pub static mut FILESYSTEMS: [Option<Box<dyn FileSystem>>; 256] = [const { None }; 256];
use alloc::collections::BTreeMap;
use crate::sync::Mutex;

pub struct GlobalFileTable {
    pub files: BTreeMap<usize, Box<FileHandle>>,
    pub refcounts: BTreeMap<usize, u16>,
    pub next_fd: usize,
}

pub static GLOBAL_FILES: Mutex<GlobalFileTable> = Mutex::new(GlobalFileTable {
    files: BTreeMap::new(),
    refcounts: BTreeMap::new(),
    next_fd: 3,
});

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
    let (uid, gid) = get_current_ids();
    if !check_access(uid, gid, &node.stat(), ACCESS_READ) {
        return Err(String::from("Permission denied (read)"));
    }
    
    let mut table = GLOBAL_FILES.lock();
    let fd = table.next_fd;
    table.next_fd += 1;
    
    table.files.insert(fd, Box::new(FileHandle::File { node, offset: 0 }));
    table.refcounts.insert(fd, 1);
    
    Ok(fd)
}

pub fn get_file(fd: usize) -> Option<&'static mut FileHandle> {
    let mut table = GLOBAL_FILES.lock();
    if let Some(boxed_handle) = table.files.get_mut(&fd) {
        unsafe {
            Some(&mut *(boxed_handle.as_mut() as *mut FileHandle))
        }
    } else {
        None
    }
}

pub fn close_file(fd: usize) {
    let mut table = GLOBAL_FILES.lock();
    if let Some(count) = table.refcounts.get_mut(&fd) {
        if *count > 0 {
            *count -= 1;
            if *count == 0 {
                if let Some(boxed_handle) = table.files.remove(&fd) {
                    if let FileHandle::Pipe { pipe } = *boxed_handle {
                        pipe.close();
                    }
                }
                table.refcounts.remove(&fd);
            }
        }
    }
}

pub fn increment_ref(fd: usize) {
    let mut table = GLOBAL_FILES.lock();
    if table.files.contains_key(&fd) {
        if let Some(count) = table.refcounts.get_mut(&fd) {
            *count += 1;
        }
    }
}

fn get_current_ids() -> (u16, u16) {
    let tm = crate::task::TASK_MANAGER.int_lock();
    if let Some(idx) = tm.current_task_idx() {
        if let Some(thread) = tm.tasks.get(&idx) {
            if let Some(proc) = &thread.process {
                return (proc.uid, proc.gid);
            }
        }
    }
    (0, 0) // Root/Kernel
}

pub fn open(disk_id: u8, path_str: &str) -> Result<Box<dyn VfsNode>, String> {
    let (actual_disk, actual_path) = if path_str.starts_with('/') {
        (0xE0, path_str)
    } else {
        (disk_id, path_str)
    };

    let components: Vec<String> = actual_path.split('/').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
    let (uid, gid) = get_current_ids();

    unsafe {
        if let Some(fs) = &mut FILESYSTEMS[actual_disk as usize] {
            let mut node = fs.root()?;
            for component in components.iter() {
                // Check traversal permission (X bit) on directories
                if node.kind() == FileType::Directory {
                    if !check_access(uid, gid, &node.stat(), ACCESS_EXEC) {
                        return Err(String::from("Permission denied"));
                    }
                }
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
    Symlink,
    Unknown,
}

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

pub const ACCESS_READ: u32 = 4;
pub const ACCESS_WRITE: u32 = 2;
pub const ACCESS_EXEC: u32 = 1;

pub fn check_access(proc_uid: u16, proc_gid: u16, stat: &Stat, requested: u32) -> bool {
    if proc_uid == 0 { return true; } // Root

    let mode = stat.mode;
    let actual = if proc_uid as u32 == stat.uid {
        (mode >> 6) & 7
    } else if proc_gid as u32 == stat.gid {
        (mode >> 3) & 7
    } else {
        mode & 7
    };

    (actual & requested) == requested
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
            uid: 0,
            gid: 0,
            nlink: 1,
            size: self.size(),
            atime: 0,
            mtime: 0,
            ctime: 0,
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

    fn mmap(&mut self, _offset: u64, _len: usize) -> Result<u64, String> {
        Err(String::from("Not supported"))
    }

    fn link(&mut self, _name: &str, _src: &mut dyn VfsNode) -> Result<(), String> {
        Err(String::from("Not supported"))
    }

    fn symlink(&mut self, _name: &str, _target_path: &str) -> Result<(), String> {
        Err(String::from("Not supported"))
    }

    fn set_times(&mut self, _atime: u64, _mtime: u64) -> Result<(), String> {
        Ok(())
    }

    fn readlink(&mut self) -> Result<String, String> {
        Err(String::from("Not a symlink"))
    }
}
