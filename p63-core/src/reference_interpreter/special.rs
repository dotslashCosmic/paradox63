//! PACK UNP BEXT BINS LEA LDI MOVQ DAA.

use super::operand::{alu_dest_bits, width_mask};
use super::ReferenceInterpreter;
use crate::{Bus, CpuError, Cycles, Flags};

const ILLEGAL: Result<Cycles, CpuError> = Ok(Cycles(1));

/// True for registers that hold one byte and can be written (A B E F GP DP).
fn is_byte_reg(reg: u8) -> bool {
    alu_dest_bits(reg) == Some(8)
}

impl<B: Bus> ReferenceInterpreter<B> {
    fn set_nz_width(&mut self, value: u32, bits: u32) {
        let mut cc = self.registers.cc();
        cc.set(Flags::N, value & (1 << (bits - 1)) != 0);
        cc.set(Flags::Z, value == 0);
        self.registers.set_cc(cc);
    }

    /// PACK hi,lo into a 16-bit register (opcode 0x50). Postbyte `[hi][lo]`, then dest.
    pub(super) fn exec_pack(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let (hi, lo) = (postbyte >> 4, postbyte & 0x0F);
        let dest = self.fetch_u8();
        if !is_byte_reg(hi) || !is_byte_reg(lo) || alu_dest_bits(dest) != Some(16) {
            return ILLEGAL;
        }
        let value = (self.read_register(hi)? << 8) | self.read_register(lo)?;
        self.write_register(dest, value)?;
        Ok(Cycles(1))
    }

    /// UNP hi,lo from a 16-bit register (opcode 0x51). Postbyte `[hi][lo]`, then src.
    pub(super) fn exec_unp(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let (hi, lo) = (postbyte >> 4, postbyte & 0x0F);
        let src = self.fetch_u8();
        if !is_byte_reg(hi) || !is_byte_reg(lo) || alu_dest_bits(src) != Some(16) {
            return ILLEGAL;
        }
        // read first: hi/lo may be halves of src
        let value = self.read_register(src)?;
        self.write_register(hi, value >> 8)?;
        self.write_register(lo, value & 0xFF)?;
        Ok(Cycles(1))
    }

    /// Decodes `[src][dest]` plus a `[pos][len-1]` field byte.
    /// Returns `(src, dest, dest_bits, pos, len)`.
    fn field_operand(&mut self, postbyte: u8) -> Option<(u32, u8, u32, u32, u32)> {
        let (src, dest) = (postbyte >> 4, postbyte & 0x0F);
        let field = self.fetch_u8();
        let (pos, len) = (u32::from(field >> 4), u32::from(field & 0x0F) + 1);
        let src_bits = alu_dest_bits(src)?;
        let dest_bits = alu_dest_bits(dest)?;
        let value = self.read_register(src).ok()?;
        Some((value & width_mask(src_bits), dest, dest_bits, pos, len)).filter(|_| pos + len <= src_bits.max(dest_bits))
    }

    /// BEXT dest,src,#pos,#len (opcode 0x52): dest = src[pos..pos+len], sets N Z, clears V C
    pub(super) fn exec_bext(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let Some((src, dest, dest_bits, pos, len)) = self.field_operand(postbyte) else { return ILLEGAL };
        if len > dest_bits {
            return ILLEGAL;
        }
        let value = (src >> pos) & width_mask(len);
        self.write_register(dest, value)?;
        self.set_nz_width(value, dest_bits);
        let mut cc = self.registers.cc();
        cc.remove(Flags::V | Flags::C);
        self.registers.set_cc(cc);
        Ok(Cycles(2))
    }

    /// BINS dest,src,#pos,#len (opcode 0x53): dest[pos..pos+len] = src's low bits. No flags.
    pub(super) fn exec_bins(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let Some((src, dest, dest_bits, pos, len)) = self.field_operand(postbyte) else { return ILLEGAL };
        if pos + len > dest_bits {
            return ILLEGAL;
        }
        let mask = width_mask(len) << pos;
        let old = self.read_register(dest)?;
        self.write_register(dest, (old & !mask) | ((src << pos) & mask))?;
        Ok(Cycles(2))
    }

    /// LEA reg,offset,reg (opcode 0x54): the postbyte is an indexed postbyte and the
    /// effective address goes back into its base register (X or Y). No flags.
    pub(super) fn exec_lea(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let Some((ea, extra)) = self.decode_indexed(postbyte) else { return ILLEGAL };
        if postbyte & 0x80 == 0 {
            self.registers.set_x(ea);
        } else {
            self.registers.set_y(ea);
        }
        Ok(Cycles(1 + extra))
    }

    /// LDI reg,#imm (opcode 0x5A): `[0000][reg]`, immediate sized to reg. Sets N Z.
    pub(super) fn exec_ldi(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let reg = postbyte & 0x0F;
        let Some(bits) = alu_dest_bits(reg).filter(|_| postbyte >> 4 == 0) else { return ILLEGAL };
        let value = self.fetch_le(bits / 8);
        self.write_register(reg, value)?;
        self.set_nz_width(value, bits);
        Ok(Cycles(if bits == 8 { 1 } else { 2 }))
    }

    /// MOVQ Q,src (opcode 0x5B): `[src][0000]`, source as in the ALU form. Sets N Z.
    pub(super) fn exec_movq(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if postbyte & 0x0F != 0 {
            return ILLEGAL;
        }
        let Some((value, cycles)) = self.alu_source(postbyte >> 4, 32) else { return ILLEGAL };
        self.registers.set_q(value);
        self.set_nz_width(value, 32);
        // memory forms add the indexed extra cycles
        Ok(Cycles(3 + cycles.saturating_sub(2)))
    }

    /// DAA (opcode 0x5D): postbyte 0x00 adjusts A after an add, 0x01 after a subtract.
    pub(super) fn exec_daa(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let a = self.registers.a();
        let mut cc = self.registers.cc();
        let half = cc.contains(Flags::H);
        let carry = cc.contains(Flags::C);
        let (result, carry_out) = match postbyte {
            0x00 => {
                let mut fix = 0u8;
                let mut c = carry;
                if half || a & 0x0F > 9 {
                    fix |= 0x06;
                }
                if carry || a > 0x99 {
                    fix |= 0x60;
                    c = true;
                }
                (a.wrapping_add(fix), c)
            }
            0x01 => {
                let mut fix = 0u8;
                if half {
                    fix |= 0x06;
                }
                if carry {
                    fix |= 0x60;
                }
                (a.wrapping_sub(fix), carry)
            }
            _ => return ILLEGAL,
        };
        self.registers.set_a(result);
        cc.set(Flags::N, result & 0x80 != 0);
        cc.set(Flags::Z, result == 0);
        cc.set(Flags::C, carry_out);
        self.registers.set_cc(cc);
        Ok(Cycles(1))
    }
}
