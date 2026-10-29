//! The input ports: two gamepads, the keyboard controller (with its pad mirror and the
//! system shortcuts) and the pointer.

use crate::devices::Device;
use p63_spec as spec;
use std::collections::VecDeque;

const FIFO_LEN: usize = 16;
const ST_DR: u8 = 0x01;
const ST_SF: u8 = 0x02;
const ST_OV: u8 = 0x10;
const FS_EMPTY: u8 = 0x40;
const FS_OV: u8 = 0x80;
const KBC_ON: u8 = spec::KBC_ON as u8;
const KBC_IRQ: u8 = spec::KBC_IRQ as u8;
const KBC_MIRROR: u8 = spec::KBC_MIRROR as u8;
const RELEASE: u8 = 0x80;

/// Scan codes the controller treats specially.
pub mod scan {
    pub const ESC: u8 = 0x01;
    pub const BACKSPACE: u8 = 0x0E;
    pub const TAB: u8 = 0x0F;
    pub const ENTER: u8 = 0x1C;
    pub const LCTRL: u8 = 0x1D;
    pub const LSHIFT: u8 = 0x2A;
    pub const Z: u8 = 0x2C;
    pub const X: u8 = 0x2D;
    pub const RSHIFT: u8 = 0x36;
    pub const ALT: u8 = 0x38;
    pub const CAPS: u8 = 0x3A;
    pub const F1: u8 = 0x3B;
    pub const F4: u8 = 0x3E;
    pub const F11: u8 = 0x45;
    pub const F12: u8 = 0x46;
    pub const HOME: u8 = 0x47;
    pub const UP: u8 = 0x48;
    pub const PGUP: u8 = 0x49;
    pub const LEFT: u8 = 0x4B;
    pub const RIGHT: u8 = 0x4D;
    pub const END: u8 = 0x4F;
    pub const DOWN: u8 = 0x50;
    pub const PGDN: u8 = 0x51;
    pub const INSERT: u8 = 0x52;
    pub const DELETE: u8 = 0x53;
    pub const NUMLOCK: u8 = 0x54;
    pub const SCROLL: u8 = 0x55;
    pub const RCTRL: u8 = 0x56;
    pub const ALTGR: u8 = 0x57;
}

/// The keyboard matrix, rows 0-6, by scan code (0: no key), as the handbook lays it out.
const MATRIX: [[u8; 12]; 7] = [
    [0x01, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45],
    [0x46, 0x29, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B],
    [0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17],
    [0x18, 0x19, 0x1A, 0x1B, 0x2B, 0x3A, 0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23],
    [0x24, 0x25, 0x26, 0x27, 0x28, 0x1C, 0x2C, 0x2D, 0x2E, 0x2F, 0x30, 0x31],
    [0x32, 0x33, 0x34, 0x35, 0x2A, 0x36, 0x1D, 0x56, 0x38, 0x57, 0x39, 0x53],
    [0x52, 0x47, 0x4F, 0x49, 0x51, 0x4B, 0x4D, 0x48, 0x50, 0x54, 0x55, 0x00],
];

/// What scan codes $01-$39 type, unshifted and shifted (0: no character).
const TYPED: &[(u8, u8)] = &[
    (0, 0),
    (0x1B, 0x1B), (b'1', b'!'), (b'2', b'@'), (b'3', b'#'), (b'4', b'$'), (b'5', b'%'),
    (b'6', b'^'), (b'7', b'&'), (b'8', b'*'), (b'9', b'('), (b'0', b')'), (b'-', b'_'),
    (b'=', b'+'), (0x08, 0x08), (0x09, 0x09),
    (b'q', b'Q'), (b'w', b'W'), (b'e', b'E'), (b'r', b'R'), (b't', b'T'), (b'y', b'Y'),
    (b'u', b'U'), (b'i', b'I'), (b'o', b'O'), (b'p', b'P'), (b'[', b'{'), (b']', b'}'),
    (0x0D, 0x0D), (0, 0),
    (b'a', b'A'), (b's', b'S'), (b'd', b'D'), (b'f', b'F'), (b'g', b'G'), (b'h', b'H'),
    (b'j', b'J'), (b'k', b'K'), (b'l', b'L'), (b';', b':'), (b'\'', b'"'), (b'`', b'~'),
    (0, 0), (b'\\', b'|'),
    (b'z', b'Z'), (b'x', b'X'), (b'c', b'C'), (b'v', b'V'), (b'b', b'B'), (b'n', b'N'),
    (b'm', b'M'), (b',', b'<'), (b'.', b'>'), (b'/', b'?'), (0, 0), (0, 0), (0, 0), (0, 0),
    (b' ', b' '),
];

/// The pad mirror: keys that also press gamepad 1's buttons.
const MIRROR: [(u8, u8); 8] = [
    (scan::UP, 0x08),
    (scan::DOWN, 0x04),
    (scan::LEFT, 0x02),
    (scan::RIGHT, 0x01),
    (scan::Z, 0x80),
    (scan::X, 0x40),
    (scan::ENTER, 0x20),
    (scan::BACKSPACE, 0x10),
];

/// The keyboard controller and the gamepad ports, `$FFFFC0-$FFFFDF`.
#[derive(Debug, Clone)]
pub struct Keyboard {
    /// The gamepads in ports 1 and 2 (the keyboard's pad mirror adds to port 1).
    pub pad: [u8; 2],
    /// What's plugged in (`PORTS`).
    pub ports: u8,
    /// Keys held now, by scan code. The frontend sets these each frame.
    pub keys: [bool; 128],
    /// System shortcuts made this frame, as the pad button with the same meaning.
    pub chords: Vec<u8>,
    held: [bool; 128],
    ctrl: u8,
    rep: u8,
    delay: u8,
    rate: u8,
    led: u8,
    row: u8,
    caps: bool,
    num: bool,
    codes: VecDeque<u8>,
    scans: VecDeque<u8>,
    overflow: bool,
    repeat: Option<(u8, u8)>,
}

impl Default for Keyboard {
    fn default() -> Self {
        Self {
            pad: [0; 2],
            ports: 0,
            keys: [false; 128],
            chords: Vec::new(),
            held: [false; 128],
            ctrl: KBC_ON | KBC_MIRROR,
            rep: 1,
            delay: 30,
            rate: 3,
            led: 0,
            row: 0,
            caps: false,
            num: false,
            codes: VecDeque::new(),
            scans: VecDeque::new(),
            overflow: false,
            repeat: None,
        }
    }
}

impl Keyboard {
    fn down(&self, s: u8) -> bool {
        self.keys[usize::from(s)]
    }

    fn shift(&self) -> bool {
        self.down(scan::LSHIFT) || self.down(scan::RSHIFT)
    }

    fn ctrl_key(&self) -> bool {
        self.down(scan::LCTRL) || self.down(scan::RCTRL)
    }

    fn alt(&self) -> bool {
        self.down(scan::ALT) || self.down(scan::ALTGR)
    }

    /// `KB_MOD`.
    #[must_use]
    pub fn modifiers(&self) -> u8 {
        u8::from(self.down(scan::LSHIFT))
            | u8::from(self.down(scan::RSHIFT)) << 1
            | u8::from(self.ctrl_key()) << 2
            | u8::from(self.alt()) << 3
            | u8::from(self.caps) << 5
            | u8::from(self.num) << 6
    }

    /// The key code a press of scan code `s` puts in the FIFO, if it has one.
    fn code(&self, s: u8) -> Option<u8> {
        if matches!(s, scan::LSHIFT | scan::RSHIFT | scan::LCTRL | scan::RCTRL | scan::ALT | scan::ALTGR | scan::CAPS | scan::NUMLOCK | scan::SCROLL) {
            return None;
        }
        if s == scan::DELETE {
            return Some(0x7F);
        }
        let Some(&(plain, shifted)) = TYPED.get(usize::from(s)) else { return Some(0x80 | s) };
        if plain == 0 {
            return Some(0x80 | s);
        }
        if self.ctrl_key() && plain.is_ascii_lowercase() {
            return Some(plain - b'a' + 1);
        }
        let upper = self.shift() ^ (self.caps && plain.is_ascii_lowercase());
        Some(if upper { shifted } else { plain })
    }

    /// The pad button a system shortcut on scan code `s` means, if it is one.
    fn chord(&self, s: u8) -> Option<u8> {
        let (ctrl, alt) = (self.ctrl_key(), self.alt());
        match s {
            scan::DELETE if ctrl && alt => Some(spec::BTN_DOWN as u8),
            scan::TAB if alt => Some(spec::BTN_LEFT as u8),
            scan::F4 if alt => Some(spec::BTN_A as u8),
            scan::F11 => Some(spec::BTN_B as u8),
            scan::ESC if ctrl => Some(spec::BTN_START as u8),
            _ => None,
        }
    }

    fn push(queue: &mut VecDeque<u8>, value: u8, overflow: &mut bool) {
        if queue.len() < FIFO_LEN {
            queue.push_back(value);
        } else {
            *overflow = true;
        }
    }

    fn press(&mut self, s: u8) {
        if let Some(button) = self.chord(s) {
            self.chords.push(button);
            return;
        }
        match s {
            scan::CAPS => self.caps = !self.caps,
            scan::NUMLOCK => self.num = !self.num,
            _ => {}
        }
        Self::push(&mut self.scans, s, &mut self.overflow);
        if let Some(code) = self.code(s) {
            Self::push(&mut self.codes, code, &mut self.overflow);
            // the repeat check later this frame counts the first frame
            self.repeat = Some((s, self.delay.max(1).saturating_add(1)));
        }
    }

    /// Once a frame: the presses and releases since the last frame, and key repeat.
    pub fn frame(&mut self) {
        if self.ctrl & KBC_ON == 0 {
            self.held = self.keys;
            return;
        }
        for s in 1..128u8 {
            let (now, was) = (self.keys[usize::from(s)], self.held[usize::from(s)]);
            if now && !was {
                self.press(s);
            } else if was && !now {
                Self::push(&mut self.scans, s | RELEASE, &mut self.overflow);
                if self.repeat.is_some_and(|(r, _)| r == s) {
                    self.repeat = None;
                }
            }
        }
        if let Some((s, timer)) = self.repeat {
            if self.rep & 1 == 0 || !self.down(s) {
                self.repeat = None;
            } else if timer <= 1 {
                if let Some(code) = self.code(s) {
                    Self::push(&mut self.codes, code, &mut self.overflow);
                }
                self.repeat = Some((s, self.rate.max(1)));
            } else {
                self.repeat = Some((s, timer - 1));
            }
        }
        self.held = self.keys;
    }

    /// Gamepad 1 as programs see it: the pad in port 1, plus the keyboard's pad mirror.
    #[must_use]
    pub fn pad1(&self) -> u8 {
        let mirror = if self.ctrl & KBC_MIRROR == 0 { 0 } else { MIRROR.iter().filter(|(s, _)| self.down(*s)).fold(0, |a, (_, b)| a | b) };
        self.pad[0] | mirror
    }

    /// Row `r` of the matrix as 12 column bits; row 7 is gamepad 1.
    fn matrix_row(&self, r: usize) -> u16 {
        match MATRIX.get(r) {
            Some(cols) => cols.iter().enumerate().filter(|(_, &s)| s != 0 && self.down(s)).fold(0, |a, (c, _)| a | 1 << c),
            None => u16::from(self.pad1()),
        }
    }

    /// True while `/IRQ` is held: interrupt mode with a key code waiting.
    #[must_use]
    pub fn irq(&self) -> bool {
        self.irq_with(self.ctrl)
    }

    /// The same for a program with its own `KB_CTRL`.
    #[must_use]
    pub fn irq_with(&self, ctrl: u8) -> bool {
        ctrl & KBC_IRQ != 0 && !self.codes.is_empty()
    }

    /// Puts in another program's `KB_CTRL` (each has its own pad mirror and interrupt
    /// setting) and gives back the one that was there.
    pub fn swap_ctrl(&mut self, ctrl: u8) -> u8 {
        std::mem::replace(&mut self.ctrl, ctrl)
    }
}

impl Device for Keyboard {
    fn read(&mut self, off: u32) -> u8 {
        let matrix = spec::KB_MATRIX - spec::KB_IO_BASE;
        match off + spec::KB_IO_BASE {
            spec::KB_CTRL => self.ctrl,
            spec::KB_STAT => {
                let stat = (u8::from(!self.codes.is_empty()) * ST_DR) | (u8::from(!self.scans.is_empty()) * ST_SF) | (u8::from(self.overflow) * ST_OV);
                self.overflow = false;
                stat
            }
            spec::KB_SCAN => self.scans.pop_front().unwrap_or(0),
            spec::KB_DATA | spec::KB_FIFO => self.codes.pop_front().unwrap_or(0),
            spec::KB_MOD => self.modifiers(),
            spec::KB_ROW => self.row,
            spec::KB_COL => self.matrix_row(usize::from(self.row & 7)) as u8,
            spec::KB_FIFOSTAT => self.codes.len() as u8 | if self.codes.is_empty() { FS_EMPTY } else { 0 } | if self.overflow { FS_OV } else { 0 },
            spec::KB_REP => self.rep,
            spec::KB_DELAY => self.delay,
            spec::KB_RATE => self.rate,
            spec::KB_LED => self.led,
            spec::PAD2 => self.pad[1],
            spec::PORTS => self.ports,
            a if (spec::KB_MATRIX..spec::KB_MATRIX + 16).contains(&a) => {
                let i = off - matrix;
                (self.matrix_row((i / 2) as usize) >> (8 * (i % 2))) as u8
            }
            _ => 0,
        }
    }

    fn write(&mut self, off: u32, value: u8) {
        match off + spec::KB_IO_BASE {
            spec::KB_CTRL => self.ctrl = value & (KBC_ON | KBC_IRQ | KBC_MIRROR),
            spec::KB_ROW => self.row = value & 7,
            spec::KB_REP => self.rep = value & 1,
            spec::KB_DELAY => self.delay = value,
            spec::KB_RATE => self.rate = value,
            spec::KB_LED => self.led = value & 7,
            spec::KB_CMD if value == 0xFF => {
                let keep = (self.pad, self.ports, self.keys, self.held, self.caps, self.num, self.led);
                *self = Self::default();
                (self.pad, self.ports, self.keys, self.held, self.caps, self.num, self.led) = keep;
            }
            _ => {}
        }
    }
}

/// The pointer port, `MS_X` to `MS_WHEEL`: free-running counters and the buttons.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pointer {
    pub x: u16,
    pub y: u16,
    pub buttons: u8,
    pub wheel: u8,
}

impl Pointer {
    /// The pointer moved by `dx`, `dy` pixels and the wheel by `wheel` clicks.
    pub fn moved(&mut self, dx: i32, dy: i32, wheel: i32) {
        self.x = self.x.wrapping_add(dx as u16);
        self.y = self.y.wrapping_add(dy as u16);
        self.wheel = self.wheel.wrapping_add(wheel as u8);
    }

    #[must_use]
    pub fn read(&self, a: u32) -> u8 {
        match a {
            a if a == spec::MS_X => self.x as u8,
            a if a == spec::MS_X + 1 => (self.x >> 8) as u8,
            a if a == spec::MS_Y => self.y as u8,
            a if a == spec::MS_Y + 1 => (self.y >> 8) as u8,
            a if a == spec::MS_BTN => self.buttons,
            a if a == spec::MS_WHEEL => self.wheel,
            _ => 0xFF,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tap(kb: &mut Keyboard, keys: &[u8]) {
        for &k in keys {
            kb.keys[usize::from(k)] = true;
        }
        kb.frame();
        for &k in keys {
            kb.keys[usize::from(k)] = false;
        }
        kb.frame();
    }

    fn read(kb: &mut Keyboard, a: u32) -> u8 {
        kb.read(a - spec::KB_IO_BASE)
    }

    #[test]
    fn typed_keys_come_out_as_characters_and_scan_codes() {
        let mut kb = Keyboard::default();
        tap(&mut kb, &[0x23]); // H
        kb.keys[usize::from(scan::LSHIFT)] = true;
        kb.frame();
        tap(&mut kb, &[0x02]); // shift + 1
        kb.keys[usize::from(scan::LSHIFT)] = false;
        kb.frame();
        tap(&mut kb, &[scan::UP]);
        assert_eq!(read(&mut kb, spec::KB_STAT) & ST_DR, ST_DR);
        assert_eq!(read(&mut kb, spec::KB_FIFOSTAT), 3);
        assert_eq!(read(&mut kb, spec::KB_DATA), b'h');
        assert_eq!(read(&mut kb, spec::KB_FIFO), b'!');
        assert_eq!(read(&mut kb, spec::KB_DATA), spec::KEY_UP as u8);
        assert_eq!(read(&mut kb, spec::KB_FIFOSTAT), FS_EMPTY);
        assert_eq!(read(&mut kb, spec::KB_SCAN), 0x23, "H down");
        assert_eq!(read(&mut kb, spec::KB_SCAN), 0x23 | RELEASE, "and up");
        assert_eq!(read(&mut kb, spec::KB_SCAN), scan::LSHIFT);
    }

    #[test]
    fn caps_ctrl_and_repeat() {
        let mut kb = Keyboard::default();
        tap(&mut kb, &[scan::CAPS]);
        tap(&mut kb, &[0x1E]); // A
        assert_eq!(read(&mut kb, spec::KB_DATA), b'A', "caps lock");
        assert_eq!(read(&mut kb, spec::KB_MOD) & 0x20, 0x20);
        kb.keys[usize::from(scan::LCTRL)] = true;
        kb.frame();
        tap(&mut kb, &[0x2E]); // C
        assert_eq!(read(&mut kb, spec::KB_DATA), 3, "Ctrl+C");
        kb.keys[usize::from(scan::LCTRL)] = false;
        kb.keys[0x1E] = true;
        for _ in 0..37 {
            kb.frame();
        }
        assert_eq!(read(&mut kb, spec::KB_FIFOSTAT), 1 + 3, "the press, then repeats 30, 33 and 36 frames later");
    }

    #[test]
    fn the_pad_mirror_and_shortcuts() {
        let mut kb = Keyboard::default();
        kb.keys[usize::from(scan::Z)] = true;
        kb.keys[usize::from(scan::LEFT)] = true;
        kb.pad[0] = 0x20;
        assert_eq!(read(&mut kb, spec::KB_PAD), 0x80 | 0x02 | 0x20, "Z = A, Left, and the real pad's Start");
        kb.write(spec::KB_CTRL - spec::KB_IO_BASE, KBC_ON);
        assert_eq!(read(&mut kb, spec::KB_PAD), 0x20, "mirror off");
        let mut kb = Keyboard::default();
        kb.keys[usize::from(scan::LCTRL)] = true;
        kb.keys[usize::from(scan::ALT)] = true;
        kb.frame();
        kb.keys[usize::from(scan::DELETE)] = true;
        kb.frame();
        assert_eq!(kb.chords, vec![spec::BTN_DOWN as u8], "Ctrl+Alt+Del");
        assert_eq!(read(&mut kb, spec::KB_FIFOSTAT), FS_EMPTY, "never typed");
    }

    #[test]
    fn the_matrix_shows_held_keys() {
        let mut kb = Keyboard::default();
        kb.keys[0x1E] = true; // A: row 3, column 6
        assert_eq!(read(&mut kb, spec::KB_MATRIX + 6), 1 << 6);
        kb.write(spec::KB_ROW - spec::KB_IO_BASE, 3);
        assert_eq!(read(&mut kb, spec::KB_COL), 1 << 6);
    }
}
