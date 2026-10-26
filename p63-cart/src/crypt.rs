//! Encrypted and sealed cartridges (header v1.1).
//!
//! An encrypted image is ChaCha20 under a key only the console can work out: the
//! cart carries a fresh X25519 public key after the image, and the console's secret
//! half lives in its BIOS. A sealed cart ends with an Ed25519 signature over
//! everything before it, made with the seal key.

use chacha20::cipher::{KeyIvInit, StreamCipher};
use chacha20::ChaCha20;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};

use crate::{le, put, BootError, HEADER_LEN};
use p63_spec as spec;

/// FLAGS bits, from header v1.1 on.
pub const FLAG_ENCRYPTED: u32 = spec::CF_ENCRYPTED;
pub const FLAG_SEALED: u32 = spec::CF_SEALED;
/// Bytes after the image: the cart's public key, then the seal.
pub const KEY_LEN: usize = 32;
pub const SEAL_LEN: usize = 64;

/// The console's public key: anyone can encrypt a cart to it.
pub const CONSOLE_KEY: [u8; 32] = [
    0xD9, 0x49, 0x16, 0x4E, 0x8D, 0xE5, 0xA4, 0xBA, 0x33, 0x2E, 0x42, 0xFD, 0xE4, 0x18, 0xA5, 0x93,
    0x85, 0xF3, 0x74, 0x99, 0x48, 0x66, 0x85, 0x81, 0x0D, 0x10, 0x39, 0xE0, 0x61, 0x74, 0x67, 0x75,
];

fn cipher(shared: &[u8; 32], cart_key: &[u8; 32]) -> ChaCha20 {
    let key: [u8; 32] = Sha256::new().chain_update(b"P63 cart").chain_update(shared).chain_update(cart_key).finalize().into();
    ChaCha20::new(&key.into(), &[0u8; 12].into())
}

/// The public half of a console's secret key.
#[must_use]
pub fn public_key(secret: [u8; 32]) -> [u8; 32] {
    PublicKey::from(&StaticSecret::from(secret)).to_bytes()
}

/// Encrypts a packed v1.0 cart to `console`, with `secret` as the cart's own key
/// (fresh random bytes each time).
pub fn encrypt(cart: &[u8], console: &[u8; 32], secret: [u8; 32]) -> Vec<u8> {
    let mut out = cart.to_vec();
    put(&mut out, spec::CH_VER_MINOR, 1, 1);
    put(&mut out, spec::CH_FLAGS, 2, FLAG_ENCRYPTED);
    let secret = StaticSecret::from(secret);
    let cart_key = PublicKey::from(&secret).to_bytes();
    let shared = secret.diffie_hellman(&PublicKey::from(*console)).to_bytes();
    cipher(&shared, &cart_key).apply_keystream(&mut out[HEADER_LEN..]);
    out.extend_from_slice(&cart_key);
    out
}

/// What the console's engine makes of a cart: its flags, and where each part is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub encrypted: bool,
    pub sealed: bool,
    pub image_len: usize,
}

impl Layout {
    /// Reads the flags and checks the file holds every part they promise.
    pub fn of(data: &[u8]) -> Result<Self, BootError> {
        if data.len() < HEADER_LEN {
            return Err(BootError::Header);
        }
        let flags = if le(data, spec::CH_VER_MINOR, 1) == 0 { 0 } else { le(data, spec::CH_FLAGS, 2) };
        let layout = Self {
            encrypted: flags & FLAG_ENCRYPTED != 0,
            sealed: flags & FLAG_SEALED != 0,
            image_len: le(data, spec::CH_IMAGE_LEN, 3) as usize,
        };
        if data.len() < layout.len() {
            return Err(BootError::Length);
        }
        Ok(layout)
    }

    /// Bytes from the start of the file to the end of the seal.
    #[must_use]
    pub fn len(&self) -> usize {
        self.signed_len() + if self.sealed { SEAL_LEN } else { 0 }
    }

    /// Bytes the seal covers: the header, the image and the cart's key.
    #[must_use]
    pub fn signed_len(&self) -> usize {
        HEADER_LEN + self.image_len + if self.encrypted { KEY_LEN } else { 0 }
    }
}

/// Checks a sealed cart's signature against `seal_key`.
#[must_use]
pub fn seal_ok(data: &[u8], layout: Layout, seal_key: &[u8; 32]) -> bool {
    let at = layout.signed_len();
    let (Ok(key), Ok(sig)) = (VerifyingKey::from_bytes(seal_key), Signature::from_slice(&data[at..at + SEAL_LEN])) else {
        return false;
    };
    key.verify(&data[..at], &sig).is_ok()
}

/// The image as the CPU should see it, decrypted with the console's secret key.
#[must_use]
pub fn image(data: &[u8], layout: Layout, console_secret: &[u8; 32]) -> Vec<u8> {
    let mut image = data[HEADER_LEN..HEADER_LEN + layout.image_len].to_vec();
    if layout.encrypted {
        let at = HEADER_LEN + layout.image_len;
        let cart_key: [u8; 32] = data[at..at + KEY_LEN].try_into().expect("32 bytes");
        let shared = StaticSecret::from(*console_secret).diffie_hellman(&PublicKey::from(cart_key)).to_bytes();
        cipher(&shared, &cart_key).apply_keystream(&mut image);
    }
    image
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{pack, Cart, PackOptions};

    #[test]
    fn encrypt_then_open() {
        let console = StaticSecret::from([7u8; 32]);
        let public = PublicKey::from(&console).to_bytes();
        let plain = pack(&[1, 2, 3, 4, 5, 6], &PackOptions::default()).unwrap();
        let sealed = encrypt(&plain, &public, [9u8; 32]);
        let layout = Layout::of(&sealed).unwrap();
        assert!(layout.encrypted && !layout.sealed);
        assert_ne!(&sealed[HEADER_LEN..HEADER_LEN + 6], &[1, 2, 3, 4, 5, 6], "scrambled");
        assert_eq!(image(&sealed, layout, &console.to_bytes()), [1, 2, 3, 4, 5, 6]);
        assert_ne!(image(&sealed, layout, &[8u8; 32]), [1, 2, 3, 4, 5, 6], "only the console's key opens it");
        assert_eq!(Cart::parse(sealed).unwrap().image().len(), 6, "the header still parses");
    }

    #[test]
    fn a_short_file_is_refused() {
        let plain = pack(&[1, 2], &PackOptions::default()).unwrap();
        let mut sealed = encrypt(&plain, &CONSOLE_KEY, [9u8; 32]);
        sealed.truncate(sealed.len() - 1);
        assert_eq!(Layout::of(&sealed), Err(BootError::Length));
    }
}
