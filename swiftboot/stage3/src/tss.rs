#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct TaskStateSegment {
    pub reserved1: u32,
    pub rsp0: u64,
    pub rsp1: u64,
    pub rsp2: u64,
    pub reserved2: u64,
    pub ist1: u64,
    pub ist2: u64,
    pub ist3: u64,
    pub ist4: u64,
    pub ist5: u64,
    pub ist6: u64,
    pub ist7: u64,
    pub reserved3: u64,
    pub reserved4: u16,
    pub iopb_offset: u16,
}

pub static mut BASE_TSS: TaskStateSegment = TaskStateSegment {
    reserved1: 0,
    rsp0: 0x30_0000, 
    rsp1: 0,
    rsp2: 0,
    reserved2: 0,
    ist1: 0,
    ist2: 0,
    ist3: 0,
    ist4: 0,
    ist5: 0,
    ist6: 0,
    ist7: 0,
    reserved3: 0,
    reserved4: 0,
    iopb_offset: 0, 
};