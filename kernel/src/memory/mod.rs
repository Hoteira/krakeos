pub mod pmm;
pub mod vmm;
pub mod paging;

pub fn init() {
    crate::debugln!("[MEMORY] Init...");
    pmm::init();
    vmm::init();
    crate::debugln!("[MEMORY] Init Done.");
}
