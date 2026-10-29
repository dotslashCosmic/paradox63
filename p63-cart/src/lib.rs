//! p63-cart: `.p63` cartridge images: pack, verify, and load.

pub mod card;
pub mod crypt;

use std::fmt;

use p63_core::{Addr, Bus};
use p63_spec as spec;

pub const MAGIC: &[u8; 4] = b"P63C";
pub const VERSION: (u8, u8) = (1, 0);
pub const HEADER_LEN: usize = spec::CH_LEN as usize;
/// Largest image: everything from the load address to the top of main RAM.
pub const MAX_IMAGE: usize = (spec::MAIN_TOP - spec::FIXED_ENTRY + 1) as usize;

/// A boot failure, with the boot ROM's error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootError {
    Magic,
    Major,
    Minor,
    Flags,
    Mapper,
    Header,
    Entry,
    Checksum,
    Length,
    NoCart,
}

impl BootError {
    #[must_use]
    pub fn code(self) -> u32 {
        match self {
            Self::Magic => spec::ERR_MAGIC,
            Self::Major => spec::ERR_MAJOR,
            Self::Minor => spec::ERR_MINOR,
            Self::Flags => spec::ERR_FLAGS,
            Self::Mapper => spec::ERR_MAPPER,
            Self::Header => spec::ERR_HEADER,
            Self::Entry => spec::ERR_ENTRY,
            Self::Checksum => spec::ERR_CHECKSUM,
            Self::Length => spec::ERR_LENGTH,
            Self::NoCart => spec::ERR_NOCART,
        }
    }
}

impl fmt::Display for BootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match self {
            Self::Magic => "cartridge magic is not P63C",
            Self::Major => "unsupported major version",
            Self::Minor => "unsupported minor version",
            Self::Flags => "header flags set",
            Self::Mapper => "unsupported mapper",
            Self::Header => "header size or reserved bytes invalid",
            Self::Entry => "entry address is not the fixed entry",
            Self::Checksum => "image checksum mismatch",
            Self::Length => "image length out of range",
            Self::NoCart => "no cartridge inserted",
        };
        write!(f, "ERR {:02X}: {what}", self.code())
    }
}

impl std::error::Error for BootError {}

/// Interrupt vectors a cartridge can ask the boot ROM to install (0 = leave alone).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Vectors {
    pub swi: u32,
    pub nmi: u32,
    pub irq: u32,
    /// Only the low 16 bits are stored; the top byte is the fixed $00.
    pub firq: u32,
}

/// Options for `pack`.
#[derive(Debug, Clone, Default)]
pub struct PackOptions {
    pub title: String,
    pub mapper: u8,
    pub vectors: Vectors,
}

/// A verified cartridge.
#[derive(Debug, Clone)]
pub struct Cart {
    /// The whole file: header followed by the image.
    pub data: Vec<u8>,
    pub title: String,
    pub mapper: u8,
    pub vectors: Vectors,
}

/// 32-bit sum of the image's little-endian 16-bit words.
#[must_use]
pub fn checksum(image: &[u8]) -> u32 {
    image
        .chunks(2)
        .map(|w| u32::from(w[0]) | u32::from(*w.get(1).unwrap_or(&0)) << 8)
        .fold(0u32, u32::wrapping_add)
}

pub(crate) fn le(data: &[u8], at: u32, len: usize) -> u32 {
    data[at as usize..at as usize + len].iter().rev().fold(0, |acc, &b| (acc << 8) | u32::from(b))
}

pub(crate) fn put(out: &mut [u8], at: u32, len: usize, value: u32) {
    for i in 0..len {
        out[at as usize + i] = (value >> (8 * i)) as u8;
    }
}

/// Builds a cartridge from a raw image that loads at the fixed entry.
/// Odd-length images get one zero byte of padding.
pub fn pack(image: &[u8], opts: &PackOptions) -> Result<Vec<u8>, BootError> {
    let mut image = image.to_vec();
    if image.len() % 2 == 1 {
        image.push(0);
    }
    if !(2..=MAX_IMAGE).contains(&image.len()) {
        return Err(BootError::Length);
    }
    let mut hdr = vec![0u8; HEADER_LEN];
    hdr[..4].copy_from_slice(MAGIC);
    put(&mut hdr, spec::CH_VER_MAJOR, 1, u32::from(VERSION.0));
    put(&mut hdr, spec::CH_VER_MINOR, 1, u32::from(VERSION.1));
    put(&mut hdr, spec::CH_MAPPER, 1, u32::from(opts.mapper));
    put(&mut hdr, spec::CH_HDR_SIZE, 1, spec::CH_LEN);
    put(&mut hdr, spec::CH_IMAGE_LEN, 3, image.len() as u32);
    put(&mut hdr, spec::CH_ENTRY, 3, spec::FIXED_ENTRY);
    put(&mut hdr, spec::CH_CHECKSUM, 4, checksum(&image));
    put(&mut hdr, spec::CH_VEC_SWI, 3, opts.vectors.swi);
    put(&mut hdr, spec::CH_VEC_NMI, 3, opts.vectors.nmi);
    put(&mut hdr, spec::CH_VEC_IRQ, 3, opts.vectors.irq);
    put(&mut hdr, spec::CH_VEC_FIRQ, 2, opts.vectors.firq);
    let title = opts.title.as_bytes();
    let n = title.len().min(16);
    hdr[spec::CH_TITLE as usize..spec::CH_TITLE as usize + n].copy_from_slice(&title[..n]);
    hdr.extend(image);
    Ok(hdr)
}

impl Cart {
    /// Runs the boot ROM's header checks, in order.
    pub fn parse(data: Vec<u8>) -> Result<Self, BootError> {
        if data.is_empty() {
            return Err(BootError::NoCart);
        }
        if data.len() < HEADER_LEN {
            return Err(BootError::Header);
        }
        if &data[..4] != MAGIC {
            return Err(BootError::Magic);
        }
        if le(&data, spec::CH_VER_MAJOR, 1) != u32::from(VERSION.0) {
            return Err(BootError::Major);
        }
        // 1.0 has no flags; 1.1 adds encrypted and sealed
        let flags = le(&data, spec::CH_FLAGS, 2);
        match le(&data, spec::CH_VER_MINOR, 1) {
            0 if flags != 0 => return Err(BootError::Flags),
            0 => {}
            1 if flags & !(crypt::FLAG_ENCRYPTED | crypt::FLAG_SEALED) != 0 => return Err(BootError::Flags),
            1 => {}
            _ => return Err(BootError::Minor),
        }
        // saves go on memory cards, so there is no save RAM mapper any more
        let mapper = le(&data, spec::CH_MAPPER, 1);
        if mapper != spec::MAPPER_ROM {
            return Err(BootError::Mapper);
        }
        let reserved_clear = data[spec::CH_RESERVED as usize..HEADER_LEN].iter().all(|&b| b == 0);
        if le(&data, spec::CH_HDR_SIZE, 1) != spec::CH_LEN || !reserved_clear || le(&data, spec::CH_SAVE_KB, 2) != 0 {
            return Err(BootError::Header);
        }
        if le(&data, spec::CH_ENTRY, 3) != spec::FIXED_ENTRY {
            return Err(BootError::Entry);
        }
        let len = le(&data, spec::CH_IMAGE_LEN, 3) as usize;
        if len % 2 == 1 || !(2..=MAX_IMAGE).contains(&len) {
            return Err(BootError::Length);
        }
        let layout = crypt::Layout::of(&data)?;
        // an encrypted image can only be summed once the console has opened it
        if !layout.encrypted && checksum(&data[HEADER_LEN..HEADER_LEN + len]) != le(&data, spec::CH_CHECKSUM, 4) {
            return Err(BootError::Checksum);
        }
        let title_bytes = &data[spec::CH_TITLE as usize..spec::CH_TITLE as usize + 16];
        let title = String::from_utf8_lossy(title_bytes).trim_end_matches('\0').to_string();
        let vectors = Vectors {
            swi: le(&data, spec::CH_VEC_SWI, 3),
            nmi: le(&data, spec::CH_VEC_NMI, 3),
            irq: le(&data, spec::CH_VEC_IRQ, 3),
            firq: le(&data, spec::CH_VEC_FIRQ, 2),
        };
        Ok(Self { data, title, mapper: mapper as u8, vectors })
    }

    /// The program image, without the header.
    #[must_use]
    pub fn image(&self) -> &[u8] {
        let len = le(&self.data, spec::CH_IMAGE_LEN, 3) as usize;
        &self.data[HEADER_LEN..HEADER_LEN + len]
    }

    /// Does what the boot ROM does after the checks: copies the image to the fixed
    /// entry and installs any non-zero vectors.
    pub fn load(&self, bus: &mut impl Bus) {
        for (i, b) in self.image().iter().enumerate() {
            bus.write(Addr::new(spec::FIXED_ENTRY + i as u32), *b);
        }
        let v = self.vectors;
        for (addr, value) in [(spec::VEC_SWI, v.swi), (spec::VEC_NMI, v.nmi), (spec::VEC_IRQ, v.irq)] {
            if value != 0 {
                bus.write_u24_le(Addr::new(addr), value);
            }
        }
        if v.firq != 0 {
            bus.write_u16_le(Addr::new(spec::VEC_FIRQ), v.firq as u16);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_then_parse_round_trips() {
        let opts = PackOptions { title: "PONG".into(), vectors: Vectors { irq: 0x1234, ..Vectors::default() }, ..PackOptions::default() };
        let cart = Cart::parse(pack(&[1, 2, 3], &opts).unwrap()).unwrap();
        assert_eq!(cart.image(), &[1, 2, 3, 0]);
        assert_eq!(cart.title, "PONG");
        assert_eq!(cart.vectors.irq, 0x1234);
    }

    #[test]
    fn checks_fail_with_the_right_codes() {
        let good = pack(&[0xAA; 8], &PackOptions::default()).unwrap();
        let broken = |at: usize, v: u8| {
            let mut d = good.clone();
            d[at] = v;
            Cart::parse(d).unwrap_err().code()
        };
        assert_eq!(broken(0, b'X'), spec::ERR_MAGIC);
        assert_eq!(broken(spec::CH_VER_MAJOR as usize, 2), spec::ERR_MAJOR);
        assert_eq!(broken(spec::CH_MAPPER as usize, 9), spec::ERR_MAPPER);
        assert_eq!(broken(spec::CH_MAPPER as usize, 1), spec::ERR_MAPPER, "the old save RAM mapper is gone");
        assert_eq!(broken(spec::CH_SAVE_KB as usize, 1), spec::ERR_HEADER);
        assert_eq!(broken(spec::CH_ENTRY as usize + 1, 2), spec::ERR_ENTRY);
        assert_eq!(broken(HEADER_LEN + 3, 0), spec::ERR_CHECKSUM);
        assert_eq!(broken(spec::CH_IMAGE_LEN as usize, 9), spec::ERR_LENGTH);
        assert_eq!(Cart::parse(Vec::new()).unwrap_err().code(), spec::ERR_NOCART);
    }

    #[test]
    fn checksum_sums_words() {
        assert_eq!(checksum(&[0x01, 0x02, 0xFF, 0xFF]), 0x0201 + 0xFFFF);
    }
}
