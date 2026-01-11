use crate::rust_alloc::string::String;
use crate::rust_alloc::vec::Vec;
use crate::rust_alloc::vec;
use super::types::*;

#[derive(Debug)]
pub enum ParseError {
    UnexpectedEof,
    InvalidMagic,
    InvalidVersion,
    InvalidSectionId(u8),
    InvalidValType(u8),
    InvalidUtf8,
    MalformedLeb128,
    Other(&'static str),
}

pub type Result<T> = core::result::Result<T, ParseError>;

pub struct Parser<'a> {
    data: &'a [u8],
    pub pos: usize,
}

impl<'a> Parser<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn parse_module(&mut self) -> Result<Module> {
        if self.pos + 8 > self.data.len() { return Err(ParseError::UnexpectedEof); }
        let magic = &self.data[self.pos..self.pos+4];
        if magic != b"\0asm" { return Err(ParseError::InvalidMagic); }
        let version = &self.data[self.pos+4..self.pos+8];
        
        if version == b"\x01\0\0\0" {
            self.pos += 8;
            self.parse_core_module_content()
        } else {
            // Component Model
            self.pos += 8;
            self.parse_component()
        }
    }

    fn parse_component(&mut self) -> Result<Module> {
        while self.pos < self.data.len() {
            let section_id = self.read_u8()?;
            let section_len = self.read_u32_leb128()?;
            let start_pos = self.pos;
            if section_id == 1 { // Module section
                let module_data = &self.data[self.pos..self.pos + (section_len as usize)];
                let mut sub_parser = Parser::new(module_data);
                return sub_parser.parse_module();
            }
            self.skip(section_len as usize)?;
        }
        Err(ParseError::Other("No Module in Component"))
    }

    fn parse_core_module_content(&mut self) -> Result<Module> {
        let mut module = Module {
            types: Vec::new(), imports: Vec::new(), functions: Vec::new(),
            tables: Vec::new(), memories: Vec::new(), globals: Vec::new(),
            exports: Vec::new(), start: None, elements: Vec::new(),
            code: Vec::new(), data: Vec::new(),
        };

        while self.pos < self.data.len() {
            let section_id = self.read_u8()?;
            let section_len = self.read_u32_leb128()?;
            let start_pos = self.pos;

            match section_id {
                1 => self.parse_type_section(&mut module)?,
                2 => self.parse_import_section(&mut module)?,
                3 => self.parse_function_section(&mut module)?,
                4 => self.parse_table_section(&mut module)?,
                5 => self.parse_memory_section(&mut module)?,
                6 => self.parse_global_section(&mut module)?,
                7 => self.parse_export_section(&mut module)?,
                8 => self.parse_start_section(&mut module)?,
                9 => self.parse_element_section(&mut module)?,
                10 => self.parse_code_section(&mut module)?,
                11 => self.parse_data_section(&mut module)?,
                _ => self.skip(section_len as usize)?,
            }

            self.pos = start_pos + (section_len as usize);
        }
        Ok(module)
    }

    fn parse_type_section(&mut self, module: &mut Module) -> Result<()> {
        let count = self.read_u32_leb128()?;
        for _ in 0..count {
            if self.read_u8()? != 0x60 { return Err(ParseError::Other("Invalid func type")); }
            let p_count = self.read_u32_leb128()?;
            let mut params = Vec::new(); for _ in 0..p_count { params.push(self.read_val_type()?); }
            let r_count = self.read_u32_leb128()?;
            let mut results = Vec::new(); for _ in 0..r_count { results.push(self.read_val_type()?); }
            module.types.push(FuncType { params, results });
        }
        Ok(())
    }

    fn parse_import_section(&mut self, module: &mut Module) -> Result<()> {
        let count = self.read_u32_leb128()?;
        for _ in 0..count {
            let m = self.read_name()?; let n = self.read_name()?;
            let kind = self.read_u8()?;
            let desc = match kind {
                0x00 => ImportDesc::Func(self.read_u32_leb128()?),
                0x01 => ImportDesc::Table(self.read_table_type()?),
                0x02 => ImportDesc::Memory(self.read_memory_type()?),
                0x03 => ImportDesc::Global(self.read_global_type()?),
                _ => return Err(ParseError::Other("Invalid import")),
            };
            module.imports.push(Import { module: m, name: n, desc });
        }
        Ok(())
    }

    fn parse_function_section(&mut self, module: &mut Module) -> Result<()> {
        let count = self.read_u32_leb128()?;
        for _ in 0..count { module.functions.push(self.read_u32_leb128()?); }
        Ok(())
    }

    fn parse_table_section(&mut self, module: &mut Module) -> Result<()> {
        let count = self.read_u32_leb128()?;
        for _ in 0..count { module.tables.push(self.read_table_type()?); }
        Ok(())
    }

    fn parse_memory_section(&mut self, module: &mut Module) -> Result<()> {
        let count = self.read_u32_leb128()?;
        for _ in 0..count { module.memories.push(self.read_memory_type()?); }
        Ok(())
    }

    fn parse_global_section(&mut self, module: &mut Module) -> Result<()> {
        let count = self.read_u32_leb128()?;
        for _ in 0..count {
            let ty = self.read_global_type()?;
            let init = self.read_expr()?;
            module.globals.push(Global { ty, init });
        }
        Ok(())
    }

    fn parse_export_section(&mut self, module: &mut Module) -> Result<()> {
        let count = self.read_u32_leb128()?;
        for _ in 0..count {
            let name = self.read_name()?;
            let kind = self.read_u8()?;
            let idx = self.read_u32_leb128()?;
            let desc = match kind {
                0x00 => ExportDesc::Func(idx),
                0x01 => ExportDesc::Table(idx),
                0x02 => ExportDesc::Memory(idx),
                0x03 => ExportDesc::Global(idx),
                _ => return Err(ParseError::Other("Invalid export")),
            };
            module.exports.push(Export { name, desc });
        }
        Ok(())
    }

    fn parse_start_section(&mut self, module: &mut Module) -> Result<()> {
        module.start = Some(self.read_u32_leb128()?);
        Ok(())
    }

    fn parse_element_section(&mut self, module: &mut Module) -> Result<()> {
        let count = self.read_u32_leb128()?;
        for _ in 0..count {
            let flags = self.read_u32_leb128()?;
            // Flag bitmask:
            // 0: active, table 0, expr offset, vec<funcidx>
            // 1: passive, kind, vec<funcidx>
            // 2: active, tableidx, expr offset, kind, vec<funcidx>
            // ... and so on.
            
            let table_index = if (flags & 2) != 0 { self.read_u32_leb128()? } else { 0 };
            let offset = if (flags & 1) == 0 { self.read_expr()? } else { Expr { instructions: Vec::new() } };
            
            if (flags & 3) != 0 && (flags & 4) == 0 {
                 let _kind = self.read_u8()?; // elemkind (0x00 = funcref)
            }

            let num_funcs = self.read_u32_leb128()?;
            let mut init = Vec::new();
            for _ in 0..num_funcs {
                // If flag bit 3 set, it's vec<expr>, else vec<funcidx>
                if (flags & 4) != 0 {
                     let expr = self.read_expr()?;
                     // Extract funcidx from (ref.func idx)
                     if expr.instructions[0] == 0xD2 { // ref.func
                          let mut sub_ip = 1;
                          init.push(Self::read_u32_leb128_static(&expr.instructions, &mut sub_ip).unwrap_or(0));
                     }
                } else {
                    init.push(self.read_u32_leb128()?);
                }
            }
            module.elements.push(Element { table_index, offset, init });
        }
        Ok(())
    }

    fn parse_code_section(&mut self, module: &mut Module) -> Result<()> {
        let count = self.read_u32_leb128()?;
        for _ in 0..count {
            let size = self.read_u32_leb128()?;
            let start = self.pos;
            let l_count = self.read_u32_leb128()?;
            let mut locals = Vec::new();
            for _ in 0..l_count { locals.push(Local { count: self.read_u32_leb128()?, ty: self.read_val_type()? }); }
            let code_len = (size as usize) - (self.pos - start);
            let mut code = vec![0u8; code_len];
            self.read_exact(&mut code)?;
            module.code.push(FunctionBody { locals, code });
        }
        Ok(())
    }

    fn parse_data_section(&mut self, module: &mut Module) -> Result<()> {
        let count = self.read_u32_leb128()?;
        for _ in 0..count {
            let flags = self.read_u32_leb128()?;
            let mem_idx = if flags == 2 { self.read_u32_leb128()? } else { 0 };
            let offset = if (flags & 1) == 0 { Some(self.read_expr()?) } else { None };
            let size = self.read_u32_leb128()?;
            let mut init = vec![0u8; size as usize];
            self.read_exact(&mut init)?;
            module.data.push(DataSegment { memory_index: mem_idx, offset, init });
        }
        Ok(())
    }

    fn read_u8(&mut self) -> Result<u8> {
        if self.pos >= self.data.len() { return Err(ParseError::UnexpectedEof); }
        let b = self.data[self.pos]; self.pos += 1; Ok(b)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        if self.pos + buf.len() > self.data.len() { return Err(ParseError::UnexpectedEof); }
        buf.copy_from_slice(&self.data[self.pos..self.pos + buf.len()]);
        self.pos += buf.len(); Ok(())
    }

    fn skip(&mut self, n: usize) -> Result<()> {
        if self.pos + n > self.data.len() { return Err(ParseError::UnexpectedEof); }
        self.pos += n; Ok(())
    }

    fn read_u32_leb128(&mut self) -> Result<u32> {
        let mut res = 0; let mut shift = 0;
        loop {
            let b = self.read_u8()?; res |= ((b & 0x7F) as u32) << shift;
            if (b & 0x80) == 0 { break; } shift += 7;
        }
        Ok(res)
    }

    fn read_u32_leb128_static(data: &[u8], ip: &mut usize) -> Result<u32> {
        let mut res = 0; let mut shift = 0;
        loop {
            if *ip >= data.len() { return Err(ParseError::UnexpectedEof); }
            let b = data[*ip]; *ip += 1; res |= ((b & 0x7F) as u32) << shift;
            if (b & 0x80) == 0 { break; } shift += 7;
        }
        Ok(res)
    }

    fn read_val_type(&mut self) -> Result<ValType> {
        match self.read_u8()? {
            0x7F => Ok(ValType::I32), 0x7E => Ok(ValType::I64),
            0x7D => Ok(ValType::F32), 0x7C => Ok(ValType::F64),
            0x70 => Ok(ValType::FuncRef), 0x6F => Ok(ValType::ExternRef),
            b => Err(ParseError::InvalidValType(b)),
        }
    }

    fn read_table_type(&mut self) -> Result<TableType> { Ok(TableType { element_type: self.read_val_type()?, limits: self.read_limits()? }) }
    fn read_memory_type(&mut self) -> Result<MemoryType> { Ok(MemoryType { limits: self.read_limits()? }) }
    fn read_global_type(&mut self) -> Result<GlobalType> { Ok(GlobalType { content_type: self.read_val_type()?, mutability: self.read_u8()? == 1 }) }
    fn read_limits(&mut self) -> Result<Limits> {
        let flags = self.read_u8()?; let min = self.read_u32_leb128()?;
        let max = if flags == 1 { Some(self.read_u32_leb128()?) } else { None };
        Ok(Limits { min, max })
    }

    fn read_name(&mut self) -> Result<String> {
        let len = self.read_u32_leb128()?;
        let mut buf = vec![0u8; len as usize];
        self.read_exact(&mut buf)?;
        String::from_utf8(buf).map_err(|_| ParseError::InvalidUtf8)
    }

    fn read_expr(&mut self) -> Result<Expr> {
        let mut instructions = Vec::new();
        let mut depth = 1;
        while depth > 0 {
            let op = self.read_u8()?; instructions.push(op);
            match op {
                0x0B => depth -= 1,
                0x41 | 0x20 | 0x21 | 0x22 | 0x23 | 0x24 | 0x10 | 0xD2 => { 
                    let start = self.pos; self.read_u32_leb128()?; 
                    instructions.extend_from_slice(&self.data[start..self.pos]); 
                },
                0x42 => { 
                    let start = self.pos; /* i64 leb */ 
                    loop { let b = self.read_u8()?; if (b & 0x80) == 0 { break; } }; 
                    instructions.extend_from_slice(&self.data[start..self.pos]); 
                },
                0xD0 => {
                    instructions.push(self.read_u8()?); // reftype
                },
                _ => {}
            }
        }
        Ok(Expr { instructions })
    }
}