use crate::wasm::common::value::Value;
use crate::wasm::interpreter::simd_utils;
use crate::wasm::common::reader::types::opcode::fd_extensions::*;

pub fn handle_simd(sub: u32, stack: &mut Vec<Value>) {
    // This will be a copy-paste of the interpreter's SIMD logic
    // but adapted for AOT trampoline use.
    match sub {
        V128_CONST => panic!("v128.const should be handled inline"),
        I8X16_ADD => {
            let b = stack.pop().unwrap().try_into().unwrap();
            let a = stack.pop().unwrap().try_into().unwrap();
            stack.push(Value::V128(simd_utils::i8x16_add(a, b)));
        }
        // ... and all other 200+ opcodes ...
        _ => panic!("Unimplemented SIMD in AOT handler"),
    }
}
