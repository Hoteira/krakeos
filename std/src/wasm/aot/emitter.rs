use super::memory::ExecutableBuffer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Reg {
    RAX = 0, RCX = 1, RDX = 2, RBX = 3, RSP = 4, RBP = 5, RSI = 6, RDI = 7,
    R8 = 8, R9 = 9, R10 = 10, R11 = 11, R12 = 12, R13 = 13, R14 = 14, R15 = 15,
}

pub struct Assembler {
    pub buf: ExecutableBuffer,
}

impl Assembler {
    pub fn new() -> Self {
        Self { buf: ExecutableBuffer::new() }
    }

    pub fn ret(&mut self) {
        self.buf.emit_u8(0xC3);
    }

    pub fn push_reg(&mut self, reg: Reg) {
        let r = reg as u8;
        if r < 8 {
            self.buf.emit_u8(0x50 + r);
        } else {
            self.buf.emit_u8(0x41);
            self.buf.emit_u8(0x50 + (r - 8));
        }
    }

    pub fn pop_reg(&mut self, reg: Reg) {
        let r = reg as u8;
        if r < 8 {
            self.buf.emit_u8(0x58 + r);
        } else {
            self.buf.emit_u8(0x41);
            self.buf.emit_u8(0x58 + (r - 8));
        }
    }

    pub fn mov_reg_reg(&mut self, dst: Reg, src: Reg) {
        let dst_code = dst as u8;
        let src_code = src as u8;
        let mut rex = 0x48;
        if src_code >= 8 { rex |= 0x04; }
        if dst_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0x89);
        let modrm = 0xC0 | ((src_code & 7) << 3) | (dst_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn mov_reg_imm64(&mut self, reg: Reg, imm: u64) {
        let r = reg as u8;
        if r < 8 {
            self.buf.emit_u8(0x48);
            self.buf.emit_u8(0xB8 + r);
        } else {
            self.buf.emit_u8(0x49);
            self.buf.emit_u8(0xB8 + (r - 8));
        }
        self.buf.emit_u64(imm);
    }

    pub fn add_reg_reg(&mut self, dst: Reg, src: Reg) {
        let dst_code = dst as u8;
        let src_code = src as u8;
        let mut rex = 0x48;
        if src_code >= 8 { rex |= 0x04; }
        if dst_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0x01);
        let modrm = 0xC0 | ((src_code & 7) << 3) | (dst_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn sub_reg_reg(&mut self, dst: Reg, src: Reg) {
        let dst_code = dst as u8;
        let src_code = src as u8;
        let mut rex = 0x48;
        if src_code >= 8 { rex |= 0x04; }
        if dst_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0x29);
        let modrm = 0xC0 | ((src_code & 7) << 3) | (dst_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn add_r32_r32(&mut self, dst: Reg, src: Reg) {
        let dst_code = dst as u8;
        let src_code = src as u8;
        let mut rex = 0;
        if src_code >= 8 { rex |= 0x04; }
        if dst_code >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x01);
        let modrm = 0xC0 | ((src_code & 7) << 3) | (dst_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn sub_r32_r32(&mut self, dst: Reg, src: Reg) {
        let dst_code = dst as u8;
        let src_code = src as u8;
        let mut rex = 0;
        if src_code >= 8 { rex |= 0x04; }
        if dst_code >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x29);
        let modrm = 0xC0 | ((src_code & 7) << 3) | (dst_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn imul_r32_r32(&mut self, dst: Reg, src: Reg) {
        let dst_code = dst as u8;
        let src_code = src as u8;
        let mut rex = 0;
        if src_code >= 8 { rex |= 0x01; }
        if dst_code >= 8 { rex |= 0x04; } 
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0xAF);
        let modrm = 0xC0 | ((dst_code & 7) << 3) | (src_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn and_r32_r32(&mut self, dst: Reg, src: Reg) {
        let dst_code = dst as u8;
        let src_code = src as u8;
        let mut rex = 0;
        if src_code >= 8 { rex |= 0x04; }
        if dst_code >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x21);
        let modrm = 0xC0 | ((src_code & 7) << 3) | (dst_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn or_r32_r32(&mut self, dst: Reg, src: Reg) {
        let dst_code = dst as u8;
        let src_code = src as u8;
        let mut rex = 0;
        if src_code >= 8 { rex |= 0x04; }
        if dst_code >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x09);
        let modrm = 0xC0 | ((src_code & 7) << 3) | (dst_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn xor_r32_r32(&mut self, dst: Reg, src: Reg) {
        let dst_code = dst as u8;
        let src_code = src as u8;
        let mut rex = 0;
        if src_code >= 8 { rex |= 0x04; }
        if dst_code >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x31);
        let modrm = 0xC0 | ((src_code & 7) << 3) | (dst_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn shl_r32_cl(&mut self, dst: Reg) {
        let dst_code = dst as u8;
        let mut rex = 0;
        if dst_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0xD3);
        let modrm = 0xC0 | (4 << 3) | (dst_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn shr_r32_cl(&mut self, dst: Reg) {
        let dst_code = dst as u8;
        let mut rex = 0;
        if dst_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0xD3);
        let modrm = 0xC0 | (5 << 3) | (dst_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn sar_r32_cl(&mut self, dst: Reg) {
        let dst_code = dst as u8;
        let mut rex = 0;
        if dst_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0xD3);
        let modrm = 0xC0 | (7 << 3) | (dst_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn mov_reg_mem(&mut self, dst: Reg, src: Reg, offset: i32) {
        let dst_code = dst as u8;
        let src_code = src as u8;
        let mut rex = 0x48;
        if dst_code >= 8 { rex |= 0x04; }
        if src_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0x8B);
        if offset == 0 && (src_code & 7) != 5 {
             let modrm = ((dst_code & 7) << 3) | (src_code & 7);
             self.buf.emit_u8(modrm);
             if (src_code & 7) == 4 { self.buf.emit_u8(0x24); }
        } else if offset >= -128 && offset <= 127 {
             let modrm = 0x40 | ((dst_code & 7) << 3) | (src_code & 7);
             self.buf.emit_u8(modrm);
             if (src_code & 7) == 4 { self.buf.emit_u8(0x24); }
             self.buf.emit_u8(offset as u8);
        } else {
             let modrm = 0x80 | ((dst_code & 7) << 3) | (src_code & 7);
             self.buf.emit_u8(modrm);
             if (src_code & 7) == 4 { self.buf.emit_u8(0x24); }
             self.buf.emit_u32(offset as u32);
        }
    }

    fn emit_sib_mem(&mut self, reg_code: u8, base_code: u8, idx_code: u8, disp: i32) {
        let mut mod_bits = 0x80;
        if disp == 0 && (base_code & 7) != 5 { mod_bits = 0x00; }
        else if disp >= -128 && disp <= 127 { mod_bits = 0x40; }
        let modrm = mod_bits | ((reg_code & 7) << 3) | 4;
        self.buf.emit_u8(modrm);
        let sib = ((idx_code & 7) << 3) | (base_code & 7);
        self.buf.emit_u8(sib);
        if mod_bits == 0x40 {
            self.buf.emit_u8(disp as u8);
        } else if mod_bits == 0x80 || (base_code & 7) == 5 {
            self.buf.emit_u32(disp as u32);
        }
    }

    pub fn mov_mem_reg(&mut self, dst: Reg, offset: i32, src: Reg) {
        let dst_code = dst as u8;
        let src_code = src as u8;
        let mut rex = 0x48;
        if src_code >= 8 { rex |= 0x04; }
        if dst_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0x89);
        if offset == 0 && (dst_code & 7) != 5 {
             let modrm = ((src_code & 7) << 3) | (dst_code & 7);
             self.buf.emit_u8(modrm);
             if (dst_code & 7) == 4 { self.buf.emit_u8(0x24); }
        } else if offset >= -128 && offset <= 127 {
             let modrm = 0x40 | ((src_code & 7) << 3) | (dst_code & 7);
             self.buf.emit_u8(modrm);
             if (dst_code & 7) == 4 { self.buf.emit_u8(0x24); }
             self.buf.emit_u8(offset as u8);
        } else {
             let modrm = 0x80 | ((src_code & 7) << 3) | (dst_code & 7);
             self.buf.emit_u8(modrm);
             if (dst_code & 7) == 4 { self.buf.emit_u8(0x24); }
             self.buf.emit_u32(offset as u32);
        }
    }

    pub fn imul_reg_reg(&mut self, dst: Reg, src: Reg) {
        let dst_code = dst as u8;
        let src_code = src as u8;
        let mut rex = 0x48;
        if src_code >= 8 { rex |= 0x01; }
        if dst_code >= 8 { rex |= 0x04; } 
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0xAF);
        let modrm = 0xC0 | ((dst_code & 7) << 3) | (src_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn idiv_reg(&mut self, src: Reg) {
        let src_code = src as u8;
        let mut rex = 0x48;
        if src_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0xF7);
        let modrm = 0xC0 | (7 << 3) | (src_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn cqo(&mut self) {
        self.buf.emit_u8(0x48);
        self.buf.emit_u8(0x99);
    }

    pub fn and_reg_reg(&mut self, dst: Reg, src: Reg) {
        let dst_code = dst as u8;
        let src_code = src as u8;
        let mut rex = 0x48;
        if src_code >= 8 { rex |= 0x04; }
        if dst_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0x21);
        let modrm = 0xC0 | ((src_code & 7) << 3) | (dst_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn or_reg_reg(&mut self, dst: Reg, src: Reg) {
        let dst_code = dst as u8;
        let src_code = src as u8;
        let mut rex = 0x48;
        if src_code >= 8 { rex |= 0x04; }
        if dst_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0x09);
        let modrm = 0xC0 | ((src_code & 7) << 3) | (dst_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn xor_reg_reg(&mut self, dst: Reg, src: Reg) {
        let dst_code = dst as u8;
        let src_code = src as u8;
        let mut rex = 0x48;
        if src_code >= 8 { rex |= 0x04; }
        if dst_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0x31);
        let modrm = 0xC0 | ((src_code & 7) << 3) | (dst_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn shl_reg_cl(&mut self, dst: Reg) {
        let dst_code = dst as u8;
        let mut rex = 0x48;
        if dst_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0xD3);
        let modrm = 0xC0 | (4 << 3) | (dst_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn shr_reg_cl(&mut self, dst: Reg) {
        let dst_code = dst as u8;
        let mut rex = 0x48;
        if dst_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0xD3);
        let modrm = 0xC0 | (5 << 3) | (dst_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn sar_reg_cl(&mut self, dst: Reg) {
        let dst_code = dst as u8;
        let mut rex = 0x48;
        if dst_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0xD3);
        let modrm = 0xC0 | (7 << 3) | (dst_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn cmp_reg_reg(&mut self, r1: Reg, r2: Reg) {
        let dst_code = r1 as u8;
        let src_code = r2 as u8;
        let mut rex = 0x48;
        if src_code >= 8 { rex |= 0x04; }
        if dst_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0x39);
        let modrm = 0xC0 | ((src_code & 7) << 3) | (dst_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn setcc(&mut self, cond_code: u8, dst: Reg) {
        let r = dst as u8;
        let mut rex = 0;
        if r >= 8 { rex |= 0x41; }
        else if r >= 4 { rex |= 0x40; }
        if rex != 0 { self.buf.emit_u8(rex); }
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0x90 | (cond_code & 0xF));
        let modrm = 0xC0 | (0 << 3) | (r & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn jmp_rel32(&mut self, offset: i32) {
        self.buf.emit_u8(0xE9);
        self.buf.emit_u32(offset as u32);
    }

    pub fn jcc_rel32(&mut self, cond_code: u8, offset: i32) {
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0x80 | (cond_code & 0xF));
        self.buf.emit_u32(offset as u32);
    }

    pub fn mov_r32_mem_base_idx(&mut self, dst: Reg, base: Reg, idx: Reg, offset: i32) {
        let dst_code = dst as u8;
        let base_code = base as u8;
        let idx_code = idx as u8;
        let mut rex = 0;
        if dst_code >= 8 { rex |= 0x04; }
        if idx_code >= 8 { rex |= 0x02; }
        if base_code >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x8B); 
        self.emit_sib_mem(dst_code, base_code, idx_code, offset);
    }

    pub fn mov_r64_mem_base_idx(&mut self, dst: Reg, base: Reg, idx: Reg, offset: i32) {
        let dst_code = dst as u8;
        let base_code = base as u8;
        let idx_code = idx as u8;
        let mut rex = 0x48; 
        if dst_code >= 8 { rex |= 0x04; }
        if idx_code >= 8 { rex |= 0x02; }
        if base_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0x8B);
        self.emit_sib_mem(dst_code, base_code, idx_code, offset);
    }

    pub fn movzx_r8_mem_base_idx(&mut self, dst: Reg, base: Reg, idx: Reg, offset: i32) {
        let dst_code = dst as u8;
        let base_code = base as u8;
        let idx_code = idx as u8;
        let mut rex = 0;
        if dst_code >= 8 { rex |= 0x04; }
        if idx_code >= 8 { rex |= 0x02; }
        if base_code >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0xB6);
        self.emit_sib_mem(dst_code, base_code, idx_code, offset);
    }

    // movsx r8 (Load 8-bit sign extend)
    pub fn movsx_r8_mem_base_idx(&mut self, dst: Reg, base: Reg, idx: Reg, offset: i32) {
        let dst_code = dst as u8;
        let base_code = base as u8;
        let idx_code = idx as u8;
        let mut rex = 0;
        if dst_code >= 8 { rex |= 0x04; }
        if idx_code >= 8 { rex |= 0x02; }
        if base_code >= 8 { rex |= 0x01; }
        // 0F BE /r
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0xBE);
        self.emit_sib_mem(dst_code, base_code, idx_code, offset);
    }

    pub fn movzx_r16_mem_base_idx(&mut self, dst: Reg, base: Reg, idx: Reg, offset: i32) {
        let dst_code = dst as u8;
        let base_code = base as u8;
        let idx_code = idx as u8;
        let mut rex = 0x48;
        if dst_code >= 8 { rex |= 0x04; }
        if idx_code >= 8 { rex |= 0x02; }
        if base_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0xB7);
        self.emit_sib_mem(dst_code, base_code, idx_code, offset);
    }

    pub fn movsx_r16_mem_base_idx(&mut self, dst: Reg, base: Reg, idx: Reg, offset: i32) {
        let dst_code = dst as u8;
        let base_code = base as u8;
        let idx_code = idx as u8;
        let mut rex = 0x48;
        if dst_code >= 8 { rex |= 0x04; }
        if idx_code >= 8 { rex |= 0x02; }
        if base_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0xBF);
        self.emit_sib_mem(dst_code, base_code, idx_code, offset);
    }

    pub fn mov_mem_base_idx_r32(&mut self, base: Reg, idx: Reg, offset: i32, src: Reg) {
        let src_code = src as u8;
        let base_code = base as u8;
        let idx_code = idx as u8;
        let mut rex = 0;
        if src_code >= 8 { rex |= 0x04; }
        if idx_code >= 8 { rex |= 0x02; }
        if base_code >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x89);
        self.emit_sib_mem(src_code, base_code, idx_code, offset);
    }

    pub fn mov_mem_base_idx_r64(&mut self, base: Reg, idx: Reg, offset: i32, src: Reg) {
        let src_code = src as u8;
        let base_code = base as u8;
        let idx_code = idx as u8;
        let mut rex = 0x48;
        if src_code >= 8 { rex |= 0x04; }
        if idx_code >= 8 { rex |= 0x02; }
        if base_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0x89);
        self.emit_sib_mem(src_code, base_code, idx_code, offset);
    }

    pub fn mov_mem_base_idx_r8(&mut self, base: Reg, idx: Reg, offset: i32, src: Reg) {
        let src_code = src as u8;
        let base_code = base as u8;
        let idx_code = idx as u8;
        let mut rex = 0;
        if src_code >= 4 { rex |= 0x40; } // Need REX for SIL/DIL access
        if src_code >= 8 { rex |= 0x04; }
        if idx_code >= 8 { rex |= 0x02; }
        if base_code >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x88);
        self.emit_sib_mem(src_code, base_code, idx_code, offset);
    }

    pub fn mov_mem_base_idx_r16(&mut self, base: Reg, idx: Reg, offset: i32, src: Reg) {
        let src_code = src as u8;
        let base_code = base as u8;
        let idx_code = idx as u8;
        self.buf.emit_u8(0x66);
        let mut rex = 0;
        if src_code >= 8 { rex |= 0x04; }
        if idx_code >= 8 { rex |= 0x02; }
        if base_code >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x89);
        self.emit_sib_mem(src_code, base_code, idx_code, offset);
    }

    pub fn current_offset(&self) -> usize {
        self.buf.len()
    }

    pub fn patch_i32(&mut self, offset: usize, value: i32) {
        let bytes = value.to_le_bytes();
        for (i, b) in bytes.iter().enumerate() {
            self.buf.buffer[offset + i] = *b;
        }
    }

    pub fn call_reg(&mut self, reg: Reg) {
        let r = reg as u8;
        let mut rex = 0;
        if r >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0xFF);
        let modrm = 0xC0 | (2 << 3) | (r & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn cmov_reg_reg(&mut self, cond_code: u8, dst: Reg, src: Reg) {
        let dst_code = dst as u8;
        let src_code = src as u8;
        let mut rex = 0x48; 
        if src_code >= 8 { rex |= 0x04; }
        if dst_code >= 8 { rex |= 0x01; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0x40 | (cond_code & 0xF));
        let modrm = 0xC0 | ((dst_code & 7) << 3) | (src_code & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn movq_xmm_reg(&mut self, xmm: u8, reg: Reg) {
        let r = reg as u8;
        self.buf.emit_u8(0x66);
        let mut rex = 0x48;
        if r >= 8 { rex |= 0x01; }
        if xmm >= 8 { rex |= 0x04; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0x6E);
        let modrm = 0xC0 | ((xmm & 7) << 3) | (r & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn movq_reg_xmm(&mut self, reg: Reg, xmm: u8) {
        let r = reg as u8;
        self.buf.emit_u8(0x66);
        let mut rex = 0x48;
        if r >= 8 { rex |= 0x01; }
        if xmm >= 8 { rex |= 0x04; }
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0x7E);
        let modrm = 0xC0 | ((xmm & 7) << 3) | (r & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn addss_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.buf.emit_u8(0xF3);
        let mut rex = 0;
        if dst >= 8 { rex |= 0x04; }
        if src >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0x58);
        let modrm = 0xC0 | ((dst & 7) << 3) | (src & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn addsd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.buf.emit_u8(0xF2);
        let mut rex = 0;
        if dst >= 8 { rex |= 0x04; }
        if src >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0x58);
        let modrm = 0xC0 | ((dst & 7) << 3) | (src & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn subss_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.buf.emit_u8(0xF3);
        let mut rex = 0;
        if dst >= 8 { rex |= 0x04; }
        if src >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0x5C);
        let modrm = 0xC0 | ((dst & 7) << 3) | (src & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn subsd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.buf.emit_u8(0xF2);
        let mut rex = 0;
        if dst >= 8 { rex |= 0x04; }
        if src >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0x5C);
        let modrm = 0xC0 | ((dst & 7) << 3) | (src & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn mulss_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.buf.emit_u8(0xF3);
        let mut rex = 0;
        if dst >= 8 { rex |= 0x04; }
        if src >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0x59);
        let modrm = 0xC0 | ((dst & 7) << 3) | (src & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn mulsd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.buf.emit_u8(0xF2);
        let mut rex = 0;
        if dst >= 8 { rex |= 0x04; }
        if src >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0x59);
        let modrm = 0xC0 | ((dst & 7) << 3) | (src & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn divss_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.buf.emit_u8(0xF3);
        let mut rex = 0;
        if dst >= 8 { rex |= 0x04; }
        if src >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0x5E);
        let modrm = 0xC0 | ((dst & 7) << 3) | (src & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn divsd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.buf.emit_u8(0xF2);
        let mut rex = 0;
        if dst >= 8 { rex |= 0x04; }
        if src >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0x5E);
        let modrm = 0xC0 | ((dst & 7) << 3) | (src & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn ucomiss_xmm_xmm(&mut self, dst: u8, src: u8) {
        let mut rex = 0;
        if dst >= 8 { rex |= 0x04; }
        if src >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0x2E);
        let modrm = 0xC0 | ((dst & 7) << 3) | (src & 7);
        self.buf.emit_u8(modrm);
    }

    pub fn ucomisd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.buf.emit_u8(0x66);
        let mut rex = 0;
        if dst >= 8 { rex |= 0x04; }
        if src >= 8 { rex |= 0x01; }
        if rex != 0 { self.buf.emit_u8(0x40 | rex); }
        self.buf.emit_u8(0x0F);
        self.buf.emit_u8(0x2E);
        let modrm = 0xC0 | ((dst & 7) << 3) | (src & 7);
        self.buf.emit_u8(modrm);
    }

    // Unsigned Division (DIV src)
    pub fn div_reg(&mut self, src: Reg) {
        // REX.W + F7 /6
        let src_code = src as u8;
        let mut rex = 0x48;
        if src_code >= 8 { rex |= 0x01; }
        
        self.buf.emit_u8(rex);
        self.buf.emit_u8(0xF7);
        let modrm = 0xC0 | (6 << 3) | (src_code & 7);
        self.buf.emit_u8(modrm);
    }
}
