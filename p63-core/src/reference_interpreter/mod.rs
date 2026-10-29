//! Reference interpreter for the Paradox63 CPU.

// Truncating and sign-reinterpreting casts are how register widths work on
// this CPU, and every handler returns `Result` so dispatch stays uniform.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::unnecessary_wraps
)]

mod data_transfer;
mod arithmetic;
mod bitops;
mod block;
mod branch;
mod control;
mod interrupt;

pub use interrupt::Interrupt;
mod operand;
mod shift;
mod special;
mod system;

use crate::{Bus, Registers};

/// Block operation type for interruptible operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockOp {
    /// Move bytes (MOVB)
    MoveByte,
    /// Move words (MOVW)
    MoveWord,
    /// Fill memory (FILL)
    Fill,
    /// Compare memory (CPY)
    Compare,
    /// Scan byte (SCNB)
    ScanByte,
}

/// Continuation record for interruptible block operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockContinuation {
    /// Next source address for the transfer
    pub x: u32,
    /// Next destination address for the transfer
    pub y: u32,
    /// Number of units remaining to transfer
    pub remaining: u32,
    /// The type of block operation being performed
    pub operation: BlockOp,
    /// Address of the interrupted block instruction
    pub pc: u32,
}

/// Reference interpreter for the Paradox63 CPU.
#[allow(clippy::struct_excessive_bools)] // one per interrupt line
pub struct ReferenceInterpreter<B: Bus> {
    /// CPU register state
    pub registers: Registers,

    /// Memory bus
    pub bus: B,

    /// Non-maskable interrupt pending flag
    nmi_pending: bool,

    /// Standard interrupt request pending flag
    irq_pending: bool,

    /// Fast interrupt request pending flag
    firq_pending: bool,

    /// Block operation continuation state for interruptibility
    block_continuation: Option<BlockContinuation>,

    /// Set by WAIT/WAITV until the CPU is woken
    halt: Halt,

    /// Inside an NMI handler (NMI doesn't nest)
    nmi_active: bool,

    /// One entry per interrupt being handled, innermost last: its kind and the block
    /// operation it interrupted, so a handler's own block ops can't disturb it
    frames: Vec<(Interrupt, Option<BlockContinuation>)>,
}

/// Interrupt nesting kept track of; deeper frames (a handler that never returns) drop the oldest.
const MAX_FRAMES: usize = 64;

/// Everything the CPU keeps between instructions, so one program can be set aside
/// and another run (virtual mode), then the first picked up exactly where it was.
#[derive(Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)] // the interpreter's own flags, as they are
pub struct CpuState {
    pub registers: Registers,
    nmi_pending: bool,
    irq_pending: bool,
    firq_pending: bool,
    block_continuation: Option<BlockContinuation>,
    halt: Halt,
    nmi_active: bool,
    frames: Vec<(Interrupt, Option<BlockContinuation>)>,
}

impl CpuState {
    #[must_use]
    pub fn halt(&self) -> Halt {
        self.halt
    }

    /// What vblank does to a CPU in WAITV, for a program that isn't running just now.
    pub fn signal_vblank(&mut self) {
        if self.halt == Halt::VBlank {
            self.halt = Halt::Running;
        }
    }
}

/// The CPU just before an instruction, for `rollback`.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)] // the interpreter's own flags, as they are
pub struct Checkpoint {
    registers: Registers,
    nmi_pending: bool,
    irq_pending: bool,
    firq_pending: bool,
    block_continuation: Option<BlockContinuation>,
    halt: Halt,
    nmi_active: bool,
    depth: usize,
    top: Option<(Interrupt, Option<BlockContinuation>)>,
    all: Option<Vec<(Interrupt, Option<BlockContinuation>)>>,
}

impl Checkpoint {
    #[must_use]
    pub fn halt(&self) -> Halt {
        self.halt
    }
}

/// Why the CPU is halted, if it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Halt {
    /// Executing normally
    #[default]
    Running,
    /// WAIT: woken by any pending interrupt
    Interrupt,
    /// WAITV: woken by the next vertical blank
    VBlank,
}

impl<B: Bus> ReferenceInterpreter<B> {
    /// Creates a new reference interpreter with the given bus.
    #[must_use]
    pub fn new(bus: B) -> Self {
        Self {
            registers: Registers::new(),
            bus,
            nmi_pending: false,
            irq_pending: false,
            firq_pending: false,
            block_continuation: None,
            halt: Halt::Running,
            nmi_active: false,
            frames: Vec::new(),
        }
    }

    /// Puts the CPU in its reset state.
    pub fn reset(&mut self) {
        self.registers = Registers::new();
        self.registers.set_sp(p63_spec::SP_RESET);
        self.registers.set_cc(crate::Flags::F | crate::Flags::I);
        let pc = self.bus.read_u24_le(crate::Addr::new(p63_spec::VEC_RESET));
        self.registers.set_pc(pc);
        self.nmi_pending = false;
        self.irq_pending = false;
        self.firq_pending = false;
        self.block_continuation = None;
        self.halt = Halt::Running;
        self.nmi_active = false;
        self.frames.clear();
    }

    /// A copy of the CPU's whole state.
    #[must_use]
    pub fn save_state(&self) -> CpuState {
        CpuState {
            registers: self.registers.clone(),
            nmi_pending: self.nmi_pending,
            irq_pending: self.irq_pending,
            firq_pending: self.firq_pending,
            block_continuation: self.block_continuation.clone(),
            halt: self.halt,
            nmi_active: self.nmi_active,
            frames: self.frames.clone(),
        }
    }

    /// What `rollback` needs to undo the next instruction: the registers and flags, and of
    /// the interrupt frames only how deep they were and the top one (an instruction pushes
    /// or pops at most one). Much cheaper than `save_state` while handlers are running.
    #[must_use]
    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            registers: self.registers.clone(),
            nmi_pending: self.nmi_pending,
            irq_pending: self.irq_pending,
            firq_pending: self.firq_pending,
            block_continuation: self.block_continuation.clone(),
            halt: self.halt,
            nmi_active: self.nmi_active,
            depth: self.frames.len(),
            top: self.frames.last().cloned(),
            // at the limit a push drops the oldest frame, so keep them all
            all: (self.frames.len() >= MAX_FRAMES).then(|| self.frames.clone()),
        }
    }

    /// Puts the CPU back as it was at `checkpoint`.
    pub fn rollback(&mut self, c: Checkpoint) {
        self.registers = c.registers;
        self.nmi_pending = c.nmi_pending;
        self.irq_pending = c.irq_pending;
        self.firq_pending = c.firq_pending;
        self.block_continuation = c.block_continuation;
        self.halt = c.halt;
        self.nmi_active = c.nmi_active;
        if let Some(all) = c.all {
            self.frames = all;
            return;
        }
        self.frames.truncate(c.depth);
        if self.frames.len() < c.depth {
            self.frames.extend(c.top);
        } else if let (Some(top), Some(last)) = (c.top, self.frames.last_mut()) {
            *last = top;
        }
    }

    /// Puts back a state from `save_state`.
    pub fn load_state(&mut self, state: CpuState) {
        self.registers = state.registers;
        self.nmi_pending = state.nmi_pending;
        self.irq_pending = state.irq_pending;
        self.firq_pending = state.firq_pending;
        self.block_continuation = state.block_continuation;
        self.halt = state.halt;
        self.nmi_active = state.nmi_active;
        self.frames = state.frames;
    }

    /// Returns why the CPU is halted, if it is.
    #[must_use]
    pub fn halt(&self) -> Halt {
        self.halt
    }

    /// Wakes a CPU halted by WAITV. Called by the video side at the start of vertical blank.
    pub fn signal_vblank(&mut self) {
        if self.halt == Halt::VBlank {
            self.halt = Halt::Running;
        }
    }

    /// Returns whether an NMI is pending.
    #[must_use]
    pub fn nmi_pending(&self) -> bool {
        self.nmi_pending
    }

    /// Sets the NMI pending flag.
    pub fn set_nmi_pending(&mut self, pending: bool) {
        self.nmi_pending = pending;
    }

    /// Returns whether an IRQ is pending.
    #[must_use]
    pub fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    /// Sets the IRQ pending flag.
    pub fn set_irq_pending(&mut self, pending: bool) {
        self.irq_pending = pending;
    }

    /// Returns whether a FIRQ is pending.
    #[must_use]
    pub fn firq_pending(&self) -> bool {
        self.firq_pending
    }

    /// Sets the FIRQ pending flag.
    pub fn set_firq_pending(&mut self, pending: bool) {
        self.firq_pending = pending;
    }

    /// Returns a reference to the block continuation state, if any.
    #[must_use]
    pub fn block_continuation(&self) -> Option<&BlockContinuation> {
        self.block_continuation.as_ref()
    }

    /// Sets the block continuation state.
    pub fn set_block_continuation(&mut self, continuation: Option<BlockContinuation>) {
        self.block_continuation = continuation;
    }

    /// Fetches and decodes the next instruction.
    pub fn fetch_decode(&mut self) -> (crate::Opcode, u8) {
        let pc = crate::Addr::new(self.registers.pc());
        let opcode = crate::Opcode::new(self.bus.read(pc));
        let postbyte = self.bus.read(crate::Addr::new(pc.0 + 1));

        // Advance PC by 2 and mask to 24 bits
        self.registers.set_pc((pc.0 + 2) & 0x00FF_FFFF);

        (opcode, postbyte)
    }

    /// Executes exactly one instruction.
    #[allow(clippy::too_many_lines)] // one arm per opcode
    pub fn step(&mut self) -> Result<crate::Cycles, crate::CpuError> {
        if self.halt == Halt::VBlank {
            return Ok(crate::Cycles(1));
        }
        if let Some(kind) = self.pending_interrupt() {
            self.halt = Halt::Running;
            return Ok(self.service(kind));
        }
        if self.halt == Halt::Interrupt {
            // a masked interrupt still wakes WAIT, without being taken
            if !(self.nmi_pending || self.irq_pending || self.firq_pending) {
                return Ok(crate::Cycles(1));
            }
            self.halt = Halt::Running;
        }

        let (opcode, postbyte) = self.fetch_decode();

        // Dispatch to instruction handler based on opcode
        match opcode.0 {
            // Data Transfer Instructions (0x00-0x0F)
            0x00 => Ok(crate::Cycles(1)), // NOP - no operation, 1 cycle
            0x01 => self.exec_ld_a_imm(postbyte),
            0x02 => self.exec_ld_b_imm(postbyte),
            0x03 => self.exec_ld_d_imm(postbyte),
            0x04 => self.exec_ld_e_imm(postbyte),
            0x05 => self.exec_ld_f_imm(postbyte),
            0x06 => self.exec_ld_w_imm(postbyte),
            0x07 => self.exec_ld_q_imm(postbyte),
            0x08 => self.exec_mov(postbyte),
            0x09 => self.exec_st_a(postbyte),
            0x0A => self.exec_st_b(postbyte),
            0x0B => self.exec_st_d(postbyte),
            0x0C => self.exec_st_e(postbyte),
            0x0D => self.exec_st_f(postbyte),
            0x0E => self.exec_st_w(postbyte),
            0x0F => self.exec_st_q(postbyte),

            // Arithmetic Instructions (0x10-0x1F)
            0x10 => self.exec_add(postbyte),
            0x11 => self.exec_adc(postbyte),
            0x12 => self.exec_sub(postbyte),
            0x13 => self.exec_sbc(postbyte),
            0x14 => self.exec_and(postbyte),
            0x15 => self.exec_or(postbyte),
            0x16 => self.exec_xor(postbyte),
            0x17 => self.exec_not(postbyte),
            0x18 => self.exec_inc(postbyte),
            0x19 => self.exec_dec(postbyte),
            0x1A => self.exec_neg(postbyte),
            0x1B => self.exec_cmp(postbyte),
            0x1C => self.exec_mul(postbyte),
            0x1D => self.exec_div(postbyte),
            0x1E => self.exec_mod(postbyte),
            0x1F => self.exec_clr(postbyte),

            // Shift/Rotate and Extension Instructions (0x20-0x2F)
            0x30..=0x3F => self.exec_short_branch(opcode.0, postbyte),
            0x80..=0x8D => self.exec_long_branch(opcode.0, postbyte),

            0x60..=0x64 => self.exec_block(opcode.0, postbyte),

            0x50 => self.exec_pack(postbyte),
            0x51 => self.exec_unp(postbyte),
            0x52 => self.exec_bext(postbyte),
            0x53 => self.exec_bins(postbyte),
            0x54 => self.exec_lea(postbyte),
            0x5A => self.exec_ldi(postbyte),
            0x5B => self.exec_movq(postbyte),
            0x5D => self.exec_daa(postbyte),

            0x90 => self.exec_bset(postbyte),
            0x91 => self.exec_bclr(postbyte),
            0x92 => self.exec_bchg(postbyte),
            0x93 => self.exec_btst(postbyte),
            0x94 | 0x95 => self.exec_jump_on_bit(opcode.0, postbyte),
            0x96 => self.exec_dbnz(postbyte),
            0x97 | 0x98 => self.exec_test_bit_branch(opcode.0, postbyte),

            0x70..=0x75 => self.exec_flag_op(opcode.0),
            0xB0 | 0xB1 => self.exec_wait(opcode.0),
            0xCE | 0xCF => self.exec_ld_index_extended(opcode.0, postbyte),
            0xF0..=0xF3 => self.exec_interrupt_op(opcode.0),
            0xF4 => self.exec_reset(),
            0x43 => self.exec_rti(postbyte),
            0xFE | 0xFF => self.exec_prefix(postbyte),

            0x40 => self.exec_jmp(postbyte),
            0x41 => self.exec_jsr(postbyte),
            0x42 => self.exec_rts(postbyte),
            0x44 => self.exec_bsr(postbyte),
            0x45 => self.exec_lbra(postbyte),
            0x4D => self.exec_lbsr(postbyte),

            0x20 | 0x22 => self.exec_asl(postbyte),
            0x21 => self.exec_asr(postbyte),
            0x23 => self.exec_lsr(postbyte),
            0x24 => self.exec_rol(postbyte),
            0x25 => self.exec_ror(postbyte),
            0x26 => self.exec_rcl(postbyte),
            0x27 => self.exec_rcr(postbyte),
            0x28 => self.exec_tst(postbyte),
            0x29 => self.exec_bit(postbyte),
            0x2A => self.exec_com(postbyte),
            0x2B => self.exec_swap(postbyte),
            0x2C => self.exec_ext(postbyte),
            0x2D => self.exec_zext(postbyte),
            0x2E => self.exec_maq(postbyte),
            0x2F => self.exec_addz(postbyte),

            // Extended loads (0x40-0x4F range)
            0x46 => self.exec_ld_a_mem(postbyte),
            0x47 => self.exec_ld_b_mem(postbyte),
            0x48 => self.exec_ld_d_mem(postbyte),
            0x49 => self.exec_ld_e_mem(postbyte),
            0x4A => self.exec_ld_f_mem(postbyte),
            0x4B => self.exec_ld_w_mem(postbyte),
            0x4C => self.exec_ld_q_mem(postbyte),

            // Stack operations (0x56-0x59)
            0x55 => self.exec_smodx(postbyte),
            0x56 => self.exec_push(postbyte),
            0x57 => self.exec_push2(postbyte),
            0x58 => self.exec_pop(postbyte),
            0x59 => self.exec_pop2(postbyte),
            0x5C => self.exec_muld(postbyte),

            // Register exchange (0xA0)
            0xA0 => self.exec_exg(postbyte),

            // Extended addressing loads (0xC0-0xC6)
            0xC0 => self.exec_ld_a_extended(postbyte),
            0xC1 => self.exec_ld_b_extended(postbyte),
            0xC2 => self.exec_ld_d_extended(postbyte),
            0xC3 => self.exec_ld_e_extended(postbyte),
            0xC4 => self.exec_ld_f_extended(postbyte),
            0xC5 => self.exec_ld_w_extended(postbyte),
            0xC6 => self.exec_ld_q_extended(postbyte),

            // Extended addressing stores (0xC7-0xCD)
            0xC7 => self.exec_st_a_extended(postbyte),
            0xC8 => self.exec_st_b_extended(postbyte),
            0xC9 => self.exec_st_d_extended(postbyte),
            0xCA => self.exec_st_e_extended(postbyte),
            0xCB => self.exec_st_f_extended(postbyte),
            0xCC => self.exec_st_w_extended(postbyte),
            0xCD => self.exec_st_q_extended(postbyte),

            // Indexed addressing loads (0xD0-0xD6)
            0xD0 => self.exec_ld_a_indexed(postbyte),
            0xD1 => self.exec_ld_b_indexed(postbyte),
            0xD2 => self.exec_ld_d_indexed(postbyte),
            0xD3 => self.exec_ld_e_indexed(postbyte),
            0xD4 => self.exec_ld_f_indexed(postbyte),
            0xD5 => self.exec_ld_w_indexed(postbyte),
            0xD6 => self.exec_ld_q_indexed(postbyte),

            // Indexed addressing stores (0xD7-0xDD)
            0xD7 => self.exec_st_a_indexed(postbyte),
            0xD8 => self.exec_st_b_indexed(postbyte),
            0xD9 => self.exec_st_d_indexed(postbyte),
            0xDA => self.exec_st_e_indexed(postbyte),
            0xDB => self.exec_st_f_indexed(postbyte),
            0xDC => self.exec_st_w_indexed(postbyte),
            0xDD => self.exec_st_q_indexed(postbyte),

            // All other opcodes currently unimplemented
            _ => Err(crate::CpuError::IllegalOpcode(opcode)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Ram;

    #[test]
    fn test_reset_state() {
        let mut ram = Ram::new(0x0100_0000);
        for (i, b) in p63_spec::LOW_VECTOR_ROM.iter().enumerate() {
            ram.write(crate::Addr::new(i as u32), *b);
        }
        let mut interp = ReferenceInterpreter::new(ram);
        interp.registers.set_q(0xDEAD_BEEF);
        interp.set_irq_pending(true);
        interp.reset();

        assert_eq!(interp.registers.q(), 0);
        assert_eq!(interp.registers.sp(), p63_spec::SP_RESET);
        assert_eq!(interp.registers.cc().bits(), 0x50);
        // RESET -> boot ROM base (low-vector ROM bytes 1..3)
        assert_eq!(interp.registers.pc(), 0xFB_B600);
        assert!(!interp.irq_pending());
    }

    #[test]
    fn rollback_undoes_taking_an_interrupt_and_its_rti() {
        let mut ram = Ram::new(0x0100_0000);
        // IRQ vector -> $000200: RTI; the program at $000100: NOPs
        ram.write(crate::Addr::new(p63_spec::VEC_IRQ), 0x00);
        ram.write(crate::Addr::new(p63_spec::VEC_IRQ + 1), 0x02);
        ram.write(crate::Addr::new(0x200), 0x43);
        let mut cpu = ReferenceInterpreter::new(ram);
        cpu.registers.set_pc(0x100);
        cpu.registers.set_sp(0x8000);
        cpu.registers.set_cc(crate::Flags::empty());
        cpu.set_irq_pending(true);
        let before = cpu.checkpoint();
        cpu.step().unwrap();
        assert_eq!((cpu.registers.pc(), cpu.frames.len()), (0x200, 1), "the IRQ was taken");
        cpu.rollback(before);
        assert_eq!((cpu.registers.pc(), cpu.registers.sp(), cpu.frames.len()), (0x100, 0x8000, 0), "and undone");
        cpu.step().unwrap();
        cpu.set_irq_pending(false);
        let inside = cpu.checkpoint();
        cpu.step().unwrap();
        assert_eq!((cpu.registers.pc(), cpu.frames.len()), (0x100, 0), "RTI");
        cpu.rollback(inside);
        assert_eq!((cpu.registers.pc(), cpu.frames.len()), (0x200, 1), "back inside the handler");
        assert_eq!(cpu.frames[0].0, Interrupt::Irq);
    }

    #[test]
    fn test_new_interpreter() {
        let ram = Ram::new(1024);
        let interp = ReferenceInterpreter::new(ram);

        assert_eq!(interp.registers.a(), 0);
        assert_eq!(interp.registers.pc(), 0);

        assert!(!interp.nmi_pending());
        assert!(!interp.irq_pending());
        assert!(!interp.firq_pending());

        assert!(interp.block_continuation().is_none());
    }

    #[test]
    fn test_interrupt_flags() {
        let ram = Ram::new(1024);
        let mut interp = ReferenceInterpreter::new(ram);

        // Test NMI
        interp.set_nmi_pending(true);
        assert!(interp.nmi_pending());
        interp.set_nmi_pending(false);
        assert!(!interp.nmi_pending());

        // Test IRQ
        interp.set_irq_pending(true);
        assert!(interp.irq_pending());
        interp.set_irq_pending(false);
        assert!(!interp.irq_pending());

        // Test FIRQ
        interp.set_firq_pending(true);
        assert!(interp.firq_pending());
        interp.set_firq_pending(false);
        assert!(!interp.firq_pending());
    }

    #[test]
    fn test_block_continuation() {
        let ram = Ram::new(1024);
        let mut interp = ReferenceInterpreter::new(ram);

        // Initially no continuation
        assert!(interp.block_continuation().is_none());

        // Set continuation
        let continuation = BlockContinuation {
            x: 0x1000,
            y: 0x2000,
            remaining: 42,
            operation: BlockOp::MoveByte,
            pc: 0x100,
        };
        interp.set_block_continuation(Some(continuation.clone()));

        assert!(interp.block_continuation().is_some());
        let retrieved = interp.block_continuation().unwrap();
        assert_eq!(retrieved.x, 0x1000);
        assert_eq!(retrieved.y, 0x2000);
        assert_eq!(retrieved.remaining, 42);
        assert_eq!(retrieved.operation, BlockOp::MoveByte);

        // Clear continuation
        interp.set_block_continuation(None);
        assert!(interp.block_continuation().is_none());
    }

    #[test]
    fn test_block_op_variants() {
        // Test all BlockOp variants
        let ops = [
            BlockOp::MoveByte,
            BlockOp::MoveWord,
            BlockOp::Fill,
            BlockOp::Compare,
            BlockOp::ScanByte,
        ];

        for op in &ops {
            let continuation = BlockContinuation {
                x: 0,
                y: 0,
                remaining: 0,
                operation: *op,
                pc: 0,
            };
            assert_eq!(continuation.operation, *op);
        }
    }

    #[test]
    fn test_fetch_decode_basic() {
        use crate::{Addr, Opcode};

        let mut ram = Ram::new(1024);
        // Write test instruction bytes
        ram.write(Addr::new(0), 0x01); // opcode
        ram.write(Addr::new(1), 0x42); // postbyte

        let mut interp = ReferenceInterpreter::new(ram);

        // Initial PC should be 0
        assert_eq!(interp.registers.pc(), 0);

        // Fetch and decode
        let (opcode, postbyte) = interp.fetch_decode();

        assert_eq!(opcode, Opcode::new(0x01));
        assert_eq!(postbyte, 0x42);

        assert_eq!(interp.registers.pc(), 2);
    }

    #[test]
    fn test_fetch_decode_pc_advance() {
        use crate::{Addr, Opcode};

        let mut ram = Ram::new(1024);
        // Write multiple instructions
        ram.write(Addr::new(0x100), 0xAA);
        ram.write(Addr::new(0x101), 0xBB);
        ram.write(Addr::new(0x102), 0xCC);
        ram.write(Addr::new(0x103), 0xDD);

        let mut interp = ReferenceInterpreter::new(ram);
        interp.registers.set_pc(0x100);

        // First fetch
        let (opcode1, postbyte1) = interp.fetch_decode();
        assert_eq!(opcode1, Opcode::new(0xAA));
        assert_eq!(postbyte1, 0xBB);
        assert_eq!(interp.registers.pc(), 0x102);

        // Second fetch
        let (opcode2, postbyte2) = interp.fetch_decode();
        assert_eq!(opcode2, Opcode::new(0xCC));
        assert_eq!(postbyte2, 0xDD);
        assert_eq!(interp.registers.pc(), 0x104);
    }

    #[test]
    fn test_fetch_decode_24bit_wraparound() {
        use crate::{Addr, Opcode};

        let mut ram = Ram::new(16 * 1024 * 1024); // 16 MB
        // Write instruction at the very end of 24-bit address space
        ram.write(Addr::new(0x00FF_FFFE), 0x55);
        ram.write(Addr::new(0x00FF_FFFF), 0x66);

        let mut interp = ReferenceInterpreter::new(ram);
        interp.registers.set_pc(0x00FF_FFFE);

        // Fetch at boundary
        let (opcode, postbyte) = interp.fetch_decode();
        assert_eq!(opcode, Opcode::new(0x55));
        assert_eq!(postbyte, 0x66);

        // PC should wrap to 0 (0x00FF_FFFF + 2 = 0x0100_0001, masked to 0x0000_0001)
        assert_eq!(interp.registers.pc(), 0x0000_0000);
    }

    #[test]
    fn test_fetch_decode_preserves_other_state() {
        use crate::Addr;

        let mut ram = Ram::new(1024);
        ram.write(Addr::new(0), 0x12);
        ram.write(Addr::new(1), 0x34);

        let mut interp = ReferenceInterpreter::new(ram);

        // Set some state
        interp.registers.set_a(0xAA);
        interp.registers.set_x(0x0012_3456);
        interp.set_nmi_pending(true);

        // Fetch
        let (_opcode, _postbyte) = interp.fetch_decode();

        assert_eq!(interp.registers.a(), 0xAA);
        assert_eq!(interp.registers.x(), 0x0012_3456);
        assert!(interp.nmi_pending());
    }

    #[test]
    fn test_step_returns_illegal_opcode_for_unimplemented() {
        use crate::{Addr, CpuError, Opcode};

        let mut ram = Ram::new(1024);
        // 0x4E is illegal
        ram.write(Addr::new(0), 0x4E);
        ram.write(Addr::new(1), 0x00);

        let mut interp = ReferenceInterpreter::new(ram);

        // Step should return IllegalOpcode error
        let result = interp.step();
        assert!(result.is_err());

        match result {
            Err(CpuError::IllegalOpcode(opcode)) => {
                assert_eq!(opcode, Opcode::new(0x4E));
            }
            _ => panic!("Expected IllegalOpcode error"),
        }

        // PC should have been advanced by fetch_decode
        assert_eq!(interp.registers.pc(), 2);
    }

    #[test]
    fn test_step_advances_pc() {
        use crate::Addr;

        let mut ram = Ram::new(1024);
        ram.write(Addr::new(0x100), 0x4E);
        ram.write(Addr::new(0x101), 0xAA);

        let mut interp = ReferenceInterpreter::new(ram);
        interp.registers.set_pc(0x100);

        // Execute step (will fail with IllegalOpcode, but PC should advance)
        let _result = interp.step();

        // PC should be at 0x102 after fetch_decode
        assert_eq!(interp.registers.pc(), 0x102);
    }

    #[test]
    fn test_step_preserves_other_state_on_error() {
        use crate::Addr;

        let mut ram = Ram::new(1024);
        ram.write(Addr::new(0), 0x99);
        ram.write(Addr::new(1), 0x88);

        let mut interp = ReferenceInterpreter::new(ram);

        // Set some state
        interp.registers.set_a(0x55);
        interp.registers.set_b(0xAA);
        interp.registers.set_x(0x0012_3456);
        interp.registers.set_cc(crate::Flags::I);
        interp.set_irq_pending(true);

        // Execute step (will error)
        let _result = interp.step();

        // All state except PC should be preserved
        assert_eq!(interp.registers.a(), 0x55);
        assert_eq!(interp.registers.b(), 0xAA);
        assert_eq!(interp.registers.x(), 0x0012_3456);
        assert!(interp.irq_pending());
    }

    #[test]
    fn test_illegal_opcodes_return_error_without_panic() {
        use crate::{Addr, CpuError, Opcode};

        let illegal_opcodes = p63_spec::OPCODES.iter().filter(|o| !o.legal).map(|o| o.opcode);

        for opcode_byte in illegal_opcodes {
            let mut ram = Ram::new(1024);
            ram.write(Addr::new(0), opcode_byte);
            ram.write(Addr::new(1), 0x00);

            let mut interp = ReferenceInterpreter::new(ram);

            // Step should return IllegalOpcode error, not panic
            let result = interp.step();
            assert!(result.is_err(), "Opcode 0x{opcode_byte:02X} should return error");

            match result {
                Err(CpuError::IllegalOpcode(opcode)) => {
                    assert_eq!(opcode, Opcode::new(opcode_byte),
                               "Error should contain correct opcode for 0x{opcode_byte:02X}");
                }
                Err(e) => panic!("Expected IllegalOpcode for 0x{opcode_byte:02X}, got: {e:?}"),
                Ok(_) => panic!("Opcode 0x{opcode_byte:02X} should not succeed"),
            }
        }
    }

    #[test]
    fn test_nop_does_not_return_illegal_opcode() {
        use crate::{Addr, CpuError};

        // Test that NOP (0x00) is implemented and doesn't return IllegalOpcode
        let mut ram = Ram::new(1024);
        ram.write(Addr::new(0), 0x00);
        ram.write(Addr::new(1), 0x00);

        let mut interp = ReferenceInterpreter::new(ram);

        let result = interp.step();
        // NOP should succeed
        assert!(result.is_ok(), "NOP should not return an error");
        if let Err(CpuError::IllegalOpcode(_)) = result {
            panic!("NOP (0x00) should not return IllegalOpcode error");
        }
    }
}
