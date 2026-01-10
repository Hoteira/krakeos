use crate::wasm::{Value, interpreter::Interpreter};
use crate::wasm::wasi::types::*;

pub fn register(interpreter: &mut Interpreter, mod_name: &str) {
    interpreter.add_host_function(mod_name, "clock_res_get", |interp, args| {
        let _clock_id = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let res_ptr = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        
        let resolution: u64 = 1_000_000; // 1ms
        
        if res_ptr + 8 > interp.memory.len() { return Some(Value::I32(WASI_EFAULT as i32)); }
        interp.memory[res_ptr..res_ptr+8].copy_from_slice(&resolution.to_le_bytes());
        
        Some(Value::I32(WASI_ESUCCESS as i32))
    });

    interpreter.add_host_function(mod_name, "clock_time_get", |interp, args| {
        let clock_id = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let _precision = match args.get(1) { Some(Value::I64(v)) => *v, _ => 0 };
        let time_ptr = match args.get(2) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };

        let nanos = if clock_id == 0 { // CLOCK_REALTIME
            let (d, m, y) = crate::os::get_date();
            let (h, min, s) = crate::os::get_time();
            
            let mut days = 0;
            for cur_y in 1970..y {
                if (cur_y % 4 == 0 && cur_y % 100 != 0) || (cur_y % 400 == 0) {
                    days += 366;
                } else {
                    days += 365;
                }
            }
            
            let is_leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
            let days_in_months = [31, if is_leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
            
            for i in 0..(m - 1) {
                days += days_in_months[i as usize] as u64;
            }
            
            days += (d - 1) as u64;
            
            let seconds = days * 86400 + (h as u64) * 3600 + (min as u64) * 60 + (s as u64);
            seconds * 1_000_000_000
        } else {
            crate::os::get_system_ticks() * 1_000_000
        };
        
        if time_ptr + 8 > interp.memory.len() { return Some(Value::I32(WASI_EFAULT as i32)); }
        interp.memory[time_ptr..time_ptr+8].copy_from_slice(&nanos.to_le_bytes());

        Some(Value::I32(WASI_ESUCCESS as i32))
    });

    interpreter.add_host_function(mod_name, "random_get", |interp, args| {
        let buf_ptr = match args.get(0) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let buf_len = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        
        if buf_ptr + buf_len > interp.memory.len() { return Some(Value::I32(WASI_EFAULT as i32)); }
        
        let mut seed = crate::os::get_system_ticks();
        for i in 0..buf_len { 
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1); 
            interp.memory[buf_ptr + i] = (seed >> 32) as u8; 
        }

        Some(Value::I32(WASI_ESUCCESS as i32))
    });
}
