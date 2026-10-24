//! Shared operand decoding: register codes, indexed sub-modes
//! and the ALU source operand.

use super::ReferenceInterpreter;
use crate::{Addr, Bus, CpuError, Flags};

/// Register code for X.
pub(super) const REG_X: u8 = 0x8;
/// ALU source code: immediate follows.
pub(super) const SRC_IMM: u8 = 0xE;
/// ALU source code: memory operand follows.
pub(super) const SRC_MEM: u8 = 0xF;

/// Width in bits of a register code, or `None` for reserved codes.
pub(super) fn reg_bits(reg: u8) -> Option<u32> {
    match reg {
        0x0 | 0x1 | 0x3 | 0x4 | 0x7 | 0xC | 0xD => Some(8), // A B E F GP DP CC
        0x2 | 0x5 => Some(16),                              // D W
        0x8..=0xB => Some(24),                              // X Y SP PC
        0x6 => Some(32),                                    // Q
        _ => None,
    }
}

/// Width of a register that may be an ALU destination: anything but PC and CC.
pub(super) fn alu_dest_bits(reg: u8) -> Option<u32> {
    match reg {
        0xB | 0xD => None,
        _ => reg_bits(reg),
    }
}

/// All-ones mask for a width in bits.
pub(super) fn width_mask(bits: u32) -> u32 {
    if bits >= 32 {
        u32::MAX
    } else {
        (1 << bits) - 1
    }
}

impl<B: Bus> ReferenceInterpreter<B> {
    /// Size in bytes of a register (used by PUSH/POP); unknown codes count as 1.
    pub(super) fn register_size(reg: u8) -> u32 {
        reg_bits(reg).map_or(1, |b| b / 8)
    }

    /// Reads a register by its 4-bit code, zero-extended to 32 bits.
    pub(super) fn read_register(&self, reg: u8) -> Result<u32, CpuError> {
        let r = &self.registers;
        Ok(match reg {
            0x0 => u32::from(r.a()),
            0x1 => u32::from(r.b()),
            0x2 => u32::from(r.d()),
            0x3 => u32::from(r.e()),
            0x4 => u32::from(r.f()),
            0x5 => u32::from(r.w()),
            0x6 => r.q(),
            0x7 => u32::from(r.gp()),
            0x8 => r.x(),
            0x9 => r.y(),
            0xA => r.sp(),
            0xB => r.pc(),
            0xC => u32::from(r.dp()),
            0xD => u32::from(r.cc().bits()),
            _ => return Err(CpuError::IllegalOpcode(crate::Opcode::new(reg))),
        })
    }

    /// Writes a register by its 4-bit code, truncating to its width.
    #[allow(clippy::cast_possible_truncation)]
    pub(super) fn write_register(&mut self, reg: u8, value: u32) -> Result<(), CpuError> {
        let r = &mut self.registers;
        match reg {
            0x0 => r.set_a(value as u8),
            0x1 => r.set_b(value as u8),
            0x2 => r.set_d(value as u16),
            0x3 => r.set_e(value as u8),
            0x4 => r.set_f(value as u8),
            0x5 => r.set_w(value as u16),
            0x6 => r.set_q(value),
            0x7 => r.set_gp(value as u8),
            0x8 => r.set_x(value),
            0x9 => r.set_y(value),
            0xA => r.set_sp(value),
            0xB => r.set_pc(value),
            0xC => r.set_dp(value as u8),
            0xD => r.set_cc(Flags::from_bits_truncate(value as u8)),
            _ => return Err(CpuError::IllegalOpcode(crate::Opcode::new(reg))),
        }
        Ok(())
    }

    /// Reads one byte at PC and advances PC.
    pub(super) fn fetch_u8(&mut self) -> u8 {
        let pc = self.registers.pc();
        let v = self.bus.read(Addr::new(pc));
        self.registers.set_pc(pc.wrapping_add(1));
        v
    }

    /// Reads `bytes` bytes (1-4) at PC, little-endian, and advances PC.
    pub(super) fn fetch_le(&mut self, bytes: u32) -> u32 {
        (0..bytes).fold(0, |acc, i| acc | (u32::from(self.fetch_u8()) << (8 * i)))
    }

    /// Reads `bytes` bytes (1-4) at `addr`, little-endian.
    pub(super) fn read_le(&mut self, addr: u32, bytes: u32) -> u32 {
        (0..bytes).fold(0, |acc, i| {
            acc | (u32::from(self.bus.read(Addr::new(addr.wrapping_add(i)))) << (8 * i))
        })
    }

    /// Decodes an indexed postbyte, fetching any displacement bytes.
    /// Returns `(effective_address, extra_cycles)`, or `None` for an illegal postbyte.
    pub(super) fn decode_indexed(&mut self, postbyte: u8) -> Option<(u32, u32)> {
        let base = if postbyte & 0x80 != 0 { self.registers.y() } else { self.registers.x() };
        let sext = |v: u32, bits: u32| -> u32 {
            let sign = 1 << (bits - 1);
            (v ^ sign).wrapping_sub(sign)
        };
        let low = u32::from(postbyte & 0x7F);
        let (offset, extra) = match low {
            0x00 => (0, 0),
            0x01 => (sext(self.fetch_le(1), 8), 1),
            0x02 => (sext(self.fetch_le(2), 16), 2),
            0x03 => (self.fetch_le(3), 3),
            0x08..=0x0F => (sext(low & 0x07, 3), 0),
            0x20..=0x3F => (sext(low & 0x1F, 5), 0),
            0x40..=0x7F => (sext(low & 0x3F, 6), 0),
            _ => return None,
        };
        Some((base.wrapping_add(offset) & 0x00FF_FFFF, extra))
    }

    /// Fetches an ALU source operand at `bits` width.
    /// Returns `(value, cycles)`, or `None` if the encoding is illegal.
    pub(super) fn alu_source(&mut self, src: u8, bits: u32) -> Option<(u32, u32)> {
        let mask = width_mask(bits);
        match src {
            SRC_IMM => Some((self.fetch_le(bits / 8), 2)),
            SRC_MEM => {
                let (addr, extra) = self.mem_operand()?;
                Some((self.read_le(addr, bits / 8), 2 + extra))
            }
            reg => self.read_register(reg).ok().map(|v| (v & mask, 1)),
        }
    }

    /// Decodes a memory mode byte and its address bytes: 0x00 DP, 0x01 extended, 0x02 indexed.
    /// Returns `(address, extra_cycles)`, or `None` if the encoding is illegal.
    pub(super) fn mem_operand(&mut self) -> Option<(u32, u32)> {
        match self.fetch_u8() {
            0x00 => {
                let off = self.fetch_u8();
                if off & 0xC0 != 0 {
                    return None;
                }
                Some(((u32::from(self.registers.dp()) << 8) | u32::from(off), 0))
            }
            0x01 => Some((self.fetch_le(3), 0)),
            0x02 => {
                let pb = self.fetch_u8();
                self.decode_indexed(pb)
            }
            _ => None,
        }
    }

    /// Decodes a `[count][reg]` postbyte (INC/DEC, shifts).
    /// Returns `(reg, bits, count, cycles)`, or `None` if the encoding is illegal.
    pub(super) fn count_operand(&mut self, postbyte: u8) -> Option<(u8, u32, u32, u32)> {
        let reg = postbyte & 0x0F;
        let bits = alu_dest_bits(reg)?;
        let (count, cycles) = match postbyte >> 4 {
            0 => (1, 1),
            SRC_IMM => match self.fetch_u8() {
                0 => return None,
                c => (u32::from(c), 1 + u32::from(c)),
            },
            _ => return None,
        };
        Some((reg, bits, count, cycles))
    }
}
