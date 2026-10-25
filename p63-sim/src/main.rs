//! p63-sim: command-line tools for Paradox63.
//!
//! `asm`  assembles a file and reports errors or a summary.
//! `call` assembles a file, calls one routine with the given registers and
//!        prints the registers it returns with.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use p63_core::{Ram, ReferenceInterpreter};

#[derive(Parser)]
#[command(name = "p63-sim", about = "Paradox63 assembler and emulator tools")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Assemble a file; optionally write the raw image
    Asm {
        file: PathBuf,
        /// Write the assembled bytes (from the lowest address) to this file
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Print the symbol table
        #[arg(short, long)]
        symbols: bool,
    },
    /// Call routines: `call utils/bitops.p63 POP_COUNT A=$A5`.
    /// Several in a row share state: `call utils/prng.p63 RAND_SEED8,RAND8 A=$42`
    Call {
        file: PathBuf,
        /// Routine name, or several separated by commas
        routine: String,
        /// Registers to set first, e.g. A=$A5 D=1000 X=$2000
        regs: Vec<String>,
        /// Give up after this many instructions
        #[arg(long, default_value_t = 10_000_000)]
        max_steps: u64,
    },
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Asm { file, output, symbols } => asm(&file, output.as_deref(), symbols),
        Command::Call { file, routine, regs, max_steps } => call(&file, &routine, &regs, max_steps),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn errors_to_string(errors: &[p63_asm::AsmError]) -> String {
    errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")
}

fn asm(file: &Path, output: Option<&Path>, symbols: bool) -> Result<(), String> {
    let program = p63_asm::assemble_file(file).map_err(|e| errors_to_string(&e))?;
    for (addr, bytes) in &program.segments {
        println!("${addr:06X}: {} bytes", bytes.len());
    }
    if symbols {
        for (name, value) in &program.symbols {
            println!("  {name:<24} ${value:06X}");
        }
    }
    if let Some(out) = output {
        let start = program.segments.iter().map(|(a, _)| *a).min().unwrap_or(0);
        let end = program.segments.iter().map(|(a, b)| *a + b.len() as u32).max().unwrap_or(start);
        let mut image = vec![0u8; (end - start) as usize];
        for (addr, bytes) in &program.segments {
            let at = (addr - start) as usize;
            image[at..at + bytes.len()].copy_from_slice(bytes);
        }
        std::fs::write(out, &image).map_err(|e| format!("{}: {e}", out.display()))?;
        println!("wrote {} bytes from ${start:06X} to {}", image.len(), out.display());
    }
    Ok(())
}

fn parse_value(text: &str) -> Result<u32, String> {
    let t = text.trim();
    let (digits, radix) = if let Some(h) = t.strip_prefix('$').or_else(|| t.strip_prefix("0x")) {
        (h, 16)
    } else if let Some(b) = t.strip_prefix('%') {
        (b, 2)
    } else {
        (t, 10)
    };
    if let Some(neg) = digits.strip_prefix('-') {
        return i64::from_str_radix(neg, radix).map(|v| (-v) as u32).map_err(|_| format!("bad value `{text}`"));
    }
    u32::from_str_radix(digits, radix).map_err(|_| format!("bad value `{text}`"))
}

fn set_register(cpu: &mut ReferenceInterpreter<Ram>, assignment: &str) -> Result<(), String> {
    let (name, value) = assignment.split_once('=').ok_or(format!("expected REG=VALUE, got `{assignment}`"))?;
    if value.trim().is_empty() {
        return Err(format!(
            "no value for {name}: if you typed {name}=$.., your shell ate the `$`; \
             quote it ('{name}=$52') or use {name}=0x52"
        ));
    }
    let v = parse_value(value)?;
    let upper = name.trim().to_ascii_uppercase();
    let bits = match upper.as_str() {
        "D" | "W" => 16,
        "Q" => 32,
        "X" | "Y" | "SP" => 24,
        _ => 8,
    };
    if bits < 32 && v >> bits != 0 && v >> (bits - 1) != (u32::MAX >> (bits - 1)) {
        return Err(format!("{upper} is {bits}-bit; `{value}` doesn't fit"));
    }
    let r = &mut cpu.registers;
    match upper.as_str() {
        "A" => r.set_a(v as u8),
        "B" => r.set_b(v as u8),
        "D" => r.set_d(v as u16),
        "E" => r.set_e(v as u8),
        "F" => r.set_f(v as u8),
        "W" => r.set_w(v as u16),
        "Q" => r.set_q(v),
        "GP" => r.set_gp(v as u8),
        "X" => r.set_x(v),
        "Y" => r.set_y(v),
        "SP" => r.set_sp(v),
        "DP" => r.set_dp(v as u8),
        "CC" => r.set_cc(p63_core::Flags::from_bits_truncate(v as u8)),
        other => return Err(format!("unknown register `{other}`")),
    }
    Ok(())
}

fn call(file: &Path, routine: &str, regs: &[String], max_steps: u64) -> Result<(), String> {
    let path = std::fs::canonicalize(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let calls: String = routine
        .split(',')
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(|r| format!("    JSR {r}\n"))
        .collect();
    let source = format!(
        "    ORG $1000\nP63SIM_START:\n{calls}P63SIM_DONE: BRA P63SIM_DONE\n    INCLUDE \"{}\"\n",
        path.display()
    );
    let program = p63_asm::assemble("call", &source).map_err(|e| errors_to_string(&e))?;
    let start = program.symbol("P63SIM_START").unwrap_or(0x1000);
    let done = program.symbol("P63SIM_DONE").unwrap_or(0x1004);

    let mut cpu = ReferenceInterpreter::new(Ram::new(0x0100_0000));
    program.load(&mut cpu.bus);
    cpu.registers.set_sp(0x8000);
    for assignment in regs {
        set_register(&mut cpu, assignment)?;
    }
    cpu.registers.set_pc(start);

    let mut cycles: u64 = 0;
    for _ in 0..max_steps {
        if cpu.registers.pc() == done {
            print_registers(&cpu, cycles);
            return Ok(());
        }
        let c = cpu.step().map_err(|e| format!("at ${:06X}: {e}", cpu.registers.pc()))?;
        cycles += u64::from(c.0);
    }
    print_registers(&cpu, cycles);
    Err(format!("{routine} didn't return within {max_steps} instructions"))
}

fn print_registers(cpu: &ReferenceInterpreter<Ram>, cycles: u64) {
    let r = &cpu.registers;
    let cc = r.cc();
    let flags: String = [
        (p63_core::Flags::E, 'E'),
        (p63_core::Flags::F, 'F'),
        (p63_core::Flags::H, 'H'),
        (p63_core::Flags::I, 'I'),
        (p63_core::Flags::N, 'N'),
        (p63_core::Flags::Z, 'Z'),
        (p63_core::Flags::V, 'V'),
        (p63_core::Flags::C, 'C'),
    ]
    .iter()
    .map(|&(f, c)| if cc.contains(f) { c } else { '.' })
    .collect();
    println!("A=${:02X} B=${:02X} D=${:04X}  E=${:02X} F=${:02X} W=${:04X}  Q=${:08X}", r.a(), r.b(), r.d(), r.e(), r.f(), r.w(), r.q());
    println!("GP=${:02X} X=${:06X} Y=${:06X} SP=${:06X} DP=${:02X}  CC={flags}", r.gp(), r.x(), r.y(), r.sp(), r.dp());
    println!("{cycles} cycles (including the JSRs)");
}
