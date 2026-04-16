use crate::sync::Mutex;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use crate::task::{TASK_MANAGER, ThreadState};
use std::wasm::runner::{WasmRunResult, run_with_buffer};
use core::sync::atomic::Ordering;

pub struct AotRequest {
    pub pid: u64,
    pub name: String,
    pub buffer: Vec<u8>,
    pub args: Vec<String>,
    pub cwd: String,
    pub slot_id: u16,
    pub debug: bool,
}

static AOT_QUEUE: Mutex<VecDeque<AotRequest>> = Mutex::new(VecDeque::new());
static AOT_SEMAPHORE: std::sync::Semaphore = std::sync::Semaphore::new(0);

static PENDING_SPAWN: Mutex<VecDeque<(String, bool)>> = Mutex::new(VecDeque::new());

pub fn request_spawn(path: &str, debug: bool) {
    let mut q = PENDING_SPAWN.lock();
    q.push_back((String::from(path), debug));
    drop(q);
    AOT_SEMAPHORE.signal();
}

extern "C" fn aot_thread_main() {
    crate::spawn_debugln!("[AOTWorker] aot_thread_main entry");
    crate::spawn_debugln!("[AOTWorker] entering outer loop");
    loop {
        crate::spawn_debugln!("[AOTWorker] before AOT_SEMAPHORE.wait()");
        AOT_SEMAPHORE.wait();
        crate::spawn_debugln!("[AOTWorker] after AOT_SEMAPHORE.wait()");
        crate::spawn_debugln!("[AOTWorker] entering inner loop");
        loop {
            crate::spawn_debugln!("[AOTWorker] before PENDING_SPAWN.lock()");
            let mut q_lock = PENDING_SPAWN.lock();
            crate::spawn_debugln!("[AOTWorker] after PENDING_SPAWN.lock()");
            crate::spawn_debugln!("[AOTWorker] before q_lock.pop_front()");
            let item = q_lock.pop_front();
            crate::spawn_debugln!("[AOTWorker] after q_lock.pop_front()");
            crate::spawn_debugln!("[AOTWorker] before drop(q_lock)");
            drop(q_lock);
            crate::spawn_debugln!("[AOTWorker] after drop(q_lock)");
            match item {
                Some((p, debug)) => {
                    crate::spawn_debugln!("[AOTWorker] item found: {}", p);
                    if debug {
                        crate::spawn_debugln!("[AOTWorker] before SPAWN_DEBUG.store(true)");
                        crate::debug::SPAWN_DEBUG.store(true, Ordering::SeqCst);
                        crate::spawn_debugln!("[AOTWorker] after SPAWN_DEBUG.store(true)");
                    }
                    crate::spawn_debugln!("[AOTWorker] [AOTWorker] Deferred spawn: {}", p);
                    crate::spawn_debugln!("[AOTWorker] before spawn_process");
                    let _ = crate::syscalls::process::spawn_process(&p, None, None, None, debug);
                    crate::spawn_debugln!("[AOTWorker] after spawn_process");
                    if debug {
                        crate::spawn_debugln!("[AOTWorker] before SPAWN_DEBUG.store(false)");
                        crate::debug::SPAWN_DEBUG.store(false, Ordering::SeqCst);
                        crate::spawn_debugln!("[AOTWorker] after SPAWN_DEBUG.store(false)");
                    }
                    crate::spawn_debugln!("[AOTWorker] item processed");
                }
                None => {
                    crate::spawn_debugln!("[AOTWorker] no more items, breaking inner loop");
                    break;
                }
            }
            crate::spawn_debugln!("[AOTWorker] inner loop iteration end");
        }
        crate::spawn_debugln!("[AOTWorker] exited inner loop");
        crate::spawn_debugln!("[AOTWorker] before AOT_QUEUE.lock()");
        let mut aot_lock = AOT_QUEUE.lock();
        crate::spawn_debugln!("[AOTWorker] after AOT_QUEUE.lock()");
        crate::spawn_debugln!("[AOTWorker] before aot_lock.pop_front()");
        let req_opt = aot_lock.pop_front();
        crate::spawn_debugln!("[AOTWorker] after aot_lock.pop_front()");
        crate::spawn_debugln!("[AOTWorker] before drop(aot_lock)");
        drop(aot_lock);
        crate::spawn_debugln!("[AOTWorker] after drop(aot_lock)");
        if let Some(req) = req_opt {
            crate::spawn_debugln!("[AOTWorker] request found for PID {}", req.pid);
            if req.debug {
                crate::spawn_debugln!("[AOTWorker] before SPAWN_DEBUG.store(true)");
                crate::debug::SPAWN_DEBUG.store(true, Ordering::SeqCst);
                crate::spawn_debugln!("[AOTWorker] after SPAWN_DEBUG.store(true)");
            }
            crate::spawn_debugln!("[AOTWorker] [AOTWorker] Pulled request for PID {}", req.pid);
            crate::spawn_debugln!("[AOTWorker] before process_aot_request");
            process_aot_request(req);
            crate::spawn_debugln!("[AOTWorker] after process_aot_request");
            crate::spawn_debugln!("[AOTWorker] before SPAWN_DEBUG.store(false)");
            crate::debug::SPAWN_DEBUG.store(false, Ordering::SeqCst);
            crate::spawn_debugln!("[AOTWorker] after SPAWN_DEBUG.store(false)");
        }
        crate::spawn_debugln!("[AOTWorker] outer loop iteration end");
    }
}

pub fn init() {
    let mut tm = TASK_MANAGER.lock();
    let _ = tm.spawn_thread(0, aot_thread_main as u64, 0, 0);
}

pub fn submit_request(req: AotRequest) {
    crate::spawn_debugln!("[AOTWorker] submit_request entry for PID {}", req.pid);
    crate::spawn_debugln!("[AOTWorker] before AOT_QUEUE.lock()");
    let mut q = AOT_QUEUE.lock();
    crate::spawn_debugln!("[AOTWorker] after AOT_QUEUE.lock()");
    crate::spawn_debugln!("[AOTWorker] before q.push_back(req)");
    q.push_back(req);
    crate::spawn_debugln!("[AOTWorker] after q.push_back(req)");
    crate::spawn_debugln!("[AOTWorker] before drop(q)");
    drop(q);
    crate::spawn_debugln!("[AOTWorker] after drop(q)");
    crate::spawn_debugln!("[AOTWorker] before AOT_SEMAPHORE.signal()");
    AOT_SEMAPHORE.signal();
    crate::spawn_debugln!("[AOTWorker] after AOT_SEMAPHORE.signal()");
    crate::spawn_debugln!("[AOTWorker] submit_request exit");
}

fn process_aot_request(req: AotRequest) {
    crate::spawn_debugln!("[AOTWorker] process_aot_request entry: pid={}", req.pid);
    crate::spawn_debugln!("[AOTWorker] before run_with_buffer");
    let res = run_with_buffer(
        &req.name,
        &req.buffer,
        req.args,
        &req.cwd,
        &[],
        Vec::new(),
        true,
        req.pid,
        req.slot_id,
    );
    crate::spawn_debugln!("[AOTWorker] after run_with_buffer");
    match res {
        WasmRunResult::AotReady(info) => {
            crate::spawn_debugln!("[AOTWorker] [AOTWorker] Aot ready for PID {}. Entry={:#x}", req.pid, info.entry_addr);
            crate::spawn_debugln!("[AOTWorker] before TASK_MANAGER.lock()");
            let mut tm = TASK_MANAGER.lock();
            crate::spawn_debugln!("[AOTWorker] after TASK_MANAGER.lock()");
            let pid_idx = req.pid as usize;
            crate::spawn_debugln!("[AOTWorker] before tm.tasks.get_mut(&pid_idx)");
            let task_opt = tm.tasks.get_mut(&pid_idx);
            crate::spawn_debugln!("[AOTWorker] after tm.tasks.get_mut(&pid_idx)");
            if let Some(task) = task_opt {
                crate::spawn_debugln!("[AOTWorker] task found for PID {}", pid_idx);
                unsafe {
                    crate::spawn_debugln!("[AOTWorker] entering unsafe block");
                    let state = &mut *(task.cpu_state_ptr as *mut crate::task::CPUState);
                    state.rip = info.entry_addr;
                    state.rdi = info.ctx_ptr;
                    state.cs = 0x23;
                    crate::spawn_debugln!("[AOTWorker] partial state updated");
                    state.ss = 0x1B;
                    state.rflags = 0x202;
                    crate::spawn_debugln!("[AOTWorker] state updated fully");
                    let ctx = &mut *(info.ctx_ptr as *mut std::wasm::aot::runtime::Ring3Context);
                    ctx.stack_base = info.stack_base as *mut u128;
                    ctx.stack_limit = info.stack_limit as usize;
                    ctx.module_addr = info.module_addr;
                    crate::spawn_debugln!("[AOTWorker] partial ctx updated");
                    ctx.locals_base = (info.stack_base - 8 * 1024 * 1024) as *mut u128;
                    crate::spawn_debugln!("[AOTWorker] ctx updated fully");
                    let jump_table = ctx.blob_base as *const u64;
                    crate::spawn_debugln!("[AOTWorker] before jump_table.add(1023)");
                    let exit_fn_addr_ptr = jump_table.add(1023);
                    crate::spawn_debugln!("[AOTWorker] after jump_table.add(1023)");
                    let exit_fn_addr = *exit_fn_addr_ptr;
                    let rsp = ((info.stack_base - 4096) & !15) - 8;
                    *(rsp as *mut u64) = exit_fn_addr;
                    state.rsp = rsp;
                    crate::spawn_debugln!("[AOTWorker] rsp and exit_fn_addr updated");
                    crate::spawn_debugln!("[AOTWorker] before task.process.as_ref()");
                    let proc_opt = task.process.as_ref();
                    crate::spawn_debugln!("[AOTWorker] after task.process.as_ref()");
                    crate::spawn_debugln!("[AOTWorker] before proc_opt.expect()");
                    let proc = proc_opt.expect("Thread has no process");
                    crate::spawn_debugln!("[AOTWorker] after proc_opt.expect()");
                    crate::spawn_debugln!("[AOTWorker] before proc.linear_memory_size.lock()");
                    let mut mem_lock = proc.linear_memory_size.lock();
                    crate::spawn_debugln!("[AOTWorker] after proc.linear_memory_size.lock()");
                    *mem_lock = ctx.memory_size;
                    crate::spawn_debugln!("[AOTWorker] linear_memory_size updated");
                    crate::spawn_debugln!("[AOTWorker] leaving unsafe block");
                }
                task.state = ThreadState::Ready;
                crate::spawn_debugln!("[AOTWorker] [AOTWorker] pushing PID {} to run queue now", pid_idx);
                crate::spawn_debugln!("[AOTWorker] before tm.push_to_run_queue(pid_idx)");
                tm.push_to_run_queue(pid_idx);
                crate::spawn_debugln!("[AOTWorker] after tm.push_to_run_queue(pid_idx)");
            } else {
                crate::spawn_debugln!("[AOTWorker] [AOTWorker] CRITICAL: PID {} not found in task list!!", req.pid);
            }
            crate::spawn_debugln!("[AOTWorker] before drop(tm)");
            drop(tm);
            crate::spawn_debugln!("[AOTWorker] after drop(tm)");
        }
        WasmRunResult::Finished(exit_code) => {
            crate::spawn_debugln!("[AOTWorker] [AOTWorker] WASM finished during AOT for PID {} with code {}", req.pid, exit_code);
            crate::spawn_debugln!("[AOTWorker] before kill_process");
            crate::task::manager::kill_process(req.pid);
            crate::spawn_debugln!("[AOTWorker] after kill_process");
        }
    }
    crate::spawn_debugln!("[AOTWorker] process_aot_request exit");
}
