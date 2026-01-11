use super::runtime::{Store, Stack, Frame, Value, FuncInstance, ExternalVal, WasmLabel};
use super::types::ValType;
use crate::rust_alloc::vec::Vec;
use crate::rust_alloc::rc::Rc;
use crate::rust_alloc::vec;
use crate::math::FloatMath;

pub struct Interpreter<'a> {
    pub store: &'a mut Store,
    pub stack: &'a mut Stack,
}

impl<'a> Interpreter<'a> {
    pub fn new(store: &'a mut Store, stack: &'a mut Stack) -> Self {
        Self { store, stack }
    }

    pub fn invoke(&mut self, func_addr: u32, args: Vec<Value>) -> Result<Vec<Value>, &'static str> {
        self.execute_call_frame(func_addr, Some(args))?;
        self.run_loop()
    }

    fn read_leb_u32(code: &[u8], ip: &mut usize) -> Result<u32, &'static str> {
        let mut res: u32 = 0; let mut shift = 0;
        loop {
            if *ip >= code.len() { return Err("EOF u32"); }
            let b = code[*ip]; *ip += 1;
            res |= ((b & 0x7F) as u32) << shift;
            if (b & 0x80) == 0 { break; } shift += 7;
        }
        Ok(res)
    }

    fn read_leb_i32(code: &[u8], ip: &mut usize) -> Result<i32, &'static str> {
        let mut res: i32 = 0; let mut shift = 0; let mut b: u8;
        loop {
            if *ip >= code.len() { return Err("EOF i32"); }
            b = code[*ip]; *ip += 1;
            res |= ((b & 0x7F) as i32) << shift;
            shift += 7; if (b & 0x80) == 0 { break; }
        }
        if shift < 32 && (b & 0x40) != 0 { res |= (-1i32) << shift; }
        Ok(res)
    }

    fn read_leb_i64(code: &[u8], ip: &mut usize) -> Result<i64, &'static str> {
        let mut res: i64 = 0; let mut shift = 0; let mut b: u8;
        loop {
            if *ip >= code.len() { return Err("EOF i64"); }
            b = code[*ip]; *ip += 1;
            res |= ((b & 0x7F) as i64) << shift;
            shift += 7; if (b & 0x80) == 0 { break; }
        }
        if shift < 64 && (b & 0x40) != 0 { res |= (-1i64) << shift; }
        Ok(res)
    }

    fn skip_immediates(code: &[u8], ip: &mut usize, op: u8) -> Result<(), &'static str> {
        match op {
            0x02 | 0x03 | 0x04 => { Self::read_leb_i32(code, ip)?; },
            0x0C | 0x0D => { Self::read_leb_u32(code, ip)?; },
            0x0E => {
                let count = Self::read_leb_u32(code, ip)?;
                for _ in 0..count { Self::read_leb_u32(code, ip)?; }
                Self::read_leb_u32(code, ip)?;
            },
            0x10 => { Self::read_leb_u32(code, ip)?; },
            0x11 => { Self::read_leb_u32(code, ip)?; Self::read_leb_u32(code, ip)?; },
            0x20..=0x24 => { Self::read_leb_u32(code, ip)?; },
            0x28..=0x3E => { Self::read_leb_u32(code, ip)?; Self::read_leb_u32(code, ip)?; },
            0x3F | 0x40 => { Self::read_leb_u32(code, ip)?; },
            0x41 => { Self::read_leb_i32(code, ip)?; },
            0x42 => { Self::read_leb_i64(code, ip)?; },
            0x43 => { *ip += 4; }, 0x44 => { *ip += 8; },
            0xD2 => { Self::read_leb_u32(code, ip)?; },
            0xFC => {
                let sub_op = Self::read_leb_u32(code, ip)?;
                match sub_op {
                    8 | 10 | 12 | 14 => { Self::read_leb_u32(code, ip)?; Self::read_leb_u32(code, ip)?; if sub_op == 14 { Self::read_leb_u32(code, ip)?; } },
                    9 | 11 | 13 | 15..=17 => { Self::read_leb_u32(code, ip)?; },
                    _ => {}
                }
            },
            _ => {}
        }
        Ok(())
    }

    fn find_end(code: &[u8], mut ip: usize) -> Result<usize, &'static str> {
        let mut depth = 1;
        while depth > 0 {
            if ip >= code.len() { return Err("EOF end"); }
            let op = code[ip]; ip += 1;
            match op {
                0x02 | 0x03 | 0x04 => { depth += 1; Self::skip_immediates(code, &mut ip, op)?; },
                0x0B => depth -= 1,
                _ => Self::skip_immediates(code, &mut ip, op)?,
            }
        }
        Ok(ip - 1)
    }

    fn find_else_or_end(code: &[u8], mut ip: usize) -> Result<usize, &'static str> {
        let mut depth = 1;
        while depth > 0 {
            if ip >= code.len() { return Err("EOF else"); }
            let op = code[ip]; ip += 1;
            match op {
                0x02 | 0x03 | 0x04 => { depth += 1; Self::skip_immediates(code, &mut ip, op)?; },
                0x0B => depth -= 1,
                0x05 if depth == 1 => return Ok(ip - 1),
                _ => Self::skip_immediates(code, &mut ip, op)?,
            }
        }
        Ok(ip - 1)
    }

    fn pop_i32(&mut self) -> Result<i32, &'static str> { match self.stack.values.pop().ok_or("under i32")? { Value::I32(v) => Ok(v), _ => Err("tm i32") } }
    fn pop_i64(&mut self) -> Result<i64, &'static str> { match self.stack.values.pop().ok_or("under i64")? { Value::I64(v) => Ok(v), _ => Err("tm i64") } }
    fn pop_f32(&mut self) -> Result<f32, &'static str> { match self.stack.values.pop().ok_or("under f32")? { Value::F32(v) => Ok(v), _ => Err("tm f32") } }
    fn pop_f64(&mut self) -> Result<f64, &'static str> { match self.stack.values.pop().ok_or("under f64")? { Value::F64(v) => Ok(v), _ => Err("tm f64") } }

    fn run_loop(&mut self) -> Result<Vec<Value>, &'static str> {
        loop {
            if self.stack.frames.is_empty() { return Ok(Vec::new()); }
            let (op, mut current_ip, frame_code, inst_start_ip);
            {
                let frame = self.stack.frames.last().unwrap();
                frame_code = frame.code.clone();
                inst_start_ip = frame.ip;
                if frame.ip >= frame.code.len() { op = 0x0B; current_ip = frame.ip; }
                else { op = frame.code[frame.ip]; current_ip = frame.ip + 1; }
            }
            self.stack.frames.last_mut().unwrap().ip = current_ip;

            match op {
                0x00 => return Err("unreachable trap"),
                0x01 => {}, // nop
                0x02 | 0x03 | 0x04 => {
                    let bt = Self::read_leb_i32(&frame_code, &mut current_ip)?;
                    let end_ip = Self::find_end(&frame_code, current_ip)?;
                    let arity = if bt == -0x40 { 0 } else { 1 };
                    self.stack.frames.last_mut().unwrap().ip = current_ip;
                    if op == 0x04 {
                        let cond = self.pop_i32()? != 0;
                        self.stack.frames.last_mut().unwrap().labels.push(WasmLabel { target_ip: end_ip, stack_height: self.stack.values.len(), arity });
                        if !cond {
                            let frame = self.stack.frames.last_mut().unwrap();
                            frame.ip = Self::find_else_or_end(&frame_code, frame.ip)?;
                            if frame.code[frame.ip] == 0x05 { frame.ip += 1; }
                        }
                    } else if op == 0x02 { 
                        self.stack.frames.last_mut().unwrap().labels.push(WasmLabel { target_ip: end_ip, stack_height: self.stack.values.len(), arity }); 
                    } else { 
                        // Loop: target is the START of the loop instruction
                        self.stack.frames.last_mut().unwrap().labels.push(WasmLabel { target_ip: inst_start_ip, stack_height: self.stack.values.len(), arity: 0 }); 
                    }
                },
                0x05 => { let label = self.stack.frames.last().unwrap().labels.last().ok_or("else fail")?.clone(); self.stack.frames.last_mut().unwrap().ip = label.target_ip; },
                0x0B => {
                    if !self.stack.frames.last().unwrap().labels.is_empty() { self.stack.frames.last_mut().unwrap().labels.pop(); }
                    else {
                        let f = self.stack.frames.pop().unwrap();
                        let mut res = Vec::new(); for _ in 0..f.return_arity { res.push(self.stack.values.pop().ok_or("under ret")?); } res.reverse();
                        if self.stack.frames.is_empty() { return Ok(res); } else { for v in res { self.stack.values.push(v); } }
                    }
                },
                0x0C | 0x0D => {
                    let idx = Self::read_leb_u32(&frame_code, &mut current_ip)?;
                    self.stack.frames.last_mut().unwrap().ip = current_ip;
                    let cond = if op == 0x0D { self.pop_i32()? != 0 } else { true };
                    if cond {
                        let label = self.stack.frames.last().unwrap().labels.iter().rev().nth(idx as usize).ok_or("bad br")?.clone();
                        let mut res_vals = Vec::new(); for _ in 0..label.arity { res_vals.push(self.stack.values.pop().ok_or("under br res")?); }
                        for _ in 0..=idx { self.stack.frames.last_mut().unwrap().labels.pop(); }
                        self.stack.values.truncate(label.stack_height);
                        for v in res_vals.into_iter().rev() { self.stack.values.push(v); }
                        self.stack.frames.last_mut().unwrap().ip = label.target_ip;
                        continue;
                    }
                },
                0x0E => {
                    let mut targets = Vec::new(); let d_target;
                    { let frame = self.stack.frames.last_mut().unwrap(); let count = Self::read_leb_u32(&frame.code, &mut frame.ip)?; for _ in 0..count { targets.push(Self::read_leb_u32(&frame.code, &mut frame.ip)?); } d_target = Self::read_leb_u32(&frame.code, &mut frame.ip)?; }
                    self.stack.frames.last_mut().unwrap().ip = current_ip;
                    let index = self.pop_i32()? as usize;
                    let t_idx = if index < targets.len() { targets[index] } else { d_target };
                    let label = self.stack.frames.last().unwrap().labels.iter().rev().nth(t_idx as usize).ok_or("bad br_t")?.clone();
                    let mut res_vals = Vec::new(); for _ in 0..label.arity { res_vals.push(self.stack.values.pop().ok_or("under br_t res")?); }
                    for _ in 0..=t_idx { self.stack.frames.last_mut().unwrap().labels.pop(); }
                    self.stack.values.truncate(label.stack_height);
                    for v in res_vals.into_iter().rev() { self.stack.values.push(v); }
                    self.stack.frames.last_mut().unwrap().ip = label.target_ip;
                    continue;
                },
                0x0F => { self.stack.frames.last_mut().unwrap().labels.clear(); let f = self.stack.frames.pop().unwrap(); let mut res = Vec::new(); for _ in 0..f.return_arity { res.push(self.stack.values.pop().ok_or("under ret instr")?); } res.reverse(); if self.stack.frames.is_empty() { return Ok(res); } else { for v in res { self.stack.values.push(v); } continue; } },
                0x10 => { let f_idx = Self::read_leb_u32(&frame_code, &mut current_ip)?; self.stack.frames.last_mut().unwrap().ip = current_ip; let f_addr = self.stack.frames.last().unwrap().module.func_addrs[f_idx as usize]; self.execute_call_frame(f_addr, None)?; },
                0x11 => {
                    let _t_idx = Self::read_leb_u32(&frame_code, &mut current_ip)?;
                    let tbl_idx = Self::read_leb_u32(&frame_code, &mut current_ip)?;
                    self.stack.frames.last_mut().unwrap().ip = current_ip;
                    let idx = self.pop_i32()? as usize;
                    let f_addr = { let frame = self.stack.frames.last().unwrap(); let tbl_addr = frame.module.table_addrs[tbl_idx as usize]; self.store.tables[tbl_addr as usize].elements[idx].ok_or("null indirect")? };
                    self.execute_call_frame(f_addr, None)?;
                },
                0x1A => { self.stack.values.pop().ok_or("under drop")?; },
                0x1B => { let cond = self.pop_i32()? != 0; let v2 = self.stack.values.pop().ok_or("under select v2")?; let v1 = self.stack.values.pop().ok_or("under select v1")?; self.stack.values.push(if cond { v1 } else { v2 }); },
                0x20..=0x22 => {
                    let idx = Self::read_leb_u32(&frame_code, &mut current_ip)? as usize;
                    self.stack.frames.last_mut().unwrap().ip = current_ip;
                    if op == 0x20 { let v = self.stack.frames.last().unwrap().locals[idx]; self.stack.values.push(v); }
                    else if op == 0x21 { let v = self.stack.values.pop().ok_or("pop local.set")?; self.stack.frames.last_mut().unwrap().locals[idx] = v; }
                    else { let v = *self.stack.values.last().ok_or("pop local.tee")?; self.stack.frames.last_mut().unwrap().locals[idx] = v; }
                },
                0x23 | 0x24 => {
                    let idx = Self::read_leb_u32(&frame_code, &mut current_ip)? as usize;
                    self.stack.frames.last_mut().unwrap().ip = current_ip;
                    let addr = self.stack.frames.last().unwrap().module.global_addrs[idx];
                    if op == 0x23 { self.stack.values.push(self.store.globals[addr as usize].value); }
                    else { let v = self.stack.values.pop().ok_or("pop global.set")?; self.store.globals[addr as usize].value = v; }
                },
                0x28..=0x3E => {
                    let _align = Self::read_leb_u32(&frame_code, &mut current_ip)?;
                    let offset = Self::read_leb_u32(&frame_code, &mut current_ip)?;
                    self.stack.frames.last_mut().unwrap().ip = current_ip;
                    let mem_idx = self.stack.frames.last().unwrap().module.mem_addrs[0] as usize;
                    if op >= 0x36 {
                        let v = match op {
                            0x36 | 0x3A | 0x3B => Value::I32(self.pop_i32()?),
                            0x37 | 0x3C | 0x3D | 0x3E => Value::I64(self.pop_i64()?),
                            0x38 => Value::F32(self.pop_f32()?),
                            0x39 => Value::F64(self.pop_f64()?),
                            _ => return Err("Invalid store op value pop"),
                        };
                        let b = self.pop_i32()?; let addr = (b as u32).wrapping_add(offset) as usize;
                        let mem = &mut self.store.memories[mem_idx];
                        if addr >= mem.data.len() { return Err("OOB store base"); }
                        match (op, v) {
                            (0x36, Value::I32(v)) => { if addr + 4 > mem.data.len() { return Err("OOB i32 store"); } mem.data[addr..addr+4].copy_from_slice(&v.to_le_bytes()); },
                            (0x37, Value::I64(v)) => { if addr + 8 > mem.data.len() { return Err("OOB i64 store"); } mem.data[addr..addr+8].copy_from_slice(&v.to_le_bytes()); },
                            (0x38, Value::F32(v)) => { if addr + 4 > mem.data.len() { return Err("OOB f32 store"); } mem.data[addr..addr+4].copy_from_slice(&v.to_bits().to_le_bytes()); },
                            (0x39, Value::F64(v)) => { if addr + 8 > mem.data.len() { return Err("OOB f64 store"); } mem.data[addr..addr+8].copy_from_slice(&v.to_bits().to_le_bytes()); },
                            (0x3A, Value::I32(v)) => { mem.data[addr] = v as u8; },
                            (0x3B, Value::I32(v)) => { if addr + 2 > mem.data.len() { return Err("OOB i32 store16"); } mem.data[addr..addr+2].copy_from_slice(&(v as u16).to_le_bytes()); },
                            (0x3C, Value::I64(v)) => { mem.data[addr] = v as u8; },
                            (0x3D, Value::I64(v)) => { if addr + 2 > mem.data.len() { return Err("OOB i64 store16"); } mem.data[addr..addr+2].copy_from_slice(&(v as u16).to_le_bytes()); },
                            (0x3E, Value::I64(v)) => { if addr + 4 > mem.data.len() { return Err("OOB i64 store32"); } mem.data[addr..addr+4].copy_from_slice(&(v as u32).to_le_bytes()); },
                            _ => return Err("Invalid store op/value mismatch"),
                        }
                    } else {
                        let b = self.pop_i32()?; let addr = (b as u32).wrapping_add(offset) as usize;
                        let mem = &self.store.memories[mem_idx]; if addr >= mem.data.len() { return Err("OOB load base"); }
                        match op {
                            0x28 => { if addr + 4 > mem.data.len() { return Err("OOB i32 load"); } self.stack.values.push(Value::I32(i32::from_le_bytes(mem.data[addr..addr+4].try_into().unwrap()))); },
                            0x29 => { if addr + 8 > mem.data.len() { return Err("OOB i64 load"); } self.stack.values.push(Value::I64(i64::from_le_bytes(mem.data[addr..addr+8].try_into().unwrap()))); },
                            0x2A => { if addr + 4 > mem.data.len() { return Err("OOB f32 load"); } self.stack.values.push(Value::F32(f32::from_bits(u32::from_le_bytes(mem.data[addr..addr+4].try_into().unwrap())))); },
                            0x2B => { if addr + 8 > mem.data.len() { return Err("OOB f64 load"); } self.stack.values.push(Value::F64(f64::from_bits(u64::from_le_bytes(mem.data[addr..addr+8].try_into().unwrap())))); },
                            0x2D => self.stack.values.push(Value::I32(mem.data[addr] as i32)),
                            0x2E => self.stack.values.push(Value::I32(mem.data[addr] as i8 as i32)),
                            0x2F => { if addr + 2 > mem.data.len() { return Err("OOB u16 load"); } self.stack.values.push(Value::I32(u16::from_le_bytes(mem.data[addr..addr+2].try_into().unwrap()) as i32)); },
                            0x30 => { if addr + 2 > mem.data.len() { return Err("OOB s16 load"); } self.stack.values.push(Value::I32(i16::from_le_bytes(mem.data[addr..addr+2].try_into().unwrap()) as i32)); },
                            0x31 => self.stack.values.push(Value::I64(mem.data[addr] as i8 as i64)),
                            0x32 => self.stack.values.push(Value::I64(mem.data[addr] as i64)),
                            0x33 => { if addr + 2 > mem.data.len() { return Err("OOB s16 load i64"); } self.stack.values.push(Value::I64(i16::from_le_bytes(mem.data[addr..addr+2].try_into().unwrap()) as i64)); },
                            0x34 => { if addr + 2 > mem.data.len() { return Err("OOB u16 load i64"); } self.stack.values.push(Value::I64(u16::from_le_bytes(mem.data[addr..addr+2].try_into().unwrap()) as i64)); },
                            0x35 => { if addr + 4 > mem.data.len() { return Err("OOB s32 load i64"); } self.stack.values.push(Value::I64(i32::from_le_bytes(mem.data[addr..addr+4].try_into().unwrap()) as i64)); },
                            0x36 => { if addr + 4 > mem.data.len() { return Err("OOB u32 load i64"); } self.stack.values.push(Value::I64(u32::from_le_bytes(mem.data[addr..addr+4].try_into().unwrap()) as i64)); },
                            _ => return Err("Invalid load op"),
                        }
                    }
                },
                0x3F | 0x40 => {
                    let _r = Self::read_leb_u32(&frame_code, &mut current_ip)?; self.stack.frames.last_mut().unwrap().ip = current_ip;
                    let mem_idx = self.stack.frames.last().unwrap().module.mem_addrs[0] as usize;
                    if op == 0x3F { let size = (self.store.memories[mem_idx].data.len() / 65536) as i32; self.stack.values.push(Value::I32(size)); }
                    else { let n = self.pop_i32()?; let old = (self.store.memories[mem_idx].data.len() / 65536) as i32; self.store.memories[mem_idx].data.resize((old + n) as usize * 65536, 0); self.stack.values.push(Value::I32(old)); }
                },
                0x41 => { let v = Self::read_leb_i32(&frame_code, &mut current_ip)?; self.stack.frames.last_mut().unwrap().ip = current_ip; self.stack.values.push(Value::I32(v)); },
                0x42 => { let v = Self::read_leb_i64(&frame_code, &mut current_ip)?; self.stack.frames.last_mut().unwrap().ip = current_ip; self.stack.values.push(Value::I64(v)); },
                0x43 => { let mut bytes = [0u8; 4]; bytes.copy_from_slice(&frame_code[current_ip..current_ip+4]); current_ip += 4; self.stack.frames.last_mut().unwrap().ip = current_ip; self.stack.values.push(Value::F32(f32::from_le_bytes(bytes))); },
                0x44 => { let mut bytes = [0u8; 8]; bytes.copy_from_slice(&frame_code[current_ip..current_ip+8]); current_ip += 8; self.stack.frames.last_mut().unwrap().ip = current_ip; self.stack.values.push(Value::F64(f64::from_le_bytes(bytes))); },
                0x45..=0xBF => {
                    match op {
                        0x45 => { let v = self.pop_i32()?; self.stack.values.push(Value::I32(if v == 0 {1} else {0})); },
                        0x46 => { let b = self.pop_i32()?; let a = self.pop_i32()?; self.stack.values.push(Value::I32(if a == b {1} else {0})); },
                        0x47 => { let b = self.pop_i32()?; let a = self.pop_i32()?; self.stack.values.push(Value::I32(if a != b {1} else {0})); },
                        0x48 => { let b = self.pop_i32()?; let a = self.pop_i32()?; self.stack.values.push(Value::I32(if a < b {1} else {0})); },
                        0x49 => { let b = self.pop_i32()? as u32; let a = self.pop_i32()? as u32; self.stack.values.push(Value::I32(if a < b {1} else {0})); },
                        0x4A => { let b = self.pop_i32()?; let a = self.pop_i32()?; self.stack.values.push(Value::I32(if a > b {1} else {0})); },
                        0x4B => { let b = self.pop_i32()? as u32; let a = self.pop_i32()? as u32; self.stack.values.push(Value::I32(if a > b {1} else {0})); },
                        0x4C => { let b = self.pop_i32()?; let a = self.pop_i32()?; self.stack.values.push(Value::I32(if a <= b {1} else {0})); },
                        0x4D => { let b = self.pop_i32()? as u32; let a = self.pop_i32()? as u32; self.stack.values.push(Value::I32(if a <= b {1} else {0})); },
                        0x4E => { let b = self.pop_i32()?; let a = self.pop_i32()?; self.stack.values.push(Value::I32(if a >= b {1} else {0})); },
                        0x4F => { let b = self.pop_i32()? as u32; let a = self.pop_i32()? as u32; self.stack.values.push(Value::I32(if a >= b {1} else {0})); },
                        0x50 => { let v = self.pop_i64()?; self.stack.values.push(Value::I32(if v == 0 {1} else {0})); },
                        0x51 => { let b = self.pop_i64()?; let a = self.pop_i64()?; self.stack.values.push(Value::I32(if a == b {1} else {0})); },
                        0x52 => { let b = self.pop_i64()?; let a = self.pop_i64()?; self.stack.values.push(Value::I32(if a != b {1} else {0})); },
                        0x53 => { let b = self.pop_i64()?; let a = self.pop_i64()?; self.stack.values.push(Value::I32(if a < b {1} else {0})); },
                        0x54 => { let b = self.pop_i64()? as u64; let a = self.pop_i64()? as u64; self.stack.values.push(Value::I32(if a < b {1} else {0})); },
                        0x55 => { let b = self.pop_i64()?; let a = self.pop_i64()?; self.stack.values.push(Value::I32(if a > b {1} else {0})); },
                        0x56 => { let b = self.pop_i64()? as u64; let a = self.pop_i64()? as u64; self.stack.values.push(Value::I32(if a > b {1} else {0})); },
                        0x57 => { let b = self.pop_i64()?; let a = self.pop_i64()?; self.stack.values.push(Value::I32(if a <= b {1} else {0})); },
                        0x58 => { let b = self.pop_i64()? as u64; let a = self.pop_i64()? as u64; self.stack.values.push(Value::I32(if a <= b {1} else {0})); },
                        0x59 => { let b = self.pop_i64()?; let a = self.pop_i64()?; self.stack.values.push(Value::I32(if a >= b {1} else {0})); },
                        0x5A => { let b = self.pop_i64()? as u64; let a = self.pop_i64()? as u64; self.stack.values.push(Value::I32(if a >= b {1} else {0})); },
                        0x5B => { let b = self.pop_f32()?; let a = self.pop_f32()?; self.stack.values.push(Value::I32(if a == b {1} else {0})); },
                        0x5C => { let b = self.pop_f32()?; let a = self.pop_f32()?; self.stack.values.push(Value::I32(if a != b {1} else {0})); },
                        0x5D => { let b = self.pop_f32()?; let a = self.pop_f32()?; self.stack.values.push(Value::I32(if a < b {1} else {0})); },
                        0x5E => { let b = self.pop_f32()?; let a = self.pop_f32()?; self.stack.values.push(Value::I32(if a > b {1} else {0})); },
                        0x5F => { let b = self.pop_f32()?; let a = self.pop_f32()?; self.stack.values.push(Value::I32(if a <= b {1} else {0})); },
                        0x60 => { let b = self.pop_f32()?; let a = self.pop_f32()?; self.stack.values.push(Value::I32(if a >= b {1} else {0})); },
                        0x61 => { let b = self.pop_f64()?; let a = self.pop_f64()?; self.stack.values.push(Value::I32(if a == b {1} else {0})); },
                        0x62 => { let b = self.pop_f64()?; let a = self.pop_f64()?; self.stack.values.push(Value::I32(if a != b {1} else {0})); },
                        0x63 => { let b = self.pop_f64()?; let a = self.pop_f64()?; self.stack.values.push(Value::I32(if a < b {1} else {0})); },
                        0x64 => { let b = self.pop_f64()?; let a = self.pop_f64()?; self.stack.values.push(Value::I32(if a > b {1} else {0})); },
                        0x65 => { let b = self.pop_f64()?; let a = self.pop_f64()?; self.stack.values.push(Value::I32(if a <= b {1} else {0})); },
                        0x66 => { let b = self.pop_f64()?; let a = self.pop_f64()?; self.stack.values.push(Value::I32(if a >= b {1} else {0})); },
                        0x67..=0x69 => { let v = self.pop_i32()?; if op == 0x67 { self.stack.values.push(Value::I32(v.count_ones() as i32)); } else if op == 0x68 { self.stack.values.push(Value::I32(v.trailing_zeros() as i32)); } else { self.stack.values.push(Value::I32(v.leading_zeros() as i32)); } },
                        0x6A => { let b = self.pop_i32()?; let a = self.pop_i32()?; self.stack.values.push(Value::I32(a.wrapping_add(b))); },
                        0x6B => { let b = self.pop_i32()?; let a = self.pop_i32()?; self.stack.values.push(Value::I32(a.wrapping_sub(b))); },
                        0x6C => { let b = self.pop_i32()?; let a = self.pop_i32()?; self.stack.values.push(Value::I32(a.wrapping_mul(b))); },
                        0x6D => { let b = self.pop_i32()?; let a = self.pop_i32()?; if b == 0 { return Err("Divide by zero"); } self.stack.values.push(Value::I32(a.checked_div(b).ok_or("Overflow")?)); },
                        0x6E => { let b = self.pop_i32()? as u32; let a = self.pop_i32()? as u32; if b == 0 { return Err("Divide by zero"); } self.stack.values.push(Value::I32((a / b) as i32)); },
                        0x6F => { let b = self.pop_i32()?; let a = self.pop_i32()?; if b == 0 { return Err("Divide by zero"); } self.stack.values.push(Value::I32(a.checked_rem(b).ok_or("Overflow")?)); },
                        0x70 => { let b = self.pop_i32()? as u32; let a = self.pop_i32()? as u32; if b == 0 { return Err("Divide by zero"); } self.stack.values.push(Value::I32((a % b) as i32)); },
                        0x71 => { let b = self.pop_i32()?; let a = self.pop_i32()?; self.stack.values.push(Value::I32(a & b)); },
                        0x72 => { let b = self.pop_i32()?; let a = self.pop_i32()?; self.stack.values.push(Value::I32(a | b)); },
                        0x73 => { let b = self.pop_i32()?; let a = self.pop_i32()?; self.stack.values.push(Value::I32(a ^ b)); },
                        0x74 => { let b = self.pop_i32()? as u32; let a = self.pop_i32()?; self.stack.values.push(Value::I32(a << (b % 32))); },
                        0x75 => { let b = self.pop_i32()? as u32; let a = self.pop_i32()?; self.stack.values.push(Value::I32(a >> (b % 32))); },
                        0x76 => { let b = self.pop_i32()? as u32; let a = self.pop_i32()? as u32; self.stack.values.push(Value::I32((a >> (b % 32)) as i32)); },
                        0x77 => { let b = self.pop_i32()? as u32; let a = self.pop_i32()? as u32; self.stack.values.push(Value::I32(a.rotate_left(b % 32) as i32)); },
                        0x78 => { let b = self.pop_i32()? as u32; let a = self.pop_i32()? as u32; self.stack.values.push(Value::I32(a.rotate_right(b % 32) as i32)); },
                        0x79 => { let v = self.pop_i64()?; self.stack.values.push(Value::I64(v.count_ones() as i64)); },
                        0x7A => { let v = self.pop_i64()?; self.stack.values.push(Value::I64(v.leading_zeros() as i64)); },
                        0x7B => { let v = self.pop_i64()?; self.stack.values.push(Value::I64(v.trailing_zeros() as i64)); },
                        0x7C => { let b = self.pop_i64()?; let a = self.pop_i64()?; self.stack.values.push(Value::I64(a.wrapping_add(b))); },
                        0x7D => { let b = self.pop_i64()?; let a = self.pop_i64()?; self.stack.values.push(Value::I64(a.wrapping_sub(b))); },
                        0x7E => { let b = self.pop_i64()?; let a = self.pop_i64()?; self.stack.values.push(Value::I64(a.wrapping_mul(b))); },
                        0x7F => { let b = self.pop_i64()?; let a = self.pop_i64()?; if b == 0 { return Err("Divide by zero"); } self.stack.values.push(Value::I64(a.checked_div(b).ok_or("Overflow")?)); },
                        0x80 => { let b = self.pop_i64()? as u64; let a = self.pop_i64()? as u64; if b == 0 { return Err("Divide by zero"); } self.stack.values.push(Value::I64((a / b) as i64)); },
                        0x81 => { let b = self.pop_i64()?; let a = self.pop_i64()?; if b == 0 { return Err("Divide by zero"); } self.stack.values.push(Value::I64(a.checked_rem(b).ok_or("Overflow")?)); },
                        0x82 => { let b = self.pop_i64()? as u64; let a = self.pop_i64()? as u64; if b == 0 { return Err("Divide by zero"); } self.stack.values.push(Value::I64((a % b) as i64)); },
                        0x83 => { let b = self.pop_i64()?; let a = self.pop_i64()?; self.stack.values.push(Value::I64(a & b)); },
                        0x84 => { let b = self.pop_i64()?; let a = self.pop_i64()?; self.stack.values.push(Value::I64(a | b)); },
                        0x85 => { let b = self.pop_i64()?; let a = self.pop_i64()?; self.stack.values.push(Value::I64(a ^ b)); },
                        0x86 => { let b = self.pop_i64()? as u32; let a = self.pop_i64()?; self.stack.values.push(Value::I64(a << (b % 64))); },
                        0x87 => { let b = self.pop_i64()? as u32; let a = self.pop_i64()?; self.stack.values.push(Value::I64(a >> (b % 64))); },
                        0x88 => { let b = self.pop_i64()? as u32; let a = self.pop_i64()? as u64; self.stack.values.push(Value::I64((a >> (b % 64)) as i64)); },
                        0x89 => { let b = self.pop_i64()? as u32; let a = self.pop_i64()? as u64; self.stack.values.push(Value::I64(a.rotate_left(b % 64) as i64)); },
                        0x8A => { let b = self.pop_i64()? as u32; let a = self.pop_i64()? as u64; self.stack.values.push(Value::I64(a.rotate_right(b % 64) as i64)); },
                        0x8B => { let v = self.pop_f32()?; self.stack.values.push(Value::F32(v.abs())); },
                        0x8C => { let v = self.pop_f32()?; self.stack.values.push(Value::F32(-v)); },
                        0x8D => { let v = self.pop_f32()?; self.stack.values.push(Value::F32(v.ceil())); },
                        0x8E => { let v = self.pop_f32()?; self.stack.values.push(Value::F32(v.floor())); },
                        0x8F => { let v = self.pop_f32()?; self.stack.values.push(Value::F32(v.trunc())); },
                        0x90 => { let v = self.pop_f32()?; self.stack.values.push(Value::F32(v.round())); },
                        0x91 => { let v = self.pop_f32()?; self.stack.values.push(Value::F32(v.sqrt())); },
                        0x92 => { let b = self.pop_f32()?; let a = self.pop_f32()?; self.stack.values.push(Value::F32(a + b)); },
                        0x93 => { let b = self.pop_f32()?; let a = self.pop_f32()?; self.stack.values.push(Value::F32(a - b)); },
                        0x94 => { let b = self.pop_f32()?; let a = self.pop_f32()?; self.stack.values.push(Value::F32(a * b)); },
                        0x95 => { let b = self.pop_f32()?; let a = self.pop_f32()?; self.stack.values.push(Value::F32(a / b)); },
                        0x96 => { let b = self.pop_f32()?; let a = self.pop_f32()?; self.stack.values.push(Value::F32(if a < b { a } else { b })); },
                        0x97 => { let b = self.pop_f32()?; let a = self.pop_f32()?; self.stack.values.push(Value::F32(if a > b { a } else { b })); },
                        0x98 => { let b = self.pop_f32()?; let a = self.pop_f32()?; self.stack.values.push(Value::F32(if b.is_sign_negative() { -a.abs() } else { a.abs() })); },
                        0x99 => { let v = self.pop_f64()?; self.stack.values.push(Value::F64(v.abs())); },
                        0x9A => { let v = self.pop_f64()?; self.stack.values.push(Value::F64(-v)); },
                        0x9B => { let v = self.pop_f64()?; self.stack.values.push(Value::F64(v.ceil())); },
                        0x9C => { let v = self.pop_f64()?; self.stack.values.push(Value::F64(v.floor())); },
                        0x9D => { let v = self.pop_f64()?; self.stack.values.push(Value::F64(v.trunc())); },
                        0x9E => { let v = self.pop_f64()?; self.stack.values.push(Value::F64(v.round())); },
                        0x9F => { let v = self.pop_f64()?; self.stack.values.push(Value::F64(v.sqrt())); },
                        0xA0 => { let b = self.pop_f64()?; let a = self.pop_f64()?; self.stack.values.push(Value::F64(a + b)); },
                        0xA1 => { let b = self.pop_f64()?; let a = self.pop_f64()?; self.stack.values.push(Value::F64(a - b)); },
                        0xA2 => { let b = self.pop_f64()?; let a = self.pop_f64()?; self.stack.values.push(Value::F64(a * b)); },
                        0xA3 => { let b = self.pop_f64()?; let a = self.pop_f64()?; self.stack.values.push(Value::F64(a / b)); },
                        0xA4 => { let b = self.pop_f64()?; let a = self.pop_f64()?; self.stack.values.push(Value::F64(if a < b { a } else { b })); },
                        0xA5 => { let b = self.pop_f64()?; let a = self.pop_f64()?; self.stack.values.push(Value::F64(if a > b { a } else { b })); },
                        0xA6 => { let b = self.pop_f64()?; let a = self.pop_f64()?; self.stack.values.push(Value::F64(if b.is_sign_negative() { -a.abs() } else { a.abs() })); },
                        0xA7 => { let v = self.pop_i64()?; self.stack.values.push(Value::I32(v as i32)); },
                        0xA8 => { let v = self.pop_f32()?; self.stack.values.push(Value::I32(v as i32)); },
                        0xA9 => { let v = self.pop_f32()?; self.stack.values.push(Value::I32(v as u32 as i32)); },
                        0xAA => { let v = self.pop_f64()?; self.stack.values.push(Value::I32(v as i32)); },
                        0xAB => { let v = self.pop_f64()?; self.stack.values.push(Value::I32(v as u32 as i32)); },
                        0xAC => { let v = self.pop_i32()?; self.stack.values.push(Value::I64(v as i64)); },
                        0xAD => { let v = self.pop_i32()? as u32; self.stack.values.push(Value::I64(v as i64)); },
                        0xAE => { let v = self.pop_f32()?; self.stack.values.push(Value::I64(v as i64)); },
                        0xAF => { let v = self.pop_f32()?; self.stack.values.push(Value::I64(v as u64 as i64)); },
                        0xB0 => { let v = self.pop_f64()?; self.stack.values.push(Value::I64(v as i64)); },
                        0xB1 => { let v = self.pop_f64()?; self.stack.values.push(Value::I64(v as u64 as i64)); },
                        0xB2 => { let v = self.pop_i32()?; self.stack.values.push(Value::F32(v as f32)); },
                        0xB3 => { let v = self.pop_i32()? as u32; self.stack.values.push(Value::F32(v as f32)); },
                        0xB4 => { let v = self.pop_i64()?; self.stack.values.push(Value::F32(v as f32)); },
                        0xB5 => { let v = self.pop_i64()? as u64; self.stack.values.push(Value::F32(v as f32)); },
                        0xB6 => { let v = self.pop_f64()?; self.stack.values.push(Value::F32(v as f32)); },
                        0xB7 => { let v = self.pop_i32()?; self.stack.values.push(Value::F64(v as f64)); },
                        0xB8 => { let v = self.pop_i32()? as u32; self.stack.values.push(Value::F64(v as f64)); },
                        0xB9 => { let v = self.pop_i64()?; self.stack.values.push(Value::F64(v as f64)); },
                        0xBA => { let v = self.pop_i64()? as u64; self.stack.values.push(Value::F64(v as f64)); },
                        0xBB => { let v = self.pop_f32()?; self.stack.values.push(Value::F64(v as f64)); },
                        0xBC => { let v = self.pop_f32()?; self.stack.values.push(Value::I32(v.to_bits() as i32)); },
                        0xBD => { let v = self.pop_f64()?; self.stack.values.push(Value::I64(v.to_bits() as i64)); },
                        0xBE => { let v = self.pop_i32()?; self.stack.values.push(Value::F32(f32::from_bits(v as u32))); },
                        0xBF => { let v = self.pop_i64()?; self.stack.values.push(Value::F64(f64::from_bits(v as u64))); },
                        0xC0 => { let v = self.pop_i32()?; self.stack.values.push(Value::I32(v as i8 as i32)); },
                        0xC1 => { let v = self.pop_i32()?; self.stack.values.push(Value::I32(v as i16 as i32)); },
                        0xC2 => { let v = self.pop_i64()?; self.stack.values.push(Value::I64(v as i8 as i64)); },
                        0xC3 => { let v = self.pop_i64()?; self.stack.values.push(Value::I64(v as i16 as i64)); },
                        0xC4 => { let v = self.pop_i64()?; self.stack.values.push(Value::I64(v as i32 as i64)); },
                        _ => {}
                    }
                },
                0xFC => {
                    let sub_op = Self::read_leb_u32(&frame_code, &mut current_ip)?;
                    self.stack.frames.last_mut().unwrap().ip = current_ip;
                    match sub_op {
                        8 => { // memory.init
                            let data_idx = Self::read_leb_u32(&frame_code, &mut current_ip)? as usize;
                            let _mem_idx = Self::read_leb_u32(&frame_code, &mut current_ip)?;
                            self.stack.frames.last_mut().unwrap().ip = current_ip;
                            let n = self.pop_i32()? as usize;
                            let s = self.pop_i32()? as usize;
                            let d = self.pop_i32()? as usize;
                            let frame = self.stack.frames.last().unwrap();
                            let data_segs = frame.module.data_segments.borrow();
                            let data = data_segs.get(data_idx).ok_or("Invalid data index")?.as_ref().ok_or("Data segment dropped")?;
                            let mem_idx = frame.module.mem_addrs[0] as usize;
                            let mem = &mut self.store.memories[mem_idx].data;
                            if s + n <= data.len() && d + n <= mem.len() {
                                mem[d..d+n].copy_from_slice(&data[s..s+n]);
                            } else { return Err("memory.init OOB"); }
                        },
                        9 => { // data.drop
                            let data_idx = Self::read_leb_u32(&frame_code, &mut current_ip)? as usize;
                            self.stack.frames.last_mut().unwrap().ip = current_ip;
                            let frame = self.stack.frames.last().unwrap();
                            let mut data_segs = frame.module.data_segments.borrow_mut();
                            if data_idx < data_segs.len() {
                                data_segs[data_idx] = None;
                            } else { return Err("data.drop invalid index"); }
                        },
                        10 => { // memory.copy
                            let _dst_mem = Self::read_leb_u32(&frame_code, &mut current_ip)?;
                            let _src_mem = Self::read_leb_u32(&frame_code, &mut current_ip)?;
                            self.stack.frames.last_mut().unwrap().ip = current_ip;
                            let n = self.pop_i32()? as usize;
                            let s = self.pop_i32()? as usize;
                            let d = self.pop_i32()? as usize;
                            let mem_idx = self.stack.frames.last().unwrap().module.mem_addrs[0] as usize;
                            let mem = &mut self.store.memories[mem_idx].data;
                            if d + n <= mem.len() && s + n <= mem.len() {
                                mem.copy_within(s..s+n, d);
                            } else { return Err("memory.copy OOB"); }
                        },
                        11 => { // memory.fill
                            let _mem_idx = Self::read_leb_u32(&frame_code, &mut current_ip)?;
                            self.stack.frames.last_mut().unwrap().ip = current_ip;
                            let n = self.pop_i32()? as usize;
                            let val = self.pop_i32()? as u8;
                            let d = self.pop_i32()? as usize;
                            let mem_idx = self.stack.frames.last().unwrap().module.mem_addrs[0] as usize;
                            let mem = &mut self.store.memories[mem_idx].data;
                            if d + n <= mem.len() {
                                for i in 0..n { mem[d + i] = val; }
                            } else { return Err("memory.fill OOB"); }
                        },
                        _ => return Err("Unsupported 0xFC subop"),
                    }
                },
                0xFD => return Err("SIMD not supported"),
                _ => {}
            }
        }
    }

    fn execute_call_frame(&mut self, func_addr: u32, args_opt: Option<Vec<Value>>) -> Result<(), &'static str> {
        let func = self.store.funcs[func_addr as usize].clone();
        let mut args = if let Some(a) = args_opt { a } else { let mut a = Vec::new(); for _ in 0..func.ty.params.len() { a.push(self.stack.values.pop().ok_or("under call params")?); } a.reverse(); a };
        if let Some(host_code) = func.host_code { let res = (host_code)(self.store, &args); for v in res { self.stack.values.push(v); } Ok(()) }
        else {
            let body = func.code.as_ref().ok_or("no code")?;
            let mut locals = args; for d in &body.locals { for _ in 0..d.count { locals.push(Value::default(d.ty)); } }
            self.stack.frames.push(Frame { module: func.module.unwrap(), locals, code: Rc::new(body.code.clone()), labels: Vec::new(), return_arity: func.ty.results.len(), ip: 0 }); Ok(())
        }
    }
}
