//! Interrupt entry, priority, stacking and RTI.
//!
//! Frame layout, deepest byte first: PC, then for a full frame the SP from
//! before the interrupt, Y, X, DP, GP, Q, and CC last so RTI can read it
//! first. Multi-byte values are little-endian. Full frame 19 bytes, FIRQ 4.

use super::ReferenceInterpreter;
use crate::{Addr, Bus, CpuError, Cycles, Flags};

/// Interrupt kinds, highest priority first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interrupt {
    Nmi,
    Firq,
    Irq,
    Swi,
}

impl<B: Bus> ReferenceInterpreter<B> {
    fn push_u8(&mut self, value: u8) {
        let sp = self.registers.sp().wrapping_sub(1) & 0x00FF_FFFF;
        self.registers.set_sp(sp);
        self.bus.write(Addr::new(sp), value);
    }

    fn pop_u8(&mut self) -> u8 {
        let sp = self.registers.sp();
        self.registers.set_sp(sp.wrapping_add(1));
        self.bus.read(Addr::new(sp))
    }

    fn push_u32(&mut self, value: u32) {
        let sp = self.registers.sp().wrapping_sub(4) & 0x00FF_FFFF;
        self.registers.set_sp(sp);
        self.bus.write_u32_le(Addr::new(sp), value);
    }

    fn pop_u32(&mut self) -> u32 {
        let sp = self.registers.sp();
        self.registers.set_sp(sp.wrapping_add(4));
        self.bus.read_u32_le(Addr::new(sp))
    }

    /// The highest-priority interrupt that should be taken now, if any.
    /// NMI can't interrupt an NMI handler; FIRQ and IRQ obey F and I.
    pub(super) fn pending_interrupt(&self) -> Option<Interrupt> {
        let cc = self.registers.cc();
        if self.nmi_pending && !self.nmi_active {
            Some(Interrupt::Nmi)
        } else if self.firq_pending && !cc.contains(Flags::F) {
            Some(Interrupt::Firq)
        } else if self.irq_pending && !cc.contains(Flags::I) {
            Some(Interrupt::Irq)
        } else {
            None
        }
    }

    /// Stacks the context and jumps to the vector. Returns the entry latency.
    pub(super) fn service(&mut self, kind: Interrupt) -> Cycles {
        let full = kind != Interrupt::Firq;
        let mut cc = self.registers.cc();
        cc.set(Flags::E, full);

        let sp_before = self.registers.sp();
        let pc = self.registers.pc();
        self.push_u24(pc);
        if full {
            self.push_u24(sp_before);
            self.push_u24(self.registers.y());
            self.push_u24(self.registers.x());
            self.push_u8(self.registers.dp());
            self.push_u8(self.registers.gp());
            self.push_u32(self.registers.q());
        }
        self.push_u8(cc.bits());

        // masks go on after CC is saved, so RTI brings back the old ones
        cc.insert(Flags::I);
        if kind == Interrupt::Firq {
            cc.insert(Flags::F);
        }
        self.registers.set_cc(cc);

        let vector = match kind {
            Interrupt::Nmi => p63_spec::VEC_NMI,
            Interrupt::Firq => p63_spec::VEC_FIRQ,
            Interrupt::Irq => p63_spec::VEC_IRQ,
            Interrupt::Swi => p63_spec::VEC_SWI,
        };
        let target = self.bus.read_u24_le(Addr::new(vector));
        self.registers.set_pc(target);

        if kind == Interrupt::Nmi {
            // edge-triggered: taken once per assertion
            self.nmi_pending = false;
            self.nmi_active = true;
        }
        Cycles(if full { 5 } else { 3 })
    }

    /// NMI IRQ FIRQ SWI as instructions (opcodes 0xF0-0xF3), ignoring the masks
    pub(super) fn exec_interrupt_op(&mut self, opcode: u8) -> Result<Cycles, CpuError> {
        let kind = match opcode {
            0xF0 => Interrupt::Nmi,
            0xF1 => Interrupt::Irq,
            0xF2 => Interrupt::Firq,
            _ => Interrupt::Swi,
        };
        Ok(self.service(kind))
    }

    /// RTI (opcode 0x43): CC first, then the rest of the frame if E was set
    pub(super) fn exec_rti(&mut self, _postbyte: u8) -> Result<Cycles, CpuError> {
        let cc = Flags::from_bits_truncate(self.pop_u8());
        let cycles = if cc.contains(Flags::E) {
            let q = self.pop_u32();
            let gp = self.pop_u8();
            let dp = self.pop_u8();
            let x = self.pop_u24();
            let y = self.pop_u24();
            let _sp_before = self.pop_u24();
            let pc = self.pop_u24();
            self.registers.set_q(q);
            self.registers.set_gp(gp);
            self.registers.set_dp(dp);
            self.registers.set_x(x);
            self.registers.set_y(y);
            self.registers.set_pc(pc);
            5
        } else {
            let pc = self.pop_u24();
            self.registers.set_pc(pc);
            3
        };
        self.registers.set_cc(cc);
        self.nmi_active = false;
        Ok(Cycles(cycles))
    }
}
