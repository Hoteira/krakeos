#![allow(dead_code)]
use core::slice;

const FDT_MAGIC: u32 = 0xd00dfeed;
const FDT_BEGIN_NODE: u32 = 1;
const FDT_END_NODE: u32 = 2;
const FDT_PROP: u32 = 3;
const FDT_END: u32 = 9;

#[repr(C)]
struct FdtHeader {
    magic: u32,
    totalsize: u32,
    off_dt_struct: u32,
    off_dt_strings: u32,
    off_mem_rsvmap: u32,
    version: u32,
    last_comp_version: u32,
    boot_cpuid_phys: u32,
    size_dt_strings: u32,
    size_dt_struct: u32,
}

pub struct FdtParser<'a> {
    data: &'a [u8],
    dt_struct: &'a [u8],
    dt_strings: &'a [u8],
}

impl<'a> FdtParser<'a> {
    pub unsafe fn from_ptr(ptr: *const u8) -> Option<Self> {
        let header = &*(ptr as *const FdtHeader);
        if u32::from_be(header.magic) != FDT_MAGIC {
            return None;
        }

        let totalsize = u32::from_be(header.totalsize) as usize;
        let data = slice::from_raw_parts(ptr, totalsize);
        
        let off_dt_struct = u32::from_be(header.off_dt_struct) as usize;
        let size_dt_struct = u32::from_be(header.size_dt_struct) as usize;
        let dt_struct = &data[off_dt_struct..off_dt_struct + size_dt_struct];
        
        let off_dt_strings = u32::from_be(header.off_dt_strings) as usize;
        let size_dt_strings = u32::from_be(header.size_dt_strings) as usize;
        let dt_strings = &data[off_dt_strings..off_dt_strings + size_dt_strings];

        Some(Self {
            data,
            dt_struct,
            dt_strings,
        })
    }

    pub fn get_memory_region(&self) -> Option<(usize, usize)> {
        let mut offset = 0;
        let mut in_memory_node = false;
        
        while offset < self.dt_struct.len() {
            let tag = u32::from_be_bytes(self.dt_struct[offset..offset+4].try_into().unwrap());
            offset += 4;
            
            match tag {
                FDT_BEGIN_NODE => {
                    let mut end = offset;
                    while self.dt_struct[end] != 0 {
                        end += 1;
                    }
                    let name = core::str::from_utf8(&self.dt_struct[offset..end]).unwrap_or("");
                    offset = (end + 1 + 3) & !3; // align 4
                    
                    if name.starts_with("memory@") {
                        in_memory_node = true;
                    } else {
                        in_memory_node = false;
                    }
                }
                FDT_END_NODE => {
                    in_memory_node = false;
                }
                FDT_PROP => {
                    let len = u32::from_be_bytes(self.dt_struct[offset..offset+4].try_into().unwrap()) as usize;
                    let nameoff = u32::from_be_bytes(self.dt_struct[offset+4..offset+8].try_into().unwrap()) as usize;
                    offset += 8;
                    
                    if in_memory_node {
                        let name_end = self.dt_strings[nameoff..].iter().position(|&c| c == 0).unwrap_or(0);
                        let prop_name = core::str::from_utf8(&self.dt_strings[nameoff..nameoff+name_end]).unwrap_or("");
                        
                        if prop_name == "reg" && len >= 16 {
                            // Assuming #address-cells = 2 and #size-cells = 2 for RISC-V 64-bit
                            let addr = u64::from_be_bytes(self.dt_struct[offset..offset+8].try_into().unwrap());
                            let size = u64::from_be_bytes(self.dt_struct[offset+8..offset+16].try_into().unwrap());
                            return Some((addr as usize, size as usize));
                        }
                    }
                    offset = (offset + len + 3) & !3;
                }
                FDT_END => break,
                _ => {} // NOP
            }
        }
        None
    }
}
