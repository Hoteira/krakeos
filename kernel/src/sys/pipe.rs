// Pipes between the terminal and a console app. The app's runner opens dev/pipe/attach/<id>,
// then its fds 0, 1 and 2 go through the pipe instead of the serial port.

use alloc::collections::VecDeque;

use crate::sys::scheduler::{current_tid, MAX_THREADS};

pub const PIPE_FD_BASE: usize = 0x6000_0000;
const MAX_PIPES: usize = 8;
// Bytes buffered in each direction before writers are cut short.
const PIPE_CAP: usize = 64 * 1024;
// Stdin read result when nothing is typed yet; the runner sleeps and retries.
pub const WOULD_BLOCK: usize = usize::MAX - 1;

struct Pipe {
    used: bool,
    // Terminal to app (the app's stdin).
    input: VecDeque<u8>,
    // App to terminal (its stdout and stderr).
    output: VecDeque<u8>,
    // Ctrl + D from the terminal; the app's next read returns 0 once.
    eof: bool,
    owner: usize,
    // True from creation, so the app isn't seen as finished before it starts.
    alive: bool,
}

impl Pipe {
    const fn new() -> Self {
        Pipe { used: false, input: VecDeque::new(), output: VecDeque::new(), eof: false, owner: 0, alive: false }
    }
}

static mut PIPES: [Pipe; MAX_PIPES] = [const { Pipe::new() }; MAX_PIPES];
static mut ATTACHED: [Option<usize>; MAX_THREADS] = [None; MAX_THREADS];

fn pipes() -> &'static mut [Pipe; MAX_PIPES] {
    unsafe { &mut *core::ptr::addr_of_mut!(PIPES) }
}

pub fn is_pipe_fd(fd: usize) -> bool {
    fd >= PIPE_FD_BASE && fd < PIPE_FD_BASE + MAX_PIPES
}

// dev/pipe: a new pipe for the terminal.
pub fn open() -> Option<usize> {
    let i = pipes().iter().position(|p| !p.used)?;
    let p = &mut pipes()[i];
    *p = Pipe::new();
    p.used = true;
    p.owner = current_tid();
    p.alive = true;
    Some(PIPE_FD_BASE + i)
}

// dev/pipe/attach/<id>: route the calling thread's stdio to the pipe.
pub fn attach(id: usize) -> bool {
    if id >= MAX_PIPES || !pipes()[id].used {
        return false;
    }
    unsafe { ATTACHED[current_tid()] = Some(id) };
    true
}

fn attached() -> Option<&'static mut Pipe> {
    let id = unsafe { ATTACHED[current_tid()] }?;
    Some(&mut pipes()[id])
}

pub fn app_write(buf: &[u8]) -> Option<usize> {
    let p = attached()?;
    let n = buf.len().min(PIPE_CAP.saturating_sub(p.output.len()));
    p.output.extend(&buf[..n]);
    Some(n)
}

pub fn app_read(buf: &mut [u8]) -> Option<usize> {
    let p = attached()?;
    if p.input.is_empty() {
        if p.eof {
            p.eof = false;
            return Some(0);
        }
        return Some(WOULD_BLOCK);
    }
    // One line at a time, like a terminal in line mode.
    let mut n = 0;
    while n < buf.len() {
        let Some(b) = p.input.pop_front() else { break };
        buf[n] = b;
        n += 1;
        if b == b'\n' {
            break;
        }
    }
    Some(n)
}

pub fn read(fd: usize, buf: &mut [u8]) -> usize {
    let p = &mut pipes()[fd - PIPE_FD_BASE];
    let mut n = 0;
    while n < buf.len() {
        let Some(b) = p.output.pop_front() else { break };
        buf[n] = b;
        n += 1;
    }
    n
}

// Terminal side. An empty write is end-of-input.
pub fn write(fd: usize, buf: &[u8]) -> usize {
    let p = &mut pipes()[fd - PIPE_FD_BASE];
    if buf.is_empty() {
        p.eof = true;
        return 0;
    }
    let n = buf.len().min(PIPE_CAP.saturating_sub(p.input.len()));
    p.input.extend(&buf[..n]);
    n
}

// 1 while the app runs or has output left, else 0.
pub fn status(fd: usize) -> usize {
    let p = &pipes()[fd - PIPE_FD_BASE];
    (p.alive || !p.output.is_empty()) as usize
}

// Also kills the app if it is still running.
pub fn close(fd: usize) {
    let id = fd - PIPE_FD_BASE;
    pipes()[id] = Pipe::new();
    for tid in 0..MAX_THREADS {
        if unsafe { ATTACHED[tid] } == Some(id) {
            unsafe { ATTACHED[tid] = None };
            crate::sys::scheduler::Scheduler::kill(tid);
        }
    }
}

// Marks its pipe's app finished and closes pipes it opened.
pub fn thread_exited(tid: usize) {
    if let Some(id) = unsafe { ATTACHED[tid].take() } {
        pipes()[id].alive = false;
    }
    for id in 0..MAX_PIPES {
        let p = &pipes()[id];
        if p.used && p.owner == tid {
            close(PIPE_FD_BASE + id);
        }
    }
}
