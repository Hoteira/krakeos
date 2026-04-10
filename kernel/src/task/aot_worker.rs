use crate::sync::Mutex;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use crate::task::{TASK_MANAGER, ThreadState};
use std::wasm::runner::{WasmRunResult, run_with_buffer};

pub struct AotRequest {
    pub pid: u64,
    pub name: String,
    pub buffer: Vec<u8>,
    pub args: Vec<String>,
    pub cwd: String,
    pub slot_id: u16,
}

static AOT_QUEUE: Mutex<VecDeque<AotRequest>> = Mutex::new(VecDeque::new());
static AOT_SEMAPHORE: std::sync::Semaphore = std::sync::Semaphore::new(0);

extern "C" fn aot_thread_main() {
    loop {
        AOT_SEMAPHORE.wait();
        let req_opt = AOT_QUEUE.lock().pop_front();
        if let Some(req) = req_opt {
            process_aot_request(req);
        }
    }
}

pub fn init() {
    let mut tm = TASK_MANAGER.lock();
    let _ = tm.spawn_thread(0, aot_thread_main as u64, 0, 0);
}

pub fn submit_request(req: AotRequest) {
    AOT_QUEUE.lock().push_back(req);
    AOT_SEMAPHORE.signal();
}

fn process_aot_request(req: AotRequest) {
    crate::debugln!("[AOTWorker] Processing request for PID {}", req.pid);
    
    let res = run_with_buffer(
        &req.name,
        &req.buffer,
        req.args,
        &req.cwd,
        &[],
        Vec::new(),
        true, // AOT
        req.pid,
        req.slot_id,
    );

    match res {
        WasmRunResult::AotReady(info) => {
            crate::debugln!("[AOTWorker] AOT ready for PID {}. Entry={:#x}", req.pid, info.entry_addr);
            let mut tm = TASK_MANAGER.lock();
            let pid_idx = req.pid as usize;
            if let Some(task) = tm.tasks.get_mut(&pid_idx) {
                unsafe {
                    let state = &mut *(task.cpu_state_ptr as *mut crate::task::CPUState);
                    state.rip = info.entry_addr;
                    state.rdi = info.ctx_ptr;
                    state.cs = 0x23;
                    state.ss = 0x1B;
                    state.rflags = 0x202;

                    let ctx = &mut *(info.ctx_ptr as *mut std::wasm::aot::runtime::Ring3Context);
                    ctx.stack_base = info.stack_base as *mut u128;
                    ctx.stack_limit = info.stack_limit as usize;
                    ctx.locals_base = (info.stack_base - 8 * 1024 * 1024) as *mut u128;

                    let jump_table = ctx.blob_base as *const u64;
                    let exit_fn_addr = *jump_table.add(1023);
                    let rsp = ((info.stack_base - 4096) & !15) - 8;
                    *(rsp as *mut u64) = exit_fn_addr;
                    state.rsp = rsp;

                    let proc = task.process.as_ref().expect("Thread has no process");
                    *proc.linear_memory_size.lock() = ctx.memory_size;
                }
                task.state = ThreadState::Ready;
                tm.push_to_run_queue(pid_idx);
            }
        }
        WasmRunResult::Finished(exit_code) => {
            crate::debugln!("[AOTWorker] WASM finished during AOT for PID {} with code {}", req.pid, exit_code);
            crate::task::manager::kill_process(req.pid);
        }
    }
}
