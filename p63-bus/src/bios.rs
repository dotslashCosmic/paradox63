//! Boot ROM images: a `P63B` header, an Ed25519 signature, then the ChaCha20-encrypted image.
//! Only images signed with the BIOS key verify. The cipher key comes from its own file,
//! bios.key, which isn't part of this program.

use std::cell::RefCell;
use std::collections::hash_map::RandomState;
use std::fmt;
use std::hash::{BuildHasher, Hasher};
use std::hint::black_box;

use chacha20::cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};
use zeroize::Zeroize;
use chacha20::ChaCha20;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use p63_spec as spec;

pub const MAGIC: &[u8; 4] = b"P63B";
const HEADER: usize = 32;
const SIGNATURE: usize = 64;
const NONCE: std::ops::Range<usize> = 12..24;
/// Bytes decrypted at a time, and how many such pages are kept plain at once.
const PAGE: usize = 256;
const CACHED: usize = 4;

/// The BIOS signing key's public half.
const PUBLIC_KEY: [u8; 32] = [
    0xC1, 0xD3, 0xE9, 0x09, 0x56, 0x91, 0x04, 0x3B, 0xDF, 0xAC, 0xAD, 0x67, 0xF1, 0xC3, 0x63, 0xF6,
    0x21, 0xE9, 0x17, 0x1D, 0x8D, 0x40, 0x11, 0x20, 0x85, 0x22, 0xEF, 0x0D, 0xB8, 0xA7, 0xA8, 0x00,
];

pub const KEY_LEN: usize = 32;

/// The cipher key as it's held: two pieces that XOR to it, so it's never stored whole.
#[derive(Clone, Default)]
struct Key([u8; KEY_LEN], [u8; KEY_LEN]);

impl Key {
    fn new(key: &[u8; KEY_LEN]) -> Self {
        let a = random_piece();
        Self(a, std::array::from_fn(|i| key[i] ^ a[i]))
    }

    /// Puts the key together for one use; the caller wipes it after.
    fn whole(&self) -> [u8; KEY_LEN] {
        // black_box keeps the compiler from folding the pieces back into the key
        let (a, b) = (black_box(&self.0), black_box(&self.1));
        std::array::from_fn(|i| a[i] ^ b[i])
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        self.0.zeroize();
        self.1.zeroize();
    }
}

fn random_piece() -> [u8; KEY_LEN] {
    std::array::from_fn(|i| {
        let mut h = RandomState::new().build_hasher();
        h.write_usize(i);
        h.finish() as u8
    })
}

/// A boot ROM image as the emulator holds it, so a dump of its memory shows no BIOS.
/// An official one stays encrypted as it came in the file: a page is decrypted when the CPU
/// first reads it, and only the last few pages read are kept plain.
#[derive(Clone, Default)]
pub struct BootRom {
    data: Vec<u8>,
    hidden: Hidden,
    /// Plain pages: their number, then their bytes; the last one read first.
    pages: RefCell<Vec<(usize, Box<[u8; PAGE]>)>>,
}

#[derive(Clone, Default)]
enum Hidden {
    /// Each byte XORed with a mask made from a per-run seed and its offset.
    #[default]
    Scrambled,
    Seeded(u64),
    /// ChaCha20 under the BIOS key, with this nonce.
    Encrypted([u8; 12], Key),
}

impl BootRom {
    /// Scrambles a plain image and wipes the plain copy.
    #[must_use]
    pub fn new(mut plain: Vec<u8>) -> Self {
        let seed = RandomState::new().build_hasher().finish();
        let data = plain.iter().enumerate().map(|(i, &b)| b ^ mask(seed, i)).collect();
        plain.zeroize();
        Self { data, hidden: Hidden::Seeded(seed), pages: RefCell::default() }
    }

    fn encrypted(data: Vec<u8>, nonce: [u8; 12], key: &[u8; KEY_LEN]) -> Self {
        Self { data, hidden: Hidden::Encrypted(nonce, Key::new(key)), pages: RefCell::default() }
    }

    /// The byte at `offset`, as the CPU sees it.
    #[must_use]
    pub fn get(&self, offset: usize) -> Option<u8> {
        let b = *self.data.get(offset)?;
        match &self.hidden {
            Hidden::Scrambled => Some(b),
            Hidden::Seeded(seed) => Some(b ^ mask(*seed, offset)),
            Hidden::Encrypted(nonce, key) => Some(self.plain_page(offset / PAGE, *nonce, key)[offset % PAGE]),
        }
    }

    // a page decrypted, from the few kept or freshly; the oldest kept one is wiped to make room
    fn plain_page(&self, n: usize, nonce: [u8; 12], key: &Key) -> [u8; PAGE] {
        let mut pages = self.pages.borrow_mut();
        if let Some(k) = pages.iter().position(|p| p.0 == n) {
            if k != 0 {
                let p = pages.remove(k);
                pages.insert(0, p);
            }
            return *pages[0].1;
        }
        let start = n * PAGE;
        let end = (start + PAGE).min(self.data.len());
        let mut page = Box::new([0u8; PAGE]);
        page[..end - start].copy_from_slice(&self.data[start..end]);
        let mut whole = key.whole();
        let mut c = ChaCha20::new(&whole.into(), &nonce.into());
        whole.zeroize();
        c.seek(start as u64);
        c.apply_keystream(&mut page[..end - start]);
        if pages.len() == CACHED {
            if let Some(mut old) = pages.pop() {
                old.1.zeroize();
            }
        }
        let plain = *page;
        pages.insert(0, (n, page));
        plain
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
    Key,
}

impl fmt::Display for BiosError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotBios => write!(f, "not a P63 BIOS image (no P63B header)"),
            Self::Version(major, minor) => write!(f, "BIOS image format {major}.{minor} isn't supported"),
            Self::Header => write!(f, "BIOS image header is damaged"),
            Self::Length => write!(f, "BIOS image length doesn't match the file, or doesn't fit the boot ROM"),
            Self::Signature => write!(f, "BIOS image signature doesn't verify: it's modified or not an official build"),
            Self::Key => write!(f, "bios.key isn't a BIOS key"),
        }
    }
}

impl std::error::Error for BiosError {}

/// Checks a boot ROM file and returns the image inside it, still encrypted: pages are
/// decrypted with `cipher` (the contents of bios.key) as they're read.
pub fn verify(file: &[u8], cipher: &[u8]) -> Result<BootRom, BiosError> {
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
    let mut cipher: [u8; KEY_LEN] = cipher.try_into().map_err(|_| BiosError::Key)?;
    let nonce: [u8; 12] = file[NONCE].try_into().expect("12 bytes");
    let rom = BootRom::encrypted(body.to_vec(), nonce, &cipher);
    cipher.zeroize();
    Ok(rom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn junk_and_tampering_are_refused() {
        let key = [3u8; KEY_LEN];
        assert_eq!(verify(b"hello", &key).err(), Some(BiosError::NotBios));
        let mut file = vec![0u8; HEADER + SIGNATURE + 4];
        file[..4].copy_from_slice(MAGIC);
        file[4] = 2;
        file[8] = 4;
        assert_eq!(verify(&file, &key).err(), Some(BiosError::Signature), "an unsigned image");
        file[8] = 5;
        assert_eq!(verify(&file, &key).err(), Some(BiosError::Length));
        file[4] = 1;
        assert_eq!(verify(&file, &key).err(), Some(BiosError::Version(1, 0)), "the old unencrypted format");
    }

    #[test]
    fn key_is_held_in_pieces() {
        let key = [0xA5u8; KEY_LEN];
        let held = Key::new(&key);
        assert_ne!(held.0, key);
        assert_ne!(held.1, key);
        assert_eq!(held.whole(), key);
    }

    #[test]
    fn pages_decrypt_as_read_and_few_stay_plain() {
        let key = [0x5Au8; KEY_LEN];
        let plain: Vec<u8> = (0..PAGE * 6).map(|i| (i * 7 + i / 256) as u8).collect();
        let mut data = plain.clone();
        let nonce = [5u8; 12];
        ChaCha20::new(&key.into(), &nonce.into()).apply_keystream(&mut data);
        let rom = BootRom::encrypted(data.clone(), nonce, &key);
        assert_eq!(rom.data, data, "kept encrypted");
        for i in (0..plain.len()).rev().step_by(37).chain(0..plain.len()) {
            assert_eq!(rom.get(i), Some(plain[i]), "byte {i}");
        }
        assert_eq!(rom.pages.borrow().len(), CACHED, "only a few pages plain");
        assert_eq!(rom.get(plain.len()), None);
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
