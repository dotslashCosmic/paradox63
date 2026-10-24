//! Conditional and relative branch handlers.

use super::ReferenceInterpreter;
use crate::{Bus, CpuError, Cycles, Flags};

impl<B: Bus> ReferenceInterpreter<B> {
    /// Evaluates a branch condition by its short-branch index (0x30 + n).
    fn condition(&self, index: u8) -> bool {
        let cc = self.registers.cc();
        let zero = cc.contains(Flags::Z);
        let carry = cc.contains(Flags::C);
        let neg = cc.contains(Flags::N);
        let ovf = cc.contains(Flags::V);
        match index {
            0x0 => true,                   // BRA
            0x1 => false,                  // BRN
            0x2 => zero,                   // BEQ
            0x3 => !zero,                  // BNE
            0x4 => !carry,                 // BCC
            0x5 => carry,                  // BCS
            0x6 => !carry && !zero,        // BHI
            0x7 => carry || zero,          // BLS
            0x8 => neg == ovf,             // BGE
            0x9 => neg != ovf,             // BLT
            0xA => !zero && neg == ovf,    // BGT
            0xB => zero || neg != ovf,     // BLE
            0xC => neg,                    // BMI
            0xD => !neg,                   // BPL
            0xE => ovf,                    // BVS
            _ => !ovf,                     // BVC
        }
    }

    /// Short branch, opcode 0x30-0x3F, 8-bit offset from the next instruction.
    pub(super) fn exec_short_branch(&mut self, opcode: u8, postbyte: u8) -> Result<Cycles, CpuError> {
        if !self.condition(opcode & 0x0F) {
            return Ok(Cycles(1));
        }
        let offset = i32::from(postbyte as i8) as u32;
        self.registers.set_pc(self.registers.pc().wrapping_add(offset));
        Ok(Cycles(2))
    }

    /// Long branch, opcode 0x80-0x8D, 16-bit offset from the next instruction.
    /// Same conditions as BEQ..BVC.
    pub(super) fn exec_long_branch(&mut self, opcode: u8, postbyte: u8) -> Result<Cycles, CpuError> {
        let rel = u16::from(postbyte) | (u16::from(self.fetch_u8()) << 8);
        if !self.condition(opcode - 0x80 + 2) {
            return Ok(Cycles(2));
        }
        let offset = i32::from(rel as i16) as u32;
        self.registers.set_pc(self.registers.pc().wrapping_add(offset));
        Ok(Cycles(3))
    }
}
