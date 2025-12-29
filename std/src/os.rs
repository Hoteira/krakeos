use core::arch::asm;

pub unsafe fn syscall(num: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    let result: u64;

    unsafe {
        asm!(
            "syscall",
            in("rax") num,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            lateout("rax") result,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }

    result
}

pub unsafe fn syscall4(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> u64 {
    let result: u64;

    unsafe {
        asm!(
            "syscall",
            in("rax") num,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            in("r10") arg4,
            lateout("rax") result,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }

    result
}

pub fn print(s: &str) {
    unsafe {
        syscall(1, s.as_ptr() as u64, s.len() as u64, 0);
    }
}

pub fn sleep(ms: u64) {
    unsafe {
        syscall(76, ms, 0, 0);
    }
}

pub fn yield_task() {
    unsafe {
        asm!("int 0x20");
    }

}



pub fn read(buffer: &mut [u8]) -> usize {
    unsafe {
        syscall(0, buffer.as_mut_ptr() as u64, buffer.len() as u64, 0) as usize
    }
}

pub fn file_read(fd: usize, buffer: &mut [u8]) -> usize {
    unsafe {
        syscall(62, fd as u64, buffer.as_mut_ptr() as u64, buffer.len() as u64) as usize
    }
}

pub fn file_write(fd: usize, buffer: &[u8]) -> usize {
    unsafe {
        syscall(63, fd as u64, buffer.as_ptr() as u64, buffer.len() as u64) as usize
    }
}

pub fn file_seek(fd: usize, offset: i64, whence: usize) -> u64 {
    unsafe {
        syscall(75, fd as u64, offset as u64, whence as u64)
    }
}

pub fn pipe(fds: &mut [i32; 2]) -> i32 {
    unsafe {
        syscall(42, fds.as_mut_ptr() as u64, 0, 0) as i32
    }
}

pub fn file_close(fd: usize) -> i32 {
    unsafe {
        syscall(67, fd as u64, 0, 0) as i32
    }
}

pub fn exit(code: u64) -> ! {

    unsafe {

        syscall(60, code, 0, 0);

        loop { asm!("hlt"); }

    }

}

pub fn exec(path: &str) {
    unsafe {
        syscall(66, path.as_ptr() as u64, path.len() as u64, 0);
    }
}

pub fn spawn(path: &str) -> usize {
    unsafe {
        syscall(66, path.as_ptr() as u64, path.len() as u64, 0) as usize
    }
}

pub fn spawn_with_fds(path: &str, fds: &[(u8, u8)]) -> usize {
    unsafe {
        syscall4(66, path.as_ptr() as u64, path.len() as u64, fds.as_ptr() as u64, fds.len() as u64) as usize
    }
}

pub fn waitpid(pid: usize) -> usize {
    unsafe {
        loop {
            let status = syscall(68, pid as u64, 0, 0);
            if status != u64::MAX {
                return status as usize;
            }
            yield_task();
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct PollFd {
    pub fd: i32,
    pub events: i16,
    pub revents: i16,
}

pub const POLLIN: i16 = 0x001;
pub const POLLOUT: i16 = 0x004;

pub fn poll(fds: &mut [PollFd], timeout: i32) -> i32 {
    unsafe {
        syscall(70, fds.as_mut_ptr() as u64, fds.len() as u64, timeout as u64) as i32
    }
}

pub fn get_time() -> (u8, u8, u8) {
    let res = unsafe { syscall(54, 0, 0, 0) };
    let h = ((res >> 16) & 0xFF) as u8;
    let m = ((res >> 8) & 0xFF) as u8;
    let s = (res & 0xFF) as u8;
    (h, m, s)
}
