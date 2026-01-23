use crate::rust_alloc::{string::String, vec::Vec};
use crate::wasm::{
    core::reader::types::{FuncType, MemType, TableType},
    execution::config::Config,
    execution::linker::Linker,
    execution::resumable::{ResumableRef, RunState},
    execution::store::addrs::{FuncAddr, GlobalAddr, MemAddr, ModuleAddr, TableAddr},
    execution::store::InstantiationOutcome, execution::store::Store, execution::store::StoreId, ExternVal, GlobalType, RuntimeError, ValidationInfo,
};
use core::num::NonZeroU32;
mod interop;
mod value;
pub use interop::*;
pub use value::*;
impl<'b, T: Config> Store<'b, T> {
    pub fn module_instantiate(
        &mut self,
        validation_info: &ValidationInfo<'b>,
        extern_vals: Vec<StoredExternVal>,
        maybe_fuel: Option<u32>,
    ) -> Result<StoredInstantiationOutcome, RuntimeError> {
        let extern_vals = extern_vals.into_iter()
            .map(|ev| ev.try_unwrap_into_bare(self.id))
            .collect::<Result<Vec<ExternVal>, RuntimeError>>()?;
        let outcome = self.module_instantiate_unchecked(validation_info, extern_vals, maybe_fuel)?;
        Ok(unsafe { StoredInstantiationOutcome::from_bare(outcome, self.id) })
    }
    pub fn instance_export(
        &self,
        module_addr: Stored<ModuleAddr>,
        name: &str,
    ) -> Result<StoredExternVal, RuntimeError> {
        let module_addr = module_addr.try_unwrap_into_bare(self.id)?;
        let extern_val = self.instance_export_unchecked(module_addr, name)?;
        Ok(unsafe { StoredExternVal::from_bare(extern_val, self.id) })
    }
    pub fn func_type(&self, func_addr: Stored<FuncAddr>) -> Result<FuncType, RuntimeError> {
        let func_addr = func_addr.try_unwrap_into_bare(self.id)?;
        Ok(self.func_type_unchecked(func_addr))
    }
    pub fn invoke(
        &mut self,
        func_addr: Stored<FuncAddr>,
        params: Vec<StoredValue>,
        maybe_fuel: Option<u32>,
    ) -> Result<StoredRunState, RuntimeError> {
        let func_addr = func_addr.try_unwrap_into_bare(self.id)?;
        let params = try_unwrap_values(params, self.id)?;
        let run_state = self.invoke_unchecked(func_addr, params, maybe_fuel)?;
        Ok(unsafe { StoredRunState::from_bare(run_state, self.id) })
    }
    pub fn table_alloc(&mut self, table_type: TableType, r#ref: StoredRef) -> Result<Stored<TableAddr>, RuntimeError> {
        let r#ref = r#ref.try_unwrap_into_bare(self.id)?;
        let addr = self.table_alloc_unchecked(table_type, r#ref)?;
        Ok(unsafe { Stored::from_bare(addr, self.id) })
    }
    pub fn table_type(&self, table_addr: Stored<TableAddr>) -> Result<TableType, RuntimeError> {
        let table_addr = table_addr.try_unwrap_into_bare(self.id)?;
        Ok(self.table_type_unchecked(table_addr))
    }
    pub fn table_read(&self, table_addr: Stored<TableAddr>, i: u32) -> Result<StoredRef, RuntimeError> {
        let table_addr = table_addr.try_unwrap_into_bare(self.id)?;
        let r#ref = self.table_read_unchecked(table_addr, i)?;
        Ok(unsafe { StoredRef::from_bare(r#ref, self.id) })
    }
    pub fn table_write(&mut self, table_addr: Stored<TableAddr>, i: u32, r#ref: StoredRef) -> Result<(), RuntimeError> {
        let table_addr = table_addr.try_unwrap_into_bare(self.id)?;
        let r#ref = r#ref.try_unwrap_into_bare(self.id)?;
        self.table_write_unchecked(table_addr, i, r#ref)
    }
    pub fn table_size(&self, table_addr: Stored<TableAddr>) -> Result<u32, RuntimeError> {
        let table_addr = table_addr.try_unwrap_into_bare(self.id)?;
        Ok(self.table_size_unchecked(table_addr))
    }
    pub fn mem_alloc(&mut self, mem_type: MemType) -> Stored<MemAddr> {
        let addr = self.mem_alloc_unchecked(mem_type);
        unsafe { Stored::from_bare(addr, self.id) }
    }
    pub fn mem_type(&self, mem_addr: Stored<MemAddr>) -> Result<MemType, RuntimeError> {
        let mem_addr = mem_addr.try_unwrap_into_bare(self.id)?;
        Ok(self.mem_type_unchecked(mem_addr))
    }
    pub fn mem_read(&self, mem_addr: Stored<MemAddr>, i: u32) -> Result<u8, RuntimeError> {
        let mem_addr = mem_addr.try_unwrap_into_bare(self.id)?;
        self.mem_read_unchecked(mem_addr, i)
    }
    pub fn mem_write(&mut self, mem_addr: Stored<MemAddr>, i: u32, byte: u8) -> Result<(), RuntimeError> {
        let mem_addr = mem_addr.try_unwrap_into_bare(self.id)?;
        self.mem_write_unchecked(mem_addr, i, byte)
    }
    pub fn mem_size(&self, mem_addr: Stored<MemAddr>) -> Result<u32, RuntimeError> {
        let mem_addr = mem_addr.try_unwrap_into_bare(self.id)?;
        Ok(self.mem_size_unchecked(mem_addr))
    }
    pub fn mem_grow(&mut self, mem_addr: Stored<MemAddr>, n: u32) -> Result<(), RuntimeError> {
        let mem_addr = mem_addr.try_unwrap_into_bare(self.id)?;
        self.mem_grow_unchecked(mem_addr, n)
    }
    pub fn global_alloc(&mut self, global_type: GlobalType, val: StoredValue) -> Result<Stored<GlobalAddr>, RuntimeError> {
        let val = val.try_unwrap_into_bare(self.id)?;
        let addr = self.global_alloc_unchecked(global_type, val)?;
        Ok(unsafe { Stored::from_bare(addr, self.id) })
    }
    pub fn global_type(&self, global_addr: Stored<GlobalAddr>) -> Result<GlobalType, RuntimeError> {
        let global_addr = global_addr.try_unwrap_into_bare(self.id)?;
        Ok(self.global_type_unchecked(global_addr))
    }
    pub fn global_read(&self, global_addr: Stored<GlobalAddr>) -> Result<StoredValue, RuntimeError> {
        let global_addr = global_addr.try_unwrap_into_bare(self.id)?;
        let val = self.global_read_unchecked(global_addr);
        Ok(unsafe { StoredValue::from_bare(val, self.id) })
    }
    pub fn global_write(&mut self, global_addr: Stored<GlobalAddr>, val: StoredValue) -> Result<(), RuntimeError> {
        let global_addr = global_addr.try_unwrap_into_bare(self.id)?;
        let val = val.try_unwrap_into_bare(self.id)?;
        self.global_write_unchecked(global_addr, val)
    }
    pub fn create_resumable(&self, func_addr: Stored<FuncAddr>, params: Vec<StoredValue>, maybe_fuel: Option<u32>) -> Result<Stored<ResumableRef>, RuntimeError> {
        let func_addr = func_addr.try_unwrap_into_bare(self.id)?;
        let params = try_unwrap_values(params, self.id)?;
        let ref_ = self.create_resumable_unchecked(func_addr, params, maybe_fuel)?;
        Ok(unsafe { Stored::from_bare(ref_, self.id) })
    }
    pub fn resume(&mut self, resumable_ref: Stored<ResumableRef>) -> Result<StoredRunState, RuntimeError> {
        let ref_ = resumable_ref.try_unwrap_into_bare(self.id)?;
        let state = self.resume_unchecked(ref_)?;
        Ok(unsafe { StoredRunState::from_bare(state, self.id) })
    }
    pub fn access_fuel_mut<R>(&mut self, resumable_ref: &mut Stored<ResumableRef>, f: impl FnOnce(&mut Option<u32>) -> R) -> Result<R, RuntimeError> {
        let ref_ = resumable_ref.as_mut().try_unwrap_into_bare(self.id)?;
        self.access_fuel_mut_unchecked(ref_, f)
    }
    pub fn invoke_without_fuel(&mut self, func_addr: Stored<FuncAddr>, params: Vec<StoredValue>) -> Result<Vec<StoredValue>, RuntimeError> {
        let func_addr = func_addr.try_unwrap_into_bare(self.id)?;
        let params = try_unwrap_values(params, self.id)?;
        let returns = self.invoke_without_fuel_unchecked(func_addr, params)?;
        Ok(unsafe { wrap_vec_elements(returns, self.id) })
    }
    
    pub fn compile_module_aot(&mut self, module_addr: Stored<ModuleAddr>) -> Result<(), RuntimeError> {
        let module_addr = module_addr.try_unwrap_into_bare(self.id)?;
        self.compile_module_aot_unchecked(module_addr);
        Ok(())
    }
}
impl Linker {
    pub fn define(&mut self, module_name: String, name: String, val: StoredExternVal) -> Result<(), RuntimeError> {
        let id = val.id().expect("StoreId missing");
        let lid = *self.store_id.get_or_insert(id);
        if lid != id { return Err(RuntimeError::StoreIdMismatch); }
        self.define_unchecked(module_name, name, val.try_unwrap_into_bare(lid)?)
    }
    pub fn module_instantiate<'a, T: Config>(&mut self, store: &mut Store<'a, T>, info: &ValidationInfo<'a>, fuel: Option<u32>) -> Result<StoredInstantiationOutcome, RuntimeError> {
        let lid = *self.store_id.get_or_insert(store.id);
        if lid != store.id { return Err(RuntimeError::StoreIdMismatch); }
        let outcome = self.module_instantiate_unchecked(store, info, fuel)?;
        Ok(unsafe { StoredInstantiationOutcome::from_bare(outcome, lid) })
    }
}
pub trait AbstractStored: Sized {
    type BareTy: Sized;
    unsafe fn from_bare(bare: Self::BareTy, id: StoreId) -> Self;
    fn id(&self) -> Option<StoreId>;
    fn into_bare(self) -> Self::BareTy;
    fn try_unwrap_into_bare(self, expected: StoreId) -> Result<Self::BareTy, RuntimeError> {
        if let Some(id) = self.id() { if id != expected { return Err(RuntimeError::StoreIdMismatch); } }
        Ok(self.into_bare())
    }
}
pub struct Stored<T> {
    inner: T,
    id: StoreId,
}
impl<T> AbstractStored for Stored<T> {
    type BareTy = T;
    unsafe fn from_bare(inner: T, id: StoreId) -> Self { Self { inner, id } }
    fn id(&self) -> Option<StoreId> { Some(self.id) }
    fn into_bare(self) -> Self::BareTy { self.inner }
    fn try_unwrap_into_bare(self, expected: StoreId) -> Result<T, RuntimeError> {
        if self.id != expected { return Err(RuntimeError::StoreIdMismatch); }
        Ok(self.inner)
    }
}
impl<T: Clone> Clone for Stored<T> { fn clone(&self) -> Self { Self { inner: self.inner.clone(), id: self.id } } }
impl<T: Copy> Copy for Stored<T> {}
impl<T: core::fmt::Debug> core::fmt::Debug for Stored<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Stored").field("inner", &self.inner).field("id", &self.id).finish()
    }
}
impl<T: PartialEq> PartialEq for Stored<T> { fn eq(&self, other: &Self) -> bool { self.id == other.id && self.inner == other.inner } }
impl<T: Eq> Eq for Stored<T> {}
impl<T> Stored<T> { fn as_mut(&mut self) -> Stored<&mut T> { Stored { id: self.id, inner: &mut self.inner } } }
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum StoredExternVal { Func(Stored<FuncAddr>), Table(Stored<TableAddr>), Mem(Stored<MemAddr>), Global(Stored<GlobalAddr>) }
impl AbstractStored for StoredExternVal {
    type BareTy = ExternVal;
    unsafe fn from_bare(bare: ExternVal, id: StoreId) -> Self {
        match bare {
            ExternVal::Func(a) => Self::Func(Stored::from_bare(a, id)),
            ExternVal::Table(a) => Self::Table(Stored::from_bare(a, id)),
            ExternVal::Mem(a) => Self::Mem(Stored::from_bare(a, id)),
            ExternVal::Global(a) => Self::Global(Stored::from_bare(a, id)),
        }
    }
    fn id(&self) -> Option<StoreId> {
        match self {
            Self::Func(s) => s.id(),
            Self::Table(s) => s.id(),
            Self::Mem(s) => s.id(),
            Self::Global(s) => s.id()
        }
    }
    fn into_bare(self) -> ExternVal {
        match self {
            Self::Func(s) => ExternVal::Func(s.into_bare()),
            Self::Table(s) => ExternVal::Table(s.into_bare()),
            Self::Mem(s) => ExternVal::Mem(s.into_bare()),
            Self::Global(s) => ExternVal::Global(s.into_bare())
        }
    }
}
impl StoredExternVal {
    pub fn as_func(self) -> Option<Stored<FuncAddr>> {
        if let StoredExternVal::Func(f) = self { Some(f) } else { None }
    }
}
#[derive(Debug)]
pub enum StoredRunState {
    Finished { values: Vec<StoredValue>, maybe_remaining_fuel: Option<u32> },
    Resumable { resumable_ref: Stored<ResumableRef>, required_fuel: NonZeroU32 },
}
impl AbstractStored for StoredRunState {
    type BareTy = RunState;
    unsafe fn from_bare(bare: RunState, id: StoreId) -> Self {
        match bare {
            RunState::Finished { values, maybe_remaining_fuel } => Self::Finished { values: wrap_vec_elements(values, id), maybe_remaining_fuel },
            RunState::Resumable { resumable_ref, required_fuel } => Self::Resumable { resumable_ref: Stored::from_bare(resumable_ref, id), required_fuel },
        }
    }
    fn id(&self) -> Option<StoreId> {
        match self {
            Self::Finished { values, .. } => values.first().and_then(|v| v.id()),
            Self::Resumable { resumable_ref, .. } => resumable_ref.id()
        }
    }
    fn into_bare(self) -> RunState {
        match self {
            Self::Finished { values, maybe_remaining_fuel } => RunState::Finished { values: values.into_iter().map(|v| v.into_bare()).collect(), maybe_remaining_fuel },
            Self::Resumable { resumable_ref, required_fuel } => RunState::Resumable { resumable_ref: resumable_ref.into_bare(), required_fuel },
        }
    }
}
pub struct StoredInstantiationOutcome {
    pub module_addr: Stored<ModuleAddr>,
    pub maybe_remaining_fuel: Option<u32>,
}
impl AbstractStored for StoredInstantiationOutcome {
    type BareTy = InstantiationOutcome;
    unsafe fn from_bare(bare: InstantiationOutcome, id: StoreId) -> Self { Self { module_addr: Stored::from_bare(bare.module_addr, id), maybe_remaining_fuel: bare.maybe_remaining_fuel } }
    fn id(&self) -> Option<StoreId> { self.module_addr.id() }
    fn into_bare(self) -> InstantiationOutcome { InstantiationOutcome { module_addr: self.module_addr.into_bare(), maybe_remaining_fuel: self.maybe_remaining_fuel } }
}
unsafe fn wrap_vec_elements<S: AbstractStored>(values: Vec<S::BareTy>, id: StoreId) -> Vec<S> {
    values.into_iter().map(|v| unsafe { S::from_bare(v, id) }).collect()
}
fn try_unwrap_values<S: AbstractStored>(stored: Vec<S>, id: StoreId) -> Result<Vec<S::BareTy>, RuntimeError> {
    stored.into_iter().map(|s| s.try_unwrap_into_bare(id)).collect()
}