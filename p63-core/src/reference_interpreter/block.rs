//! Interruptible block operations: MOVB MOVW FILL CPY SCNB.
//!
//! Each step runs one unit (a byte, or up to two words). Until the operation is
//! done, PC stays on the block instruction and X, Y and the remaining count are
//! kept in the continuation, so interrupts are taken between units and the
//! operation picks up where it stopped when the handler returns.

use super::operand::odd_penalty;
use super::{BlockContinuation, BlockOp, ReferenceInterpreter};
use crate::{Addr, Bus, CpuError, Cycles, Flags};

/// Result of one unit of work: (units done, cycles, stop early).
type Unit = (u32, u32, bool);

impl<B: Bus> ReferenceInterpreter<B> {
    fn advance(&mut self, x: u32, y: u32) {
        self.registers.set_x(self.registers.x().wrapping_add(x));
        self.registers.set_y(self.registers.y().wrapping_add(y));
    }

    fn movb_unit(&mut self) -> Unit {
        let b = self.bus.read(Addr::new(self.registers.x()));
        self.bus.write(Addr::new(self.registers.y()), b);
        self.advance(1, 1);
        (1, 1, false)
    }

    /// Copies up to 2 words; each word is a read and a write, so 1 cycle per word.
    fn movw_unit(&mut self, remaining: u32) -> Unit {
        let words = remaining.min(2);
        let mut cycles = words;
        for _ in 0..words {
            let (src, dst) = (self.registers.x(), self.registers.y());
            let word = self.bus.read_u16_le(Addr::new(src));
            self.bus.write_u16_le(Addr::new(dst), word);
            cycles += odd_penalty(src, 2) + odd_penalty(dst, 2);
            self.advance(2, 2);
        }
        (words, cycles, false)
    }

    /// Writes up to 2 words in one cycle.
    fn fill_unit(&mut self, remaining: u32, value: u16) -> Unit {
        let words = remaining.min(2);
        let mut cycles = 1;
        for _ in 0..words {
            let x = self.registers.x();
            self.bus.write_u16_le(Addr::new(x), value);
            cycles += odd_penalty(x, 2);
            self.advance(2, 0);
        }
        (words, cycles, false)
    }

    /// Compares up to 2 words, stopping at the first mismatch with X/Y left on it.
    fn cpy_unit(&mut self, remaining: u32) -> Unit {
        let words = remaining.min(2);
        let mut penalty = 0;
        for done in 0..words {
            let (src, dst) = (self.registers.x(), self.registers.y());
            penalty += odd_penalty(src, 2) + odd_penalty(dst, 2);
            let a = u32::from(self.bus.read_u16_le(Addr::new(src)));
            let b = u32::from(self.bus.read_u16_le(Addr::new(dst)));
            let diff = a.wrapping_sub(b) & 0xFFFF;
            let mut cc = self.registers.cc();
            cc.set(Flags::N, diff & 0x8000 != 0);
            cc.set(Flags::Z, diff == 0);
            cc.set(Flags::V, ((a ^ b) & (a ^ diff)) & 0x8000 != 0);
            cc.set(Flags::C, a < b);
            self.registers.set_cc(cc);
            if a != b {
                return (done, done + 1 + penalty, true);
            }
            self.advance(2, 2);
        }
        (words, words + penalty, false)
    }

    /// Checks one byte; on a match X is left pointing at it.
    fn scnb_unit(&mut self, value: u8) -> Unit {
        let b = self.bus.read(Addr::new(self.registers.x()));
        let found = b == value;
        let mut cc = self.registers.cc();
        cc.set(Flags::N, b.wrapping_sub(value) & 0x80 != 0);
        cc.set(Flags::Z, found);
        cc.set(Flags::C, found);
        cc.set(Flags::V, false);
        self.registers.set_cc(cc);
        if found {
            return (1, 1, true);
        }
        self.advance(1, 0);
        (1, 1, false)
    }

    /// MOVB MOVW FILL CPY SCNB (opcodes 0x60-0x64), one unit per step.
    /// Postbyte: bit 7 clear = 7-bit count, set = 16-bit count follows.
    /// FILL then takes a 16-bit value, SCNB an 8-bit one.
    pub(super) fn exec_block(&mut self, opcode: u8, postbyte: u8) -> Result<Cycles, CpuError> {
        let start = self.registers.pc().wrapping_sub(2) & 0x00FF_FFFF;
        let mut remaining = if postbyte & 0x80 == 0 { u32::from(postbyte & 0x7F) } else { self.fetch_le(2) };
        let value = match opcode {
            0x62 => self.fetch_le(2),
            0x64 => self.fetch_le(1),
            _ => 0,
        };
        let op = match opcode {
            0x60 => BlockOp::MoveByte,
            0x61 => BlockOp::MoveWord,
            0x62 => BlockOp::Fill,
            0x63 => BlockOp::Compare,
            _ => BlockOp::ScanByte,
        };

        // carrying on: X, Y and the count come from the continuation
        let mut resumed = false;
        if let Some(k) = self.block_continuation.take() {
            if k.pc == start && k.operation == op {
                self.registers.set_x(k.x);
                self.registers.set_y(k.y);
                remaining = k.remaining;
                resumed = true;
            } else {
                self.block_continuation = Some(k);
            }
        }

        if !resumed {
            let mut cc = self.registers.cc();
            match op {
                BlockOp::Compare => cc.insert(Flags::Z),
                BlockOp::ScanByte => cc.remove(Flags::Z | Flags::C),
                _ => {}
            }
            self.registers.set_cc(cc);
        }
        if remaining == 0 {
            return Ok(Cycles(1));
        }

        let (done, cycles, stop) = match op {
            BlockOp::MoveByte => self.movb_unit(),
            BlockOp::MoveWord => self.movw_unit(remaining),
            BlockOp::Fill => self.fill_unit(remaining, value as u16),
            BlockOp::Compare => self.cpy_unit(remaining),
            BlockOp::ScanByte => self.scnb_unit(value as u8),
        };
        remaining -= done;
        if !stop && remaining > 0 {
            self.block_continuation = Some(BlockContinuation {
                x: self.registers.x(),
                y: self.registers.y(),
                remaining,
                operation: op,
                pc: start,
            });
            self.registers.set_pc(start);
        }
        Ok(Cycles(cycles))
    }
}
