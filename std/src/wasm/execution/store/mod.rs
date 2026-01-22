use self::component::ComponentInst;
use super::assert_validated::UnwrapValidatedExt;
use super::interop::InteropValueList;
use super::interpreter_loop::{data_drop, elem_drop};
use crate::rust_alloc::collections::btree_map::BTreeMap;
use crate::rust_alloc::string::String;
use crate::rust_alloc::vec;
use crate::rust_alloc::vec::Vec;
use crate::wasm::core::indices::TypeIdx;
use crate::wasm::core::reader::span::Span;
use crate::wasm::core::reader::types::data::{DataModeActive, DataSegment};
use crate::wasm::core::reader::types::element::{ActiveElem, ElemItems, ElemMode, ElemType};
use crate::wasm::core::reader::types::export::{Export, ExportDesc};
use crate::wasm::core::reader::types::global::GlobalType;
use crate::wasm::core::reader::types::{
    ExternType, FuncType, ImportSubTypeRelation, MemType, TableType,
};
use crate::wasm::core::reader::WasmReader;
use crate::wasm::execution::config::Config;
use crate::wasm::execution::interpreter_loop::{self, memory_init, table_init};
use crate::wasm::execution::resumable::{
    Dormitory, FreshResumableRef, Resumable, ResumableRef, RunState,
};
use crate::wasm::execution::store::addrs::{
    AddrVec, ComponentInstAddr, DataAddr, ElemAddr, FuncAddr, GlobalAddr, MemAddr, ModuleAddr, TableAddr,
};
use crate::wasm::execution::value::{Ref, Value};
use crate::wasm::execution::{run_const_span, Stack};
use crate::wasm::{RefType, RuntimeError, TrapError, ValidationInfo};
use core::sync::atomic::{AtomicU64, Ordering};
use instances::{
    DataInst, ElemInst, FuncInst, GlobalInst, HostFuncInst, MemInst, ModuleInst, TableInst,
    WasmFuncInst,
};
use linear_memory::LinearMemory;
pub mod addrs;
pub(crate) mod instances;
pub(crate) mod linear_memory;
pub mod component;
pub struct Store<'a, T: Config> {
    pub functions: AddrVec<FuncAddr, FuncInst<T>>,
    pub tables: AddrVec<TableAddr, TableInst>,
    pub memories: AddrVec<MemAddr, MemInst>,
    pub globals: AddrVec<GlobalAddr, GlobalInst>,
    pub(crate) elements: AddrVec<ElemAddr, ElemInst>,
    pub(crate) data: AddrVec<DataAddr, DataInst>,
    pub(crate) modules: AddrVec<ModuleAddr, ModuleInst<'a>>,
    pub(crate) component_instances: AddrVec<ComponentInstAddr, ComponentInst>,
    pub(crate) id: StoreId,
    pub user_data: T,
    pub(crate) dormitory: Dormitory,
    pub caller_module: Option<ModuleAddr>,
    pub wasi_ctx: Option<crate::wasm::wasi::WasiCtx>,
}
impl<'a, T: Config> Store<'a, T> {
    pub fn new(user_data: T) -> Self {
        Self {
            functions: AddrVec::default(),
            tables: AddrVec::default(),
            memories: AddrVec::default(),
            globals: AddrVec::default(),
            elements: AddrVec::default(),
            data: AddrVec::default(),
            modules: AddrVec::default(),
            component_instances: AddrVec::default(),
            id: StoreId::new(),
            dormitory: Dormitory::default(),
            user_data,
            caller_module: None,
            wasi_ctx: None,
        }
    }
    pub fn module_instantiate_unchecked(
        &mut self,
        validation_info: &ValidationInfo<'a>,
        extern_vals: Vec<ExternVal>,
        maybe_fuel: Option<u32>,
    ) -> Result<InstantiationOutcome, RuntimeError> {
        if validation_info.imports.len() != extern_vals.len() {
            return Err(RuntimeError::ExternValsLenMismatch);
        }
        let module_inst = ModuleInst {
            types: validation_info.types.clone(),
            func_addrs: extern_vals.iter().funcs().collect(),
            table_addrs: Vec::new(),
            mem_addrs: Vec::new(),
            global_addrs: extern_vals.iter().globals().collect(),
            elem_addrs: Vec::new(),
            data_addrs: Vec::new(),
            exports: BTreeMap::new(),
            wasm_bytecode: validation_info.wasm,
            sidetable: validation_info.sidetable.clone(),
        };
        let module_addr = self.modules.insert(module_inst);
        let func_addrs: Vec<FuncAddr> = validation_info
            .functions
            .iter()
            .zip(validation_info.func_blocks_stps.iter())
            .map(|(ty_idx, (span, stp))| self.alloc_func((*ty_idx, (*span, *stp)), module_addr))
            .collect();
        self.modules.get_mut(module_addr).func_addrs.extend(func_addrs);
        let maybe_global_init_vals: Result<Vec<Value>, _> = validation_info
            .globals
            .iter()
            .map(|global| {
                run_const_span(validation_info.wasm, &global.init_expr, module_addr, self)
                    .transpose()
                    .unwrap_validated()
            })
            .collect();
        let global_init_vals = maybe_global_init_vals?;
        let mut element_init_ref_lists: Vec<Vec<Ref>> = Vec::with_capacity(validation_info.elements.len());
        for elem in &validation_info.elements {
            let mut new_list = Vec::new();
            match &elem.init {
                ElemItems::RefFuncs(ref_funcs) => {
                    for func_idx in ref_funcs {
                        let func_addr = *self.modules.get(module_addr).func_addrs.get(*func_idx as usize).unwrap_validated();
                        new_list.push(Ref::Func(func_addr));
                    }
                }
                ElemItems::Exprs(_, exprs) => {
                    for expr in exprs {
                        new_list.push(run_const_span(validation_info.wasm, expr, module_addr, self)?.unwrap_validated().try_into().unwrap_validated())
                    }
                }
            }
            element_init_ref_lists.push(new_list);
        }
        let table_addrs: Vec<TableAddr> = validation_info.tables.iter().map(|t| self.alloc_table(*t, Ref::Null(t.et))).collect();
        let mem_addrs: Vec<MemAddr> = validation_info.memories.iter().map(|m| self.alloc_mem(*m)).collect();
        let global_addrs: Vec<GlobalAddr> = validation_info.globals.iter().zip(global_init_vals).map(|(g, v)| self.alloc_global(g.ty, v)).collect();
        let elem_addrs = validation_info.elements.iter().zip(element_init_ref_lists).map(|(e, r)| self.alloc_elem(e.ty(), r)).collect();
        let data_addrs = validation_info.data.iter().map(|d| self.alloc_data(&d.init)).collect();
        let mut table_addrs_mod: Vec<TableAddr> = extern_vals.iter().tables().collect();
        table_addrs_mod.extend(table_addrs);
        let mut mem_addrs_mod: Vec<MemAddr> = extern_vals.iter().mems().collect();
        mem_addrs_mod.extend(mem_addrs);
        self.modules.get_mut(module_addr).global_addrs.extend(global_addrs);
        let export_insts: BTreeMap<String, ExternVal> = validation_info.exports.iter().map(|Export { name, desc }| {
            let module_inst = self.modules.get(module_addr);
            let value = match desc {
                ExportDesc::FuncIdx(func_idx) => ExternVal::Func(module_inst.func_addrs[*func_idx]),
                ExportDesc::TableIdx(table_idx) => ExternVal::Table(table_addrs_mod[*table_idx]),
                ExportDesc::MemIdx(mem_idx) => ExternVal::Mem(mem_addrs_mod[*mem_idx]),
                ExportDesc::GlobalIdx(global_idx) => ExternVal::Global(module_inst.global_addrs[*global_idx]),
            };
            (String::from(name), value)
        }).collect();
        let module_inst = self.modules.get_mut(module_addr);
        module_inst.table_addrs = table_addrs_mod;
        module_inst.mem_addrs = mem_addrs_mod;
        module_inst.elem_addrs = elem_addrs;
        module_inst.data_addrs = data_addrs;
        module_inst.exports = export_insts;
        // Initialize active element segments
        for (i, ElemType { init: elem_items, mode }) in validation_info.elements.iter().enumerate() {
            if let ElemMode::Active(ActiveElem { table_idx, init_expr }) = mode {
                let n = elem_items.len() as u32;
                let d: i32 = run_const_span(validation_info.wasm, init_expr, module_addr, self)?.unwrap_validated().try_into().unwrap_validated();
                table_init(&self.modules, &mut self.tables, &self.elements, module_addr, i, *table_idx as usize, n, 0, d)?;
                elem_drop(&self.modules, &mut self.elements, module_addr, i)?;
            } else if let ElemMode::Declarative = mode {
                elem_drop(&self.modules, &mut self.elements, module_addr, i)?;
            }
        }
        // Initialize active data segments
        for (i, DataSegment { init, mode }) in validation_info.data.iter().enumerate() {
            if let crate::wasm::core::reader::types::data::DataMode::Active(DataModeActive { memory_idx, offset }) = mode {
                if *memory_idx != 0 { return Err(RuntimeError::MoreThanOneMemory); }
                let n = init.len() as u32;
                let d: i32 = run_const_span(validation_info.wasm, offset, module_addr, self)?.unwrap_validated().try_into().unwrap_validated();
                memory_init(&self.modules, &mut self.memories, &self.data, module_addr, i, 0, n, 0, d)?;
                data_drop(&self.modules, &mut self.data, module_addr, i)?;
            }
        }
        let maybe_remaining_fuel = if let Some(func_idx) = validation_info.start {
            let func_addr = self.modules.get(module_addr).func_addrs[func_idx];
            let RunState::Finished { maybe_remaining_fuel, .. } = self.invoke_unchecked(func_addr, Vec::new(), maybe_fuel)? else { return Err(RuntimeError::OutOfFuel); };
            maybe_remaining_fuel
        } else { maybe_fuel };
        Ok(InstantiationOutcome { module_addr, maybe_remaining_fuel })
    }
    pub fn func_alloc_unchecked(&mut self, func_type: FuncType, host_func: for<'x> fn(&mut Store<'x, T>, Vec<Value>) -> Result<Vec<Value>, HaltExecutionError>) -> FuncAddr {
        self.functions.insert(FuncInst::HostFunc(HostFuncInst { function_type: func_type, hostcode: host_func }))
    }
    pub fn func_type_unchecked(&self, func_addr: FuncAddr) -> FuncType {
        self.functions.get(func_addr).ty()
    }
    fn alloc_func(&mut self, func: (TypeIdx, (Span, usize)), module_addr: ModuleAddr) -> FuncAddr {
        let (ty, (span, stp)) = func;
        let mut reader = WasmReader::new(self.modules.get(module_addr).wasm_bytecode);
        reader.move_start_to(span).unwrap_validated();
        let (locals, bytes_read) = reader.measure_num_read_bytes(crate::wasm::validation::code::read_declared_locals).unwrap_validated();
        let code_expr = reader.make_span(span.len() - bytes_read).unwrap_validated();
        self.functions.insert(FuncInst::WasmFunc(WasmFuncInst { function_type: self.modules.get(module_addr).types[ty].clone(), _ty: ty, locals, code_expr, stp, module_addr }))
    }
    fn alloc_table(&mut self, ty: TableType, reff: Ref) -> TableAddr {
        self.tables.insert(TableInst { ty, elem: vec![reff; ty.lim.min as usize] })
    }
    fn alloc_mem(&mut self, ty: MemType) -> MemAddr {
        self.memories.insert(MemInst { ty, mem: LinearMemory::new_with_initial_pages(ty.limits.min.try_into().unwrap_validated()) })
    }
    fn alloc_global(&mut self, ty: GlobalType, value: Value) -> GlobalAddr {
        self.globals.insert(GlobalInst { ty, value })
    }
    fn alloc_elem(&mut self, _ty: RefType, references: Vec<Ref>) -> ElemAddr {
        self.elements.insert(ElemInst { _ty, references })
    }
    fn alloc_data(&mut self, bytes: &[u8]) -> DataAddr {
        self.data.insert(DataInst { data: Vec::from(bytes) })
    }
    pub fn invoke_unchecked(&mut self, func_addr: FuncAddr, params: Vec<Value>, maybe_fuel: Option<u32>) -> Result<RunState, RuntimeError> {
        self.resume_unchecked(self.create_resumable_unchecked(func_addr, params, maybe_fuel)?)
    }
    pub fn create_resumable_unchecked(&self, func_addr: FuncAddr, params: Vec<Value>, maybe_fuel: Option<u32>) -> Result<ResumableRef, RuntimeError> {
        let func_inst = self.functions.get(func_addr);
        let func_ty = func_inst.ty();
        let param_types = params.iter().map(|v| v.to_ty()).collect::<Vec<_>>();
        if func_ty.params.valtypes != param_types { return Err(RuntimeError::FunctionInvocationSignatureMismatch); }
        Ok(ResumableRef::Fresh(FreshResumableRef { func_addr, params, maybe_fuel }))
    }
    pub fn resume_unchecked(&mut self, mut resumable_ref: ResumableRef) -> Result<RunState, RuntimeError> {
        match resumable_ref {
            ResumableRef::Fresh(FreshResumableRef { func_addr, params, maybe_fuel }) => {
                let func_inst = self.functions.get(func_addr);
                match func_inst {
                    FuncInst::HostFunc(host_func_inst) => {
                        let hostcode = host_func_inst.hostcode;
                        let returns = hostcode(self, params).map_err(|HaltExecutionError(code)| RuntimeError::HostFunctionHaltedExecution(code))?;
                        Ok(RunState::Finished { values: returns, maybe_remaining_fuel: maybe_fuel })
                    }
                    FuncInst::WasmFunc(wasm_func_inst) => {
                        let mut stack = Stack::new_with_values(params);
                        stack.push_call_frame::<T>(usize::MAX, &wasm_func_inst.function_type, &wasm_func_inst.locals, usize::MAX, usize::MAX)?;
                        let mut resumable = Resumable { current_func_addr: func_addr, stack, pc: wasm_func_inst.code_expr.from, stp: wasm_func_inst.stp, maybe_fuel };
                        let result = interpreter_loop::run(&mut resumable, self)?;
                        match result {
                            None => Ok(RunState::Finished { values: resumable.stack.into_values(), maybe_remaining_fuel: resumable.maybe_fuel }),
                            Some(required_fuel) => Ok(RunState::Resumable { resumable_ref: ResumableRef::Invoked(self.dormitory.insert(resumable)), required_fuel }),
                        }
                    }
                }
            }
            _ => Err(RuntimeError::Trap(TrapError::ReachedUnreachable)),
        }
    }
    pub fn instance_export_unchecked(&self, module_addr: ModuleAddr, name: &str) -> Result<ExternVal, RuntimeError> {
        self.modules.get(module_addr).exports.get(name).copied().ok_or(RuntimeError::UnknownExport)
    }
    pub fn table_alloc_unchecked(&mut self, table_type: TableType, r#ref: Ref) -> Result<TableAddr, RuntimeError> { Ok(self.alloc_table(table_type, r#ref)) }
    pub fn table_type_unchecked(&self, table_addr: TableAddr) -> TableType { self.tables.get(table_addr).ty }
    pub fn table_read_unchecked(&self, table_addr: TableAddr, i: u32) -> Result<Ref, RuntimeError> {
        self.tables.get(table_addr).elem.get(i as usize).copied().ok_or(RuntimeError::Trap(TrapError::TableOrElementAccessOutOfBounds))
    }
    pub fn table_write_unchecked(&mut self, table_addr: TableAddr, i: u32, r#ref: Ref) -> Result<(), RuntimeError> {
        let ti = self.tables.get_mut(table_addr);
        *ti.elem.get_mut(i as usize).ok_or(RuntimeError::Trap(TrapError::TableOrElementAccessOutOfBounds))? = r#ref;
        Ok(())
    }
    pub fn table_size_unchecked(&self, table_addr: TableAddr) -> u32 { self.tables.get(table_addr).elem.len() as u32 }
    pub fn mem_alloc_unchecked(&mut self, mem_type: MemType) -> MemAddr { self.alloc_mem(mem_type) }
    pub fn mem_type_unchecked(&self, mem_addr: MemAddr) -> MemType { self.memories.get(mem_addr).ty }
    pub fn mem_read_unchecked(&self, mem_addr: MemAddr, i: u32) -> Result<u8, RuntimeError> {
        self.memories.get(mem_addr).mem.load::<1, u8>(i as usize).map_err(|_| RuntimeError::Trap(TrapError::MemoryOrDataAccessOutOfBounds))
    }
    pub fn mem_write_unchecked(&self, mem_addr: MemAddr, i: u32, byte: u8) -> Result<(), RuntimeError> {
        self.memories.get(mem_addr).mem.store::<1, u8>(i as usize, byte).map_err(|_| RuntimeError::Trap(TrapError::MemoryOrDataAccessOutOfBounds))
    }
    pub fn mem_size_unchecked(&self, mem_addr: MemAddr) -> u32 { self.memories.get(mem_addr).size() as u32 }
    pub fn mem_grow_unchecked(&mut self, mem_addr: MemAddr, n: u32) -> Result<(), RuntimeError> { self.memories.get_mut(mem_addr).grow(n) }
    pub fn global_alloc_unchecked(&mut self, global_type: GlobalType, val: Value) -> Result<GlobalAddr, RuntimeError> { Ok(self.alloc_global(global_type, val)) }
    pub fn global_type_unchecked(&self, global_addr: GlobalAddr) -> GlobalType { self.globals.get(global_addr).ty }
    pub fn global_read_unchecked(&self, global_addr: GlobalAddr) -> Value { self.globals.get(global_addr).value }
    pub fn global_write_unchecked(&mut self, global_addr: GlobalAddr, val: Value) -> Result<(), RuntimeError> {
        let gi = self.globals.get_mut(global_addr);
        gi.value = val;
        Ok(())
    }

    pub fn get_wasm_base_ptr(&self) -> *mut u8 {
        let module_addr = self.caller_module.unwrap_or(0);
        let mem_addr = *self.modules.get(module_addr).mem_addrs.get(0).unwrap_or(&0);
        self.memories.get(mem_addr).mem.get_base_ptr()
    }

    pub fn access_fuel_mut_unchecked<R>(&mut self, _resumable_ref: &mut ResumableRef, _f: impl FnOnce(&mut Option<u32>) -> R) -> Result<R, RuntimeError> {
        Err(RuntimeError::Trap(TrapError::ReachedUnreachable))
    }
    pub fn invoke_without_fuel_unchecked(&mut self, func_addr: FuncAddr, params: Vec<Value>) -> Result<Vec<Value>, RuntimeError> {
        match self.invoke_unchecked(func_addr, params, None)? {
            RunState::Finished { values, .. } => Ok(values),
            _ => unreachable!(),
        }
    }
}
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct StoreId(u64);
impl StoreId {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(NEXT.fetch_add(1, Ordering::SeqCst))
    }
}
#[derive(Debug, Copy, Clone)]
pub struct HaltExecutionError(pub i32);
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ExternVal { Func(FuncAddr), Table(TableAddr), Mem(MemAddr), Global(GlobalAddr) }
impl ExternVal {
    pub fn extern_type<'a, T: Config>(&self, store: &Store<'a, T>) -> ExternType {
        match self {
            ExternVal::Func(addr) => ExternType::Func(store.functions.get(*addr).ty()),
            ExternVal::Table(addr) => ExternType::Table(store.tables.get(*addr).ty),
            ExternVal::Mem(addr) => ExternType::Mem(store.memories.get(*addr).ty),
            ExternVal::Global(addr) => ExternType::Global(store.globals.get(*addr).ty),
        }
    }
    pub fn as_func(self) -> Option<FuncAddr> { if let ExternVal::Func(a) = self { Some(a) } else { None } }
}
pub trait ExternFilterable {
    fn funcs(self) -> impl Iterator<Item=FuncAddr>;
    fn globals(self) -> impl Iterator<Item=GlobalAddr>;
    fn tables(self) -> impl Iterator<Item=TableAddr>;
    fn mems(self) -> impl Iterator<Item=MemAddr>;
}
impl<'a, I> ExternFilterable for I
where
    I: Iterator<Item=&'a ExternVal>,
{
    fn funcs(self) -> impl Iterator<Item=FuncAddr> { self.filter_map(|v| v.as_func()) }
    fn globals(self) -> impl Iterator<Item=GlobalAddr> { self.filter_map(|v| if let ExternVal::Global(a) = v { Some(*a) } else { None }) }
    fn tables(self) -> impl Iterator<Item=TableAddr> { self.filter_map(|v| if let ExternVal::Table(a) = v { Some(*a) } else { None }) }
    fn mems(self) -> impl Iterator<Item=MemAddr> { self.filter_map(|v| if let ExternVal::Mem(a) = v { Some(*a) } else { None }) }
}
pub struct InstantiationOutcome {
    pub module_addr: ModuleAddr,
    pub maybe_remaining_fuel: Option<u32>,
}