#[macro_export]
macro_rules! export_method {
    (
        $module:expr, $name:expr, 
        [$($compat:expr),*], 
        $params:expr, $returns:expr,
        $vis:vis fn $func_name:ident <$t:ident: Config>($store:tt: &mut Store<'_, $t_ignore:ident>, $args:tt: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> $body:block
    ) => {
        $vis fn $func_name<$t: Config>($store: &mut Store<'_, $t>, $args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> $body

        #[allow(non_snake_case)]
        pub mod $func_name {
            use super::*;
            pub fn register<$t: Config>(linker: &mut $crate::wasm::Linker, store: &mut $crate::wasm::Store<'_, $t>) {
                let func_type = $crate::wasm::common::reader::types::FuncType {
                    params: $crate::wasm::common::reader::types::ResultType { valtypes: $params },
                    returns: $crate::wasm::common::reader::types::ResultType { valtypes: $returns },
                };
                let func_addr = store.func_alloc_unchecked(func_type, super::$func_name);
                let _ = linker.define_unchecked(
                    $crate::alloc::string::String::from($module),
                    $crate::alloc::string::String::from($name),
                    $crate::wasm::interpreter::store::ExternVal::Func(func_addr),
                );
                $(
                    let (c_mod, c_name) = $compat;
                    let _ = linker.define_unchecked(
                        $crate::alloc::string::String::from(c_mod),
                        $crate::alloc::string::String::from(c_name),
                        $crate::wasm::interpreter::store::ExternVal::Func(func_addr),
                    );
                )*
            }
        }
    };
}
