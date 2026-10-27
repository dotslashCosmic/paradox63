//! p63-bus: the Paradox63 memory map, its devices, and the frame loop.

pub mod bios;
pub mod devices;

use devices::{CartPort, Device, Keyboard, Video};
use p63_apu::Apu;
use p63_cart::{crypt, BootError, Cart};
use p63_core::{Addr, Bus, CpuError, Flags, ReferenceInterpreter};
use p63_spec as spec;

/// Every address, routed to RAM, ROM or a device.
pub struct SystemBus {
    ram: Vec<u8>,
    /// Boot ROM image at `BOOT_ROM`; empty when the high-level boot is used.
    pub boot_rom: bios::BootRom,
    /// Battery-backed settings RAM at `NVRAM_BASE`, kept by the boot ROM.
    pub nvram: [u8; NVRAM_LEN],
    /// Set when the settings RAM was written, so the frontend can save it.
    pub nvram_dirty: bool,
    /// Set by a write to `SYS_LOCK`: the boot ROM reads $FF until the next reset.
    pub boot_locked: bool,
    /// The last value written to `SYS_RESET`, kept over the reset (0 at power-on).
    pub reset_reason: u8,
    /// A write to `SYS_RESET` waiting for the machine to act on it.
    pub reset_requested: bool,
    /// The CPU clock in use (`SPEED_NORMAL`, `SPEED_TURBO` or `SPEED_LOW`).
    pub speed: u8,
    /// The clock the program picked, and a pick waiting for the next vblank.
    pub speed_program: u8,
    pub speed_requested: Option<u8>,
    /// The front panel's speed switch: forces a clock whatever the program asks for.
    pub speed_forced: Option<u8>,
    pub engine: Engine,
    /// Cycles the engine took from the CPU, still to be counted.
    pub stall: u32,
    pub video: Video,
    pub keyboard: Keyboard,
    pub cart: CartPort,
    pub audio: Apu,
}

impl Default for SystemBus {
    fn default() -> Self {
        Self {
            ram: vec![0; 0x0100_0000],
            boot_rom: bios::BootRom::default(),
            nvram: [0; NVRAM_LEN],
            nvram_dirty: false,
            boot_locked: false,
            reset_reason: 0,
            reset_requested: false,
            speed: SPEED_NORMAL,
            speed_program: SPEED_NORMAL,
            speed_requested: None,
            speed_forced: None,
            engine: Engine::default(),
            stall: 0,
            video: Video::default(),
            keyboard: Keyboard::default(),
            cart: CartPort::default(),
            audio: Apu::default(),
        }
    }
}

impl SystemBus {
    /// The whole address space as the video chip sees it (VRAM, OAM, palette).
    #[must_use]
    pub fn memory(&self) -> &[u8] {
        &self.ram
    }

    /// A read for tools (peeks, debuggers): as the CPU would see it, except that the
    /// boot ROM always reads $FF, locked or not.
    pub fn peek(&mut self, addr: Addr) -> u8 {
        if within(addr.0, spec::BOOT_ROM, spec::CART_WIN - 1) {
            0xFF
        } else {
            self.read(addr)
        }
    }

    /// The engine's job: 1 checks the cart, 2 also loads its image at `ENGINE_DST`.
    fn run_engine(&mut self, op: u8) {
        let status = self.engine_status(op).unwrap_or(ENGINE_FAIL);
        self.engine.status = status;
    }

    fn engine_status(&mut self, op: u8) -> Option<u8> {
        let e = &self.engine;
        let (check, load) = (spec::ENGINE_CHECK as u8, spec::ENGINE_LOAD as u8);
        if e.locked || (op != check && op != load) {
            return None;
        }
        let data = &self.cart.data;
        let layout = crypt::Layout::of(data).ok()?;
        if layout.sealed {
            self.stall += ENGINE_SEAL_CYCLES + layout.signed_len() as u32;
            if !crypt::seal_ok(data, layout, &e.seal) {
                return None;
            }
        }
        if op == load {
            let end = e.dst as usize + layout.image_len;
            if end > spec::MAIN_TOP as usize + 1 {
                return None;
            }
            let image = crypt::image(data, layout, &e.key);
            self.ram[e.dst as usize..end].copy_from_slice(&image);
            self.stall += layout.image_len as u32 * ENGINE_CYCLES_PER_BYTE;
        }
        Some(ENGINE_OK | if layout.sealed { ENGINE_SEALED } else { 0 })
    }

    /// Runs the audio engine for one output tick over Audio RAM.
    pub fn audio_tick(&mut self) -> (i16, i16) {
        self.audio.tick(&self.ram[spec::ARAM_BASE as usize..=spec::ARAM_END as usize])
    }
}

/// The settings RAM: 64 bytes inside the reserved I/O block.
pub use spec::NVRAM_BASE;
pub const NVRAM_LEN: usize = 64;
const NVRAM_END: u32 = NVRAM_BASE + NVRAM_LEN as u32 - 1;

/// The cartridge engine: copies the cart's image into RAM, decrypting it and
/// checking its seal on the way, with keys the BIOS gives it.
pub use spec::{ENGINE_CTRL, ENGINE_DST, ENGINE_KEY, ENGINE_SEAL};
const ENGINE_END: u32 = ENGINE_CTRL;
pub const ENGINE_OK: u8 = spec::ENGINE_OK as u8;
pub const ENGINE_SEALED: u8 = spec::ENGINE_SEALED as u8;
pub const ENGINE_FAIL: u8 = spec::ENGINE_FAIL as u8;
/// CPU cycles the engine holds the bus for: per byte loaded, and per seal check.
const ENGINE_CYCLES_PER_BYTE: u32 = 2;
const ENGINE_SEAL_CYCLES: u32 = 20_000;

/// Any write hides the boot ROM until the next reset; reads 1 while hidden.
/// Any write to `SYS_RESET` resets the console; it reads the value last written.
/// `SYS_SPEED` picks the CPU clock from the next vblank and reads the one in use.
pub use spec::{SYS_LOCK, SYS_RESET, SYS_SPEED};
pub const SPEED_NORMAL: u8 = spec::SPEED_NORMAL as u8;
pub const SPEED_TURBO: u8 = spec::SPEED_TURBO as u8;
pub const SPEED_LOW: u8 = spec::SPEED_LOW as u8;

/// Master clock ticks per CPU cycle at a speed: the master clock is twice the dot clock.
#[must_use]
pub fn ticks_per_cycle(speed: u8) -> u32 {
    match speed {
        SPEED_TURBO => 1,
        SPEED_LOW => 4,
        _ => 2,
    }
}

/// The cartridge engine's state. Its keys can be written, never read, and are wiped
/// when the boot ROM locks.
#[derive(Default)]
pub struct Engine {
    key: [u8; 32],
    seal: [u8; 32],
    dst: u32,
    status: u8,
    locked: bool,
}

fn within(a: u32, base: u32, end: u32) -> bool {
    (base..=end).contains(&a)
}

impl Bus for SystemBus {
    fn read(&mut self, addr: Addr) -> u8 {
        let a = addr.0;
        if a < 4 {
            spec::LOW_VECTOR_ROM[a as usize]
        } else if a <= spec::MAIN_TOP {
            self.ram[a as usize]
        } else if within(a, spec::BOOT_ROM, spec::CART_WIN - 1) {
            // without a boot ROM image the high-level boot does its job
            if self.boot_locked {
                0xFF
            } else {
                self.boot_rom.get((a - spec::BOOT_ROM) as usize).unwrap_or(0xFF)
            }
        } else if within(a, spec::CART_WIN, spec::CART_WIN_END) {
            self.cart.window(a - spec::CART_WIN)
        } else if within(a, spec::VIDEO_IO_BASE, spec::VIDEO_IO_END) {
            self.video.read(a - spec::VIDEO_IO_BASE)
        } else if within(a, spec::KB_IO_BASE, spec::KB_IO_END) {
            self.keyboard.read(a - spec::KB_IO_BASE)
        } else if within(a, spec::AUDIO_IO_BASE, spec::AUDIO_IO_END) {
            self.audio.read(a - spec::AUDIO_IO_BASE)
        } else if within(a, spec::CART_STAT, spec::AUDIO_IO_BASE - 1) {
            self.cart.read(a - spec::CART_STAT)
        } else if within(a, NVRAM_BASE, NVRAM_END) {
            self.nvram[(a - NVRAM_BASE) as usize]
        } else if within(a, ENGINE_DST, ENGINE_DST + 2) {
            (self.engine.dst >> (8 * (a - ENGINE_DST))) as u8
        } else if a == ENGINE_CTRL {
            self.engine.status
        } else if a == SYS_LOCK {
            u8::from(self.boot_locked)
        } else if a == SYS_RESET {
            self.reset_reason
        } else if a == SYS_SPEED {
            self.speed
        } else if within(a, spec::VRAM_END + 1, spec::CART_STAT - 1) {
            // reserved I/O
            0xFF
        } else {
            // audio RAM, OAM, VRAM and the high vectors are plain RAM
            self.ram[a as usize]
        }
    }

    fn write(&mut self, addr: Addr, value: u8) {
        let a = addr.0;
        if within(a, NVRAM_BASE, NVRAM_END) {
            self.nvram[(a - NVRAM_BASE) as usize] = value;
            self.nvram_dirty = true;
        } else if within(a, ENGINE_KEY, ENGINE_END) {
            let e = &mut self.engine;
            match a - ENGINE_KEY {
                off @ 0x00..=0x1F if !e.locked => e.key[off as usize] = value,
                off @ 0x20..=0x3F if !e.locked => e.seal[off as usize - 0x20] = value,
                off @ 0x40..=0x42 => {
                    let shift = 8 * (off - 0x40);
                    e.dst = (e.dst & !(0xFF << shift)) | u32::from(value) << shift;
                }
                0x43 => self.run_engine(value),
                _ => {}
            }
        } else if a == SYS_LOCK {
            // the engine's keys go with the boot ROM
            self.boot_locked = true;
            self.engine = Engine { locked: true, ..Engine::default() };
        } else if a == SYS_RESET {
            self.reset_reason = value;
            self.reset_requested = true;
        } else if a == SYS_SPEED {
            if matches!(value, SPEED_NORMAL | SPEED_TURBO | SPEED_LOW) {
                self.speed_requested = Some(value);
            }
        } else if a < 4 || within(a, spec::BOOT_ROM, spec::CART_WIN_END) || within(a, spec::VRAM_END + 1, spec::CART_STAT - 1) {
            // ROM, the cart window and reserved I/O ignore writes
        } else if a <= spec::MAIN_TOP {
            self.ram[a as usize] = value;
        } else if within(a, spec::VIDEO_IO_BASE, spec::VIDEO_IO_END) {
            self.video.write(a - spec::VIDEO_IO_BASE, value);
        } else if within(a, spec::KB_IO_BASE, spec::KB_IO_END) {
            self.keyboard.write(a - spec::KB_IO_BASE, value);
        } else if within(a, spec::AUDIO_IO_BASE, spec::AUDIO_IO_END) {
            self.audio.write(a - spec::AUDIO_IO_BASE, value);
        } else if within(a, spec::CART_STAT, spec::AUDIO_IO_BASE - 1) {
            self.cart.write(a - spec::CART_STAT, value);
        } else {
            self.ram[a as usize] = value;
        }
    }
}

/// CPU cycles per scanline at normal speed, where the CPU runs one cycle per dot.
pub const CYCLES_PER_LINE: u32 = spec::DOTS_PER_LINE;
/// Master clock ticks per scanline.
const TICKS_PER_LINE: u32 = spec::DOTS_PER_LINE * 2;

/// The whole console: CPU, memory map and devices.
pub struct Machine {
    pub cpu: ReferenceInterpreter<SystemBus>,
    pub frames: u64,
    /// The last finished picture, `p63_ppu::WIDTH * HEIGHT` pixels of 0x00RRGGBB.
    pub frame: Vec<u32>,
    /// Stereo samples from the last frame, `p63_apu::SAMPLE_RATE` per second.
    pub audio: Vec<(i16, i16)>,
    /// Master ticks run past the end of the last line, carried into the next.
    debt: u32,
    /// Master ticks since the last audio tick.
    audio_ticks: u32,
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine {
    #[must_use]
    pub fn new() -> Self {
        Self {
            cpu: ReferenceInterpreter::new(SystemBus::default()),
            frames: 0,
            frame: vec![0; p63_ppu::WIDTH * p63_ppu::HEIGHT],
            audio: Vec::new(),
            debt: 0,
            audio_ticks: 0,
        }
    }

    /// High-level boot: checks the cart like the boot ROM, loads it and jumps to it.
    pub fn boot(&mut self, cart_file: Vec<u8>) -> Result<Cart, BootError> {
        let cart = Cart::parse(cart_file)?;
        cart.load(&mut self.cpu.bus);
        self.cpu.bus.cart.data.clone_from(&cart.data);
        let r = &mut self.cpu.registers;
        r.set_sp(spec::SP_RESET);
        r.set_cc(Flags::F | Flags::I);
        r.set_pc(spec::FIXED_ENTRY);
        Ok(cart)
    }

    /// Boots through a boot ROM image: the cartridge goes in as it is and the CPU
    /// starts from RESET, so the boot ROM does the checking and loading.
    pub fn boot_with_rom(&mut self, boot_rom: Vec<u8>, cart_file: Vec<u8>) -> Result<(), String> {
        let size = (spec::CART_WIN - spec::BOOT_ROM) as usize;
        if boot_rom.len() > size {
            return Err(format!("boot ROM is {} bytes; the slot holds {size}", boot_rom.len()));
        }
        self.cpu.bus.boot_rom = bios::BootRom::new(boot_rom);
        self.cpu.bus.cart.data = cart_file;
        self.cpu.reset();
        Ok(())
    }

    /// The reset line: the CPU starts over from RESET, the boot ROM shows again and the
    /// chips go back to power-on. RAM, settings RAM and the reset reason stay.
    pub fn reset(&mut self) {
        let bus = &mut self.cpu.bus;
        bus.boot_locked = false;
        bus.reset_requested = false;
        bus.speed_program = SPEED_NORMAL;
        bus.speed_requested = None;
        bus.speed = bus.speed_forced.unwrap_or(SPEED_NORMAL);
        bus.engine = Engine::default();
        bus.video = Video::default();
        bus.audio = Apu::default();
        bus.cart.reset();
        self.cpu.reset();
    }

    /// Runs one frame: 262 lines of 455 dots, with vertical blank at line 240. A line is
    /// 455 CPU cycles at normal speed, 910 in turbo and 227.5 in low power.
    /// Each visible line is drawn as it starts, so changes made during the frame show
    /// from the next line on.
    pub fn run_frame(&mut self) -> Result<(), CpuError> {
        self.audio.clear();
        for line in 0..spec::LINES_PER_FRAME as u16 {
            self.cpu.bus.video.start_line(line);
            if usize::from(line) < p63_ppu::HEIGHT {
                self.render_line(usize::from(line));
            } else if line == spec::SCREEN_H as u16 {
                self.switch_speed();
                self.cpu.signal_vblank();
            }
            let mut spent = self.debt;
            while spent < TICKS_PER_LINE {
                self.cpu.bus.video.dot = spent / 2;
                self.cpu.set_irq_pending(self.cpu.bus.video.irq());
                self.cpu.set_firq_pending(self.cpu.bus.audio.firq());
                let cycles = self.cpu.step()?.0 + std::mem::take(&mut self.cpu.bus.stall);
                if self.cpu.bus.reset_requested {
                    self.reset();
                }
                let ticks = cycles * ticks_per_cycle(self.cpu.bus.speed);
                spent += ticks;
                self.audio_ticks += ticks;
                while self.audio_ticks >= p63_apu::TICK_CYCLES * 2 {
                    self.audio_ticks -= p63_apu::TICK_CYCLES * 2;
                    let sample = self.cpu.bus.audio_tick();
                    self.audio.push(sample);
                }
            }
            self.debt = spent - TICKS_PER_LINE;
        }
        self.frames += 1;
        Ok(())
    }

    /// At vblank: the front panel's switch if it's set, else what the program asked for.
    fn switch_speed(&mut self) {
        let bus = &mut self.cpu.bus;
        if let Some(asked) = bus.speed_requested.take() {
            bus.speed_program = asked;
        }
        bus.speed = bus.speed_forced.unwrap_or(bus.speed_program);
    }

    /// Draws one line from VRAM, OAM and the registers as they stand now.
    fn render_line(&mut self, line: usize) {
        let video = &self.cpu.bus.video;
        let word = |a: u32| u16::from_le_bytes([video.register(a), video.register(a + 1)]);
        let regs = p63_ppu::Regs {
            ctrl: video.register(spec::V_CTRL),
            bright: video.register(spec::V_BRIGHT),
            scroll_x0: word(spec::V_SCROLLX0),
            scroll_y0: word(spec::V_SCROLLY0),
            scroll_x1: word(spec::V_SCROLLX1),
            scroll_y1: word(spec::V_SCROLLY1),
            bank: video.register(spec::V_BANK),
            palgrp: video.register(spec::V_PALGRP),
        };
        if p63_ppu::render_line(self.cpu.bus.memory(), regs, line, &mut self.frame) {
            self.cpu.bus.video.stat |= p63_ppu::STAT_OVERFLOW;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(bus: &mut SystemBus, a: u32) -> u8 {
        bus.read(Addr::new(a))
    }

    #[test]
    fn regions_route_correctly() {
        let mut bus = SystemBus::default();
        for a in [0x0000_0004, spec::MAIN_BASE, spec::MAIN_TOP, spec::OAM_BASE, spec::VRAM_END, spec::VEC_IRQ] {
            bus.write(Addr::new(a), 0x5A);
            assert_eq!(read(&mut bus, a), 0x5A, "RAM at ${a:06X}");
        }
        for a in [0u32, 3, spec::BOOT_ROM, spec::CART_WIN, spec::CART_WIN_END, spec::VRAM_END + 1] {
            let before = read(&mut bus, a);
            bus.write(Addr::new(a), 0x5A);
            assert_eq!(read(&mut bus, a), before, "${a:06X} ignores writes");
        }
        assert_eq!(read(&mut bus, 1), spec::LOW_VECTOR_ROM[1]);
        assert_eq!(read(&mut bus, spec::VRAM_END + 1), 0xFF, "reserved I/O reads $FF");
    }

    #[test]
    fn settings_ram_keeps_what_is_written() {
        let mut bus = SystemBus::default();
        assert!(!bus.nvram_dirty);
        bus.write(Addr::new(NVRAM_BASE + 2), 0x63);
        assert_eq!(read(&mut bus, NVRAM_BASE + 2), 0x63);
        assert_eq!(bus.nvram[2], 0x63);
        assert!(bus.nvram_dirty, "a write marks it for saving");
        assert_eq!(read(&mut bus, NVRAM_BASE - 1), 0xFF, "the reserved block around it is unchanged");
        assert_eq!(read(&mut bus, NVRAM_BASE + NVRAM_LEN as u32), 0xFF);
    }

    #[test]
    fn boot_rom_hides_until_reset() {
        let mut m = Machine::new();
        m.cpu.bus.boot_rom = bios::BootRom::new(vec![0x12, 0x34]);
        assert_eq!(read(&mut m.cpu.bus, spec::BOOT_ROM), 0x12);
        assert_eq!(m.cpu.bus.peek(Addr::new(spec::BOOT_ROM)), 0xFF, "tools never see it");
        m.cpu.bus.write(Addr::new(SYS_LOCK), 0);
        assert_eq!(read(&mut m.cpu.bus, spec::BOOT_ROM), 0xFF, "hidden once locked");
        assert_eq!(read(&mut m.cpu.bus, SYS_LOCK), 1);
        m.cpu.bus.write(Addr::new(SYS_LOCK + 1), 0x0B);
        assert!(m.cpu.bus.reset_requested);
        m.reset();
        assert_eq!(read(&mut m.cpu.bus, spec::BOOT_ROM), 0x12, "back after a reset");
        assert_eq!(read(&mut m.cpu.bus, SYS_RESET), 0x0B, "the reason stays");
    }

    #[test]
    fn engine_loads_an_encrypted_cart_until_locked() {
        let secret = [7u8; 32];
        let plain = p63_cart::pack(&[1, 2, 3, 4], &p63_cart::PackOptions::default()).unwrap();
        let mut bus = SystemBus::default();
        bus.cart.data = crypt::encrypt(&plain, &crypt::public_key(secret), [9u8; 32]);
        for (i, b) in secret.into_iter().enumerate() {
            bus.write(Addr::new(ENGINE_KEY + i as u32), b);
        }
        assert_eq!(read(&mut bus, ENGINE_KEY), 0xFF, "the key can't be read back");
        bus.write_u24_le(Addr::new(ENGINE_DST), 0x2000);
        bus.write(Addr::new(ENGINE_CTRL), 2);
        assert_eq!(read(&mut bus, ENGINE_CTRL), ENGINE_OK);
        assert_eq!(&bus.memory()[0x2000..0x2004], &[1, 2, 3, 4]);
        assert!(bus.stall > 0, "the copy takes bus time");
        bus.write(Addr::new(SYS_LOCK), 0);
        bus.write(Addr::new(ENGINE_CTRL), 2);
        assert_eq!(read(&mut bus, ENGINE_CTRL), ENGINE_FAIL, "nothing once locked");
        bus.write(Addr::new(ENGINE_KEY), 1);
        assert_eq!(bus.engine.key, [0; 32], "and no new keys");
    }

    #[test]
    fn video_irq_is_write_one_to_clear() {
        let mut bus = SystemBus::default();
        bus.write(Addr::new(spec::V_IRQEN), 0x01);
        bus.video.start_line(spec::SCREEN_H as u16);
        assert!(bus.video.irq());
        assert_eq!(read(&mut bus, spec::V_STAT) & 1, 1);
        bus.write(Addr::new(spec::V_IRQST), 0x01);
        assert!(!bus.video.irq());
    }

    #[test]
    fn keyboard_pad_reads_through() {
        let mut bus = SystemBus::default();
        bus.keyboard.pad[0] = 0x81;
        assert_eq!(read(&mut bus, spec::KB_PAD), 0x81);
    }

    #[test]
    fn frame_wakes_waitv_and_counts_lines() {
        // MAIN: WAITV / INC GP / BRA MAIN
        let image = [0xB1, 0x00, 0x18, 0x07, 0x30, 0xFA];
        let cart = p63_cart::pack(&image, &p63_cart::PackOptions::default()).unwrap();
        let mut m = Machine::new();
        m.boot(cart).unwrap();
        for _ in 0..10 {
            m.run_frame().unwrap();
        }
        assert_eq!(m.cpu.registers.gp(), 10, "one wake-up per frame");
    }

    #[test]
    fn frame_makes_audio_and_endx_drives_firq() {
        // MAIN: WAITV / INC GP / BRA MAIN
        let image = [0xB1, 0x00, 0x18, 0x07, 0x30, 0xFA];
        let mut m = Machine::new();
        m.boot(p63_cart::pack(&image, &p63_cart::PackOptions::default()).unwrap()).unwrap();
        let bus = &mut m.cpu.bus;
        // directory at Audio RAM $0000: sample 0 = 16 bytes of 100 at $0100, one-shot
        for (i, b) in [0x00, 0x01, 0x00, 0x01, 0x10, 0x01].into_iter().enumerate() {
            bus.write(Addr::new(spec::ARAM_BASE + i as u32), b);
        }
        for i in 0..16 {
            bus.write(Addr::new(spec::ARAM_BASE + 0x100 + i), 100);
        }
        for (a, v) in [
            (spec::AUD_ACTRL, p63_apu::ACTRL_ENABLE),
            (spec::AUD_MVOL_L, 255),
            (spec::AUD_V0_VOL_L, 255),
            (spec::AUD_V0_PITCH_H, 0x10),
            (spec::AUD_V0_ADSR1, 0xF0),
            (spec::AUD_V0_ADSR2, 0xF0),
            (spec::AUD_IRQEN, 0x01),
            (spec::AUD_KON, 0x01),
        ] {
            bus.write(Addr::new(a), v);
        }
        m.run_frame().unwrap();
        let per_frame = spec::DOTS_PER_LINE * spec::LINES_PER_FRAME / p63_apu::TICK_CYCLES;
        assert!((per_frame..=per_frame + 1).contains(&(m.audio.len() as u32)), "{} samples", m.audio.len());
        assert!(m.audio[..16].iter().all(|&(l, r)| l > 0 && r == 0), "voice 0 is left only");
        assert!(m.audio[16..].iter().all(|&s| s == (0, 0)), "one-shot ends");
        assert!(m.cpu.firq_pending(), "ENDX with IRQEN holds /FIRQ");
        m.cpu.bus.write(Addr::new(spec::AUD_ENDX), 0x01);
        m.run_frame().unwrap();
        assert!(!m.cpu.firq_pending());
    }

    #[test]
    fn vblank_irq_runs_the_handler_each_frame() {
        // MAIN: CLI / loop: BRA loop
        // handler at $0110: ADDZ $2000,#1 / LD A,#1 / ST A,V_IRQST / RTI
        // (the count lives in memory: RTI restores every register)
        let mut image = vec![0u8; 0x20];
        image[..4].copy_from_slice(&[0x74, 0x00, 0x30, 0xFE]);
        let v = spec::V_IRQST;
        let handler = [
            0x2F, 0x00, 0x20, 0x00, 0x01, 0x01, 0x01, 0xC7, v as u8, (v >> 8) as u8, (v >> 16) as u8, 0x43, 0x00,
        ];
        image[0x10..0x10 + handler.len()].copy_from_slice(&handler);
        let opts = p63_cart::PackOptions {
            vectors: p63_cart::Vectors { irq: spec::FIXED_ENTRY + 0x10, ..Default::default() },
            ..Default::default()
        };
        let mut m = Machine::new();
        m.boot(p63_cart::pack(&image, &opts).unwrap()).unwrap();
        m.cpu.bus.write(Addr::new(spec::V_IRQEN), 0x01);
        for _ in 0..5 {
            m.run_frame().unwrap();
        }
        assert_eq!(m.cpu.bus.read(Addr::new(0x2000)), 5);
    }
}
