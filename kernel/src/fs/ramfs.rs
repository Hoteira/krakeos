use alloc::vec::Vec;
use alloc::string::String;

pub const MAX_RAM_FILES: usize = 16;
pub const RAMFS_DESC_OFFSET: usize = 1024; // Indices >= 1024 are RAM files

pub struct RamFile {
    pub name: String,
    pub data: Vec<u8>,
    pub waiting_thread: Option<usize>, // Thread ID waiting for updates
}

pub struct RamFS {
    pub files: [Option<RamFile>; MAX_RAM_FILES],
}

pub static mut RAM_FS: RamFS = RamFS {
    files: [
        None, None, None, None, None, None, None, None,
        None, None, None, None, None, None, None, None,
    ],
};

pub fn is_ram_file(name: &str) -> bool {
    name.ends_with(".ram")
}

pub fn get_ram_basename(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

pub fn find_file(name: &str) -> Option<usize> {
    let basename = get_ram_basename(name);
    unsafe {
        let files = &*core::ptr::addr_of!(RAM_FS.files);
        for (i, file_opt) in files.iter().enumerate() {
            if let Some(file) = file_opt {
                if get_ram_basename(&file.name) == basename {
                    return Some(RAMFS_DESC_OFFSET + i);
                }
            }
        }
    }
    None
}

pub fn create_file(name: &str) -> Option<usize> {
    if let Some(idx) = find_file(name) {
        return Some(idx);
    }

    let basename = get_ram_basename(name);
    let mut abs_name = String::from("/");
    abs_name.push_str(basename);

    unsafe {
        let files = &mut *core::ptr::addr_of_mut!(RAM_FS.files);
        for (i, file_opt) in files.iter_mut().enumerate() {
            if file_opt.is_none() {
                *file_opt = Some(RamFile {
                    name: abs_name,
                    data: Vec::new(),
                    waiting_thread: None,
                });
                return Some(RAMFS_DESC_OFFSET + i);
            }
        }
    }
    None
}

pub fn get_file_size(desc_idx: usize) -> usize {
    let i = desc_idx - RAMFS_DESC_OFFSET;
    unsafe {
        if let Some(file) = &RAM_FS.files[i] {
            file.data.len()
        } else {
            0
        }
    }
}

pub fn truncate_file(desc_idx: usize, length: usize) {
    let i = desc_idx - RAMFS_DESC_OFFSET;
    unsafe {
        if let Some(file) = &mut RAM_FS.files[i] {
            file.data.truncate(length);
        }
    }
}

pub fn read_file(desc_idx: usize, offset: usize, buf: &mut [u8]) -> usize {
    let i = desc_idx - RAMFS_DESC_OFFSET;
    unsafe {
        if let Some(file) = &RAM_FS.files[i] {
            if offset >= file.data.len() {
                return 0;
            }
            let to_read = core::cmp::min(buf.len(), file.data.len() - offset);
            buf[..to_read].copy_from_slice(&file.data[offset..offset + to_read]);
            to_read
        } else {
            0
        }
    }
}

pub fn write_file(desc_idx: usize, offset: usize, buf: &[u8]) -> usize {
    let i = desc_idx - RAMFS_DESC_OFFSET;
    let mut thread_to_wake = None;
    
    let bytes_written = unsafe {
        if let Some(file) = &mut RAM_FS.files[i] {
            let end = offset + buf.len();
            if end > file.data.len() {
                file.data.resize(end, 0);
            }
            file.data[offset..end].copy_from_slice(buf);
            
            // Capture waiting thread
            thread_to_wake = file.waiting_thread.take();
            buf.len()
        } else {
            0
        }
    };
    
    if let Some(tid) = thread_to_wake {
        // Unblock the thread (mark it Ready)
        unsafe {
            if crate::sys::scheduler::SCHEDULER.threads[tid].state == crate::sys::scheduler::ThreadState::Waiting {
                crate::sys::scheduler::SCHEDULER.threads[tid].state = crate::sys::scheduler::ThreadState::Ready;
            }
        }
    }
    
    bytes_written
}

pub fn wait_for_event(desc_idx: usize, current_tid: usize) -> bool {
    let i = desc_idx - RAMFS_DESC_OFFSET;
    unsafe {
        if let Some(file) = &mut RAM_FS.files[i] {
            file.waiting_thread = Some(current_tid);
            true
        } else {
            false
        }
    }
}
