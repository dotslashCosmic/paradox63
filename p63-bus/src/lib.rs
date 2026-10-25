//! p63-bus: the Paradox63 memory map, its devices, and the frame loop.

pub mod devices;

use devices::{Audio, CartPort, Device, Keyboard, Video};
use p63_cart::{BootError, Cart};
use p63_core::{Addr, Bus, CpuError, Flags, ReferenceInterpreter};
use p63_spec as spec;

/// Every address, routed to RAM, ROM or a device.
pub struct SystemBus {
    ram: Vec<u8>,
    pub video: Video,
    pub keyboard: Keyboard,
    pub cart: CartPort,
    pub audio: Audio,
}

impl Default for SystemBus {
    fn default() -> Self {
        Self {
            ram: vec![0; 0x0100_0000],
            video: Video::default(),
            keyboard: Keyboard::default(),
            cart: CartPort::default(),
            audio: Audio::default(),
        }
    }
}

impl SystemBus {
    /// The whole address space as the video chip sees it (VRAM, OAM, palette).
    #[must_use]
    pub fn memory(&self) -> &[u8] {
        &self.ram
    }
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
            // no boot ROM image; the high-level boot does its job
            0xFF
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
        if a < 4 || within(a, spec::BOOT_ROM, spec::CART_WIN_END) || within(a, spec::VRAM_END + 1, spec::CART_STAT - 1) {
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

/// CPU cycles per scanline: the CPU runs one cycle per dot.
pub const CYCLES_PER_LINE: u32 = spec::DOTS_PER_LINE;

/// The whole console: CPU, memory map and devices.
pub struct Machine {
    pub cpu: ReferenceInterpreter<SystemBus>,
    pub frames: u64,
    /// The last finished picture, `p63_ppu::WIDTH * HEIGHT` pixels of 0x00RRGGBB.
    pub frame: Vec<u32>,
    /// Cycles run past the end of the last line, carried into the next.
    debt: u32,
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
            debt: 0,
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

    /// Runs one frame: 262 lines of 455 CPU cycles, with vertical blank at line 240.
    pub fn run_frame(&mut self) -> Result<(), CpuError> {
        for line in 0..spec::LINES_PER_FRAME as u16 {
            self.cpu.bus.video.start_line(line);
            if line == spec::SCREEN_H as u16 {
                self.render();
                self.cpu.signal_vblank();
            }
            let mut spent = self.debt;
            while spent < CYCLES_PER_LINE {
                self.cpu.set_irq_pending(self.cpu.bus.video.irq());
                spent += self.cpu.step()?.0;
            }
            self.debt = spent - CYCLES_PER_LINE;
        }
        self.frames += 1;
        Ok(())
    }

    /// Draws the picture from VRAM and OAM as they stand now.
    fn render(&mut self) {
        let video = &self.cpu.bus.video;
        let regs = p63_ppu::Regs { ctrl: video.register(spec::V_CTRL), bright: video.register(spec::V_BRIGHT) };
        let overflow = p63_ppu::render(self.cpu.bus.memory(), regs, &mut self.frame);
        let video = &mut self.cpu.bus.video;
        if overflow {
            video.stat |= p63_ppu::STAT_OVERFLOW;
        } else {
            video.stat &= !p63_ppu::STAT_OVERFLOW;
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
