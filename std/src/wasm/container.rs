use crate::alloc::collections::BTreeMap;
use crate::alloc::sync::Arc;
use crate::sync::Mutex;
use core::sync::atomic::{AtomicU64, Ordering};

pub struct WasmContainer {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub memory_base: u64,     // VA in SAS where linear memory starts
    pub memory_size: u64,     // current allocated size
    pub memory_max: u64,      // maximum allowed
    pub return_value: Option<i32>,
}

pub static CONTAINER_REGISTRY: Mutex<BTreeMap<u64, Arc<Mutex<WasmContainer>>>> = Mutex::new(BTreeMap::new());
static NEXT_CONTAINER_ID: AtomicU64 = AtomicU64::new(1);

pub fn register_container(parent_id: Option<u64>, memory_base: u64, memory_size: u64, memory_max: u64) -> u64 {
    let id = NEXT_CONTAINER_ID.fetch_add(1, Ordering::SeqCst);
    let container = Arc::new(Mutex::new(WasmContainer {
        id,
        parent_id,
        memory_base,
        memory_size,
        memory_max,
        return_value: None,
    }));
    
    CONTAINER_REGISTRY.lock().insert(id, container);
    id
}

pub fn unregister_container(id: u64, return_value: i32) {
    let mut registry = CONTAINER_REGISTRY.lock();
    if let Some(container) = registry.get(&id) {
        container.lock().return_value = Some(return_value);
    }
    // We keep the entry in the registry for now so parents can "harvest" the return value
    // Cleanup will be handled in later steps.
}
