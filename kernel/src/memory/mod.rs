pub mod pmm;
pub mod vmm;
pub mod paging;
pub mod address;

pub fn init() {
    pmm::init();
    vmm::init();
}
