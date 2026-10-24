//! Arithmetic and logic instruction handlers.

use super::operand::{alu_dest_bits, reg_bits, width_mask, REG_X, SRC_IMM};
use super::ReferenceInterpreter;
use crate::{Addr, Bus, CpuError, Cycles, Flags};

/// An illegal ALU postbyte runs as a 1-cycle NOP.
const ILLEGAL: Result<Cycles, CpuError> = Ok(Cycles(1));

/// Two-operand ALU operations.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Alu {
    Add,
    Adc,
    Sub,
    Sbc,
    And,
    Or,
    Xor,
    Cmp,
    Bit,
}

/// Single-register operations.
#[derive(Clone, Copy)]
enum Unary {
    Not,
    Neg,
    Clr,
}

fn sign_bit(bits: u32) -> u32 {
    1 << (bits - 1)
}

/// Sign-extends the low `bits` of `v` to 32 bits.
fn sext(v: u32, bits: u32) -> u32 {
    if bits >= 32 {
        return v;
    }
    let s = sign_bit(bits);
    ((v & width_mask(bits)) ^ s).wrapping_sub(s)
}

impl<B: Bus> ReferenceInterpreter<B> {
    /// Sets N and Z for a result of `bits` width.
    fn set_nz(flags: &mut Flags, result: u32, bits: u32) {
        flags.set(Flags::N, result & sign_bit(bits) != 0);
        flags.set(Flags::Z, result == 0);
    }

    /// Updates N, Z, V and C for 8-bit division and modulo results.
    fn update_div_flags_8(&mut self, result: u8, remainder_nonzero: bool) {
        let mut flags = self.registers.cc();
        Self::set_nz(&mut flags, u32::from(result), 8);
        flags.set(Flags::V, false);
        flags.set(Flags::C, remainder_nonzero);
        self.registers.set_cc(flags);
    }

    /// Two-operand ALU instruction, `[src][dest]` postbyte.
    fn exec_alu(&mut self, op: Alu, postbyte: u8) -> Result<Cycles, CpuError> {
        let dest = postbyte & 0x0F;
        let Some(bits) = alu_dest_bits(dest) else { return ILLEGAL };
        let Some((s, cycles)) = self.alu_source(postbyte >> 4, bits) else { return ILLEGAL };
        let d = self.read_register(dest)?;
        let mask = width_mask(bits);
        let sign = sign_bit(bits);
        let mut flags = self.registers.cc();
        let carry_in = u32::from(flags.contains(Flags::C));

        let result = match op {
            Alu::Add | Alu::Adc => {
                let cin = if op == Alu::Adc { carry_in } else { 0 };
                let wide = u64::from(d) + u64::from(s) + u64::from(cin);
                let r = (wide as u32) & mask;
                flags.set(Flags::C, wide > u64::from(mask));
                flags.set(Flags::V, (!(d ^ s) & (d ^ r)) & sign != 0);
                flags.set(Flags::H, (d & 0xF) + (s & 0xF) + cin > 0xF);
                r
            }
            Alu::Sub | Alu::Sbc | Alu::Cmp => {
                let cin = if op == Alu::Sbc { carry_in } else { 0 };
                let r = d.wrapping_sub(s).wrapping_sub(cin) & mask;
                flags.set(Flags::C, u64::from(d) < u64::from(s) + u64::from(cin));
                flags.set(Flags::V, ((d ^ s) & (d ^ r)) & sign != 0);
                flags.set(Flags::H, (d & 0xF) < (s & 0xF) + cin);
                r
            }
            Alu::And | Alu::Or | Alu::Xor | Alu::Bit => {
                flags.set(Flags::V, false);
                flags.set(Flags::C, false);
                match op {
                    Alu::And | Alu::Bit => d & s,
                    Alu::Or => d | s,
                    _ => d ^ s,
                }
            }
        };
        Self::set_nz(&mut flags, result, bits);
        self.registers.set_cc(flags);
        if !matches!(op, Alu::Cmp | Alu::Bit) {
            self.write_register(dest, result)?;
        }
        Ok(Cycles(cycles))
    }

    /// Single-register instruction, `[0000][reg]` postbyte.
    fn exec_unary(&mut self, op: Unary, postbyte: u8) -> Result<Cycles, CpuError> {
        let reg = postbyte & 0x0F;
        if postbyte >> 4 != 0 {
            return ILLEGAL;
        }
        let Some(bits) = alu_dest_bits(reg) else { return ILLEGAL };
        let d = self.read_register(reg)?;
        let mask = width_mask(bits);
        let mut flags = self.registers.cc();
        let result = match op {
            Unary::Not => !d & mask,
            Unary::Neg => 0u32.wrapping_sub(d) & mask,
            Unary::Clr => 0,
        };
        match op {
            // NEG: V if the input was the most negative value, C if the result is non-zero
            Unary::Neg => {
                flags.set(Flags::V, d == sign_bit(bits));
                flags.set(Flags::C, result != 0);
            }
            Unary::Not | Unary::Clr => {
                flags.set(Flags::V, false);
                flags.set(Flags::C, false);
            }
        }
        Self::set_nz(&mut flags, result, bits);
        self.registers.set_cc(flags);
        self.write_register(reg, result)?;
        Ok(Cycles(1))
    }

    /// INC/DEC, `[count][reg]` postbyte. C is unaffected.
    fn exec_inc_dec(&mut self, inc: bool, postbyte: u8) -> Result<Cycles, CpuError> {
        let reg = postbyte & 0x0F;
        let Some(bits) = alu_dest_bits(reg) else { return ILLEGAL };
        let (count, cycles) = match postbyte >> 4 {
            0 => (1, 1),
            SRC_IMM => match self.fetch_u8() {
                0 => return ILLEGAL,
                c => (u32::from(c), 1 + u32::from(c)),
            },
            _ => return ILLEGAL,
        };
        let d = self.read_register(reg)?;
        let mask = width_mask(bits);
        let sign = sign_bit(bits);
        let s = count & mask;
        let (result, overflow) = if inc {
            let r = d.wrapping_add(s) & mask;
            (r, (!(d ^ s) & (d ^ r)) & sign != 0)
        } else {
            let r = d.wrapping_sub(s) & mask;
            (r, ((d ^ s) & (d ^ r)) & sign != 0)
        };
        let mut flags = self.registers.cc();
        Self::set_nz(&mut flags, result, bits);
        flags.set(Flags::V, overflow);
        self.registers.set_cc(flags);
        self.write_register(reg, result)?;
        Ok(Cycles(cycles))
    }

    /// Decodes the signed-mode postbyte of MUL/DIV/MOD/MULD.
    fn signed_mode(postbyte: u8) -> Option<bool> {
        match postbyte {
            0x00 => Some(false),
            0x01 => Some(true),
            _ => None,
        }
    }

    // Instruction handlers - Arithmetic group (opcodes 0x10-0x1F)

    /// ADD dest,src (opcode 0x10)
    pub(super) fn exec_add(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_alu(Alu::Add, postbyte)
    }

    /// ADC dest,src (opcode 0x11)
    pub(super) fn exec_adc(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_alu(Alu::Adc, postbyte)
    }

    /// SUB dest,src (opcode 0x12)
    pub(super) fn exec_sub(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_alu(Alu::Sub, postbyte)
    }

    /// SBC dest,src (opcode 0x13)
    pub(super) fn exec_sbc(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_alu(Alu::Sbc, postbyte)
    }

    /// AND dest,src (opcode 0x14)
    pub(super) fn exec_and(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_alu(Alu::And, postbyte)
    }

    /// OR dest,src (opcode 0x15)
    pub(super) fn exec_or(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_alu(Alu::Or, postbyte)
    }

    /// XOR dest,src (opcode 0x16)
    pub(super) fn exec_xor(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_alu(Alu::Xor, postbyte)
    }

    /// NOT reg (opcode 0x17)
    pub(super) fn exec_not(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_unary(Unary::Not, postbyte)
    }

    /// INC reg[,#count] (opcode 0x18)
    pub(super) fn exec_inc(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_inc_dec(true, postbyte)
    }

    /// DEC reg[,#count] (opcode 0x19)
    pub(super) fn exec_dec(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_inc_dec(false, postbyte)
    }

    /// NEG reg (opcode 0x1A)
    pub(super) fn exec_neg(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_unary(Unary::Neg, postbyte)
    }

    /// CMP dest,src (opcode 0x1B)
    pub(super) fn exec_cmp(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_alu(Alu::Cmp, postbyte)
    }

    /// BIT dest,src — AND that only sets flags (opcode 0x29)
    pub(super) fn exec_bit(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_alu(Alu::Bit, postbyte)
    }

    /// CLR reg (opcode 0x1F)
    pub(super) fn exec_clr(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        self.exec_unary(Unary::Clr, postbyte)
    }

    /// MUL A,B[,S] — A × B → D (opcode 0x1C, 2 cycles). C if the product doesn't fit in 8 bits.
    pub(super) fn exec_mul(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let Some(signed) = Self::signed_mode(postbyte) else { return ILLEGAL };
        let (a, b) = (self.registers.a(), self.registers.b());
        let (result, overflow) = if signed {
            let p = i16::from(a as i8) * i16::from(b as i8);
            (p as u16, !(-128..=127).contains(&p))
        } else {
            let p = u16::from(a) * u16::from(b);
            (p, p > 0xFF)
        };
        self.registers.set_d(result);
        let mut flags = self.registers.cc();
        Self::set_nz(&mut flags, u32::from(result), 16);
        flags.set(Flags::V, false);
        flags.set(Flags::C, overflow);
        self.registers.set_cc(flags);
        Ok(Cycles(2))
    }

    /// DIV D,E[,S] — quotient → A, remainder → B (opcode 0x1D, 5 cycles).
    /// Divide by zero: A = 0xFF, B = low byte of D, no trap.
    pub(super) fn exec_div(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let Some(signed) = Self::signed_mode(postbyte) else { return ILLEGAL };
        let dividend = self.registers.d();
        let divisor = self.registers.e();
        if divisor == 0 {
            self.registers.set_a(0xFF);
            self.registers.set_b(dividend as u8);
            self.update_div_flags_8(0xFF, false);
            return Ok(Cycles(5));
        }
        let (q, r) = if signed {
            let (n, m) = (i32::from(dividend as i16), i32::from(divisor as i8));
            ((n / m) as u8, (n % m) as u8)
        } else {
            let m = u16::from(divisor);
            ((dividend / m) as u8, (dividend % m) as u8)
        };
        self.registers.set_a(q);
        self.registers.set_b(r);
        self.update_div_flags_8(q, r != 0);
        Ok(Cycles(5))
    }

    /// MOD D,E[,S] — remainder → A (opcode 0x1E, 5 cycles). Modulo by zero: A = low byte of D.
    pub(super) fn exec_mod(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let Some(signed) = Self::signed_mode(postbyte) else { return ILLEGAL };
        let dividend = self.registers.d();
        let divisor = self.registers.e();
        let r = if divisor == 0 {
            dividend as u8
        } else if signed {
            (i32::from(dividend as i16) % i32::from(divisor as i8)) as u8
        } else {
            (dividend % u16::from(divisor)) as u8
        };
        self.registers.set_a(r);
        self.update_div_flags_8(r, false);
        Ok(Cycles(5))
    }

    // Extension and specialized arithmetic (opcodes 0x2C-0x2F, 0x5C, 0x55)

    /// EXT A,D — Sign-extend A into D (opcode 0x2C, 1 cycle). N and Z only.
    pub(super) fn exec_ext(&mut self, _postbyte: u8) -> Result<Cycles, CpuError> {
        let result = sext(u32::from(self.registers.a()), 8) & 0xFFFF;
        self.registers.set_d(result as u16);
        let mut flags = self.registers.cc();
        Self::set_nz(&mut flags, result, 16);
        self.registers.set_cc(flags);
        Ok(Cycles(1))
    }

    /// ZEXT A,D — Zero-extend A into D (opcode 0x2D, 1 cycle).
    pub(super) fn exec_zext(&mut self, _postbyte: u8) -> Result<Cycles, CpuError> {
        let result = u16::from(self.registers.a());
        self.registers.set_d(result);
        let mut flags = self.registers.cc();
        Self::set_nz(&mut flags, u32::from(result), 16);
        flags.set(Flags::V, false);
        flags.set(Flags::C, false);
        self.registers.set_cc(flags);
        Ok(Cycles(1))
    }

    /// MAQ D,W — Q = Q + D × W (opcode 0x2E, 2 cycles).
    pub(super) fn exec_maq(&mut self, _postbyte: u8) -> Result<Cycles, CpuError> {
        let q = self.registers.q();
        let product = u32::from(self.registers.d()) * u32::from(self.registers.w());
        let (result, carry) = q.overflowing_add(product);
        self.registers.set_q(result);
        let mut flags = self.registers.cc();
        Self::set_nz(&mut flags, result, 32);
        flags.set(Flags::V, (!(q ^ product) & (q ^ result)) & 0x8000_0000 != 0);
        flags.set(Flags::C, carry);
        self.registers.set_cc(flags);
        Ok(Cycles(2))
    }

    /// ADDZ addr,#imm — add an 8-bit immediate to memory (opcode 0x2F, 5 cycles).
    /// Postbyte and the next two bytes are the 24-bit address; the immediate follows.
    pub(super) fn exec_addz(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let addr = u32::from(postbyte) | (self.fetch_le(2) << 8);
        let imm = self.fetch_u8();
        let m = self.bus.read(Addr::new(addr));
        let wide = u16::from(m) + u16::from(imm);
        let result = wide as u8;
        self.bus.write(Addr::new(addr), result);
        let mut flags = self.registers.cc();
        Self::set_nz(&mut flags, u32::from(result), 8);
        flags.set(Flags::V, (!(m ^ imm) & (m ^ result)) & 0x80 != 0);
        flags.set(Flags::C, wide > 0xFF);
        flags.set(Flags::H, (m & 0xF) + (imm & 0xF) > 0xF);
        self.registers.set_cc(flags);
        Ok(Cycles(5))
    }

    /// MULD D,W[,S] — D × W → Q (opcode 0x5C, 2 cycles). C if the product doesn't fit in 16 bits.
    pub(super) fn exec_muld(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        let Some(signed) = Self::signed_mode(postbyte) else { return ILLEGAL };
        let (d, w) = (self.registers.d(), self.registers.w());
        let (result, overflow) = if signed {
            let p = i32::from(d as i16) * i32::from(w as i16);
            (p as u32, !(-32768..=32767).contains(&p))
        } else {
            let p = u32::from(d) * u32::from(w);
            (p, p > 0xFFFF)
        };
        self.registers.set_q(result);
        let mut flags = self.registers.cc();
        Self::set_nz(&mut flags, result, 32);
        flags.set(Flags::V, false);
        flags.set(Flags::C, overflow);
        self.registers.set_cc(flags);
        Ok(Cycles(2))
    }

    /// SMODX X,src — add a sign-extended register or 24-bit immediate to X
    /// (opcode 0x55, `[src][1000]` postbyte, 1 cycle register / 2 immediate). C unaffected.
    pub(super) fn exec_smodx(&mut self, postbyte: u8) -> Result<Cycles, CpuError> {
        if postbyte & 0x0F != REG_X {
            return ILLEGAL;
        }
        let (offset, cycles) = match postbyte >> 4 {
            src @ 0x0..=0x7 => {
                // Q contributes its low 24 bits
                let bits = reg_bits(src).map_or(8, |b| b.min(24));
                (sext(self.read_register(src)?, bits), 1)
            }
            SRC_IMM => (sext(self.fetch_le(3), 24), 2),
            _ => return ILLEGAL,
        };
        let x = self.registers.x();
        let result = x.wrapping_add(offset) & 0x00FF_FFFF;
        self.registers.set_x(result);
        let mut flags = self.registers.cc();
        Self::set_nz(&mut flags, result, 24);
        flags.set(Flags::V, (!(x ^ offset) & (x ^ result)) & 0x0080_0000 != 0);
        self.registers.set_cc(flags);
        Ok(Cycles(cycles))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Ram;

    #[test]
    fn test_add_basic() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(10);
        interp.registers.set_b(20);

        let result = interp.exec_add(0x10);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(1));
        assert_eq!(interp.registers.a(), 30);

        // Flags: N=0, Z=0, V=0, C=0
        assert!(!interp.registers.cc().contains(Flags::N));
        assert!(!interp.registers.cc().contains(Flags::Z));
        assert!(!interp.registers.cc().contains(Flags::V));
        assert!(!interp.registers.cc().contains(Flags::C));
    }

    #[test]
    fn test_add_carry() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(0xFF);
        interp.registers.set_b(0x01);

        let result = interp.exec_add(0x10);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 0x00);

        // Flags: N=0, Z=1, V=0, C=1
        assert!(!interp.registers.cc().contains(Flags::N));
        assert!(interp.registers.cc().contains(Flags::Z));
        assert!(!interp.registers.cc().contains(Flags::V));
        assert!(interp.registers.cc().contains(Flags::C));
    }

    #[test]
    fn test_add_overflow() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // 0x7F + 0x01 = 0x80 (overflow from positive to negative)
        interp.registers.set_a(0x7F);
        interp.registers.set_b(0x01);

        let result = interp.exec_add(0x10);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 0x80);

        // Flags: N=1, Z=0, V=1, C=0
        assert!(interp.registers.cc().contains(Flags::N));
        assert!(!interp.registers.cc().contains(Flags::Z));
        assert!(interp.registers.cc().contains(Flags::V));
        assert!(!interp.registers.cc().contains(Flags::C));
    }

    #[test]
    fn test_adc_with_carry() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // Set carry flag
        let mut flags = interp.registers.cc();
        flags.insert(Flags::C);
        interp.registers.set_cc(flags);

        interp.registers.set_a(10);
        interp.registers.set_b(20);

        let result = interp.exec_adc(0x10);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 31); // 10 + 20 + 1
    }

    #[test]
    fn test_sub_basic() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(30);
        interp.registers.set_b(10);

        let result = interp.exec_sub(0x10);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 20);

        // Flags: N=0, Z=0, V=0, C=0 (no borrow)
        assert!(!interp.registers.cc().contains(Flags::C));
    }

    #[test]
    fn test_sub_borrow() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(10);
        interp.registers.set_b(20);

        let result = interp.exec_sub(0x10);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 246); // 10 - 20 = -10 = 0xF6

        // Flags: C=1 (borrow)
        assert!(interp.registers.cc().contains(Flags::C));
    }

    #[test]
    fn test_mul_basic() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(5);
        interp.registers.set_b(6);

        let result = interp.exec_mul(0x00);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(2));
        assert_eq!(interp.registers.d(), 30);

        // C=0 (no overflow)
        assert!(!interp.registers.cc().contains(Flags::C));
    }

    #[test]
    fn test_mul_overflow() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(200);
        interp.registers.set_b(2);

        let result = interp.exec_mul(0x00);
        assert!(result.is_ok());
        assert_eq!(interp.registers.d(), 400);

        // C=1 (result > 255)
        assert!(interp.registers.cc().contains(Flags::C));
    }

    #[test]
    fn test_div_basic() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));
        interp.registers.set_d(1000);
        interp.registers.set_e(7);

        let result = interp.exec_div(0x00);
        assert_eq!(result.unwrap(), Cycles(5));
        assert_eq!(interp.registers.a(), 142); // 1000 / 7
        assert_eq!(interp.registers.b(), 6); // 1000 % 7
        assert!(interp.registers.cc().contains(Flags::C)); // remainder != 0
    }

    #[test]
    fn test_div_by_zero() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));
        interp.registers.set_d(0x0123);
        interp.registers.set_e(0);

        let result = interp.exec_div(0x00);
        assert!(result.is_ok()); // Not an error!
        assert_eq!(interp.registers.a(), 0xFF); // Defined result
        assert_eq!(interp.registers.b(), 0x23); // Low byte of the dividend

        assert!(!interp.registers.cc().contains(Flags::V));
        assert!(!interp.registers.cc().contains(Flags::C));
    }

    #[test]
    fn test_mod_basic() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));
        interp.registers.set_d(300);
        interp.registers.set_e(44);

        let result = interp.exec_mod(0x00);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 36); // 300 % 44
    }

    #[test]
    fn test_mod_by_zero() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));
        interp.registers.set_d(0x0123);
        interp.registers.set_e(0);

        let result = interp.exec_mod(0x00);
        assert!(result.is_ok()); // Not an error!
        assert_eq!(interp.registers.a(), 0x23); // Low byte of the dividend
    }

    #[test]
    fn test_inc_basic() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(42);

        let result = interp.exec_inc(0x00);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 43);

        // V=0 (no overflow)
        assert!(!interp.registers.cc().contains(Flags::V));
    }

    #[test]
    fn test_inc_overflow() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(0x7F);

        let result = interp.exec_inc(0x00);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 0x80);

        // V=1 (signed overflow)
        assert!(interp.registers.cc().contains(Flags::V));
    }

    #[test]
    fn test_dec_basic() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(42);

        let result = interp.exec_dec(0x00);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 41);

        // V=0 (no underflow)
        assert!(!interp.registers.cc().contains(Flags::V));
    }

    #[test]
    fn test_dec_underflow() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(0x80);

        let result = interp.exec_dec(0x00);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 0x7F);

        // V=1 (signed underflow)
        assert!(interp.registers.cc().contains(Flags::V));
    }

    #[test]
    fn test_neg_positive() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(5);

        let result = interp.exec_neg(0x00);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 251); // -5 in two's complement = 0xFB

        // C=1 (result != 0)
        assert!(interp.registers.cc().contains(Flags::C));
    }

    #[test]
    fn test_neg_zero() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(0);

        let result = interp.exec_neg(0x00);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 0);

        // C=0 (result == 0)
        assert!(!interp.registers.cc().contains(Flags::C));
    }

    #[test]
    fn test_neg_min_int() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(0x80); // -128

        let result = interp.exec_neg(0x00);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 0x80); // Still -128

        // V=1 (overflow: -128 negated is still -128)
        assert!(interp.registers.cc().contains(Flags::V));
    }

    #[test]
    fn test_cmp_equal() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(42);
        interp.registers.set_b(42);

        let result = interp.exec_cmp(0x10);
        assert!(result.is_ok());

        // A unchanged
        assert_eq!(interp.registers.a(), 42);

        // Z=1 (equal)
        assert!(interp.registers.cc().contains(Flags::Z));
    }

    #[test]
    fn test_and() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(0b1111_0000);
        interp.registers.set_b(0b1010_1010);

        let result = interp.exec_and(0x10);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 0b1010_0000);
    }

    #[test]
    fn test_or() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(0b1111_0000);
        interp.registers.set_b(0b1010_1010);

        let result = interp.exec_or(0x10);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 0b1111_1010);
    }

    #[test]
    fn test_xor() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(0b1111_0000);
        interp.registers.set_b(0b1010_1010);

        let result = interp.exec_xor(0x10);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 0b0101_1010);
    }

    #[test]
    fn test_not() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(0b1010_1010);

        let result = interp.exec_not(0x00);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 0b0101_0101);
    }

    #[test]
    fn test_clr() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(0xFF);

        let result = interp.exec_clr(0x00);
        assert!(result.is_ok());
        assert_eq!(interp.registers.a(), 0);

        // Flags: N=0, Z=1, V=0, C=0
        assert!(!interp.registers.cc().contains(Flags::N));
        assert!(interp.registers.cc().contains(Flags::Z));
        assert!(!interp.registers.cc().contains(Flags::V));
        assert!(!interp.registers.cc().contains(Flags::C));
    }

    #[test]
    fn test_ext_positive() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(0x42); // Positive (bit 7 = 0)

        let result = interp.exec_ext(0x00);
        assert!(result.is_ok());
        assert_eq!(interp.registers.d(), 0x0042); // High byte = 0x00
    }

    #[test]
    fn test_ext_negative() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(0x80); // Negative (bit 7 = 1)

        let result = interp.exec_ext(0x00);
        assert!(result.is_ok());
        assert_eq!(interp.registers.d(), 0xFF80); // High byte = 0xFF
    }

    #[test]
    fn test_zext() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_a(0xFF);

        let result = interp.exec_zext(0x00);
        assert!(result.is_ok());
        assert_eq!(interp.registers.d(), 0x00FF); // High byte = 0x00
    }

    #[test]
    fn test_muld() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_d(1000);
        interp.registers.set_w(2000);

        let result = interp.exec_muld(0x00);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Cycles(2));
        assert_eq!(interp.registers.q(), 2_000_000);

        // C=1 (result > 0xFFFF)
        assert!(interp.registers.cc().contains(Flags::C));
    }

    #[test]
    fn test_maq() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        // MAQ multiplies D * W and adds to Q
        // Since Q = D:W (composite), setting them has interactions
        // Q = 0x0001_0064 means D = 0x0001, W = 0x0064 (100 decimal)
        // MAQ will compute Q + (D * W) = 0x0001_0064 + (1 * 100) = 0x0001_0064 + 0x64 = 0x0001_00C8
        interp.registers.set_q(0x0001_0064);

        let result = interp.exec_maq(0x00);
        assert!(result.is_ok());

        // Q should now be 0x0001_0064 + (0x0001 * 0x0064)
        // = 0x0001_0064 + 0x0000_0064 = 0x0001_00C8 (65736 decimal)
        assert_eq!(interp.registers.q(), 0x0001_00C8);
    }

    #[test]
    fn test_smodx_positive_offset() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_x(0x0000_1000);
        interp.registers.set_a(0x10); // Positive offset

        let result = interp.exec_smodx(0x08);
        assert!(result.is_ok());
        assert_eq!(interp.registers.x(), 0x0000_1010);
    }

    #[test]
    fn test_smodx_negative_offset() {
        let mut interp = ReferenceInterpreter::new(Ram::new(1024));

        interp.registers.set_x(0x0000_1000);
        interp.registers.set_a(0xF0); // -16 in two's complement

        let result = interp.exec_smodx(0x08);
        assert!(result.is_ok());
        assert_eq!(interp.registers.x(), 0x0000_0FF0);
    }
}
