use crate::alloc::{string::String, vec, vec::Vec};
use crate::alloc::boxed::Box;
use crate::wasm::common::value::Value;
use crate::wasm::common::config::Config;
use crate::wasm::interpreter::store::Store;
use crate::wasm::component::types::{PrimitiveValType, ComponentValType, DefinedType, VariantCase, ComponentFuncType, CanonOpt};

#[derive(Debug, Clone)]
pub enum ComponentValue {
    Bool(bool),
    S8(i8),
    U8(u8),
    S16(i16),
    U16(u16),
    S32(i32),
    U32(u32),
    S64(i64),
    U64(u64),
    F32(f32),
    F64(f64),
    Char(char),
    String(String),
    List(Vec<ComponentValue>),
    Record(Vec<(String, ComponentValue)>),
    Variant { label: String, value: Option<Box<ComponentValue>> },
    Enum(String),
    Flags(Vec<String>),
    Option(Option<Box<ComponentValue>>),
    Result(core::result::Result<Option<Box<ComponentValue>>, Option<Box<ComponentValue>>>),
    Tuple(Vec<ComponentValue>),
}

pub struct CanonicalAbi;

impl CanonicalAbi {
    pub fn lower_flat(val: ComponentValue) -> Vec<Value> {
        let mut results = Vec::new();
        match val {
            ComponentValue::Bool(b) => results.push(Value::I32(if b { 1 } else { 0 })),
            ComponentValue::S8(v) => results.push(Value::I32(v as u32)),
            ComponentValue::U8(v) => results.push(Value::I32(v as u32)),
            ComponentValue::S16(v) => results.push(Value::I32(v as u32)),
            ComponentValue::U16(v) => results.push(Value::I32(v as u32)),
            ComponentValue::S32(v) => results.push(Value::I32(v as u32)),
            ComponentValue::U32(v) => results.push(Value::I32(v)),
            ComponentValue::S64(v) => results.push(Value::I64(v as u64)),
            ComponentValue::U64(v) => results.push(Value::I64(v)),
            ComponentValue::F32(v) => results.push(Value::F32(crate::wasm::common::value::F32(v))),
            ComponentValue::F64(v) => results.push(Value::F64(crate::wasm::common::value::F64(v))),
            ComponentValue::Char(c) => results.push(Value::I32(c as u32)),
            ComponentValue::String(_) | ComponentValue::List(_) => {
                panic!("lower_flat: String/List require memory access");
            }
            ComponentValue::Record(fields) => {
                for (_, f_val) in fields {
                    results.extend(Self::lower_flat(f_val));
                }
            }
            ComponentValue::Tuple(items) => {
                for v in items {
                    results.extend(Self::lower_flat(v));
                }
            }
            ComponentValue::Variant { label: _, value } => {
                // Simplified: tag + value
                if let Some(v) = value {
                    results.extend(Self::lower_flat(*v));
                }
            }
            ComponentValue::Enum(_) => {
                results.push(Value::I32(0)); // Tag only
            }
            ComponentValue::Option(opt) => {
                if let Some(v) = opt {
                    results.push(Value::I32(1));
                    results.extend(Self::lower_flat(*v));
                } else {
                    results.push(Value::I32(0));
                }
            }
            _ => panic!("lower_flat: Unimplemented type"),
        }
        results
    }

    pub fn lift_flat<T: Config>(
        store: &Store<'_, T>,
        values: &[Value],
        ty: &ComponentValType,
        options: &[CanonOpt],
        types: &[DefinedType]
    ) -> (ComponentValue, usize) {
        match ty {
            ComponentValType::Primitive(p) => match p {
                PrimitiveValType::Bool => (ComponentValue::Bool(values[0].to_u128() != 0), 1),
                PrimitiveValType::S8 => (ComponentValue::S8(values[0].to_u128() as i8), 1),
                PrimitiveValType::U8 => (ComponentValue::U8(values[0].to_u128() as u8), 1),
                PrimitiveValType::S16 => (ComponentValue::S16(values[0].to_u128() as i16), 1),
                PrimitiveValType::U16 => (ComponentValue::U16(values[0].to_u128() as u16), 1),
                PrimitiveValType::S32 => (ComponentValue::S32(values[0].to_u128() as i32), 1),
                PrimitiveValType::U32 => (ComponentValue::U32(values[0].to_u128() as u32), 1),
                PrimitiveValType::S64 => (ComponentValue::S64(values[0].to_u128() as i64), 1),
                PrimitiveValType::U64 => (ComponentValue::U64(values[0].to_u128() as u64), 1),
                PrimitiveValType::F32 => {
                    let bits = values[0].to_u128() as u32;
                    (ComponentValue::F32(f32::from_bits(bits)), 1)
                }
                PrimitiveValType::F64 => {
                    let bits = values[0].to_u128() as u64;
                    (ComponentValue::F64(f64::from_bits(bits)), 1)
                }
                PrimitiveValType::Char => (ComponentValue::Char(core::char::from_u32(values[0].to_u128() as u32).unwrap()), 1),
                PrimitiveValType::String => {
                    let ptr = values[0].to_u128() as u32;
                    let len = values[1].to_u128() as u32;
                    let mut buf = vec![0u8; len as usize];
                    let mem_idx = options.iter().find_map(|o| if let CanonOpt::Memory(idx) = o { Some(*idx) } else { None }).unwrap_or(0);
                    let module_addr = store.caller_module.unwrap();
                    let mem_addr = *store.modules.get(module_addr).mem_addrs.get(mem_idx as usize).unwrap();
                    let mem = store.memories.get(mem_addr);
                    mem.mem.read_slice(ptr as usize, &mut buf).unwrap();
                    (ComponentValue::String(String::from_utf8(buf).unwrap()), 2)
                }
            },
            ComponentValType::Type(idx) => {
                let def = &types[*idx as usize];
                match def {
                    DefinedType::Record(fields) => {
                        let mut field_vals = Vec::new();
                        let mut consumed = 0;
                        for (name, f_ty) in fields {
                            let (v, c) = Self::lift_flat(store, &values[consumed..], f_ty, options, types);
                            field_vals.push((name.clone(), v));
                            consumed += c;
                        }
                        (ComponentValue::Record(field_vals), consumed)
                    }
                    DefinedType::Tuple(items) => {
                        let mut item_vals = Vec::new();
                        let mut consumed = 0;
                        for i_ty in items {
                            let (v, c) = Self::lift_flat(store, &values[consumed..], i_ty, options, types);
                            item_vals.push(v);
                            consumed += c;
                        }
                        (ComponentValue::Tuple(item_vals), consumed)
                    }
                    DefinedType::Option(inner_ty) => {
                        let tag = values[0].to_u128() as u32;
                        if tag == 0 {
                            (ComponentValue::Option(None), 1)
                        } else {
                            let (v, c) = Self::lift_flat(store, &values[1..], inner_ty, options, types);
                            (ComponentValue::Option(Some(Box::new(v))), 1 + c)
                        }
                    }
                    DefinedType::Result { ok, err } => {
                        let tag = values[0].to_u128() as u32;
                        if tag == 0 {
                            if let Some(ok_ty) = ok {
                                let (v, c) = Self::lift_flat(store, &values[1..], ok_ty, options, types);
                                (ComponentValue::Result(Ok(Some(Box::new(v)))), 1 + c)
                            } else {
                                (ComponentValue::Result(Ok(None)), 1)
                            }
                        } else {
                            if let Some(err_ty) = err {
                                let (v, c) = Self::lift_flat(store, &values[1..], err_ty, options, types);
                                (ComponentValue::Result(Err(Some(Box::new(v)))), 1 + c)
                            } else {
                                (ComponentValue::Result(Err(None)), 1)
                            }
                        }
                    }
                    _ => panic!("lift_flat: Unimplemented defined type"),
                }
            }
        }
    }
}
