use crate::rust_alloc::vec::Vec;
use crate::rust_alloc::rc::Rc;
use crate::rust_alloc::collections::BTreeMap;
use core::cell::RefCell;
use super::types::{ValType, Module};
use crate::rust_alloc::string::String;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
}

impl Value {
    pub fn default(ty: ValType) -> Self {
        match ty {
            ValType::I32 => Value::I32(0),
            ValType::I64 => Value::I64(0),
            ValType::F32 => Value::F32(0.0),
            ValType::F64 => Value::F64(0.0),
            _ => Value::I32(0),
        }
    }
}

pub struct WasiCtx {
    pub env: Vec<(String, String)>,
    pub args: Vec<String>,
    pub files: BTreeMap<u32, usize>, // WASI FD -> Host FD
}

impl WasiCtx {
    pub fn new() -> Self {
        let mut files = BTreeMap::new();
        files.insert(0, 0); // stdin
        files.insert(1, 1); // stdout
        files.insert(2, 2); // stderr
        Self {
            env: Vec::new(),
            args: Vec::new(),
            files,
        }
    }
}

pub struct Store {
    pub funcs: Vec<FuncInstance>,
    pub tables: Vec<TableInstance>,
    pub memories: Vec<MemoryInstance>,
    pub globals: Vec<GlobalInstance>,
    pub wasi: WasiCtx,
}

impl Store {
    pub fn new() -> Self {
        Self {
            funcs: Vec::new(),
            tables: Vec::new(),
            memories: Vec::new(),
            globals: Vec::new(),
            wasi: WasiCtx::new(),
        }
    }

    pub fn instantiate(&mut self, module: Rc<Module>, imports_src: &[Rc<ModuleInstance>]) -> Result<Rc<ModuleInstance>, &'static str> {
        let mut func_addrs = Vec::new();
        let mut table_addrs = Vec::new();
        let mut mem_addrs = Vec::new();
        let mut global_addrs = Vec::new();
        
        for import in &module.imports {
             let mut found = None;
             for src in imports_src {
                 for exp in &src.exports {
                     if exp.name == import.name {
                         found = Some(exp.value);
                         break;
                     }
                 }
                 if found.is_some() { break; }
             }
             
             match (found, &import.desc) {
                 (Some(ExternalVal::Func(addr)), super::types::ImportDesc::Func(_)) => func_addrs.push(addr),
                 (Some(ExternalVal::Table(addr)), super::types::ImportDesc::Table(_)) => table_addrs.push(addr),
                 (Some(ExternalVal::Memory(addr)), super::types::ImportDesc::Memory(_)) => mem_addrs.push(addr),
                 (Some(ExternalVal::Global(addr)), super::types::ImportDesc::Global(_)) => global_addrs.push(addr),
                 (None, _) => return Err("Import not found"),
                 _ => return Err("Import type mismatch"),
             }
        }
        
        for (i, type_idx) in module.functions.iter().enumerate() {
            let ty = module.types.get(*type_idx as usize).ok_or("Invalid type index")?.clone();
            let code = module.code.get(i).ok_or("Invalid code index")?.clone();
            self.funcs.push(FuncInstance { ty, module: None, code: Some(code), host_code: None });
            func_addrs.push((self.funcs.len() - 1) as u32);
        }
        
        for table_type in &module.tables {
             let size = table_type.limits.min as usize;
             let elements = crate::rust_alloc::vec![None; size];
             self.tables.push(TableInstance { elements, max: table_type.limits.max });
             table_addrs.push((self.tables.len() - 1) as u32);
        }
        
        let mut total_mem_size = 0;
        for mem_type in &module.memories {
             let min_pages = if mem_type.limits.min < 2048 { 2048 } else { mem_type.limits.min }; 
             let size = (min_pages as usize) * 65536;
             total_mem_size = size;
             let data = crate::rust_alloc::vec![0u8; size];
             self.memories.push(MemoryInstance { data, max: mem_type.limits.max });
             mem_addrs.push((self.memories.len() - 1) as u32);
        }
        
        for global_def in &module.globals {
             let val = self.eval_const_expr(&global_def.init)?;
             self.globals.push(GlobalInstance { value: val, mutability: global_def.ty.mutability });
             global_addrs.push((self.globals.len() - 1) as u32);
        }
        
        for addr in &global_addrs {
            let g = &mut self.globals[*addr as usize];
            if g.mutability {
                if let Value::I32(v) = g.value {
                    if v < 65536 {
                        g.value = Value::I32(1024 * 1024 * 32); 
                        break;
                    }
                }
            }
        }

        let mut exports = Vec::new();
        for export in &module.exports {
             let val = match export.desc {
                 super::types::ExportDesc::Func(idx) => ExternalVal::Func(func_addrs[idx as usize]),
                 super::types::ExportDesc::Table(idx) => ExternalVal::Table(table_addrs[idx as usize]),
                 super::types::ExportDesc::Memory(idx) => ExternalVal::Memory(mem_addrs[idx as usize]),
                 super::types::ExportDesc::Global(idx) => ExternalVal::Global(global_addrs[idx as usize]),
             };
             exports.push(ExportInstance { name: export.name.clone(), value: val });
        }
        
        let mut data_segments = Vec::new();
        for data in &module.data { data_segments.push(Some(data.init.clone())); }

        let mut element_segments = Vec::new();
        for elem in &module.elements { element_segments.push(Some(elem.init.clone())); }

        let instance = Rc::new(ModuleInstance { 
            func_addrs: func_addrs.clone(), 
            table_addrs: table_addrs.clone(), 
            mem_addrs: mem_addrs.clone(), 
            global_addrs, 
            data_segments: RefCell::new(data_segments),
            element_segments: RefCell::new(element_segments),
            exports 
        });
        let num_defined = module.functions.len();
        for i in 0..num_defined {
             let imported_funcs_count = func_addrs.len() - num_defined;
             let store_func_idx = func_addrs[imported_funcs_count + i];
             if let Some(func) = self.funcs.get_mut(store_func_idx as usize) { func.module = Some(instance.clone()); }
        }
        
        for elem in &module.elements {
             let offset = match self.eval_const_expr(&elem.offset)? { Value::I32(v) => v as usize, _ => return Err("Elem offset tm") };
             let table_idx = elem.table_index as usize;
             let store_table_idx = table_addrs.get(table_idx).ok_or("Invalid table index")?;
             let table = self.tables.get_mut(*store_table_idx as usize).ok_or("Table not found")?;
             for (i, func_idx) in elem.init.iter().enumerate() {
                 if offset + i < table.elements.len() {
                     table.elements[offset + i] = Some(func_addrs[*func_idx as usize]);
                 }
             }
        }

        for data in &module.data {
            if let Some(offset_expr) = &data.offset {
                 let offset = match self.eval_const_expr(offset_expr)? { Value::I32(v) => v as usize, _ => return Err("Data offset must be i32") };
                 let mem_idx = data.memory_index as usize;
                 let store_mem_idx = mem_addrs.get(mem_idx).ok_or("Invalid memory index")?;
                 let mem = self.memories.get_mut(*store_mem_idx as usize).ok_or("Memory not found")?;
                 if offset + data.init.len() > mem.data.len() { return Err("Data segment out of bounds"); }
                 mem.data[offset..offset+data.init.len()].copy_from_slice(&data.init);
            }
        }
        
        Ok(instance)
    }

    fn eval_const_expr(&self, expr: &super::types::Expr) -> Result<Value, &'static str> {
         if expr.instructions.is_empty() { return Ok(Value::I32(0)); }
         let mut ip = 0;
         let op = expr.instructions[ip]; ip += 1;
         match op {
             0x41 => {
                 let mut res: i32 = 0; let mut shift = 0;
                 loop { let b = expr.instructions[ip]; ip += 1; res |= ((b & 0x7F) as i32) << shift; if (b & 0x80) == 0 { break; } shift += 7; }
                 Ok(Value::I32(res))
             },
             0x42 => {
                 let mut res: i64 = 0; let mut shift = 0;
                 loop { let b = expr.instructions[ip]; ip += 1; res |= ((b & 0x7F) as i64) << shift; if (b & 0x80) == 0 { break; } shift += 7; }
                 Ok(Value::I64(res))
             },
             _ => Ok(Value::I32(0)),
         }
    }
}

#[derive(Clone)]
pub struct FuncInstance {
    pub ty: super::types::FuncType,
    pub module: Option<Rc<ModuleInstance>>,
    pub code: Option<super::types::FunctionBody>,
    pub host_code: Option<HostFunc>,
}

pub type HostFunc = fn(&mut Store, &[Value]) -> Vec<Value>;

pub struct TableInstance {
    pub elements: Vec<Option<u32>>,
    pub max: Option<u32>,
}

pub struct MemoryInstance {
    pub data: Vec<u8>,
    pub max: Option<u32>,
}

pub struct GlobalInstance {
    pub value: Value,
    pub mutability: bool,
}

pub struct ModuleInstance {
    pub func_addrs: Vec<u32>,
    pub table_addrs: Vec<u32>,
    pub mem_addrs: Vec<u32>,
    pub global_addrs: Vec<u32>,
    pub data_segments: RefCell<Vec<Option<Vec<u8>>>>,
    pub element_segments: RefCell<Vec<Option<Vec<u32>>>>,
    pub exports: Vec<ExportInstance>,
}

#[derive(Clone)]
pub struct ExportInstance {
    pub name: String,
    pub value: ExternalVal,
}

#[derive(Clone, Copy, Debug)]
pub enum ExternalVal {
    Func(u32),
    Table(u32),
    Memory(u32),
    Global(u32),
}

pub struct Stack {
    pub values: Vec<Value>,
    pub frames: Vec<Frame>,
}

impl Stack {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            frames: Vec::new(),
        }
    }

    pub fn push(&mut self, v: Value) {
        self.values.push(v);
    }

    pub fn pop(&mut self) -> Value {
        self.values.pop().expect("Stack underflow")
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WasmLabel {
    pub target_ip: usize,
    pub stack_height: usize,
    pub arity: usize,
}

pub struct Frame {
    pub module: Rc<ModuleInstance>,
    pub locals: Vec<Value>,
    pub code: Rc<Vec<u8>>,
    pub labels: Vec<WasmLabel>,
    pub return_arity: usize,
    pub ip: usize,
}