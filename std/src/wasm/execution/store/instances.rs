use super::{
    addrs::{DataAddr, ElemAddr, FuncAddr, GlobalAddr, MemAddr, ModuleAddr, TableAddr},
    ExternVal, HaltExecutionError, Store,
};
use crate::rust_alloc::{collections::btree_map::BTreeMap, string::String, vec, vec::Vec};
use crate::wasm::{
    core::{
        indices::TypeIdx,
        reader::{
            span::Span,
            types::{FuncType, MemType, TableType},
        },
        sidetable::Sidetable,
    },
    execution::store::linear_memory::LinearMemory,
    execution::value::Ref,
    GlobalType, Limits, RefType, RuntimeError, TrapError, ValType, Value,
};
#[derive(Debug)]
pub enum FuncInst<T: crate::wasm::execution::config::Config> {
    WasmFunc(WasmFuncInst),
    HostFunc(HostFuncInst<T>),
}
#[derive(Debug)]
pub struct WasmFuncInst {
    pub function_type: FuncType,
    pub _ty: TypeIdx,
    pub locals: Vec<ValType>,
    pub code_expr: Span,
    pub stp: usize,
    pub module_addr: ModuleAddr,
}
#[derive(Debug)]
pub struct HostFuncInst<T: crate::wasm::execution::config::Config> {
    pub function_type: FuncType,
    pub hostcode: for<'a> fn(&mut Store<'a, T>, Vec<Value>) -> Result<Vec<Value>, HaltExecutionError>,
}
impl<T: crate::wasm::execution::config::Config> FuncInst<T> {
    pub fn ty(&self) -> FuncType {
        match self {
            FuncInst::WasmFunc(wasm_func_inst) => wasm_func_inst.function_type.clone(),
            FuncInst::HostFunc(host_func_inst) => host_func_inst.function_type.clone(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct ElemInst {
    pub _ty: RefType,
    pub references: Vec<Ref>,
}
impl ElemInst {
    pub fn len(&self) -> usize {
        self.references.len()
    }
}
#[derive(Debug)]
pub struct TableInst {
    pub ty: TableType,
    pub elem: Vec<Ref>,
}
impl TableInst {
    pub fn len(&self) -> usize {
        self.elem.len()
    }
    pub fn grow(&mut self, n: u32, reff: Ref) -> Result<(), RuntimeError> {
        let len = n
            .checked_add(self.elem.len() as u32)
            .ok_or(TrapError::TableOrElementAccessOutOfBounds)?;
        if self.ty.lim.max.map(|max| len > max).unwrap_or(false) {
            return Err(TrapError::TableOrElementAccessOutOfBounds.into());
        }
        let limits_prime = Limits {
            min: len,
            max: self.ty.lim.max,
        };
        self.elem.extend(vec![reff; n as usize]);
        self.ty.lim = limits_prime;
        Ok(())
    }
}
pub struct MemInst {
    pub ty: MemType,
    pub mem: LinearMemory,
}
impl core::fmt::Debug for MemInst {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MemInst")
            .field("ty", &self.ty)
            .finish_non_exhaustive()
    }
}
impl MemInst {
    /// <https://webassembly.github.io/spec/core/exec/modules.html#growing-memories>
    pub fn grow(&mut self, n: u32) -> Result<(), RuntimeError> {
        let len = n + self.mem.pages() as u32;
        if len > Limits::MAX_MEM_PAGES {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if self.ty.limits.max.map(|max| len > max).unwrap_or(false) {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        let limits_prime = Limits {
            min: len,
            max: self.ty.limits.max,
        };
        if self.mem.grow(n.try_into().unwrap()).is_err() {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        self.ty.limits = limits_prime;
        Ok(())
    }
    /// Can never be bigger than 65,356 pages
    pub fn size(&self) -> usize {
        self.mem.len() / (crate::wasm::Limits::MEM_PAGE_SIZE as usize)
    }
}
// pub struct GlobalInstV2 {
//     Local(LocalGlobalInst),
//     Imported(ImportedGlobalInst)
// }
#[derive(Debug)]
pub struct GlobalInst {
    pub ty: GlobalType,
    /// Must be of the same type as specified in `ty`
    pub value: Value,
}
pub struct DataInst {
    pub data: Vec<u8>,
}
impl core::fmt::Debug for DataInst {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DataInst").finish_non_exhaustive()
    }
}
#[derive(Debug)]
pub struct ModuleInst<'b> {
    pub types: Vec<FuncType>,
    pub func_addrs: Vec<FuncAddr>,
    pub table_addrs: Vec<TableAddr>,
    pub mem_addrs: Vec<MemAddr>,
    pub global_addrs: Vec<GlobalAddr>,
    pub elem_addrs: Vec<ElemAddr>,
    pub data_addrs: Vec<DataAddr>,
    ///<https://webassembly.github.io/spec/core/exec/runtime.html#export-instances>
    /// matches the list of ExportInst structs in the spec, however the spec never uses the name attribute
    /// except during linking, which is up to the embedder to implement.
    /// therefore this is a map data structure instead.
    pub exports: BTreeMap<String, ExternVal>,
    pub wasm_bytecode: &'b [u8],
    pub sidetable: Sidetable,
}
