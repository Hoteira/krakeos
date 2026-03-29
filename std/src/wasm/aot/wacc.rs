//! `.wacc` — WASM Ahead-of-time Compiled Cache format.
//!
//! Binary format that caches AOT-compiled x86_64 code so unchanged WASM
//! modules skip recompilation on subsequent launches.

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

use crate::wasm::common::indices::TypeIdx;
use crate::wasm::common::reader::types::export::{Export, ExportDesc};
use crate::wasm::common::reader::types::global::GlobalType;
use crate::wasm::common::reader::types::import::{Import, ImportDesc};
use crate::wasm::common::reader::types::{
    FuncType, Limits, MemType, NumType, RefType, ResultType, TableType, ValType,
};
use crate::wasm::common::validation::ImportsLength;
use crate::wasm::common::value::Value;

// ── Magic & version ──────────────────────────────────────────────────

const WACC_MAGIC: [u8; 4] = [0x57, 0x41, 0x43, 0x43]; // "WACC"
const WACC_VERSION: u32 = 2;
const HEADER_SIZE: usize = 40; // magic(4) + version(4) + mtime(8) + size(8) + num_sections(4) + flags(4) + reserved(8)

// ── Section IDs ──────────────────────────────────────────────────────

const SEC_CODE: u32 = 0;
const SEC_FUNC_OFFSETS: u32 = 1;
const SEC_TYPES: u32 = 2;
const SEC_FUNCTIONS_TYPES: u32 = 3;
const SEC_IMPORTS: u32 = 4;
const SEC_EXPORTS: u32 = 5;
const SEC_MEMORIES: u32 = 6;
const SEC_GLOBALS: u32 = 7;
const SEC_TABLES: u32 = 8;
const SEC_DATA_SEGMENTS: u32 = 9;
const SEC_ELEM_SEGMENTS: u32 = 10;
const SEC_START_FUNC: u32 = 11;
const SEC_IMPORT_COUNTS: u32 = 12;
const SEC_LOCAL_FUNC_TYPES: u32 = 13;

const NUM_SECTIONS: u32 = 14;

// ── Deserialized cache info ──────────────────────────────────────────

/// Everything needed to instantiate a module without re-validating or
/// re-compiling the WASM source.
pub struct WaccInfo {
    pub code: Vec<u8>,
    pub func_offsets: Vec<usize>,
    pub types: Vec<FuncType>,
    pub functions_types: Vec<TypeIdx>,   // TypeIdx per func (imported + local)
    pub functions: Vec<TypeIdx>,         // TypeIdx per LOCAL function
    pub imports: Vec<Import>,
    pub exports: Vec<Export>,
    pub memories: Vec<MemType>,
    pub globals: Vec<WaccGlobal>,
    pub tables: Vec<TableType>,
    pub data_segments: Vec<WaccDataSeg>,
    pub elem_segments: Vec<WaccElemSeg>,
    pub start: Option<usize>,
    pub imports_length: ImportsLength,
}

pub struct WaccGlobal {
    pub ty: GlobalType,
    pub init_value: Value,
}

pub struct WaccDataSeg {
    pub init: Vec<u8>,
    pub mode: WaccDataMode,
    /// Byte range into the original `.wasm` file for the data bytes.
    /// Used when `init` is empty and we need to reconstruct from wasm.
    pub wasm_range: Option<(usize, usize)>, // (offset, length)
}

pub enum WaccDataMode {
    Passive,
    Active { memory_idx: usize, offset: i32 },
}

pub struct WaccElemSeg {
    pub mode: WaccElemMode,
    pub items: WaccElemItems,
}

#[derive(Clone)]
pub enum WaccElemMode {
    Passive,
    Active { table_idx: u32, offset: i32 },
    Declarative,
}

pub enum WaccElemItems {
    RefFuncs(Vec<u32>),
}

// ── Quick header check ───────────────────────────────────────────────

/// Read just the header of a `.wacc` file and check if it matches the
/// given mtime and size.  Returns `true` on cache hit.
pub fn wacc_header_matches(wacc_bytes: &[u8], wasm_mtime: u64, wasm_size: u64) -> bool {
    if wacc_bytes.len() < HEADER_SIZE {
        return false;
    }
    if wacc_bytes[0..4] != WACC_MAGIC {
        return false;
    }
    let version = read_u32(wacc_bytes, 4);
    if version != WACC_VERSION {
        return false;
    }
    let stored_mtime = read_u64(wacc_bytes, 8);
    let stored_size = read_u64(wacc_bytes, 16);
    stored_mtime == wasm_mtime && stored_size == wasm_size
}

// ── Serialization ────────────────────────────────────────────────────

pub struct WaccSerializer {
    buf: Vec<u8>,
}

impl WaccSerializer {
    /// Serialize an AOT compilation result into `.wacc` bytes.
    ///
    /// `global_init_vals` must contain the pre-evaluated init values for
    /// **local** (non-imported) globals, in declaration order.
    /// `data_offsets` / `elem_offsets` contain the pre-evaluated constant-
    /// expression offset for each active segment (None for passive).
    pub fn serialize(
        aot: &super::runtime::AotModule,
        vi: &crate::wasm::common::validation::ValidationInfo,
        wasm_mtime: u64,
        wasm_size: u64,
        global_init_vals: &[Value],
        data_offsets: &[Option<i32>],
        elem_offsets: &[Option<i32>],
    ) -> Vec<u8> {
        let mut s = Self { buf: Vec::new() };

        // ── Header (placeholder, will be written at end) ────────────
        s.buf.resize(HEADER_SIZE, 0);

        // ── Section directory (placeholder) ─────────────────────────
        let dir_offset = HEADER_SIZE;
        let dir_size = NUM_SECTIONS as usize * 12;
        s.buf.resize(dir_offset + dir_size, 0);

        let mut sections: Vec<(u32, u32, u32)> = Vec::new(); // (type, offset, length)

        // SEC_CODE (0)
        let off = s.buf.len() as u32;
        s.buf.extend_from_slice(&aot.code);
        sections.push((SEC_CODE, off, aot.code.len() as u32));

        // SEC_FUNC_OFFSETS (1)
        let off = s.buf.len() as u32;
        s.write_u32(aot.func_offsets.len() as u32);
        for &fo in &aot.func_offsets {
            s.write_u32(fo as u32);
        }
        sections.push((SEC_FUNC_OFFSETS, off, s.buf.len() as u32 - off));

        // SEC_TYPES (2)
        let off = s.buf.len() as u32;
        s.write_u32(vi.types.len() as u32);
        for ft in &vi.types {
            s.write_func_type(ft);
        }
        sections.push((SEC_TYPES, off, s.buf.len() as u32 - off));

        // SEC_FUNCTIONS_TYPES (3) — TypeIdx per function (imported + local)
        let off = s.buf.len() as u32;
        s.write_u32(vi.functions_types.len() as u32);
        for &ti in &vi.functions_types {
            s.write_u32(ti as u32);
        }
        sections.push((SEC_FUNCTIONS_TYPES, off, s.buf.len() as u32 - off));

        // SEC_IMPORTS (4)
        let off = s.buf.len() as u32;
        s.write_u32(vi.imports.len() as u32);
        for imp in &vi.imports {
            s.write_import(imp);
        }
        sections.push((SEC_IMPORTS, off, s.buf.len() as u32 - off));

        // SEC_EXPORTS (5)
        let off = s.buf.len() as u32;
        s.write_u32(vi.exports.len() as u32);
        for exp in &vi.exports {
            s.write_export(exp);
        }
        sections.push((SEC_EXPORTS, off, s.buf.len() as u32 - off));

        // SEC_MEMORIES (6)
        let off = s.buf.len() as u32;
        s.write_u32(vi.memories.len() as u32);
        for m in &vi.memories {
            s.write_u32(m.limits.min);
            s.write_u32(m.limits.max.unwrap_or(u32::MAX));
        }
        sections.push((SEC_MEMORIES, off, s.buf.len() as u32 - off));

        // SEC_GLOBALS (7) — local globals with pre-evaluated init values
        let off = s.buf.len() as u32;
        s.write_u32(vi.globals.len() as u32);
        for (i, g) in vi.globals.iter().enumerate() {
            s.write_u8(valtype_to_byte(&g.ty.ty));
            s.write_u8(if g.ty.is_mut { 1 } else { 0 });
            let val = if i < global_init_vals.len() {
                global_init_vals[i].to_u128()
            } else {
                0u128
            };
            s.write_u128(val);
        }
        sections.push((SEC_GLOBALS, off, s.buf.len() as u32 - off));

        // SEC_TABLES (8)
        let off = s.buf.len() as u32;
        s.write_u32(vi.tables.len() as u32);
        for t in &vi.tables {
            s.write_u8(reftype_to_byte(&t.et));
            s.write_u32(t.lim.min);
            s.write_u32(t.lim.max.unwrap_or(u32::MAX));
        }
        sections.push((SEC_TABLES, off, s.buf.len() as u32 - off));

        // SEC_DATA_SEGMENTS (9) — store (wasm_offset, len) references, not bytes
        let off = s.buf.len() as u32;
        s.write_u32(vi.data.len() as u32);
        for (i, ds) in vi.data.iter().enumerate() {
            match &ds.mode {
                crate::wasm::common::reader::types::data::DataMode::Passive => {
                    s.write_u8(0);
                }
                crate::wasm::common::reader::types::data::DataMode::Active(a) => {
                    s.write_u8(1);
                    s.write_u32(a.memory_idx as u32);
                    let offset_val = data_offsets.get(i).copied().flatten().unwrap_or(0);
                    s.write_i32(offset_val);
                }
            }
            s.write_u32(ds.wasm_init_offset as u32);
            s.write_u32(ds.init.len() as u32);
        }
        sections.push((SEC_DATA_SEGMENTS, off, s.buf.len() as u32 - off));

        // SEC_ELEM_SEGMENTS (10)
        let off = s.buf.len() as u32;
        s.write_u32(vi.elements.len() as u32);
        for (i, es) in vi.elements.iter().enumerate() {
            match &es.mode {
                crate::wasm::common::reader::types::element::ElemMode::Passive => s.write_u8(0),
                crate::wasm::common::reader::types::element::ElemMode::Active(a) => {
                    s.write_u8(1);
                    s.write_u32(a.table_idx);
                    let offset_val = elem_offsets.get(i).copied().flatten().unwrap_or(0);
                    s.write_i32(offset_val);
                }
                crate::wasm::common::reader::types::element::ElemMode::Declarative => s.write_u8(2),
            }
            match &es.init {
                crate::wasm::common::reader::types::element::ElemItems::RefFuncs(funcs) => {
                    s.write_u8(0); // kind = RefFuncs
                    s.write_u32(funcs.len() as u32);
                    for &f in funcs {
                        s.write_u32(f);
                    }
                }
                crate::wasm::common::reader::types::element::ElemItems::Exprs(_, exprs) => {
                    // Store as RefFuncs with sentinel 0xFFFFFFFF for non-func refs
                    s.write_u8(0);
                    s.write_u32(exprs.len() as u32);
                    for _ in exprs {
                        s.write_u32(0xFFFF_FFFF);
                    }
                }
            }
        }
        sections.push((SEC_ELEM_SEGMENTS, off, s.buf.len() as u32 - off));

        // SEC_START_FUNC (11)
        let off = s.buf.len() as u32;
        if let Some(start) = vi.start {
            s.write_u8(1);
            s.write_u32(start as u32);
        } else {
            s.write_u8(0);
        }
        sections.push((SEC_START_FUNC, off, s.buf.len() as u32 - off));

        // SEC_IMPORT_COUNTS (12)
        let off = s.buf.len() as u32;
        s.write_u32(vi.imports_length.imported_functions as u32);
        s.write_u32(vi.imports_length.imported_globals as u32);
        s.write_u32(vi.imports_length.imported_memories as u32);
        s.write_u32(vi.imports_length.imported_tables as u32);
        sections.push((SEC_IMPORT_COUNTS, off, s.buf.len() as u32 - off));

        // SEC_LOCAL_FUNC_TYPES (13) — TypeIdx per LOCAL function only
        let off = s.buf.len() as u32;
        s.write_u32(vi.functions.len() as u32);
        for &ti in &vi.functions {
            s.write_u32(ti as u32);
        }
        sections.push((SEC_LOCAL_FUNC_TYPES, off, s.buf.len() as u32 - off));

        // ── Write header ────────────────────────────────────────────
        s.buf[0..4].copy_from_slice(&WACC_MAGIC);
        write_u32_at(&mut s.buf, 4, WACC_VERSION);
        write_u64_at(&mut s.buf, 8, wasm_mtime);
        write_u64_at(&mut s.buf, 16, wasm_size);
        write_u32_at(&mut s.buf, 24, sections.len() as u32);
        write_u32_at(&mut s.buf, 28, 0); // flags
        // reserved 8 bytes already 0

        // ── Write section directory ─────────────────────────────────
        for (i, (sec_type, offset, length)) in sections.iter().enumerate() {
            let base = dir_offset + i * 12;
            write_u32_at(&mut s.buf, base, *sec_type);
            write_u32_at(&mut s.buf, base + 4, *offset);
            write_u32_at(&mut s.buf, base + 8, *length);
        }

        s.buf
    }

    fn write_u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    fn write_u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn write_i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn write_u128(&mut self, v: u128) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn write_str(&mut self, s: &str) {
        self.write_u32(s.len() as u32);
        self.buf.extend_from_slice(s.as_bytes());
    }
    fn write_func_type(&mut self, ft: &FuncType) {
        self.write_u32(ft.params.valtypes.len() as u32);
        for vt in &ft.params.valtypes {
            self.write_u8(valtype_to_byte(vt));
        }
        self.write_u32(ft.returns.valtypes.len() as u32);
        for vt in &ft.returns.valtypes {
            self.write_u8(valtype_to_byte(vt));
        }
    }
    fn write_import(&mut self, imp: &Import) {
        self.write_str(&imp.module_name);
        self.write_str(&imp.name);
        match &imp.desc {
            ImportDesc::Func(ti) => {
                self.write_u8(0);
                self.write_u32(*ti as u32);
            }
            ImportDesc::Table(t) => {
                self.write_u8(1);
                self.write_u8(reftype_to_byte(&t.et));
                self.write_u32(t.lim.min);
                self.write_u32(t.lim.max.unwrap_or(u32::MAX));
            }
            ImportDesc::Mem(m) => {
                self.write_u8(2);
                self.write_u32(m.limits.min);
                self.write_u32(m.limits.max.unwrap_or(u32::MAX));
            }
            ImportDesc::Global(g) => {
                self.write_u8(3);
                self.write_u8(valtype_to_byte(&g.ty));
                self.write_u8(if g.is_mut { 1 } else { 0 });
            }
        }
    }
    fn write_export(&mut self, exp: &Export) {
        self.write_str(&exp.name);
        match &exp.desc {
            ExportDesc::FuncIdx(i) => { self.write_u8(0); self.write_u32(*i as u32); }
            ExportDesc::TableIdx(i) => { self.write_u8(1); self.write_u32(*i as u32); }
            ExportDesc::MemIdx(i) => { self.write_u8(2); self.write_u32(*i as u32); }
            ExportDesc::GlobalIdx(i) => { self.write_u8(3); self.write_u32(*i as u32); }
        }
    }
}

// ── Deserialization ──────────────────────────────────────────────────

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }
    fn read_u8(&mut self) -> Option<u8> {
        if self.pos >= self.data.len() { return None; }
        let v = self.data[self.pos];
        self.pos += 1;
        Some(v)
    }
    fn read_u32(&mut self) -> Option<u32> {
        if self.pos + 4 > self.data.len() { return None; }
        let v = u32::from_le_bytes(self.data[self.pos..self.pos+4].try_into().ok()?);
        self.pos += 4;
        Some(v)
    }
    fn read_i32(&mut self) -> Option<i32> {
        if self.pos + 4 > self.data.len() { return None; }
        let v = i32::from_le_bytes(self.data[self.pos..self.pos+4].try_into().ok()?);
        self.pos += 4;
        Some(v)
    }
    fn read_u128(&mut self) -> Option<u128> {
        if self.pos + 16 > self.data.len() { return None; }
        let v = u128::from_le_bytes(self.data[self.pos..self.pos+16].try_into().ok()?);
        self.pos += 16;
        Some(v)
    }
    fn read_bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.pos + n > self.data.len() { return None; }
        let s = &self.data[self.pos..self.pos+n];
        self.pos += n;
        Some(s)
    }
    fn read_str(&mut self) -> Option<String> {
        let len = self.read_u32()? as usize;
        let bytes = self.read_bytes(len)?;
        Some(String::from(core::str::from_utf8(bytes).ok()?))
    }
    fn read_func_type(&mut self) -> Option<FuncType> {
        let pc = self.read_u32()? as usize;
        let mut params = Vec::with_capacity(pc);
        for _ in 0..pc { params.push(byte_to_valtype(self.read_u8()?)?); }
        let rc = self.read_u32()? as usize;
        let mut returns = Vec::with_capacity(rc);
        for _ in 0..rc { returns.push(byte_to_valtype(self.read_u8()?)?); }
        Some(FuncType {
            params: ResultType { valtypes: params },
            returns: ResultType { valtypes: returns },
        })
    }
    fn read_import(&mut self) -> Option<Import> {
        let module_name = self.read_str()?;
        let name = self.read_str()?;
        let kind = self.read_u8()?;
        let desc = match kind {
            0 => ImportDesc::Func(self.read_u32()? as TypeIdx),
            1 => {
                let et = byte_to_reftype(self.read_u8()?)?;
                let min = self.read_u32()?;
                let max = self.read_u32()?;
                ImportDesc::Table(TableType { et, lim: Limits { min, max: Some(max) } })
            }
            2 => {
                let min = self.read_u32()?;
                let max = self.read_u32()?;
                ImportDesc::Mem(MemType { limits: Limits { min, max: Some(max) } })
            }
            3 => {
                let ty = byte_to_valtype(self.read_u8()?)?;
                let is_mut = self.read_u8()? != 0;
                ImportDesc::Global(GlobalType { ty, is_mut })
            }
            _ => return None,
        };
        Some(Import { module_name, name, desc })
    }
    fn read_export(&mut self) -> Option<Export> {
        let name = self.read_str()?;
        let kind = self.read_u8()?;
        let idx = self.read_u32()? as usize;
        let desc = match kind {
            0 => ExportDesc::FuncIdx(idx),
            1 => ExportDesc::TableIdx(idx),
            2 => ExportDesc::MemIdx(idx),
            3 => ExportDesc::GlobalIdx(idx),
            _ => return None,
        };
        Some(Export { name, desc })
    }
}

/// Deserialize a `.wacc` file into a `WaccInfo`.
/// The caller has already verified the header via `wacc_header_matches`.
/// `wasm_bytes` is the original `.wasm` file used to reconstruct data segment bytes.
pub fn deserialize_wacc(wacc_bytes: &[u8], wasm_bytes: &[u8]) -> Option<WaccInfo> {
    if wacc_bytes.len() < HEADER_SIZE { return None; }
    if wacc_bytes[0..4] != WACC_MAGIC { return None; }
    if read_u32(wacc_bytes, 4) != WACC_VERSION { return None; }

    let num_sections = read_u32(wacc_bytes, 24) as usize;
    let dir_offset = HEADER_SIZE;
    if wacc_bytes.len() < dir_offset + num_sections * 12 { return None; }

    // Parse section directory
    let mut sec_map: alloc::collections::BTreeMap<u32, (usize, usize)> = alloc::collections::BTreeMap::new();
    for i in 0..num_sections {
        let base = dir_offset + i * 12;
        let sec_type = read_u32(wacc_bytes, base);
        let offset = read_u32(wacc_bytes, base + 4) as usize;
        let length = read_u32(wacc_bytes, base + 8) as usize;
        sec_map.insert(sec_type, (offset, length));
    }

    let get_section = |id: u32| -> Option<&[u8]> {
        let &(off, len) = sec_map.get(&id)?;
        if off + len > wacc_bytes.len() { return None; }
        Some(&wacc_bytes[off..off+len])
    };

    // SEC_CODE
    let code = get_section(SEC_CODE)?.to_vec();

    // SEC_FUNC_OFFSETS
    let fo_sec = get_section(SEC_FUNC_OFFSETS)?;
    let mut r = Reader::new(fo_sec);
    let fo_count = r.read_u32()? as usize;
    let mut func_offsets = Vec::with_capacity(fo_count);
    for _ in 0..fo_count { func_offsets.push(r.read_u32()? as usize); }

    // SEC_TYPES
    let types_sec = get_section(SEC_TYPES)?;
    let mut r = Reader::new(types_sec);
    let tc = r.read_u32()? as usize;
    let mut types = Vec::with_capacity(tc);
    for _ in 0..tc { types.push(r.read_func_type()?); }

    // SEC_FUNCTIONS_TYPES
    let ft_sec = get_section(SEC_FUNCTIONS_TYPES)?;
    let mut r = Reader::new(ft_sec);
    let ftc = r.read_u32()? as usize;
    let mut functions_types = Vec::with_capacity(ftc);
    for _ in 0..ftc { functions_types.push(r.read_u32()? as usize); }

    // SEC_IMPORTS
    let imp_sec = get_section(SEC_IMPORTS)?;
    let mut r = Reader::new(imp_sec);
    let ic = r.read_u32()? as usize;
    let mut imports = Vec::with_capacity(ic);
    for _ in 0..ic { imports.push(r.read_import()?); }

    // SEC_EXPORTS
    let exp_sec = get_section(SEC_EXPORTS)?;
    let mut r = Reader::new(exp_sec);
    let ec = r.read_u32()? as usize;
    let mut exports = Vec::with_capacity(ec);
    for _ in 0..ec { exports.push(r.read_export()?); }

    // SEC_MEMORIES
    let mem_sec = get_section(SEC_MEMORIES)?;
    let mut r = Reader::new(mem_sec);
    let mc = r.read_u32()? as usize;
    let mut memories = Vec::with_capacity(mc);
    for _ in 0..mc {
        let min = r.read_u32()?;
        let max = r.read_u32()?;
        memories.push(MemType { limits: Limits { min, max: Some(max) } });
    }

    // SEC_GLOBALS
    let glob_sec = get_section(SEC_GLOBALS)?;
    let mut r = Reader::new(glob_sec);
    let gc = r.read_u32()? as usize;
    let mut globals = Vec::with_capacity(gc);
    for _ in 0..gc {
        let ty = byte_to_valtype(r.read_u8()?)?;
        let is_mut = r.read_u8()? != 0;
        let raw = r.read_u128()?;
        let init_value = u128_to_value(&ty, raw);
        globals.push(WaccGlobal {
            ty: GlobalType { ty, is_mut },
            init_value,
        });
    }

    // SEC_TABLES
    let tbl_sec = get_section(SEC_TABLES)?;
    let mut r = Reader::new(tbl_sec);
    let tc = r.read_u32()? as usize;
    let mut tables = Vec::with_capacity(tc);
    for _ in 0..tc {
        let et = byte_to_reftype(r.read_u8()?)?;
        let min = r.read_u32()?;
        let max = r.read_u32()?;
        tables.push(TableType { et, lim: Limits { min, max: Some(max) } });
    }

    // SEC_DATA_SEGMENTS — read (wasm_offset, len) and slice from wasm_bytes
    let ds_sec = get_section(SEC_DATA_SEGMENTS)?;
    let mut r = Reader::new(ds_sec);
    let dc = r.read_u32()? as usize;
    let mut data_segments = Vec::with_capacity(dc);
    for _ in 0..dc {
        let mode_byte = r.read_u8()?;
        let mode = match mode_byte {
            0 => WaccDataMode::Passive,
            1 => {
                let memory_idx = r.read_u32()? as usize;
                let offset = r.read_i32()?;
                WaccDataMode::Active { memory_idx, offset }
            }
            _ => return None,
        };
        let wasm_offset = r.read_u32()? as usize;
        let data_len = r.read_u32()? as usize;
        if wasm_offset + data_len > wasm_bytes.len() { return None; }
        let init = wasm_bytes[wasm_offset..wasm_offset + data_len].to_vec();
        data_segments.push(WaccDataSeg { init, mode, wasm_range: Some((wasm_offset, data_len)) });
    }

    // SEC_ELEM_SEGMENTS
    let es_sec = get_section(SEC_ELEM_SEGMENTS)?;
    let mut r = Reader::new(es_sec);
    let esc = r.read_u32()? as usize;
    let mut elem_segments = Vec::with_capacity(esc);
    for _ in 0..esc {
        let mode_byte = r.read_u8()?;
        let mode = match mode_byte {
            0 => WaccElemMode::Passive,
            1 => {
                let table_idx = r.read_u32()?;
                let offset = r.read_i32()?;
                WaccElemMode::Active { table_idx, offset }
            }
            2 => WaccElemMode::Declarative,
            _ => return None,
        };
        let _kind = r.read_u8()?; // always 0 = RefFuncs
        let item_count = r.read_u32()? as usize;
        let mut funcs = Vec::with_capacity(item_count);
        for _ in 0..item_count { funcs.push(r.read_u32()?); }
        elem_segments.push(WaccElemSeg {
            mode,
            items: WaccElemItems::RefFuncs(funcs),
        });
    }

    // SEC_START_FUNC
    let sf_sec = get_section(SEC_START_FUNC)?;
    let mut r = Reader::new(sf_sec);
    let has_start = r.read_u8()?;
    let start = if has_start != 0 { Some(r.read_u32()? as usize) } else { None };

    // SEC_IMPORT_COUNTS
    let ic_sec = get_section(SEC_IMPORT_COUNTS)?;
    let mut r = Reader::new(ic_sec);
    let imports_length = ImportsLength {
        imported_functions: r.read_u32()? as usize,
        imported_globals: r.read_u32()? as usize,
        imported_memories: r.read_u32()? as usize,
        imported_tables: r.read_u32()? as usize,
    };

    // SEC_LOCAL_FUNC_TYPES
    let lft_sec = get_section(SEC_LOCAL_FUNC_TYPES)?;
    let mut r = Reader::new(lft_sec);
    let lfc = r.read_u32()? as usize;
    let mut functions = Vec::with_capacity(lfc);
    for _ in 0..lfc { functions.push(r.read_u32()? as usize); }

    Some(WaccInfo {
        code,
        func_offsets,
        types,
        functions_types,
        functions,
        imports,
        exports,
        memories,
        globals,
        tables,
        data_segments,
        elem_segments,
        start,
        imports_length,
    })
}

// ── Helpers ──────────────────────────────────────────────────────────

fn read_u32(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(buf[offset..offset+4].try_into().unwrap_or([0; 4]))
}

fn read_u64(buf: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(buf[offset..offset+8].try_into().unwrap_or([0; 8]))
}

fn write_u32_at(buf: &mut [u8], offset: usize, val: u32) {
    buf[offset..offset+4].copy_from_slice(&val.to_le_bytes());
}

fn write_u64_at(buf: &mut [u8], offset: usize, val: u64) {
    buf[offset..offset+8].copy_from_slice(&val.to_le_bytes());
}

fn valtype_to_byte(vt: &ValType) -> u8 {
    match vt {
        ValType::NumType(NumType::I32) => 0x7F,
        ValType::NumType(NumType::I64) => 0x7E,
        ValType::NumType(NumType::F32) => 0x7D,
        ValType::NumType(NumType::F64) => 0x7C,
        ValType::VecType => 0x7B,
        ValType::RefType(RefType::FuncRef) => 0x70,
        ValType::RefType(RefType::ExternRef) => 0x6F,
    }
}

fn byte_to_valtype(b: u8) -> Option<ValType> {
    match b {
        0x7F => Some(ValType::NumType(NumType::I32)),
        0x7E => Some(ValType::NumType(NumType::I64)),
        0x7D => Some(ValType::NumType(NumType::F32)),
        0x7C => Some(ValType::NumType(NumType::F64)),
        0x7B => Some(ValType::VecType),
        0x70 => Some(ValType::RefType(RefType::FuncRef)),
        0x6F => Some(ValType::RefType(RefType::ExternRef)),
        _ => None,
    }
}

fn reftype_to_byte(rt: &RefType) -> u8 {
    match rt {
        RefType::FuncRef => 0x70,
        RefType::ExternRef => 0x6F,
    }
}

fn byte_to_reftype(b: u8) -> Option<RefType> {
    match b {
        0x70 => Some(RefType::FuncRef),
        0x6F => Some(RefType::ExternRef),
        _ => None,
    }
}

fn u128_to_value(ty: &ValType, raw: u128) -> Value {
    match ty {
        ValType::NumType(NumType::I32) => Value::I32(raw as u32),
        ValType::NumType(NumType::I64) => Value::I64(raw as u64),
        ValType::NumType(NumType::F32) => Value::F32(crate::wasm::common::value::F32(f32::from_bits(raw as u32))),
        ValType::NumType(NumType::F64) => Value::F64(crate::wasm::common::value::F64(f64::from_bits(raw as u64))),
        ValType::VecType => Value::V128(raw.to_le_bytes()),
        ValType::RefType(_) => Value::I64(raw as u64), // funcref/externref stored as addr
    }
}
