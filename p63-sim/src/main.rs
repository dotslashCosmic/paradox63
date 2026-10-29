//! p63-sim: command-line tools for Paradox63.
//!
//! `asm`  assembles a file and reports errors or a summary.
//! `call` assembles a file, calls one routine with the given registers and
//!        prints the registers it returns with.
//! `run`  boots a program or cartridge on the full machine for N frames.
//! `play` opens the console in a window: the PC's keyboard, mouse and controllers, sound, and the
//!        front panel (cartridge slot, power, reset, CPU speed) as menus.
//! `test` runs test ROMs (built on games/tests/testlib.p63) and reports pass or fail.

mod audio;
mod input;
mod window;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use p63_core::{Ram, ReferenceInterpreter};

#[derive(Parser)]
#[command(name = "p63-sim", about = "Paradox63 assembler and emulator tools")]
struct Cli {
    /// The signed BIOS image (default: bios.bin next to p63-sim, or in this folder)
    #[arg(long, global = true)]
    bios: Option<PathBuf>,
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
        /// Encrypt the cartridge so only a P63 console can read it
        #[arg(long, requires = "cart")]
        encrypt: bool,
        /// A library folder to look in for INCLUDE files (repeatable; P63_LIB adds more)
        #[arg(short = 'I', long = "lib")]
        lib: Vec<PathBuf>,
    },
    /// Open the console in a window, with this cartridge in the slot (or none).
    /// Keyboard, mouse (click in the window) and up to two controllers; Help > Controls lists the keys
    Play {
        file: Option<PathBuf>,
        /// Print a line a second: fps and where each frame's time went
        #[arg(long)]
        stats: bool,
        /// A memory card file for slot 1, instead of the one the window had last time
        #[arg(long)]
        card1: Option<PathBuf>,
        /// A memory card file for slot 2
        #[arg(long)]
        card2: Option<PathBuf>,
    },
    /// Boot a cartridge through the BIOS and run it headless (fresh settings: Home comes up)
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
        /// Save the sound of the whole run as a .wav
        #[arg(long)]
        wav: Option<PathBuf>,
        /// A memory card file for slot 1 (a new one is made if it doesn't exist); kept afterwards
        #[arg(long)]
        card1: Option<PathBuf>,
        /// A memory card file for slot 2
        #[arg(long)]
        card2: Option<PathBuf>,
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
        /// A library folder to look in for INCLUDE files (repeatable; P63_LIB adds more)
        #[arg(short = 'I', long = "lib")]
        lib: Vec<PathBuf>,
    },
    /// Run test ROM cartridges through the BIOS (Boot to Cartridge on) and report pass or fail
    Test {
        files: Vec<PathBuf>,
        /// Give each ROM this many frames to finish
        #[arg(long, default_value_t = 600)]
        frames: u64,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let bios = cli.bios.as_deref();
    let result = match cli.command {
        Command::Asm { file, output, symbols, cart, encrypt, lib } => {
            asm(&file, output.as_deref(), symbols, cart.as_deref(), encrypt, &libraries(lib))
        }
        Command::Play { file, stats, card1, card2 } => window::play(file.as_deref(), bios, stats, [card1, card2]),
        Command::Run { file, frames, peek, hold, screenshot, wav, card1, card2 } => {
            run(&file, bios, frames, &peek, &hold, screenshot.as_deref(), wav.as_deref(), [card1.as_deref(), card2.as_deref()])
        }
        Command::Call { file, routine, regs, max_steps, lib } => {
            call(&file, &routine, &regs, max_steps, &libraries(lib))
        }
        Command::Test { files, frames } => test(&files, bios, frames),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

/// The BIOS's settings with Boot to Cartridge on: its marker, then the switch.
const BOOT_TO_CART: [u8; 2] = [0x63, 1];

/// Where test ROMs leave their results (see games/tests/testlib.p63).
const TEST_RESULT: u32 = 0x00F000;
const TEST_LOG: u32 = TEST_RESULT + 0x10;
const TEST_LOG_ENTRY: u32 = 16;
/// Pure red ($E0) as drawn: on the PPU page any pixel of it is a bug.
const ACID_RED: u32 = 0x00FF_0000;

/// Little-endian value of `bytes` bytes at `addr`.
fn peek(machine: &mut p63_bus::Machine, addr: u32, bytes: u32) -> u32 {
    use p63_core::Addr;
    (0..bytes).fold(0, |v, i| v | u32::from(machine.cpu.bus.peek(Addr::new(addr + i))) << (8 * i))
}

/// Zero-terminated text at `addr`, up to 40 characters.
fn peek_text(machine: &mut p63_bus::Machine, addr: u32) -> String {
    (0..40)
        .map(|i| peek(machine, addr + i, 1) as u8)
        .take_while(|&c| c != 0)
        .map(char::from)
        .collect()
}

/// One logged failure: `#n SECTION: what was wrong`.
fn describe_failure(machine: &mut p63_bus::Machine, entry: u32) -> String {
    let at = TEST_LOG + entry * TEST_LOG_ENTRY;
    let num = peek(machine, at, 2);
    let kind = peek(machine, at + 2, 1);
    let digits = peek(machine, at + 3, 1).clamp(1, 4) as usize * 2;
    let (got, want) = (peek(machine, at + 4, 4), peek(machine, at + 8, 4));
    let section = peek(machine, at + 12, 3);
    let section = peek_text(machine, section);
    let what = match kind {
        2 => format!("flags ${got:02X}, want ${want:02X}"),
        3 => format!("${got:02X} not in ${:02X}-${:02X}", want & 0xFF, want >> 8),
        _ => format!("${got:0digits$X}, want ${want:0digits$X}"),
    };
    format!("#{num} {section}: {what}")
}

fn test(files: &[PathBuf], bios: Option<&Path>, frames: u64) -> Result<(), String> {
    let mut failed = 0;
    for file in files {
        let name = file.display();
        let (mut machine, _) = boot(file, bios)?;
        machine.cpu.bus.nvram[..2].copy_from_slice(&BOOT_TO_CART);
        let mut status = 0;
        for _ in 0..frames {
            machine.run_frame().map_err(|e| format!("{name}: at ${:06X}: {e}", machine.cpu.registers.pc()))?;
            // 1 passed, 2 failed; anything else is still running (or a boot ROM's RAM test)
            status = peek(&mut machine, TEST_RESULT, 1);
            if status == 1 || status == 2 {
                break;
            }
        }
        let checks = peek(&mut machine, TEST_RESULT + 2, 2);
        let fails = peek(&mut machine, TEST_RESULT + 4, 2);
        let logged = peek(&mut machine, TEST_RESULT + 6, 1);
        let mut ok = true;
        match status {
            1 => println!("PASS  {name} ({checks} checks)"),
            2 => {
                println!("FAIL  {name}: {fails} of {checks} checks failed");
                for entry in 0..logged {
                    println!("  {}", describe_failure(&mut machine, entry));
                }
                if fails > logged {
                    println!("  ({} more not logged)", fails - logged);
                }
                ok = false;
            }
            _ => {
                println!("FAIL  {name}: no result after {frames} frames ({checks} checks run)");
                ok = false;
            }
        }
        if peek(&mut machine, TEST_RESULT + 1, 1) == 1 {
            // let the last screen settle, then look for red
            for _ in 0..2 {
                machine.run_frame().map_err(|e| format!("{name}: {e}"))?;
            }
            let width = p63_ppu::WIDTH;
            let red: Vec<usize> = (0..machine.frame.len()).filter(|&i| machine.frame[i] == ACID_RED).collect();
            if red.is_empty() {
                println!("PASS  {name}: PPU page has no red");
            } else {
                let mut tiles: Vec<(usize, usize)> = red.iter().map(|&i| (i % width / 8, i / width / 8)).collect();
                tiles.dedup();
                tiles.sort_unstable();
                tiles.dedup();
                let shown: Vec<String> = tiles.iter().take(24).map(|(x, y)| format!("{x},{y}")).collect();
                let more = if tiles.len() > shown.len() { format!(" (+{} more)", tiles.len() - shown.len()) } else { String::new() };
                println!("FAIL  {name}: PPU page has {} red pixels in tiles {}{more}", red.len(), shown.join(" "));
                ok = false;
            }
        }
        if !ok {
            failed += 1;
        }
    }
    if failed == 0 {
        Ok(())
    } else {
        Err(format!("{failed} of {} failed", files.len()))
    }
}

fn errors_to_string(errors: &[p63_asm::AsmError]) -> String {
    errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")
}

/// The library folders for INCLUDE: those given with -I, then those in P63_LIB.
fn libraries(mut given: Vec<PathBuf>) -> Vec<PathBuf> {
    if let Some(env) = std::env::var_os("P63_LIB") {
        given.extend(std::env::split_paths(&env));
    }
    given
}

fn asm(
    file: &Path,
    output: Option<&Path>,
    symbols: bool,
    cart: Option<&Path>,
    encrypt: bool,
    lib: &[PathBuf],
) -> Result<(), String> {
    let program = p63_asm::assemble_file_in(file, lib).map_err(|e| errors_to_string(&e))?;
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
        let mut data = make_cart(&program, file)?;
        if encrypt {
            let mut secret = [0u8; 32];
            getrandom::getrandom(&mut secret).map_err(|e| format!("no random numbers for the key: {e}"))?;
            data = p63_cart::crypt::encrypt(&data, &p63_cart::crypt::CONSOLE_KEY, secret);
        }
        std::fs::write(out, &data).map_err(|e| format!("{}: {e}", out.display()))?;
        let how = if encrypt { "encrypted cartridge" } else { "cartridge" };
        println!("wrote {how} {} ({} bytes)", out.display(), data.len());
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

/// What `run` shows about the cartridge it booted.
struct BootInfo {
    title: String,
    image_len: usize,
}

/// Boots a cartridge the way the console does: through the BIOS, which checks it and
/// shows any error itself. Assembly source isn't a cartridge; `asm -c` builds one.
fn boot(file: &Path, bios: Option<&Path>) -> Result<(p63_bus::Machine, BootInfo), String> {
    if is_source(file) {
        let cart = file.with_extension("p63c");
        return Err(format!(
            "{} is assembly source, not a cartridge. Build one first:\n  p63-sim asm {} -c {}",
            file.display(),
            file.display(),
            cart.display()
        ));
    }
    let cart = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let bios = find_bios(bios)?;
    let info = match p63_cart::Cart::parse(cart.clone()) {
        Ok(c) => BootInfo { title: c.title.clone(), image_len: c.image().len() },
        Err(e) => BootInfo { title: format!("bad cartridge, {e}"), image_len: 0 },
    };
    let mut machine = p63_bus::Machine::new();
    machine.boot_with_rom(load_bios(&bios)?, cart)?;
    Ok((machine, info))
}

fn is_source(file: &Path) -> bool {
    file.extension().is_some_and(|e| e.eq_ignore_ascii_case("p63") || e.eq_ignore_ascii_case("asm"))
}

/// The BIOS: the one given, else bios.bin next to p63-sim, else bios.bin here.
fn find_bios(given: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(path) = given {
        return Ok(path.to_path_buf());
    }
    let beside_exe = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("bios.bin")));
    beside_exe
        .into_iter()
        .chain([PathBuf::from("bios.bin")])
        .find(|p| p.is_file())
        .ok_or_else(|| "no BIOS: put bios.bin next to p63-sim or in this folder, or pass --bios FILE".to_string())
}

/// A signed boot ROM file: refused unless its signature verifies.
fn load_bios(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    p63_bus::bios::verify(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Per-second timing for `play --stats`: how evenly frames reach the window, and where the time goes.
#[derive(Default)]
struct Stats {
    /// gaps between one frame reaching the window and the next, in ms
    gaps: Vec<f64>,
    wait: std::time::Duration,
    emulate: std::time::Duration,
    draw: std::time::Duration,
    started: Option<std::time::Instant>,
    last: Option<std::time::Instant>,
}

impl Stats {
    fn frame(&mut self, wait: std::time::Duration, emulate: std::time::Duration, draw: std::time::Duration, queued: usize) {
        let now = std::time::Instant::now();
        let start = *self.started.get_or_insert(now);
        if let Some(last) = self.last {
            self.gaps.push((now - last).as_secs_f64() * 1000.0);
        }
        self.last = Some(now);
        self.wait += wait;
        self.emulate += emulate;
        self.draw += draw;
        let elapsed = (now - start).as_secs_f64();
        if elapsed >= 1.0 && !self.gaps.is_empty() {
            let n = self.gaps.len() as f64;
            let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0 / n;
            let mut sorted = self.gaps.clone();
            sorted.sort_by(f64::total_cmp);
            let pick = |p: f64| sorted[((sorted.len() - 1) as f64 * p) as usize];
            eprintln!(
                "{:3.0} fps | gaps p50 {:4.1} p95 {:4.1} max {:4.1} ms | early {:2} late {:2} | emulate {:4.1} draw {:4.1} wait {:4.1} ms | sound {:3.0} ms",
                n / elapsed,
                pick(0.5),
                pick(0.95),
                sorted[sorted.len() - 1],
                self.gaps.iter().filter(|&&g| g < 12.0).count(),
                self.gaps.iter().filter(|&&g| g > 21.0).count(),
                ms(self.emulate),
                ms(self.draw),
                ms(self.wait),
                queued as f64 * 1000.0 / f64::from(p63_apu::SAMPLE_RATE),
            );
            *self = Self { started: Some(now), last: Some(now), ..Self::default() };
        }
    }
}

/// Releases one frame every console frame period, on a fixed schedule.
struct FrameClock {
    period: std::time::Duration,
    next: std::time::Instant,
}

impl FrameClock {
    fn new() -> Self {
        let cycles = f64::from(p63_spec::DOTS_PER_LINE * p63_spec::LINES_PER_FRAME);
        let period = std::time::Duration::from_secs_f64(cycles / f64::from(p63_spec::CLOCK_HZ));
        Self { period, next: std::time::Instant::now() }
    }

    /// Waits for the next frame's slot: sleeps most of the way, then spins the last stretch exactly.
    fn wait(&mut self) {
        let now = std::time::Instant::now();
        if now > self.next + self.period * 4 {
            self.next = now; // fell far behind (window dragged, debugger...): start the schedule again
        }
        while let Some(left) = self.next.checked_duration_since(std::time::Instant::now()) {
            if left > std::time::Duration::from_millis(2) {
                std::thread::sleep(left - std::time::Duration::from_millis(2));
            } else {
                std::thread::yield_now();
            }
        }
        self.next += self.period;
    }
}

/// Asks Windows for 1 ms timer precision while playing, so the sleeps above land on time.
struct FineTimer;

#[cfg(windows)]
#[link(name = "winmm")]
extern "system" {
    fn timeBeginPeriod(ms: u32) -> u32;
    fn timeEndPeriod(ms: u32) -> u32;
}

impl FineTimer {
    fn new() -> Self {
        #[cfg(windows)]
        // SAFETY: plain winmm calls with no pointers; paired with timeEndPeriod in drop
        unsafe {
            timeBeginPeriod(1);
        }
        Self
    }
}

impl Drop for FineTimer {
    fn drop(&mut self) {
        #[cfg(windows)]
        // SAFETY: undoes the timeBeginPeriod(1) from new
        unsafe {
            timeEndPeriod(1);
        }
    }
}

/// Where a boot ROM's settings RAM is kept between runs: next to it, as .nvram.
fn nvram_path(bios: &Path) -> PathBuf {
    bios.with_extension("nvram")
}

/// A memory card file. One that doesn't exist yet is a new, formatted card.
fn read_card(path: &Path) -> Result<Vec<u8>, String> {
    match std::fs::read(path) {
        Ok(data) if data.len() == p63_cart::card::SIZE => Ok(data),
        Ok(_) => Err(format!("{}: not a memory card (a card file is exactly 1 MB)", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(p63_cart::card::blank()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn write_card(path: &Path, data: &[u8]) -> Result<(), String> {
    std::fs::write(path, data).map_err(|e| format!("{}: {e}", path.display()))
}

#[allow(clippy::too_many_arguments)]
fn run(
    file: &Path,
    bios: Option<&Path>,
    frames: u64,
    peek: &[String],
    hold: &[String],
    screenshot: Option<&Path>,
    wav: Option<&Path>,
    cards: [Option<&Path>; 2],
) -> Result<(), String> {
    let (mut machine, info) = boot(file, bios)?;
    println!("booted {:?} ({} byte image)", info.title, info.image_len);
    for (slot, path) in cards.iter().enumerate() {
        if let Some(path) = path {
            machine.cpu.bus.cart.insert_card(slot, read_card(path)?);
        }
    }
    for name in hold {
        machine.cpu.bus.keyboard.pad[0] |= button(name)?;
    }
    let mut sound = Vec::new();
    for frame in 0..frames {
        if let Err(e) = machine.run_frame() {
            print_registers(&machine.cpu, 0);
            return Err(format!("frame {frame}, at ${:06X}: {e}", machine.cpu.registers.pc()));
        }
        if wav.is_some() {
            sound.extend_from_slice(&machine.audio);
        }
    }
    println!("ran {frames} frames");
    for (slot, path) in cards.iter().enumerate() {
        if let (Some(path), Some(card)) = (path, &machine.cpu.bus.cart.cards[slot]) {
            if card.dirty || !path.exists() {
                write_card(path, &card.data)?;
                println!("saved memory card {} to {}", slot + 1, path.display());
            }
        }
    }
    if let Some(path) = wav {
        audio::save_wav(path, &sound)?;
        println!("saved {} ({} samples at {} Hz)", path.display(), sound.len(), p63_apu::SAMPLE_RATE);
    }
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
            (0..len).map(|i| format!("{:02X}", machine.cpu.bus.peek(p63_core::Addr::new(addr + i)))).collect();
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

fn call(file: &Path, routine: &str, regs: &[String], max_steps: u64, lib: &[PathBuf]) -> Result<(), String> {
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
    let program = p63_asm::assemble_in("call", &source, lib).map_err(|e| errors_to_string(&e))?;
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
