use crate::rust_alloc::vec::Vec;
pub type Sidetable = Vec<SidetableEntry>;
#[derive(Debug, Clone)]
pub struct SidetableEntry {
    pub delta_pc: isize,
    pub delta_stp: isize,
    pub valcnt: usize,
    pub popcnt: usize,
}
