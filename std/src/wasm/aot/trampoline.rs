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




