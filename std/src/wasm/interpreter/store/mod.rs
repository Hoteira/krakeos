use self::component::ComponentInst;
use crate::alloc::collections::btree_map::BTreeMap;
use crate::alloc::string::String;
use crate::alloc::vec;
use crate::alloc::vec::Vec;
use crate::wasm::common::assert_validated::UnwrapValidatedExt;
use crate::wasm::common::config::Config;
use crate::wasm::common::indices::TypeIdx;
use crate::wasm::common::reader::WasmReader;
use crate::wasm::common::reader::span::Span;
use crate::wasm::common::reader::types::Limits;
use crate::wasm::common::reader::types::RefType;
use crate::wasm::common::reader::types::ValType;
use crate::wasm::common::reader::types::data::{DataMode, DataSegment};
use crate::wasm::common::reader::types::element::{ActiveElem, ElemItems, ElemMode, ElemType};
use crate::wasm::common::reader::types::export::{Export, ExportDesc};
use crate::wasm::common::reader::types::global::GlobalType;
use crate::wasm::common::reader::types::{ExternType, FuncType, MemType, TableType};
use crate::wasm::common::runtime_error::{RuntimeError, TrapError};
use crate::wasm::common::validation::ValidationInfo;
use crate::wasm::common::value::{Ref, Value};
use crate::wasm::common::value_stack::Stack;
use crate::wasm::interpreter::loop_executor::{self, memory_init, table_init};
use crate::wasm::interpreter::loop_executor::{data_drop, elem_drop};
use crate::wasm::interpreter::resumable::{
    Dormitory, FreshResumableRef, Resumable, ResumableRef, RunState,
};
use crate::wasm::interpreter::store::addrs::{
    AddrVec, ComponentInstAddr, DataAddr, ElemAddr, FuncAddr, GlobalAddr, MemAddr, ModuleAddr,
    TableAddr,
};
use core::sync::atomic::{AtomicU64, Ordering};
use instances::{
    DataInst, ElemInst, FuncInst, GlobalInst, HostFuncInst, MemInst, ModuleInst, TableInst,
    WasmFuncInst,
};
use linear_memory::LinearMemory;

pub mod addrs;
pub mod component;
pub(crate) mod instances;
pub(crate) mod linear_memory;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct StoreId(u64);
impl StoreId {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(NEXT.fetch_add(1, Ordering::SeqCst))
    }
}

pub struct Store<'a, T: Config> {
    pub functions: AddrVec<FuncAddr, FuncInst<T>>,
    pub tables: AddrVec<TableAddr, TableInst>,
    pub memories: AddrVec<MemAddr, MemInst>,
    pub globals: AddrVec<GlobalAddr, GlobalInst>,
    pub(crate) elements: AddrVec<ElemAddr, ElemInst>,
    pub(crate) data: AddrVec<DataAddr, DataInst>,
    pub(crate) modules: AddrVec<ModuleAddr, ModuleInst<'a>>,
    pub(crate) component_instances: AddrVec<ComponentInstAddr, ComponentInst>,
    pub(crate) aot_modules: Vec<crate::wasm::aot::runtime::AotModule>,
    pub aot_enabled: bool,
    pub(crate) id: StoreId,
    pub user_data: T,
    pub(crate) dormitory: Dormitory,
    pub caller_module: Option<ModuleAddr>,
    pub wasi_ctx: Option<crate::wasm::wasi::WasiCtx>,
    pub sas_memory_base: Option<u64>,
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
            aot_modules: Vec::new(),
            aot_enabled: false,
            id: StoreId::new(),
            dormitory: Dormitory::default(),
            user_data,
            caller_module: None,
            wasi_ctx: None,
            sas_memory_base: None,
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
        self.modules
            .get_mut(module_addr)
            .func_addrs
            .extend(func_addrs);

        // Constant expressions handling
        let mut global_init_vals = Vec::new();
        for global in &validation_info.globals {
            let val = crate::wasm::interpreter::loop_executor::run_const_span(
                validation_info.wasm,
                &global.init_expr,
                module_addr,
                self,
            )?
            .unwrap_validated();
            global_init_vals.push(val);
        }

        let mut element_init_ref_lists: Vec<Vec<Ref>> =
            Vec::with_capacity(validation_info.elements.len());
        for elem in &validation_info.elements {
            let mut new_list = Vec::new();
            match &elem.init {
                ElemItems::RefFuncs(ref_funcs) => {
                    for func_idx in ref_funcs {
                        let func_addr = *self
                            .modules
                            .get(module_addr)
                            .func_addrs
                            .get(*func_idx as usize)
                            .unwrap_validated();
                        new_list.push(Ref::Func(func_addr));
                    }
                }
                ElemItems::Exprs(_, exprs) => {
                    for expr in exprs {
                        new_list.push(
                            crate::wasm::interpreter::loop_executor::run_const_span(
                                validation_info.wasm,
                                expr,
                                module_addr,
                                self,
                            )?
                            .unwrap_validated()
                            .try_into()
                            .unwrap_validated(),
                        )
                    }
                }
            }
            element_init_ref_lists.push(new_list);
        }
        let table_addrs: Vec<TableAddr> = validation_info
            .tables
            .iter()
            .map(|t| self.alloc_table(*t, Ref::Null(t.et)))
            .collect();
        let mem_addrs: Vec<MemAddr> = validation_info
            .memories
            .iter()
            .map(|m| self.alloc_mem(*m))
            .collect();
        let global_addrs: Vec<GlobalAddr> = validation_info
            .globals
            .iter()
            .zip(global_init_vals)
            .map(|(g, v)| self.alloc_global(g.ty, v))
            .collect();
        let elem_addrs = validation_info
            .elements
            .iter()
            .zip(element_init_ref_lists)
            .map(|(e, r)| self.alloc_elem(e.ty(), r))
            .collect();
        let data_addrs = validation_info
            .data
            .iter()
            .map(|d| self.alloc_data(&d.init))
            .collect();
        let mut table_addrs_mod: Vec<TableAddr> = extern_vals.iter().tables().collect();
        table_addrs_mod.extend(table_addrs);
        let mut mem_addrs_mod: Vec<MemAddr> = extern_vals.iter().mems().collect();
        mem_addrs_mod.extend(mem_addrs);
        self.modules
            .get_mut(module_addr)
            .global_addrs
            .extend(global_addrs);
        let export_insts: BTreeMap<String, ExternVal> = validation_info
            .exports
            .iter()
            .map(|Export { name, desc }| {
                let module_inst = self.modules.get(module_addr);
                let value = match desc {
                    ExportDesc::FuncIdx(func_idx) => {
                        ExternVal::Func(module_inst.func_addrs[*func_idx])
                    }
                    ExportDesc::TableIdx(table_idx) => {
                        ExternVal::Table(table_addrs_mod[*table_idx])
                    }
                    ExportDesc::MemIdx(mem_idx) => ExternVal::Mem(mem_addrs_mod[*mem_idx]),
                    ExportDesc::GlobalIdx(global_idx) => {
                        ExternVal::Global(module_inst.global_addrs[*global_idx])
                    }
                };
                (String::from(name), value)
            })
            .collect();
        let module_inst = self.modules.get_mut(module_addr);
        module_inst.table_addrs = table_addrs_mod;
        module_inst.mem_addrs = mem_addrs_mod;
        module_inst.elem_addrs = elem_addrs;
        module_inst.data_addrs = data_addrs;
        module_inst.exports = export_insts;

        // Initialize active element segments
        for (
            i,
            ElemType {
                init: elem_items,
                mode,
            },
        ) in validation_info.elements.iter().enumerate()
        {
            if let ElemMode::Active(active) = mode {
                let n = elem_items.len() as u32;
                let d: i32 = crate::wasm::interpreter::loop_executor::run_const_span(
                    validation_info.wasm,
                    &active.init_expr,
                    module_addr,
                    self,
                )?
                .unwrap_validated()
                .try_into()
                .unwrap_validated();
                table_init(
                    &self.modules,
                    &mut self.tables,
                    &self.elements,
                    module_addr,
                    i,
                    active.table_idx as usize,
                    n,
                    0,
                    d,
                )?;
                elem_drop(&self.modules, &mut self.elements, module_addr, i)?;
            } else if let ElemMode::Declarative = mode {
                elem_drop(&self.modules, &mut self.elements, module_addr, i)?;
            }
        }
        // Initialize active data segments
        for (i, DataSegment { init, mode }) in validation_info.data.iter().enumerate() {
            if let DataMode::Active(active) = mode {
                if active.memory_idx != 0 {
                    return Err(RuntimeError::MoreThanOneMemory);
                }
                let n = init.len() as u32;
                let d: i32 = crate::wasm::interpreter::loop_executor::run_const_span(
                    validation_info.wasm,
                    &active.offset,
                    module_addr,
                    self,
                )?
                .unwrap_validated()
                .try_into()
                .unwrap_validated();
                memory_init(
                    &self.modules,
                    &mut self.memories,
                    &self.data,
                    module_addr,
                    i,
                    0,
                    n,
                    0,
                    d,
                )?;
                data_drop(&self.modules, &mut self.data, module_addr, i)?;
            }
        }

        // --- AOT Compilation ---
        if self.aot_enabled {
            let mut compiler = crate::wasm::aot::AotCompiler::new(validation_info);
            let aot_module = compiler.compile_module();
            for (i, offset) in aot_module.func_offsets.iter().enumerate() {
                let func_idx = validation_info.imports_length.imported_functions + i;
                let func_addr = self.modules.get(module_addr).func_addrs[func_idx];
                if let FuncInst::WasmFunc(wasm_func) = self.functions.get_mut(func_addr) {
                    wasm_func.aot_ptr = Some(unsafe { aot_module.code_ptr.add(*offset) as usize });
                }
            }
            self.aot_modules.push(aot_module);
        }
        // -----------------------

        let maybe_remaining_fuel = if let Some(func_idx) = validation_info.start {
            let func_addr = self.modules.get(module_addr).func_addrs[func_idx];
            let RunState::Finished {
                maybe_remaining_fuel,
                ..
            } = self.invoke_unchecked(func_addr, Vec::new(), maybe_fuel)?
            else {
                return Err(RuntimeError::OutOfFuel);
            };
            maybe_remaining_fuel
        } else {
            maybe_fuel
        };
        Ok(InstantiationOutcome {
            module_addr,
            maybe_remaining_fuel,
        })
    }
    pub fn func_alloc_unchecked(
        &mut self,
        func_type: FuncType,
        host_func: for<'x> fn(
            &mut Store<'x, T>,
            Vec<Value>,
        ) -> Result<Vec<Value>, HaltExecutionError>,
    ) -> FuncAddr {
        self.functions.insert(FuncInst::HostFunc(HostFuncInst {
            function_type: func_type,
            hostcode: host_func,
        }))
    }
    pub fn func_type_unchecked(&self, func_addr: FuncAddr) -> FuncType {
        self.functions.get(func_addr).ty()
    }
    fn alloc_func(&mut self, func: (TypeIdx, (Span, usize)), module_addr: ModuleAddr) -> FuncAddr {
        let (ty, (span, stp)) = func;
        let mut reader = WasmReader::new(self.modules.get(module_addr).wasm_bytecode);
        reader.move_start_to(span).unwrap_validated();
        let (locals, bytes_read) = reader
            .measure_num_read_bytes(crate::wasm::common::validation::code::read_declared_locals)
            .unwrap_validated();
        let code_expr = reader.make_span(span.len() - bytes_read).unwrap_validated();
        self.functions.insert(FuncInst::WasmFunc(WasmFuncInst {
            function_type: self.modules.get(module_addr).types[ty].clone(),
            _ty: ty,
            locals,
            code_expr,
            stp,
            module_addr,
            aot_ptr: None,
        }))
    }
    fn alloc_table(&mut self, ty: TableType, reff: Ref) -> TableAddr {
        self.tables.insert(TableInst {
            ty,
            elem: vec![reff; ty.lim.min as usize],
        })
    }
    fn alloc_mem(&mut self, ty: MemType) -> MemAddr {
        let mem = if let Some(base) = self.sas_memory_base {
            LinearMemory::new_sas(base, ty.limits.min.try_into().unwrap_validated())
        } else {
            LinearMemory::new_with_initial_pages(ty.limits.min.try_into().unwrap_validated())
        };
        self.memories.insert(MemInst {
            ty,
            mem,
        })
    }
    fn alloc_global(&mut self, ty: GlobalType, value: Value) -> GlobalAddr {
        self.globals.insert(GlobalInst { ty, value })
    }
    fn alloc_elem(&mut self, _ty: RefType, references: Vec<Ref>) -> ElemAddr {
        self.elements.insert(ElemInst { _ty, references })
    }
    fn alloc_data(&mut self, bytes: &[u8]) -> DataAddr {
        self.data.insert(DataInst {
            data: Vec::from(bytes),
        })
    }
    pub fn invoke_unchecked(
        &mut self,
        func_addr: FuncAddr,
        params: Vec<Value>,
        maybe_fuel: Option<u32>,
    ) -> Result<RunState, RuntimeError> {
        self.resume_unchecked(self.create_resumable_unchecked(func_addr, params, maybe_fuel)?)
    }
    pub fn create_resumable_unchecked(
        &self,
        func_addr: FuncAddr,
        params: Vec<Value>,
        maybe_fuel: Option<u32>,
    ) -> Result<ResumableRef, RuntimeError> {
        let func_inst = self.functions.get(func_addr);
        let func_ty = func_inst.ty();
        let param_types = params.iter().map(|v| v.to_ty()).collect::<Vec<_>>();
        if func_ty.params.valtypes != param_types {
            return Err(RuntimeError::FunctionInvocationSignatureMismatch);
        }
        Ok(ResumableRef::Fresh(FreshResumableRef {
            func_addr,
            params,
            maybe_fuel,
        }))
    }
    pub fn resume_unchecked(
        &mut self,
        resumable_ref: ResumableRef,
    ) -> Result<RunState, RuntimeError> {
        match resumable_ref {
            ResumableRef::Fresh(FreshResumableRef {
                func_addr,
                params,
                maybe_fuel,
            }) => {
                let func_inst = self.functions.get(func_addr);
                match func_inst {
                    FuncInst::HostFunc(host_func_inst) => {
                        let hostcode = host_func_inst.hostcode;
                        let returns =
                            hostcode(self, params).map_err(|HaltExecutionError(code)| {
                                RuntimeError::HostFunctionHaltedExecution(code)
                            })?;
                        Ok(RunState::Finished {
                            values: returns,
                            maybe_remaining_fuel: maybe_fuel,
                        })
                    }
                    FuncInst::WasmFunc(wasm_func_inst) => {
                        let aot_ptr = wasm_func_inst.aot_ptr;
                        let func_type = wasm_func_inst.function_type.clone();
                        let module_addr = wasm_func_inst.module_addr;

                        if self.aot_enabled && aot_ptr.is_some() {
                            let aot_ptr = aot_ptr.unwrap();
                            let mut fuel = maybe_fuel.unwrap_or(1_000_000_000);
                            let mem_addr = self.modules.get(module_addr).mem_addrs.get(0).copied();
                            let (memory_base, memory_size) = if let Some(mem_addr) = mem_addr {
                                let mem = &self.memories.get(mem_addr).mem;
                                (mem.get_base_ptr(), mem.len())
                            } else {
                                (core::ptr::null_mut(), 0)
                            };

                            let stack_size = 1024 * 1024 * 4; // 4MB stack
                            let stack_ptr =
                                unsafe { crate::memory::malloc(stack_size) } as *mut u128;
                            let locals_size = 1024 * 64; // 64KB locals
                            let locals_ptr =
                                unsafe { crate::memory::malloc(locals_size) } as *mut u128;

                            let mut trap_code: i32 = 0;

                            let mut ctx = crate::wasm::aot::runtime::AotContext {
                                store: self as *mut _ as *mut usize,
                                fuel: &mut fuel as *mut u32,
                                memory_base,
                                memory_size,
                                stack_base: stack_ptr,
                                locals_base: locals_ptr,
                                module_addr,
                                stack_limit: stack_ptr as usize,
                                trap_code: &mut trap_code as *mut i32,
                            };

                            // Prepare AOT stack with parameters
                            let mut sp = unsafe { stack_ptr.add(stack_size / 16) };
                            for param in params.iter() {
                                sp = unsafe { sp.sub(1) };
                                unsafe {
                                    *sp = param.to_u128();
                                }
                            }

                            // crate::debugln!("WASI: [AOT] Entering generated machine code at {:#x}...", aot_ptr);

                            let final_sp: *mut u128;

                            #[cfg(not(target_arch = "wasm32"))]
                            unsafe {
                                core::arch::asm!(
                                    "push rbp",
                                    "mov rbp, rsp",
                                    "mov rsp, {0}",       // Switch to AOT stack
                                    "call {1}",           // Call AOT code
                                    "mov rsp, rbp",       // Restore host RSP
                                    "pop rbp",
                                    in(reg) sp,
                                    in(reg) aot_ptr,
                                    inout("rdi") &mut ctx => _,
                                    lateout("rax") final_sp,
                                    clobber_abi("C"),
                                );
                            }

                            #[cfg(target_arch = "wasm32")]
                            {
                                let _ = sp;
                                let _ = aot_ptr;
                                let _ = &mut ctx;
                                panic!("AOT execution only supported on native targets");
                            }

                            // Collect results BEFORE freeing the stack memory
                            let aot_result: Result<Vec<Value>, RuntimeError> = if trap_code != 0 {
                                Err(RuntimeError::HostFunctionHaltedExecution(trap_code))
                            } else {
                                let num_returns = func_type.returns.valtypes.len();
                                let mut results = Vec::with_capacity(num_returns);
                                for i in 0..num_returns {
                                    let val_ptr = unsafe { final_sp.add(i) };
                                    let val_type = func_type.returns.valtypes[num_returns - 1 - i];
                                    let val = Value::from_u128(unsafe { *val_ptr }, val_type);
                                    results.push(val);
                                }
                                results.reverse();
                                Ok(results)
                            };

                            // Free the AOT stack and locals — these were leaking on every call.
                            unsafe {
                                crate::memory::free(stack_ptr as usize, stack_size);
                                crate::memory::free(locals_ptr as usize, locals_size);
                            }

                            return match aot_result {
                                Ok(values) => Ok(RunState::Finished {
                                    values,
                                    maybe_remaining_fuel: if maybe_fuel.is_some() {
                                        Some(fuel)
                                    } else {
                                        None
                                    },
                                }),
                                Err(e) => Err(e),
                            };
                        }

                        // Fallback to interpreter
                        let mut stack = Stack::new();
                        for param in params {
                            stack.push_value::<T>(param)?;
                        }
                        stack.push_call_frame::<T>(
                            0, // dummy return func
                            &wasm_func_inst.function_type,
                            &wasm_func_inst.locals,
                            0, // return addr
                            0, // return stp
                        )?;
                        let mut resumable = Resumable {
                            stack,
                            pc: wasm_func_inst.code_expr.from(),
                            stp: wasm_func_inst.stp,
                            current_func_addr: func_addr,
                            maybe_fuel,
                        };
                        match loop_executor::run::<T>(&mut resumable, self)? {
                            None => Ok(RunState::Finished {
                                values: resumable.stack.into_values(),
                                maybe_remaining_fuel: resumable.maybe_fuel,
                            }),
                            Some(required_fuel) => Ok(RunState::Resumable {
                                resumable_ref: ResumableRef::Invoked(
                                    self.dormitory.insert(resumable),
                                ),
                                required_fuel,
                            }),
                        }
                    }
                }
            }
            _ => Err(RuntimeError::Trap(TrapError::ReachedUnreachable)),
        }
    }
    pub fn instance_export_unchecked(
        &self,
        module_addr: ModuleAddr,
        name: &str,
    ) -> Result<ExternVal, RuntimeError> {
        self.modules
            .get(module_addr)
            .exports
            .get(name)
            .copied()
            .ok_or(RuntimeError::UnknownExport)
    }
    pub fn table_alloc_unchecked(
        &mut self,
        table_type: TableType,
        r#ref: Ref,
    ) -> Result<TableAddr, RuntimeError> {
        Ok(self.alloc_table(table_type, r#ref))
    }
    pub fn table_type_unchecked(&self, table_addr: TableAddr) -> TableType {
        self.tables.get(table_addr).ty
    }
    pub fn table_read_unchecked(&self, table_addr: TableAddr, i: u32) -> Result<Ref, RuntimeError> {
        self.tables
            .get(table_addr)
            .elem
            .get(i as usize)
            .copied()
            .ok_or(RuntimeError::Trap(
                TrapError::TableOrElementAccessOutOfBounds,
            ))
    }
    pub fn table_write_unchecked(
        &mut self,
        table_addr: TableAddr,
        i: u32,
        r#ref: Ref,
    ) -> Result<(), RuntimeError> {
        let ti = self.tables.get_mut(table_addr);
        *ti.elem.get_mut(i as usize).ok_or(RuntimeError::Trap(
            TrapError::TableOrElementAccessOutOfBounds,
        ))? = r#ref;
        Ok(())
    }
    pub fn table_size_unchecked(&self, table_addr: TableAddr) -> u32 {
        self.tables.get(table_addr).elem.len() as u32
    }
    pub fn mem_alloc_unchecked(&mut self, mem_type: MemType) -> MemAddr {
        self.alloc_mem(mem_type)
    }
    pub fn mem_type_unchecked(&self, mem_addr: MemAddr) -> MemType {
        self.memories.get(mem_addr).ty
    }
    pub fn mem_read_unchecked(&self, mem_addr: MemAddr, i: u32) -> Result<u8, RuntimeError> {
        self.memories
            .get(mem_addr)
            .mem
            .load::<1, u8>(i as usize)
            .map_err(|_| RuntimeError::Trap(TrapError::MemoryOrDataAccessOutOfBounds))
    }
    pub fn mem_write_unchecked(
        &self,
        mem_addr: MemAddr,
        i: u32,
        byte: u8,
    ) -> Result<(), RuntimeError> {
        self.memories
            .get(mem_addr)
            .mem
            .store::<1, u8>(i as usize, byte)
            .map_err(|_| RuntimeError::Trap(TrapError::MemoryOrDataAccessOutOfBounds))
    }
    pub fn mem_size_unchecked(&self, mem_addr: MemAddr) -> u32 {
        self.memories.get(mem_addr).size() as u32
    }
    pub fn mem_grow_unchecked(&mut self, mem_addr: MemAddr, n: u32) -> Result<(), RuntimeError> {
        self.memories.get_mut(mem_addr).grow(n)
    }
    pub fn global_alloc_unchecked(
        &mut self,
        global_type: GlobalType,
        val: Value,
    ) -> Result<GlobalAddr, RuntimeError> {
        Ok(self.alloc_global(global_type, val))
    }
    pub fn global_type_unchecked(&self, global_addr: GlobalAddr) -> GlobalType {
        self.globals.get(global_addr).ty
    }
    pub fn global_read_unchecked(&self, global_addr: GlobalAddr) -> Value {
        self.globals.get(global_addr).value
    }
    pub fn global_write_unchecked(
        &mut self,
        global_addr: GlobalAddr,
        val: Value,
    ) -> Result<(), RuntimeError> {
        let gi = self.globals.get_mut(global_addr);
        gi.value = val;
        Ok(())
    }

    pub fn get_wasm_base_ptr(&self) -> *mut u8 {
        let module_addr = self.caller_module.unwrap_or(0);
        let mem_addr = *self.modules.get(module_addr).mem_addrs.get(0).unwrap_or(&0);
        self.memories.get(mem_addr).mem.get_base_ptr()
    }

    pub fn access_fuel_mut_unchecked<R>(
        &mut self,
        _resumable_ref: &mut ResumableRef,
        _f: impl FnOnce(&mut Option<u32>) -> R,
    ) -> Result<R, RuntimeError> {
        Err(RuntimeError::Trap(TrapError::ReachedUnreachable))
    }
    pub fn invoke_without_fuel_unchecked(
        &mut self,
        func_addr: FuncAddr,
        params: Vec<Value>,
    ) -> Result<Vec<Value>, RuntimeError> {
        match self.invoke_unchecked(func_addr, params, None)? {
            RunState::Finished { values, .. } => Ok(values),
            _ => unreachable!(),
        }
    }
    pub fn compile_all(&mut self) {
        let validation_info = ValidationInfo {
            wasm: &[], // Not used for this purpose
            types: Vec::new(),
            imports: Vec::new(),
            functions: Vec::new(),
            tables: Vec::new(),
            memories: Vec::new(),
            globals: Vec::new(),
            functions_types: Vec::new(),
            exports: Vec::new(),
            func_blocks_stps: Vec::new(),
            sidetable: Vec::new(),
            data: Vec::new(),
            start: None,
            elements: Vec::new(),
            imports_length: crate::wasm::common::validation::ImportsLength {
                imported_functions: 0,
                imported_globals: 0,
                imported_memories: 0,
                imported_tables: 0,
            },
            component: None,
        };
        // This is a placeholder since we don't have the original validation info here.
        // In a real scenario, we'd store it or re-validate.
    }
}

#[derive(Debug, Copy, Clone)]
pub struct HaltExecutionError(pub i32);

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ExternVal {
    Func(FuncAddr),
    Table(TableAddr),
    Mem(MemAddr),
    Global(GlobalAddr),
}

impl ExternVal {
    pub fn extern_type<'a, T: Config>(&self, store: &Store<'a, T>) -> ExternType {
        match self {
            ExternVal::Func(addr) => ExternType::Func(store.functions.get(*addr).ty()),
            ExternVal::Table(addr) => ExternType::Table(store.tables.get(*addr).ty),
            ExternVal::Mem(addr) => ExternType::Mem(store.memories.get(*addr).ty),
            ExternVal::Global(addr) => ExternType::Global(store.globals.get(*addr).ty),
        }
    }
    pub fn as_func(self) -> Option<FuncAddr> {
        if let ExternVal::Func(a) = self {
            Some(a)
        } else {
            None
        }
    }
}

pub trait ExternFilterable {
    fn funcs(self) -> impl Iterator<Item = FuncAddr>;
    fn globals(self) -> impl Iterator<Item = GlobalAddr>;
    fn tables(self) -> impl Iterator<Item = TableAddr>;
    fn mems(self) -> impl Iterator<Item = MemAddr>;
}

impl<'a, I> ExternFilterable for I
where
    I: Iterator<Item = &'a ExternVal>,
{
    fn funcs(self) -> impl Iterator<Item = FuncAddr> {
        self.filter_map(|v| v.as_func())
    }
    fn globals(self) -> impl Iterator<Item = GlobalAddr> {
        self.filter_map(|v| {
            if let ExternVal::Global(a) = v {
                Some(*a)
            } else {
                None
            }
        })
    }
    fn tables(self) -> impl Iterator<Item = TableAddr> {
        self.filter_map(|v| {
            if let ExternVal::Table(a) = v {
                Some(*a)
            } else {
                None
            }
        })
    }
    fn mems(self) -> impl Iterator<Item = MemAddr> {
        self.filter_map(|v| {
            if let ExternVal::Mem(a) = v {
                Some(*a)
            } else {
                None
            }
        })
    }
}

pub struct InstantiationOutcome {
    pub module_addr: ModuleAddr,
    pub maybe_remaining_fuel: Option<u32>,
}
