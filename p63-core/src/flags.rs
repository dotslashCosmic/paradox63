//! Condition Code (CC) register flags

use bitflags::bitflags;

bitflags! {
    /// Condition Code register flags
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct Flags: u8 {
        const E = 0x80;  // Entire context flag
        const F = 0x40;  // Fast interrupt mask (reserved)
        const H = 0x20;  // Half carry
        const I = 0x10;  // Interrupt mask
        const N = 0x08;  // Negative
        const Z = 0x04;  // Zero
        const V = 0x02;  // Overflow
        const C = 0x01;  // Carry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flag_values() {
        assert_eq!(Flags::E.bits(), 0x80);
        assert_eq!(Flags::F.bits(), 0x40);
        assert_eq!(Flags::H.bits(), 0x20);
        assert_eq!(Flags::I.bits(), 0x10);
        assert_eq!(Flags::N.bits(), 0x08);
        assert_eq!(Flags::Z.bits(), 0x04);
        assert_eq!(Flags::V.bits(), 0x02);
        assert_eq!(Flags::C.bits(), 0x01);
    }

    #[test]
    fn test_empty_flags() {
        let flags = Flags::empty();
        assert_eq!(flags.bits(), 0);
        assert!(!flags.contains(Flags::E));
        assert!(!flags.contains(Flags::F));
        assert!(!flags.contains(Flags::H));
        assert!(!flags.contains(Flags::I));
        assert!(!flags.contains(Flags::N));
        assert!(!flags.contains(Flags::Z));
        assert!(!flags.contains(Flags::V));
        assert!(!flags.contains(Flags::C));
    }

    #[test]
    fn test_all_flags() {
        let flags = Flags::all();
        assert_eq!(flags.bits(), 0xFF);
        assert!(flags.contains(Flags::E));
        assert!(flags.contains(Flags::F));
        assert!(flags.contains(Flags::H));
        assert!(flags.contains(Flags::I));
        assert!(flags.contains(Flags::N));
        assert!(flags.contains(Flags::Z));
        assert!(flags.contains(Flags::V));
        assert!(flags.contains(Flags::C));
    }

    #[test]
    fn test_insert_flag() {
        let mut flags = Flags::empty();

        // Insert Z flag
        flags.insert(Flags::Z);
        assert!(flags.contains(Flags::Z));
        assert!(!flags.contains(Flags::N));
        assert_eq!(flags.bits(), 0x04);

        // Insert N flag
        flags.insert(Flags::N);
        assert!(flags.contains(Flags::Z));
        assert!(flags.contains(Flags::N));
        assert_eq!(flags.bits(), 0x0C);

        // Insert C flag
        flags.insert(Flags::C);
        assert!(flags.contains(Flags::Z));
        assert!(flags.contains(Flags::N));
        assert!(flags.contains(Flags::C));
        assert_eq!(flags.bits(), 0x0D);
    }

    #[test]
    fn test_remove_flag() {
        let mut flags = Flags::all();

        // Remove Z flag
        flags.remove(Flags::Z);
        assert!(!flags.contains(Flags::Z));
        assert!(flags.contains(Flags::N));
        assert_eq!(flags.bits(), 0xFB);

        // Remove N flag
        flags.remove(Flags::N);
        assert!(!flags.contains(Flags::Z));
        assert!(!flags.contains(Flags::N));
        assert_eq!(flags.bits(), 0xF3);

        // Remove C flag
        flags.remove(Flags::C);
        assert!(!flags.contains(Flags::Z));
        assert!(!flags.contains(Flags::N));
        assert!(!flags.contains(Flags::C));
        assert_eq!(flags.bits(), 0xF2);
    }

    #[test]
    fn test_set_flag() {
        let mut flags = Flags::empty();

        // Set Z to true
        flags.set(Flags::Z, true);
        assert!(flags.contains(Flags::Z));
        assert_eq!(flags.bits(), 0x04);

        // Set Z to false
        flags.set(Flags::Z, false);
        assert!(!flags.contains(Flags::Z));
        assert_eq!(flags.bits(), 0x00);

        // Set multiple flags
        flags.set(Flags::N, true);
        flags.set(Flags::C, true);
        assert!(flags.contains(Flags::N));
        assert!(flags.contains(Flags::C));
        assert_eq!(flags.bits(), 0x09);
    }

    #[test]
    fn test_toggle_flag() {
        let mut flags = Flags::empty();

        // Toggle Z flag on
        flags.toggle(Flags::Z);
        assert!(flags.contains(Flags::Z));
        assert_eq!(flags.bits(), 0x04);

        // Toggle Z flag off
        flags.toggle(Flags::Z);
        assert!(!flags.contains(Flags::Z));
        assert_eq!(flags.bits(), 0x00);

        // Toggle multiple flags
        flags.toggle(Flags::N);
        flags.toggle(Flags::C);
        assert!(flags.contains(Flags::N));
        assert!(flags.contains(Flags::C));
        assert_eq!(flags.bits(), 0x09);

        // Toggle N flag off
        flags.toggle(Flags::N);
        assert!(!flags.contains(Flags::N));
        assert!(flags.contains(Flags::C));
        assert_eq!(flags.bits(), 0x01);
    }

    #[test]
    fn test_contains_flag() {
        let mut flags = Flags::empty();

        // Initially no flags are set
        assert!(!flags.contains(Flags::E));
        assert!(!flags.contains(Flags::F));
        assert!(!flags.contains(Flags::H));
        assert!(!flags.contains(Flags::I));
        assert!(!flags.contains(Flags::N));
        assert!(!flags.contains(Flags::Z));
        assert!(!flags.contains(Flags::V));
        assert!(!flags.contains(Flags::C));

        // Set some flags
        flags.insert(Flags::Z);
        flags.insert(Flags::N);

        // Check contains works correctly
        assert!(!flags.contains(Flags::E));
        assert!(!flags.contains(Flags::F));
        assert!(!flags.contains(Flags::H));
        assert!(!flags.contains(Flags::I));
        assert!(flags.contains(Flags::N));
        assert!(flags.contains(Flags::Z));
        assert!(!flags.contains(Flags::V));
        assert!(!flags.contains(Flags::C));
    }

    #[test]
    fn test_flag_combinations() {
        let mut flags = Flags::empty();

        // Set common ALU flags (N, Z, V, C)
        flags.insert(Flags::N | Flags::Z | Flags::V | Flags::C);
        assert!(flags.contains(Flags::N));
        assert!(flags.contains(Flags::Z));
        assert!(flags.contains(Flags::V));
        assert!(flags.contains(Flags::C));
        assert!(!flags.contains(Flags::H));
        assert_eq!(flags.bits(), 0x0F);

        // Set interrupt flags (I, F)
        let mut int_flags = Flags::empty();
        int_flags.insert(Flags::I | Flags::F);
        assert!(int_flags.contains(Flags::I));
        assert!(int_flags.contains(Flags::F));
        assert_eq!(int_flags.bits(), 0x50);
    }

    #[test]
    fn test_default_flags() {
        let flags = Flags::default();
        assert_eq!(flags.bits(), 0);
        assert_eq!(flags, Flags::empty());
    }

    #[test]
    fn test_flag_equality() {
        let flags1 = Flags::empty();
        let flags2 = Flags::empty();
        assert_eq!(flags1, flags2);

        let mut flags3 = Flags::empty();
        flags3.insert(Flags::Z);
        let mut flags4 = Flags::empty();
        flags4.insert(Flags::Z);
        assert_eq!(flags3, flags4);

        let mut flags5 = Flags::empty();
        flags5.insert(Flags::N);
        assert_ne!(flags3, flags5);
    }

    #[test]
    fn test_interrupt_mask_flags() {
        let mut flags = Flags::empty();

        // Test I flag (interrupt mask)
        flags.insert(Flags::I);
        assert!(flags.contains(Flags::I));
        assert_eq!(flags.bits(), 0x10);

        // Test F flag (fast interrupt mask)
        flags.insert(Flags::F);
        assert!(flags.contains(Flags::F));
        assert!(flags.contains(Flags::I));
        assert_eq!(flags.bits(), 0x50);

        // Test E flag (entire context flag)
        flags.insert(Flags::E);
        assert!(flags.contains(Flags::E));
        assert!(flags.contains(Flags::F));
        assert!(flags.contains(Flags::I));
        assert_eq!(flags.bits(), 0xD0);
    }

    #[test]
    fn test_arithmetic_flags() {
        let mut flags = Flags::empty();

        // Simulate ADD result: negative, carry, half-carry
        flags.insert(Flags::N | Flags::C | Flags::H);
        assert!(flags.contains(Flags::N));
        assert!(flags.contains(Flags::C));
        assert!(flags.contains(Flags::H));
        assert!(!flags.contains(Flags::Z));
        assert!(!flags.contains(Flags::V));
        assert_eq!(flags.bits(), 0x29);

        // Simulate zero result: clear N, C, H; set Z
        flags.remove(Flags::N | Flags::C | Flags::H);
        flags.insert(Flags::Z);
        assert!(!flags.contains(Flags::N));
        assert!(flags.contains(Flags::Z));
        assert!(!flags.contains(Flags::C));
        assert_eq!(flags.bits(), 0x04);
    }
}
