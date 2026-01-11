use alloc::boxed::Box;
use alloc::vec::IntoIter;
use core::ffi::{c_char, c_int, c_uchar, c_ulong, c_ushort};

#[repr(C)]
pub struct dirent {
    pub d_ino: c_ulong,
    pub d_off: c_ulong,
    pub d_reclen: c_ushort,
    pub d_type: c_uchar,
    pub d_name: [c_char; 256],
}

pub struct DIR {
    iter: IntoIter<std::fs::DirEntry>,
    current: dirent,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn opendir(name: *const c_char) -> *mut DIR {
    let s = core::ffi::CStr::from_ptr(name).to_string_lossy();
    if let Ok(entries) = std::fs::read_dir(&s) {
        let dir = Box::new(DIR {
            iter: entries.into_iter(),
            current: core::mem::zeroed(),
        });
        Box::into_raw(dir)
    } else {
        core::ptr::null_mut()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn readdir(dirp: *mut DIR) -> *mut dirent {
    if dirp.is_null() { return core::ptr::null_mut(); }
    let dir = &mut *dirp;

    if let Some(entry) = dir.iter.next() {
        for i in 0..256 { dir.current.d_name[i] = 0; }


        let name_bytes = entry.name.as_bytes();
        let len = core::cmp::min(name_bytes.len(), 255);
        for i in 0..len {
            dir.current.d_name[i] = name_bytes[i] as c_char;
        }
        dir.current.d_name[len] = 0;

        dir.current.d_type = match entry.file_type {
            std::fs::FileType::Directory => 4,
            std::fs::FileType::File => 8,
            _ => 0,
        };

        &mut dir.current as *mut dirent
    } else {
        core::ptr::null_mut()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn closedir(dirp: *mut DIR) -> c_int {
    if !dirp.is_null() {
        drop(Box::from_raw(dirp));
    }
    0
}
