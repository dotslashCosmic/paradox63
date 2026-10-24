//! Memory bus abstraction for the Paradox63 CPU.

use crate::Addr;

/// Memory and I/O bus interface.
pub trait Bus {
    /// Read a single byte from the bus.
    fn read(&mut self, addr: Addr) -> u8;

    /// Write a single byte to the bus.
    fn write(&mut self, addr: Addr, val: u8);

    /// Read a 16-bit little-endian value from the bus.
    fn read_u16_le(&mut self, addr: Addr) -> u16 {
        let lo = self.read(addr);
        let hi = self.read(Addr::new(addr.0.wrapping_add(1) & 0x00FF_FFFF));
        u16::from_le_bytes([lo, hi])
    }

    /// Write a 16-bit little-endian value to the bus.
    fn write_u16_le(&mut self, addr: Addr, val: u16) {
        let bytes = val.to_le_bytes();
        self.write(addr, bytes[0]);
        self.write(Addr::new(addr.0.wrapping_add(1) & 0x00FF_FFFF), bytes[1]);
    }

    /// Read a 24-bit little-endian value from the bus.
    fn read_u24_le(&mut self, addr: Addr) -> u32 {
        let b0 = self.read(addr);
        let b1 = self.read(Addr::new(addr.0.wrapping_add(1) & 0x00FF_FFFF));
        let b2 = self.read(Addr::new(addr.0.wrapping_add(2) & 0x00FF_FFFF));
        u32::from_le_bytes([b0, b1, b2, 0])
    }

    /// Write a 24-bit little-endian value to the bus.
    fn write_u24_le(&mut self, addr: Addr, val: u32) {
        let bytes = val.to_le_bytes();
        self.write(addr, bytes[0]);
        self.write(Addr::new(addr.0.wrapping_add(1) & 0x00FF_FFFF), bytes[1]);
        self.write(Addr::new(addr.0.wrapping_add(2) & 0x00FF_FFFF), bytes[2]);
    }

    /// Read a 32-bit little-endian value from the bus.
    fn read_u32_le(&mut self, addr: Addr) -> u32 {
        let b0 = self.read(addr);
        let b1 = self.read(Addr::new(addr.0.wrapping_add(1) & 0x00FF_FFFF));
        let b2 = self.read(Addr::new(addr.0.wrapping_add(2) & 0x00FF_FFFF));
        let b3 = self.read(Addr::new(addr.0.wrapping_add(3) & 0x00FF_FFFF));
        u32::from_le_bytes([b0, b1, b2, b3])
    }

    /// Write a 32-bit little-endian value to the bus.
    fn write_u32_le(&mut self, addr: Addr, val: u32) {
        let bytes = val.to_le_bytes();
        self.write(addr, bytes[0]);
        self.write(Addr::new(addr.0.wrapping_add(1) & 0x00FF_FFFF), bytes[1]);
        self.write(Addr::new(addr.0.wrapping_add(2) & 0x00FF_FFFF), bytes[2]);
        self.write(Addr::new(addr.0.wrapping_add(3) & 0x00FF_FFFF), bytes[3]);
    }
}

/// A simple flat RAM implementation backed by a contiguous byte array.
#[derive(Debug, Clone)]
pub struct Ram {
    data: Vec<u8>,
}

impl Ram {
    /// Creates a new `Ram` instance with the specified size.
    #[must_use]
    pub fn new(size: usize) -> Self {
        assert!(
            size <= 0x0100_0000,
            "Ram size cannot exceed 16 MB (0x01000000 bytes)"
        );
        Self {
            data: vec![0; size],
        }
    }

    /// Returns the size of the RAM in bytes.
    #[must_use]
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Loads data into RAM at the specified address.
    pub fn load(&mut self, addr: Addr, data: &[u8]) {
        let start = addr.0 as usize;
        let end = start + data.len();
        assert!(
            end <= self.data.len(),
            "load would exceed RAM bounds: addr={:06X}, len={}, RAM size={}",
            addr.0,
            data.len(),
            self.data.len()
        );
        self.data[start..end].copy_from_slice(data);
    }
}

impl Bus for Ram {
    fn read(&mut self, addr: Addr) -> u8 {
        let index = (addr.0 as usize) % self.data.len();
        self.data[index]
    }

    fn write(&mut self, addr: Addr, val: u8) {
        let index = (addr.0 as usize) % self.data.len();
        self.data[index] = val;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ram_basic_read_write() {
        let mut ram = Ram::new(1024);

        ram.write(Addr::new(0x100), 0x42);
        assert_eq!(ram.read(Addr::new(0x100)), 0x42);

        ram.write(Addr::new(0), 0xFF);
        assert_eq!(ram.read(Addr::new(0)), 0xFF);
    }

    #[test]
    fn ram_read_u16_le() {
        let mut ram = Ram::new(1024);

        // Write bytes in little-endian order
        ram.write(Addr::new(0x100), 0x34);
        ram.write(Addr::new(0x101), 0x12);

        assert_eq!(ram.read_u16_le(Addr::new(0x100)), 0x1234);
    }

    #[test]
    fn ram_write_u16_le() {
        let mut ram = Ram::new(1024);

        ram.write_u16_le(Addr::new(0x100), 0x1234);

        assert_eq!(ram.read(Addr::new(0x100)), 0x34);
        assert_eq!(ram.read(Addr::new(0x101)), 0x12);
    }

    #[test]
    fn ram_read_u24_le() {
        let mut ram = Ram::new(1024);

        ram.write(Addr::new(0x100), 0x78);
        ram.write(Addr::new(0x101), 0x56);
        ram.write(Addr::new(0x102), 0x34);

        assert_eq!(ram.read_u24_le(Addr::new(0x100)), 0x0034_5678);
    }

    #[test]
    fn ram_write_u24_le() {
        let mut ram = Ram::new(1024);

        ram.write_u24_le(Addr::new(0x100), 0x0034_5678);

        assert_eq!(ram.read(Addr::new(0x100)), 0x78);
        assert_eq!(ram.read(Addr::new(0x101)), 0x56);
        assert_eq!(ram.read(Addr::new(0x102)), 0x34);
    }

    #[test]
    fn ram_write_u24_le_ignores_upper_byte() {
        let mut ram = Ram::new(1024);

        // Upper byte (0xAB) should be ignored
        ram.write_u24_le(Addr::new(0x100), 0xAB34_5678);

        assert_eq!(ram.read(Addr::new(0x100)), 0x78);
        assert_eq!(ram.read(Addr::new(0x101)), 0x56);
        assert_eq!(ram.read(Addr::new(0x102)), 0x34);
    }

    #[test]
    fn ram_read_u32_le() {
        let mut ram = Ram::new(1024);

        ram.write(Addr::new(0x100), 0x78);
        ram.write(Addr::new(0x101), 0x56);
        ram.write(Addr::new(0x102), 0x34);
        ram.write(Addr::new(0x103), 0x12);

        assert_eq!(ram.read_u32_le(Addr::new(0x100)), 0x1234_5678);
    }

    #[test]
    fn ram_write_u32_le() {
        let mut ram = Ram::new(1024);

        ram.write_u32_le(Addr::new(0x100), 0x1234_5678);

        assert_eq!(ram.read(Addr::new(0x100)), 0x78);
        assert_eq!(ram.read(Addr::new(0x101)), 0x56);
        assert_eq!(ram.read(Addr::new(0x102)), 0x34);
        assert_eq!(ram.read(Addr::new(0x103)), 0x12);
    }

    #[test]
    fn ram_load() {
        let mut ram = Ram::new(1024);
        let data = [0x01, 0x02, 0x03, 0x04, 0x05];

        ram.load(Addr::new(0x100), &data);

        assert_eq!(ram.read(Addr::new(0x100)), 0x01);
        assert_eq!(ram.read(Addr::new(0x101)), 0x02);
        assert_eq!(ram.read(Addr::new(0x102)), 0x03);
        assert_eq!(ram.read(Addr::new(0x103)), 0x04);
        assert_eq!(ram.read(Addr::new(0x104)), 0x05);
    }

    #[test]
    fn address_wraparound_u16() {
        let mut ram = Ram::new(16 * 1024 * 1024); // Full 24-bit space

        // Write at the boundary
        ram.write(Addr::new(0x00FF_FFFF), 0xAB);
        ram.write(Addr::new(0x0000_0000), 0xCD);

        // Read should wrap around
        let value = ram.read_u16_le(Addr::new(0x00FF_FFFF));
        assert_eq!(value, 0xCDAB); // 0xAB at FFFFFF, 0xCD at 000000
    }

    #[test]
    fn address_wraparound_u24() {
        let mut ram = Ram::new(16 * 1024 * 1024); // Full 24-bit space

        // Write at the boundary
        ram.write(Addr::new(0x00FF_FFFE), 0x11);
        ram.write(Addr::new(0x00FF_FFFF), 0x22);
        ram.write(Addr::new(0x0000_0000), 0x33);

        // Read should wrap around
        let value = ram.read_u24_le(Addr::new(0x00FF_FFFE));
        assert_eq!(value, 0x0033_2211);
    }

    #[test]
    fn address_wraparound_u32() {
        let mut ram = Ram::new(16 * 1024 * 1024); // Full 24-bit space

        // Write at the boundary
        ram.write(Addr::new(0x00FF_FFFD), 0xAA);
        ram.write(Addr::new(0x00FF_FFFE), 0xBB);
        ram.write(Addr::new(0x00FF_FFFF), 0xCC);
        ram.write(Addr::new(0x0000_0000), 0xDD);

        // Read should wrap around
        let value = ram.read_u32_le(Addr::new(0x00FF_FFFD));
        assert_eq!(value, 0xDDCC_BBAA);
    }

    #[test]
    fn write_wraparound_u16() {
        let mut ram = Ram::new(16 * 1024 * 1024); // Full 24-bit space

        ram.write_u16_le(Addr::new(0x00FF_FFFF), 0xABCD);

        assert_eq!(ram.read(Addr::new(0x00FF_FFFF)), 0xCD);
        assert_eq!(ram.read(Addr::new(0x0000_0000)), 0xAB);
    }

    #[test]
    fn write_wraparound_u24() {
        let mut ram = Ram::new(16 * 1024 * 1024); // Full 24-bit space

        ram.write_u24_le(Addr::new(0x00FF_FFFE), 0x0011_2233);

        assert_eq!(ram.read(Addr::new(0x00FF_FFFE)), 0x33);
        assert_eq!(ram.read(Addr::new(0x00FF_FFFF)), 0x22);
        assert_eq!(ram.read(Addr::new(0x0000_0000)), 0x11);
    }

    #[test]
    fn write_wraparound_u32() {
        let mut ram = Ram::new(16 * 1024 * 1024); // Full 24-bit space

        ram.write_u32_le(Addr::new(0x00FF_FFFD), 0xAABB_CCDD);

        assert_eq!(ram.read(Addr::new(0x00FF_FFFD)), 0xDD);
        assert_eq!(ram.read(Addr::new(0x00FF_FFFE)), 0xCC);
        assert_eq!(ram.read(Addr::new(0x00FF_FFFF)), 0xBB);
        assert_eq!(ram.read(Addr::new(0x0000_0000)), 0xAA);
    }

    #[test]
    fn ram_size() {
        let ram = Ram::new(1024);
        assert_eq!(ram.size(), 1024);

        let ram = Ram::new(64 * 1024);
        assert_eq!(ram.size(), 64 * 1024);
    }

    #[test]
    #[should_panic(expected = "Ram size cannot exceed 16 MB")]
    fn ram_size_exceeds_24_bit_space() {
        let _ram = Ram::new(0x0100_0001);
    }

    #[test]
    #[should_panic(expected = "load would exceed RAM bounds")]
    fn ram_load_exceeds_bounds() {
        let mut ram = Ram::new(1024);
        let data = [0u8; 100];
        ram.load(Addr::new(1000), &data); // Would write beyond 1024
    }
}
