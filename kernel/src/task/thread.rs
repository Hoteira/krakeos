use crate::task::process::Process;
use alloc::sync::Arc;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u64)]
pub enum ThreadState {
    Null,
    Ready,
    Zombie,
    Sleeping,
    Blocked,
    Reserved,
    WaitingForEvent,
}

#[repr(C)]
pub struct Thread {
    pub fpu_state: [u8; 528],
    pub kernel_stack: u64,
    pub user_stack: u64,
    pub cpu_state_ptr: u64,
    pub state: ThreadState,
    pub wake_ticks: u64,
    pub exit_code: u64,
    pub name: [u8; 32],
    pub uid: u32,
    pub gid: u32,
    pub is_queued: bool,
    pub process: Option<Arc<Process>>,
    /// If Some(cpu_id), this thread must only run on that CPU.
    pub pinned_cpu: Option<usize>,
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct CPUState {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    pub rbp: u64,

    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl Thread {
    pub fn new(name: &[u8]) -> Self {
        let mut t_name = [0; 32];
        let len = core::cmp::min(name.len(), 32);
        t_name[..len].copy_from_slice(&name[..len]);

        let mut fpu_state = [0u8; 528];
        // Initialize x87 FCW at offset 0 to 0x037F (all x87 exceptions masked, double precision)
        fpu_state[0] = 0x7F;
        fpu_state[1] = 0x03;
        // Initialize MXCSR at offset 24 to 0x1F80 (all SSE exceptions masked)
        fpu_state[24] = 0x80;
        fpu_state[25] = 0x1F;

        Self {
            fpu_state,
            kernel_stack: 0,
            user_stack: 0,
            cpu_state_ptr: 0,
            state: ThreadState::Null,
            wake_ticks: 0,
            exit_code: 0,
            name: t_name,
            uid: 0,
            gid: 0,
            is_queued: false,
            process: None,
            pinned_cpu: None,
        }
    }
}
