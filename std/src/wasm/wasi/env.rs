use crate::rust_alloc::string::String;
use crate::rust_alloc::vec::Vec;

#[derive(Debug, Clone, Copy)]
pub struct FdStat {
    pub filetype: u8,
    pub rights_base: u64,
    pub rights_inheriting: u64,
    pub flags: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct FileStat {
    pub dev: u64,
    pub ino: u64,
    pub filetype: u8,
    pub nlink: u64,
    pub size: u64,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
}

pub trait WasiEnv {
    fn args_get(&self) -> Result<Vec<String>, i32>;
    fn environ_get(&self) -> Result<Vec<(String, String)>, i32>;
    fn clock_res_get(&self, id: u32) -> Result<u64, i32>;
    fn clock_time_get(&self, id: u32, precision: u64) -> Result<u64, i32>;
    fn fd_close(&mut self, fd: i32) -> Result<(), i32>;
    fn fd_fdstat_get(&self, fd: i32) -> Result<FdStat, i32>;
    fn fd_filestat_get(&self, fd: i32) -> Result<FileStat, i32>;
    fn fd_filestat_set_size(&mut self, fd: i32, size: u64) -> Result<(), i32>;
    fn fd_prestat_get(&self, fd: i32) -> Result<u32, i32>; // returns type? or path len? prestat is usually type + path_len
    fn fd_prestat_dir_name(&self, fd: i32) -> Result<String, i32>;
    fn fd_read(&mut self, fd: i32, iovs: &mut [(&mut [u8])]) -> Result<usize, i32>;
    fn fd_write(&mut self, fd: i32, iovs: &[&[u8]]) -> Result<usize, i32>;
    fn fd_seek(&mut self, fd: i32, offset: i64, whence: u8) -> Result<u64, i32>;
    fn fd_tell(&mut self, fd: i32) -> Result<u64, i32>;
    fn fd_sync(&mut self, fd: i32) -> Result<(), i32>;
    fn fd_datasync(&mut self, fd: i32) -> Result<(), i32>;
    fn fd_advise(&mut self, fd: i32, offset: u64, len: u64, advice: u8) -> Result<(), i32>;
    fn fd_fdstat_set_flags(&mut self, fd: i32, flags: u16) -> Result<(), i32>;
    fn fd_filestat_set_times(&mut self, fd: i32, atime: u64, mtime: u64, fst_flags: u16) -> Result<(), i32>;
    fn fd_pread(&mut self, fd: i32, iovs: &mut [(&mut [u8])], offset: u64) -> Result<usize, i32>;
    fn fd_pwrite(&mut self, fd: i32, iovs: &[&[u8]], offset: u64) -> Result<usize, i32>;
    fn path_open(&mut self, dirfd: i32, dirflags: u32, path: &str, oflags: u32, fs_rights_base: u64, fs_rights_inheriting: u64, fdflags: u16) -> Result<i32, i32>;
    fn path_create_directory(&mut self, dirfd: i32, path: &str) -> Result<(), i32>;
    fn path_remove_directory(&mut self, dirfd: i32, path: &str) -> Result<(), i32>;
    fn path_unlink_file(&mut self, dirfd: i32, path: &str) -> Result<(), i32>;
    fn path_rename(&mut self, old_fd: i32, old_path: &str, new_fd: i32, new_path: &str) -> Result<(), i32>;
    fn path_readlink(&mut self, dirfd: i32, path: &str, buf: &mut [u8]) -> Result<usize, i32>;
    fn path_link(&mut self, old_fd: i32, old_flags: u32, old_path: &str, new_fd: i32, new_path: &str) -> Result<(), i32>;
    fn path_symlink(&mut self, old_path: &str, fd: i32, new_path: &str) -> Result<(), i32>;
    fn path_filestat_get(&mut self, dirfd: i32, flags: u32, path: &str) -> Result<FileStat, i32>;
    fn path_filestat_set_times(&mut self, dirfd: i32, flags: u32, path: &str, atime: u64, mtime: u64, fst_flags: u16) -> Result<(), i32>;
    fn random_get(&mut self, buf: &mut [u8]) -> Result<(), i32>;
    fn sched_yield(&mut self) -> Result<(), i32>;
    fn poll_oneoff(&mut self, in_events: &[u8], out_events: &mut [u8], nsubscriptions: u32) -> Result<u32, i32>; // simplified for now, struct handling is complex
    fn proc_exit(&mut self, code: i32) -> !;
    
    // Sockets
    fn sock_accept(&mut self, fd: i32, flags: u16) -> Result<i32, i32>;
    fn sock_recv(&mut self, fd: i32, ri_data: &mut [(&mut [u8])], ri_flags: u16) -> Result<(usize, u16), i32>;
    fn sock_send(&mut self, fd: i32, si_data: &[&[u8]], si_flags: u16) -> Result<usize, i32>;
    fn sock_shutdown(&mut self, fd: i32, how: u8) -> Result<(), i32>;

    // Extensions/Extra
    fn fd_readdir(&mut self, fd: i32, cookie: u64) -> Result<Vec<(String, u8, u64)>, i32>; // name, type, inode
}
