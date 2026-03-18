use crate::memory::address::PhysAddr;
use crate::memory::{paging, pmm, vmm};
use crate::sync::Mutex;
use crate::task::process::Process;
use crate::task::thread::{CPUState, Thread, ThreadState};
use alloc::boxed::Box;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::sync::Arc;
use alloc::vec::Vec;

pub const MAX_THREADS: usize = 999999; 
pub const MAX_PROCESSES: usize = 64;
pub const STACK_SIZE: u64 = 1024 * 1024;

pub struct TaskManager {
    pub current_task: isize,
    pub thread_count: usize,
    pub tasks: BTreeMap<usize, Box<Thread>>,
    pub run_queue: VecDeque<usize>,
    pub next_tid: usize,
}

pub static TASK_MANAGER: Mutex<TaskManager> = Mutex::new(TaskManager {
    current_task: -1,
    thread_count: 0,
    tasks: BTreeMap::new(),
    run_queue: VecDeque::new(),
    next_tid: 1,
});

impl TaskManager {
    pub fn init(&mut self) {
        let mut idle_thread = Thread::new(b"idle");
        idle_thread.state = ThreadState::Ready;

        unsafe {
            let kernel_proc = Process::new(0, 0, 0, None);
            idle_thread.process = Some(kernel_proc);

            let stack_pages = (STACK_SIZE / 4096) as usize;
            let stack_phys = pmm::allocate_frames(stack_pages).expect("Idle stack allocation failed");
            idle_thread.kernel_stack = stack_phys + STACK_SIZE + paging::HHDM_OFFSET;

            let state_size = core::mem::size_of::<CPUState>();
            let state_ptr = (idle_thread.kernel_stack - state_size as u64) as *mut CPUState;
            idle_thread.cpu_state_ptr = state_ptr as u64;

            (*state_ptr).rip = crate::task::scheduler::idle as u64;
            (*state_ptr).cs = 0x08; // 64-bit kernel code segment (GDT index 1)
            (*state_ptr).rflags = 0x202;
            (*state_ptr).rsp = idle_thread.kernel_stack;
            (*state_ptr).ss = 0x10;

            self.tasks.insert(0, Box::new(idle_thread));
            self.run_queue.push_back(0);
            self.thread_count = 1;
            self.current_task = 0;
        }
    }

    pub fn current_task_idx(&self) -> Option<usize> {
        if self.current_task >= 0 {
            Some(self.current_task as usize)
        } else {
            None
        }
    }

    pub fn schedule(&mut self, cpu_state: *mut CPUState) -> (*mut CPUState, u64) {
        let sp = cpu_state as u64;
        if sp < 0x1000 {
            crate::debugln!("CRITICAL: Stack pointer is dangerously low: {:#x}", sp);
        }

        let mut to_wake = Vec::new();
        for (tid, thread) in self.tasks.iter_mut() {
            if thread.state == ThreadState::Sleeping && unsafe { crate::task::scheduler::SYSTEM_TICKS } >= thread.wake_ticks {
                thread.state = ThreadState::Ready;
                to_wake.push(*tid);
            }
        }
        for tid in to_wake {
            if !self.run_queue.contains(&tid) {
                self.run_queue.push_back(tid);
            }
        }

        if self.current_task >= 0 {
            if let Some(thread) = self.tasks.get_mut(&(self.current_task as usize)) {
                thread.cpu_state_ptr = cpu_state as u64;
                if thread.state == ThreadState::Ready {
                    self.run_queue.push_back(self.current_task as usize);
                }
            }
        }

        self.current_task = self.get_next_thread();

        if self.current_task < 0 {
            return (cpu_state, 0);
        }

        let thread = self.tasks.get(&(self.current_task as usize)).unwrap();
        (
            thread.cpu_state_ptr as *mut CPUState,
            thread.kernel_stack,
        )
    }

    fn get_next_thread(&mut self) -> isize {
        while let Some(tid) = self.run_queue.pop_front() {
            if let Some(thread) = self.tasks.get(&tid) {
                if thread.state == ThreadState::Ready {
                    return tid as isize;
                }
            }
        }
        0 // Fallback to idle task (0)
    }

    pub fn reserve_pid(&mut self) -> Result<usize, pmm::FrameError> {
        let tid = self.next_tid;
        self.next_tid += 1;
        let mut t = Thread::new(b"reserved");
        t.state = ThreadState::Reserved;
        self.tasks.insert(tid, Box::new(t));
        self.run_queue.push_back(tid);
        self.thread_count += 1;
        Ok(tid)
    }

    pub fn kill_process(&mut self, pid: u64) {
        for (_, thread) in self.tasks.iter_mut() {
            if let Some(proc) = &thread.process {
                if proc.pid == pid {
                    thread.state = ThreadState::Zombie;
                    *proc.event_queue.lock() = (0, 0, 0);
                    unsafe {
                        (*(&raw mut crate::window_manager::composer::COMPOSER)).remove_windows_by_pid(pid);
                    }
                }
            }
        }
    }

    pub fn init_user_task(&mut self, slot: usize, entry_point: u64, _pml4: u64, args: Option<&[&str]>, fd_table: Option<Vec<i16>>, name: &[u8], terminal_size: (u16, u16), parent_pid: Option<u64>) -> Result<(), pmm::FrameError> {
        let pid = slot as u64;
        let mut thread = Thread::new(name);

        let (uid, gid) = if let Some(ppid) = parent_pid {
            if let Some(parent_thread) = self.tasks.values().find(|t| t.process.as_ref().map_or(false, |p| p.pid == ppid)) {
                let p = parent_thread.process.as_ref().unwrap();
                (p.uid, p.gid)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        };

        let proc = Process::new(pid, uid, gid, parent_pid);

        if let Some(fds) = fd_table {
            *proc.fd_table.lock() = fds;
        }
        *proc.terminal_width.lock() = terminal_size.0;
        *proc.terminal_height.lock() = terminal_size.1;

        thread.process = Some(proc.clone());

        let k_frame = pmm::allocate_frames(16).ok_or(pmm::FrameError::NoMemory)?;
        thread.kernel_stack = k_frame + 4096 * 16 + paging::HHDM_OFFSET;

        let stack_pages = (STACK_SIZE / 4096) as usize;
        let u_frame_phys = pmm::allocate_frames(stack_pages).ok_or(pmm::FrameError::NoMemory)?;

        let u_stack_top = proc.stack_base;
        let u_stack_base = u_stack_top - STACK_SIZE;

        for i in 1..stack_pages {
            let offset = i as u64 * 4096;
            vmm::map_page(u_stack_base + offset, PhysAddr::new(u_frame_phys + offset),
                          paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER,
                          None);
        }
        thread.user_stack = u_stack_top;

        let state_size = core::mem::size_of::<CPUState>();
        let state_ptr = (thread.kernel_stack - state_size as u64) as *mut CPUState;
        thread.cpu_state_ptr = state_ptr as u64;

        unsafe {
            let stack_phys_base = u_frame_phys + paging::HHDM_OFFSET;
            let mut current_virt_sp = thread.user_stack;

            let mut arg_ptrs = Vec::new();
            let mut push_str = |s: &[u8]| {
                let len = s.len() + 1;
                current_virt_sp -= len as u64;
                let offset = current_virt_sp - u_stack_base;
                let dest = (stack_phys_base + offset) as *mut u8;
                core::ptr::copy_nonoverlapping(s.as_ptr(), dest, s.len());
                *dest.add(s.len()) = 0;
                current_virt_sp
            };

            arg_ptrs.push(push_str(name));
            if let Some(a_list) = args {
                for &a in a_list {
                    arg_ptrs.push(push_str(a.as_bytes()));
                }
            }

            current_virt_sp &= !15;
            let mut push_u64 = |val: u64| {
                current_virt_sp -= 8;
                let offset = current_virt_sp - u_stack_base;
                let dest = (stack_phys_base + offset) as *mut u64;
                *dest = val;
            };

            push_u64(0);
            push_u64(0);
            for &ptr in arg_ptrs.iter().rev() { push_u64(ptr); }
            push_u64(arg_ptrs.len() as u64);

            (*state_ptr).rax = 0;
            (*state_ptr).rip = entry_point;
            (*state_ptr).cs = 0x23; 
            (*state_ptr).rflags = 0x202;
            (*state_ptr).rsp = current_virt_sp;
            (*state_ptr).ss = 0x1B; 
        }

        thread.state = if entry_point == 0 { ThreadState::Reserved } else { ThreadState::Ready };
        self.tasks.insert(slot, Box::new(thread));
        self.run_queue.push_back(slot);
        Ok(())
    }

    pub fn spawn_thread(&mut self, parent_tid: usize, entry_point: u64, user_stack: u64, arg: u64) -> Result<usize, pmm::FrameError> {
        let tid = self.reserve_pid()?;

        let parent_process = if let Some(t) = self.tasks.get(&(parent_tid)) {
            if let Some(p) = &t.process {
                p.clone()
            } else {
                return Err(pmm::FrameError::IndexOutOfBounds);
            }
        } else {
            return Err(pmm::FrameError::IndexOutOfBounds);
        };

        let mut thread = Thread::new(b"thread");
        thread.process = Some(parent_process.clone());

        let k_frame = pmm::allocate_frames(64).ok_or(pmm::FrameError::NoMemory)?;
        thread.kernel_stack = k_frame + 4096 * 64 + paging::HHDM_OFFSET;

        let state_size = core::mem::size_of::<CPUState>();
        let state_ptr = (thread.kernel_stack - state_size as u64) as *mut CPUState;
        thread.cpu_state_ptr = state_ptr as u64;

        unsafe {
            core::ptr::write_bytes(state_ptr, 0, 1);
            (*state_ptr).rip = entry_point;
            
            if user_stack == 0 {
                (*state_ptr).cs = 0x08; 
                (*state_ptr).ss = 0x10;
                (*state_ptr).rsp = thread.kernel_stack - state_size as u64 - 8; 
                (*state_ptr).rflags = 0x202;
            } else {
                (*state_ptr).cs = 0x23; 
                (*state_ptr).ss = 0x1B; 
                (*state_ptr).rsp = (user_stack & !15) - 8;
                (*state_ptr).rflags = 0x202;
            }
            
            (*state_ptr).rdi = arg;
        }

        thread.state = ThreadState::Ready;
        self.tasks.insert(tid, Box::new(thread));
        self.run_queue.push_back(tid);

        Ok(tid)
    }

    pub fn get_tasks(&self) -> alloc::collections::btree_map::Values<usize, Box<Thread>> { self.tasks.values() }

    pub fn current_thread(&self) -> &Thread {
        self.tasks.get(&(self.current_task as usize)).expect("No current thread")
    }

    pub fn current_thread_mut(&mut self) -> &mut Thread {
        self.tasks.get_mut(&(self.current_task as usize)).expect("No current thread")
    }
}
