// The BIOS cipher key is private: it's read here at build time, never kept
// P63_BIOS_KEY names the key file; without one this build can't decrypt a BIOS.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::path::PathBuf;

fn random_piece() -> [u8; 32] {
    std::array::from_fn(|i| {
        let mut h = RandomState::new().build_hasher();
        h.write_usize(i);
        h.finish() as u8
    })
}

fn main() {
    println!("cargo:rerun-if-env-changed=P63_BIOS_KEY");
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let path = std::env::var_os("P63_BIOS_KEY")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../p63-files/bios/keys/bios_cipher_key.bin"));
    println!("cargo:rerun-if-changed={}", path.display());
    let (has, pieces) = match std::fs::read(&path) {
        Ok(key) if key.len() == 32 => {
            let (a, b) = (random_piece(), random_piece());
            let c: [u8; 32] = std::array::from_fn(|i| key[i] ^ a[i] ^ b[i]);
            (true, [a, b, c])
        }
        Ok(_) => panic!("{} isn't a 32-byte key", path.display()),
        Err(_) => (false, [[0; 32]; 3]),
    };
    let [a, b, c] = pieces;
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("bios_key.rs");
    std::fs::write(
        out,
        format!(
            "const HAS_KEY: bool = {has};\nstatic KEY_A: [u8; 32] = {a:?};\nstatic KEY_B: [u8; 32] = {b:?};\nstatic KEY_C: [u8; 32] = {c:?};\n"
        ),
    )
    .unwrap();
}
