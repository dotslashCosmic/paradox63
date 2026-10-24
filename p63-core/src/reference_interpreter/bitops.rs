//! Bit operations and bit/count branches: BSET BCLR BCHG BTST JBS JBC DBNZ TBZ TBNZ.
//!
//! Bit operand postbyte: `[operand][bit]`. Operand 0x0-0xD is a register,
//! 0xF a memory byte (mode byte follows, as in the ALU memory form).

use super::operand::{alu_dest_bits, reg_bits, width_mask, SRC_MEM};
use super::ReferenceInterpreter;
use crate::{Addr, Bus, CpuError, Cycles, Flags};

const ILLEGAL: Result<Cycles, CpuError> = Ok(Cycles(1));

#[derive(Clone, Copy)]
enum Target {
    Reg(u8),
    Mem(u32),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BitOp {
    Set,
    Clear,
    Change,
    Test,
}

impl<B: Bus> ReferenceInterpreter<B> {
    /// Decodes a bit operand. Returns `(target, bit, extra_cycles)`.
    fn bit_operand(&mut self, postbyte: u8) -> Option<(Target, u32, u32)> {
        let src = postbyte >> 4;
        let bit = u32::from(postbyte & 0x0F);
        let (target, bits, extra) = if src == SRC_MEM {
            let (addr, extra) = self.mem_operand()?;
            (Target::Mem(addr), 8, extra)
        } else {
            (Target::Reg(src), alu_dest_bits(src)?, 0)
        };
        (bit < bits).then_some((target, bit, extra))
    }

    fn read_target(&mut self, target: Target) -> Result<u32, CpuError> {
        match target {
            Target::Reg(r) => self.read_register(r),
            Target::Mem(a) => Ok(u32::from(self.bus.read(Addr::new(a)))),
        }
    }

    fn write_target(&mut self, target: Target, value: u32) -> Result<(), CpuError> {
        match target {
            Target::Reg(r) => self.write_register(r, value),
            Target::Mem(a) => {
                self.bus.write(Addr::new(a), value as u8);
                Ok(())
            }
        }
    }

    /// Adds a sign-extended 8-bit offset to PC.
    fn branch_rel8(&mut self, rel: u8) {
        let pc = self.registers.pc().wrapping_add(i32::from(rel as i8) as u32);
        self.registers.set_pc(pc);
    }

    fn exec_bit_op(&mut self, op: BitOp, postbyte: u8) -> Result<Cycles, CpuError> {
        let Some((target, bit, extra)) = self.bit_operand(postbyte) else { return ILLEGAL };
        let value = self.read_target(target)?;
        let mask = 1 << bit;
        let new = match op {
            BitOp::Set => value | mask,
            BitOp::Clear => value & !mask,
            BitOp::Change => value ^ mask,
            BitOp::Test => {
                // N = the bit, Z = its inverse
                let set = value & mask != 0;
                let mut cc = self.registers.cc();
                cc.set(Flags::N, set);
                cc.set(Flags::Z, !set);
                cc.set(Flags::V, false);
                self.registers.set_cc(cc);
                return Ok(Cycles(3 + extra));
            }
        };
        self.write_target(target, new)?;
        Ok(Cycles(4 + extra))
    }

    /// BSET operand,#bit (opcode 0x90)
    pub(super) fn exec_bset(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_bit_op(BitOp::Set, postbyte)
    }

    /// BCLR operand,#bit (opcode 0x91)
    pub(super) fn exec_bclr(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_bit_op(BitOp::Clear, postbyte)
    }

    /// BCHG operand,#bit (opcode 0x92)
    pub(super) fn exec_bchg(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_bit_op(BitOp::Change, postbyte)
    }

    /// BTST operand,#bit (opcode 0x93)
    pub(super) fn exec_btst(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_bit_op(BitOp::Test, postbyte)
    }

    /// JBS / JBC operand,#bit,rel8 (opcodes 0x94, 0x95). No flags.
    pub(super) fn exec_jump_on_bit(&mut self, opcode: u8, postbyte: u8) -> Result<Cycles, CpuError> {
        let Some((target, bit, extra)) = self.bit_operand(postbyte) else { return ILLEGAL };
        let offset = self.fetch_u8();
        let set = self.read_target(target)? & (1 << bit) != 0;
        if set == (opcode == 0x94) {
            self.branch_rel8(offset);
        }
        Ok(Cycles(4 + extra))
    }

    /// DBNZ reg,rel8 (opcode 0x96): decrement, set N and Z, branch if not zero
    pub(super) fn exec_dbnz(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let reg = postbyte & 0x0F;
        let Some(bits) = alu_dest_bits(reg).filter(|_| postbyte >> 4 == 0) else { return ILLEGAL };
        let offset = self.fetch_u8();
        let value = self.read_register(reg)?.wrapping_sub(1) & width_mask(bits);
        self.write_register(reg, value)?;
        let mut cc = self.registers.cc();
        cc.set(Flags::N, value & (1 << (bits - 1)) != 0);
        cc.set(Flags::Z, value == 0);
        self.registers.set_cc(cc);
        if value != 0 {
            self.branch_rel8(offset);
        }
        Ok(Cycles(2))
    }

    /// TBZ / TBNZ reg,#bit,rel8 (opcodes 0x97, 0x98). No flags.
    pub(super) fn exec_test_bit_branch(&mut self, opcode: u8, postbyte: u8) -> Result<Cycles, CpuError> {
        let reg = postbyte >> 4;
        let bit = u32::from(postbyte & 0x0F);
        let Some(bits) = reg_bits(reg) else { return ILLEGAL };
        if bit >= bits {
            return ILLEGAL;
        }
        let offset = self.fetch_u8();
        let set = self.read_register(reg)? & (1 << bit) != 0;
        if set == (opcode == 0x98) {
            self.branch_rel8(offset);
        }
        Ok(Cycles(2))
    }
}
