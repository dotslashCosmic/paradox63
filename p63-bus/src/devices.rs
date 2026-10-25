//! I/O devices: video registers, keyboard/pad, cart registers, audio.

use p63_spec as spec;

/// A memory-mapped device; `off` is relative to the device's base address.
pub trait Device {
    fn read(&mut self, off: u32) -> u8;
    fn write(&mut self, off: u32, value: u8);
}

const VBLANK: u8 = 0x01;
const RASTER: u8 = 0x02;

/// Video registers. The frame loop drives `line`, `stat` and `irqst`.
#[derive(Debug, Clone)]
pub struct Video {
    regs: [u8; 64],
    pub line: u16,
    pub stat: u8,
    pub irqst: u8,
}

impl Default for Video {
    fn default() -> Self {
        Self { regs: [0; 64], line: 0, stat: 0, irqst: 0 }
    }
}

impl Video {
    fn reg(addr: u32) -> u32 {
        addr - spec::VIDEO_IO_BASE
    }

    /// Value last written to a register, by absolute address.
    #[must_use]
    pub fn register(&self, addr: u32) -> u8 {
        self.regs[Self::reg(addr) as usize]
    }

    /// True while an enabled video interrupt is pending (`/IRQ` held low).
    #[must_use]
    pub fn irq(&self) -> bool {
        self.irqst & self.register(spec::V_IRQEN) != 0
    }

    /// Called at the start of each scanline.
    pub fn start_line(&mut self, line: u16) {
        self.line = line;
        if line == spec::SCREEN_H as u16 {
            self.stat |= VBLANK;
            self.irqst |= VBLANK;
        } else if line == 0 {
            self.stat &= !VBLANK;
        }
        if line == u16::from(self.register(spec::V_IRQLINE)) {
            self.irqst |= RASTER;
        }
    }
}

impl Device for Video {
    fn read(&mut self, off: u32) -> u8 {
        match off + spec::VIDEO_IO_BASE {
            spec::V_LINE => self.line.min(255) as u8,
            spec::V_STAT => self.stat,
            spec::V_IRQST => self.irqst,
            _ => self.regs[off as usize],
        }
    }

    fn write(&mut self, off: u32, value: u8) {
        match off + spec::VIDEO_IO_BASE {
            spec::V_LINE | spec::V_STAT => {}
            spec::V_IRQST => self.irqst &= !value,
            _ => self.regs[off as usize] = value,
        }
    }
}

/// Keyboard controller; only the gamepad byte is live so far.
#[derive(Debug, Clone, Default)]
pub struct Keyboard {
    regs: [u8; 32],
    /// Gamepad buttons, one bit each.
    pub pad: [u8; 2],
}

impl Device for Keyboard {
    fn read(&mut self, off: u32) -> u8 {
        let pad = spec::KB_PAD - spec::KB_IO_BASE;
        if (pad..pad + 2).contains(&off) {
            self.pad[(off - pad) as usize]
        } else {
            self.regs[off as usize]
        }
    }

    fn write(&mut self, off: u32, value: u8) {
        self.regs[off as usize] = value;
    }
}

/// Cartridge registers plus the banked 64 KB window onto the cart data.
#[derive(Debug, Clone, Default)]
pub struct CartPort {
    pub data: Vec<u8>,
    bank: u8,
}

impl CartPort {
    pub fn window(&self, off: u32) -> u8 {
        let at = usize::from(self.bank) * spec::CART_BANK_SIZE as usize + off as usize;
        self.data.get(at).copied().unwrap_or(0xFF)
    }
}

impl Device for CartPort {
    fn read(&mut self, off: u32) -> u8 {
        match off + spec::CART_STAT {
            spec::CART_STAT => u8::from(!self.data.is_empty()),
            spec::CART_BANK => self.bank,
            _ => 0,
        }
    }

    fn write(&mut self, off: u32, value: u8) {
        if off + spec::CART_STAT == spec::CART_BANK {
            self.bank = value;
        }
    }
}

impl Device for p63_apu::Apu {
    fn read(&mut self, off: u32) -> u8 {
        p63_apu::Apu::read(self, off)
    }

    fn write(&mut self, off: u32, value: u8) {
        p63_apu::Apu::write(self, off, value);
    }
}
