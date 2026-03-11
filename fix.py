import sys

content = open("std/src/wasm/wasi/preview2/mod.rs").read()
if "debug-print" not in content:
    content = content.replace(
        'crate::process::wasi::native_file_open);',
        'crate::process::wasi::native_file_open);\n        define(linker, store, module, "debug-print",\n            vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)],\n            vec![],\n            crate::process::wasi::debug_print);'
    )
    open("std/src/wasm/wasi/preview2/mod.rs", "w").write(content)
