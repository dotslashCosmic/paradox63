//! CPU Register state

use crate::flags::Flags;

/// Complete CPU register state for the Paradox63
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Registers {
    // 8-bit accumulators
    a: u8,
    b: u8,
    e: u8,
    f: u8,
    gp: u8,

    // 24-bit index/pointer registers (stored as u32, upper byte masked)
    x: u32,
    y: u32,
    sp: u32,
    pc: u32,

    // Control registers
    dp: u8,
    cc: Flags,
}

impl Registers {
    /// Create a new Registers instance with all fields set to zero
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // 8-bit register accessors

    #[must_use]
    pub fn a(&self) -> u8 {
        self.a
    }

    pub fn set_a(&mut self, val: u8) {
        self.a = val;
    }

    #[must_use]
    pub fn b(&self) -> u8 {
        self.b
    }

    pub fn set_b(&mut self, val: u8) {
        self.b = val;
    }

    #[must_use]
    pub fn e(&self) -> u8 {
        self.e
    }

    pub fn set_e(&mut self, val: u8) {
        self.e = val;
    }

    #[must_use]
    pub fn f(&self) -> u8 {
        self.f
    }

    pub fn set_f(&mut self, val: u8) {
        self.f = val;
    }

    #[must_use]
    pub fn gp(&self) -> u8 {
        self.gp
    }

    pub fn set_gp(&mut self, val: u8) {
        self.gp = val;
    }

    #[must_use]
    pub fn dp(&self) -> u8 {
        self.dp
    }

    pub fn set_dp(&mut self, val: u8) {
        self.dp = val;
    }

    // 24-bit register accessors with masking

    #[must_use]
    pub fn x(&self) -> u32 {
        self.x
    }

    pub fn set_x(&mut self, val: u32) {
        self.x = val & 0x00FF_FFFF;
    }

    #[must_use]
    pub fn y(&self) -> u32 {
        self.y
    }

    pub fn set_y(&mut self, val: u32) {
        self.y = val & 0x00FF_FFFF;
    }

    #[must_use]
    pub fn sp(&self) -> u32 {
        self.sp
    }

    pub fn set_sp(&mut self, val: u32) {
        self.sp = val & 0x00FF_FFFF;
    }

    #[must_use]
    pub fn pc(&self) -> u32 {
        self.pc
    }

    pub fn set_pc(&mut self, val: u32) {
        self.pc = val & 0x00FF_FFFF;
    }

    // Condition code (flags) accessors

    #[must_use]
    pub fn cc(&self) -> Flags {
        self.cc
    }

    pub fn set_cc(&mut self, val: Flags) {
        self.cc = val;
    }

    // Composite 16-bit register: D = A:B

    /// Get the 16-bit D register (A:B composition)
    #[must_use]
    pub fn d(&self) -> u16 {
        u16::from(self.a) << 8 | u16::from(self.b)
    }

    /// Set the 16-bit D register (A:B composition)
    #[allow(clippy::cast_possible_truncation)]
    pub fn set_d(&mut self, val: u16) {
        self.a = (val >> 8) as u8;
        self.b = val as u8;
    }

    // Composite 16-bit register: W = E:F

    /// Get the 16-bit W register (E:F composition)
    #[must_use]
    pub fn w(&self) -> u16 {
        u16::from(self.e) << 8 | u16::from(self.f)
    }

    /// Set the 16-bit W register (E:F composition)
    #[allow(clippy::cast_possible_truncation)]
    pub fn set_w(&mut self, val: u16) {
        self.e = (val >> 8) as u8;
        self.f = val as u8;
    }

    // Composite 32-bit register: Q = D:W

    /// Get the 32-bit Q register (D:W composition)
    #[must_use]
    pub fn q(&self) -> u32 {
        u32::from(self.d()) << 16 | u32::from(self.w())
    }

    /// Set the 32-bit Q register (D:W composition)
    #[allow(clippy::cast_possible_truncation)]
    pub fn set_q(&mut self, val: u32) {
        self.set_d((val >> 16) as u16);
        self.set_w(val as u16);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_registers() {
        let regs = Registers::default();
        assert_eq!(regs.a(), 0);
        assert_eq!(regs.b(), 0);
        assert_eq!(regs.e(), 0);
        assert_eq!(regs.f(), 0);
        assert_eq!(regs.gp(), 0);
        assert_eq!(regs.x(), 0);
        assert_eq!(regs.y(), 0);
        assert_eq!(regs.sp(), 0);
        assert_eq!(regs.pc(), 0);
        assert_eq!(regs.dp(), 0);
        assert_eq!(regs.cc(), Flags::default());
    }

    #[test]
    fn test_8bit_accessors() {
        let mut regs = Registers::new();
        regs.set_a(0x42);
        assert_eq!(regs.a(), 0x42);
        regs.set_b(0x84);
        assert_eq!(regs.b(), 0x84);
        regs.set_e(0x11);
        assert_eq!(regs.e(), 0x11);
        regs.set_f(0x22);
        assert_eq!(regs.f(), 0x22);
        regs.set_gp(0x99);
        assert_eq!(regs.gp(), 0x99);
        regs.set_dp(0x10);
        assert_eq!(regs.dp(), 0x10);
    }

    #[test]
    fn test_24bit_masking() {
        let mut regs = Registers::new();

        // Test that values exceeding 24 bits are masked
        regs.set_x(0xFF12_3456);
        assert_eq!(regs.x(), 0x0012_3456);

        regs.set_y(0xABCD_EFFF);
        assert_eq!(regs.y(), 0x00CD_EFFF);

        regs.set_sp(0x8000_0001);
        assert_eq!(regs.sp(), 0x0000_0001);

        regs.set_pc(0xFFFF_FFFF);
        assert_eq!(regs.pc(), 0x00FF_FFFF);
    }

    #[test]
    fn test_d_register_composition() {
        let mut regs = Registers::new();

        // Test set_d and d() round-trip
        regs.set_d(0x1234);
        assert_eq!(regs.d(), 0x1234);
        assert_eq!(regs.a(), 0x12);
        assert_eq!(regs.b(), 0x34);

        // Test with zero
        regs.set_d(0);
        assert_eq!(regs.d(), 0);

        // Test with max value
        regs.set_d(0xFFFF);
        assert_eq!(regs.d(), 0xFFFF);
        assert_eq!(regs.a(), 0xFF);
        assert_eq!(regs.b(), 0xFF);

        // Test with mid-range value
        regs.set_d(0x5555);
        assert_eq!(regs.d(), 0x5555);
    }

    #[test]
    fn test_w_register_composition() {
        let mut regs = Registers::new();

        // Test set_w and w() round-trip
        regs.set_w(0xABCD);
        assert_eq!(regs.w(), 0xABCD);
        assert_eq!(regs.e(), 0xAB);
        assert_eq!(regs.f(), 0xCD);

        // Test with zero
        regs.set_w(0);
        assert_eq!(regs.w(), 0);

        // Test with max value
        regs.set_w(0xFFFF);
        assert_eq!(regs.w(), 0xFFFF);

        // Test with mid-range value
        regs.set_w(0x5555);
        assert_eq!(regs.w(), 0x5555);
    }

    #[test]
    fn test_q_register_composition() {
        let mut regs = Registers::new();

        // Test set_q and q() round-trip
        regs.set_q(0x1234_5678);
        assert_eq!(regs.q(), 0x1234_5678);
        assert_eq!(regs.d(), 0x1234);
        assert_eq!(regs.w(), 0x5678);
        assert_eq!(regs.a(), 0x12);
        assert_eq!(regs.b(), 0x34);
        assert_eq!(regs.e(), 0x56);
        assert_eq!(regs.f(), 0x78);

        // Test with zero
        regs.set_q(0);
        assert_eq!(regs.q(), 0);

        // Test with max value (note: Q is 32-bit)
        regs.set_q(0xFFFF_FFFF);
        assert_eq!(regs.q(), 0xFFFF_FFFF);

        // Test with mid-range value
        regs.set_q(0x5555_5555);
        assert_eq!(regs.q(), 0x5555_5555);
    }

    #[test]
    fn test_flag_operations() {
        let mut regs = Registers::new();

        // Test setting individual flags
        let mut flags = Flags::empty();
        flags.insert(Flags::Z);
        flags.insert(Flags::C);
        regs.set_cc(flags);

        assert!(regs.cc().contains(Flags::Z));
        assert!(regs.cc().contains(Flags::C));
        assert!(!regs.cc().contains(Flags::N));
    }
}
