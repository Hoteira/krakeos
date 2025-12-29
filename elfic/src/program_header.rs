#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Phdr {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ProgramType {
    Null = 0,
    Load = 1,
    Dynamic = 2,
    Interp = 3,
    Note = 4,
    Shlib = 5,
    Phdr = 6,
    Tls = 7,
}

impl From<u32> for ProgramType {
    fn from(val: u32) -> Self {
        match val {
            1 => ProgramType::Load,
            2 => ProgramType::Dynamic,
            3 => ProgramType::Interp,
            4 => ProgramType::Note,
            5 => ProgramType::Shlib,
            6 => ProgramType::Phdr,
            7 => ProgramType::Tls,
            _ => ProgramType::Null,
        }
    }
}

pub struct ProgramFlags;

impl ProgramFlags {
    pub const EXECUTE: u32 = 1;
    pub const WRITE: u32 = 2;
    pub const READ: u32 = 4;
}
