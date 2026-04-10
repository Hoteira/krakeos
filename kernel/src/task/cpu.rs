#[repr(C)]
pub struct CpuLocal {
    pub self_ptr: u64,          // Offset 0
    pub kernel_stack_ptr: u64,   // Offset 8
    pub scratch: u64,            // Offset 16
    pub current_task_idx: i64,   // Offset 24
    pub cpu_id: u32,
    pub lapic_id: u8,
}

pub static mut CPUS: [CpuLocal; 64] = [const { CpuLocal {
    self_ptr: 0,
    kernel_stack_ptr: 0,
    scratch: 0,
    current_task_idx: -1,
    cpu_id: 0,
    lapic_id: 0,
} }; 64];

pub fn init_per_cpu(cpu_id: u32, lapic_id: u8) {
    unsafe {
        let cpu = &mut CPUS[cpu_id as usize];
        cpu.cpu_id = cpu_id;
        cpu.lapic_id = lapic_id;
        cpu.self_ptr = cpu as *const _ as u64;
        
        let gs_base = cpu.self_ptr;
        crate::arch::x86_64::wrmsr(0xC0000101, gs_base); // GS_BASE
        crate::arch::x86_64::wrmsr(0xC0000102, gs_base); // KERNEL_GS_BASE
    }
}

pub fn get_cpu_id() -> u32 {
    let id: u32;
    unsafe {
        core::arch::asm!("mov {:e}, gs:[32]", out(reg) id);
    }
    id
}

pub fn get_current_task_idx() -> i64 {
    let idx: i64;
    unsafe {
        core::arch::asm!("mov {}, gs:[24]", out(reg) idx);
    }
    idx
}
