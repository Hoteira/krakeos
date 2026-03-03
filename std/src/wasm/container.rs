use crate::alloc::collections::BTreeMap;
use crate::alloc::string::String;
use crate::alloc::sync::Arc;
use crate::alloc::vec::Vec;
use crate::sync::Mutex;
use crate::wasm::common::config::Config;
use crate::wasm::interpreter::store::Store;
use crate::wasm::wasi::{create_wasi_imports, create_wasi_p2_imports};
use core::sync::atomic::{AtomicU64, Ordering};

pub struct WasmContainer {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub memory_base: u64, // VA in SAS where linear memory starts
    pub memory_size: u64, // current allocated size
    pub memory_max: u64,  // maximum allowed
    pub return_value: Option<i32>,
}

pub static CONTAINER_REGISTRY: Mutex<BTreeMap<u64, Arc<Mutex<WasmContainer>>>> =
    Mutex::new(BTreeMap::new());
static NEXT_CONTAINER_ID: AtomicU64 = AtomicU64::new(1);

pub fn register_container(
    parent_id: Option<u64>,
    memory_base: u64,
    memory_size: u64,
    memory_max: u64,
) -> u64 {
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
}

/// Plants a child WASM container within a sub-region of the parent's linear memory.
pub fn plant<T: Config + Clone>(
    parent_store: &Store<'_, T>,
    wasm_bytes: &[u8],
    offset_in_parent: u32,
    size_bytes: u32,
) -> Result<u64, String> {
    // 1. Validate parent memory bounds
    let parent_mem_addr = parent_store
        .memories
        .get(0) // WASM32 only supports 1 memory
        .mem
        .get_base_ptr() as u64;
    let parent_mem_len = parent_store.memories.get(0).mem.len() as u64;

    if (offset_in_parent as u64 + size_bytes as u64) > parent_mem_len {
        return Err(String::from("Plant error: Child region exceeds parent memory"));
    }

    let child_mem_base = parent_mem_addr + offset_in_parent as u64;

    // 2. Create child container entry
    let parent_id = None; // TODO: Track current container ID in thread-local
    let child_id = register_container(
        parent_id,
        child_mem_base,
        size_bytes as u64,
        size_bytes as u64,
    );

    // 3. Prepare child Store with the memory view
    let mut child_store = Store::new(parent_store.user_data.clone());
    child_store.sas_memory_base = Some(child_mem_base);

    let mut linker = crate::wasm::common::interop::Linker::new();
    create_wasi_imports(&mut linker, &mut child_store);
    create_wasi_p2_imports(&mut linker, &mut child_store);

    // 4. Load and run (synchronous for now as per Step 10 Rust API)
    let res = crate::wasm::runner::run_in_container(
        "child_container",
        wasm_bytes,
        &mut linker,
        &mut child_store,
    );

    unregister_container(child_id, res);
    Ok(child_id)
}

/// Retrieves the return value of a child container.
pub fn harvest(child_id: u64) -> Option<i32> {
    let registry = CONTAINER_REGISTRY.lock();
    registry.get(&child_id).and_then(|c| c.lock().return_value)
}
