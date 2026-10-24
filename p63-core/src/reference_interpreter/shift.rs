//! Shift and rotate instruction handlers.

use super::arithmetic::sign_bit;
use super::operand::width_mask;
use super::ReferenceInterpreter;
use crate::{Bus, CpuError, Cycles, Flags};

#[derive(Clone, Copy)]
enum Shift {
    Asl,
    Asr,
    Lsr,
    Rol,
    Ror,
    Rcl,
    Rcr,
}

impl<B: Bus> ReferenceInterpreter<B> {
    /// Shift/rotate `reg[,#count]`, one bit at a time.
    fn exec_shift(&mut self, op: Shift, postbyte: u8) -> Result<Cycles, CpuError> {
        let Some((reg, bits, count, cycles)) = self.count_operand(postbyte) else {
            return Ok(Cycles(1));
        };
        let mask = width_mask(bits);
        let sign = sign_bit(bits);
        let mut v = self.read_register(reg)? & mask;
        let mut flags = self.registers.cc();
        let mut carry = flags.contains(Flags::C);

        for _ in 0..count {
            let msb = v & sign != 0;
            let lsb = v & 1 != 0;
            v = match op {
                Shift::Asl => (v << 1) & mask,
                Shift::Asr => (v >> 1) | (v & sign),
                Shift::Lsr => v >> 1,
                Shift::Rol => ((v << 1) | u32::from(msb)) & mask,
                Shift::Ror => (v >> 1) | if lsb { sign } else { 0 },
                Shift::Rcl => ((v << 1) | u32::from(carry)) & mask,
                Shift::Rcr => (v >> 1) | if carry { sign } else { 0 },
            };
            carry = match op {
                Shift::Asl | Shift::Rol | Shift::Rcl => msb,
                Shift::Asr | Shift::Lsr | Shift::Ror | Shift::Rcr => lsb,
            };
        }

        let negative = v & sign != 0;
        flags.set(Flags::N, negative);
        flags.set(Flags::Z, v == 0);
        flags.set(Flags::C, carry);
        match op {
            Shift::Asl | Shift::Rol | Shift::Rcl => flags.set(Flags::V, negative != carry),
            Shift::Asr | Shift::Lsr => flags.set(Flags::V, false),
            // ROR and RCR leave V alone
            Shift::Ror | Shift::Rcr => {}
        }
        self.registers.set_cc(flags);
        self.write_register(reg, v)?;
        Ok(Cycles(cycles))
    }

    /// ASL reg[,#count] (opcode 0x20); LSL (0x22) is the same operation
    pub(super) fn exec_asl(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_shift(Shift::Asl, postbyte)
    }

    /// ASR reg[,#count] (opcode 0x21)
    pub(super) fn exec_asr(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_shift(Shift::Asr, postbyte)
    }

    /// LSR reg[,#count] (opcode 0x23)
    pub(super) fn exec_lsr(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_shift(Shift::Lsr, postbyte)
    }

    /// ROL reg[,#count] (opcode 0x24)
    pub(super) fn exec_rol(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_shift(Shift::Rol, postbyte)
    }

    /// ROR reg[,#count] (opcode 0x25)
    pub(super) fn exec_ror(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_shift(Shift::Ror, postbyte)
    }

    /// RCL reg[,#count] (opcode 0x26)
    pub(super) fn exec_rcl(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_shift(Shift::Rcl, postbyte)
    }

    /// RCR reg[,#count] (opcode 0x27)
    pub(super) fn exec_rcr(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_shift(Shift::Rcr, postbyte)
    }
}
