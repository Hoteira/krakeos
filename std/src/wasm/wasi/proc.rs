use crate::wasm::{Value, interpreter::Interpreter};
use crate::wasm::wasi::types::*;

pub fn register(interpreter: &mut Interpreter, mod_name: &str) {
    interpreter.add_host_function(mod_name, "args_sizes_get", |interp, args| {
        let argc_ptr = match args.get(0) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let buf_size_ptr = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };

        let count = crate::env::args().count();
        let mut size = 0;
        for arg in crate::env::args() {
            size += arg.as_bytes().len() + 1;
        }

        if argc_ptr + 4 > interp.memory.len() || buf_size_ptr + 4 > interp.memory.len() {
            return Some(Value::I32(WASI_EFAULT as i32));
        }

        interp.memory[argc_ptr..argc_ptr+4].copy_from_slice(&(count as u32).to_le_bytes());
        interp.memory[buf_size_ptr..buf_size_ptr+4].copy_from_slice(&(size as u32).to_le_bytes());
        Some(Value::I32(WASI_ESUCCESS as i32))
    });

    interpreter.add_host_function(mod_name, "args_get", |interp, args| {
        let argv_ptr = match args.get(0) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let argv_buf_ptr = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        
        let mut offset = 0;
        let mut argv_offset = 0;
        
        for arg in crate::env::args() {
            let bytes = arg.as_bytes();
            let len = bytes.len();
            
            if argv_buf_ptr + offset + len + 1 > interp.memory.len() { return Some(Value::I32(WASI_EFAULT as i32)); }
            interp.memory[argv_buf_ptr + offset..argv_buf_ptr + offset + len].copy_from_slice(bytes);
            interp.memory[argv_buf_ptr + offset + len] = 0; 
            
            if argv_ptr + argv_offset + 4 > interp.memory.len() { return Some(Value::I32(WASI_EFAULT as i32)); }
            let ptr = (argv_buf_ptr + offset) as u32;
            interp.memory[argv_ptr + argv_offset..argv_ptr + argv_offset + 4].copy_from_slice(&ptr.to_le_bytes());
            
            offset += len + 1;
            argv_offset += 4;
        }
        Some(Value::I32(WASI_ESUCCESS as i32))
    });

    interpreter.add_host_function(mod_name, "environ_sizes_get", |interp, args| {
        let count_ptr = match args.get(0) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let size_ptr = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };

        let mut count = 0;
        let mut size = 0;
        for (k, v) in crate::env::vars() {
            count += 1;
            size += k.len() + v.len() + 2; 
        }

        if count_ptr + 4 > interp.memory.len() || size_ptr + 4 > interp.memory.len() {
             return Some(Value::I32(WASI_EFAULT as i32));
        }

        interp.memory[count_ptr..count_ptr+4].copy_from_slice(&(count as u32).to_le_bytes());
        interp.memory[size_ptr..size_ptr+4].copy_from_slice(&(size as u32).to_le_bytes());
        Some(Value::I32(WASI_ESUCCESS as i32))
    });

    interpreter.add_host_function(mod_name, "environ_get", |interp, args| {
        let environ_ptr = match args.get(0) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let environ_buf_ptr = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };

        let mut offset = 0;
        let mut env_offset = 0;
        for (k, v) in crate::env::vars() {
            let s = crate::rust_alloc::format!("{}={}", k, v);
            let bytes = s.as_bytes();
            let len = bytes.len();

            if environ_buf_ptr + offset + len + 1 > interp.memory.len() { return Some(Value::I32(WASI_EFAULT as i32)); }
            interp.memory[environ_buf_ptr + offset..environ_buf_ptr + offset + len].copy_from_slice(bytes);
            interp.memory[environ_buf_ptr + offset + len] = 0;

            if environ_ptr + env_offset + 4 > interp.memory.len() { return Some(Value::I32(WASI_EFAULT as i32)); }
            let ptr = (environ_buf_ptr + offset) as u32;
            interp.memory[environ_ptr + env_offset..environ_ptr + env_offset + 4].copy_from_slice(&ptr.to_le_bytes());

            offset += len + 1;
            env_offset += 4;
        }
        Some(Value::I32(WASI_ESUCCESS as i32))
    });

    interpreter.add_host_function(mod_name, "proc_exit", |_interp, args| {
        let code = match args.get(0) { Some(Value::I32(v)) => *v, _ => 0 };
        crate::os::exit(code as u64);
        None
    });

    interpreter.add_host_function(mod_name, "proc_raise", |_interp, _args| { crate::os::yield_task(); Some(Value::I32(WASI_ESUCCESS as i32)) });
    interpreter.add_host_function(mod_name, "sched_yield", |_interp, _args| { crate::os::yield_task(); Some(Value::I32(WASI_ESUCCESS as i32)) });
}