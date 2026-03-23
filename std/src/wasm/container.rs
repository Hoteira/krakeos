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
    pub slot_id: u16,
    pub parent_id: Option<u64>,
    pub linear_memory_base: u64,
    pub linear_memory_size: u64,
    pub linear_memory_max: u64,
    pub code_base: u64,
    pub stack_base: u64,
    pub shm_mappings: Vec<(u64, u64, String)>,
    pub return_value: Option<i32>,
}

pub static CONTAINER_REGISTRY: Mutex<BTreeMap<u64, Arc<Mutex<WasmContainer>>>> =
    Mutex::new(BTreeMap::new());
static NEXT_CONTAINER_ID: AtomicU64 = AtomicU64::new(1);

pub fn register_container(
    id: u64,
    slot_id: u16,
    parent_id: Option<u64>,
    linear_memory_base: u64,
    linear_memory_size: u64,
    linear_memory_max: u64,
    code_base: u64,
    stack_base: u64,
) {
    let container = Arc::new(Mutex::new(WasmContainer {
        id,
        slot_id,
        parent_id,
        linear_memory_base,
        linear_memory_size,
        linear_memory_max,
        code_base,
        stack_base,
        shm_mappings: Vec::new(),
        return_value: None,
    }));

    CONTAINER_REGISTRY.lock().insert(id, container);
}

pub fn unregister_container(id: u64, return_value: i32) {
    let mut registry = CONTAINER_REGISTRY.lock();
    if let Some(container) = registry.get(&id) {
        container.lock().return_value = Some(return_value);
    }
}

pub fn plant<T: Config + Clone + Send + 'static>(
    parent_store: &Store<'_, T>,
    wasm_bytes: &[u8],
    offset_in_parent: u32,
    size_bytes: u32,
    fds_map: Option<&[(u8, u8)]>,
) -> Result<u64, String> {
    // 1. Validate parent memory bounds
    let parent_mem_addr = parent_store
        .memories
        .get(0)
        .mem
        .get_base_ptr() as u64;
    let parent_mem_len = parent_store.memories.get(0).mem.len() as u64;

    if (offset_in_parent as u64 + size_bytes as u64) > parent_mem_len {
        return Err(String::from("Plant error: Child region exceeds parent memory"));
    }

    let child_mem_base = parent_mem_addr + offset_in_parent as u64;

    // 2. Create child container entry
    let parent_id = parent_store.container_id;
    let child_id = NEXT_CONTAINER_ID.fetch_add(1, Ordering::SeqCst);
    
    // For nested containers, they inherit the slot_id from the parent
    let slot_id = {
        let registry = CONTAINER_REGISTRY.lock();
        if let Some(parent) = parent_id.and_then(|id| registry.get(&id)) {
            parent.lock().slot_id
        } else {
            0 // Should not happen for nested
        }
    };

    register_container(
        child_id,
        slot_id,
        parent_id,
        child_mem_base,
        size_bytes as u64,
        size_bytes as u64,
        0, // Inherited code_base
        0, // Inherited stack_base
    );

    // 3. Prepare child Store
    let mut child_store = Store::new(parent_store.user_data.clone());
    child_store.sas_memory_base = Some(child_mem_base);
    child_store.container_id = Some(child_id);

    let mut wasi_ctx = crate::wasm::wasi::ctx::WasiCtx::default();
    if let (Some(parent_wasi), Some(map)) = (&parent_store.wasi_ctx, fds_map) {
        for &(guest_fd, host_fd) in map {
            if let Some(resource) = parent_wasi.resource_table.get(&(host_fd as i32)) {
                let resource: &crate::wasm::wasi::ctx::WasiResource = resource;
                wasi_ctx.resource_table.insert(guest_fd as i32, resource.clone());
            }
        }
    }
    child_store.wasi_ctx = Some(wasi_ctx);

    let mut linker = crate::wasm::common::interop::Linker::new();
    create_wasi_imports(&mut linker, &mut child_store);
    create_wasi_p2_imports(&mut linker, &mut child_store);

    let wasm_bytes_vec = wasm_bytes.to_vec();
    
    crate::sys::spawn_thread(move || {
        let mut child_store = child_store;
        let mut linker = linker;
        let res = crate::wasm::runner::run_in_container(
            "child_container",
            &wasm_bytes_vec,
            &mut linker,
            &mut child_store,
        );
        let exit_code = match res {
            crate::wasm::runner::WasmRunResult::Finished(code) => code,
            crate::wasm::runner::WasmRunResult::AotReady(_) => 0, // Should not happen for nested yet
        };
        unregister_container(child_id, exit_code);
    });

    Ok(child_id)
}

pub fn harvest(child_id: u64) -> Option<i32> {
    let registry = CONTAINER_REGISTRY.lock();
    registry.get(&child_id).and_then(|c| c.lock().return_value)
}

pub fn list_children(parent_id: Option<u64>) -> Vec<u64> {
    let registry = CONTAINER_REGISTRY.lock();
    registry
        .values()
        .filter(|c| c.lock().parent_id == parent_id)
        .map(|c| c.lock().id)
        .collect()
}

pub fn kill_child(child_id: u64) -> Result<(), String> {
    let mut registry = CONTAINER_REGISTRY.lock();
    if registry.contains_key(&child_id) {
        registry.remove(&child_id);
        Ok(())
    } else {
        Err(String::from("Child container not found"))
    }
}
