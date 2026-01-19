use super::config::Config;
use crate::rust_alloc::{
    collections::btree_map::{BTreeMap, Entry},
    string::String,
    vec::Vec,
};
use crate::wasm::{
    execution::store::addrs::ModuleAddr, execution::store::InstantiationOutcome, execution::store::Store, execution::store::StoreId, ExternVal, RuntimeError,
    ValidationInfo,
};
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
                    .ok_or(RuntimeError::UnableToResolveExternLookup)
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
