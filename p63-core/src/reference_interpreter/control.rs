//! Jumps, subroutine calls and returns.

use super::ReferenceInterpreter;
use crate::{Addr, Bus, CpuError, Cycles};

impl<B: Bus> ReferenceInterpreter<B> {
    /// Pushes a 24-bit value: SP moves down 3, low byte at the lowest address.
    pub(super) fn push_u24(&mut self, value: u32) {
        let sp = self.registers.sp().wrapping_sub(3) & 0x00FF_FFFF;
        self.registers.set_sp(sp);
        self.bus.write_u24_le(Addr::new(sp), value);
    }

    /// Pops a 24-bit value pushed by `push_u24`.
    pub(super) fn pop_u24(&mut self) -> u32 {
        let sp = self.registers.sp();
        let value = self.bus.read_u24_le(Addr::new(sp));
        self.registers.set_sp(sp.wrapping_add(3));
        value
    }

    /// Reads the 16-bit offset of a long branch (postbyte is the low byte).
    fn rel16(&mut self, postbyte: u8) -> u32 {
        let rel = u16::from(postbyte) | (u16::from(self.fetch_u8()) << 8);
        i32::from(rel as i16) as u32
    }

    /// Reads a 24-bit address (postbyte is the low byte).
    fn addr24(&mut self, postbyte: u8) -> u32 {
        u32::from(postbyte) | (self.fetch_le(2) << 8)
    }

    /// JMP addr24 (opcode 0x40)
    pub(super) fn exec_jmp(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let target = self.addr24(postbyte);
        self.registers.set_pc(target);
        Ok(Cycles(2))
    }

    /// JSR addr24 (opcode 0x41), pushes the address of the next instruction
    pub(super) fn exec_jsr(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let target = self.addr24(postbyte);
        self.push_u24(self.registers.pc());
        self.registers.set_pc(target);
        Ok(Cycles(3))
    }

    /// RTS (opcode 0x42)
    pub(super) fn exec_rts(&mut self, _postbyte: u8) -> Result<Cycles, CpuError> {
        let ret = self.pop_u24();
        self.registers.set_pc(ret);
        Ok(Cycles(3))
    }

    /// BSR rel8 (opcode 0x44)
    pub(super) fn exec_bsr(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let ret = self.registers.pc();
        self.push_u24(ret);
        self.registers.set_pc(ret.wrapping_add(i32::from(postbyte as i8) as u32));
        Ok(Cycles(2))
    }

    /// LBRA rel16 (opcode 0x45)
    pub(super) fn exec_lbra(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let offset = self.rel16(postbyte);
        self.registers.set_pc(self.registers.pc().wrapping_add(offset));
        Ok(Cycles(2))
    }

    /// LBSR rel16 (opcode 0x4D)
    pub(super) fn exec_lbsr(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let offset = self.rel16(postbyte);
        let ret = self.registers.pc();
        self.push_u24(ret);
        self.registers.set_pc(ret.wrapping_add(offset));
        Ok(Cycles(3))
    }
}
