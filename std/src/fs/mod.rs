use crate::io::{Error, Read, Result, Seek, SeekFrom, Write};
use rust_alloc::string::String;
use rust_alloc::vec::Vec;

pub mod async_file;
pub use async_file::AsyncFile;

// --- Filesystem method_export! bindings (from wasi/filesystem.rs) ---

method_export!("wasi:filesystem/types@0.2.0", "[method]descriptor.open-at",
    pub unsafe fn open_at(
        dir_handle: i32,
        _flags: u32,
        path_ptr: *const u8,
        path_len: usize,
        oflags: u32,
        _flags_val: u32,
        result_ptr: *mut u8,
    ) {
        let syscall_num = if (oflags & 0x1) != 0 { 85 } else { 2 };
        let res = crate::sys::syscall(syscall_num, path_ptr as u64, path_len as u64, 0);
        if res == u64::MAX {
            core::ptr::write_unaligned(result_ptr as *mut u32, 1); // Err
        } else {
            core::ptr::write_unaligned(result_ptr as *mut u32, 0); // Ok
            core::ptr::write_unaligned(result_ptr.add(4) as *mut i32, res as i32);
        }
    }
);

method_export!("wasi:filesystem/types@0.2.0", "[method]descriptor.stat",
    pub unsafe fn stat(handle: i32, result_ptr: *mut u8) {
        let mut s = core::mem::zeroed::<Stat>();
        let res = crate::sys::syscall(5, handle as u64, 0, &mut s as *mut _ as u64);
        if res == u64::MAX {
            *result_ptr = 1; // Err
        } else {
            *result_ptr = 0; // Ok
            core::ptr::copy_nonoverlapping(
                &s as *const _ as *const u8,
                result_ptr.add(8),
                core::mem::size_of::<Stat>(),
            );
        }
    }
);

method_export!("wasi:filesystem/types@0.2.0", "[method]descriptor.set-size",
    pub unsafe fn set_size(handle: i32, size: u64, result_ptr: *mut u8) {
        let res = crate::sys::syscall(77, handle as u64, size, 0);
        if res == u64::MAX {
            *result_ptr = 1;
        } else {
            *result_ptr = 0;
        }
    }
);

method_export!("wasi:filesystem/types@0.2.0", "[method]descriptor.seek",
    pub unsafe fn seek(handle: i32, offset: u64, whence: i32, result_ptr: *mut u8) {
        let res = crate::sys::syscall(8, handle as u64, offset, whence as u64);
        if res == u64::MAX {
            *result_ptr = 1;
        } else {
            *result_ptr = 0;
            core::ptr::write_unaligned(result_ptr.add(8) as *mut u64, res);
        }
    }
);

method_export!("wasi:filesystem/types@0.2.0", "[resource-drop]descriptor",
    pub unsafe fn descriptor_drop(handle: i32) {
        crate::sys::syscall(3, handle as u64, 0, 0);
    }
);

method_export!("wasi:filesystem/types@0.2.0", "[method]descriptor.create-directory-at",
    pub unsafe fn create_directory_at(
        handle: i32,
        path_ptr: *const u8,
        path_len: usize,
        result_ptr: *mut u8,
    ) {
        let _ = handle;
        let res = crate::sys::syscall(83, path_ptr as u64, path_len as u64, 0);
        if res == u64::MAX {
            *result_ptr = 1;
        } else {
            *result_ptr = 0;
        }
    }
);

method_export!("wasi:filesystem/types@0.2.0", "[method]descriptor.unlink-file-at",
    pub unsafe fn unlink_file_at(handle: i32, path_ptr: *const u8, path_len: usize, result_ptr: *mut u8) {
        let _ = handle;
        let res = crate::sys::syscall(87, path_ptr as u64, path_len as u64, 0);
        if res == u64::MAX {
            *result_ptr = 1;
        } else {
            *result_ptr = 0;
        }
    }
);

method_export!("wasi:filesystem/types@0.2.0", "[method]descriptor.remove-directory-at",
    pub unsafe fn remove_directory_at(
        handle: i32,
        path_ptr: *const u8,
        path_len: usize,
        result_ptr: *mut u8,
    ) {
        let _ = handle;
        let res = crate::sys::syscall(84, path_ptr as u64, path_len as u64, 0);
        if res == u64::MAX {
            *result_ptr = 1;
        } else {
            *result_ptr = 0;
        }
    }
);

method_export!("wasi:filesystem/types@0.2.0", "[method]descriptor.rename-at",
    pub unsafe fn rename_at(
        handle: i32,
        old_path_ptr: *const u8,
        old_path_len: usize,
        new_handle: i32,
        new_path_ptr: *const u8,
        new_path_len: usize,
        result_ptr: *mut u8,
    ) {
        let _ = (handle, new_handle);
        let res = crate::sys::syscall4(
            82,
            old_path_ptr as u64,
            old_path_len as u64,
            new_path_ptr as u64,
            new_path_len as u64,
        );
        if res == u64::MAX {
            *result_ptr = 1;
        } else {
            *result_ptr = 0;
        }
    }
);

// readdir support - uses WASI preview1 fd_readdir on wasm32, KrakeOS syscall 78 on native
#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi_snapshot_preview1")]
unsafe extern "C" {
    pub fn fd_readdir(
        fd: i32,
        buf_ptr: *mut u8,
        buf_len: u32,
        cookie: u64,
        bufused_ptr: *mut u32,
    ) -> i32;
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn readdir(fd: i32, buf: &mut [u8]) -> u64 {
    crate::sys::syscall(78, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64)
}

// --- Arch-gated helper functions ---

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn create_dir_raw(path: &str) -> i32 {
    crate::sys::syscall(83, path.as_ptr() as u64, path.len() as u64, 0) as i32
}

#[cfg(target_arch = "wasm32")]
pub unsafe fn create_dir_raw(path: &str) -> i32 {
    let mut result_buf = [0u8; 4];
    create_directory_at(3, path.as_ptr(), path.len(), result_buf.as_mut_ptr());
    if result_buf[0] == 0 { 0 } else { -1 }
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn remove_file_raw(path: &str) -> i32 {
    crate::sys::syscall(87, path.as_ptr() as u64, path.len() as u64, 0) as i32
}

#[cfg(target_arch = "wasm32")]
pub unsafe fn remove_file_raw(path: &str) -> i32 {
    let mut result_buf = [0u8; 4];
    unlink_file_at(3, path.as_ptr(), path.len(), result_buf.as_mut_ptr());
    if result_buf[0] == 0 { 0 } else { -1 }
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn remove_dir_raw(path: &str) -> i32 {
    crate::sys::syscall(84, path.as_ptr() as u64, path.len() as u64, 0) as i32
}

#[cfg(target_arch = "wasm32")]
pub unsafe fn remove_dir_raw(path: &str) -> i32 {
    let mut result_buf = [0u8; 4];
    remove_directory_at(3, path.as_ptr(), path.len(), result_buf.as_mut_ptr());
    if result_buf[0] == 0 { 0 } else { -1 }
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn rename_raw(from: &str, to: &str) -> i32 {
    crate::sys::syscall4(
        82,
        from.as_ptr() as u64,
        from.len() as u64,
        to.as_ptr() as u64,
        to.len() as u64,
    ) as i32
}

#[cfg(target_arch = "wasm32")]
pub unsafe fn rename_raw(from: &str, to: &str) -> i32 {
    let mut result_buf = [0u8; 4];
    rename_at(
        3,
        from.as_ptr(),
        from.len(),
        3,
        to.as_ptr(),
        to.len(),
        result_buf.as_mut_ptr(),
    );
    if result_buf[0] == 0 { 0 } else { -1 }
}

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
        unsafe {
            open_at(3, 0, path.as_ptr(), path.len(), 0, 0, result.as_mut_ptr());
        }

        if result[0] != 0 {
            Err(Error::from_raw_os_error(2)) // ENOENT
        } else {
            let fd = unsafe { core::ptr::read_unaligned(result.as_ptr().add(4) as *const i32) };
            Ok(File { fd: fd as usize })
        }
    }

    pub fn create(path: &str) -> Result<Self> {
        let mut result = [0u8; 8];
        unsafe {
            open_at(3, 0, path.as_ptr(), path.len(), 1, 0, result.as_mut_ptr());
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
        let mut result = [0u8; 128];
        unsafe {
            stat(self.fd as i32, result.as_mut_ptr());
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
            set_size(self.fd as i32, size, result.as_mut_ptr());
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
        let mut result = [0u8; 24];

        unsafe {
            crate::io::streams::input_stream_read(self.fd as i32, buffer.len() as u64, result.as_mut_ptr());
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
        unsafe {
            crate::io::streams::output_stream_blocking_write_and_flush(
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
            seek(self.fd as i32, offset as u64, whence, result.as_mut_ptr());
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
            descriptor_drop(self.fd as i32);
        }
    }
}

pub fn create_dir(path: &str) -> Result<()> {
    let res = unsafe { create_dir_raw(path) };
    if res == 0 {
        Ok(())
    } else {
        Err(Error::from_raw_os_error(1))
    }
}

pub fn remove_file(path: &str) -> Result<()> {
    let res = unsafe { remove_file_raw(path) };
    if res == 0 {
        Ok(())
    } else {
        Err(Error::from_raw_os_error(1))
    }
}

pub fn remove_dir(path: &str) -> Result<()> {
    let res = unsafe { remove_dir_raw(path) };
    if res == 0 {
        Ok(())
    } else {
        Err(Error::from_raw_os_error(1))
    }
}

pub fn rename(from: &str, to: &str) -> Result<()> {
    let res = unsafe { rename_raw(from, to) };
    if res == 0 {
        Ok(())
    } else {
        Err(Error::from_raw_os_error(1))
    }
}

pub fn mount(disk_id: u8, fs_type: &str) -> Result<()> {
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

    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut buffer = [0u8; 1024];
        loop {
            let res = unsafe { readdir(file.fd as i32, &mut buffer) };
            if res == u64::MAX {
                return Err(Error::from_raw_os_error(5));
            }
            let bytes_read = res as usize;
            if bytes_read == 0 {
                break;
            }

            let mut offset = 0;
            while offset < bytes_read {
                if offset + 2 > bytes_read {
                    break;
                }
                let type_byte = buffer[offset];
                let name_len = buffer[offset + 1] as usize;
                if offset + 2 + name_len > bytes_read {
                    break;
                }
                let name = String::from_utf8_lossy(&buffer[offset + 2..offset + 2 + name_len])
                    .into_owned();
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
    }

    #[cfg(target_arch = "wasm32")]
    {
        let mut buffer = [0u8; 4096];
        let mut cookie: u64 = 0;
        loop {
            let mut bufused: u32 = 0;
            let errno = unsafe {
                fd_readdir(
                    file.fd as i32,
                    buffer.as_mut_ptr(),
                    buffer.len() as u32,
                    cookie,
                    &mut bufused,
                )
            };
            if errno != 0 {
                return Err(Error::from_raw_os_error(errno as i32));
            }
            let used = bufused as usize;
            if used == 0 {
                break;
            }

            let mut offset = 0;
            while offset + 24 <= used {
                let d_next =
                    u64::from_le_bytes(buffer[offset..offset + 8].try_into().unwrap_or([0; 8]));
                let d_namlen = u32::from_le_bytes(
                    buffer[offset + 16..offset + 20]
                        .try_into()
                        .unwrap_or([0; 4]),
                ) as usize;
                let d_type = buffer[offset + 20];
                if offset + 24 + d_namlen > used {
                    break;
                }
                let name = String::from_utf8_lossy(&buffer[offset + 24..offset + 24 + d_namlen])
                    .into_owned();
                let file_type = match d_type {
                    4 => FileType::File,
                    3 => FileType::Directory,
                    2 => FileType::Device,
                    _ => FileType::Unknown,
                };
                entries.push(DirEntry { name, file_type });
                offset += 24 + d_namlen;
                cookie = d_next;
            }
            if used < buffer.len() {
                break;
            }
        }
    }

    Ok(entries)
}
