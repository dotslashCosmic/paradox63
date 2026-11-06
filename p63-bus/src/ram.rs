//! The machine's RAM as the emulator holds it. Main RAM (everything under the boot ROM) is
//! kept XORed with a pad picked at random each run, so a dump of the emulator's memory
//! doesn't show a running game. Audio RAM, OAM and VRAM are kept plain, and so is any part
//! of main RAM that something reads whole while it's in use: a guest's own video and audio.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::ops::Range;

use p63_spec as spec;

const PAD: usize = 4096;
/// Bytes below this are scrambled: main RAM.
const MAIN_END: usize = spec::BOOT_ROM as usize;
const LEN: usize = 0x0100_0000;

pub struct Ram {
    bytes: Vec<u8>,
    pad: Box<[u8; PAD]>,
    /// Parts of main RAM kept plain, for the readers that take them whole.
    plain: Vec<Range<usize>>,
}

impl Default for Ram {
    fn default() -> Self {
        let mut seed = RandomState::new().build_hasher().finish();
        let mut pad = Box::new([0u8; PAD]);
        for chunk in pad.chunks_mut(8) {
            // splitmix64
            seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = seed;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            chunk.copy_from_slice(&(z ^ (z >> 31)).to_le_bytes());
        }
        // all zeros, as stored: every scrambled byte starts as its pad
        let mut bytes = vec![0u8; LEN];
        for (i, b) in bytes[..MAIN_END].iter_mut().enumerate() {
            *b = pad[i % PAD];
        }
        Self { bytes, pad, plain: Vec::new() }
    }
}

impl Ram {
    #[inline]
    fn hidden(&self, a: usize) -> bool {
        a < MAIN_END && !self.plain.iter().any(|r| r.contains(&a))
    }

    /// The byte at `a`.
    #[inline]
    #[must_use]
    pub fn get(&self, a: usize) -> u8 {
        let b = self.bytes[a];
        if self.hidden(a) { b ^ self.pad[a % PAD] } else { b }
    }

    #[inline]
    pub fn set(&mut self, a: usize, value: u8) {
        self.bytes[a] = if self.hidden(a) { value ^ self.pad[a % PAD] } else { value };
    }

    /// A copy of `range`, plain.
    #[must_use]
    pub fn read(&self, range: Range<usize>) -> Vec<u8> {
        range.map(|a| self.get(a)).collect()
    }

    pub fn write(&mut self, at: usize, data: &[u8]) {
        for (i, &b) in data.iter().enumerate() {
            self.set(at + i, b);
        }
    }

    /// `range` as it's stored, for readers that take it whole: only plain parts (above
    /// main RAM, or kept plain with `keep_plain`) read right.
    #[must_use]
    pub fn slice(&self, range: Range<usize>) -> Option<&[u8]> {
        debug_assert!(self.is_plain(&range), "a slice of scrambled RAM");
        self.bytes.get(range)
    }

    fn is_plain(&self, r: &Range<usize>) -> bool {
        r.start >= MAIN_END || self.plain.iter().any(|p| p.start <= r.start && r.end.min(MAIN_END) <= p.end)
    }

    /// From now on these parts of main RAM are the plain ones; what they and the ones
    /// before hold is kept.
    pub fn keep_plain(&mut self, ranges: Vec<Range<usize>>) {
        let ranges: Vec<Range<usize>> = ranges.into_iter().map(|r| r.start..r.end.min(MAIN_END)).filter(|r| !r.is_empty()).collect();
        if ranges == self.plain {
            return;
        }
        let touched: Vec<Range<usize>> = self.plain.iter().chain(&ranges).cloned().collect();
        let held: Vec<Vec<u8>> = touched.iter().map(|r| self.read(r.clone())).collect();
        self.plain = ranges;
        for (r, data) in touched.iter().zip(&held) {
            self.write(r.start, data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_ram_is_stored_scrambled() {
        let mut ram = Ram::default();
        assert_eq!(ram.get(0x1234), 0, "starts as zeros");
        ram.write(0x100, b"PARADOX63 GAME CODE");
        assert_eq!(ram.read(0x100..0x113), b"PARADOX63 GAME CODE");
        assert!(!ram.bytes.windows(19).any(|w| w == b"PARADOX63 GAME CODE"), "not stored as it is");
        ram.set(spec::OAM_BASE as usize, 7);
        assert_eq!(ram.slice(spec::OAM_BASE as usize..spec::OAM_BASE as usize + 1), Some(&[7][..]), "video RAM is plain");
    }

    #[test]
    fn kept_plain_parts_read_whole_and_keep_their_bytes() {
        let mut ram = Ram::default();
        ram.write(0x10_0000, &[1, 2, 3, 4]);
        ram.keep_plain(vec![0x10_0000..0x10_0004]);
        assert_eq!(ram.slice(0x10_0000..0x10_0004), Some(&[1, 2, 3, 4][..]));
        ram.set(0x10_0001, 9);
        ram.keep_plain(vec![0x20_0000..0x20_0010]);
        assert_eq!(ram.read(0x10_0000..0x10_0004), [1, 9, 3, 4], "scrambled again, same bytes");
        assert_ne!(ram.bytes[0x10_0000..0x10_0004], [1, 9, 3, 4]);
    }
}
