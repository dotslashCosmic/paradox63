//! I/O devices: video registers, keyboard/pad, cart registers, audio.

use p63_spec as spec;

/// A memory-mapped device; `off` is relative to the device's base address.
pub trait Device {
    fn read(&mut self, off: u32) -> u8;
    fn write(&mut self, off: u32, value: u8);
}

const VBLANK: u8 = 0x01;
const RASTER: u8 = 0x02;
const STAT_HBLANK: u8 = 0x02;
const STAT_OVERFLOW: u8 = 0x04;

/// Video registers. The frame loop drives `line`, `dot`, `stat` and `irqst`.
#[derive(Debug, Clone)]
pub struct Video {
    regs: [u8; 64],
    pub line: u16,
    /// Cycles into the current line; HBlank is dots 320-454.
    pub dot: u32,
    pub stat: u8,
    pub irqst: u8,
}

impl Default for Video {
    fn default() -> Self {
        Self { regs: [0; 64], line: 0, dot: 0, stat: 0, irqst: 0 }
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
            self.stat &= !STAT_OVERFLOW;
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
            spec::V_STAT => self.stat | if self.dot >= spec::SCREEN_W { STAT_HBLANK } else { 0 },
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

/// The cart registers one program has set: bank, control and save bank.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CartView {
    pub bank: u8,
    pub ctrl: u8,
    pub sbank: u8,
}

const CTRL_CARD: u8 = spec::CARTC_CARD as u8;
const CTRL_WREN: u8 = spec::CARTC_WREN as u8;
const CTRL_SLOT2: u8 = spec::CARTC_SLOT2 as u8;
const STAT_NEW: [u8; 2] = [spec::CARTS_NEW1 as u8, spec::CARTS_NEW2 as u8];

/// A memory card in one of the two card slots.
#[derive(Debug, Clone)]
pub struct MemoryCard {
    /// Always `CARD_SIZE` bytes.
    pub data: Vec<u8>,
    /// Set when the card was written, so the frontend can keep it.
    pub dirty: bool,
}

/// Cartridge registers plus the banked 64 KB window onto the cart data, or onto a
/// memory card.
#[derive(Debug, Clone, Default)]
pub struct CartPort {
    pub data: Vec<u8>,
    pub cards: [Option<MemoryCard>; 2],
    /// `CARTS_NEW1` and `CARTS_NEW2`: a card went in or out since the program cleared them.
    changed: u8,
    bank: u8,
    ctrl: u8,
    sbank: u8,
}

impl CartPort {
    pub fn insert(&mut self, data: Vec<u8>) {
        self.data = data;
    }

    /// Puts a card in slot 0 or 1. A short image is padded as erased flash.
    pub fn insert_card(&mut self, slot: usize, mut data: Vec<u8>) {
        data.resize(spec::CARD_SIZE as usize, 0xFF);
        self.cards[slot] = Some(MemoryCard { data, dirty: false });
        self.changed |= STAT_NEW[slot];
    }

    /// Takes the card out of slot 0 or 1, if there is one.
    pub fn eject_card(&mut self, slot: usize) -> Option<MemoryCard> {
        self.changed |= STAT_NEW[slot];
        self.cards[slot].take()
    }

    #[must_use]
    pub fn view(&self) -> CartView {
        CartView { bank: self.bank, ctrl: self.ctrl, sbank: self.sbank }
    }

    pub fn set_view(&mut self, v: CartView) {
        self.bank = v.bank;
        self.ctrl = v.ctrl;
        self.sbank = v.sbank;
    }

    /// Back to bank 0 with the window on ROM, as at power-on.
    pub fn reset(&mut self) {
        self.bank = 0;
        self.ctrl = 0;
        self.sbank = 0;
    }

    /// The card the window shows, as (slot, byte offset), while `CARTC_CARD` is set.
    fn card_at(&self, off: u32) -> Option<(usize, usize)> {
        if self.ctrl & CTRL_CARD == 0 {
            return None;
        }
        let slot = usize::from(self.ctrl & CTRL_SLOT2 != 0);
        Some((slot, usize::from(self.sbank) * spec::CART_BANK_SIZE as usize + off as usize))
    }

    pub fn window(&self, off: u32) -> u8 {
        if let Some((slot, at)) = self.card_at(off) {
            return self.cards[slot].as_ref().and_then(|c| c.data.get(at)).copied().unwrap_or(0xFF);
        }
        let at = usize::from(self.bank) * spec::CART_BANK_SIZE as usize + off as usize;
        self.data.get(at).copied().unwrap_or(0xFF)
    }

    /// A write to the window reaches a card only while it is shown and WREN is set.
    pub fn window_write(&mut self, off: u32, value: u8) {
        if self.ctrl & CTRL_WREN == 0 {
            return;
        }
        if let Some((slot, at)) = self.card_at(off) {
            if let Some(card) = &mut self.cards[slot] {
                if let Some(b) = card.data.get_mut(at) {
                    *b = value;
                    card.dirty = true;
                }
            }
        }
    }
}

impl Device for CartPort {
    fn read(&mut self, off: u32) -> u8 {
        match off + spec::CART_STAT {
            spec::CART_STAT => {
                let present = |slot: usize, bit: u32| if self.cards[slot].is_some() { bit as u8 } else { 0 };
                u8::from(!self.data.is_empty()) | present(0, spec::CARTS_CARD1) | present(1, spec::CARTS_CARD2) | self.changed
            }
            spec::CART_BANK => self.bank,
            spec::CART_CTRL => self.ctrl,
            spec::CART_SBANK => self.sbank,
            _ => 0,
        }
    }

    fn write(&mut self, off: u32, value: u8) {
        match off + spec::CART_STAT {
            spec::CART_STAT => self.changed &= !value,
            spec::CART_BANK => self.bank = value,
            spec::CART_CTRL => self.ctrl = value & (CTRL_CARD | CTRL_WREN | CTRL_SLOT2),
            spec::CART_SBANK => self.sbank = value,
            _ => {}
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
