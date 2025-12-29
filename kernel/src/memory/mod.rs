pub mod pmm;
pub mod vmm;
pub mod paging;

pub fn init() {
    pmm::init();
    vmm::init();
}
