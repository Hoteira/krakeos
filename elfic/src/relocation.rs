#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Rela {
    pub r_offset: u64,
    pub r_info: u64,
    pub r_addend: i64,
}

impl Elf64Rela {
    pub fn get_type(&self) -> u32 {
        (self.r_info & 0xFFFFFFFF) as u32
    }

    pub fn get_symbol(&self) -> u32 {
        (self.r_info >> 32) as u32
    }
}
