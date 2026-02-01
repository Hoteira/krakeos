use crate::rust_alloc::string::String;
use crate::rust_alloc::vec::Vec;

#[derive(Debug)]
pub struct ComponentInst {
    pub component_idx: u32,
    pub exports: Vec<ComponentExportInst>,
}

#[derive(Debug)]
pub struct ComponentExportInst {
    pub name: String,
    pub kind: u8,
    pub idx: usize,
}
