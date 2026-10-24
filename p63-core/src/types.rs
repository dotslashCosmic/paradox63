//! Domain primitive types for the Paradox63 CPU emulator.

use std::fmt;

/// A 24-bit address in the Paradox63 address space (`0x0000_0000` - `0x00FF_FFFF`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Addr(pub u32);

impl Addr {
    /// Creates a new `Addr`, masking the input to 24 bits.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value & 0x00FF_FFFF)
    }
}

impl From<u32> for Addr {
    /// Converts a `u32` to an `Addr`, masking to 24 bits.
    fn from(value: u32) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:06X}", self.0)
    }
}

impl fmt::UpperHex for Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

/// A cycle count representing CPU execution time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cycles(pub u32);

impl Cycles {
    /// Creates a new `Cycles` value.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}

impl From<u32> for Cycles {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

/// An 8-bit opcode byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Opcode(pub u8);

impl Opcode {
    /// Creates a new `Opcode`.
    #[must_use]
    pub const fn new(value: u8) -> Self {
        Self(value)
    }
}

impl From<u8> for Opcode {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl fmt::Display for Opcode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:02X}", self.0)
    }
}

impl fmt::UpperHex for Opcode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addr_masks_to_24_bits() {
        // Test masking of values exceeding 24 bits
        assert_eq!(Addr::new(0xFFFF_FFFF).0, 0x00FF_FFFF);
        assert_eq!(Addr::new(0x1234_5678).0, 0x0034_5678);
        assert_eq!(Addr::new(0xFF00_0000).0, 0x0000_0000);
    }

    #[test]
    fn addr_preserves_valid_24_bit_values() {
        assert_eq!(Addr::new(0x0000_0000).0, 0x0000_0000);
        assert_eq!(Addr::new(0x00FF_FFFF).0, 0x00FF_FFFF);
        assert_eq!(Addr::new(0x0012_3456).0, 0x0012_3456);
    }

    #[test]
    fn addr_from_u32_masks_correctly() {
        let addr: Addr = 0x8765_4321_u32.into();
        assert_eq!(addr.0, 0x0065_4321);
    }

    #[test]
    fn cycles_stores_value() {
        assert_eq!(Cycles::new(0).0, 0);
        assert_eq!(Cycles::new(42).0, 42);
        assert_eq!(Cycles::new(1000).0, 1000);
    }

    #[test]
    fn cycles_from_u32() {
        let cycles: Cycles = 123_u32.into();
        assert_eq!(cycles.0, 123);
    }

    #[test]
    fn opcode_stores_value() {
        assert_eq!(Opcode::new(0x00).0, 0x00);
        assert_eq!(Opcode::new(0xFF).0, 0xFF);
        assert_eq!(Opcode::new(0x42).0, 0x42);
    }

    #[test]
    fn opcode_from_u8() {
        let opcode: Opcode = 0xAB_u8.into();
        assert_eq!(opcode.0, 0xAB);
    }
}
