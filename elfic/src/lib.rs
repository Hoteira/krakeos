#![no_std]

pub mod header;
pub mod program_header;
pub mod section_header;
pub mod relocation;
pub mod symbol;

pub use header::Elf64Ehdr;
pub use program_header::{Elf64Phdr, ProgramType, ProgramFlags};
pub use section_header::Elf64Shdr;
pub use relocation::Elf64Rela;
pub use symbol::Elf64Sym;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError {
    BufferTooSmall,
    InvalidMagic,
    InvalidClass,
    InvalidData,
    InvalidVersion,
    InvalidMachine,
}

pub struct Elf64<'a> {
    pub data: &'a [u8],
    pub header: &'a Elf64Ehdr,
}

impl<'a> Elf64<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self, ElfError> {
        if data.len() < core::mem::size_of::<Elf64Ehdr>() {
            return Err(ElfError::BufferTooSmall);
        }

        let header = unsafe { &*(data.as_ptr() as *const Elf64Ehdr) };

        
        if header.e_ident[0] != 0x7f 
            || header.e_ident[1] != b'E' 
            || header.e_ident[2] != b'L' 
            || header.e_ident[3] != b'F' 
        {
            return Err(ElfError::InvalidMagic);
        }

        
        if header.e_ident[4] != 2 {
            return Err(ElfError::InvalidClass);
        }

        
        if header.e_ident[5] != 1 {
            return Err(ElfError::InvalidData);
        }

        
        if header.e_ident[6] != 1 {
            return Err(ElfError::InvalidVersion);
        }

        
        if header.e_machine != 0x3E {
            return Err(ElfError::InvalidMachine);
        }

        Ok(Self { data, header })
    }

    pub fn program_headers(&self) -> &'a [Elf64Phdr] {
        if self.header.e_phoff == 0 || self.header.e_phnum == 0 {
            return &[];
        }

        let offset = self.header.e_phoff as usize;
        let num = self.header.e_phnum as usize;
        let size = self.header.e_phentsize as usize;

        if offset + (num * size) > self.data.len() {
            return &[]; 
        }

        unsafe {
            core::slice::from_raw_parts(
                self.data.as_ptr().add(offset) as *const Elf64Phdr,
                num
            )
        }
    }

    pub fn section_headers(&self) -> &'a [Elf64Shdr] {
        if self.header.e_shoff == 0 || self.header.e_shnum == 0 {
            return &[];
        }

        let offset = self.header.e_shoff as usize;
        let num = self.header.e_shnum as usize;
        let size = self.header.e_shentsize as usize;

        if offset + (num * size) > self.data.len() {
            return &[];
        }

        unsafe {
            core::slice::from_raw_parts(
                self.data.as_ptr().add(offset) as *const Elf64Shdr,
                num
            )
        }
    }
}
