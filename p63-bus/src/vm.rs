//! Virtual mode: the host (P63 OS) runs a guest program boxed in. The guest's RAM is on
//! 256 KB pages the host hands out, its vectors and saved registers live in a table in
//! host RAM, and Select+Start chords give control back to the host.

use crate::devices::{CartView, Video};
use p63_core::{CpuState, Flags, Registers};
use p63_spec as spec;

pub const SLOTS: usize = 4;
/// The guest's interrupt vectors, $FFFFE0-$FFFFFF as it sees them.
pub const VECTORS: u32 = 0xFF_FFE0;
/// Saved registers in the table: CC, Q, GP, DP, X, Y, SP, PC.
pub const STATE_LEN: usize = 19;
/// The end of the VM register block, which guests can't see or write.
pub const GUARDED_END: u32 = spec::VM_VKB + 1;
/// RAM in use is counted in blocks of this many bytes (1 KB).
const BLOCK_SHIFT: u32 = 10;
const BLOCKS: usize = (spec::MAIN_TOP as usize >> BLOCK_SHIFT) + 1;
pub const SHOW_FULL: u8 = spec::SHOW_FULL as u8;
pub const VMF_WAITV: u8 = spec::VMF_WAITV as u8;
pub const VMF_NOPAD: u8 = spec::VMF_NOPAD as u8;
pub const VMF_WINDOW: u8 = spec::VMF_WINDOW as u8;
pub const VMF_SLEEP: u8 = spec::VMF_SLEEP as u8;
pub const VMF_HOLD: u8 = spec::VMF_HOLD as u8;
pub const VMF_LIVE: u8 = spec::VMF_LIVE as u8;
/// A live guest's interrupt handler that runs longer than this many lines goes back to
/// its own turn to finish.
pub const LIVE_LINES: u32 = 40;
const ARAM_LEN: u32 = spec::ARAM_END - spec::ARAM_BASE + 1;
const PAGE_MASK: u32 = (1 << spec::VM_PAGE_SHIFT) - 1;
const PAGE_THERE: u8 = 0x80;
/// Real pages 62 and 63 also hold the boot ROM, cart window and devices, so a page
/// entry can't point at them.
const RAM_PAGES: u8 = 62;
const SELECT_START: u8 = 0x30;

pub const WHY_CHORD: u8 = spec::WHY_CHORD as u8;
pub const WHY_SLICE: u8 = spec::WHY_SLICE as u8;
pub const WHY_WAITV: u8 = spec::WHY_WAITV as u8;
pub const WHY_SYSTEM: u8 = spec::WHY_SYSTEM as u8;
pub const WHY_PAGE: u8 = spec::WHY_PAGE as u8;
pub const WHY_ILLEGAL: u8 = spec::WHY_ILLEGAL as u8;
pub const WHY_RESET: u8 = spec::WHY_RESET as u8;

/// One guest: where its table is, its cart offset, and how it last stopped.
#[derive(Debug, Clone, Default)]
pub struct Slot {
    pub table: u32,
    pub cartbase: u8,
    pub why: u8,
    pub arg: u32,
    /// The guest's CPU while it's stopped; `None` until it has run.
    pub cpu: Option<CpuState>,
    pub cart: CartView,
    /// Its video context: OAM and VRAM in host RAM (0: the real ones), and its registers.
    pub vram: u32,
    pub video: Video,
    pub flags: u8,
    pub slice: u16,
    /// Lines run since its `VM_GO`.
    pub lines: u32,
    /// Its window: where, and how deep among the others.
    pub wx: u16,
    pub wy: u8,
    pub depth: u8,
    /// Its sound: Audio RAM in host RAM (0: the real one), its voices, its volume.
    pub aram: u32,
    pub apu: p63_apu::Apu,
    pub vol: u8,
    /// Its own CPU clock, used while it runs, and a pick waiting for the next vblank.
    pub speed: u8,
    pub speed_requested: Option<u8>,
    /// Where each of its pages starts in RAM (`NO_PAGE`: not mapped), read from its
    /// table when its turn starts; the host only changes the table while it's stopped.
    pub pages: Vec<u32>,
    /// Lines it has spent running.
    pub used: u32,
    /// Its RAM in use: which 1 KB blocks, and how many.
    pub blocks: Vec<u64>,
    pub kb: u16,
    /// Its OAM and VRAM in use, the same way.
    pub vblocks: [u64; 3],
    pub vkb: u16,
    /// The low byte of a `VM_MARK` write, until the high byte comes.
    pub mark_lo: u8,
    /// The line its interrupt was last taken on while it was stopped, so it's taken once.
    pub live_line: Option<u16>,
    /// Its own `KB_CTRL`: pad mirror and keyboard interrupt, its business only.
    pub kb_ctrl: u8,
    /// Frames it has finished (stops for WAITV that weren't already waiting), for tools.
    pub frames: u32,
}

pub const NO_PAGE: u32 = u32::MAX;

impl Slot {
    /// Where its OAM or VRAM byte `a` is in host RAM, if it has a video context.
    #[must_use]
    pub fn video_at(&self, a: u32) -> Option<usize> {
        (self.vram != 0 && (spec::OAM_BASE..=spec::VRAM_END).contains(&a)).then(|| ((self.vram + a - spec::OAM_BASE) & 0xFF_FFFF) as usize)
    }

    /// Where its Audio RAM byte `a` is in host RAM, if it has its own sound.
    #[must_use]
    pub fn aram_at(&self, a: u32) -> Option<usize> {
        (self.aram != 0 && (spec::ARAM_BASE..=spec::ARAM_END).contains(&a)).then(|| ((self.aram + a - spec::ARAM_BASE) & 0xFF_FFFF) as usize)
    }

    /// Its Audio RAM, if it has its own sound and it fits in RAM.
    #[must_use]
    pub fn aram_range(&self) -> Option<std::ops::Range<usize>> {
        let start = self.aram as usize;
        (self.aram != 0 && self.aram + ARAM_LEN <= spec::MAIN_TOP + 1).then(|| start..start + ARAM_LEN as usize)
    }

    /// Reads its page table into `pages`.
    pub fn load_pages(&mut self, ram: &[u8]) {
        self.pages = (0..64)
            .map(|p| page(ram, self.table, p << spec::VM_PAGE_SHIFT).map_or(NO_PAGE, |at| at as u32))
            .collect();
    }

    /// Where its RAM address `a` is, if its page is mapped.
    #[must_use]
    pub fn ram_at(&self, a: u32) -> Option<usize> {
        let base = *self.pages.get((a >> spec::VM_PAGE_SHIFT) as usize)?;
        (base != NO_PAGE).then_some((base | (a & PAGE_MASK)) as usize)
    }

    /// Counts the 1 KB block guest address `a` is in as used.
    pub fn touch(&mut self, a: u32) {
        let block = (a >> BLOCK_SHIFT) as usize;
        if self.blocks.len() != BLOCKS.div_ceil(64) {
            self.blocks = vec![0; BLOCKS.div_ceil(64)];
        }
        let (word, bit) = (block / 64, 1u64 << (block % 64));
        if self.blocks[word] & bit == 0 {
            self.blocks[word] |= bit;
            self.kb = self.kb.saturating_add(1);
        }
    }

    /// Counts the 1 KB block of its OAM or VRAM that address `a` is in as used.
    pub fn touch_video(&mut self, a: u32) {
        let block = ((a - spec::OAM_BASE) >> BLOCK_SHIFT) as usize;
        let (word, bit) = (block / 64, 1u64 << (block % 64));
        if self.vblocks[word] & bit == 0 {
            self.vblocks[word] |= bit;
            self.vkb += 1;
        }
    }

    /// `VM_MARK`: the tally starts over with blocks 0 to n - 1 in use, and none of its
    /// video memory.
    pub fn mark(&mut self, n: u16) {
        self.blocks = vec![0; BLOCKS.div_ceil(64)];
        self.kb = 0;
        self.vblocks = [0; 3];
        self.vkb = 0;
        for block in 0..u32::from(n).min(BLOCKS as u32) {
            self.touch(block << BLOCK_SHIFT);
        }
    }

    /// Shown in its window on the host's screen.
    #[must_use]
    pub fn windowed(&self) -> bool {
        self.flags & VMF_WINDOW != 0 && self.vram != 0
    }
}

#[derive(Debug, Default)]
pub struct Vm {
    pub slots: [Slot; SLOTS],
    /// The slot the `VM_*` registers show.
    pub sel: u8,
    /// The slot running now, if a guest is running.
    pub guest: Option<usize>,
    /// The slot that stopped last.
    pub last: u8,
    /// A chord made while the host was running.
    pub chord: u8,
    /// Buttons hidden from guests until they're all let go.
    pub mask: u8,
    last_pad: u8,
    /// A `VM_GO` write, acted on once its instruction is done.
    pub go: Option<u8>,
    /// Why the guest has to stop, acted on once its instruction is done (or undone).
    pub stop: Option<(u8, u32)>,
    /// RAM the guest's current instruction wrote, oldest first, for a page stop to undo.
    pub undo: Vec<(usize, u8)>,
    /// The host's CPU and cart registers while a guest runs.
    pub host: Option<(CpuState, CartView)>,
    /// What's on screen (`VM_SHOW`).
    pub show: u8,
    /// Lines the CPU spent waiting, and lines since power-on.
    pub idle: u32,
    pub ticks: u32,
    /// A live guest running its interrupt handler in someone else's time: the stack
    /// pointer it had, and whether the interrupt has been taken yet.
    pub borrowed: Option<(u32, bool)>,
    /// The guest it cut into, if it wasn't the host.
    pub from: Option<usize>,
    /// Set at each line: a live guest's interrupt may have come due.
    pub live_check: bool,
}

impl Vm {
    /// `live_due`, looked at only once a line or after a handler, when it can change.
    pub fn take_live_due(&mut self) -> Option<usize> {
        if !self.live_check {
            return None;
        }
        let due = self.live_due();
        if due.is_none() {
            self.live_check = false;
        }
        due
    }

    /// A stopped live guest whose video interrupt is due and not masked, if there is one.
    #[must_use]
    pub fn live_due(&self) -> Option<usize> {
        if self.borrowed.is_some() {
            return None;
        }
        self.slots.iter().position(|s| {
            s.flags & VMF_LIVE != 0
                && s.vram != 0
                && s.video.irq()
                && s.live_line != Some(s.video.line)
                && s.cpu.as_ref().is_some_and(|c| !c.registers.cc().contains(Flags::I))
        })
    }

    /// Counts one line for whoever has the CPU: a running guest, or waiting.
    pub fn count_line(&mut self, waiting: bool) {
        self.ticks = self.ticks.wrapping_add(1);
        if waiting {
            self.idle = self.idle.wrapping_add(1);
        } else if let Some(g) = self.guest {
            self.slots[g].used = self.slots[g].used.wrapping_add(1);
        }
    }
}

impl Vm {
    /// Watches the pad for Select+Start plus a newly pressed button.
    pub fn pad_changed(&mut self, pad: u8) {
        let pressed = pad & !self.last_pad;
        self.last_pad = pad;
        if pad & self.mask == 0 {
            self.mask = 0;
        }
        let other = pressed & !SELECT_START;
        if pad & SELECT_START != SELECT_START || other == 0 {
            return;
        }
        let button = other & other.wrapping_neg();
        self.mask = SELECT_START | button;
        if self.guest.is_some() {
            self.stop.get_or_insert((WHY_CHORD, u32::from(button)));
        } else {
            self.chord = button;
        }
    }

    /// A system shortcut typed on the keyboard, as the pad chord that means the same.
    pub fn key_chord(&mut self, button: u8) {
        if self.guest.is_some() {
            self.stop.get_or_insert((WHY_CHORD, u32::from(button)));
        } else {
            self.chord = button;
        }
    }

    /// Asks the running guest to stop, keeping the first reason given.
    pub fn stop_with(&mut self, why: u8, arg: u32) {
        self.stop.get_or_insert((why, arg));
    }
}

/// Where guest address `a` (a RAM address) really is, if its page is mapped.
#[must_use]
pub fn page(ram: &[u8], table: u32, a: u32) -> Option<usize> {
    let entry = ram[((table + (a >> spec::VM_PAGE_SHIFT)) & 0xFF_FFFF) as usize];
    let real = entry & 0x3F;
    (entry & PAGE_THERE != 0 && real < RAM_PAGES).then(|| ((u32::from(real) << spec::VM_PAGE_SHIFT) | (a & PAGE_MASK)) as usize)
}

/// Where the guest's vector byte `a` is in host RAM.
#[must_use]
pub fn vector(table: u32, a: u32) -> usize {
    ((table + spec::VMT_VECS + (a - VECTORS)) & 0xFF_FFFF) as usize
}

/// Registers as the table keeps them.
#[must_use]
pub fn state_bytes(r: &Registers) -> [u8; STATE_LEN] {
    let mut out = [0; STATE_LEN];
    out[0] = r.cc().bits();
    out[1..5].copy_from_slice(&r.q().to_le_bytes());
    out[5] = r.gp();
    out[6] = r.dp();
    for (at, v) in [(7, r.x()), (10, r.y()), (13, r.sp()), (16, r.pc())] {
        out[at..at + 3].copy_from_slice(&v.to_le_bytes()[..3]);
    }
    out
}

/// The table's register bytes back into registers.
#[must_use]
pub fn registers_from(b: &[u8]) -> Registers {
    let u24 = |at: usize| u32::from_le_bytes([b[at], b[at + 1], b[at + 2], 0]);
    let mut r = Registers::new();
    r.set_cc(Flags::from_bits_truncate(b[0]));
    r.set_q(u32::from_le_bytes([b[1], b[2], b[3], b[4]]));
    r.set_gp(b[5]);
    r.set_dp(b[6]);
    r.set_x(u24(7));
    r.set_y(u24(10));
    r.set_sp(u24(13));
    r.set_pc(u24(16));
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chord_needs_select_and_start_first() {
        let mut vm = Vm::default();
        vm.pad_changed(0x80 | 0x30);
        assert_eq!(vm.chord, 0x80, "all three in the same frame counts");
        let mut vm = Vm::default();
        vm.pad_changed(0x10);
        vm.pad_changed(0x10 | 0x80);
        assert_eq!(vm.chord, 0, "Select alone is no chord");
        vm.pad_changed(0x30 | 0x80);
        assert_eq!(vm.chord, 0, "A was already down");
        vm.pad_changed(0x30 | 0x80 | 0x08);
        assert_eq!(vm.chord, 0x08);
        assert_eq!(vm.mask, 0x38);
        vm.pad_changed(0x80 | 0x08);
        assert_eq!(vm.mask, 0x38, "still hidden while Up is held");
        vm.pad_changed(0x80);
        assert_eq!(vm.mask, 0, "free once all three are let go");
    }

    #[test]
    fn registers_round_trip() {
        let mut r = Registers::new();
        r.set_q(0x1234_5678);
        r.set_x(0x00AB_CDEF);
        r.set_pc(0x0000_0100);
        r.set_sp(0x00FB_B5FF);
        r.set_cc(Flags::from_bits_truncate(0x50));
        assert_eq!(registers_from(&state_bytes(&r)), r);
    }
}
