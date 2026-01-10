use crate::wasm::{Value, interpreter::Interpreter};
use crate::wasm::leb128::Leb128;

impl Interpreter {
    pub(crate) fn execute_memory(&mut self, opcode: u8, pc: &mut usize, bytecode: &[u8]) -> Result<(), &'static str> {
        match opcode {
            0x28 | 0x29 | 0x2A | 0x2B | 0x2C | 0x2D | 0x2E | 0x2F | 0x30 | 0x31 | 0x32 | 0x33 | 0x34 | 0x35 => {
                let size = match opcode {
                    0x28 | 0x2A => 4,
                    0x29 | 0x2B => 8,
                    0x2C | 0x2D | 0x30 | 0x31 => 1,
                    0x2E | 0x2F | 0x34 | 0x35 => 2,  // FIXED: i32.load16_*, i64.load16_* load 2 bytes
                    0x32 | 0x33 => 4,  // FIXED: i64.load32_* load 4 bytes
                    _ => 4
                };
                let addr = self.get_mem_addr(bytecode, pc, size)?;
                match opcode {
                    0x28 => self.stack.push(Value::I32(i32::from_le_bytes(self.memory[addr..addr+4].try_into().unwrap()))),
                    0x29 => self.stack.push(Value::I64(i64::from_le_bytes(self.memory[addr..addr+8].try_into().unwrap()))),
                    0x2A => self.stack.push(Value::F32(f32::from_le_bytes(self.memory[addr..addr+4].try_into().unwrap()))),
                    0x2B => self.stack.push(Value::F64(f64::from_le_bytes(self.memory[addr..addr+8].try_into().unwrap()))),
                    0x2C => self.stack.push(Value::I32(self.memory[addr] as i8 as i32)),
                    0x2D => self.stack.push(Value::I32(self.memory[addr] as i32)),
                    0x2E => self.stack.push(Value::I32(i16::from_le_bytes(self.memory[addr..addr+2].try_into().unwrap()) as i32)),
                    0x2F => self.stack.push(Value::I32(u16::from_le_bytes(self.memory[addr..addr+2].try_into().unwrap()) as i32)),
                    0x30 => self.stack.push(Value::I64(self.memory[addr] as i8 as i64)),
                    0x31 => self.stack.push(Value::I64(self.memory[addr] as i64)),
                    // FIXED: Swapped operations for 0x32-0x35
                    0x32 => self.stack.push(Value::I64(i32::from_le_bytes(self.memory[addr..addr+4].try_into().unwrap()) as i64)),
                    0x33 => self.stack.push(Value::I64(u32::from_le_bytes(self.memory[addr..addr+4].try_into().unwrap()) as i64)),
                    0x34 => self.stack.push(Value::I64(i16::from_le_bytes(self.memory[addr..addr+2].try_into().unwrap()) as i64)),
                    0x35 => self.stack.push(Value::I64(u16::from_le_bytes(self.memory[addr..addr+2].try_into().unwrap()) as i64)),
                    _ => unreachable!(),
                }
            }
            0x36 | 0x37 | 0x38 | 0x39 | 0x3A | 0x3B | 0x3C | 0x3D | 0x3E => {
                let val = self.stack.pop().ok_or("val missing")?;
                let size = match opcode {
                    0x3A | 0x3C => 1,
                    0x3B | 0x3D => 2,
                    0x36 | 0x38 | 0x3E => 4,
                    0x37 | 0x39 => 8,
                    _ => 4
                };
                let addr = self.get_mem_addr(bytecode, pc, size)?;
                match val {
                    Value::I32(v) => match opcode { 0x3A => self.memory[addr] = v as u8, 0x3B => self.memory[addr..addr+2].copy_from_slice(&(v as u16).to_le_bytes()), _ => self.memory[addr..addr+4].copy_from_slice(&v.to_le_bytes()) },
                    Value::I64(v) => match opcode { 0x3C => self.memory[addr] = v as u8, 0x3D => self.memory[addr..addr+2].copy_from_slice(&(v as u16).to_le_bytes()), 0x3E => self.memory[addr..addr+4].copy_from_slice(&(v as u32).to_le_bytes()), _ => self.memory[addr..addr+8].copy_from_slice(&v.to_le_bytes()) },
                    Value::F32(v) => self.memory[addr..addr+4].copy_from_slice(&v.to_le_bytes()),
                    Value::F64(v) => self.memory[addr..addr+8].copy_from_slice(&v.to_le_bytes()),
                }
            }
            0x3F => { Leb128::decode_u32(bytecode, pc); self.stack.push(Value::I32((self.memory.len() / (64*1024)) as i32)); }
            0x40 => {
                Leb128::decode_u32(bytecode, pc);
                let n = self.pop_i32()?;
                let old = self.memory.len() / (64*1024);
                self.memory.resize(self.memory.len() + n as usize * 64 * 1024, 0);
                self.stack.push(Value::I32(old as i32));
            }
            _ => return Err("Invalid memory opcode"),
        }
        Ok(())
    }
}