use crate::rust_alloc::vec::Vec;
use crate::wasm::execution::store::Store;
use crate::wasm::execution::config::Config;
use crate::wasm::execution::value::{Value, Ref};
use crate::wasm::{RefType, ValType, NumType, TrapError};

// This function is called by AOT code to invoke other functions (imports or WASM/AOT)
// It mimics the signature expected by the generated assembly call
pub unsafe extern "C" fn aot_invoke_trampoline<T: Config>(
    store_ptr: *mut (),
    func_idx: u32,
    params_ptr: *const u64,
    results_ptr: *mut u64,
) {
    let store = &mut *(store_ptr as *mut Store<T>);
    let func_addr = func_idx as usize;
    let func_inst = store.functions.get(func_addr);
    let func_ty = func_inst.ty();
    
    let param_count = func_ty.params.valtypes.len();
    let mut params = Vec::with_capacity(param_count);
    let raw_params = core::slice::from_raw_parts(params_ptr, param_count);
    
    for (i, ty) in func_ty.params.valtypes.iter().enumerate() {
        let raw = raw_params[i];
        let val = match ty {
            ValType::NumType(NumType::I32) => Value::I32(raw as u32),
            ValType::NumType(NumType::I64) => Value::I64(raw),
            ValType::NumType(NumType::F32) => Value::F32(crate::wasm::execution::value::F32::from_bits(raw as u32)),
            ValType::NumType(NumType::F64) => Value::F64(crate::wasm::execution::value::F64::from_bits(raw)),
            ValType::RefType(rt) => match rt {
                RefType::FuncRef => Value::Ref(Ref::Func(raw as usize)),
                RefType::ExternRef => Value::Ref(Ref::Extern(crate::wasm::execution::value::ExternAddr(raw as usize))),
            },
            ValType::VecType => Value::V128([0; 16]),
        };
        params.push(val);
    }
    
    let result = store.invoke_unchecked(func_addr, params, None);
    
    match result {
        Ok(crate::wasm::execution::resumable::RunState::Finished { values, .. }) => {
            let raw_results = core::slice::from_raw_parts_mut(results_ptr, values.len());
            for (i, val) in values.iter().enumerate() {
                let raw = match val {
                    Value::I32(v) => *v as u64,
                    Value::I64(v) => *v,
                    Value::F32(v) => v.to_bits() as u64,
                    Value::F64(v) => v.to_bits(),
                    Value::Ref(r) => match r {
                        Ref::Null(_) => 0,
                        Ref::Func(addr) => *addr as u64,
                        Ref::Extern(addr) => addr.0 as u64,
                    },
                    Value::V128(_) => 0,
                };
                raw_results[i] = raw;
            }
        }
        _ => {
            crate::debugln!("[AOT Trampoline] Execution failed or suspended. Trapping...");
            // We must not return to AOT code as the state is invalid.
            // Ideally we should propagate the trap. For now, panic/exit.
            panic!("AOT Trampoline: Sub-call failed.");
        }
    }
}

pub unsafe extern "C" fn aot_global_get<T: Config>(store_ptr: *mut (), global_idx: u32, module_addr: usize) -> u64 {
    let store = &mut *(store_ptr as *mut Store<T>);
    // global_idx is module-relative. Resolve to absolute address.
    let global_addr = store.modules.get(module_addr).global_addrs[global_idx as usize];
    let val = store.globals.get(global_addr).value;
    
    match val {
        Value::I32(v) => v as u64,
        Value::I64(v) => v,
        Value::F32(v) => v.to_bits() as u64,
        Value::F64(v) => v.to_bits(),
        _ => 0, // Refs/Vecs
    }
}

pub unsafe extern "C" fn aot_global_set<T: Config>(store_ptr: *mut (), global_idx: u32, val: u64, module_addr: usize) {
    let store = &mut *(store_ptr as *mut Store<T>);
    let global_addr = store.modules.get(module_addr).global_addrs[global_idx as usize];
    let global_inst = store.globals.get_mut(global_addr);
    let ty = global_inst.ty.ty;
    
    let value = match ty {
        ValType::NumType(NumType::I32) => Value::I32(val as u32),
        ValType::NumType(NumType::I64) => Value::I64(val),
        ValType::NumType(NumType::F32) => Value::F32(crate::wasm::execution::value::F32::from_bits(val as u32)),
        ValType::NumType(NumType::F64) => Value::F64(crate::wasm::execution::value::F64::from_bits(val)),
        _ => Value::I64(0),
    };
    global_inst.value = value;
}

pub unsafe extern "C" fn aot_call_indirect<T: Config>(
    store_ptr: *mut (),
    type_idx: u32,
    table_idx: u32,
    table_elem_idx: u32,
    params_ptr: *const u64,
    results_ptr: *mut u64,
    module_addr: usize
) {
    let store = &mut *(store_ptr as *mut Store<T>);
    let module = store.modules.get(module_addr);
    
    let table_addr = module.table_addrs[table_idx as usize];
    let tab = store.tables.get(table_addr);
    
    if table_elem_idx as usize >= tab.elem.len() {
        crate::debugln!("[AOT Trap] Table access out of bounds");
        return; // Should trap ideally
    }
    
    let r = tab.elem[table_elem_idx as usize];
    let func_addr = match r {
        Ref::Func(a) => a,
        _ => {
            crate::debugln!("[AOT Trap] Indirect call null ref");
            return;
        }
    };
    
    // Check signature
    let expected_ty = &module.types[type_idx as usize];
    let actual_ty = store.functions.get(func_addr).ty();
    
    if expected_ty != &actual_ty {
        crate::debugln!("[AOT Trap] Indirect call signature mismatch");
        return;
    }
    
    // Redirect to invoke trampoline
    aot_invoke_trampoline::<T>(store_ptr, func_addr as u32, params_ptr, results_ptr);
}

pub unsafe extern "C" fn aot_memory_size<T: Config>(store_ptr: *mut (), mem_idx: u32, module_addr: usize) -> u32 {
    let store = &mut *(store_ptr as *mut Store<T>);
    let mem_addr = store.modules.get(module_addr).mem_addrs[mem_idx as usize];
    store.memories.get(mem_addr).size().try_into().unwrap()
}

pub unsafe extern "C" fn aot_get_mem_base<T: Config>(store_ptr: *mut (), module_addr: usize) -> u64 {

    let store = &*(store_ptr as *const Store<T>);

    let mem_addr = store.modules.get(module_addr).mem_addrs[0]; // Assume memory 0

    store.memories.get(mem_addr).mem.get_base_ptr() as u64

}



pub unsafe extern "C" fn aot_memory_grow<T: Config>(store_ptr: *mut (), mem_idx: u32, delta: u32, module_addr: usize) -> u32 {

    let store = &mut *(store_ptr as *mut Store<T>);

    let mem_addr = store.modules.get(module_addr).mem_addrs[mem_idx as usize];

    let old_size = store.memories.get(mem_addr).size().try_into().unwrap();

    match store.memories.get_mut(mem_addr).grow(delta) {

        Ok(_) => old_size,

        Err(_) => u32::MAX,

    }

}

pub unsafe extern "C" fn aot_memory_copy<T: Config>(store_ptr: *mut (), dst: u32, src: u32, len: u32, mem_idx_dst: u32, mem_idx_src: u32, module_addr: usize) {
    let store = &mut *(store_ptr as *mut Store<T>);
    let module = store.modules.get(module_addr);
    let mem_addr_dst = module.mem_addrs[mem_idx_dst as usize];
    let mem_addr_src = module.mem_addrs[mem_idx_src as usize];
    
    // Check bounds roughly (Store implementation handles details but we need to resolve addresses)
    // Actually, Store has a `copy` method on LinearMemory, but we need to access two memories.
    // Since KrakeOS/Wasm usually has one memory (index 0), mem_addr_dst == mem_addr_src.
    
    let mem_dst = store.memories.get(mem_addr_dst);
    // Unsafe hack to get a second mutable reference if they are different, or same.
    // Since we are single-threaded here effectively:
    let mem_src = store.memories.get(mem_addr_src); 
    
    // We can use the Store::mem_copy method if we exposed it or implemented it.
    // Or just use LinearMemory::copy.
    // LinearMemory::copy takes &self and &other.
    
    if let Err(_) = mem_dst.mem.copy(dst as usize, &mem_src.mem, src as usize, len as usize) {
        crate::debugln!("[AOT Trap] memory.copy out of bounds");
        // Trap
    }
}

pub unsafe extern "C" fn aot_memory_fill<T: Config>(store_ptr: *mut (), dst: u32, val: u32, len: u32, mem_idx: u32, module_addr: usize) {
    let store = &mut *(store_ptr as *mut Store<T>);
    let mem_addr = store.modules.get(module_addr).mem_addrs[mem_idx as usize];
    let mem = store.memories.get(mem_addr);
    if let Err(_) = mem.mem.fill(dst as usize, val as u8, len as usize) {
        crate::debugln!("[AOT Trap] memory.fill out of bounds");
    }
}

pub unsafe extern "C" fn aot_memory_init<T: Config>(store_ptr: *mut (), dst: u32, src: u32, len: u32, data_idx: u32, mem_idx: u32, module_addr: usize) {
    let store = &mut *(store_ptr as *mut Store<T>);
    let module = store.modules.get(module_addr);
    let mem_addr = module.mem_addrs[mem_idx as usize];
    let data_addr = module.data_addrs[data_idx as usize];
    
    let mem = store.memories.get(mem_addr);
    let data = store.data.get(data_addr);
    
    if let Err(_) = mem.mem.init(dst as usize, &data.data, src as usize, len as usize) {
        crate::debugln!("[AOT Trap] memory.init out of bounds");
    }
}

pub unsafe extern "C" fn aot_data_drop<T: Config>(store_ptr: *mut (), data_idx: u32, module_addr: usize) {
    let store = &mut *(store_ptr as *mut Store<T>);
    let module = store.modules.get(module_addr);
    let data_addr = module.data_addrs[data_idx as usize];
    // In many implementations data.drop just clears the segment.
    // For now we might do nothing or clear it if `DataInst` supports it.
    // Our DataInst is `pub struct DataInst { pub data: Vec<u8> }`
    // We can clear the vec.
    let data = store.data.get_mut(data_addr);
    data.data.clear();
}

// Saturating Truncation Trampolines
// Input: value in XMM0 (f32/f64)
// Output: result in RAX (i32/i64)

pub unsafe extern "C" fn aot_i32_trunc_sat_f32_s(val: f32) -> i32 {
    if val.is_nan() { return 0; }
    if val >= 2147483648.0 { return i32::MAX; }
    if val <= -2147483649.0 { return i32::MIN; }
    val as i32
}

pub unsafe extern "C" fn aot_i32_trunc_sat_f32_u(val: f32) -> u32 {
    if val.is_nan() { return 0; }
    if val >= 4294967296.0 { return u32::MAX; }
    if val <= -1.0 { return 0; }
    val as u32
}

pub unsafe extern "C" fn aot_i32_trunc_sat_f64_s(val: f64) -> i32 {
    if val.is_nan() { return 0; }
    if val >= 2147483648.0 { return i32::MAX; }
    if val <= -2147483649.0 { return i32::MIN; }
    val as i32
}

pub unsafe extern "C" fn aot_i32_trunc_sat_f64_u(val: f64) -> u32 {
    if val.is_nan() { return 0; }
    if val >= 4294967296.0 { return u32::MAX; }
    if val <= -1.0 { return 0; }
    val as u32
}

pub unsafe extern "C" fn aot_i64_trunc_sat_f32_s(val: f32) -> i64 {
    if val.is_nan() { return 0; }
    if val >= 9223372036854775808.0 { return i64::MAX; }
    if val <= -9223372036854775809.0 { return i64::MIN; }
    val as i64
}

pub unsafe extern "C" fn aot_i64_trunc_sat_f32_u(val: f32) -> u64 {
    if val.is_nan() { return 0; }
    if val >= 18446744073709551616.0 { return u64::MAX; }
    if val <= -1.0 { return 0; }
    val as u64
}

pub unsafe extern "C" fn aot_i64_trunc_sat_f64_s(val: f64) -> i64 {
    if val.is_nan() { return 0; }
    if val >= 9223372036854775808.0 { return i64::MAX; }
    if val <= -9223372036854775809.0 { return i64::MIN; }
    val as i64
}

pub unsafe extern "C" fn aot_i64_trunc_sat_f64_u(val: f64) -> u64 {
    if val.is_nan() { return 0; }
    if val >= 18446744073709551616.0 { return u64::MAX; }
    if val <= -1.0 { return 0; }
    val as u64
}





