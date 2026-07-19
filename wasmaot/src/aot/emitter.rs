//! RV64GC machine-code emitter — the RISC-V counterpart of the x86_64
//! `X64Emitter` in ref/std/src/wasm/aot/emitter.rs. Same label/reloc design;
//! only uncompressed 4-byte encodings are emitted.
//!
//! Label control flow always uses the far form (`auipc+jalr`, ±2GiB) because
//! trap handlers live at the start of the module while functions can sit
//! megabytes in. Conditional branches emit an inverted short branch over the
//! far jump. T6 is reserved as the reloc scratch register.

use alloc::vec::Vec;

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Reg {
    Zero = 0,
    Ra = 1,
    Sp = 2,
    Gp = 3,
    Tp = 4,
    T0 = 5,
    T1 = 6,
    T2 = 7,
    S0 = 8,
    S1 = 9,
    A0 = 10,
    A1 = 11,
    A2 = 12,
    A3 = 13,
    A4 = 14,
    A5 = 15,
    A6 = 16,
    A7 = 17,
    S2 = 18,
    S3 = 19,
    S4 = 20,
    S5 = 21,
    S6 = 22,
    S7 = 23,
    S8 = 24,
    S9 = 25,
    S10 = 26,
    S11 = 27,
    T3 = 28,
    T4 = 29,
    T5 = 30,
    T6 = 31,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FReg {
    Ft0 = 0,
    Ft1 = 1,
    Ft2 = 2,
    Ft3 = 3,
    Ft4 = 4,
    Ft5 = 5,
    Ft6 = 6,
    Ft7 = 7,
    Fs0 = 8,
    Fs1 = 9,
    Fa0 = 10,
    Fa1 = 11,
    Fa2 = 12,
    Fa3 = 13,
    Fa4 = 14,
    Fa5 = 15,
    Fa6 = 16,
    Fa7 = 17,
    Fs2 = 18,
    Fs3 = 19,
    Fs4 = 20,
    Fs5 = 21,
    Fs6 = 22,
    Fs7 = 23,
    Fs8 = 24,
    Fs9 = 25,
    Fs10 = 26,
    Fs11 = 27,
    Ft8 = 28,
    Ft9 = 29,
    Ft10 = 30,
    Ft11 = 31,
}

/// Branch condition funct3 codes (branch taken when condition holds).
pub mod cond {
    pub const EQ: u8 = 0b000;
    pub const NE: u8 = 0b001;
    pub const LT: u8 = 0b100;
    pub const GE: u8 = 0b101;
    pub const LTU: u8 = 0b110;
    pub const GEU: u8 = 0b111;

    pub fn invert(c: u8) -> u8 {
        c ^ 1
    }
}

pub enum RelocKind {
    /// Unconditional jump / call (2 words). Resolves to a direct `jal`
    /// (±1 MiB, TCG-chainable) or `auipc+jalr` when far.
    Jump { link: u8 },
    /// Conditional branch (3 words). Resolves to a direct `bcc` (±4 KiB) or
    /// `inverted-bcc over auipc+jalr` when far.
    Cond { c: u8, rs1: u8, rs2: u8 },
}

pub struct Reloc {
    /// Offset of the first placeholder word.
    pub pos: usize,
    pub label_id: usize,
    pub kind: RelocKind,
}

pub struct Rv64Emitter {
    pub code: Vec<u8>,
    pub label_offsets: Vec<Option<usize>>,
    pub relocs: Vec<Reloc>,
}

impl Rv64Emitter {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            label_offsets: Vec::new(),
            relocs: Vec::new(),
        }
    }

    #[inline]
    pub fn emit_u32(&mut self, v: u32) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }

    // ── raw encoders ────────────────────────────────────────────────

    #[inline]
    fn enc_r(&mut self, opcode: u32, rd: u8, funct3: u32, rs1: u8, rs2: u8, funct7: u32) {
        self.emit_u32(
            (funct7 << 25)
                | ((rs2 as u32) << 20)
                | ((rs1 as u32) << 15)
                | (funct3 << 12)
                | ((rd as u32) << 7)
                | opcode,
        );
    }

    #[inline]
    fn enc_i(&mut self, opcode: u32, rd: u8, funct3: u32, rs1: u8, imm: i32) {
        debug_assert!((-2048..=2047).contains(&imm) || funct3 == 0b001 || funct3 == 0b101);
        self.emit_u32(
            (((imm as u32) & 0xFFF) << 20)
                | ((rs1 as u32) << 15)
                | (funct3 << 12)
                | ((rd as u32) << 7)
                | opcode,
        );
    }

    #[inline]
    fn enc_s(&mut self, opcode: u32, funct3: u32, rs1: u8, rs2: u8, imm: i32) {
        debug_assert!((-2048..=2047).contains(&imm));
        let imm = imm as u32;
        self.emit_u32(
            (((imm >> 5) & 0x7F) << 25)
                | ((rs2 as u32) << 20)
                | ((rs1 as u32) << 15)
                | (funct3 << 12)
                | ((imm & 0x1F) << 7)
                | opcode,
        );
    }

    #[inline]
    fn enc_b(&mut self, funct3: u32, rs1: u8, rs2: u8, imm: i32) {
        debug_assert!(imm % 2 == 0 && (-4096..=4094).contains(&imm));
        let imm = imm as u32;
        self.emit_u32(
            (((imm >> 12) & 1) << 31)
                | (((imm >> 5) & 0x3F) << 25)
                | ((rs2 as u32) << 20)
                | ((rs1 as u32) << 15)
                | (funct3 << 12)
                | (((imm >> 1) & 0xF) << 8)
                | (((imm >> 11) & 1) << 7)
                | 0b1100011,
        );
    }

    #[inline]
    fn enc_u(&mut self, opcode: u32, rd: u8, imm20: u32) {
        self.emit_u32((imm20 << 12) | ((rd as u32) << 7) | opcode);
    }

    /// J-type (jal). imm is a byte offset, must be even, ±1 MiB.
    fn enc_j_at(&mut self, at: usize, rd: u8, imm: i32) {
        let u = imm as u32;
        let word = (((u >> 20) & 1) << 31)
            | (((u >> 1) & 0x3FF) << 21)
            | (((u >> 11) & 1) << 20)
            | (((u >> 12) & 0xFF) << 12)
            | ((rd as u32) << 7)
            | 0b1101111;
        self.code[at..at + 4].copy_from_slice(&word.to_le_bytes());
    }

    fn enc_b_at(&mut self, at: usize, funct3: u32, rs1: u8, rs2: u8, imm: i32) {
        let imm = imm as u32;
        let word = (((imm >> 12) & 1) << 31)
            | (((imm >> 5) & 0x3F) << 25)
            | ((rs2 as u32) << 20)
            | ((rs1 as u32) << 15)
            | (funct3 << 12)
            | (((imm >> 1) & 0xF) << 8)
            | (((imm >> 11) & 1) << 7)
            | 0b1100011;
        self.code[at..at + 4].copy_from_slice(&word.to_le_bytes());
    }

    fn word_at(&mut self, at: usize, word: u32) {
        self.code[at..at + 4].copy_from_slice(&word.to_le_bytes());
    }

    const NOP: u32 = 0x0000_0013; // addi x0, x0, 0

    // ── integer ALU ─────────────────────────────────────────────────

    pub fn add(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0110011, rd as u8, 0b000, rs1 as u8, rs2 as u8, 0); }
    pub fn sub(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0110011, rd as u8, 0b000, rs1 as u8, rs2 as u8, 0b0100000); }
    pub fn addw(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0111011, rd as u8, 0b000, rs1 as u8, rs2 as u8, 0); }
    pub fn subw(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0111011, rd as u8, 0b000, rs1 as u8, rs2 as u8, 0b0100000); }
    pub fn and(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0110011, rd as u8, 0b111, rs1 as u8, rs2 as u8, 0); }
    pub fn or(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0110011, rd as u8, 0b110, rs1 as u8, rs2 as u8, 0); }
    pub fn xor(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0110011, rd as u8, 0b100, rs1 as u8, rs2 as u8, 0); }
    pub fn sll(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0110011, rd as u8, 0b001, rs1 as u8, rs2 as u8, 0); }
    pub fn srl(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0110011, rd as u8, 0b101, rs1 as u8, rs2 as u8, 0); }
    pub fn sra(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0110011, rd as u8, 0b101, rs1 as u8, rs2 as u8, 0b0100000); }
    pub fn sllw(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0111011, rd as u8, 0b001, rs1 as u8, rs2 as u8, 0); }
    pub fn srlw(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0111011, rd as u8, 0b101, rs1 as u8, rs2 as u8, 0); }
    pub fn sraw(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0111011, rd as u8, 0b101, rs1 as u8, rs2 as u8, 0b0100000); }
    pub fn slt(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0110011, rd as u8, 0b010, rs1 as u8, rs2 as u8, 0); }
    pub fn sltu(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0110011, rd as u8, 0b011, rs1 as u8, rs2 as u8, 0); }

    pub fn addi(&mut self, rd: Reg, rs1: Reg, imm: i32) { self.enc_i(0b0010011, rd as u8, 0b000, rs1 as u8, imm); }
    pub fn addiw(&mut self, rd: Reg, rs1: Reg, imm: i32) { self.enc_i(0b0011011, rd as u8, 0b000, rs1 as u8, imm); }
    pub fn andi(&mut self, rd: Reg, rs1: Reg, imm: i32) { self.enc_i(0b0010011, rd as u8, 0b111, rs1 as u8, imm); }
    pub fn ori(&mut self, rd: Reg, rs1: Reg, imm: i32) { self.enc_i(0b0010011, rd as u8, 0b110, rs1 as u8, imm); }
    pub fn xori(&mut self, rd: Reg, rs1: Reg, imm: i32) { self.enc_i(0b0010011, rd as u8, 0b100, rs1 as u8, imm); }
    pub fn slti(&mut self, rd: Reg, rs1: Reg, imm: i32) { self.enc_i(0b0010011, rd as u8, 0b010, rs1 as u8, imm); }
    pub fn sltiu(&mut self, rd: Reg, rs1: Reg, imm: i32) { self.enc_i(0b0010011, rd as u8, 0b011, rs1 as u8, imm); }
    pub fn slli(&mut self, rd: Reg, rs1: Reg, sh: u32) { self.enc_i(0b0010011, rd as u8, 0b001, rs1 as u8, sh as i32); }
    pub fn srli(&mut self, rd: Reg, rs1: Reg, sh: u32) { self.enc_i(0b0010011, rd as u8, 0b101, rs1 as u8, sh as i32); }
    pub fn srai(&mut self, rd: Reg, rs1: Reg, sh: u32) { self.enc_i(0b0010011, rd as u8, 0b101, rs1 as u8, (sh | 0x400) as i32); }
    pub fn slliw(&mut self, rd: Reg, rs1: Reg, sh: u32) { self.enc_i(0b0011011, rd as u8, 0b001, rs1 as u8, sh as i32); }
    pub fn srliw(&mut self, rd: Reg, rs1: Reg, sh: u32) { self.enc_i(0b0011011, rd as u8, 0b101, rs1 as u8, sh as i32); }
    pub fn sraiw(&mut self, rd: Reg, rs1: Reg, sh: u32) { self.enc_i(0b0011011, rd as u8, 0b101, rs1 as u8, (sh | 0x400) as i32); }

    pub fn lui(&mut self, rd: Reg, imm20: u32) { self.enc_u(0b0110111, rd as u8, imm20 & 0xFFFFF); }
    pub fn auipc(&mut self, rd: Reg, imm20: u32) { self.enc_u(0b0010111, rd as u8, imm20 & 0xFFFFF); }

    // ── M extension ─────────────────────────────────────────────────

    pub fn mul(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0110011, rd as u8, 0b000, rs1 as u8, rs2 as u8, 1); }
    pub fn mulw(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0111011, rd as u8, 0b000, rs1 as u8, rs2 as u8, 1); }
    pub fn div(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0110011, rd as u8, 0b100, rs1 as u8, rs2 as u8, 1); }
    pub fn divu(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0110011, rd as u8, 0b101, rs1 as u8, rs2 as u8, 1); }
    pub fn rem(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0110011, rd as u8, 0b110, rs1 as u8, rs2 as u8, 1); }
    pub fn remu(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0110011, rd as u8, 0b111, rs1 as u8, rs2 as u8, 1); }
    pub fn divw(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0111011, rd as u8, 0b100, rs1 as u8, rs2 as u8, 1); }
    pub fn divuw(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0111011, rd as u8, 0b101, rs1 as u8, rs2 as u8, 1); }
    pub fn remw(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0111011, rd as u8, 0b110, rs1 as u8, rs2 as u8, 1); }
    pub fn remuw(&mut self, rd: Reg, rs1: Reg, rs2: Reg) { self.enc_r(0b0111011, rd as u8, 0b111, rs1 as u8, rs2 as u8, 1); }

    // ── loads/stores ────────────────────────────────────────────────

    pub fn lb(&mut self, rd: Reg, base: Reg, off: i32) { self.enc_i(0b0000011, rd as u8, 0b000, base as u8, off); }
    pub fn lh(&mut self, rd: Reg, base: Reg, off: i32) { self.enc_i(0b0000011, rd as u8, 0b001, base as u8, off); }
    pub fn lw(&mut self, rd: Reg, base: Reg, off: i32) { self.enc_i(0b0000011, rd as u8, 0b010, base as u8, off); }
    pub fn ld(&mut self, rd: Reg, base: Reg, off: i32) { self.enc_i(0b0000011, rd as u8, 0b011, base as u8, off); }
    pub fn lbu(&mut self, rd: Reg, base: Reg, off: i32) { self.enc_i(0b0000011, rd as u8, 0b100, base as u8, off); }
    pub fn lhu(&mut self, rd: Reg, base: Reg, off: i32) { self.enc_i(0b0000011, rd as u8, 0b101, base as u8, off); }
    pub fn lwu(&mut self, rd: Reg, base: Reg, off: i32) { self.enc_i(0b0000011, rd as u8, 0b110, base as u8, off); }
    pub fn sb(&mut self, base: Reg, off: i32, src: Reg) { self.enc_s(0b0100011, 0b000, base as u8, src as u8, off); }
    pub fn sh(&mut self, base: Reg, off: i32, src: Reg) { self.enc_s(0b0100011, 0b001, base as u8, src as u8, off); }
    pub fn sw(&mut self, base: Reg, off: i32, src: Reg) { self.enc_s(0b0100011, 0b010, base as u8, src as u8, off); }
    pub fn sd(&mut self, base: Reg, off: i32, src: Reg) { self.enc_s(0b0100011, 0b011, base as u8, src as u8, off); }

    // ── jumps / branches ────────────────────────────────────────────

    pub fn jalr(&mut self, rd: Reg, base: Reg, off: i32) { self.enc_i(0b1100111, rd as u8, 0b000, base as u8, off); }

    pub fn ret(&mut self) { self.jalr(Reg::Zero, Reg::Ra, 0); }

    /// Raw short conditional branch with a fixed byte offset.
    pub fn bcc(&mut self, c: u8, rs1: Reg, rs2: Reg, off: i32) { self.enc_b(c as u32, rs1 as u8, rs2 as u8, off); }

    pub fn ebreak(&mut self) { self.emit_u32(0x0010_0073); }

    /// csrrwi rd, csr, uimm5 (rd = old csr, csr = uimm)
    pub fn csrrwi(&mut self, rd: Reg, csr: u32, uimm: u32) {
        self.emit_u32((csr << 20) | ((uimm & 0x1F) << 15) | (0b101 << 12) | ((rd as u32) << 7) | 0b1110011);
    }

    /// csrrs rd, csr, rs1 (rd = csr, csr |= rs1; rs1=x0 is a pure read)
    pub fn csrrs(&mut self, rd: Reg, csr: u32, rs1: Reg) {
        self.emit_u32((csr << 20) | ((rs1 as u32) << 15) | (0b010 << 12) | ((rd as u32) << 7) | 0b1110011);
    }

    // ── labels + relocs (far form: auipc+jalr via T6) ───────────────

    pub fn new_label(&mut self) -> usize {
        let id = self.label_offsets.len();
        self.label_offsets.push(None);
        id
    }

    pub fn bind_label(&mut self, label_id: usize) {
        self.label_offsets[label_id] = Some(self.code.len());
    }

    /// Unconditional jump to label (2 words reserved).
    pub fn jmp_label(&mut self, label_id: usize) {
        let pos = self.code.len();
        self.emit_u32(Self::NOP);
        self.emit_u32(Self::NOP);
        self.relocs.push(Reloc { pos, label_id, kind: RelocKind::Jump { link: Reg::Zero as u8 } });
    }

    /// Call a label, link in RA (2 words reserved).
    pub fn call_label(&mut self, label_id: usize) {
        let pos = self.code.len();
        self.emit_u32(Self::NOP);
        self.emit_u32(Self::NOP);
        self.relocs.push(Reloc { pos, label_id, kind: RelocKind::Jump { link: Reg::Ra as u8 } });
    }

    /// Branch to `label` when `rs1 <cond> rs2` holds (3 words reserved).
    pub fn bcc_label(&mut self, c: u8, rs1: Reg, rs2: Reg, label_id: usize) {
        let pos = self.code.len();
        self.emit_u32(Self::NOP);
        self.emit_u32(Self::NOP);
        self.emit_u32(Self::NOP);
        self.relocs.push(Reloc {
            pos,
            label_id,
            kind: RelocKind::Cond { c, rs1: rs1 as u8, rs2: rs2 as u8 },
        });
    }

    pub fn beqz_label(&mut self, rs: Reg, label_id: usize) { self.bcc_label(cond::EQ, rs, Reg::Zero, label_id); }
    pub fn bnez_label(&mut self, rs: Reg, label_id: usize) { self.bcc_label(cond::NE, rs, Reg::Zero, label_id); }

    /// Call through a register.
    pub fn call_reg(&mut self, r: Reg) {
        self.jalr(Reg::Ra, r, 0);
    }

    /// Jump through a register (no link).
    pub fn jmp_reg(&mut self, r: Reg) {
        self.jalr(Reg::Zero, r, 0);
    }

    pub fn finalize(&mut self) {
        // The auipc+jalr scratch register (must not be a value/context reg).
        const T6: u8 = Reg::T6 as u8;
        let relocs = core::mem::take(&mut self.relocs);
        for reloc in &relocs {
            let target = self.label_offsets[reloc.label_id].expect("unbound label at finalize");
            let pos = reloc.pos;
            match reloc.kind {
                RelocKind::Jump { link } => {
                    let delta = target as i64 - pos as i64;
                    if (-1_048_576..=1_048_574).contains(&delta) {
                        // Direct jal (TCG-chainable) + nop filler.
                        self.enc_j_at(pos, link, delta as i32);
                        self.word_at(pos + 4, Self::NOP);
                    } else {
                        let hi = ((delta + 0x800) >> 12) as i32;
                        let lo = (delta - ((hi as i64) << 12)) as i32;
                        // auipc t6, hi ; jalr link, t6, lo
                        self.word_at(pos, (((hi as u32) & 0xFFFFF) << 12) | ((T6 as u32) << 7) | 0b0010111);
                        self.word_at(
                            pos + 4,
                            (((lo as u32) & 0xFFF) << 20) | ((T6 as u32) << 15) | ((link as u32) << 7) | 0b1100111,
                        );
                    }
                }
                RelocKind::Cond { c, rs1, rs2 } => {
                    let delta = target as i64 - pos as i64;
                    if (-4096..=4094).contains(&delta) {
                        // Direct branch to target + 2 nop fillers.
                        self.enc_b_at(pos, c as u32, rs1, rs2, delta as i32);
                        self.word_at(pos + 4, Self::NOP);
                        self.word_at(pos + 8, Self::NOP);
                    } else {
                        // Inverted branch over an auipc+jalr far jump.
                        self.enc_b_at(pos, cond::invert(c) as u32, rs1, rs2, 12);
                        let jdelta = target as i64 - (pos as i64 + 4);
                        let hi = ((jdelta + 0x800) >> 12) as i32;
                        let lo = (jdelta - ((hi as i64) << 12)) as i32;
                        self.word_at(pos + 4, (((hi as u32) & 0xFFFFF) << 12) | ((T6 as u32) << 7) | 0b0010111);
                        self.word_at(
                            pos + 8,
                            (((lo as u32) & 0xFFF) << 20) | ((T6 as u32) << 15) | (0 << 7) | 0b1100111,
                        );
                    }
                }
            }
        }
    }

    // ── pseudo-ops ──────────────────────────────────────────────────

    pub fn mv(&mut self, rd: Reg, rs: Reg) { self.addi(rd, rs, 0); }
    pub fn neg(&mut self, rd: Reg, rs: Reg) { self.sub(rd, Reg::Zero, rs); }
    pub fn negw(&mut self, rd: Reg, rs: Reg) { self.subw(rd, Reg::Zero, rs); }
    pub fn not(&mut self, rd: Reg, rs: Reg) { self.xori(rd, rs, -1); }

    /// Sign-extend the low 32 bits of rs into rd.
    pub fn sext_w(&mut self, rd: Reg, rs: Reg) { self.addiw(rd, rs, 0); }

    /// Zero-extend the low 32 bits of rs into rd.
    pub fn zext_w(&mut self, rd: Reg, rs: Reg) {
        self.slli(rd, rs, 32);
        self.srli(rd, rd, 32);
    }

    /// rd = (rs != 0) ? 1 : 0
    pub fn snez(&mut self, rd: Reg, rs: Reg) { self.sltu(rd, Reg::Zero, rs); }

    /// rd = (rs == 0) ? 1 : 0
    pub fn seqz(&mut self, rd: Reg, rs: Reg) { self.sltiu(rd, rs, 1); }

    /// Materialize an arbitrary 64-bit constant.
    pub fn li(&mut self, rd: Reg, imm: i64) {
        if (-2048..=2047).contains(&imm) {
            self.addi(rd, Reg::Zero, imm as i32);
            return;
        }
        if imm >= i32::MIN as i64 && imm <= i32::MAX as i64 {
            let imm32 = imm as i32;
            let hi = ((imm32 as i64 + 0x800) >> 12) as i32; // upper 20 with rounding
            let lo = imm32.wrapping_sub(hi << 12);
            self.lui(rd, (hi as u32) & 0xFFFFF);
            if lo != 0 {
                self.addiw(rd, rd, lo);
            }
            return;
        }
        // General 64-bit: build the high part, shift, add the low 12 bits.
        let lo12 = ((imm << 52) >> 52) as i32; // sign-extended low 12
        let hi = (imm - lo12 as i64) >> 12;
        self.li(rd, hi);
        self.slli(rd, rd, 12);
        if lo12 != 0 {
            self.addi(rd, rd, lo12);
        }
    }

    /// rd = rs + imm for any 64-bit imm (uses T6 when out of i12 range;
    /// rs must not be T6 in that case).
    pub fn addi_any(&mut self, rd: Reg, rs: Reg, imm: i64) {
        if (-2048..=2047).contains(&imm) {
            if imm != 0 || rd != rs {
                self.addi(rd, rs, imm as i32);
            }
        } else {
            debug_assert!(rs != Reg::T6);
            self.li(Reg::T6, imm);
            self.add(rd, rs, Reg::T6);
        }
    }

    /// Load from base+off for any 64-bit off (uses T6 when needed).
    pub fn ld_any(&mut self, rd: Reg, base: Reg, off: i64) {
        if (-2048..=2047).contains(&off) {
            self.ld(rd, base, off as i32);
        } else {
            self.addi_any(Reg::T6, base, off);
            self.ld(rd, Reg::T6, 0);
        }
    }

    /// Store to base+off for any 64-bit off (uses T6 when needed;
    /// src must not be T6).
    pub fn sd_any(&mut self, base: Reg, off: i64, src: Reg) {
        if (-2048..=2047).contains(&off) {
            self.sd(base, off as i32, src);
        } else {
            debug_assert!(src != Reg::T6);
            self.addi_any(Reg::T6, base, off);
            self.sd(Reg::T6, 0, src);
        }
    }

    // ── F/D float ops ───────────────────────────────────────────────

    fn enc_fr(&mut self, rd: u8, rm: u32, rs1: u8, rs2: u8, funct7: u32) {
        self.emit_u32(
            (funct7 << 25)
                | ((rs2 as u32) << 20)
                | ((rs1 as u32) << 15)
                | (rm << 12)
                | ((rd as u32) << 7)
                | 0b1010011,
        );
    }

    pub fn flw(&mut self, rd: FReg, base: Reg, off: i32) { self.enc_i(0b0000111, rd as u8, 0b010, base as u8, off); }
    pub fn fld(&mut self, rd: FReg, base: Reg, off: i32) { self.enc_i(0b0000111, rd as u8, 0b011, base as u8, off); }
    pub fn fsw(&mut self, base: Reg, off: i32, src: FReg) { self.enc_s(0b0100111, 0b010, base as u8, src as u8, off); }
    pub fn fsd(&mut self, base: Reg, off: i32, src: FReg) { self.enc_s(0b0100111, 0b011, base as u8, src as u8, off); }

    pub fn fadd_s(&mut self, rd: FReg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b111, rs1 as u8, rs2 as u8, 0b0000000); }
    pub fn fsub_s(&mut self, rd: FReg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b111, rs1 as u8, rs2 as u8, 0b0000100); }
    pub fn fmul_s(&mut self, rd: FReg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b111, rs1 as u8, rs2 as u8, 0b0001000); }
    pub fn fdiv_s(&mut self, rd: FReg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b111, rs1 as u8, rs2 as u8, 0b0001100); }
    pub fn fsqrt_s(&mut self, rd: FReg, rs1: FReg) { self.enc_fr(rd as u8, 0b111, rs1 as u8, 0, 0b0101100); }
    pub fn fadd_d(&mut self, rd: FReg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b111, rs1 as u8, rs2 as u8, 0b0000001); }
    pub fn fsub_d(&mut self, rd: FReg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b111, rs1 as u8, rs2 as u8, 0b0000101); }
    pub fn fmul_d(&mut self, rd: FReg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b111, rs1 as u8, rs2 as u8, 0b0001001); }
    pub fn fdiv_d(&mut self, rd: FReg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b111, rs1 as u8, rs2 as u8, 0b0001101); }
    pub fn fsqrt_d(&mut self, rd: FReg, rs1: FReg) { self.enc_fr(rd as u8, 0b111, rs1 as u8, 0, 0b0101101); }

    pub fn fmin_s(&mut self, rd: FReg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b000, rs1 as u8, rs2 as u8, 0b0010100); }
    pub fn fmax_s(&mut self, rd: FReg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b001, rs1 as u8, rs2 as u8, 0b0010100); }
    pub fn fmin_d(&mut self, rd: FReg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b000, rs1 as u8, rs2 as u8, 0b0010101); }
    pub fn fmax_d(&mut self, rd: FReg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b001, rs1 as u8, rs2 as u8, 0b0010101); }

    pub fn fsgnj_s(&mut self, rd: FReg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b000, rs1 as u8, rs2 as u8, 0b0010000); }
    pub fn fsgnjn_s(&mut self, rd: FReg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b001, rs1 as u8, rs2 as u8, 0b0010000); }
    pub fn fsgnjx_s(&mut self, rd: FReg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b010, rs1 as u8, rs2 as u8, 0b0010000); }
    pub fn fsgnj_d(&mut self, rd: FReg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b000, rs1 as u8, rs2 as u8, 0b0010001); }
    pub fn fsgnjn_d(&mut self, rd: FReg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b001, rs1 as u8, rs2 as u8, 0b0010001); }
    pub fn fsgnjx_d(&mut self, rd: FReg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b010, rs1 as u8, rs2 as u8, 0b0010001); }

    pub fn fmv_s(&mut self, rd: FReg, rs: FReg) { self.fsgnj_s(rd, rs, rs); }
    pub fn fmv_d(&mut self, rd: FReg, rs: FReg) { self.fsgnj_d(rd, rs, rs); }
    pub fn fneg_s(&mut self, rd: FReg, rs: FReg) { self.fsgnjn_s(rd, rs, rs); }
    pub fn fneg_d(&mut self, rd: FReg, rs: FReg) { self.fsgnjn_d(rd, rs, rs); }
    pub fn fabs_s(&mut self, rd: FReg, rs: FReg) { self.fsgnjx_s(rd, rs, rs); }
    pub fn fabs_d(&mut self, rd: FReg, rs: FReg) { self.fsgnjx_d(rd, rs, rs); }

    pub fn feq_s(&mut self, rd: Reg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b010, rs1 as u8, rs2 as u8, 0b1010000); }
    pub fn flt_s(&mut self, rd: Reg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b001, rs1 as u8, rs2 as u8, 0b1010000); }
    pub fn fle_s(&mut self, rd: Reg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b000, rs1 as u8, rs2 as u8, 0b1010000); }
    pub fn feq_d(&mut self, rd: Reg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b010, rs1 as u8, rs2 as u8, 0b1010001); }
    pub fn flt_d(&mut self, rd: Reg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b001, rs1 as u8, rs2 as u8, 0b1010001); }
    pub fn fle_d(&mut self, rd: Reg, rs1: FReg, rs2: FReg) { self.enc_fr(rd as u8, 0b000, rs1 as u8, rs2 as u8, 0b1010001); }

    pub fn fclass_s(&mut self, rd: Reg, rs1: FReg) { self.enc_fr(rd as u8, 0b001, rs1 as u8, 0, 0b1110000); }
    pub fn fclass_d(&mut self, rd: Reg, rs1: FReg) { self.enc_fr(rd as u8, 0b001, rs1 as u8, 0, 0b1110001); }

    pub fn fcvt_w_s(&mut self, rd: Reg, rs1: FReg, rm: u32) { self.enc_fr(rd as u8, rm, rs1 as u8, 0b00000, 0b1100000); }
    pub fn fcvt_wu_s(&mut self, rd: Reg, rs1: FReg, rm: u32) { self.enc_fr(rd as u8, rm, rs1 as u8, 0b00001, 0b1100000); }
    pub fn fcvt_l_s(&mut self, rd: Reg, rs1: FReg, rm: u32) { self.enc_fr(rd as u8, rm, rs1 as u8, 0b00010, 0b1100000); }
    pub fn fcvt_lu_s(&mut self, rd: Reg, rs1: FReg, rm: u32) { self.enc_fr(rd as u8, rm, rs1 as u8, 0b00011, 0b1100000); }
    pub fn fcvt_w_d(&mut self, rd: Reg, rs1: FReg, rm: u32) { self.enc_fr(rd as u8, rm, rs1 as u8, 0b00000, 0b1100001); }
    pub fn fcvt_wu_d(&mut self, rd: Reg, rs1: FReg, rm: u32) { self.enc_fr(rd as u8, rm, rs1 as u8, 0b00001, 0b1100001); }
    pub fn fcvt_l_d(&mut self, rd: Reg, rs1: FReg, rm: u32) { self.enc_fr(rd as u8, rm, rs1 as u8, 0b00010, 0b1100001); }
    pub fn fcvt_lu_d(&mut self, rd: Reg, rs1: FReg, rm: u32) { self.enc_fr(rd as u8, rm, rs1 as u8, 0b00011, 0b1100001); }

    pub fn fcvt_s_w(&mut self, rd: FReg, rs1: Reg) { self.enc_fr(rd as u8, 0b111, rs1 as u8, 0b00000, 0b1101000); }
    pub fn fcvt_s_wu(&mut self, rd: FReg, rs1: Reg) { self.enc_fr(rd as u8, 0b111, rs1 as u8, 0b00001, 0b1101000); }
    pub fn fcvt_s_l(&mut self, rd: FReg, rs1: Reg) { self.enc_fr(rd as u8, 0b111, rs1 as u8, 0b00010, 0b1101000); }
    pub fn fcvt_s_lu(&mut self, rd: FReg, rs1: Reg) { self.enc_fr(rd as u8, 0b111, rs1 as u8, 0b00011, 0b1101000); }
    pub fn fcvt_d_w(&mut self, rd: FReg, rs1: Reg) { self.enc_fr(rd as u8, 0b000, rs1 as u8, 0b00000, 0b1101001); }
    pub fn fcvt_d_wu(&mut self, rd: FReg, rs1: Reg) { self.enc_fr(rd as u8, 0b000, rs1 as u8, 0b00001, 0b1101001); }
    pub fn fcvt_d_l(&mut self, rd: FReg, rs1: Reg) { self.enc_fr(rd as u8, 0b111, rs1 as u8, 0b00010, 0b1101001); }
    pub fn fcvt_d_lu(&mut self, rd: FReg, rs1: Reg) { self.enc_fr(rd as u8, 0b111, rs1 as u8, 0b00011, 0b1101001); }

    pub fn fcvt_s_d(&mut self, rd: FReg, rs1: FReg) { self.enc_fr(rd as u8, 0b111, rs1 as u8, 0b00001, 0b0100000); }
    pub fn fcvt_d_s(&mut self, rd: FReg, rs1: FReg) { self.enc_fr(rd as u8, 0b000, rs1 as u8, 0b00000, 0b0100001); }

    pub fn fmv_x_w(&mut self, rd: Reg, rs1: FReg) { self.enc_fr(rd as u8, 0b000, rs1 as u8, 0b00000, 0b1110000); }
    pub fn fmv_w_x(&mut self, rd: FReg, rs1: Reg) { self.enc_fr(rd as u8, 0b000, rs1 as u8, 0b00000, 0b1111000); }
    pub fn fmv_x_d(&mut self, rd: Reg, rs1: FReg) { self.enc_fr(rd as u8, 0b000, rs1 as u8, 0b00000, 0b1110001); }
    pub fn fmv_d_x(&mut self, rd: FReg, rs1: Reg) { self.enc_fr(rd as u8, 0b000, rs1 as u8, 0b00000, 0b1111001); }
}

/// Rounding modes for fcvt.
pub mod rm {
    pub const RNE: u32 = 0b000; // nearest, ties even (wasm nearest)
    pub const RTZ: u32 = 0b001; // toward zero (wasm trunc)
    pub const RDN: u32 = 0b010; // toward -inf (wasm floor)
    pub const RUP: u32 = 0b011; // toward +inf (wasm ceil)
    pub const DYN: u32 = 0b111;
}
