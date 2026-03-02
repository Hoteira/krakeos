use crate::alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};
use crate::wasm::common::rw_spinlock::RwSpinLock;
use crate::wasm::common::slotmap::{SlotMap, SlotMapKey};
use crate::wasm::interpreter::store::addrs::FuncAddr;
use crate::wasm::common::value::Value;
use crate::wasm::common::value_stack::Stack;
use core::num::NonZeroU32;

#[derive(Debug)]
pub struct Resumable {
    pub stack: Stack,
    pub pc: usize,
    pub stp: usize,
    pub current_func_addr: FuncAddr,
    pub maybe_fuel: Option<u32>,
}

#[derive(Default)]
pub struct Dormitory(pub Arc<RwSpinLock<SlotMap<Resumable>>>);

impl Dormitory {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&self, resumable: Resumable) -> InvokedResumableRef {
        let key = self.0.write().insert(resumable);
        InvokedResumableRef {
            dormitory: Arc::downgrade(&self.0),
            key,
        }
    }
}

#[derive(Debug)]
pub struct InvokedResumableRef {
    pub dormitory: Weak<RwSpinLock<SlotMap<Resumable>>>,
    pub key: SlotMapKey<Resumable>,
}

#[derive(Debug)]
pub struct FreshResumableRef {
    pub func_addr: FuncAddr,
    pub params: Vec<Value>,
    pub maybe_fuel: Option<u32>,
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
        let _ = dormitory.write().remove(&self.key);
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
