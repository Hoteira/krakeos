use super::value::ValueTypeMismatchError;
use crate::alloc::{fmt::Debug, vec};
use crate::wasm::common::reader::types::{NumType, RefType, ValType};
use crate::wasm::common::value::{ExternAddr, Ref, Value, F32, F64};

use crate::wasm::common::config::Config;
use crate::alloc::{
    collections::btree_map::{BTreeMap, Entry},
    string::String,
    vec::Vec,
};
use crate::wasm::common::runtime_error::RuntimeError;
use crate::wasm::common::validation::ValidationInfo;
use crate::wasm::interpreter::store::{Store, StoreId, InstantiationOutcome, ExternVal};
use crate::wasm::interpreter::store::addrs::ModuleAddr;

#[derive(Clone, Default)]
pub struct Linker {
    extern_vals: BTreeMap<ImportKey, ExternVal>,
    pub(crate) store_id: Option<StoreId>,
}

impl Linker {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn define_unchecked(
        &mut self,
        module_name: String,
        name: String,
        extern_val: ExternVal,
    ) -> Result<(), RuntimeError> {
        match self.extern_vals.entry(ImportKey { module_name, name }) {
            Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(extern_val);
                Ok(())
            }
            Entry::Occupied(_occupied_entry) => Err(RuntimeError::DuplicateExternDefinition),
        }
    }
    pub fn define_module_instance_unchecked<T: Config>(
        &mut self,
        store: &Store<T>,
        module_name: String,
        module: ModuleAddr,
    ) -> Result<(), RuntimeError> {
        let module = store.modules.get(module);
        for export in &module.exports {
            self.define_unchecked(module_name.clone(), export.0.clone(), *export.1)?;
        }
        Ok(())
    }
    pub fn get_unchecked(&self, module_name: String, name: String) -> Option<ExternVal> {
        if let Some(val) = self.extern_vals.get(&ImportKey { module_name: module_name.clone(), name: name.clone() }) {
            return Some(*val);
        }
        let cleanse = |s: &str| -> String {
            let s = s.trim_start_matches(|c: char| !c.is_ascii() || c.is_control());
            s.split('@').next().unwrap_or(s).into()
        };
        let clean_module = cleanse(&module_name);
        for (key, val) in &self.extern_vals {
            if key.name == name && cleanse(&key.module_name) == clean_module {
                return Some(*val);
            }
        }
        None
    }
    pub fn get_module_exports(&self, module_name: &str) -> Option<Vec<(String, ExternVal)>> {
        let mut exports = Vec::new();
        fn cleanse(s: &str) -> &str {
            let s = s.trim_start_matches(|c: char| !c.is_ascii() || c.is_control());
            s.split('@').next().unwrap_or(s)
        }
        let requested_base = cleanse(module_name);
        if requested_base.is_empty() { return None; }
        for (key, val) in &self.extern_vals {
            if key.module_name == module_name {
                exports.push((key.name.clone(), *val));
            }
        }
        if !exports.is_empty() { return Some(exports); }
        for (key, val) in &self.extern_vals {
            if cleanse(&key.module_name) == requested_base {
                exports.push((key.name.clone(), *val));
            }
        }
        if exports.is_empty() { None } else { Some(exports) }
    }
    pub fn instantiate_pre_unchecked(
        &self,
        validation_info: &ValidationInfo,
    ) -> Result<Vec<ExternVal>, RuntimeError> {
        validation_info
            .imports
            .iter()
            .map(|import| {
                self.get_unchecked(import.module_name.clone(), import.name.clone())
                    .ok_or_else(|| RuntimeError::UnableToResolveExternLookup {
                        module: import.module_name.clone(),
                        name: import.name.clone(),
                    })
            })
            .collect()
    }
    pub fn module_instantiate_unchecked<'b, T: Config>(
        &self,
        store: &mut Store<'b, T>,
        validation_info: &ValidationInfo<'b>,
        maybe_fuel: Option<u32>,
    ) -> Result<InstantiationOutcome, RuntimeError> {
        store.module_instantiate_unchecked(
            validation_info,
            self.instantiate_pre_unchecked(validation_info)?,
            maybe_fuel,
        )
    }
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
struct ImportKey {
    module_name: String,
    name: String,
}

pub trait InteropValue
where
    Self: Copy + Debug + PartialEq + TryFrom<Value, Error=ValueTypeMismatchError>,
    Value: From<Self>,
{
    const TY: ValType;
}

impl InteropValue for u32 {
    const TY: ValType = ValType::NumType(NumType::I32);
}
impl InteropValue for i32 {
    const TY: ValType = ValType::NumType(NumType::I32);
}
impl InteropValue for u64 {
    const TY: ValType = ValType::NumType(NumType::I64);
}
impl InteropValue for i64 {
    const TY: ValType = ValType::NumType(NumType::I64);
}
impl InteropValue for f32 {
    const TY: ValType = ValType::NumType(NumType::F32);
}
impl InteropValue for f64 {
    const TY: ValType = ValType::NumType(NumType::F64);
}
impl InteropValue for [u8; 16] {
    const TY: ValType = ValType::VecType;
}
impl InteropValue for RefFunc {
    const TY: ValType = ValType::RefType(RefType::FuncRef);
}
impl InteropValue for RefExtern {
    const TY: ValType = ValType::RefType(RefType::ExternRef);
}

impl From<f32> for Value {
    fn from(value: f32) -> Self {
        F32(value).into()
    }
}
impl TryFrom<Value> for f32 {
    type Error = ValueTypeMismatchError;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        F32::try_from(value).map(|f| f.0)
    }
}
impl From<f64> for Value {
    fn from(value: f64) -> Self {
        F64(value).into()
    }
}
impl TryFrom<Value> for f64 {
    type Error = ValueTypeMismatchError;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        F64::try_from(value).map(|f| f.0)
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct RefFunc(pub Option<usize>);

impl From<RefFunc> for Value {
    fn from(value: RefFunc) -> Self {
        match value.0 {
            Some(func_addr) => Ref::Func(func_addr),
            None => Ref::Null(RefType::FuncRef),
        }
            .into()
    }
}

impl TryFrom<Value> for RefFunc {
    type Error = ValueTypeMismatchError;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match Ref::try_from(value)? {
            Ref::Func(func_addr) => Ok(Self(Some(func_addr))),
            Ref::Null(RefType::FuncRef) => Ok(Self(None)),
            _ => Err(ValueTypeMismatchError { expected: "RefFunc", actual: value }),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct RefExtern(pub Option<ExternAddr>);

impl From<RefExtern> for Value {
    fn from(value: RefExtern) -> Self {
        match value.0 {
            Some(extern_addr) => Ref::Extern(extern_addr),
            None => Ref::Null(RefType::ExternRef),
        }
            .into()
    }
}

impl TryFrom<Value> for RefExtern {
    type Error = ValueTypeMismatchError;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match Ref::try_from(value)? {
            Ref::Extern(extern_addr) => Ok(Self(Some(extern_addr))),
            Ref::Null(RefType::ExternRef) => Ok(Self(None)),
            _ => Err(ValueTypeMismatchError { expected: "RefExtern", actual: value }),
        }
    }
}

pub trait InteropValueList: Debug + Copy {
    const TYS: &'static [ValType];
    fn into_values(self) -> crate::alloc::vec::Vec<Value>;
    fn try_from_values(
        values: impl ExactSizeIterator<Item=Value>,
    ) -> Result<Self, ValueTypeMismatchError>;
}

impl InteropValueList for () {
    const TYS: &'static [ValType] = &[];
    fn into_values(self) -> crate::alloc::vec::Vec<Value> {
        crate::alloc::vec::Vec::new()
    }
    fn try_from_values(
        values: impl ExactSizeIterator<Item=Value>,
    ) -> Result<Self, ValueTypeMismatchError> {
        if values.len() != 0 {
            return Err(ValueTypeMismatchError { expected: "()", actual: values.last().unwrap_or(Value::I32(0)) });
        }
        Ok(())
    }
}

impl<A> InteropValueList for A
where
    A: InteropValue,
    Value: From<A>,
{
    const TYS: &'static [ValType] = &[A::TY];
    fn into_values(self) -> crate::alloc::vec::Vec<Value> {
        vec![self.into()]
    }
    fn try_from_values(
        mut values: impl ExactSizeIterator<Item=Value>,
    ) -> Result<Self, ValueTypeMismatchError> {
        if values.len() != Self::TYS.len() {
            return Err(ValueTypeMismatchError { expected: "1 value", actual: values.last().unwrap_or(Value::I32(0)) });
        }
        A::try_from(values.next().unwrap())
    }
}

impl<A> InteropValueList for (A,)
where
    A: InteropValue,
    Value: From<A>,
{
    const TYS: &'static [ValType] = &[A::TY];
    fn into_values(self) -> crate::alloc::vec::Vec<Value> {
        vec![self.0.into()]
    }
    fn try_from_values(
        mut values: impl ExactSizeIterator<Item=Value>,
    ) -> Result<Self, ValueTypeMismatchError> {
        if values.len() != Self::TYS.len() {
            return Err(ValueTypeMismatchError { expected: "(A,)", actual: values.last().unwrap_or(Value::I32(0)) });
        }
        Ok((A::try_from(values.next().unwrap())?,))
    }
}

impl<A, B> InteropValueList for (A, B)
where
    A: InteropValue,
    B: InteropValue,
    Value: From<A> + From<B>,
{
    const TYS: &'static [ValType] = &[A::TY, B::TY];
    fn into_values(self) -> crate::alloc::vec::Vec<Value> {
        vec![self.0.into(), self.1.into()]
    }
    fn try_from_values(
        mut values: impl ExactSizeIterator<Item=Value>,
    ) -> Result<Self, ValueTypeMismatchError> {
        if values.len() != Self::TYS.len() {
            return Err(ValueTypeMismatchError { expected: "(A, B)", actual: values.last().unwrap_or(Value::I32(0)) });
        }
        Ok((
            A::try_from(values.next().unwrap())?,
            B::try_from(values.next().unwrap())?,
        ))
    }
}

impl<A, B, C> InteropValueList for (A, B, C)
where
    A: InteropValue,
    B: InteropValue,
    C: InteropValue,
    Value: From<A> + From<B> + From<C>,
{
    const TYS: &'static [ValType] = &[A::TY, B::TY, C::TY];
    fn into_values(self) -> crate::alloc::vec::Vec<Value> {
        vec![self.0.into(), self.1.into(), self.2.into()]
    }
    fn try_from_values(
        mut values: impl ExactSizeIterator<Item=Value>,
    ) -> Result<Self, ValueTypeMismatchError> {
        if values.len() != Self::TYS.len() {
            return Err(ValueTypeMismatchError { expected: "(A, B, C)", actual: values.last().unwrap_or(Value::I32(0)) });
        }
        Ok((
            A::try_from(values.next().unwrap())?,
            B::try_from(values.next().unwrap())?,
            C::try_from(values.next().unwrap())?,
        ))
    }
}
