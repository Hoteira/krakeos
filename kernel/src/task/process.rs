use crate::sync::Mutex;
use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::string::String;
use alloc::collections::VecDeque;

#[derive(Debug)]
pub struct Process {
    pub pid: u64,
    pub uid: u16,
    pub gid: u16,
    pub slot_id: u16,
    pub parent_pid: Option<u64>,
    pub children: Mutex<Vec<u64>>,
    pub fd_table: Mutex<Vec<i16>>,
    pub socket_table: Mutex<Vec<Option<usize>>>,
    pub fd_nonblock: Mutex<Vec<bool>>,
    pub cwd: Mutex<[u8; 128]>,
    pub terminal_width: Mutex<u16>,
    pub terminal_height: Mutex<u16>,
    pub linear_memory_base: u64,
    pub linear_memory_size: Mutex<usize>,
    pub code_base: u64,
    pub stack_base: u64,
    pub heap_start: u64,
    pub heap_limit: u64,
    pub heap_end: Mutex<u64>,
    pub event_queue: Mutex<(u64, u64, u32)>,
    pub args: Mutex<Vec<String>>,
    pub env_vars: Mutex<Vec<(String, String)>>,
    pub stdin_buffer: Mutex<VecDeque<u32>>,
    }

    impl Process {
    pub fn new(pid: u64, uid: u16, gid: u16, parent_pid: Option<u64>) -> Arc<Self> {
        let mut cwd = [0; 128];
        let root = b"/";
        cwd[..root.len()].copy_from_slice(root);

        let slot_id = crate::memory::address_space::allocate_slot().expect("SAS: Out of process slots!");
        let linear_memory_base = crate::memory::address_space::allocate_linear_memory(pid, slot_id);
        let code_base = crate::memory::address_space::allocate_code(pid, slot_id);
        let stack_top = crate::memory::address_space::allocate_stack(pid, slot_id);

        let heap_start = linear_memory_base + 4 * 1024 * 1024 * 1024;
        let heap_limit = linear_memory_base + crate::memory::address_space::LINEAR_MEMORY_SLOT_SIZE - 4096;

        //crate::debugln!("Process::new: allocating Arc<Self>...");
        let arc = Arc::new(Self {
            pid,
            uid,
            gid,
            slot_id,
            parent_pid,
            children: Mutex::new(Vec::new()),
            fd_table: Mutex::new(alloc::vec![-1; 16]),
            socket_table: Mutex::new(alloc::vec![None; 16]),
            fd_nonblock: Mutex::new(alloc::vec![false; 16]),
            cwd: Mutex::new(cwd),
            terminal_width: Mutex::new(80),
            terminal_height: Mutex::new(25),
            linear_memory_base,
            linear_memory_size: Mutex::new(0),
            code_base,
            stack_base: stack_top,
            heap_start,
            heap_limit,
            heap_end: Mutex::new(heap_start),
            event_queue: Mutex::new((0, 0, 0)),
            args: Mutex::new(Vec::new()),
            env_vars: Mutex::new(Vec::new()),
            stdin_buffer: Mutex::new(VecDeque::new()),
        });

        //crate::debugln!("Process::new: allocated successfully.");
        arc
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if self.pid != 0 {
            crate::memory::address_space::free_slot(self.slot_id);
        }
    }
}
