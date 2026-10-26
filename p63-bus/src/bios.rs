//! Boot ROM images: a `P63B` header, an Ed25519 signature, then the ChaCha20-encrypted image.
//! Only images signed with the BIOS key verify

use std::fmt;

use chacha20::cipher::{KeyIvInit, StreamCipher};
use chacha20::ChaCha20;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use p63_spec as spec;

pub const MAGIC: &[u8; 4] = b"P63B";
const HEADER: usize = 32;
const SIGNATURE: usize = 64;
const NONCE: std::ops::Range<usize> = 12..24;

/// The BIOS signing key's public half.
const PUBLIC_KEY: [u8; 32] = [
    0x4C, 0xD5, 0x32, 0xAA, 0x7D, 0x50, 0x0D, 0x44, 0x0C, 0x5E, 0x35, 0x1D, 0x73, 0x6B, 0xA4, 0x98,
    0xC4, 0xE0, 0x5B, 0x73, 0x7F, 0x0D, 0x29, 0x2E, 0x13, 0xF6, 0xBB, 0x7F, 0xE5, 0xB6, 0xF1, 0x0B,
];

// the key the image is encrypted with
include!(concat!(env!("OUT_DIR"), "/bios_key.rs"));

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
    let cipher_key = CIPHER_KEY.ok_or(BiosError::NoKey)?;
    let mut image = body.to_vec();
    let nonce: [u8; 12] = file[NONCE].try_into().expect("12 bytes");
    ChaCha20::new(&cipher_key.into(), &nonce.into()).apply_keystream(&mut image);
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
}
