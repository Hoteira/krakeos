use crate::sync::Mutex;
use alloc::sync::Arc;
use alloc::vec::Vec;

#[derive(Debug)]
pub struct Process {
    pub pid: u64,
    pub uid: u16,
    pub gid: u16,
    pub slot_id: u16,
    pub parent_pid: Option<u64>,
    pub children: Mutex<Vec<u64>>,
    pub fd_table: Mutex<[i16; 16]>,
    pub socket_table: Mutex<[Option<usize>; 16]>,
    pub fd_nonblock: Mutex<[bool; 16]>,
    pub cwd: Mutex<[u8; 128]>,
    pub terminal_width: Mutex<u16>,
    pub terminal_height: Mutex<u16>,
    pub linear_memory_base: u64,
    pub code_base: u64,
    pub stack_base: u64,
    pub heap_start: u64,
    pub heap_limit: u64,
    pub heap_end: Mutex<u64>,
    pub event_queue: Mutex<(u64, u64, u32)>,
}

impl Process {
    pub fn new(pid: u64, uid: u16, gid: u16, parent_pid: Option<u64>) -> Arc<Self> {
        let mut cwd = [0; 128];
        let root = b"@0xE0/";
        cwd[..root.len()].copy_from_slice(root);

        let slot_id = crate::memory::address_space::allocate_slot().expect("SAS: Out of process slots!");
        let linear_memory_base = crate::memory::address_space::allocate_linear_memory(pid, slot_id);
        let code_base = crate::memory::address_space::allocate_code(pid, slot_id);
        let stack_top = crate::memory::address_space::allocate_stack(pid, slot_id);
        
        let heap_start = linear_memory_base;
        let heap_limit = heap_start + crate::memory::address_space::LINEAR_MEMORY_SLOT_SIZE - 4096;

        crate::debugln!("Process::new: allocating Arc<Self>...");
        let arc = Arc::new(Self {
            pid,
            uid,
            gid,
            slot_id,
            parent_pid,
            children: Mutex::new(Vec::new()),
            fd_table: Mutex::new([-1; 16]),
            socket_table: Mutex::new([None; 16]),
            fd_nonblock: Mutex::new([false; 16]),
            cwd: Mutex::new(cwd),
            terminal_width: Mutex::new(80),
            terminal_height: Mutex::new(25),
            linear_memory_base,
            code_base,
            stack_base: stack_top,
            heap_start,
            heap_limit,
            heap_end: Mutex::new(heap_start),
            event_queue: Mutex::new((0, 0, 0)),
        });

        crate::debugln!("Process::new: allocated successfully.");
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
