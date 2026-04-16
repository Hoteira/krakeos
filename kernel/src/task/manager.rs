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
    pub thread_count: usize,
    pub tasks: BTreeMap<usize, Box<Thread>>,
    pub run_queues: [VecDeque<usize>; 64],
    pub next_tid: usize,
}

pub static TASK_MANAGER: Mutex<TaskManager> = Mutex::new(TaskManager {
    thread_count: 0,
    tasks: BTreeMap::new(),
    run_queues: [const { VecDeque::new() }; 64],
    next_tid: 64,
});

impl TaskManager {
    pub fn init(&mut self) {
        for cpu_id in 0..64 {
            let mut idle_thread = Thread::new(b"idle");
            idle_thread.state = ThreadState::Ready;

            unsafe {
                let kernel_proc = Process::new(0, 0, 0, None);
                idle_thread.process = Some(kernel_proc);

                let stack_pages = (STACK_SIZE / 4096) as usize;
                let stack_phys =
                    pmm::allocate_frames(stack_pages).expect("Idle stack allocation failed");
                idle_thread.kernel_stack = stack_phys + STACK_SIZE + paging::HHDM_OFFSET;

                let state_size = core::mem::size_of::<CPUState>();
                let state_ptr = (idle_thread.kernel_stack - state_size as u64) as *mut CPUState;
                idle_thread.cpu_state_ptr = state_ptr as u64;

                (*state_ptr).rip = crate::task::scheduler::idle as u64;
                (*state_ptr).cs = 0x08; // 64-bit kernel code segment (GDT index 1)
                (*state_ptr).rflags = 0x202;
                (*state_ptr).rsp = idle_thread.kernel_stack;
                (*state_ptr).ss = 0x10;

                self.tasks.insert(cpu_id, Box::new(idle_thread));
            }
        }
        self.thread_count = 64;
        // Do not push idle tasks to run queues, handle as fallback in get_next_thread
    }

    pub fn current_task_idx(&self) -> Option<usize> {
        let idx = crate::task::cpu::get_current_task_idx();
        if idx >= 0 { Some(idx as usize) } else { None }
    }

    pub fn schedule(
        &mut self,
        cpu_state: *mut CPUState,
        _is_timer: bool,
    ) -> (*mut CPUState, u64, i64) {
        let cpu_id = crate::task::cpu::get_cpu_id() as usize;
        let current_task_idx = crate::task::cpu::get_current_task_idx();

        if current_task_idx >= 0 {
            if let Some(thread) = self.tasks.get_mut(&(current_task_idx as usize)) {
                thread.cpu_state_ptr = cpu_state as u64;
                if thread.state == ThreadState::Ready {
                    self.push_to_run_queue(current_task_idx as usize);
                }
            }
        }

        let next_tid = self.get_next_thread(cpu_id);

        if next_tid == 2 {
            //crate::debugln!("[TaskManager] SCHEDULING TID 2!");
        }

        let thread = match self.tasks.get(&(next_tid as usize)) {
            Some(t) => t,
            None => {
                crate::debugln!(
                    "TaskManager schedule: next_tid {} not found! Falling back to {}",
                    next_tid,
                    cpu_id
                );
                crate::debugln!("Current keys in tasks map:");
                for k in self.tasks.keys() {
                    crate::debugln!(" - {}", k);
                }
                self.tasks.get(&cpu_id).unwrap()
            }
        };
        (
            thread.cpu_state_ptr as *mut CPUState,
            thread.kernel_stack,
            next_tid as i64,
        )
    }

    fn get_next_thread(&mut self, cpu_id: usize) -> isize {
        // 1. Try to find a Ready task in local queue
        let q_len = self.run_queues[cpu_id].len();
        for _ in 0..q_len {
            if let Some(tid) = self.run_queues[cpu_id].pop_front() {
                if let Some(t) = self.tasks.get_mut(&tid) {
                    // Respect CPU pinning: skip tasks pinned to a different CPU
                    if let Some(pin) = t.pinned_cpu {
                        if pin != cpu_id {
                            self.run_queues[pin % 64].push_back(tid);
                            continue;
                        }
                    }
                    if t.state == ThreadState::Ready {
                        t.is_queued = false;
                        return tid as isize;
                    } else {
                        // Not ready (e.g. Sleeping or WaitingForEvent), put back at end
                        self.run_queues[cpu_id].push_back(tid);
                    }
                }
            }
        }

        // 2. Try work stealing from other CPUs (only steal non-pinned tasks)
        for i in 1..64 {
            let target_cpu = (cpu_id + i) % 64;
            let q_len = self.run_queues[target_cpu].len();
            for _ in 0..q_len {
                if let Some(tid) = self.run_queues[target_cpu].pop_front() {
                    if let Some(t) = self.tasks.get_mut(&tid) {
                        // Never steal pinned tasks
                        if t.pinned_cpu.is_some() {
                            self.run_queues[target_cpu].push_back(tid);
                            continue;
                        }
                        if t.state == ThreadState::Ready {
                            t.is_queued = false;
                            return tid as isize;
                        } else {
                            self.run_queues[target_cpu].push_back(tid);
                        }
                    }
                }
            }
        }

        cpu_id as isize // Fallback to per-CPU idle
    }

    /// Pin a thread to a specific CPU. It will only ever run on that CPU.
    pub fn pin_thread_to_cpu(&mut self, tid: usize, cpu_id: usize) {
        if let Some(thread) = self.tasks.get_mut(&tid) {
            thread.pinned_cpu = Some(cpu_id);
        }
    }

    pub fn reserve_pid(&mut self) -> Result<usize, pmm::FrameError> {
        let tid = self.next_tid;
        self.next_tid += 1;
        let mut t = Thread::new(b"reserved");
        t.state = ThreadState::Reserved;

        self.tasks.insert(tid, Box::new(t));
        self.thread_count += 1;
        Ok(tid)
    }

    pub fn push_to_run_queue(&mut self, tid: usize) {
        if tid < 64 {
            return;
        }
        if let Some(thread) = self.tasks.get_mut(&tid) {
            if !thread.is_queued {
                thread.is_queued = true;
                let mut cpu_id = crate::task::cpu::get_cpu_id() as usize;
                if cpu_id >= 64 {
                    cpu_id = 0;
                }
                self.run_queues[cpu_id].push_back(tid);
            }
        }
    }

    /// Explicitly push a newly spawned task to the least loaded CPU's run queue for
    /// the first time; use `push_to_run_queue` when re-queuing an existing thread.
    pub fn push_new_task(&mut self, tid: usize) {
        if tid < 64 {
            return;
        }
        if let Some(thread) = self.tasks.get_mut(&tid) {
            if !thread.is_queued {
                thread.is_queued = true;
                // If the thread is pinned, push directly to the pinned CPU's queue
                if let Some(pin) = thread.pinned_cpu {
                    self.run_queues[pin % 64].push_back(tid);
                    return;
                }
                let cpu_count = crate::arch::x86_64::smp::CPU_COUNT
                    .load(core::sync::atomic::Ordering::Relaxed)
                    .min(64);
                let mut best_cpu = 0usize;
                let mut min_len = usize::MAX;
                for i in 0..cpu_count {
                    let len = self.run_queues[i].len();
                    if len < min_len {
                        min_len = len;
                        best_cpu = i;
                    }
                }
                self.run_queues[best_cpu].push_back(tid);
            }
        }
    }

    pub fn init_user_task(
        &mut self,
        slot: usize,
        entry_point: u64,
        arg: u64,
        _pml4: u64,
        args: Option<&[&str]>,
        fd_table: Option<Vec<i16>>,
        name: &[u8],
        terminal_size: (u16, u16),
        parent_pid: Option<u64>,
    ) -> Result<(), pmm::FrameError> {
        crate::spawn_debugln!(
            "[TaskManager] init_user_task entry: slot={}, entry={:#x}",
            slot,
            entry_point
        );
        let pid = slot as u64;
        crate::spawn_debugln!("[TaskManager] init_user_task: pid={}", pid);

        crate::spawn_debugln!("[TaskManager] init_user_task: calling tasks.remove");
        let mut thread_box = if let Some(existing) = self.tasks.remove(&slot) {
            crate::spawn_debugln!("[TaskManager] Reusing existing task {}", slot);
            existing
        } else {
            crate::spawn_debugln!("[TaskManager] init_user_task: calling Thread::new");
            let t = Box::new(Thread::new(name));
            crate::spawn_debugln!("[TaskManager] init_user_task: Thread::new returned");
            t
        };
        crate::spawn_debugln!("[TaskManager] init_user_task: thread_box obtained");

        crate::spawn_debugln!("[TaskManager] init_user_task: resolving uid/gid");
        let (uid, gid) = if let Some(ppid) = parent_pid {
            crate::spawn_debugln!("[TaskManager] init_user_task: parent_pid={}", ppid);
            crate::spawn_debugln!("[TaskManager] init_user_task: calling tasks.values");
            let parent = self
                .tasks
                .values()
                .find(|t| t.process.as_ref().map_or(false, |p| p.pid == ppid));
            crate::spawn_debugln!("[TaskManager] init_user_task: find returned");
            if let Some(parent_thread) = parent {
                crate::spawn_debugln!("[TaskManager] init_user_task: parent found");
                let p = parent_thread.process.as_ref().unwrap();
                (p.uid, p.gid)
            } else {
                crate::spawn_debugln!(
                    "[TaskManager] init_user_task: parent not found, defaulting to 0,0"
                );
                (0, 0)
            }
        } else {
            crate::spawn_debugln!("[TaskManager] init_user_task: no parent_pid, defaulting to 0,0");
            (0, 0)
        };
        crate::spawn_debugln!("[TaskManager] init_user_task: uid={}, gid={}", uid, gid);

        crate::spawn_debugln!("[TaskManager] init_user_task: calling Process::new");
        let proc = Process::new(pid, uid, gid, parent_pid);
        crate::spawn_debugln!("[TaskManager] init_user_task: Process::new returned");

        if let Some(fds) = fd_table {
            crate::spawn_debugln!("[TaskManager] init_user_task: setting fd_table");
            crate::spawn_debugln!("[TaskManager] init_user_task: calling fd_table.lock");
            let mut guard = proc.fd_table.lock();
            crate::spawn_debugln!("[TaskManager] init_user_task: lock acquired");
            *guard = fds;
            crate::spawn_debugln!("[TaskManager] init_user_task: fd_table updated");
        }
        crate::spawn_debugln!("[TaskManager] init_user_task: setting terminal size");
        crate::spawn_debugln!("[TaskManager] init_user_task: calling terminal_width.lock");
        *proc.terminal_width.lock() = terminal_size.0;
        crate::spawn_debugln!("[TaskManager] init_user_task: terminal_width set");
        crate::spawn_debugln!("[TaskManager] init_user_task: calling terminal_height.lock");
        *proc.terminal_height.lock() = terminal_size.1;
        crate::spawn_debugln!("[TaskManager] init_user_task: terminal_height set");

        let thread = &mut *thread_box;
        crate::spawn_debugln!("[TaskManager] init_user_task: calling proc.clone");
        thread.process = Some(proc.clone());
        crate::spawn_debugln!("[TaskManager] init_user_task: proc.clone returned");

        crate::spawn_debugln!("[TaskManager] init_user_task: calling pmm::allocate_frames(256)");
        let k_frame = pmm::allocate_frames(256).ok_or(pmm::FrameError::NoMemory)?;
        crate::spawn_debugln!("[TaskManager] init_user_task: k_frame={:#x}", k_frame);
        thread.kernel_stack = k_frame + 1024 * 1024 + paging::HHDM_OFFSET;
        crate::spawn_debugln!(
            "[TaskManager] init_user_task: kernel_stack={:#x}",
            thread.kernel_stack
        );

        let stack_pages = (STACK_SIZE / 4096) as usize;
        crate::spawn_debugln!(
            "[TaskManager] init_user_task: calling pmm::allocate_frames({})",
            stack_pages
        );
        let u_frame_phys = pmm::allocate_frames(stack_pages).ok_or(pmm::FrameError::NoMemory)?;
        crate::spawn_debugln!(
            "[TaskManager] init_user_task: u_frame_phys={:#x}",
            u_frame_phys
        );

        let u_stack_top = proc.stack_base;
        let u_stack_base = u_stack_top - STACK_SIZE;
        crate::spawn_debugln!(
            "[TaskManager] init_user_task: u_stack_top={:#x}, u_stack_base={:#x}",
            u_stack_top,
            u_stack_base
        );

        crate::spawn_debugln!("[TaskManager] init_user_task: starting stack mapping loop");
        for i in 0..stack_pages {
            let offset = i as u64 * 4096;
            //crate::spawn_debugln!("[TaskManager] init_user_task: mapping stack page {}", i);
            vmm::map_page(
                u_stack_base + offset,
                PhysAddr::new(u_frame_phys + offset),
                paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER,
                None,
            );
        }
        crate::spawn_debugln!("[TaskManager] init_user_task: stack mapping loop finished");
        thread.user_stack = u_stack_top;
        crate::spawn_debugln!("[TaskManager] init_user_task: thread.user_stack set");

        let code_pages = (64 * 1024 * 1024) / 4096;
        crate::spawn_debugln!(
            "[TaskManager] init_user_task: starting code mapping loop, code_pages={}",
            code_pages
        );
        for i in 0..code_pages {
            let virt = proc.code_base + i as u64 * 4096;
            if i < 4096 {
                if i % 1024 == 0 {
                    crate::spawn_debugln!("[TaskManager] init_user_task: mapping code page {}", i);
                }
                if let Some(frame) = pmm::allocate_frame() {
                    vmm::map_page(
                        virt,
                        PhysAddr::new(frame),
                        paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER,
                        None,
                    );
                }
            }
        }
        crate::spawn_debugln!("[TaskManager] init_user_task: code mapping loop finished");

        crate::spawn_debugln!("[TaskManager] init_user_task: calling core::mem::size_of");
        let state_size = core::mem::size_of::<CPUState>();
        crate::spawn_debugln!("[TaskManager] init_user_task: state_size={}", state_size);
        let state_ptr = (thread.kernel_stack - state_size as u64) as *mut CPUState;
        thread.cpu_state_ptr = state_ptr as u64;
        crate::spawn_debugln!(
            "[TaskManager] init_user_task: thread.cpu_state_ptr={:#x}",
            thread.cpu_state_ptr
        );

        unsafe {
            crate::spawn_debugln!(
                "[TaskManager] init_user_task: entering unsafe block for stack preparation"
            );
            let stack_phys_base = u_frame_phys + paging::HHDM_OFFSET;
            let mut current_virt_sp = thread.user_stack;
            crate::spawn_debugln!(
                "[TaskManager] init_user_task: stack_phys_base={:#x}, current_virt_sp={:#x}",
                stack_phys_base,
                current_virt_sp
            );

            crate::spawn_debugln!("[TaskManager] init_user_task: calling Vec::new");
            let mut arg_ptrs = Vec::new();
            crate::spawn_debugln!("[TaskManager] init_user_task: Vec::new returned");

            let mut push_str = |s: &[u8]| {
                let len = s.len() + 1;
                current_virt_sp -= len as u64;
                let offset = current_virt_sp - u_stack_base;
                let dest = (stack_phys_base + offset) as *mut u8;
                crate::spawn_debugln!(
                    "[TaskManager] init_user_task: pushing string of len {}, dest={:#x}",
                    s.len(),
                    dest as u64
                );
                core::ptr::copy_nonoverlapping(s.as_ptr(), dest, s.len());
                *dest.add(s.len()) = 0;
                current_virt_sp
            };

            crate::spawn_debugln!("[TaskManager] init_user_task: pushing name");
            let name_ptr = push_str(name);
            arg_ptrs.push(name_ptr);
            crate::spawn_debugln!(
                "[TaskManager] init_user_task: name pushed at {:#x}",
                name_ptr
            );

            if let Some(a_list) = args {
                crate::spawn_debugln!("[TaskManager] init_user_task: starting args push loop");
                for &a in a_list {
                    crate::spawn_debugln!("[TaskManager] init_user_task: pushing arg");
                    let a_ptr = push_str(a.as_bytes());
                    arg_ptrs.push(a_ptr);
                }
                crate::spawn_debugln!("[TaskManager] init_user_task: args push loop finished");
            }

            current_virt_sp &= !15;
            crate::spawn_debugln!(
                "[TaskManager] init_user_task: current_virt_sp aligned to {:#x}",
                current_virt_sp
            );
            let mut push_u64 = |val: u64| {
                current_virt_sp -= 8;
                let offset = current_virt_sp - u_stack_base;
                let dest = (stack_phys_base + offset) as *mut u64;
                crate::spawn_debugln!(
                    "[TaskManager] init_user_task: pushing u64={:#x} to dest={:#x}",
                    val,
                    dest as u64
                );
                *dest = val;
            };

            crate::spawn_debugln!("[TaskManager] init_user_task: pushing stack frames");
            push_u64(0);
            push_u64(0);
            crate::spawn_debugln!("[TaskManager] init_user_task: starting arg_ptrs rev loop");
            for &ptr in arg_ptrs.iter().rev() {
                push_u64(ptr);
            }
            crate::spawn_debugln!("[TaskManager] init_user_task: arg_ptrs rev loop finished");
            push_u64(arg_ptrs.len() as u64);
            crate::spawn_debugln!(
                "[TaskManager] init_user_task: arg_count={} pushed",
                arg_ptrs.len()
            );

            crate::spawn_debugln!("[TaskManager] init_user_task: calling core::ptr::write_bytes");
            core::ptr::write_bytes(state_ptr, 0, 1);
            crate::spawn_debugln!("[TaskManager] init_user_task: state_ptr zeroed");
            (*state_ptr).rip = entry_point;
            (*state_ptr).rdi = arg;
            (*state_ptr).cs = 0x23;
            (*state_ptr).rflags = 0x202;
            (*state_ptr).rsp = (current_virt_sp & !15) - 8;
            (*state_ptr).ss = 0x1B;
            let final_rsp = (*state_ptr).rsp;
            crate::spawn_debugln!(
                "[TaskManager] init_user_task: CPUState initialized, rsp={:#x}",
                final_rsp
            );
        }

        let final_state = if entry_point == 0 {
            ThreadState::Reserved
        } else {
            ThreadState::Ready
        };
        thread.state = final_state;
        crate::spawn_debugln!(
            "[TaskManager] init_user_task: final_state={:?}",
            thread.state
        );
        let ptr = thread as *const _ as u64;
        crate::spawn_debugln!(
            "[TaskManager] Initialized User Task {} (Thread at {:#x}, State={:?})",
            slot,
            ptr,
            thread.state
        );

        let is_ready = thread.state == ThreadState::Ready;
        crate::spawn_debugln!("[TaskManager] init_user_task: calling tasks.insert");
        self.tasks.insert(slot, thread_box);
        crate::spawn_debugln!("[TaskManager] init_user_task: tasks.insert returned");
        if is_ready {
            crate::spawn_debugln!("[TaskManager] init_user_task: calling push_new_task");
            self.push_new_task(slot);
            crate::spawn_debugln!("[TaskManager] init_user_task: push_new_task returned");
        }
        crate::spawn_debugln!("[TaskManager] init_user_task exit success");
        Ok(())
    }

    pub fn spawn_thread(
        &mut self,
        parent_tid: usize,
        entry_point: u64,
        user_stack: u64,
        arg: u64,
    ) -> Result<usize, pmm::FrameError> {
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

        let k_frame = pmm::allocate_frames(256).ok_or(pmm::FrameError::NoMemory)?;
        thread.kernel_stack = k_frame + 1024 * 1024 + paging::HHDM_OFFSET;

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
        self.push_new_task(tid);

        Ok(tid)
    }

    pub fn get_tasks(&self) -> alloc::collections::btree_map::Values<usize, Box<Thread>> {
        self.tasks.values()
    }

    pub fn current_thread(&self) -> &Thread {
        let idx = crate::task::cpu::get_current_task_idx() as usize;
        self.tasks.get(&idx).expect("No current thread")
    }

    pub fn current_thread_mut(&mut self) -> &mut Thread {
        let idx = crate::task::cpu::get_current_task_idx() as usize;
        self.tasks.get_mut(&idx).expect("No current thread")
    }
}

pub fn kill_process(pid: u64) {
    {
        let mut composer = crate::window_manager::composer::COMPOSER.write();
        let mut to_remove = alloc::vec::Vec::new();
        for ws in 0..5 {
            for w in composer.workspaces[ws].windows.iter() {
                if w.pid == pid {
                    to_remove.push(w.id);
                }
            }
        }
        if composer.wallpaper.pid == pid {
            to_remove.push(composer.wallpaper.id);
        }
        for wid in to_remove {
            composer.remove_window(wid);
        }
    }

    {
        let mut tm = TASK_MANAGER.lock();
        for (tid, thread) in tm.tasks.iter_mut() {
            if let Some(proc) = &thread.process {
                if proc.pid == pid {
                    thread.state = ThreadState::Zombie;
                    *proc.event_queue.lock() = (0, 0, 0);
                }
            }
        }
    }
}
