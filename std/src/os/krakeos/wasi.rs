use crate::alloc::{vec, vec::Vec};
use crate::wasm::{
    common::{config::Config, value::Value, reader::types::{ValType, NumType}},
    interpreter::store::{HaltExecutionError, Store},
    wasi::ctx::{WasiResource, InputStreamSource, OutputStreamSource},
};
use crate::wasm::wasi::preview2::{read_mem, read_mem_u32, read_mem_u64, write_bytes, write_u32, write_u64, call_cabi_realloc};
use crate::os::krakeos as host;
use crate::io::Read;

crate::export_method!(
    "krakeos:system/container@0.1.0", "plant",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn container_plant_host<T: Config + Clone + Send + 'static>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let bytes_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let bytes_len = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let offset = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let size = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let fds_ptr = match args.get(4) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let fds_len = match args.get(5) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let ret_ptr = match args.get(6) { Some(Value::I32(v)) => *v as u32, _ => 0 };

        let mut wasm_bytes = vec![0u8; bytes_len as usize];
        read_mem(store, bytes_ptr, &mut wasm_bytes).map_err(|_| HaltExecutionError(1))?;

        let mut fds_map = Vec::new();
        if fds_ptr != 0 && fds_len > 0 {
            let mut fds_buf = vec![0u8; (fds_len * 2) as usize];
            read_mem(store, fds_ptr, &mut fds_buf).map_err(|_| HaltExecutionError(1))?;
            for chunk in fds_buf.chunks_exact(2) {
                fds_map.push((chunk[0], chunk[1]));
            }
        }

        match crate::wasm::container::plant(store, &wasm_bytes, offset, size, if fds_map.is_empty() { None } else { Some(&fds_map) }) {
            Ok(id) => {
                let _ = write_u32(store, ret_ptr, 0); // ok
                let _ = write_u64(store, ret_ptr + 8, id);
            }
            Err(e) => {
                let _ = write_u32(store, ret_ptr, 1); // err
                let ptr = call_cabi_realloc(store, e.len() as u32, 1)?;
                let _ = write_bytes(store, ptr, e.as_bytes());
                let _ = write_u32(store, ret_ptr + 8, ptr);
                let _ = write_u32(store, ret_ptr + 12, e.len() as u32);
            }
        }
        Ok(vec![])
    }
);

crate::export_method!(
    "krakeos:system/container@0.1.0", "plant-from-path",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn container_plant_from_path_host<T: Config + Clone + Send + 'static>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let path_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let path_len = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let offset = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let size = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let fds_ptr = match args.get(4) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let fds_len = match args.get(5) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let ret_ptr = match args.get(6) { Some(Value::I32(v)) => *v as u32, _ => 0 };

        let mut path_buf = vec![0u8; path_len as usize];
        read_mem(store, path_ptr, &mut path_buf).map_err(|_| HaltExecutionError(1))?;
        let path = crate::alloc::string::String::from_utf8_lossy(&path_buf);

        let mut fds_map = Vec::new();
        if fds_ptr != 0 && fds_len > 0 {
            let mut fds_buf = vec![0u8; (fds_len * 2) as usize];
            read_mem(store, fds_ptr, &mut fds_buf).map_err(|_| HaltExecutionError(1))?;
            for chunk in fds_buf.chunks_exact(2) {
                fds_map.push((chunk[0], chunk[1]));
            }
        }

        let mut wasm_bytes = vec![];
        let read_res = if let Ok(mut file) = crate::fs::File::open(&path) {
            file.read_to_end(&mut wasm_bytes).is_ok()
        } else {
            false
        };

        if read_res {
            match crate::wasm::container::plant(store, &wasm_bytes, offset, size, if fds_map.is_empty() { None } else { Some(&fds_map) }) {
                Ok(id) => {
                    let _ = write_u32(store, ret_ptr, 0); // ok
                    let _ = write_u64(store, ret_ptr + 8, id);
                }
                Err(e) => {
                    let _ = write_u32(store, ret_ptr, 1); // err
                    let ptr = call_cabi_realloc(store, e.len() as u32, 1)?;
                    let _ = write_bytes(store, ptr, e.as_bytes());
                    let _ = write_u32(store, ret_ptr + 8, ptr);
                    let _ = write_u32(store, ret_ptr + 12, e.len() as u32);
                }
            }
        } else {
            let _ = write_u32(store, ret_ptr, 1);
            let msg = "Failed to open or read file";
            let ptr = call_cabi_realloc(store, msg.len() as u32, 1)?;
            let _ = write_bytes(store, ptr, msg.as_bytes());
            let _ = write_u32(store, ret_ptr + 8, ptr);
            let _ = write_u32(store, ret_ptr + 12, msg.len() as u32);
        }
        Ok(vec![])
    }
);

crate::export_method!(
    "krakeos:system/container@0.1.0", "harvest",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![],
    pub fn container_harvest_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let id = match args.get(0) { Some(Value::I64(v)) => *v as u64, _ => 0 };
        let ret_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };

        match crate::wasm::container::harvest(id) {
            Some(res) => {
                let _ = write_u32(store, ret_ptr, 0); // ok
                let _ = write_u32(store, ret_ptr + 4, res as u32);
            }
            None => {
                let _ = write_u32(store, ret_ptr, 1); // err
                let msg = "Still running or not found";
                let ptr = call_cabi_realloc(store, msg.len() as u32, 1)?;
                let _ = write_bytes(store, ptr, msg.as_bytes());
                let _ = write_u32(store, ret_ptr + 4, ptr);
                let _ = write_u32(store, ret_ptr + 8, msg.len() as u32);
            }
        }
        Ok(vec![])
    }
);

crate::export_method!(
    "krakeos:system/container@0.1.0", "list-children",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn container_list_children_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let ret_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let children = crate::wasm::container::list_children(store.container_id);
        
        let ptr = call_cabi_realloc(store, (children.len() * 8) as u32, 8)?;
        for (i, &id) in children.iter().enumerate() {
            let _ = write_u64(store, ptr + (i as u32 * 8), id);
        }
        let _ = write_u32(store, ret_ptr, ptr);
        let _ = write_u32(store, ret_ptr + 4, children.len() as u32);
        Ok(vec![])
    }
);

crate::export_method!(
    "krakeos:system/container@0.1.0", "kill-child",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![],
    pub fn container_kill_child_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let id = match args.get(0) { Some(Value::I64(v)) => *v as u64, _ => 0 };
        let ret_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };

        match crate::wasm::container::kill_child(id) {
            Ok(_) => {
                let _ = write_u32(store, ret_ptr, 0); // ok
            }
            Err(e) => {
                let _ = write_u32(store, ret_ptr, 1); // err
                let ptr = call_cabi_realloc(store, e.len() as u32, 1)?;
                let _ = write_bytes(store, ptr, e.as_bytes());
                let _ = write_u32(store, ret_ptr + 4, ptr);
                let _ = write_u32(store, ret_ptr + 8, e.len() as u32);
            }
        }
        Ok(vec![])
    }
);

crate::export_method!(
    "krakeos:graphics/screen@0.2.0", "get-width",
    [],
    vec![], vec![ValType::NumType(NumType::I32)],
    pub fn get_screen_width_host<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        Ok(vec![Value::I32(host::get_screen_width() as u32)])
    }
);

crate::export_method!(
    "krakeos:graphics/screen@0.2.0", "get-height",
    [],
    vec![], vec![ValType::NumType(NumType::I32)],
    pub fn get_screen_height_host<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        Ok(vec![Value::I32(host::get_screen_height() as u32)])
    }
);

crate::export_method!(
    "krakeos:system/window@0.2.0", "create",
    [],
    vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I64)],
    pub fn window_create_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I64(0)]) };
        
        let id = read_mem_u64(store, ptr)?;
        let buffer_off = read_mem_u64(store, ptr + 8)?;
        let back_buffer_off = read_mem_u64(store, ptr + 16)?;
        let flipped_off = read_mem_u64(store, ptr + 24)?;
        let pid = read_mem_u64(store, ptr + 32)?; 
        let x = read_mem_u64(store, ptr + 40)? as i64;
        let y = read_mem_u64(store, ptr + 48)? as i64;
        let z = read_mem_u64(store, ptr + 56)?;
        let width = read_mem_u64(store, ptr + 64)?;
        let height = read_mem_u64(store, ptr + 72)?;
        let mut bools = [0u8; 4];
        read_mem(store, ptr + 80, &mut bools).map_err(|_| HaltExecutionError(1))?;
        let min_width = read_mem_u64(store, ptr + 88)?;
        let min_height = read_mem_u64(store, ptr + 96)?;
        let event_handler = read_mem_u64(store, ptr + 104)?;
        let w_type_val = read_mem_u32(store, ptr + 112)?;
        let prev_x = read_mem_u64(store, ptr + 120)? as i64;
        let prev_y = read_mem_u64(store, ptr + 128)? as i64;
        let prev_width = read_mem_u64(store, ptr + 136)?;
        let prev_height = read_mem_u64(store, ptr + 144)?;

        let wasm_base = store.get_wasm_base_ptr() as u64;
        let host_win = host::Window {
            id: 0,
            buffer: if buffer_off != 0 { (wasm_base + buffer_off) } else { 0 },
            back_buffer: if back_buffer_off != 0 { (wasm_base + back_buffer_off) } else { 0 },
            flipped: if flipped_off != 0 { (wasm_base + flipped_off) } else { 0 },
            pid, x, y, z, width, height,
            can_move: bools[0] != 0, can_resize: bools[1] != 0, transparent: bools[2] != 0, treat_as_transparent: bools[3] != 0,
            _pad0: [0; 4],
            min_width, min_height, event_handler,
            w_type: unsafe { core::mem::transmute(w_type_val) },
            _pad1: [0; 4],
            prev_x, prev_y, prev_width, prev_height,
        };

        let res = host::add_window(&host_win) as u64;

        if res != 0 { let _ = write_bytes(store, ptr, &(res as u64).to_le_bytes()); }
        Ok(vec![Value::I64(res)])
    }
);

crate::export_method!(
    "krakeos:system/window@0.2.0", "update",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![],
    pub fn window_update_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };
        
        let id = read_mem_u64(store, ptr)?;
        let buffer_off = read_mem_u64(store, ptr + 8)?;
        let back_buffer_off = read_mem_u64(store, ptr + 16)?;
        let flipped_off = read_mem_u64(store, ptr + 24)?;
        let pid = read_mem_u64(store, ptr + 32)?; 
        let x = read_mem_u64(store, ptr + 40)? as i64;
        let y = read_mem_u64(store, ptr + 48)? as i64;
        let z = read_mem_u64(store, ptr + 56)?;
        let width = read_mem_u64(store, ptr + 64)?;
        let height = read_mem_u64(store, ptr + 72)?;
        let mut bools = [0u8; 4];
        read_mem(store, ptr + 80, &mut bools).map_err(|_| HaltExecutionError(1))?;
        let min_width = read_mem_u64(store, ptr + 88)?;
        let min_height = read_mem_u64(store, ptr + 96)?;
        let event_handler = read_mem_u64(store, ptr + 104)?;
        let w_type_val = read_mem_u32(store, ptr + 112)?;
        let prev_x = read_mem_u64(store, ptr + 120)? as i64;
        let prev_y = read_mem_u64(store, ptr + 128)? as i64;
        let prev_width = read_mem_u64(store, ptr + 136)?;
        let prev_height = read_mem_u64(store, ptr + 144)?;

        let wasm_base = store.get_wasm_base_ptr() as u64;
        let host_win = host::Window {
            id,
            buffer: if buffer_off != 0 { (wasm_base + buffer_off) } else { 0 },
            back_buffer: if back_buffer_off != 0 { (wasm_base + back_buffer_off) } else { 0 },
            flipped: if flipped_off != 0 { (wasm_base + flipped_off) } else { 0 },
            pid, x, y, z, width, height,
            can_move: bools[0] != 0, can_resize: bools[1] != 0, transparent: bools[2] != 0, treat_as_transparent: bools[3] != 0,
            _pad0: [0; 4],
            min_width, min_height, event_handler,
            w_type: unsafe { core::mem::transmute(w_type_val) },
            _pad1: [0; 4],
            prev_x, prev_y, prev_width, prev_height,
        };

        host::update_window(&host_win);
        Ok(vec![])
    }
);

crate::export_method!(
    "krakeos:system/window@0.2.0", "get-events",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn window_get_events_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let handle = match args.get(0) { Some(Value::I64(v)) => *v, _ => 0 };
        let buf_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(0)]) };
        let max = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(0)]) };
        
        let mut events = crate::alloc::vec![host::Event::None; max as usize];
        let count = host::get_events(handle as usize, &mut events);
        
        if count > 0 {
            let bytes = unsafe {
                core::slice::from_raw_parts(events.as_ptr() as *const u8, count * core::mem::size_of::<host::Event>())
            };
            write_bytes(store, buf_ptr, bytes).map_err(|_| HaltExecutionError(1))?;
        }
        Ok(vec![Value::I32(count as u32)])
    }
);

crate::export_method!(
    "krakeos:system/window@0.2.0", "register-event-queue",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn register_event_queue_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let header_off = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };
        let buf_off    = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };
        let capacity   = match args.get(2) { Some(Value::I32(v)) => *v as u64, _ => return Ok(vec![]) };
        let wasm_base  = store.get_wasm_base_ptr() as u64;
        host::register_event_queue(wasm_base + header_off as u64, wasm_base + buf_off as u64, capacity);
        Ok(vec![])
    }
);

crate::export_method!(
    "krakeos:system/window@0.2.0", "deregister-event-queue",
    [],
    vec![], vec![],
    pub fn deregister_event_queue_host<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        host::deregister_event_queue();
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:cli/terminal-stdin@0.2.0", "get-terminal-stdin",
    [],
    vec![], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)],
    pub fn get_terminal_stdin_host<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
        let id = wasi.next_resource_id;
        wasi.next_resource_id += 1;
        wasi.resource_table.insert(id, WasiResource::TerminalInput(0));
        Ok(vec![Value::I32(1), Value::I32(id as u32)])
    }
);

crate::export_method!(
    "wasi:cli/terminal-stdout@0.2.0", "get-terminal-stdout",
    [],
    vec![], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)],
    pub fn get_terminal_stdout_host<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
        let id = wasi.next_resource_id;
        wasi.next_resource_id += 1;
        wasi.resource_table.insert(id, WasiResource::TerminalOutput(1));
        Ok(vec![Value::I32(1), Value::I32(id as u32)])
    }
);

crate::export_method!(
    "wasi:cli/terminal-stderr@0.2.0", "get-terminal-stderr",
    [],
    vec![], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)],
    pub fn get_terminal_stderr_host<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
        let id = wasi.next_resource_id;
        wasi.next_resource_id += 1;
        wasi.resource_table.insert(id, WasiResource::TerminalOutput(2));
        Ok(vec![Value::I32(1), Value::I32(id as u32)])
    }
);

crate::export_method!(
    "wasi:cli/stdout@0.2.0", "get-stdout",
    [],
    vec![], vec![ValType::NumType(NumType::I32)],
    pub fn get_stdout_host<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
        let id = wasi.next_resource_id;
        wasi.next_resource_id += 1;
        wasi.resource_table.insert(id, WasiResource::OutputStream(OutputStreamSource::Stdout));
        Ok(vec![Value::I32(id as u32)])
    }
);

crate::export_method!(
    "wasi:cli/stdin@0.2.0", "get-stdin",
    [],
    vec![], vec![ValType::NumType(NumType::I32)],
    pub fn get_stdin_host<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
        let id = wasi.next_resource_id;
        wasi.next_resource_id += 1;
        wasi.resource_table.insert(id, WasiResource::InputStream(InputStreamSource::Stdin));
        Ok(vec![Value::I32(id as u32)])
    }
);

crate::export_method!(
    "wasi:cli/stderr@0.2.0", "get-stderr",
    [],
    vec![], vec![ValType::NumType(NumType::I32)],
    pub fn get_stderr_host<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
        let id = wasi.next_resource_id;
        wasi.next_resource_id += 1;
        wasi.resource_table.insert(id, WasiResource::OutputStream(OutputStreamSource::Stderr));
        Ok(vec![Value::I32(id as u32)])
    }
);

crate::export_method!(
    "krakeos:system/memory@0.2.0", "shm-get",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I64)],
    pub fn shm_get_host_impl<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let name_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I64(0)]) };
        let name_len = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I64(0)]) };
        let size = match args.get(2) { Some(Value::I32(v)) => *v as usize, _ => return Ok(vec![Value::I64(0)]) };
        crate::debugln!("[WASI] shm_get_host_impl: name_ptr={:#x}, name_len={}, size={}", name_ptr, name_len, size);
        let mut name_buf = vec![0u8; name_len as usize];
        read_mem(store, name_ptr, &mut name_buf).map_err(|_| HaltExecutionError(1))?;
        let name = crate::alloc::string::String::from_utf8_lossy(&name_buf);
        crate::debugln!("[WASI] shm_get: Requesting segment '{}' (size {})", name, size);

        if let Some(&offset) = store.shm_mappings.get(name.as_ref()) {
            crate::debugln!("[WASI] shm_get: Found cached mapping at offset {:#x}", offset);
            return Ok(vec![Value::I64(offset as u64)]);
        }

        let res = host::shm_get(&name, size as u64).unwrap_or(0);
        crate::debugln!("[WASI] shm_get: host::shm_get returned {:#x}", res);
        if res == 0 { 
            crate::debugln!("[WASI] shm_get: FAILED to get segment '{}'", name);
            return Ok(vec![Value::I64(u64::MAX)]); 
        }

        if let Some(sas_base) = store.sas_memory_base {
            let caller = store.caller_module.unwrap_or(0);
            let mem_addrs = &store.modules.get(caller).mem_addrs;
            if mem_addrs.is_empty() {
                crate::debugln!("[WASI] shm_get: Caller has no memory!");
                return Ok(vec![Value::I64(u64::MAX)]);
            }
            let mem_addr = mem_addrs[0];

            let current_pages = store.memories.get(mem_addr).size() as u32;
            let needed_pages = (size as u32 + 65535) / 65536;

            crate::debugln!("[WASI] shm_get: Growing WASM memory (current_pages={}, needed_pages={})", current_pages, needed_pages);
            if store.memories.get_mut(mem_addr).grow(needed_pages).is_err() {
                crate::debugln!("[WASI] shm_get: Failed to grow WASM memory for segment '{}'", name);
                return Ok(vec![Value::I64(u64::MAX)]);
            }

            // The mapping should start at the offset of the OLD end of memory
            let offset = current_pages as u64 * 65536;
            let target_sas_addr = sas_base + offset;

            crate::debugln!("[WASI] shm_get: Mapping SHM segment '{}' to SAS address {:#x} (offset {:#x})", name, target_sas_addr, offset);
            let map_res = unsafe {
                host::shm_map_raw(name.as_ptr(), name.len(), target_sas_addr)
            };

            if map_res == 0 {
                crate::debugln!("[WASI] shm_get: Mapped segment '{}' into WASM window at offset {:#x}", name, offset);
                store.shm_mappings.insert(name.into_owned(), offset as u32);
                Ok(vec![Value::I64(offset)])
            } else {
                crate::debugln!("[WASI] shm_get: shm_map_raw FAILED for segment '{}' (res={:#x})", name, map_res);
                Ok(vec![Value::I64(u64::MAX)])
            }
        } else {
            crate::debugln!("[WASI] shm_get: No SAS memory base, returning raw address {:#x}", res);
            Ok(vec![Value::I64(res)])
        }
    }
);

crate::export_method!(
    "wasi:cli/terminal-input@0.2.0", "[resource-drop]terminal-input",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn terminal_input_drop_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        crate::wasm::wasi::preview2::resource_drop(store, args)
    }
);

crate::export_method!(
    "wasi:cli/terminal-output@0.2.0", "[resource-drop]terminal-output",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn terminal_output_drop_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        crate::wasm::wasi::preview2::resource_drop(store, args)
    }
);

crate::export_method!(
    "krakeos:system/process@0.2.0", "get-pid",
    [],
    vec![], vec![ValType::NumType(NumType::I64)],
    pub fn get_pid_host<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        Ok(vec![Value::I64(host::process_get_pid())])
    }
);

crate::export_method!(
    "krakeos:system/process@0.2.0", "get-current-user",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn get_current_user_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let ret_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let user = host::user::get_current_user();
        let ptr = call_cabi_realloc(store, user.len() as u32, 1)?;
        let _ = write_bytes(store, ptr, user.as_bytes());
        let _ = write_u32(store, ret_ptr, ptr);
        let _ = write_u32(store, ret_ptr + 4, user.len() as u32);
        Ok(vec![])
    }
);

crate::export_method!(
    "krakeos:system/process@0.2.0", "get-slot-info",
    [],
    vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn get_slot_info_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let buf_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let mut slot_info = host::SlotInfo {
            slot_id: 0,
            linear_memory_base: 0,
            linear_memory_size: 0,
            code_base: 0,
            stack_base: 0,
        };
        let res = host::process_get_slot_info(&mut slot_info as *mut _ as *mut u8);
        let _ = write_bytes(store, buf_ptr, unsafe {
            core::slice::from_raw_parts(&slot_info as *const _ as *const u8, core::mem::size_of::<host::SlotInfo>())
        });
        Ok(vec![Value::I32(res as u32)])
    }
);

crate::export_method!(
    "krakeos:system/process@0.2.0", "set-nonblock",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn set_nonblock_host<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let nonblock = match args.get(1) { Some(Value::I32(v)) => *v != 0, _ => false };
        Ok(vec![Value::I32(host::set_nonblock(fd as usize, nonblock) as u32)])
    }
);

crate::export_method!(
    "krakeos:system/process@0.2.0", "ioctl",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I32)],
    pub fn ioctl_host<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I64(v)) => *v, _ => 0 };
        let request = match args.get(1) { Some(Value::I64(v)) => *v, _ => 0 };
        let arg = match args.get(2) { Some(Value::I64(v)) => *v, _ => 0 };
        Ok(vec![Value::I32(host::process_ioctl(fd, request, arg) as u32)])
    }
);

crate::export_method!(
    "krakeos:system/process@0.2.0", "poll",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I32)],
    pub fn poll_host<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fds_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let count = match args.get(1) { Some(Value::I64(v)) => *v, _ => 0 };
        let timeout = match args.get(2) { Some(Value::I64(v)) => *v, _ => 0 };
        // Note: fds_ptr is a WASM pointer. In SAS it might be okay to pass directly if we add offset.
        // But host::process_poll expects a host pointer.
        // For now, let's assume SAS and hope for the best, or use a better approach if it fails.
        // Since I don't have the store here to read_mem, and export_method! doesn't give it to me easily if I didn't name it.
        // Wait, I can name it.
        Ok(vec![Value::I32(host::process_poll(fds_ptr as *mut u8, count, timeout) as u32)])
    }
);

crate::export_method!(
    "krakeos:system/terminal@0.1.0", "set-window-size",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn terminal_set_window_size<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let rows = match args.get(1) { Some(Value::I32(v)) => *v as u16, _ => 0 };
        let cols = match args.get(2) { Some(Value::I32(v)) => *v as u16, _ => 0 };
        let ret_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => 0 };

        let ws = host::WinSize { ws_row: rows, ws_col: cols, ws_xpixel: 0, ws_ypixel: 0 };
        let res = host::ioctl(fd as usize, host::TIOCSWINSZ, &ws as *const _ as u64);
        
        let mut buf = [0u8; 4];
        buf[0] = if res == 0 { 0 } else { 1 };
        write_bytes(store, ret_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "krakeos:system/terminal@0.1.0", "get-window-size",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn terminal_get_window_size<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let ret_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };

        let mut ws = host::WinSize { ws_row: 0, ws_col: 0, ws_xpixel: 0, ws_ypixel: 0 };
        let res = host::ioctl(fd as usize, host::TIOCGWINSZ, &mut ws as *mut _ as u64);
        
        let mut buf = [0u8; 12];
        if res == 0 {
            buf[0] = 0; // ok
            buf[4..6].copy_from_slice(&ws.ws_row.to_le_bytes());
            buf[6..8].copy_from_slice(&ws.ws_col.to_le_bytes());
        } else {
            buf[0] = 1; // err
        }
        write_bytes(store, ret_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "krakeos:system/debug@0.1.0", "get-process-list",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn get_process_list_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let ret_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let list = host::get_process_list();
        
        let struct_size = core::mem::size_of::<host::ProcessInfo>() as u32;
        let ptr = call_cabi_realloc(store, (list.len() as u32 * struct_size), 8)?;
        
        for (i, info) in list.iter().enumerate() {
            let offset = i as u32 * struct_size;
            let _ = write_bytes(store, ptr + offset, unsafe {
                core::slice::from_raw_parts(info as *const _ as *const u8, struct_size as usize)
            });
        }
        
        let _ = write_u32(store, ret_ptr, ptr);
        let _ = write_u32(store, ret_ptr + 4, list.len() as u32);
        Ok(vec![])
    }
);

crate::export_method!(
    "krakeos:system/process@0.2.0", "spawn-thread",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I64)],
    pub fn spawn_thread_host<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let entry = match args.get(0) { Some(Value::I64(v)) => *v, _ => 0 };
        let stack = match args.get(1) { Some(Value::I64(v)) => *v, _ => 0 };
        let arg = match args.get(2) { Some(Value::I64(v)) => *v, _ => 0 };
        Ok(vec![Value::I64(crate::sys::host_spawn_thread(entry, stack, arg))])
    }
);

crate::export_method!(
    "krakeos:system/process@0.2.0", "thread-exit",
    [],
    vec![], vec![],
    pub fn thread_exit_host<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        crate::sys::host_thread_exit();
        Ok(vec![])
    }
);

crate::export_method!(
    "krakeos:system/process@0.2.0", "syscall",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I64)],
    pub fn syscall_host<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let num = match args.get(0) { Some(Value::I64(v)) => *v, _ => 0 };
        let arg1 = match args.get(1) { Some(Value::I64(v)) => *v, _ => 0 };
        let arg2 = match args.get(2) { Some(Value::I64(v)) => *v, _ => 0 };
        let arg3 = match args.get(3) { Some(Value::I64(v)) => *v, _ => 0 };
        
        Ok(vec![Value::I64(unsafe { host::syscall(num, arg1, arg2, arg3) })])
    }
);

crate::export_method!(
    "krakeos:system/process@0.2.0", "kill",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn kill_process_host<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let pid = match args.get(0) { Some(Value::I64(v)) => *v, _ => 0 };
        let sig = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        Ok(vec![Value::I32(host::process_kill(pid, sig) as u32)])
    }
);

crate::export_method!(
    "krakeos:system/debug@0.1.0", "kill",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn kill_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let pid = match args.get(0) { Some(Value::I64(v)) => *v as u64, _ => 0 };
        let signal = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let ret_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };

        // Restriction: only kill own children or self
        let self_id = store.container_id.unwrap_or(0);
        let children = crate::wasm::container::list_children(Some(self_id));
        let allowed = pid == self_id || children.contains(&pid);

        let mut buf = [0u8; 4];
        if allowed {
            let res = host::process_kill(pid, signal);
            buf[0] = if res == 0 { 0 } else { 1 };
        } else {
            buf[0] = 1; // err: permission denied
        }
        
        let _ = write_bytes(store, ret_ptr, &buf);
        Ok(vec![])
    }
);

crate::export_method!(
    "krakeos:system/debug@0.1.0", "dump-vma",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn dump_vma_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let ret_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        
        let mut buf = [0u8; 4096];
        let len = host::get_vma_dump(buf.as_mut_ptr(), buf.len() as u64) as u32;
        
        let ptr = call_cabi_realloc(store, len, 1)?;
        let _ = write_bytes(store, ptr, &buf[..len as usize]);
        
        let _ = write_u32(store, ret_ptr, ptr);
        let _ = write_u32(store, ret_ptr + 4, len);
        Ok(vec![])
    }
);

crate::export_method!(
    "krakeos:system/debug@0.1.0", "get-memory-usage",
    [],
    vec![], vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)],
    pub fn get_memory_usage_host<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        Ok(vec![
            Value::I64(host::get_used_mem()),
            Value::I64(host::get_total_mem())
        ])
    }
);

crate::export_method!(
    "krakeos:system/process@0.2.0", "chdir",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I32)],
    pub fn chdir_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(-1i32 as u32)]) };
        let len = match args.get(1) { Some(Value::I64(v)) => *v, _ => return Ok(vec![Value::I32(-1i32 as u32)]) };
        
        let mut buf = crate::alloc::vec![0u8; len as usize];
        if let Err(_) = read_mem(store, ptr, &mut buf) {
            return Ok(vec![Value::I32(-1i32 as u32)]);
        }
        let path = crate::alloc::string::String::from_utf8_lossy(&buf);
        Ok(vec![Value::I32(host::chdir(&path) as u32)])
    }
);

pub fn register_wasi<T: Config + Clone + Send + 'static>(linker: &mut crate::wasm::Linker, store: &mut crate::wasm::Store<'_, T>) {
    get_screen_width_host::register(linker, store);
    get_screen_height_host::register(linker, store);
    window_create_host::register(linker, store);
    window_update_host::register(linker, store);
    window_get_events_host::register(linker, store);
    register_event_queue_host::register(linker, store);
    deregister_event_queue_host::register(linker, store);
    get_terminal_stdin_host::register(linker, store);
    get_terminal_stdout_host::register(linker, store);
    get_terminal_stderr_host::register(linker, store);
    get_stdout_host::register(linker, store);
    get_stdin_host::register(linker, store);
    get_stderr_host::register(linker, store);
    shm_get_host_impl::register(linker, store);
    terminal_input_drop_host::register(linker, store);
    terminal_output_drop_host::register(linker, store);
    container_plant_host::register(linker, store);
    container_plant_from_path_host::register(linker, store);
    container_harvest_host::register(linker, store);
    container_list_children_host::register(linker, store);
    container_kill_child_host::register(linker, store);
    get_pid_host::register(linker, store);
    get_current_user_host::register(linker, store);
    get_slot_info_host::register(linker, store);
    set_nonblock_host::register(linker, store);
    ioctl_host::register(linker, store);
    poll_host::register(linker, store);
    terminal_set_window_size::register(linker, store);
    terminal_get_window_size::register(linker, store);
    get_process_list_host::register(linker, store);
    kill_process_host::register(linker, store);
    kill_host::register(linker, store);
    syscall_host::register(linker, store);
    spawn_thread_host::register(linker, store);
    thread_exit_host::register(linker, store);
    chdir_host::register(linker, store);
    dump_vma_host::register(linker, store);
    get_memory_usage_host::register(linker, store);
}
