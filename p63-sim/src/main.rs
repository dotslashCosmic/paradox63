//! p63-sim: command-line tools for Paradox63.
//!
//! `asm`  assembles a file and reports errors or a summary.
//! `call` assembles a file, calls one routine with the given registers and
//!        prints the registers it returns with.
//! `run`  boots a program or cartridge on the full machine for N frames.
//! `play` boots it in a window with the keyboard as the gamepad.

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
        /// Write a .p63 cartridge (image loaded at $000100) to this file
        #[arg(short, long)]
        cart: Option<PathBuf>,
    },
    /// Play a program (assembly source or .p63 cartridge) in a window.
    /// Arrows = D-pad, Enter = Start, Backspace = Select, Z/X = A/B, Esc = quit
    Play {
        file: PathBuf,
    },
    /// Boot a program (assembly source or .p63 cartridge) and run it
    Run {
        file: PathBuf,
        /// Frames to run (60 per second)
        #[arg(short, long, default_value_t = 60)]
        frames: u64,
        /// Print memory afterwards: ADDR:LEN, e.g. `$10:8` (repeatable)
        #[arg(long)]
        peek: Vec<String>,
        /// Gamepad buttons held the whole run: A B START SELECT UP DOWN LEFT RIGHT
        #[arg(long)]
        hold: Vec<String>,
        /// Save the last frame as a .bmp
        #[arg(long)]
        screenshot: Option<PathBuf>,
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
        Command::Asm { file, output, symbols, cart } => asm(&file, output.as_deref(), symbols, cart.as_deref()),
        Command::Play { file } => play(&file),
        Command::Run { file, frames, peek, hold, screenshot } => run(&file, frames, &peek, &hold, screenshot.as_deref()),
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

fn asm(file: &Path, output: Option<&Path>, symbols: bool, cart: Option<&Path>) -> Result<(), String> {
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
    if let Some(out) = cart {
        let data = make_cart(&program, file)?;
        std::fs::write(out, &data).map_err(|e| format!("{}: {e}", out.display()))?;
        println!("wrote cartridge {} ({} bytes)", out.display(), data.len());
    }
    Ok(())
}

/// Packs an assembled program as a cartridge titled after the file name.
fn make_cart(program: &p63_asm::Program, file: &Path) -> Result<Vec<u8>, String> {
    let image = program.image_at(p63_spec::FIXED_ENTRY)?;
    let title = file.file_stem().map(|s| s.to_string_lossy().to_ascii_uppercase()).unwrap_or_default();
    let opts = p63_cart::PackOptions { title, ..Default::default() };
    p63_cart::pack(&image, &opts).map_err(|e| e.to_string())
}

/// Boots a cartridge, or assembles and packs a source file first, then runs frames.
fn button(name: &str) -> Result<u8, String> {
    Ok(match name.to_ascii_uppercase().as_str() {
        "A" => 0x80,
        "B" => 0x40,
        "START" => 0x20,
        "SELECT" => 0x10,
        "UP" => 0x08,
        "DOWN" => 0x04,
        "LEFT" => 0x02,
        "RIGHT" => 0x01,
        other => return Err(format!("unknown button `{other}`")),
    })
}

/// Writes a 24-bit bottom-up BMP.
fn save_bmp(path: &Path, pixels: &[u32], width: usize, height: usize) -> Result<(), String> {
    let row = (width * 3).div_ceil(4) * 4;
    let size = 54 + row * height;
    let mut out = Vec::with_capacity(size);
    out.extend(b"BM");
    for v in [size as u32, 0, 54, 40, width as u32, height as u32] {
        out.extend(v.to_le_bytes());
    }
    out.extend(1u16.to_le_bytes());
    out.extend(24u16.to_le_bytes());
    out.extend([0u8; 24]);
    for y in (0..height).rev() {
        let start = out.len();
        for &p in &pixels[y * width..(y + 1) * width] {
            out.extend([p as u8, (p >> 8) as u8, (p >> 16) as u8]);
        }
        out.resize(start + row, 0);
    }
    std::fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))
}

/// Reads a cartridge, or assembles and packs a source file, and boots it.
fn boot(file: &Path) -> Result<(p63_bus::Machine, p63_cart::Cart), String> {
    let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let cart = if bytes.starts_with(p63_cart::MAGIC) {
        bytes
    } else {
        let program = p63_asm::assemble_file(file).map_err(|e| errors_to_string(&e))?;
        make_cart(&program, file)?
    };
    let mut machine = p63_bus::Machine::new();
    let info = machine.boot(cart).map_err(|e| e.to_string())?;
    Ok((machine, info))
}

fn play(file: &Path) -> Result<(), String> {
    use minifb::{Key, Scale, Window, WindowOptions};
    let keys = [
        (Key::Up, 0x08),
        (Key::Down, 0x04),
        (Key::Left, 0x02),
        (Key::Right, 0x01),
        (Key::Enter, 0x20),
        (Key::Backspace, 0x10),
        (Key::Z, 0x80),
        (Key::X, 0x40),
    ];
    let (mut machine, info) = boot(file)?;
    let (w, h) = (p63_ppu::WIDTH, p63_ppu::HEIGHT);
    let options = WindowOptions { scale: Scale::X2, resize: true, ..WindowOptions::default() };
    let mut window = Window::new(&format!("Paradox63 - {}", info.title), w, h, options).map_err(|e| e.to_string())?;
    window.set_target_fps(60);
    while window.is_open() && !window.is_key_down(Key::Escape) {
        machine.cpu.bus.keyboard.pad[0] = keys.iter().filter(|(k, _)| window.is_key_down(*k)).map(|(_, b)| b).fold(0, |a, b| a | b);
        machine.run_frame().map_err(|e| format!("frame {}, at ${:06X}: {e}", machine.frames, machine.cpu.registers.pc()))?;
        window.update_with_buffer(&machine.frame, w, h).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn run(file: &Path, frames: u64, peek: &[String], hold: &[String], screenshot: Option<&Path>) -> Result<(), String> {
    let (mut machine, info) = boot(file)?;
    println!("booted {:?} ({} byte image)", info.title, info.image().len());
    for name in hold {
        machine.cpu.bus.keyboard.pad[0] |= button(name)?;
    }
    for frame in 0..frames {
        if let Err(e) = machine.run_frame() {
            print_registers(&machine.cpu, 0);
            return Err(format!("frame {frame}, at ${:06X}: {e}", machine.cpu.registers.pc()));
        }
    }
    println!("ran {frames} frames");
    let r = &machine.cpu.registers;
    println!("PC=${:06X} SP=${:06X} halt={:?}", r.pc(), r.sp(), machine.cpu.halt());
    if let Some(path) = screenshot {
        save_bmp(path, &machine.frame, p63_ppu::WIDTH, p63_ppu::HEIGHT)?;
        println!("saved {}", path.display());
    }
    for spec in peek {
        let (addr, len) = spec.split_once(':').ok_or(format!("--peek wants ADDR:LEN, got `{spec}`"))?;
        let (addr, len) = (parse_value(addr)?, parse_value(len)?);
        let bytes: Vec<String> =
            (0..len).map(|i| format!("{:02X}", p63_core::Bus::read(&mut machine.cpu.bus, p63_core::Addr::new(addr + i)))).collect();
        println!("${addr:06X}: {}", bytes.join(" "));
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

fn print_registers<B: p63_core::Bus>(cpu: &ReferenceInterpreter<B>, cycles: u64) {
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
