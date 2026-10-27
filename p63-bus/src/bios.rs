//! Boot ROM images: a `P63B` header, an Ed25519 signature, then the ChaCha20-encrypted image.
//! Only images signed with the BIOS key verify

use std::collections::hash_map::RandomState;
use std::fmt;
use std::hash::{BuildHasher, Hasher};
use std::hint::black_box;

use chacha20::cipher::{KeyIvInit, StreamCipher};
use zeroize::Zeroize;
use chacha20::ChaCha20;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use p63_spec as spec;

pub const MAGIC: &[u8; 4] = b"P63B";
const HEADER: usize = 32;
const SIGNATURE: usize = 64;
const NONCE: std::ops::Range<usize> = 12..24;

/// The BIOS signing key's public half.
const PUBLIC_KEY: [u8; 32] = [
    0xC1, 0xD3, 0xE9, 0x09, 0x56, 0x91, 0x04, 0x3B, 0xDF, 0xAC, 0xAD, 0x67, 0xF1, 0xC3, 0x63, 0xF6,
    0x21, 0xE9, 0x17, 0x1D, 0x8D, 0x40, 0x11, 0x20, 0x85, 0x22, 0xEF, 0x0D, 0xB8, 0xA7, 0xA8, 0x00,
];

// the key the image is encrypted with
include!(concat!(env!("OUT_DIR"), "/bios_key.rs"));

/// Puts the key together for one use; the caller wipes it after.
fn cipher_key() -> Option<[u8; 32]> {
    if !HAS_KEY {
        return None;
    }
    // black_box keeps the compiler from folding the pieces back into the key
    let (a, b, c) = (black_box(&KEY_A), black_box(&KEY_B), black_box(&KEY_C));
    Some(std::array::from_fn(|i| a[i] ^ b[i] ^ c[i]))
}

/// A boot ROM image kept scrambled in memory: each byte is XORed with a mask made from
/// a per-run seed and its offset, so a dump of the emulator's memory shows no BIOS.
#[derive(Default)]
pub struct BootRom {
    data: Vec<u8>,
    seed: u64,
}

impl BootRom {
    /// Scrambles a plain image and wipes the plain copy.
    #[must_use]
    pub fn new(mut plain: Vec<u8>) -> Self {
        let seed = RandomState::new().build_hasher().finish();
        let data = plain.iter().enumerate().map(|(i, &b)| b ^ mask(seed, i)).collect();
        plain.zeroize();
        Self { data, seed }
    }

    /// The byte at `offset`, unscrambled.
    #[must_use]
    pub fn get(&self, offset: usize) -> Option<u8> {
        self.data.get(offset).map(|&b| b ^ mask(self.seed, offset))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

// splitmix64 of the seed and offset
fn mask(seed: u64, offset: usize) -> u8 {
    let mut z = seed.wrapping_add((offset as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)) as u8
}

/// Why a boot ROM file was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BiosError {
    NotBios,
    Version(u8, u8),
    Header,
    Length,
    Signature,
    NoKey,
}

impl fmt::Display for BiosError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotBios => write!(f, "not a P63 BIOS image (no P63B header)"),
            Self::Version(major, minor) => write!(f, "BIOS image format {major}.{minor} isn't supported"),
            Self::Header => write!(f, "BIOS image header is damaged"),
            Self::Length => write!(f, "BIOS image length doesn't match the file, or doesn't fit the boot ROM"),
            Self::Signature => write!(f, "BIOS image signature doesn't verify: it's modified or not an official build"),
            Self::NoKey => write!(f, "this build of p63-sim can't run the BIOS: use an official release"),
        }
    }
}

impl std::error::Error for BiosError {}

/// Checks a boot ROM file and returns the decrypted image inside it.
pub fn verify(file: &[u8]) -> Result<Vec<u8>, BiosError> {
    if file.len() < HEADER + SIGNATURE || &file[..4] != MAGIC {
        return Err(BiosError::NotBios);
    }
    if (file[4], file[5]) != (2, 0) {
        return Err(BiosError::Version(file[4], file[5]));
    }
    if file[6..8].iter().chain(&file[NONCE.end..HEADER]).any(|&b| b != 0) {
        return Err(BiosError::Header);
    }
    let len = u32::from_le_bytes([file[8], file[9], file[10], file[11]]) as usize;
    let slot = (spec::CART_WIN - spec::BOOT_ROM) as usize;
    if len > slot || file.len() != HEADER + SIGNATURE + len {
        return Err(BiosError::Length);
    }
    let body = &file[HEADER + SIGNATURE..];
    let signature = Signature::from_slice(&file[HEADER..HEADER + SIGNATURE]).map_err(|_| BiosError::Signature)?;
    let key = VerifyingKey::from_bytes(&PUBLIC_KEY).map_err(|_| BiosError::Signature)?;
    let signed = [&file[..HEADER], body].concat();
    key.verify(&signed, &signature).map_err(|_| BiosError::Signature)?;
    let mut cipher_key = cipher_key().ok_or(BiosError::NoKey)?;
    let mut image = body.to_vec();
    let nonce: [u8; 12] = file[NONCE].try_into().expect("12 bytes");
    ChaCha20::new(&cipher_key.into(), &nonce.into()).apply_keystream(&mut image);
    cipher_key.zeroize();
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn junk_and_tampering_are_refused() {
        assert_eq!(verify(b"hello"), Err(BiosError::NotBios));
        let mut file = vec![0u8; HEADER + SIGNATURE + 4];
        file[..4].copy_from_slice(MAGIC);
        file[4] = 2;
        file[8] = 4;
        assert_eq!(verify(&file), Err(BiosError::Signature), "an unsigned image");
        file[8] = 5;
        assert_eq!(verify(&file), Err(BiosError::Length));
        file[4] = 1;
        assert_eq!(verify(&file), Err(BiosError::Version(1, 0)), "the old unencrypted format");
    }

    #[test]
    fn boot_rom_is_scrambled_in_memory() {
        let plain: Vec<u8> = (0..=255).collect();
        let rom = BootRom::new(plain.clone());
        assert_ne!(rom.data, plain, "not stored as it is");
        assert!((0..256).all(|i| rom.get(i) == Some(i as u8)), "but reads back plain");
        assert_eq!(rom.get(256), None);
    }
}
