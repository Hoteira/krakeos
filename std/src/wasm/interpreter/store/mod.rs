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
pub use instances::{
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
    pub code_base: Option<u64>,
    pub stack_base: u64,
    pub stack_limit: u64,
    pub next_code_offset: usize,
    pub container_id: Option<u64>,
    pub shm_mappings: BTreeMap<String, u32>,
    /// Raw pointer to the Ring3Context during aot_call_host processing.
    /// Set before calling host functions so that invoke_unchecked can
    /// call back into AOT-compiled WASM functions instead of the interpreter.
    pub ring3_ctx: Option<*mut crate::wasm::aot::runtime::Ring3Context>,
    /// The user-space WASM stack pointer at the time of the host call.
    #[allow(dead_code)]
    pub ring3_sp: Option<*mut u128>,
}

unsafe impl<'a, T: Config + Send> Send for Store<'a, T> {}
unsafe impl<'a, T: Config + Sync> Sync for Store<'a, T> {}

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
            code_base: None,
            stack_base: 0,
            stack_limit: 0,
            next_code_offset: 0,
            container_id: None,
            shm_mappings: BTreeMap::new(),
            ring3_ctx: None,
            ring3_sp: None,
        }
    }

    pub fn module_instantiate_unchecked(
        &mut self,
        validation_info: &ValidationInfo<'a>,
        extern_vals: Vec<ExternVal>,
        maybe_fuel: Option<u32>,
        slot_id: u16,
    ) -> Result<InstantiationOutcome, RuntimeError> {
        if validation_info.imports.len() != extern_vals.len() {
            return Err(RuntimeError::ExternValsLenMismatch);
        }
        let mut maybe_ctx_ptr = None;
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
            .zip(global_init_vals.clone())
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

            let final_code_ptr = if let Some(base) = self.code_base {
                // 1. Copy blob if this is the first module in this code slot
                if self.next_code_offset == 0 {
                    crate::debugln!("[AOT] Copying blob to {:#x}...", base);
                    let blob = crate::wasm::aot::RING3_RT_BLOB;
                    unsafe {
                        core::ptr::copy_nonoverlapping(blob.as_ptr(), base as *mut u8, blob.len());
                        crate::debugln!("[AOT] Blob copied. Fixing jump table...");
                        // Fix up jump table (1024 entries)
                        let jump_table = base as *mut u64;
                        for i in 0..1024 {
                            *jump_table.add(i) += base;
                        }
                        crate::debugln!("[AOT] Jump table fixed.");
                    }
                    self.next_code_offset = (blob.len() + 4095) & !4095;
                }

                let ptr = (base + self.next_code_offset as u64) as *mut u8;
                let code_len = aot_module.code.as_slice().len();
                crate::debugln!(
                    "[AOT] code_base={:#x} offset={} code_len={} ({} KB)",
                    base,
                    self.next_code_offset,
                    code_len,
                    code_len / 1024
                );
                const CODE_SLOT_SIZE: usize = 64 * 1024 * 1024; // 64 MiB as per plan
                if self.next_code_offset + code_len > CODE_SLOT_SIZE {
                    return Err(RuntimeError::Trap(TrapError::ReachedUnreachable));
                }

                unsafe {
                    core::ptr::copy_nonoverlapping(aot_module.code.as_ptr(), ptr, code_len);
                }
                crate::debugln!("[AOT] Module code copied.");

                let res = ptr as usize;
                // Align next offset to 4KB for isolation
                self.next_code_offset = (self.next_code_offset + code_len + 4095) & !4095;

                // 2. Set up Data Region after the code
                let data_region_ptr = (base + self.next_code_offset as u64) as *mut u8;
                crate::debugln!("[AOT] Data region at {:#x}", data_region_ptr as u64);
                
                // Ring3Context at start of data region
                let ctx_ptr = data_region_ptr as *mut crate::wasm::aot::runtime::Ring3Context;
                
                // Collect all global values (imported + local)
                let mut all_global_vals = Vec::new();
                for ga in extern_vals.iter().globals() {
                    all_global_vals.push(self.globals.get(ga).value);
                }
                for v in &global_init_vals {
                    all_global_vals.push(*v);
                }

                // Allocate space for globals
                let num_globals = all_global_vals.len();
                let globals_ptr = unsafe { data_region_ptr.add(256) }; // 256 bytes reserved for context
                
                // Allocate space for table 0
                let table0_addr = *self.modules.get(module_addr).table_addrs.first().unwrap_or(&usize::MAX);
                let (table0_ptr, table0_size) = if table0_addr != usize::MAX {
                    let t = self.tables.get(table0_addr);
                    (unsafe { globals_ptr.add((num_globals * 16 + 15) & !15) as *mut u64 }, t.elem.len() as u32)
                } else {
                    (core::ptr::null_mut(), 0)
                };

                // Resolve imported functions to blob stubs
                let mut import_stubs = Vec::new();
                for imp in &validation_info.imports {
                    if let crate::wasm::common::reader::types::import::ImportDesc::Func(_) = imp.desc {
                        let stub_idx = match (imp.module_name.as_str(), imp.name.as_str()) {
                            // WASI Preview 1 (300+)
                            ("wasi_snapshot_preview1", "fd_write") => 300,
                            ("wasi_snapshot_preview1", "fd_read") => 301,
                            ("wasi_snapshot_preview1", "fd_close") => 302,
                            ("wasi_snapshot_preview1", "proc_exit") => 303,
                            ("wasi_snapshot_preview1", "args_sizes_get") => 304,
                            ("wasi_snapshot_preview1", "args_get") => 305,
                            ("wasi_snapshot_preview1", "environ_sizes_get") => 306,
                            ("wasi_snapshot_preview1", "environ_get") => 307,
                            ("wasi_snapshot_preview1", "clock_time_get") => 308,
                            ("wasi_snapshot_preview1", "random_get") => 309,
                            ("wasi_snapshot_preview1", "fd_prestat_get") => 310,
                            ("wasi_snapshot_preview1", "fd_prestat_dir_name") => 311,
                            ("wasi_snapshot_preview1", "fd_fdstat_get") => 312,
                            ("wasi_snapshot_preview1", "fd_filestat_get") => 313,
                            ("wasi_snapshot_preview1", "fd_filestat_set_size") => 314,
                            ("wasi_snapshot_preview1", "fd_seek") => 315,
                            ("wasi_snapshot_preview1", "fd_pread") => 316,
                            ("wasi_snapshot_preview1", "fd_readdir") => 317,
                            ("wasi_snapshot_preview1", "path_open") => 318,
                            ("wasi_snapshot_preview1", "path_filestat_get") => 319,
                            ("wasi_snapshot_preview1", "path_create_directory") => 320,
                            ("wasi_snapshot_preview1", "path_unlink_file") => 321,
                            ("wasi_snapshot_preview1", "path_remove_directory") => 322,
                            ("wasi_snapshot_preview1", "path_rename") => 323,
                            ("wasi_snapshot_preview1", "path_link") => 324,
                            ("wasi_snapshot_preview1", "path_symlink") => 325,
                            ("wasi_snapshot_preview1", "path_readlink") => 326,
                            ("wasi_snapshot_preview1", "poll_oneoff") => 327,
                            ("wasi_snapshot_preview1", "sched_yield") => 328,
                            ("wasi_snapshot_preview1", "clock_res_get") => 329,

                            // KrakeOS Graphics (400+)
                            ("krakeos:graphics/screen@0.2.0", "get-width") => 400,
                            ("krakeos:graphics/screen@0.2.0", "get-height") => 401,

                            // KrakeOS Window (410+)
                            ("krakeos:system/window@0.2.0", "create") => 410,
                            ("krakeos:system/window@0.2.0", "update") => 411,
                            ("krakeos:system/window@0.2.0", "update-area") => 412,
                            ("krakeos:system/window@0.2.0", "get-events") => 413,
                            ("krakeos:system/window@0.2.0", "register-event-queue") => 414,
                            ("krakeos:system/window@0.2.0", "deregister-event-queue") => 415,

                            // KrakeOS Process (420+)
                            ("krakeos:system/process@0.2.0", "get-pid") => 420,
                            ("krakeos:system/process@0.2.0", "debug-print") => 421,
                            ("krakeos:system/process@0.2.0", "yield") => 422,
                            ("krakeos:system/process@0.2.0", "spawn") => 423,
                            ("krakeos:system/process@0.2.0", "waitpid") => 424,
                            ("krakeos:system/process@0.2.0", "pipe") => 425,
                            ("krakeos:system/process@0.2.0", "native-file-open") => 426,
                            ("krakeos:system/process@0.2.0", "native-file-stat") => 427,
                            ("krakeos:system/process@0.2.0", "file-read") => 428,
                            ("krakeos:system/process@0.2.0", "file-write") => 429,
                            ("krakeos:system/process@0.2.0", "kill") => 430,
                            ("krakeos:system/process@0.2.0", "get-list") => 431,
                            ("krakeos:system/process@0.2.0", "chdir") => 432,
                            ("krakeos:system/process@0.2.0", "get-slot-info") => 433,
                            ("krakeos:system/process@0.2.0", "ioctl") => 434,
                            ("krakeos:system/process@0.2.0", "set-nonblock") => 435,
                            ("krakeos:system/process@0.2.0", "poll") => 436,
                            ("krakeos:system/process@0.2.0", "get-current-user") => 437,
                            ("krakeos:system/process@0.2.0", "spawn-ext") => 438,
                            ("krakeos:system/process@0.2.0", "spawn-thread") => 439,
                            ("krakeos:system/process@0.2.0", "thread-exit") => 440,
                            ("krakeos:system/process@0.2.0", "syscall") => 441,

                            // KrakeOS Terminal (463+)
                            ("krakeos:system/terminal@0.1.0", "set-window-size") => 463,
                            ("krakeos:system/terminal@0.1.0", "get-window-size") => 464,

                            // KrakeOS Container (470+)
                            ("krakeos:system/container@0.1.0", "plant") => 470,
                            ("krakeos:system/container@0.1.0", "plant-from-path") => 471,
                            ("krakeos:system/container@0.1.0", "harvest") => 472,
                            ("krakeos:system/container@0.1.0", "list-children") => 473,
                            ("krakeos:system/container@0.1.0", "kill-child") => 474,

                            // KrakeOS Debug (480+)
                            ("krakeos:system/debug@0.1.0", "get-process-list") => 480,
                            ("krakeos:system/debug@0.1.0", "kill") => 481,
                            ("krakeos:system/debug@0.1.0", "dump-vma") => 482,
                            ("krakeos:system/debug@0.1.0", "get-memory-usage") => 483,

                            // KrakeOS Memory (450+)
                            ("krakeos:system/memory@0.2.0", "shm-get") => 450,
                            ("krakeos:system/memory@0.2.0", "brk") => 451,
                            ("krakeos:system/memory@0.2.0", "get-total-mem") => 452,
                            ("krakeos:system/memory@0.2.0", "get-used-mem") => 453,
                            ("krakeos:system/memory@0.2.0", "get-vma-dump") => 454,

                            // WASI Preview 2 (500+)
                            ("wasi:cli/exit@0.2.0", "exit") => 500,
                            ("wasi:cli/stdout@0.2.0", "get-stdout") => 501,
                            ("wasi:cli/stdin@0.2.0", "get-stdin") => 502,
                            ("wasi:cli/stderr@0.2.0", "get-stderr") => 503,
                            ("wasi:io/streams@0.2.0", "[method]output-stream.write") => 504,
                            ("wasi:io/streams@0.2.0", "[method]output-stream.blocking-write") => 504,
                            ("wasi:io/streams@0.2.0", "[method]output-stream.blocking-write-and-flush") => 504,
                            ("wasi:io/streams@0.2.0", "[method]input-stream.read") => 505,
                            ("wasi:io/streams@0.2.0", "[method]input-stream.blocking-read") => 505,
                            ("wasi:io/poll@0.2.0", "poll") => 506,
                            ("wasi:io/poll@0.2.0", "[method]pollable.block") => 507,
                            ("wasi:io/poll@0.2.0", "[resource-drop]pollable") => 508,
                            ("wasi:io/error@0.2.0", "[resource-drop]error") => 509,
                            ("wasi:clocks/monotonic-clock@0.2.0", "now") => 510,
                            ("wasi:clocks/monotonic-clock@0.2.0", "resolution") => 511,
                            ("wasi:clocks/monotonic-clock@0.2.0", "subscribe-duration") => 512,
                            ("wasi:clocks/monotonic-clock@0.2.0", "subscribe-instant") => 512,
                            ("wasi:clocks/wall-clock@0.2.0", "now") => 513,
                            ("wasi:filesystem/types@0.2.0", "[resource-drop]descriptor") => 514,
                            ("wasi:filesystem/types@0.2.0", "[method]descriptor.open-at") => 515,
                            ("wasi:filesystem/types@0.2.0", "[method]descriptor.stat") => 516,
                            ("wasi:filesystem/types@0.2.0", "[method]descriptor.set-size") => 517,
                            ("wasi:filesystem/types@0.2.0", "[method]descriptor.seek") => 518,
                            ("wasi:filesystem/types@0.2.0", "[method]descriptor.create-directory-at") => 519,
                            ("wasi:filesystem/types@0.2.0", "[method]descriptor.unlink-file-at") => 520,
                            ("wasi:filesystem/types@0.2.0", "[method]descriptor.remove-directory-at") => 521,
                            ("wasi:filesystem/types@0.2.0", "[method]descriptor.rename-at") => 522,
                            ("wasi:filesystem/types@0.2.0", "[method]descriptor.read-directory") => 523,
                            ("wasi:filesystem/types@0.2.0", "[resource-drop]directory-entry-stream") => 524,
                            ("wasi:random/random@0.2.0", "get-random-bytes") => 525,
                            ("wasi:sockets/instance-network@0.2.0", "instance-network") => 526,

                            // Internal / Special
                            ("env", "__wasi_init_tp") => 460,
                            ("env", "__wasm_call_dtors") => 460,
                            ("env", "host_serial_print") => 999,
                            _ => u64::MAX, // Unknown import — will return NOSYS
                        };
                        import_stubs.push(stub_idx as u64);
                    }
                }

                // Calculate import_stub_table_ptr but DON'T populate yet
                // (write_bytes zeroes the data region at line below, so we populate after)
                let import_stub_table_ptr = if !import_stubs.is_empty() {
                    let ptr = unsafe { (if table0_ptr.is_null() { globals_ptr.add((num_globals * 16 + 15) & !15) } else { table0_ptr.add(table0_size as usize) as *mut u8 }) as *mut u64 };
                    ptr
                } else {
                    core::ptr::null()
                };

                let num_funcs = self.modules.get(module_addr).func_addrs.len();
                let func_table_ptr = unsafe { (if import_stub_table_ptr.is_null() { (if table0_ptr.is_null() { globals_ptr.add((num_globals * 16 + 15) & !15) } else { table0_ptr.add(table0_size as usize) as *mut u8 }) } else { import_stub_table_ptr.add(import_stubs.len()) as *mut u8 }) as *mut u64 };

                unsafe {
                    core::ptr::write_bytes(data_region_ptr, 0, 1024 * 1024); // Zero out entire 1MB data region
                    
                    let mem_addr = *self.modules.get(module_addr).mem_addrs.get(0).unwrap_or(&usize::MAX);
                    let (memory_base, memory_size) = if mem_addr != usize::MAX {
                        let mem = &self.memories.get(mem_addr).mem;
                        (mem.get_base_ptr(), mem.len())
                    } else {
                        (core::ptr::null_mut(), 0)
                    };

                    let mut ring3_ctx = crate::wasm::aot::runtime::Ring3Context {
                        store: core::ptr::null_mut(),
                        fuel: core::ptr::null_mut(),
                        memory_base,
                        memory_size,
                        stack_base: core::ptr::null_mut(), // To be set at launch
                        locals_base: core::ptr::null_mut(), // To be set at launch
                        module_addr: 0,
                        stack_limit: 0,
                        trap_code: &mut (*ctx_ptr).trap_code_storage as *mut i32,
                        blob_base: base,
                        globals_ptr,
                        globals_count: num_globals as u32,
                        _pad0: 0,
                        table0_ptr,
                        table0_size,
                        _pad1: 0,
                        func_table_ptr,
                        func_count: num_funcs as u32,
                        _pad2: 0,
                        pid: self.container_id.unwrap_or(0),
                        slot_id: slot_id,
                        _pad3: [0; 6],
                        trap_code_storage: 0,
                        _pad4: 0,
                        num_imported_funcs: import_stubs.len() as u32,
                        _pad5: 0,
                        import_stub_table: import_stub_table_ptr,
                    };
                    
                    // Copy initial global values
                    for (i, val) in all_global_vals.iter().enumerate() {
                        *(globals_ptr.add(i * 16) as *mut u128) = val.to_u128();
                    }

                    // Populate import_stub_table AFTER the zero-fill
                    if !import_stub_table_ptr.is_null() {
                        let ist_mut = import_stub_table_ptr as *mut u64;
                        for (i, &stub) in import_stubs.iter().enumerate() {
                            *ist_mut.add(i) = stub;
                        }
                    }

                    core::ptr::write(ctx_ptr, ring3_ctx);
                    maybe_ctx_ptr = Some(ctx_ptr as u64);
                }

                self.next_code_offset = (self.next_code_offset + 1024 * 1024) & !4095; // Reserve 1MB for data region
                crate::debugln!("[AOT] Context and data region initialized.");

                res
            } else {
                aot_module.code.as_ptr() as usize
            };

            // Post-relocation: Populate func_table and table0
            if let Some(ctx_u64) = maybe_ctx_ptr {
                let ctx = unsafe { &mut *(ctx_u64 as *mut crate::wasm::aot::runtime::Ring3Context) };
                for (i, offset) in aot_module.func_offsets.iter().enumerate() {
                    let func_idx = validation_info.imports_length.imported_functions + i;
                    let func_addr = self.modules.get(module_addr).func_addrs[func_idx];
                    let absolute_ptr = final_code_ptr + *offset;
                    if let FuncInst::WasmFunc(wasm_func) = self.functions.get_mut(func_addr) {
                        wasm_func.aot_ptr = Some(absolute_ptr);
                    }
                    // Update func_table
                    unsafe {
                        *ctx.func_table_ptr.add(func_idx) = absolute_ptr as u64;
                    }
                }
                
                // Populate imported function addresses in func_table
                for i in 0..validation_info.imports_length.imported_functions {
                    // For now, imported functions in AOT are called via CallHost trampoline,
                    // so we don't necessarily need their addresses here unless using CallRef on them.
                    // But for completeness:
                    unsafe {
                        *ctx.func_table_ptr.add(i) = 0; // Or point to a "call host" stub
                    }
                }

                // Copy table 0 entries
                if !ctx.table0_ptr.is_null() {
                    let table0_addr = *self.modules.get(module_addr).table_addrs.first().unwrap();
                    let t = self.tables.get(table0_addr);
                    for (i, entry) in t.elem.iter().enumerate() {
                        unsafe {
                            *ctx.table0_ptr.add(i) = match entry {
                                Ref::Func(addr) => {
                                    if let FuncInst::WasmFunc(wf) = self.functions.get(*addr) {
                                        wf.aot_ptr.unwrap_or(0) as u64
                                    } else { 0 }
                                }
                                _ => 0,
                            };
                        }
                    }
                }
            } else {
                for (i, offset) in aot_module.func_offsets.iter().enumerate() {
                    let func_idx = validation_info.imports_length.imported_functions + i;
                    let func_addr = self.modules.get(module_addr).func_addrs[func_idx];
                    if let FuncInst::WasmFunc(wasm_func) = self.functions.get_mut(func_addr) {
                        wasm_func.aot_ptr = Some(final_code_ptr + *offset);
                    }
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
            maybe_ctx_ptr,
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
        let mem = if let Some(sas_base) = self.sas_memory_base {
            if let Some(container_id) = self.container_id {
                // Check if this container has a parent to determine if it's a view or top-level SAS
                let is_nested = {
                    let registry = crate::wasm::container::CONTAINER_REGISTRY.lock();
                    registry
                        .get(&container_id)
                        .map(|c| c.lock().parent_id.is_some())
                        .unwrap_or(false)
                };

                if is_nested {
                    LinearMemory::new_view(
                        container_id,
                        sas_base as *mut u8,
                        ty.limits.min.try_into().unwrap_validated(),
                        ty.limits.max.unwrap_or(u32::MAX),
                    )
                } else {
                    LinearMemory::new_sas(sas_base, ty.limits.min.try_into().unwrap_validated())
                }
            } else {
                LinearMemory::new_sas(sas_base, ty.limits.min.try_into().unwrap_validated())
            }
        } else {
            LinearMemory::new_with_initial_pages(ty.limits.min.try_into().unwrap_validated())
        };
        self.memories.insert(MemInst { ty, mem })
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
                        let _module_addr = wasm_func_inst.module_addr;

                        // NOTE: We cannot run AOT code from kernel mode because
                        // the AOT trampolines use `syscall` which would cause
                        // re-entrant syscalls. Fall through to interpreter instead.

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
    pub maybe_ctx_ptr: Option<u64>,
}
