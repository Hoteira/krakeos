pub mod manager;
pub mod process;
pub mod thread;
pub mod scheduler;
pub mod event_manager;

pub use manager::{TASK_MANAGER, TaskManager, MAX_THREADS, MAX_THREADS as MAX_TASKS};
pub use process::Process;
pub use thread::{Thread, ThreadState, CPUState};
pub use scheduler::{timer_handler, yield_handler, SYSTEM_TICKS};

pub fn init() {
    let mut tm = TASK_MANAGER.lock();
    tm.init();
}
