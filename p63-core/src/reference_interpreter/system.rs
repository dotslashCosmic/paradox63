//! Flag operations, system control, wait and prefix handlers.

use super::{Halt, ReferenceInterpreter};
use crate::{Addr, Bus, CpuError, Cycles, Flags};

impl<B: Bus> ReferenceInterpreter<B> {
    /// CLC SEC CLV SEV CLI SEI (opcodes 0x70-0x75)
    pub(super) fn exec_flag_op(&mut self, opcode: u8) -> Result<Cycles, CpuError> {
        let (flag, on) = match opcode {
            0x70 => (Flags::C, false),
            0x71 => (Flags::C, true),
            0x72 => (Flags::V, false),
            0x73 => (Flags::V, true),
            0x74 => (Flags::I, false),
            _ => (Flags::I, true),
        };
        let mut cc = self.registers.cc();
        cc.set(flag, on);
        self.registers.set_cc(cc);
        Ok(Cycles(1))
    }

    /// LD X,$addr / LD Y,$addr (opcodes 0xCE, 0xCF), 24-bit load, sets N and Z
    pub(super) fn exec_ld_index_extended(&mut self, opcode: u8, postbyte: u8) -> Result<Cycles, CpuError> {
        let addr = u32::from(postbyte) | (self.fetch_le(2) << 8);
        let value = self.bus.read_u24_le(Addr::new(addr));
        if opcode == 0xCE {
            self.registers.set_x(value);
        } else {
            self.registers.set_y(value);
        }
        let mut cc = self.registers.cc();
        cc.set(Flags::N, value & 0x0080_0000 != 0);
        cc.set(Flags::Z, value == 0);
        self.registers.set_cc(cc);
        Ok(Cycles(4))
    }

    /// WAIT (0xB0) / WAITV (0xB1): halt until an interrupt or vblank
    pub(super) fn exec_wait(&mut self, opcode: u8) -> Result<Cycles, CpuError> {
        self.halt = if opcode == 0xB0 { Halt::Interrupt } else { Halt::VBlank };
        Ok(Cycles(2))
    }

    /// RESET (opcode 0xF4)
    pub(super) fn exec_reset(&mut self) -> Result<Cycles, CpuError> {
        self.reset();
        Ok(Cycles(2))
    }

    /// 0xFE / 0xFF prefix: the postbyte is the extended opcode and one more byte follows.
    /// The extended table is empty, so every entry runs as a 1-cycle NOP.
    pub(super) fn exec_prefix(&mut self, _ext_opcode: u8) -> Result<Cycles, CpuError> {
        let _postbyte = self.fetch_u8();
        Ok(Cycles(1))
    }
}
