use crate::alloc::{vec, vec::Vec};
use crate::wasm::{
    common::{config::Config, value::Value, reader::types::{ValType, NumType}},
    interpreter::store::{HaltExecutionError, Store},
    wasi::ctx::{WasiResource, InputStreamSource, OutputStreamSource},
};
use crate::wasm::wasi::preview2::{read_mem, read_mem_u32, read_mem_u64, write_bytes};
use crate::os::krakeos as host;

crate::export_method!(
    "krakeos:graphics/screen@0.2.0", "get-width",
    [],
    vec![], vec![ValType::NumType(NumType::I32)],
    pub fn get_screen_width<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        Ok(vec![Value::I32(host::get_screen_width() as u32)])
    }
);

crate::export_method!(
    "krakeos:graphics/screen@0.2.0", "get-height",
    [],
    vec![], vec![ValType::NumType(NumType::I32)],
    pub fn get_screen_height<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        Ok(vec![Value::I32(host::get_screen_height() as u32)])
    }
);

crate::export_method!(
    "krakeos:system/window@0.2.0", "create",
    [],
    vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I64)],
    pub fn window_create<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I64(0)]) };
        let buffer_off = read_mem_u32(store, ptr + 4)? as u64;
        let back_buffer_off = read_mem_u32(store, ptr + 8)? as u64;
        let flipped_off = read_mem_u32(store, ptr + 12)? as u64;
        let pid = read_mem_u64(store, ptr + 16)?;
        let x = read_mem_u32(store, ptr + 24)? as i32 as isize;
        let y = read_mem_u32(store, ptr + 28)? as i32 as isize;
        let z = read_mem_u32(store, ptr + 32)? as usize;
        let width = read_mem_u32(store, ptr + 36)? as usize;
        let height = read_mem_u32(store, ptr + 40)? as usize;
        let mut bools = [0u8; 4];
        read_mem(store, ptr + 44, &mut bools).map_err(|_| HaltExecutionError(1))?;
        let min_width = read_mem_u32(store, ptr + 48)? as usize;
        let min_height = read_mem_u32(store, ptr + 52)? as usize;
        let event_handler = read_mem_u32(store, ptr + 56)? as usize;
        let w_type_val = read_mem_u32(store, ptr + 60)?;
        let prev_x = read_mem_u32(store, ptr + 64)? as i32 as isize;
        let prev_y = read_mem_u32(store, ptr + 68)? as i32 as isize;
        let prev_width = read_mem_u32(store, ptr + 72)? as usize;
        let prev_height = read_mem_u32(store, ptr + 76)? as usize;

        let wasm_base = store.get_wasm_base_ptr() as u64;
        let host_win = host::Window {
            id: 0,
            buffer: if buffer_off != 0 { (wasm_base + buffer_off) as usize } else { 0 },
            back_buffer: if back_buffer_off != 0 { (wasm_base + back_buffer_off) as usize } else { 0 },
            flipped: if flipped_off != 0 { (wasm_base + flipped_off) as usize } else { 0 },
            pid, x, y, z, width, height,
            can_move: bools[0] != 0, can_resize: bools[1] != 0, transparent: bools[2] != 0, treat_as_transparent: bools[3] != 0,
            min_width, min_height, event_handler,
            w_type: unsafe { core::mem::transmute(w_type_val) },
            prev_x, prev_y, prev_width, prev_height,
        };

        let res = host::add_window(&host_win) as u64;

        if res != 0 { let _ = write_bytes(store, ptr, &(res as u32).to_le_bytes()); }
        Ok(vec![Value::I64(res)])
    }
);

crate::export_method!(
    "krakeos:system/window@0.2.0", "update",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![],
    pub fn window_update<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };
        let id = read_mem_u32(store, ptr)? as usize;
        let buffer_off = read_mem_u32(store, ptr + 4)? as u64;
        let back_buffer_off = read_mem_u32(store, ptr + 8)? as u64;
        let flipped_off = read_mem_u32(store, ptr + 12)? as u64;
        let pid = read_mem_u64(store, ptr + 16)?;
        let x = read_mem_u32(store, ptr + 24)? as i32 as isize;
        let y = read_mem_u32(store, ptr + 28)? as i32 as isize;
        let z = read_mem_u32(store, ptr + 32)? as usize;
        let width = read_mem_u32(store, ptr + 36)? as usize;
        let height = read_mem_u32(store, ptr + 40)? as usize;
        let mut bools = [0u8; 4];
        read_mem(store, ptr + 44, &mut bools).map_err(|_| HaltExecutionError(1))?;
        let min_width = read_mem_u32(store, ptr + 48)? as usize;
        let min_height = read_mem_u32(store, ptr + 52)? as usize;
        let event_handler = read_mem_u32(store, ptr + 56)? as usize;
        let w_type_val = read_mem_u32(store, ptr + 60)?;
        let prev_x = read_mem_u32(store, ptr + 64)? as i32 as isize;
        let prev_y = read_mem_u32(store, ptr + 68)? as i32 as isize;
        let prev_width = read_mem_u32(store, ptr + 72)? as usize;
        let prev_height = read_mem_u32(store, ptr + 76)? as usize;

        let wasm_base = store.get_wasm_base_ptr() as u64;
        let host_win = host::Window {
            id,
            buffer: if buffer_off != 0 { (wasm_base + buffer_off) as usize } else { 0 },
            back_buffer: if back_buffer_off != 0 { (wasm_base + back_buffer_off) as usize } else { 0 },
            flipped: if flipped_off != 0 { (wasm_base + flipped_off) as usize } else { 0 },
            pid, x, y, z, width, height,
            can_move: bools[0] != 0, can_resize: bools[1] != 0, transparent: bools[2] != 0, treat_as_transparent: bools[3] != 0,
            min_width, min_height, event_handler,
            w_type: unsafe { core::mem::transmute(w_type_val) },
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
    pub fn window_get_events<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
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
    "wasi:cli/terminal-stdin@0.2.0", "get-terminal-stdin",
    [],
    vec![], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)],
    pub fn get_terminal_stdin<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
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
    pub fn get_terminal_stdout<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
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
    pub fn get_terminal_stderr<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
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
    pub fn get_stdout<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
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
    pub fn get_stdin<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
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
    pub fn get_stderr<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
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
    pub fn shm_get_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let name_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I64(0)]) };
        let name_len = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I64(0)]) };
        let size = match args.get(2) { Some(Value::I32(v)) => *v as usize, _ => return Ok(vec![Value::I64(0)]) };
        let mut name_buf = vec![0u8; name_len as usize];
        read_mem(store, name_ptr, &mut name_buf).map_err(|_| HaltExecutionError(1))?;
        let name = crate::alloc::string::String::from_utf8_lossy(&name_buf);
        
        let res = host::shm_get(&name, size as u64).unwrap_or(0);
        Ok(vec![Value::I64(res)])
    }
);

crate::export_method!(
    "wasi:cli/terminal-input@0.2.0", "[resource-drop]terminal-input",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn terminal_input_drop<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        crate::wasm::wasi::preview2::resource_drop(store, args)
    }
);

crate::export_method!(
    "wasi:cli/terminal-output@0.2.0", "[resource-drop]terminal-output",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn terminal_output_drop<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        crate::wasm::wasi::preview2::resource_drop(store, args)
    }
);

pub fn register_wasi<T: Config>(linker: &mut crate::wasm::Linker, store: &mut crate::wasm::Store<'_, T>) {
    get_screen_width::register(linker, store);
    get_screen_height::register(linker, store);
    window_create::register(linker, store);
    window_update::register(linker, store);
    window_get_events::register(linker, store);
    get_terminal_stdin::register(linker, store);
    get_terminal_stdout::register(linker, store);
    get_terminal_stderr::register(linker, store);
    get_stdout::register(linker, store);
    get_stdin::register(linker, store);
    get_stderr::register(linker, store);
    shm_get_host::register(linker, store);
    terminal_input_drop::register(linker, store);
    terminal_output_drop::register(linker, store);
}
