use crate::rust_alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};
use crate::wasm::{
    core::rw_spinlock::RwSpinLock,
    core::slotmap::{SlotMap, SlotMapKey},
    execution::store::addrs::FuncAddr,
    execution::value::Value,
    execution::value_stack::Stack,
};
use core::num::NonZeroU32;
#[derive(Debug)]
pub(crate) struct Resumable {
    pub(crate) stack: Stack,
    pub(crate) pc: usize,
    pub(crate) stp: usize,
    pub(crate) current_func_addr: FuncAddr,
    pub(crate) maybe_fuel: Option<u32>,
}
#[derive(Default)]
pub(crate) struct Dormitory(pub(crate) Arc<RwSpinLock<SlotMap<Resumable>>>);
impl Dormitory {
    #[allow(unused)]
    pub(crate) fn new() -> Self {
        Self::default()
    }
    pub(crate) fn insert(&self, resumable: Resumable) -> InvokedResumableRef {
        let key = self.0.write().insert(resumable);
        InvokedResumableRef {
            dormitory: Arc::downgrade(&self.0),
            key,
        }
    }
}
#[derive(Debug)]
pub struct InvokedResumableRef {
    pub(crate) dormitory: Weak<RwSpinLock<SlotMap<Resumable>>>,
    pub(crate) key: SlotMapKey<Resumable>,
}
#[derive(Debug)]
pub struct FreshResumableRef {
    pub(crate) func_addr: FuncAddr,
    pub(crate) params: Vec<Value>,
    pub(crate) maybe_fuel: Option<u32>,
}
#[derive(Debug)]
pub enum ResumableRef {
    Fresh(FreshResumableRef),
    Invoked(InvokedResumableRef),
}
impl Drop for InvokedResumableRef {
    fn drop(&mut self) {
        let Some(dormitory) = self.dormitory.upgrade() else {
            return;
        };
        dormitory.write().remove(&self.key)
            .expect("that the resumable could not have been removed already, because then this self could not exist or the dormitory weak pointer would have been None");
    }
}
pub enum RunState {
    Finished {
        values: Vec<Value>,
        maybe_remaining_fuel: Option<u32>,
    },
    Resumable {
        resumable_ref: ResumableRef,
        required_fuel: NonZeroU32,
    },
}
#[cfg(test)]
mod test {
    use super::{Dormitory, Resumable};
    use crate::wasm::execution::store::addrs::FuncAddr;
    use crate::wasm::execution::value_stack::Stack;
    #[test]
    fn dormitory_constructor() {
        let dorm = Dormitory::new();
        let resumable = Resumable {
            stack: Stack::new(),
            pc: 11,
            stp: 13,
            current_func_addr: FuncAddr::INVALID,
            maybe_fuel: None,
        };
        dorm.insert(resumable);
    }
}
