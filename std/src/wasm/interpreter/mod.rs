use crate::wasm::{WasmModule, Value, ValueType, CodeSection, FunctionType};
use crate::wasm::leb128::Leb128;
use crate::rust_alloc::vec::Vec;
use crate::rust_alloc::string::String;
use crate::rust_alloc::boxed::Box;
use crate::math::FloatMath;

pub struct Interpreter {
    pub(crate) stack: Vec<Value>,
    pub(crate) globals: Vec<Value>,
    pub(crate) memory: Vec<u8>,
    pub(crate) tables: Vec<Vec<u32>>,
    pub(crate) dropped_data: Vec<bool>,
    pub(crate) dropped_elem: Vec<bool>,
    pub(crate) host_functions: Vec<(String, String, Box<dyn Fn(&mut Interpreter, Vec<Value>) -> Option<Value>>)>,
    pub(crate) fd_paths: Vec<(usize, String)>,
}

#[derive(Debug, Clone)]
pub(crate) struct ControlFrame {
    pub opcode: u8,
    pub pc: usize,
    pub stack_depth: usize,
    pub result_type: Option<ValueType>,
}

impl Interpreter {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            globals: Vec::new(),
            memory: Vec::new(),
            tables: Vec::new(),
            dropped_data: Vec::new(),
            dropped_elem: Vec::new(),
            host_functions: Vec::new(),
            fd_paths: Vec::new(),
        }
    }

    pub fn add_host_function<F>(&mut self, module: &str, name: &str, f: F)
    where
        F: Fn(&mut Interpreter, Vec<Value>) -> Option<Value> + 'static,
    {
        self.host_functions.push((String::from(module), String::from(name), Box::new(f)));
    }

    fn init_globals(&mut self, module: &WasmModule) {
        self.globals.clear();
        for global in &module.globals {
            let mut pc = 0;
            if !global.init_bytecode.is_empty() {
                match global.init_bytecode[pc] {
                    0x41 => { pc += 1; self.globals.push(Value::I32(Leb128::decode_i32(&global.init_bytecode, &mut pc))); }
                    0x42 => { pc += 1; self.globals.push(Value::I64(Leb128::decode_i64(&global.init_bytecode, &mut pc))); }
                    0x43 => { 
                        pc += 1; 
                        let mut b = [0u8; 4]; b.copy_from_slice(&global.init_bytecode[pc..pc+4]); 
                        self.globals.push(Value::F32(f32::from_le_bytes(b))); 
                    }
                    0x44 => { 
                        pc += 1; 
                        let mut b = [0u8; 8]; b.copy_from_slice(&global.init_bytecode[pc..pc+8]); 
                        self.globals.push(Value::F64(f64::from_le_bytes(b))); 
                    }
                    _ => { self.globals.push(Value::I32(0)); }
                }
            } else { self.globals.push(Value::I32(0)); }
        }
    }

    fn init_memory(&mut self, module: &WasmModule) {
        if let Some(mem) = module.memories.first() {
            let size = mem.min_pages as usize * 64 * 1024;
            self.memory.resize(size, 0);
            for data in &module.datas {
                let mut pc = 0;
                let offset = if !data.offset_bytecode.is_empty() && data.offset_bytecode[pc] == 0x41 {
                    pc += 1; Leb128::decode_i32(&data.offset_bytecode, &mut pc) as usize
                } else { 0 };
                if offset + data.data.len() <= self.memory.len() {
                    self.memory[offset..offset + data.data.len()].copy_from_slice(&data.data);
                }
            }
        }
    }

    fn init_tables(&mut self, module: &WasmModule) {
        self.tables.clear();
        for table_def in &module.tables {
            self.tables.push(crate::rust_alloc::vec![0xFFFFFFFF; table_def.min_size as usize]);
        }
        for element in &module.elements {
            let mut pc = 0;
            let offset = if !element.offset_bytecode.is_empty() && element.offset_bytecode[pc] == 0x41 {
                pc += 1; Leb128::decode_i32(&element.offset_bytecode, &mut pc) as usize
            } else { 0 };
            let table = &mut self.tables[element.table_index as usize];
            for (i, &func_idx) in element.function_indices.iter().enumerate() {
                if offset + i < table.len() { table[offset + i] = func_idx; }
            }
        }
    }

    pub fn call(&mut self, module: &WasmModule, func_idx: u32, args: Vec<Value>) -> Result<Value, &'static str> {
        let is_first_call = self.globals.is_empty() && !module.globals.is_empty();

        if self.globals.is_empty() && !module.globals.is_empty() { self.init_globals(module); }
        if self.memory.is_empty() && !module.memories.is_empty() { self.init_memory(module); }
        if self.tables.is_empty() && !module.tables.is_empty() { self.init_tables(module); }
        if self.dropped_data.is_empty() { self.dropped_data.resize(module.datas.len(), false); }
        if self.dropped_elem.is_empty() { self.dropped_elem.resize(module.elements.len(), false); }

        if is_first_call {
            if let Some(start_idx) = module.start_func {
                self.call(module, start_idx, Vec::new())?;
            }
        }

        let mut func_import_count = 0;
        let mut target_import = None;
        for import in &module.imports {
            if import.kind == 0 {
                if func_import_count == func_idx {
                    target_import = Some(import);
                    break;
                }
                func_import_count += 1;
            }
        }

        if let Some(import) = target_import {
            let host_fn_idx = self.host_functions.iter().position(|(m, n, _)| m == &import.module && n == &import.name);
            if let Some(idx) = host_fn_idx {
                let host_fn_ptr = &self.host_functions[idx].2 as *const Box<dyn Fn(&mut Interpreter, Vec<Value>) -> Option<Value>>;
                let host_fn = unsafe { &*host_fn_ptr };
                return Ok(host_fn(self, args).unwrap_or(Value::I32(0)));
            } else {
                crate::debugln!("WASM: Host not found: {}:{}", import.module, import.name);
                return Err("Host not found");
            }
        }

        let internal_idx = func_idx.checked_sub(func_import_count).ok_or("Func OOB")?;
        let type_idx = *module.functions.get(internal_idx as usize).ok_or("Func OOB")?;
        let func_type = &module.types[type_idx as usize];
        let code = &module.codes[internal_idx as usize];

        let mut locals = Vec::new();
        locals.extend(args);
        for entry in &code.locals {
            for _ in 0..entry.count {
                match entry.val_type {
                    ValueType::I32 => locals.push(Value::I32(0)),
                    ValueType::I64 => locals.push(Value::I64(0)),
                    ValueType::F32 => locals.push(Value::F32(0.0)),
                    ValueType::F64 => locals.push(Value::F64(0.0)),
                }
            }
        }

        let base_stack_depth = self.stack.len();
        let mut control_stack: Vec<ControlFrame> = Vec::new();
        
        // Push implicit frame for the function body
        let func_rt = if func_type.results.is_empty() { None } else { Some(func_type.results[0]) };
        control_stack.push(ControlFrame {
            opcode: 0x00, // Pseudo-opcode for function body
            pc: 0,
            stack_depth: base_stack_depth,
            result_type: func_rt,
        });

        let mut pc = 0;
        let bytecode = &code.bytecode;

        while pc < bytecode.len() {
            let opcode = bytecode[pc];
            pc += 1;

            match opcode {
                0x00 => return Err("unreachable"),
                0x01 => {} // nop
                0x02..=0x05 | 0x0C..=0x11 => self.execute_control(opcode, &mut pc, bytecode, &mut control_stack, module, &mut locals)?,
                0x1A..=0x1C => self.execute_variable(opcode, &mut pc, bytecode, &mut locals)?,
                0x20..=0x26 => self.execute_variable(opcode, &mut pc, bytecode, &mut locals)?,
                0x28..=0x40 => self.execute_memory(opcode, &mut pc, bytecode)?,
                0x41..=0xA6 => self.execute_numeric(opcode, &mut pc, bytecode)?,
                0xA7..=0xC4 => self.execute_conversion(opcode, &mut pc, bytecode)?,
                0xFC => self.execute_misc(opcode, &mut pc, bytecode, module)?,
                0x0B => { // end
                    if let Some(frame) = control_stack.pop() {
                        if let Some(_rt) = frame.result_type {
                            let res = self.stack.pop().ok_or("result missing")?;
                            self.stack.truncate(frame.stack_depth);
                            self.stack.push(res);
                        } else { 
                            self.stack.truncate(frame.stack_depth); 
                        }
                        if control_stack.is_empty() { break; } // Finished function body
                    } else { break; }
                }
                _ => { crate::debugln!("Op {:#x} error", opcode); return Err("unimplemented"); }
            }
        }
        let result = if func_type.results.is_empty() { Value::I32(0) } else { self.stack.pop().unwrap_or(Value::I32(0)) };
        self.stack.truncate(base_stack_depth);
        Ok(result)
    }

    pub(crate) fn get_mem_addr(&mut self, bytecode: &[u8], pc: &mut usize, size: usize) -> Result<usize, &'static str> {
        let _align = Leb128::decode_u32(bytecode, pc);
        let offset = Leb128::decode_u32(bytecode, pc);
        let base = self.pop_i32()? as u32;
        let addr = base.wrapping_add(offset) as usize;
        if addr.saturating_add(size) > self.memory.len() { return Err("Mem OOB"); }
        Ok(addr)
    }

    pub(crate) fn skip_instruction(&self, bytecode: &[u8], pc: &mut usize) -> Result<(), &'static str> {
        let op = bytecode[*pc]; *pc += 1;
        match op {
            0x02 | 0x03 | 0x04 => { Leb128::decode_i64(bytecode, pc); }
            0x0C | 0x0D | 0x10 | 0x20..=0x24 => { Leb128::decode_u32(bytecode, pc); }
            0x41 => { Leb128::decode_i32(bytecode, pc); }
            0x42 => { Leb128::decode_i64(bytecode, pc); }
            0x43 => { *pc += 4; }
            0x44 => { *pc += 8; }
            0x28..=0x3E => { Leb128::decode_u32(bytecode, pc); Leb128::decode_u32(bytecode, pc); }
            0x11 => { Leb128::decode_u32(bytecode, pc); Leb128::decode_u32(bytecode, pc); }
            0x0E => { 
                let cnt = Leb128::decode_u32(bytecode, pc); 
                for _ in 0..cnt { Leb128::decode_u32(bytecode, pc); } 
                Leb128::decode_u32(bytecode, pc); 
            }
            0x3F | 0x40 => { Leb128::decode_u32(bytecode, pc); }
            0xFC => {
                let sub = Leb128::decode_u32(bytecode, pc);
                match sub {
                    8 => { Leb128::decode_u32(bytecode, pc); Leb128::decode_u32(bytecode, pc); }
                    9 => { Leb128::decode_u32(bytecode, pc); }
                    10 => { Leb128::decode_u32(bytecode, pc); Leb128::decode_u32(bytecode, pc); }
                    11 => { Leb128::decode_u32(bytecode, pc); }
                    12 => { Leb128::decode_u32(bytecode, pc); Leb128::decode_u32(bytecode, pc); }
                    13 => { Leb128::decode_u32(bytecode, pc); }
                    14 => { Leb128::decode_u32(bytecode, pc); Leb128::decode_u32(bytecode, pc); }
                    15 | 16 | 17 => { Leb128::decode_u32(bytecode, pc); }
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn find_block_ends(&self, start_pc: usize, bytecode: &[u8]) -> Result<(Option<usize>, usize), &'static str> {
        let mut depth = 1; let mut pc = start_pc; let mut else_pc = None;
        while pc < bytecode.len() {
            let op = bytecode[pc];
            match op {
                0x02 | 0x03 | 0x04 => { depth += 1; pc += 1; Leb128::decode_i64(bytecode, &mut pc); }
                0x05 if depth == 1 => { else_pc = Some(pc); pc += 1; }
                0x0B => { depth -= 1; if depth == 0 { return Ok((else_pc, pc + 1)); } pc += 1; }
                _ => { self.skip_instruction(bytecode, &mut pc)?; }
            }
        }
        Err("end missing")
    }

    pub(crate) fn get_type_idx(&self, module: &WasmModule, func_idx: u32) -> Result<u32, &'static str> {
        let mut current_func_idx = 0;
        for import in &module.imports {
            if import.kind == 0 {
                if current_func_idx == func_idx {
                    return Ok(import.index);
                }
                current_func_idx += 1;
            }
        }
        let internal_idx = func_idx.checked_sub(current_func_idx).ok_or("Func OOB")?;
        Ok(*module.functions.get(internal_idx as usize).ok_or("Func OOB")?)
    }

    pub(crate) fn get_arg_count(&self, module: &WasmModule, func_idx: u32) -> Result<usize, &'static str> {
        let type_idx = self.get_type_idx(module, func_idx)?;
        Ok(module.types[type_idx as usize].params.len())
    }

    pub(crate) fn nearest_f32(&self, x: f32) -> f32 { let f = FloatMath::floor(x); let d = x - f; if d < 0.5 { f } else if d > 0.5 { f + 1.0 } else if f % 2.0 == 0.0 { f } else { f + 1.0 } }
    pub(crate) fn nearest_f64(&self, x: f64) -> f64 { let f = FloatMath::floor(x); let d = x - f; if d < 0.5 { f } else if d > 0.5 { f + 1.0 } else if f % 2.0 == 0.0 { f } else { f + 1.0 } }
    pub(crate) fn copysign_f32(&self, x: f32, y: f32) -> f32 { let xb = x.to_bits(); let yb = y.to_bits(); f32::from_bits((xb & 0x7FFFFFFF) | (yb & 0x80000000)) }
    pub(crate) fn copysign_f64(&self, x: f64, y: f64) -> f64 { let xb = x.to_bits(); let yb = y.to_bits(); f64::from_bits((xb & 0x7FFFFFFFFFFFFFFF) | (yb & 0x8000000000000000)) }

    pub(crate) fn min_f32(&self, a: f32, b: f32) -> f32 {
        if a.is_nan() || b.is_nan() { return f32::NAN; }
        if a == 0.0 && b == 0.0 {
            if a.is_sign_negative() || b.is_sign_negative() { return -0.0; }
            return 0.0;
        }
        if a < b { a } else { b }
    }

    pub(crate) fn max_f32(&self, a: f32, b: f32) -> f32 {
        if a.is_nan() || b.is_nan() { return f32::NAN; }
        if a == 0.0 && b == 0.0 {
            if a.is_sign_positive() || b.is_sign_positive() { return 0.0; }
            return -0.0;
        }
        if a > b { a } else { b }
    }

    pub(crate) fn min_f64(&self, a: f64, b: f64) -> f64 {
        if a.is_nan() || b.is_nan() { return f64::NAN; }
        if a == 0.0 && b == 0.0 {
            if a.is_sign_negative() || b.is_sign_negative() { return -0.0; }
            return 0.0;
        }
        if a < b { a } else { b }
    }

    pub(crate) fn max_f64(&self, a: f64, b: f64) -> f64 {
        if a.is_nan() || b.is_nan() { return f64::NAN; }
        if a == 0.0 && b == 0.0 {
            if a.is_sign_positive() || b.is_sign_positive() { return 0.0; }
            return -0.0;
        }
        if a > b { a } else { b }
    }

    pub fn read_memory_string(&self, addr: usize, len: usize) -> String { String::from_utf8_lossy(&self.memory[addr..addr+len]).into_owned() }
    pub(crate) fn pop_i32(&mut self) -> Result<i32, &'static str> { match self.stack.pop() { Some(Value::I32(v)) => Ok(v), _ => Err("pop i32 err"), } }
    pub(crate) fn pop_i64(&mut self) -> Result<i64, &'static str> { match self.stack.pop() { Some(Value::I64(v)) => Ok(v), _ => Err("pop i64 err"), } }
    pub(crate) fn pop_f32(&mut self) -> Result<f32, &'static str> { match self.stack.pop() { Some(Value::F32(v)) => Ok(v), _ => Err("pop f32 err"), } }
    pub(crate) fn pop_f64(&mut self) -> Result<f64, &'static str> { match self.stack.pop() { Some(Value::F64(v)) => Ok(v), _ => Err("pop f64 err"), } }
}

mod control;
mod variable;
mod memory;
mod numeric;
mod conversion;
mod misc;
