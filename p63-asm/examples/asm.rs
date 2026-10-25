//! Assembles the files given on the command line and prints errors or a summary.

fn main() {
    for path in std::env::args().skip(1) {
        match p63_asm::assemble_file(&path) {
            Ok(p) => {
                let size: usize = p.segments.iter().map(|(_, b)| b.len()).sum();
                println!("{path}: ok, {size} bytes, {} symbols", p.symbols.len());
            }
            Err(errors) => {
                for e in &errors {
                    println!("{e}");
                }
            }
        }
    }
}
