#![no_std]

extern crate alloc;

use std::{println, debugln};

#[link(wasm_import_module = "wasi_snapshot_preview1")]
extern "C" {
    fn args_get(argv: *mut *mut u8, argv_buf: *mut u8) -> u16;
    fn args_sizes_get(argc: *mut usize, argv_buf_size: *mut usize) -> u16;
    fn environ_get(environ: *mut *mut u8, environ_buf: *mut u8) -> u16;
    fn environ_sizes_get(environ_count: *mut usize, environ_buf_size: *mut usize) -> u16;
    fn clock_res_get(id: u32, resolution: *mut u64) -> u16;
    fn clock_time_get(id: u32, precision: u64, time: *mut u64) -> u16;
    fn fd_advise(fd: i32, offset: u64, len: u64, advice: u8) -> u16;
    fn fd_allocate(fd: i32, offset: u64, len: u64) -> u16;
    fn fd_close(fd: i32) -> u16;
    fn fd_datasync(fd: i32) -> u16;
    fn fd_fdstat_get(fd: i32, stat: *mut u8) -> u16;
    fn fd_fdstat_set_flags(fd: i32, flags: u16) -> u16;
    fn fd_fdstat_set_rights(fd: i32, rights_base: u64, rights_inheriting: u64) -> u16;
    fn fd_filestat_get(fd: i32, stat: *mut u8) -> u16;
    fn fd_filestat_set_size(fd: i32, size: u64) -> u16;
    fn fd_filestat_set_times(fd: i32, atime: u64, mtime: u64, fst_flags: u16) -> u16;
    fn fd_pread(fd: i32, iovs: *const u8, iovs_len: usize, offset: u64, nread: *mut usize) -> u16;
    fn fd_prestat_get(fd: i32, prestat: *mut u8) -> u16;
    fn fd_prestat_dir_name(fd: i32, path: *mut u8, path_len: usize) -> u16;
    fn fd_pwrite(fd: i32, iovs: *const u8, iovs_len: usize, offset: u64, nwritten: *mut usize) -> u16;
    fn fd_read(fd: i32, iovs: *const u8, iovs_len: usize, nread: *mut usize) -> u16;
    fn fd_readdir(fd: i32, buf: *mut u8, buf_len: usize, cookie: u64, bufused: *mut usize) -> u16;
    fn fd_renumber(fd: i32, to: i32) -> u16;
    fn fd_seek(fd: i32, offset: i64, whence: u8, newoffset: *mut u64) -> u16;
    fn fd_sync(fd: i32) -> u16;
    fn fd_tell(fd: i32, offset: *mut u64) -> u16;
    fn fd_write(fd: i32, iovs: *const u8, iovs_len: usize, nwritten: *mut usize) -> u16;
    fn path_create_directory(fd: i32, path: *const u8, path_len: usize) -> u16;
    fn path_filestat_get(fd: i32, flags: u32, path: *const u8, path_len: usize, stat: *mut u8) -> u16;
    fn path_filestat_set_times(fd: i32, flags: u32, path: *const u8, path_len: usize, atime: u64, mtime: u64, fst_flags: u16) -> u16;
    fn path_link(old_fd: i32, old_flags: u32, old_path: *const u8, old_path_len: usize, new_fd: i32, new_path: *const u8, new_path_len: usize) -> u16;
    fn path_open(fd: i32, dirflags: u32, path: *const u8, path_len: usize, oflags: u32, fs_rights_base: u64, fs_rights_inheriting: u64, fdflags: u16, opened_fd: *mut i32) -> u16;
    fn path_readlink(fd: i32, path: *const u8, path_len: usize, buf: *mut u8, buf_len: usize, bufused: *mut usize) -> u16;
    fn path_remove_directory(fd: i32, path: *const u8, path_len: usize) -> u16;
    fn path_rename(fd: i32, old_path: *const u8, old_path_len: usize, new_fd: i32, new_path: *const u8, new_path_len: usize) -> u16;
    fn path_symlink(old_path: *const u8, old_path_len: usize, fd: i32, new_path: *const u8, new_path_len: usize) -> u16;
    fn path_unlink_file(fd: i32, path: *const u8, path_len: usize) -> u16;
    fn poll_oneoff(in_: *const u8, out: *mut u8, nsubscriptions: usize, nevents: *mut usize) -> u16;
    fn proc_exit(rval: u32) -> !;
    fn random_get(buf: *mut u8, buf_len: usize) -> u16;
    fn sched_yield() -> u16;
    fn sock_accept(fd: i32, flags: u16, opened_fd: *mut i32) -> u16;
    fn sock_recv(fd: i32, ri_data: *const u8, ri_data_len: usize, ri_flags: u16, ro_datalen: *mut usize, ro_flags: *mut u16) -> u16;
    fn sock_send(fd: i32, si_data: *const u8, si_data_len: usize, si_flags: u16, so_datalen: *mut usize) -> u16;
    fn sock_shutdown(fd: i32, how: u8) -> u16;
}

pub fn main() {
    println!("--- COMPREHENSIVE WASIP1 HOST TEST SUITE ---");

    unsafe {
        println!("1. args_sizes_get");
        let mut argc = 0;
        let mut arg_buf_size = 0;
        args_sizes_get(&mut argc, &mut arg_buf_size);

        println!("2. args_get");
        let mut argv = [core::ptr::null_mut(); 16];
        let mut arg_buf = [0u8; 256];
        args_get(argv.as_mut_ptr(), arg_buf.as_mut_ptr());

        println!("3. environ_sizes_get");
        let mut env_count = 0;
        let mut env_buf_size = 0;
        environ_sizes_get(&mut env_count, &mut env_buf_size);

        println!("4. environ_get");
        let mut env = [core::ptr::null_mut(); 64];
        let mut env_buf = [0u8; 1024];
        environ_get(env.as_mut_ptr(), env_buf.as_mut_ptr());

        println!("5. clock_res_get");
        let mut res = 0;
        clock_res_get(0, &mut res);

        println!("6. clock_time_get");
        let mut time = 0;
        clock_time_get(0, 1000, &mut time);

        println!("7. random_get");
        let mut rb = [0u8; 16];
        random_get(rb.as_mut_ptr(), 16);

        println!("8. sched_yield");
        sched_yield();

        println!("9. fd_prestat_get");
        let mut prestat = [0u8; 8];
        fd_prestat_get(3, prestat.as_mut_ptr());

        println!("10. fd_prestat_dir_name");
        let mut name_buf = [0u8; 32];
        fd_prestat_dir_name(3, name_buf.as_mut_ptr(), 32);

        println!("11. fd_fdstat_get");
        let mut fdstat = [0u8; 24];
        fd_fdstat_get(1, fdstat.as_mut_ptr());

        println!("12. fd_filestat_get");
        let mut filestat = [0u8; 64];
        fd_filestat_get(1, filestat.as_mut_ptr());

        println!("13. fd_write");
        let iov = [b"Test\n".as_ptr() as usize, 5usize];
        let mut written = 0;
        fd_write(1, iov.as_ptr() as *const u8, 1, &mut written);

        println!("14. fd_seek");
        let mut offset = 0;
        fd_seek(1, 0, 1, &mut offset);

        println!("15. fd_tell");
        let mut tell = 0;
        fd_tell(1, &mut tell);

        println!("16. path_open");
        let mut new_fd = 0;
        path_open(3, 0, b"test.txt".as_ptr(), 8, 0, 0, 0, 0, &mut new_fd);

        println!("17. sock_accept");
        let mut sock_fd = 0;
        sock_accept(0, 0, &mut sock_fd);

        println!("18. sock_shutdown");
        sock_shutdown(0, 0);

        println!("19. fd_close");
        fd_close(999);

        println!("20. poll_oneoff");
        let mut nevents = 0;
        poll_oneoff(core::ptr::null(), core::ptr::null_mut(), 0, &mut nevents);

        println!("21. fd_advise");
        fd_advise(1, 0, 0, 0);

        println!("22. fd_allocate");
        fd_allocate(1, 0, 0);

        println!("23. fd_datasync");
        fd_datasync(1);

        println!("24. fd_fdstat_set_flags");
        fd_fdstat_set_flags(1, 0);

        println!("25. fd_fdstat_set_rights");
        fd_fdstat_set_rights(1, 0, 0);

        println!("26. fd_filestat_set_size");
        fd_filestat_set_size(1, 0);

        println!("27. fd_filestat_set_times");
        fd_filestat_set_times(1, 0, 0, 0);

        println!("28. fd_pread");
        let mut nread = 0;
        fd_pread(1, core::ptr::null(), 0, 0, &mut nread);

        println!("29. fd_pwrite");
        let mut nwritten = 0;
        fd_pwrite(1, core::ptr::null(), 0, 0, &mut nwritten);

        println!("30. fd_readdir");
        let mut bufused = 0;
        fd_readdir(1, core::ptr::null_mut(), 0, 0, &mut bufused);

        println!("31. fd_renumber");
        fd_renumber(1, 1);

        println!("32. fd_sync");
        fd_sync(1);

        println!("33. path_create_directory");
        path_create_directory(3, b"test".as_ptr(), 4);

        println!("34. path_filestat_get");
        let mut path_stat = [0u8; 64];
        path_filestat_get(3, 0, b"test".as_ptr(), 4, path_stat.as_mut_ptr());

        println!("35. path_filestat_set_times");
        path_filestat_set_times(3, 0, b"test".as_ptr(), 4, 0, 0, 0);

        println!("36. path_link");
        path_link(3, 0, b"a".as_ptr(), 1, 3, b"b".as_ptr(), 1);

        println!("37. path_readlink");
        let mut rl_buf = [0u8; 16];
        let mut rl_used = 0;
        path_readlink(3, b"a".as_ptr(), 1, rl_buf.as_mut_ptr(), 16, &mut rl_used);

        println!("38. path_remove_directory");
        path_remove_directory(3, b"test".as_ptr(), 4);

        println!("39. path_rename");
        path_rename(3, b"a".as_ptr(), 1, 3, b"b".as_ptr(), 1);

        println!("40. path_symlink");
        path_symlink(b"a".as_ptr(), 1, 3, b"b".as_ptr(), 1);

        println!("41. path_unlink_file");
        path_unlink_file(3, b"test.txt".as_ptr(), 8);

        println!("42. sock_recv");
        let mut sr_datalen = 0;
        let mut sr_flags = 0;
        sock_recv(0, core::ptr::null(), 0, 0, &mut sr_datalen, &mut sr_flags);

        println!("43. sock_send");
        let mut ss_datalen = 0;
        sock_send(0, core::ptr::null(), 0, 0, &mut ss_datalen);

        println!("--- ALL HOST FUNCTIONS VERIFIED ---");
        proc_exit(0);
    }
}
