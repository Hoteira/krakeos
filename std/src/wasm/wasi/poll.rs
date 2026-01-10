use crate::wasm::{Value, interpreter::Interpreter};
use crate::wasm::wasi::types::*;

pub fn register(interpreter: &mut Interpreter, mod_name: &str) {
    interpreter.add_host_function(mod_name, "poll_oneoff", |interp, args| {
        let _in_ptr = match args.get(0) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let _out_ptr = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let _nsubscriptions = match args.get(2) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let nevents_ptr = match args.get(3) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };

        crate::os::yield_task();

        if nevents_ptr + 4 <= interp.memory.len() {
             interp.memory[nevents_ptr..nevents_ptr+4].copy_from_slice(&0u32.to_le_bytes());
        }

        Some(Value::I32(WASI_ESUCCESS as i32))
    });

    interpreter.add_host_function(mod_name, "sock_recv", |_interp, _args| { crate::os::yield_task(); Some(Value::I32(WASI_ENOTSUP as i32)) });
    interpreter.add_host_function(mod_name, "sock_send", |_interp, _args| { crate::os::yield_task(); Some(Value::I32(WASI_ENOTSUP as i32)) });
    interpreter.add_host_function(mod_name, "sock_shutdown", |_interp, _args| { crate::os::yield_task(); Some(Value::I32(WASI_ENOTSUP as i32)) });
}
