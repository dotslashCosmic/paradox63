//! INCLUDE search: next to the including file first, then the library folders in order.

use std::fs;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("p63asm_include_{name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("game")).unwrap();
    fs::create_dir_all(dir.join("lib/engine")).unwrap();
    fs::create_dir_all(dir.join("lib2")).unwrap();
    dir
}

#[test]
fn library_folders_are_searched_after_the_local_folder() {
    let dir = scratch("order");
    fs::write(dir.join("game/main.p63"), "    INCLUDE \"engine/part.p63\"\n    INCLUDE \"both.p63\"\n    INCLUDE \"two.p63\"\n").unwrap();
    // engine/part.p63 includes a file next to itself
    fs::write(dir.join("lib/engine/part.p63"), "    INCLUDE \"inner.p63\"\n").unwrap();
    fs::write(dir.join("lib/engine/inner.p63"), "    .BYTE 1\n").unwrap();
    fs::write(dir.join("lib/both.p63"), "    .BYTE 9\n").unwrap();
    fs::write(dir.join("game/both.p63"), "    .BYTE 2\n").unwrap(); // the local one wins
    fs::write(dir.join("lib2/two.p63"), "    .BYTE 3\n").unwrap();
    let search = [dir.join("lib"), dir.join("lib2")];
    let program = p63_asm::assemble_file_in(dir.join("game/main.p63"), &search).unwrap();
    assert_eq!(program.segments[0].1, vec![1, 2, 3]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn without_library_folders_a_missing_include_names_the_local_path() {
    let dir = scratch("missing");
    fs::write(dir.join("game/main.p63"), "    INCLUDE \"nowhere.p63\"\n").unwrap();
    let errors = p63_asm::assemble_file(dir.join("game/main.p63")).unwrap_err();
    assert!(errors[0].message.contains("nowhere.p63"), "{}", errors[0].message);
    let _ = fs::remove_dir_all(&dir);
}
