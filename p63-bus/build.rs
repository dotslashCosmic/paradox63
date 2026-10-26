// The BIOS cipher key is private: it's read here at build time, never kept
// P63_BIOS_KEY names the key file; without one this build can't decrypt a BIOS.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=P63_BIOS_KEY");
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let path = std::env::var_os("P63_BIOS_KEY")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../p63-files/bios/keys/bios_cipher_key.bin"));
    println!("cargo:rerun-if-changed={}", path.display());
    let key = match std::fs::read(&path) {
        Ok(bytes) if bytes.len() == 32 => format!("Some({bytes:?})"),
        Ok(_) => panic!("{} isn't a 32-byte key", path.display()),
        Err(_) => "None".to_string(),
    };
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("bios_key.rs");
    std::fs::write(out, format!("const CIPHER_KEY: Option<[u8; 32]> = {key};\n")).unwrap();
}
