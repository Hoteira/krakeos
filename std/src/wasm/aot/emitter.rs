use crate::alloc::vec::Vec;

#[repr(u8)]
#[derive(Copy, Clone, Debug)]
pub enum Reg {
    RAX = 0,
    RCX = 1,
    RDX = 2,
    RBX = 3,
    RSP = 4,
    RBP = 5,
    RSI = 6,
    RDI = 7,
    R8 = 8,
    R9 = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    R13 = 13,
    R14 = 14,
    R15 = 15,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug)]
pub enum XmmReg {
    XMM0 = 0,
    XMM1 = 1,
    XMM2 = 2,
    XMM3 = 3,
    XMM4 = 4,
    XMM5 = 5,
    XMM6 = 6,
    XMM7 = 7,
    XMM8 = 8,
    XMM9 = 9,
    XMM10 = 10,
    XMM11 = 11,
    XMM12 = 12,
    XMM13 = 13,
    XMM14 = 14,
    XMM15 = 15,
}

pub struct X64Emitter {
    pub code: Vec<u8>,
    pub label_offsets: Vec<Option<usize>>,
    pub relocs: Vec<Reloc>,
}

pub struct Reloc {
    pub pos: usize,
    pub label_id: usize,
    pub kind: RelocKind,
}

pub enum RelocKind {
    Jmp32,
    Jcc32(u8),
    Call32,
}

impl X64Emitter {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            label_offsets: Vec::new(),
            relocs: Vec::new(),
        }
    }

    pub fn emit_u8(&mut self, b: u8) {
        self.code.push(b);
    }
    pub fn emit_u32(&mut self, v: u32) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }
    pub fn emit_u64(&mut self, v: u64) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }

    pub fn rex(&mut self, w: bool, r: u8, x: u8, b: u8) {
        let mut prefix = 0x40;
        if w {
            prefix |= 0x08;
        }
        if r >= 8 {
            prefix |= 0x04;
        }
        if x >= 8 {
            prefix |= 0x02;
        }
        if b >= 8 {
            prefix |= 0x01;
        }
        if prefix != 0x40 || w {
            self.emit_u8(prefix);
        }
    }

    pub fn movq_xmm_reg(&mut self, dst: XmmReg, src: Reg) {
        self.emit_u8(0x66);
        self.rex(true, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x6E);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn shr_reg32_imm32(&mut self, reg: Reg, imm: u32) {
        self.rex(false, 0, 0, reg as u8);
        self.emit_u8(0xC1);
        self.modrm(3, 5, reg as u8);
        self.emit_u8(imm as u8);
    }

    pub fn modrm(&mut self, mode: u8, reg: u8, rm: u8) {
        self.emit_u8((mode << 6) | ((reg & 7) << 3) | (rm & 7));
    }

    pub fn new_label(&mut self) -> usize {
        let id = self.label_offsets.len();
        self.label_offsets.push(None);
        id
    }

    pub fn bind_label(&mut self, label_id: usize) {
        self.label_offsets[label_id] = Some(self.code.len());
    }

    pub fn jmp_label(&mut self, label_id: usize) {
        self.emit_u8(0xE9);
        let pos = self.code.len();
        self.emit_u32(0);
        self.relocs.push(Reloc {
            pos,
            label_id,
            kind: RelocKind::Jmp32,
        });
    }

    pub fn jcc_label(&mut self, cond: u8, label_id: usize) {
        self.emit_u8(0x0F);
        self.emit_u8(0x80 | (cond & 0x0F));
        let pos = self.code.len();
        self.emit_u32(0);
        self.relocs.push(Reloc {
            pos,
            label_id,
            kind: RelocKind::Jcc32(cond),
        });
    }

    pub fn finalize(&mut self) {
        let mut relocs = core::mem::take(&mut self.relocs);
        for reloc in relocs.drain(..) {
            let target = self.label_offsets[reloc.label_id].expect("Unbound label");
            let offset = (target as isize - (reloc.pos as isize + 4)) as i32;
            let bytes = offset.to_le_bytes();
            self.code[reloc.pos..reloc.pos + 4].copy_from_slice(&bytes);
        }
    }

    pub fn push_reg(&mut self, reg: Reg) {
        if (reg as u8) >= 8 {
            self.emit_u8(0x41);
        }
        self.emit_u8(0x50 + (reg as u8 & 7));
    }

    pub fn pop_reg(&mut self, reg: Reg) {
        if (reg as u8) >= 8 {
            self.emit_u8(0x41);
        }
        self.emit_u8(0x58 + (reg as u8 & 7));
    }

    pub fn mov_reg_reg(&mut self, dst: Reg, src: Reg) {
        self.rex(true, src as u8, 0, dst as u8);
        self.emit_u8(0x89);
        self.modrm(3, src as u8, dst as u8);
    }

    pub fn mov_reg32_reg32(&mut self, dst: Reg, src: Reg) {
        self.rex(false, src as u8, 0, dst as u8);
        self.emit_u8(0x89);
        self.modrm(3, src as u8, dst as u8);
    }

    pub fn mov_reg_imm64(&mut self, dst: Reg, imm: u64) {
        self.rex(true, 0, 0, dst as u8);
        self.emit_u8(0xB8 + (dst as u8 & 7));
        self.emit_u64(imm);
    }

    pub fn mov_reg_mem64(&mut self, dst: Reg, base: Reg, offset: i32) {
        self.rex(true, dst as u8, 0, base as u8);
        self.emit_u8(0x8B);
        self.emit_modrm_mem(dst, base, offset);
    }

    pub fn mov_mem64_reg(&mut self, base: Reg, offset: i32, src: Reg) {
        self.rex(true, src as u8, 0, base as u8);
        self.emit_u8(0x89);
        self.emit_modrm_mem(src, base, offset);
    }

    pub fn add_reg_reg(&mut self, dst: Reg, src: Reg) {
        self.rex(true, src as u8, 0, dst as u8);
        self.emit_u8(0x01);
        self.modrm(3, src as u8, dst as u8);
    }

    pub fn add_reg32_reg32(&mut self, dst: Reg, src: Reg) {
        self.rex(false, src as u8, 0, dst as u8);
        self.emit_u8(0x01);
        self.modrm(3, src as u8, dst as u8);
    }

    pub fn sub_reg_reg(&mut self, dst: Reg, src: Reg) {
        self.rex(true, src as u8, 0, dst as u8);
        self.emit_u8(0x29);
        self.modrm(3, src as u8, dst as u8);
    }

    pub fn sub_reg32_reg32(&mut self, dst: Reg, src: Reg) {
        self.rex(false, src as u8, 0, dst as u8);
        self.emit_u8(0x29);
        self.modrm(3, src as u8, dst as u8);
    }

    pub fn imul_reg_reg(&mut self, dst: Reg, src: Reg) {
        self.rex(true, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xAF);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn imul_reg32_reg32(&mut self, dst: Reg, src: Reg) {
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xAF);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn xor_reg_reg(&mut self, dst: Reg, src: Reg) {
        self.rex(true, src as u8, 0, dst as u8);
        self.emit_u8(0x31);
        self.modrm(3, src as u8, dst as u8);
    }

    pub fn xor_reg32_reg32(&mut self, dst: Reg, src: Reg) {
        self.rex(false, src as u8, 0, dst as u8);
        self.emit_u8(0x31);
        self.modrm(3, src as u8, dst as u8);
    }

    pub fn and_reg_reg(&mut self, dst: Reg, src: Reg) {
        self.rex(true, src as u8, 0, dst as u8);
        self.emit_u8(0x21);
        self.modrm(3, src as u8, dst as u8);
    }

    pub fn and_reg32_reg32(&mut self, dst: Reg, src: Reg) {
        self.rex(false, src as u8, 0, dst as u8);
        self.emit_u8(0x21);
        self.modrm(3, src as u8, dst as u8);
    }

    pub fn or_reg_reg(&mut self, dst: Reg, src: Reg) {
        self.rex(true, src as u8, 0, dst as u8);
        self.emit_u8(0x09);
        self.modrm(3, src as u8, dst as u8);
    }

    pub fn or_reg32_reg32(&mut self, dst: Reg, src: Reg) {
        self.rex(false, src as u8, 0, dst as u8);
        self.emit_u8(0x09);
        self.modrm(3, src as u8, dst as u8);
    }

    pub fn bsf_reg_reg(&mut self, dst: Reg, src: Reg) {
        self.rex(true, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xBC);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn bsf_reg32_reg32(&mut self, dst: Reg, src: Reg) {
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xBC);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn bsr_reg_reg(&mut self, dst: Reg, src: Reg) {
        self.rex(true, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xBD);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn bsr_reg32_reg32(&mut self, dst: Reg, src: Reg) {
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xBD);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn popcnt_reg_reg(&mut self, dst: Reg, src: Reg) {
        self.emit_u8(0xF3);
        self.rex(true, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xB8);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn popcnt_reg32_reg32(&mut self, dst: Reg, src: Reg) {
        self.emit_u8(0xF3);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xB8);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn cmp_reg_reg(&mut self, dst: Reg, src: Reg) {
        self.rex(true, src as u8, 0, dst as u8);
        self.emit_u8(0x39);
        self.modrm(3, src as u8, dst as u8);
    }

    pub fn cmp_mem32_imm32(&mut self, base: Reg, offset: i32, imm: u32) {
        self.rex(false, 0, 0, base as u8);
        self.emit_u8(0x81);
        self.emit_modrm_mem(Reg::RDI, base, offset); // Reg::RDI as 7 is used as extension for CMP
        self.emit_u32(imm);
    }

    pub fn cmp_reg32_reg32(&mut self, dst: Reg, src: Reg) {
        self.rex(false, src as u8, 0, dst as u8);
        self.emit_u8(0x39);
        self.modrm(3, src as u8, dst as u8);
    }

    pub fn test_reg_reg(&mut self, dst: Reg, src: Reg) {
        self.rex(true, src as u8, 0, dst as u8);
        self.emit_u8(0x85);
        self.modrm(3, src as u8, dst as u8);
    }

    pub fn test_reg32_reg32(&mut self, dst: Reg, src: Reg) {
        self.rex(false, src as u8, 0, dst as u8);
        self.emit_u8(0x85);
        self.modrm(3, src as u8, dst as u8);
    }

    pub fn test_reg_imm32(&mut self, reg: Reg, imm: u32) {
        self.rex(true, 0, 0, reg as u8);
        self.emit_u8(0xF7);
        self.modrm(3, 0, reg as u8);
        self.emit_u32(imm);
    }

    pub fn sub_reg_imm32(&mut self, reg: Reg, imm: u32) {
        self.rex(true, 0, 0, reg as u8);
        self.emit_u8(0x81);
        self.modrm(3, 5, reg as u8);
        self.emit_u32(imm);
    }

    pub fn sub_reg32_imm32(&mut self, reg: Reg, imm: u32) {
        self.rex(false, 0, 0, reg as u8);
        self.emit_u8(0x81);
        self.modrm(3, 5, reg as u8);
        self.emit_u32(imm);
    }

    pub fn add_reg_imm32(&mut self, reg: Reg, imm: u32) {
        self.rex(true, 0, 0, reg as u8);
        self.emit_u8(0x81);
        self.modrm(3, 0, reg as u8);
        self.emit_u32(imm);
    }

    pub fn add_reg32_imm32(&mut self, reg: Reg, imm: u32) {
        self.rex(false, 0, 0, reg as u8);
        self.emit_u8(0x81);
        self.modrm(3, 0, reg as u8);
        self.emit_u32(imm);
    }

    pub fn cmp_reg_imm32(&mut self, reg: Reg, imm: u32) {
        self.rex(true, 0, 0, reg as u8);
        self.emit_u8(0x81);
        self.modrm(3, 7, reg as u8);
        self.emit_u32(imm);
    }

    pub fn cmp_reg32_imm32(&mut self, reg: Reg, imm: u32) {
        self.rex(false, 0, 0, reg as u8);
        self.emit_u8(0x81);
        self.modrm(3, 7, reg as u8);
        self.emit_u32(imm);
    }

    pub fn ret(&mut self) {
        self.emit_u8(0xC3);
    }

    pub fn ud2(&mut self) {
        self.emit_u8(0x0F);
        self.emit_u8(0x0B);
    }

    pub fn call_reg(&mut self, reg: Reg) {
        if (reg as u8) >= 8 {
            self.emit_u8(0x41);
        }
        self.emit_u8(0xFF);
        self.modrm(3, 2, reg as u8);
    }

    pub fn push_wasm_stack(&mut self, reg: Reg) {
        self.sub_reg_imm32(Reg::RSP, 16);
        self.mov_mem64_reg(Reg::RSP, 0, reg);
        self.xor_reg_reg(Reg::R11, Reg::R11);
        self.mov_mem64_reg(Reg::RSP, 8, Reg::R11);
    }

    pub fn pop_wasm_stack(&mut self, reg: Reg) {
        self.mov_reg_mem64(reg, Reg::RSP, 0);
        self.add_reg_imm32(Reg::RSP, 16);
    }

    pub fn movups_xmm_mem(&mut self, dst: XmmReg, base: Reg, offset: i32) {
        self.rex(false, dst as u8, 0, base as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x10);
        self.emit_modrm_mem_reg_xmm(dst, base, offset);
    }

    pub fn movups_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x10);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn movups_mem_xmm(&mut self, base: Reg, offset: i32, src: XmmReg) {
        self.rex(false, src as u8, 0, base as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x11);
        self.emit_modrm_mem_reg_xmm(src, base, offset);
    }

    pub fn push_v128(&mut self, xmm: XmmReg) {
        self.sub_reg_imm32(Reg::RSP, 16);
        self.movups_mem_xmm(Reg::RSP, 0, xmm);
    }

    pub fn pop_v128(&mut self, xmm: XmmReg) {
        self.movups_xmm_mem(xmm, Reg::RSP, 0);
        self.add_reg_imm32(Reg::RSP, 16);
    }

    pub fn addss_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0xF3);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x58);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn subss_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0xF3);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x5C);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn mulss_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0xF3);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x59);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn divss_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0xF3);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x5E);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn sqrtss_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0xF3);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x51);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn andps_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x54);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn xorps_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x57);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn addsd_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0xF2);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x58);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn subsd_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0xF2);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x5C);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn mulsd_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0xF2);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x59);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn divsd_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0xF2);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x5E);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn sqrtsd_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0xF2);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x51);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn andpd_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0x66);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x54);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn orps_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x56);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn orpd_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0x66);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x56);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn xorpd_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0x66);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x57);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn ucomiss_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x2E);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn ucomisd_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0x66);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x2E);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn paddb_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0x66);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xFC);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn pcmpeqd_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0x66);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x76);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn pandn_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0x66);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xDF);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn pextrd_reg_xmm_imm8(&mut self, dst: Reg, src: XmmReg, imm: u8) {
        self.emit_u8(0x66);
        self.rex(false, src as u8, 0, dst as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x3A);
        self.emit_u8(0x16);
        self.modrm(3, src as u8, dst as u8);
        self.emit_u8(imm);
    }

    pub fn pinsrd_xmm_reg_imm8(&mut self, dst: XmmReg, src: Reg, imm: u8) {
        self.emit_u8(0x66);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x3A);
        self.emit_u8(0x22);
        self.modrm(3, dst as u8, src as u8);
        self.emit_u8(imm);
    }

    pub fn movd_xmm_reg(&mut self, dst: XmmReg, src: Reg) {
        self.emit_u8(0x66);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x6E);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn movd_reg_xmm(&mut self, dst: Reg, src: XmmReg) {
        self.emit_u8(0x66);
        self.rex(false, src as u8, 0, dst as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x7E);
        self.modrm(3, src as u8, dst as u8);
    }

    pub fn movq_reg_xmm(&mut self, dst: Reg, src: XmmReg) {
        self.emit_u8(0x66);
        self.rex(true, src as u8, 0, dst as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x7E);
        self.modrm(3, src as u8, dst as u8);
    }

    pub fn cvtsi2ss_xmm_reg(&mut self, dst: XmmReg, src: Reg) {
        self.emit_u8(0xF3);
        self.rex(true, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x2A);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn cvtsi2sd_xmm_reg(&mut self, dst: XmmReg, src: Reg) {
        self.emit_u8(0xF2);
        self.rex(true, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x2A);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn cvttss2si_reg_xmm(&mut self, dst: Reg, src: XmmReg) {
        self.emit_u8(0xF3);
        self.rex(true, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x2C);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn cvttss2si_reg32_xmm(&mut self, dst: Reg, src: XmmReg) {
        self.emit_u8(0xF3);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x2C);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn cvttsd2si_reg_xmm(&mut self, dst: Reg, src: XmmReg) {
        self.emit_u8(0xF2);
        self.rex(true, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x2C);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn cvttsd2si_reg32_xmm(&mut self, dst: Reg, src: XmmReg) {
        self.emit_u8(0xF2);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x2C);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn roundss_xmm_xmm_imm8(&mut self, dst: XmmReg, src: XmmReg, imm: u8) {
        self.emit_u8(0x66);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x3A);
        self.emit_u8(0x0A);
        self.modrm(3, dst as u8, src as u8);
        self.emit_u8(imm);
    }

    pub fn roundsd_xmm_xmm_imm8(&mut self, dst: XmmReg, src: XmmReg, imm: u8) {
        self.emit_u8(0x66);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x3A);
        self.emit_u8(0x0B);
        self.modrm(3, dst as u8, src as u8);
        self.emit_u8(imm);
    }

    pub fn cvtsd2ss_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0xF2);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x5A);
        self.modrm(3, dst as u8, src as u8);
    }
    pub fn cvtss2sd_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0xF3);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x5A);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn ldmxcsr_mem(&mut self, base: Reg, offset: i32) {
        self.emit_u8(0x0F);
        self.emit_u8(0xAE);
        self.emit_modrm_mem_with_reg(2, base, offset);
    }

    pub fn emit_modrm_mem_with_reg(&mut self, reg: u8, base: Reg, offset: i32) {
        if offset == 0 && (base as u8 & 7) != 5 && (base as u8 & 7) != 4 {
            self.modrm(0, reg, base as u8);
        } else if offset >= -128 && offset <= 127 {
            self.modrm(1, reg, base as u8);
            if (base as u8 & 7) == 4 {
                self.emit_u8(0x24);
            }
            self.emit_u8(offset as u8);
        } else {
            self.modrm(2, reg, base as u8);
            if (base as u8 & 7) == 4 {
                self.emit_u8(0x24);
            }
            self.emit_u32(offset as u32);
        }
    }

    pub fn movsxd_reg_reg(&mut self, dst: Reg, src: Reg) {
        self.rex(true, dst as u8, 0, src as u8);
        self.emit_u8(0x63);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn movsx_reg_reg8(&mut self, dst: Reg, src: Reg) {
        self.rex(true, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xBE);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn movsx_reg_reg16(&mut self, dst: Reg, src: Reg) {
        self.rex(true, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xBF);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn movzx_reg_reg8(&mut self, dst: Reg, src: Reg) {
        self.rex(true, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xB6);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn movzx_reg_reg16(&mut self, dst: Reg, src: Reg) {
        self.rex(true, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xB7);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn movzx_reg_mem8(&mut self, dst: Reg, base: Reg, offset: i32) {
        self.rex(true, dst as u8, 0, base as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xB6);
        self.emit_modrm_mem(dst, base, offset);
    }
    pub fn movsx_reg_mem8(&mut self, dst: Reg, base: Reg, offset: i32) {
        self.rex(true, dst as u8, 0, base as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xBE);
        self.emit_modrm_mem(dst, base, offset);
    }
    pub fn movzx_reg_mem16(&mut self, dst: Reg, base: Reg, offset: i32) {
        self.rex(true, dst as u8, 0, base as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xB7);
        self.emit_modrm_mem(dst, base, offset);
    }
    pub fn movsx_reg_mem16(&mut self, dst: Reg, base: Reg, offset: i32) {
        self.rex(true, dst as u8, 0, base as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xBF);
        self.emit_modrm_mem(dst, base, offset);
    }

    pub fn lock_prefix(&mut self) {
        self.emit_u8(0xF0);
    }

    pub fn xadd_mem_reg(&mut self, base: Reg, offset: i32, src: Reg, is_64: bool) {
        self.lock_prefix();
        self.rex(is_64, src as u8, 0, base as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xC1);
        self.emit_modrm_mem(src, base, offset);
    }

    pub fn xadd_mem_reg8(&mut self, base: Reg, offset: i32, src: Reg) {
        self.lock_prefix();
        self.rex(false, src as u8, 0, base as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xC0);
        self.emit_modrm_mem(src, base, offset);
    }

    pub fn xadd_mem_reg16(&mut self, base: Reg, offset: i32, src: Reg) {
        self.lock_prefix();
        self.emit_u8(0x66);
        self.rex(false, src as u8, 0, base as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xC1);
        self.emit_modrm_mem(src, base, offset);
    }

    pub fn cmpxchg_mem_reg(&mut self, base: Reg, offset: i32, src: Reg, is_64: bool) {
        self.lock_prefix();
        self.rex(is_64, src as u8, 0, base as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xB1);
        self.emit_modrm_mem(src, base, offset);
    }

    pub fn cmpxchg_mem_reg8(&mut self, base: Reg, offset: i32, src: Reg) {
        self.lock_prefix();
        self.rex(false, src as u8, 0, base as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xB0);
        self.emit_modrm_mem(src, base, offset);
    }

    pub fn cmpxchg_mem_reg16(&mut self, base: Reg, offset: i32, src: Reg) {
        self.lock_prefix();
        self.emit_u8(0x66);
        self.rex(false, src as u8, 0, base as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xB1);
        self.emit_modrm_mem(src, base, offset);
    }

    pub fn shl_reg_cl(&mut self, reg: Reg) {
        self.rex(true, 0, 0, reg as u8);
        self.emit_u8(0xD3);
        self.modrm(3, 4, reg as u8);
    }
    pub fn shl_reg32_cl(&mut self, reg: Reg) {
        self.rex(false, 0, 0, reg as u8);
        self.emit_u8(0xD3);
        self.modrm(3, 4, reg as u8);
    }

    pub fn shr_reg_cl(&mut self, reg: Reg) {
        self.rex(true, 0, 0, reg as u8);
        self.emit_u8(0xD3);
        self.modrm(3, 5, reg as u8);
    }
    pub fn shr_reg32_cl(&mut self, reg: Reg) {
        self.rex(false, 0, 0, reg as u8);
        self.emit_u8(0xD3);
        self.modrm(3, 5, reg as u8);
    }

    pub fn sar_reg_cl(&mut self, reg: Reg) {
        self.rex(true, 0, 0, reg as u8);
        self.emit_u8(0xD3);
        self.modrm(3, 7, reg as u8);
    }
    pub fn sar_reg32_cl(&mut self, reg: Reg) {
        self.rex(false, 0, 0, reg as u8);
        self.emit_u8(0xD3);
        self.modrm(3, 7, reg as u8);
    }

    pub fn rol_reg_cl(&mut self, reg: Reg) {
        self.rex(true, 0, 0, reg as u8);
        self.emit_u8(0xD3);
        self.modrm(3, 0, reg as u8);
    }
    pub fn rol_reg32_cl(&mut self, reg: Reg) {
        self.rex(false, 0, 0, reg as u8);
        self.emit_u8(0xD3);
        self.modrm(3, 0, reg as u8);
    }

    pub fn ror_reg_cl(&mut self, reg: Reg) {
        self.rex(true, 0, 0, reg as u8);
        self.emit_u8(0xD3);
        self.modrm(3, 1, reg as u8);
    }
    pub fn ror_reg32_cl(&mut self, reg: Reg) {
        self.rex(false, 0, 0, reg as u8);
        self.emit_u8(0xD3);
        self.modrm(3, 1, reg as u8);
    }

    pub fn emit_modrm_mem(&mut self, reg: Reg, base: Reg, offset: i32) {
        if offset == 0 && (base as u8 & 7) != 5 && (base as u8 & 7) != 4 {
            self.modrm(0, reg as u8, base as u8);
        } else if offset >= -128 && offset <= 127 {
            self.modrm(1, reg as u8, base as u8);
            if (base as u8 & 7) == 4 {
                self.emit_u8(0x24);
            }
            self.emit_u8(offset as u8);
        } else {
            self.modrm(2, reg as u8, base as u8);
            if (base as u8 & 7) == 4 {
                self.emit_u8(0x24);
            }
            self.emit_u32(offset as u32);
        }
    }

    fn emit_modrm_mem_reg_xmm(&mut self, reg: XmmReg, base: Reg, offset: i32) {
        if offset == 0 && (base as u8 & 7) != 5 && (base as u8 & 7) != 4 {
            self.modrm(0, reg as u8, base as u8);
        } else if offset >= -128 && offset <= 127 {
            self.modrm(1, reg as u8, base as u8);
            if (base as u8 & 7) == 4 {
                self.emit_u8(0x24);
            }
            self.emit_u8(offset as u8);
        } else {
            self.modrm(2, reg as u8, base as u8);
            if (base as u8 & 7) == 4 {
                self.emit_u8(0x24);
            }
            self.emit_u32(offset as u32);
        }
    }

    pub fn pextrw_reg_xmm_imm8(&mut self, dst: Reg, src: XmmReg, imm: u8) {
        self.emit_u8(0x66);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xC5);
        self.modrm(3, dst as u8, src as u8);
        self.emit_u8(imm);
    }

    pub fn pextrq_reg_xmm_imm8(&mut self, dst: Reg, src: XmmReg, imm: u8) {
        self.emit_u8(0x66);
        self.rex(true, src as u8, 0, dst as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x3A);
        self.emit_u8(0x16);
        self.modrm(3, src as u8, dst as u8);
        self.emit_u8(imm);
    }

    pub fn pinsrb_xmm_reg_imm8(&mut self, dst: XmmReg, src: Reg, imm: u8) {
        self.emit_u8(0x66);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x3A);
        self.emit_u8(0x20);
        self.modrm(3, dst as u8, src as u8);
        self.emit_u8(imm);
    }

    pub fn pinsrw_xmm_reg_imm8(&mut self, dst: XmmReg, src: Reg, imm: u8) {
        self.emit_u8(0x66);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0xC4);
        self.modrm(3, dst as u8, src as u8);
        self.emit_u8(imm);
    }

    pub fn pinsrq_xmm_reg_imm8(&mut self, dst: XmmReg, src: Reg, imm: u8) {
        self.emit_u8(0x66);
        self.rex(true, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x3A);
        self.emit_u8(0x22);
        self.modrm(3, dst as u8, src as u8);
        self.emit_u8(imm);
    }

    pub fn punpcklbw_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0x66);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x60);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn punpcklwd_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0x66);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x61);
        self.modrm(3, dst as u8, src as u8);
    }

    pub fn movddup_xmm_xmm(&mut self, dst: XmmReg, src: XmmReg) {
        self.emit_u8(0xF2);
        self.rex(false, dst as u8, 0, src as u8);
        self.emit_u8(0x0F);
        self.emit_u8(0x12);
        self.modrm(3, dst as u8, src as u8);
    }
}
