//! p63-bus: the Paradox63 memory map, its devices, and the frame loop.

pub mod bios;
pub mod devices;
pub mod input;
pub mod vm;

use devices::{CartPort, CartView, Device, Video};
use input::{Keyboard, Pointer};
use p63_apu::Apu;
use p63_cart::{crypt, BootError, Cart};
use p63_core::{Addr, Bus, CpuError, Cycles, Flags, Halt, ReferenceInterpreter};
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
    pub pointer: Pointer,
    pub cart: CartPort,
    pub audio: Apu,
    pub vm: vm::Vm,
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
            pointer: Pointer::default(),
            cart: CartPort::default(),
            audio: Apu::default(),
            vm: vm::Vm::default(),
        }
    }
}

impl SystemBus {
    /// The whole address space as the video chip sees it (VRAM, OAM, palette).
    #[must_use]
    pub fn memory(&self) -> &[u8] {
        &self.ram
    }

    /// A read for tools (peeks, debuggers): as the host CPU would see it, except that the
    /// boot ROM always reads $FF, locked or not, and a peek doesn't clear `VM_CHORD`.
    pub fn peek(&mut self, addr: Addr) -> u8 {
        match addr.0 {
            a if within(a, spec::BOOT_ROM, spec::CART_WIN - 1) => 0xFF,
            spec::VM_CHORD => self.vm.chord,
            // reading these pops the key FIFOs or clears the overflow
            spec::KB_STAT | spec::KB_SCAN | spec::KB_DATA | spec::KB_FIFO => 0,
            a => self.host_read(a),
        }
    }

    /// The keyboard's `/IRQ`, for whoever has the input (a guest without it never sees it).
    fn keyboard_irq(&self) -> bool {
        match self.vm.guest {
            Some(g) if self.vm.slots[g].flags & vm::VMF_NOPAD != 0 => false,
            Some(g) => self.keyboard.irq_with(self.vm.slots[g].kb_ctrl),
            None => self.keyboard.irq(),
        }
    }

    /// Undoes the RAM writes of a guest instruction that stopped on a missing page.
    fn undo_guest_writes(&mut self) {
        for (at, old) in self.vm.undo.drain(..).rev() {
            self.ram[at] = old;
        }
    }

    fn guest_read(&mut self, a: u32, slot: usize) -> u8 {
        let s = &self.vm.slots[slot];
        if (4..=spec::MAIN_TOP).contains(&a) {
            // the common case first: its RAM, through its page cache
            if let Some(at) = s.ram_at(a) {
                return self.ram[at];
            }
            self.vm.stop_with(vm::WHY_PAGE, a);
            return 0;
        }
        let (table, cartbase, flags, context) = (s.table, s.cartbase, s.flags, s.vram != 0);
        if let Some(at) = s.video_at(a).or_else(|| s.aram_at(a)) {
            return self.ram[at];
        }
        if s.aram != 0 && within(a, spec::AUDIO_IO_BASE, spec::AUDIO_IO_END) {
            return self.vm.slots[slot].apu.read(a - spec::AUDIO_IO_BASE);
        }
        if context && within(a, spec::VIDEO_IO_BASE, spec::VIDEO_IO_END) {
            return self.vm.slots[slot].video.read(a - spec::VIDEO_IO_BASE);
        }
        // a program without the input sees nothing plugged in
        if flags & vm::VMF_NOPAD != 0 && (within(a, spec::KB_IO_BASE, spec::KB_IO_END) || within(a, spec::MS_X, spec::MS_WHEEL)) {
            return 0;
        }
        if within(a, spec::KB_IO_BASE, spec::KB_IO_END) {
            // the keyboard with its own KB_CTRL; chord buttons stay hidden
            let host = self.keyboard.swap_ctrl(self.vm.slots[slot].kb_ctrl);
            let mut value = self.keyboard.read(a - spec::KB_IO_BASE);
            if a == spec::KB_PAD {
                value &= !self.vm.mask;
            }
            self.vm.slots[slot].kb_ctrl = self.keyboard.swap_ctrl(host);
            return value;
        }
        if a < 4 {
            spec::LOW_VECTOR_ROM[a as usize]
        } else if a >= vm::VECTORS {
            self.ram[vm::vector(table, a)]

        } else if a == spec::CART_BANK {
            self.cart.read(a - spec::CART_STAT).wrapping_sub(cartbase)
        } else if a == SYS_SPEED {
            self.speed_forced.unwrap_or(self.vm.slots[slot].speed)
        } else if within(a, spec::VM_SLOT, vm::GUARDED_END) {
            0xFF
        } else {
            self.host_read(a)
        }
    }

    fn guest_write(&mut self, a: u32, value: u8, slot: usize) {
        let s = &self.vm.slots[slot];
        if (4..=spec::MAIN_TOP).contains(&a) {
            if let Some(at) = s.ram_at(a) {
                self.vm.undo.push((at, self.ram[at]));
                self.ram[at] = value;
                self.vm.slots[slot].touch(a);
            } else {
                self.vm.stop_with(vm::WHY_PAGE, a);
            }
            return;
        }
        let (table, cartbase, context) = (s.table, s.cartbase, s.vram != 0);
        if let Some(at) = s.video_at(a) {
            self.vm.undo.push((at, self.ram[at]));
            self.ram[at] = value;
            self.vm.slots[slot].touch_video(a);
            return;
        }
        if let Some(at) = s.aram_at(a) {
            self.vm.undo.push((at, self.ram[at]));
            self.ram[at] = value;
            return;
        }
        if within(a, spec::KB_IO_BASE, spec::KB_IO_END) {
            if s.flags & vm::VMF_NOPAD == 0 {
                let host = self.keyboard.swap_ctrl(s.kb_ctrl);
                self.keyboard.write(a - spec::KB_IO_BASE, value);
                self.vm.slots[slot].kb_ctrl = self.keyboard.swap_ctrl(host);
            }
            return;
        }
        if s.aram != 0 && within(a, spec::AUDIO_IO_BASE, spec::AUDIO_IO_END) {
            self.vm.slots[slot].apu.write(a - spec::AUDIO_IO_BASE, value);
            return;
        }
        if context && within(a, spec::VIDEO_IO_BASE, spec::VIDEO_IO_END) {
            self.vm.slots[slot].video.write(a - spec::VIDEO_IO_BASE, value);
            return;
        }
        if a < 4 {
        } else if a >= vm::VECTORS {
            let at = vm::vector(table, a);
            self.vm.undo.push((at, self.ram[at]));
            self.ram[at] = value;
        } else if a == SYS_RESET {
            self.vm.stop_with(vm::WHY_RESET, u32::from(value));
        } else if a == SYS_SPEED {
            // its own clock, from the next vblank
            if matches!(value, SPEED_NORMAL | SPEED_TURBO | SPEED_LOW) {
                self.vm.slots[slot].speed_requested = Some(value);
            }
        } else if within(a, spec::VM_SLOT, vm::GUARDED_END) || within(a, NVRAM_BASE, NVRAM_END) {
            // the engine and SYS_LOCK are already locked by the time a host runs guests,
            // so a guest finds them as any cart does
            self.vm.stop_with(vm::WHY_SYSTEM, a | u32::from(value) << 24);
        } else if a == spec::CART_BANK {
            self.cart.write(a - spec::CART_STAT, value.wrapping_add(cartbase));
        } else {
            self.host_write(a, value);
        }
    }

    /// The `VM_*` registers as the host sees them.
    fn vm_read(&mut self, a: u32) -> u8 {
        let vm = &mut self.vm;
        let s = &vm.slots[usize::from(vm.sel)];
        match a {
            spec::VM_SLOT => vm.sel,
            spec::VM_GO => vm.last,
            spec::VM_WHY => s.why,
            spec::VM_CHORD => std::mem::take(&mut vm.chord),
            a if within(a, spec::VM_ARG, spec::VM_ARG + 3) => (s.arg >> (8 * (a - spec::VM_ARG))) as u8,
            a if within(a, spec::VM_TABLE, spec::VM_TABLE + 2) => (s.table >> (8 * (a - spec::VM_TABLE))) as u8,
            spec::VM_CARTBASE => s.cartbase,
            spec::VM_SHOW => vm.show,
            a if within(a, spec::VM_WX, spec::VM_WX + 1) => (s.wx >> (8 * (a - spec::VM_WX))) as u8,
            spec::VM_WY => s.wy,
            spec::VM_DEPTH => s.depth,
            a if within(a, spec::VM_ARAM, spec::VM_ARAM + 2) => (s.aram >> (8 * (a - spec::VM_ARAM))) as u8,
            spec::VM_VOL => s.vol,
            a if within(a, spec::VM_USED, spec::VM_USED + 3) => (s.used >> (8 * (a - spec::VM_USED))) as u8,
            a if within(a, spec::VM_IDLE, spec::VM_IDLE + 3) => (vm.idle >> (8 * (a - spec::VM_IDLE))) as u8,
            a if within(a, spec::VM_TICKS, spec::VM_TICKS + 3) => (vm.ticks >> (8 * (a - spec::VM_TICKS))) as u8,
            spec::VM_SPEED => s.speed,
            a if within(a, spec::VM_KB, spec::VM_KB + 1) => (s.kb >> (8 * (a - spec::VM_KB))) as u8,
            a if within(a, spec::VM_VKB, spec::VM_VKB + 1) => (s.vkb >> (8 * (a - spec::VM_VKB))) as u8,
            a if within(a, spec::VM_VRAM, spec::VM_VRAM + 2) => (s.vram >> (8 * (a - spec::VM_VRAM))) as u8,
            spec::VM_FLAGS => s.flags,
            a if within(a, spec::VM_SLICE, spec::VM_SLICE + 1) => (s.slice >> (8 * (a - spec::VM_SLICE))) as u8,
            _ => 0xFF,
        }
    }

    fn vm_write(&mut self, a: u32, value: u8) {
        let vm = &mut self.vm;
        let s = &mut vm.slots[usize::from(vm.sel)];
        match a {
            spec::VM_SLOT => vm.sel = value % vm::SLOTS as u8,
            spec::VM_GO => vm.go = Some(value),
            a if within(a, spec::VM_TABLE, spec::VM_TABLE + 2) => {
                let shift = 8 * (a - spec::VM_TABLE);
                s.table = (s.table & !(0xFF << shift)) | u32::from(value) << shift;
            }
            spec::VM_CARTBASE => s.cartbase = value,
            spec::VM_SHOW => vm.show = value,
            a if within(a, spec::VM_WX, spec::VM_WX + 1) => {
                let shift = 8 * (a - spec::VM_WX);
                s.wx = (s.wx & !(0xFF << shift)) | u16::from(value) << shift;
            }
            spec::VM_WY => s.wy = value,
            spec::VM_DEPTH => s.depth = value,
            a if within(a, spec::VM_ARAM, spec::VM_ARAM + 2) => {
                let shift = 8 * (a - spec::VM_ARAM);
                s.aram = (s.aram & !(0xFF << shift)) | u32::from(value) << shift;
            }
            spec::VM_VOL => s.vol = value,
            spec::VM_MARK => s.mark_lo = value,
            a if a == spec::VM_MARK + 1 => s.mark(u16::from(s.mark_lo) | u16::from(value) << 8),
            a if within(a, spec::VM_VRAM, spec::VM_VRAM + 2) => {
                let shift = 8 * (a - spec::VM_VRAM);
                s.vram = (s.vram & !(0xFF << shift)) | u32::from(value) << shift;
            }
            spec::VM_FLAGS => s.flags = value,
            a if within(a, spec::VM_SLICE, spec::VM_SLICE + 1) => {
                let shift = 8 * (a - spec::VM_SLICE);
                s.slice = (s.slice & !(0xFF << shift)) | u16::from(value) << shift;
            }
            _ => {}
        }
    }

    /// The video registers the running program sees: a guest's own, or the real ones.
    fn active_video(&mut self) -> &mut Video {
        match self.vm.guest {
            Some(g) if self.vm.slots[g].vram != 0 => &mut self.vm.slots[g].video,
            _ => &mut self.video,
        }
    }

    /// The clock the CPU runs at now: a running guest's own, else the host's; the front
    /// panel's switch beats both.
    #[must_use]
    pub fn clock(&self) -> u8 {
        match self.vm.guest {
            Some(g) => self.speed_forced.unwrap_or(self.vm.slots[g].speed),
            None => self.speed,
        }
    }

    /// The sound chip the running program's FIRQ comes from.
    fn active_firq(&self) -> bool {
        match self.vm.guest {
            Some(g) if self.vm.slots[g].aram != 0 => self.vm.slots[g].apu.firq(),
            _ => self.audio.firq(),
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

    /// Runs the audio engines for one output tick: the real one over Audio RAM, and each
    /// guest's own over its Audio RAM, at its volume, all mixed.
    pub fn audio_tick(&mut self) -> (i16, i16) {
        let (l, r) = self.audio.tick(&self.ram[spec::ARAM_BASE as usize..=spec::ARAM_END as usize]);
        let (mut l, mut r) = (i32::from(l), i32::from(r));
        for s in &mut self.vm.slots {
            let Some(range) = s.aram_range() else { continue };
            let (gl, gr) = s.apu.tick(&self.ram[range]);
            l += i32::from(gl) * i32::from(s.vol) / 255;
            r += i32::from(gr) * i32::from(s.vol) / 255;
        }
        let clip = |v: i32| v.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        (clip(l), clip(r))
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
pub use spec::{SYS_LOCK, SYS_RESET, SYS_SPEED, SYS_SWITCH};
/// `SYS_SWITCH` reads 0 while the program picks the clock, else this plus the forced speed.
pub const SWITCH_FORCED: u8 = spec::SWITCH_FORCED as u8;
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
        match self.vm.guest {
            Some(slot) => self.guest_read(addr.0, slot),
            None => self.host_read(addr.0),
        }
    }

    fn write(&mut self, addr: Addr, value: u8) {
        match self.vm.guest {
            Some(slot) => self.guest_write(addr.0, value, slot),
            None => self.host_write(addr.0, value),
        }
    }
}

impl SystemBus {
    fn host_read(&mut self, a: u32) -> u8 {
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
        } else if within(a, spec::MS_X, spec::MS_WHEEL) {
            self.pointer.read(a)
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
        } else if a == SYS_SWITCH {
            self.speed_forced.map_or(0, |s| SWITCH_FORCED | s)
        } else if within(a, spec::VM_SLOT, vm::GUARDED_END) {
            self.vm_read(a)
        } else if within(a, spec::VRAM_END + 1, spec::CART_STAT - 1) {
            // reserved I/O
            0xFF
        } else {
            // audio RAM, OAM, VRAM and the high vectors are plain RAM
            self.ram[a as usize]
        }
    }

    fn host_write(&mut self, a: u32, value: u8) {
        if within(a, spec::VM_SLOT, vm::GUARDED_END) {
            self.vm_write(a, value);
        } else if within(a, NVRAM_BASE, NVRAM_END) {
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
        } else if within(a, spec::CART_WIN, spec::CART_WIN_END) {
            self.cart.window_write(a - spec::CART_WIN, value);
        } else if a < 4 || within(a, spec::BOOT_ROM, spec::CART_WIN - 1) || within(a, spec::VRAM_END + 1, spec::CART_STAT - 1) {
            // ROM and reserved I/O ignore writes
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

const VM_FRESH: u8 = spec::VM_FRESH as u8;

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
    /// A windowed guest's picture: the one being drawn, and the last finished one,
    /// which the window shows.
    guest_next: Vec<Vec<u32>>,
    guest_shown: Vec<Vec<u32>>,
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
            guest_next: vec![vec![0; p63_ppu::WIDTH * p63_ppu::HEIGHT]; vm::SLOTS],
            guest_shown: vec![vec![0; p63_ppu::WIDTH * p63_ppu::HEIGHT]; vm::SLOTS],
        }
    }

    /// High-level boot: checks the cart like the boot ROM, loads it and jumps to it.
    pub fn boot(&mut self, cart_file: Vec<u8>) -> Result<Cart, BootError> {
        let cart = Cart::parse(cart_file)?;
        cart.load(&mut self.cpu.bus);
        self.cpu.bus.cart.insert(cart.data.clone());
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
        self.cpu.bus.cart.insert(cart_file);
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
        bus.vm = vm::Vm::default();
        self.cpu.reset();
    }

    /// One instruction, for the host or the guest. A guest that has to stop goes back
    /// to the host here; if its stop was a missing page, the instruction is undone so it
    /// can run again once the host maps the page. Nothing a guest does stops the console.
    /// Public for tools that step through a frame; `run_frame` is what runs the console.
    pub fn step(&mut self) -> Result<Cycles, CpuError> {
        if self.cpu.bus.vm.stop.is_some() {
            // a stop for the running guest goes first
        } else if let Some(slot) = self.cpu.bus.vm.take_live_due() {
            // a live guest's interrupt is due: it runs its handler now, in the time of
            // whoever has the CPU (the host, or another guest)
            self.borrow(slot);
            return Ok(Cycles(1));
        }
        let Some(_) = self.cpu.bus.vm.guest else {
            let cycles = self.cpu.step()?;
            if let Some(go) = self.cpu.bus.vm.go.take() {
                self.vm_enter(go);
            }
            return Ok(cycles);
        };
        if let Some((why, arg)) = self.cpu.bus.vm.stop.take() {
            self.guest_stop(why, arg);
            return Ok(Cycles(1));
        }
        let flags = self.cpu.bus.vm.slots[self.cpu.bus.vm.guest.unwrap_or(0)].flags;
        if flags & (vm::VMF_WAITV | vm::VMF_SLEEP) == vm::VMF_WAITV && self.cpu.halt() == Halt::VBlank {
            // still waiting for its vblank: the host can run someone else meanwhile
            self.guest_stop(vm::WHY_WAITV, 1);
            return Ok(Cycles(1));
        }
        let pc = self.cpu.registers.pc();
        let before = self.cpu.checkpoint();
        self.cpu.bus.vm.undo.clear();
        let result = self.cpu.step();
        if result.is_ok() && self.cpu.registers.pc() == spec::BOOT_ROM {
            // RESET would restart the console; in a guest it ends the guest
            self.cpu.rollback(before);
            self.cpu.bus.vm.stop = None;
            self.guest_stop(vm::WHY_RESET, 0);
            return Ok(Cycles(2));
        }
        let flags = self.cpu.bus.vm.slots[self.cpu.bus.vm.guest.unwrap_or(0)].flags;
        if flags & vm::VMF_WAITV != 0 && before.halt() == Halt::Running && self.cpu.halt() == Halt::VBlank {
            // its frame is done: the host can have the CPU until vblank
            self.cpu.bus.vm.stop_with(vm::WHY_WAITV, 0);
        }
        if flags & (vm::VMF_WAITV | vm::VMF_SLEEP | vm::VMF_LIVE) == vm::VMF_WAITV | vm::VMF_LIVE && self.cpu.halt() == Halt::Interrupt {
            // WAIT with nothing to wake it: a live guest's interrupts come on time anyway,
            // so it waits stopped, like WAITV (1: it was already waiting)
            self.cpu.bus.vm.stop_with(vm::WHY_WAITV, u32::from(before.halt() != Halt::Running));
        }
        match (result, self.cpu.bus.vm.stop.take()) {
            (Err(_), _) => {
                self.cpu.bus.undo_guest_writes();
                self.cpu.rollback(before);
                self.guest_stop(vm::WHY_ILLEGAL, pc);
                Ok(Cycles(1))
            }
            (Ok(cycles), Some((vm::WHY_PAGE, arg))) => {
                self.cpu.bus.undo_guest_writes();
                self.cpu.rollback(before);
                self.guest_stop(vm::WHY_PAGE, arg);
                Ok(cycles)
            }
            (Ok(cycles), Some((why, arg))) => {
                if why == vm::WHY_SYSTEM && self.cpu.bus.vm.borrowed.is_some() {
                    // only the host can act on it: the guest does it again in its own turn
                    self.cpu.rollback(before);
                }
                self.guest_stop(why, arg);
                Ok(cycles)
            }
            (Ok(cycles), None) => {
                if let Some((sp, taken)) = self.cpu.bus.vm.borrowed {
                    let now = self.cpu.registers.sp();
                    let lines = self.cpu.bus.vm.slots[self.cpu.bus.vm.guest.unwrap_or(0)].lines;
                    if (taken && now == sp) || (!taken && self.cpu.halt() != Halt::Running) || lines > vm::LIVE_LINES {
                        // back from its handler (or it couldn't take it, or it ran long)
                        self.give_back(None);
                    } else if now < sp {
                        self.cpu.bus.vm.borrowed = Some((sp, true));
                    }
                }
                Ok(cycles)
            }
        }
    }

    /// A guest has to stop: back to the host with why, or, if it was only running its
    /// interrupt handler in someone else's time, back to them with nothing said. A chord
    /// or a vblank cut-off made meanwhile is theirs: a guest gets it as its own stop, the
    /// host gets the chord in `VM_CHORD`.
    fn guest_stop(&mut self, why: u8, arg: u32) {
        if self.cpu.bus.vm.borrowed.is_some() {
            self.give_back(matches!(why, vm::WHY_CHORD | vm::WHY_SLICE).then_some((why, arg)));
        } else {
            self.vm_exit(why, arg);
        }
    }

    /// Live guest `slot` runs its interrupt handler in the running program's time.
    fn borrow(&mut self, slot: usize) {
        self.cpu.bus.vm.slots[slot].live_line = Some(self.cpu.bus.vm.slots[slot].video.line);
        if self.cpu.bus.vm.guest.is_some() {
            self.cpu.bus.vm.from = self.save_guest();
            self.load_guest(slot, false);
        } else {
            self.vm_enter(slot as u8);
        }
        self.cpu.bus.vm.borrowed = Some((self.cpu.registers.sp(), false));
    }

    /// The end of a live guest's interrupt: whoever had the CPU carries on, with `pass`
    /// (a stop that was really theirs).
    fn give_back(&mut self, pass: Option<(u8, u32)>) {
        let vm = &mut self.cpu.bus.vm;
        vm.borrowed = None;
        vm.live_check = true;
        match vm.from.take() {
            None => {
                if let Some((vm::WHY_CHORD, button)) = pass {
                    vm.chord = button as u8;
                }
                self.vm_leave();
            }
            Some(other) => {
                self.save_guest();
                let lines = self.cpu.bus.vm.slots[other].lines;
                self.load_guest(other, false);
                self.cpu.bus.vm.slots[other].lines = lines;
                self.cpu.bus.vm.stop = pass;
            }
        }
    }

    /// `VM_GO`: the host's CPU and cart registers are put aside and the guest's loaded,
    /// its registers from its table.
    fn vm_enter(&mut self, go: u8) {
        let slot = usize::from(go) % vm::SLOTS;
        self.cpu.bus.vm.host = Some((self.cpu.save_state(), self.cpu.bus.cart.view()));
        self.load_guest(slot, go & VM_FRESH != 0);
    }

    /// Guest `slot` gets the CPU: its registers from its table, from scratch if `fresh`.
    fn load_guest(&mut self, slot: usize, fresh: bool) {
        let bus = &mut self.cpu.bus;
        let s = &mut bus.vm.slots[slot];
        let saved = s.cpu.take();
        let fresh = fresh || saved.is_none();
        let at = (s.table + spec::VMT_STATE) as usize;
        let mut state = if fresh { p63_core::CpuState::default() } else { saved.unwrap_or_default() };
        state.registers = vm::registers_from(&bus.ram[at..at + vm::STATE_LEN]);
        if fresh {
            s.cart = CartView { bank: s.cartbase, ctrl: 0, sbank: 0 };
            s.video = Video::default();
            s.apu = Apu::default();
            s.speed = SPEED_NORMAL;
            s.speed_requested = None;
            s.kb_ctrl = (spec::KBC_ON | spec::KBC_MIRROR) as u8;
        }
        s.lines = 0;
        s.load_pages(&bus.ram);
        let cart = s.cart;
        bus.cart.set_view(cart);
        bus.vm.guest = Some(slot);
        bus.vm.stop = None;
        self.cpu.load_state(state);
    }

    /// The guest stops: its registers go to its table, the host carries on after its
    /// `VM_GO` write with `VM_WHY` and `VM_ARG` saying why.
    fn vm_exit(&mut self, why: u8, arg: u32) {
        let Some(slot) = self.vm_leave() else { return };
        let vm = &mut self.cpu.bus.vm;
        vm.slots[slot].why = why;
        vm.slots[slot].arg = arg;
        if why == vm::WHY_WAITV && arg == 0 {
            vm.slots[slot].frames = vm.slots[slot].frames.wrapping_add(1);
        }
        vm.last = slot as u8;
    }

    /// The guest's registers and cart view put away, and the host's put back.
    fn vm_leave(&mut self) -> Option<usize> {
        let slot = self.save_guest()?;
        let bus = &mut self.cpu.bus;
        if let Some((cpu, cart)) = bus.vm.host.take() {
            bus.cart.set_view(cart);
            self.cpu.load_state(cpu);
        }
        Some(slot)
    }

    /// The running guest's registers go to its table and its cart view is kept.
    fn save_guest(&mut self) -> Option<usize> {
        let slot = self.cpu.bus.vm.guest.take()?;
        let state = self.cpu.save_state();
        let bus = &mut self.cpu.bus;
        let at = (bus.vm.slots[slot].table + spec::VMT_STATE) as usize;
        bus.ram[at..at + vm::STATE_LEN].copy_from_slice(&vm::state_bytes(&state.registers));
        let s = &mut bus.vm.slots[slot];
        s.cpu = Some(state);
        s.cart = bus.cart.view();
        Some(slot)
    }

    /// Runs one frame: 262 lines of 455 dots, with vertical blank at line 240. A line is
    /// 455 CPU cycles at normal speed, 910 in turbo and 227.5 in low power.
    /// Each visible line is drawn as it starts, so changes made during the frame show
    /// from the next line on.
    pub fn run_frame(&mut self) -> Result<(), CpuError> {
        self.audio.clear();
        let bus = &mut self.cpu.bus;
        bus.keyboard.frame();
        bus.vm.pad_changed(bus.keyboard.pad1());
        for button in std::mem::take(&mut bus.keyboard.chords) {
            bus.vm.key_chord(button);
        }
        for line in 0..spec::LINES_PER_FRAME as u16 {
            let waiting = self.cpu.halt() != Halt::Running;
            let bus = &mut self.cpu.bus;
            bus.vm.count_line(waiting);
            bus.video.start_line(line);
            for s in &mut bus.vm.slots {
                s.video.start_line(line);
            }
            bus.vm.live_check = true;
            if let Some(g) = bus.vm.guest {
                let s = &mut bus.vm.slots[g];
                s.lines += 1;
                if s.slice != 0 && s.lines >= u32::from(s.slice) {
                    bus.vm.stop_with(vm::WHY_SLICE, 0);
                }
            }
            if usize::from(line) < p63_ppu::HEIGHT {
                self.render_line(usize::from(line));
            } else if line == spec::SCREEN_H as u16 {
                self.switch_speed();
                // under a live guest's handler, the program it cut into is the one running
                let from = self.cpu.bus.vm.from;
                let running_waits = match from.and_then(|f| self.cpu.bus.vm.slots[f].cpu.as_ref()) {
                    Some(cpu) => cpu.halt() == Halt::VBlank,
                    None => self.cpu.halt() == Halt::VBlank,
                };
                self.cpu.signal_vblank();
                // guests that aren't running wake from WAITV too; one that does wakes in
                // time for its frame, so whoever is running hands over now
                let mut woke = false;
                if let Some((host, _)) = &mut self.cpu.bus.vm.host {
                    // the host waits under a live guest's interrupt handler
                    host.signal_vblank();
                }
                for (i, s) in self.cpu.bus.vm.slots.iter_mut().enumerate() {
                    if let Some(cpu) = &mut s.cpu {
                        // waiting for vblank, or a live guest in WAIT, which its vblank
                        // interrupt is about to wake
                        let live_wait = cpu.halt() == Halt::Interrupt && s.flags & (vm::VMF_WAITV | vm::VMF_LIVE) == vm::VMF_WAITV | vm::VMF_LIVE;
                        woke |= (cpu.halt() == Halt::VBlank || live_wait) && from != Some(i);
                        cpu.signal_vblank();
                    }
                }
                // a guest the host schedules (VMF_WAITV) also hands over at every vblank,
                // so the host gets a turn each frame, unless it holds on (VMF_HOLD)
                if let Some(g) = self.cpu.bus.vm.guest {
                    let flags = self.cpu.bus.vm.slots[from.unwrap_or(g)].flags;
                    let scheduled = flags & (vm::VMF_WAITV | vm::VMF_HOLD) == vm::VMF_WAITV;
                    if (woke || scheduled) && !running_waits {
                        self.cpu.bus.vm.stop_with(vm::WHY_SLICE, 0);
                    }
                }
                std::mem::swap(&mut self.guest_next, &mut self.guest_shown);
            }
            let mut spent = self.debt;
            while spent < TICKS_PER_LINE {
                let dot = spent / 2;
                self.cpu.bus.video.dot = dot;
                let video = self.cpu.bus.active_video();
                video.dot = dot;
                let irq = video.irq() || self.cpu.bus.keyboard_irq();
                let firq = self.cpu.bus.active_firq();
                self.cpu.set_irq_pending(irq);
                self.cpu.set_firq_pending(firq);
                let idle = self.idle_ticks(irq || firq, TICKS_PER_LINE - spent);
                let ticks = if idle > 0 {
                    idle
                } else {
                    let cycles = self.step()?.0 + std::mem::take(&mut self.cpu.bus.stall);
                    if self.cpu.bus.reset_requested {
                        self.reset();
                    }
                    cycles * ticks_per_cycle(self.cpu.bus.clock())
                };
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

    /// Ticks the CPU can skip now, at most `left`: it's halted and nothing can wake or
    /// stop it before the end of the line (WAITV) or the next sound tick (WAIT). 0: step.
    fn idle_ticks(&self, pending: bool, left: u32) -> u32 {
        let vm = &self.cpu.bus.vm;
        if vm.stop.is_some() || vm.live_check || self.cpu.bus.reset_requested {
            return 0;
        }
        let flags = vm.guest.map_or(0, |g| vm.slots[g].flags);
        let guest = vm.guest.is_some();
        // a scheduled guest hands back at WAITV, and a live one at WAIT, so they step
        let hands_back_waitv = guest && flags & (vm::VMF_WAITV | vm::VMF_SLEEP) == vm::VMF_WAITV;
        let hands_back_wait = guest && flags & (vm::VMF_WAITV | vm::VMF_SLEEP | vm::VMF_LIVE) == vm::VMF_WAITV | vm::VMF_LIVE;
        match self.cpu.halt() {
            Halt::VBlank if !hands_back_waitv => left,
            Halt::Interrupt if !pending && !hands_back_wait => left.min(p63_apu::TICK_CYCLES * 2 - self.audio_ticks).max(1),
            _ => 0,
        }
    }

    /// At vblank: the front panel's switch if it's set, else what the program asked for.
    fn switch_speed(&mut self) {
        let bus = &mut self.cpu.bus;
        if let Some(asked) = bus.speed_requested.take() {
            bus.speed_program = asked;
        }
        bus.speed = bus.speed_forced.unwrap_or(bus.speed_program);
        for s in &mut bus.vm.slots {
            if let Some(asked) = s.speed_requested.take() {
                s.speed = asked;
            }
        }
    }

    /// Draws one line from VRAM, OAM and the registers as they stand now: a guest's
    /// fullscreen, or the host's with each windowed guest in its half-size window.
    fn render_line(&mut self, line: usize) {
        let vm = &self.cpu.bus.vm;
        let full = usize::from(vm.show & 3);
        if vm.show & vm::SHOW_FULL != 0 && vm.slots[full].vram != 0 {
            self.render_guest(full, line, false);
            return;
        }
        let mut any = false;
        for s in 0..vm::SLOTS {
            if self.cpu.bus.vm.slots[s].windowed() {
                self.render_guest(s, line, true);
                any = true;
            }
        }
        let inset = any.then(|| self.window_line(line));
        self.render_host(line, inset.as_ref().map(|i| &i[..]));
    }

    fn render_host(&mut self, line: usize, inset: Option<&[Option<u32>]>) {
        let bus = &mut self.cpu.bus;
        let regs = ppu_regs(&bus.video);
        let video = &bus.ram[spec::OAM_BASE as usize..=spec::VRAM_END as usize];
        if p63_ppu::render_video_line(video, regs, line, &mut self.frame, inset) {
            bus.video.stat |= p63_ppu::STAT_OVERFLOW;
        }
    }

    /// A guest's line from its own context: onto the screen, or into the picture its
    /// window shows from the next frame.
    fn render_guest(&mut self, slot: usize, line: usize, windowed: bool) {
        let bus = &mut self.cpu.bus;
        let s = &mut bus.vm.slots[slot];
        let regs = ppu_regs(&s.video);
        let base = s.vram as usize;
        let Some(video) = bus.ram.get(base..base + p63_ppu::VIDEO_LEN) else { return };
        let out = if windowed { &mut self.guest_next[slot] } else { &mut self.frame };
        if p63_ppu::render_video_line(video, regs, line, out, None) {
            s.video.stat |= p63_ppu::STAT_OVERFLOW;
        }
    }

    /// The windows' pixels on screen line `line`, each one the average of a 2x2 block of
    /// its guest's last picture. Deeper windows are drawn later, so they end up on top.
    fn window_line(&self, line: usize) -> [Option<u32>; p63_ppu::WIDTH] {
        let (w, h) = (p63_ppu::WIDTH, p63_ppu::HEIGHT);
        let slots = &self.cpu.bus.vm.slots;
        let mut order = [0usize; vm::SLOTS];
        let mut n = 0;
        for s in (0..vm::SLOTS).filter(|&s| slots[s].windowed()) {
            order[n] = s;
            n += 1;
        }
        order[..n].sort_by_key(|&s| slots[s].depth);
        let mut out = [None; p63_ppu::WIDTH];
        for &s in &order[..n] {
            let (wx, wy) = (usize::from(slots[s].wx), usize::from(slots[s].wy));
            let Some(row) = line.checked_sub(wy).filter(|r| *r < h / 2) else { continue };
            let pic = &self.guest_shown[s];
            for (x, px) in out.iter_mut().enumerate().skip(wx).take(w / 2) {
                let (gx, gy) = ((x - wx) * 2, row * 2);
                let four = [pic[gy * w + gx], pic[gy * w + gx + 1], pic[(gy + 1) * w + gx], pic[(gy + 1) * w + gx + 1]];
                let channel = |shift: u32| (four.iter().map(|p| (p >> shift) & 0xFF).sum::<u32>() / 4) << shift;
                *px = Some(channel(16) | channel(8) | channel(0));
            }
        }
        out
    }
}

/// The registers the PPU needs, from a set of video registers.
fn ppu_regs(video: &Video) -> p63_ppu::Regs {
    let word = |a: u32| u16::from_le_bytes([video.register(a), video.register(a + 1)]);
    p63_ppu::Regs {
        ctrl: video.register(spec::V_CTRL),
        bright: video.register(spec::V_BRIGHT),
        scroll_x0: word(spec::V_SCROLLX0),
        scroll_y0: word(spec::V_SCROLLY0),
        scroll_x1: word(spec::V_SCROLLX1),
        scroll_y1: word(spec::V_SCROLLY1),
        bank: video.register(spec::V_BANK),
        palgrp: video.register(spec::V_PALGRP),
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
        assert_eq!(read(&mut bus, spec::MS_WHEEL + 1), 0xFF);
    }

    #[test]
    fn memory_cards_show_in_the_window() {
        let (card, wren, slot2) = (spec::CARTC_CARD as u8, spec::CARTC_WREN as u8, spec::CARTC_SLOT2 as u8);
        let mut bus = SystemBus::default();
        bus.cart.insert(p63_cart::pack(&[1, 2], &p63_cart::PackOptions::default()).unwrap());
        bus.cart.insert_card(1, p63_cart::card::blank());
        assert_eq!(read(&mut bus, spec::CART_STAT), (spec::CARTS_CART | spec::CARTS_CARD2 | spec::CARTS_NEW2) as u8);
        bus.write(Addr::new(spec::CART_STAT), spec::CARTS_NEW2 as u8);
        assert_eq!(read(&mut bus, spec::CART_STAT), (spec::CARTS_CART | spec::CARTS_CARD2) as u8, "writing 1 clears NEW");

        bus.write(Addr::new(spec::CART_CTRL), card);
        assert_eq!(read(&mut bus, spec::CART_WIN), 0xFF, "slot 1 is empty");
        bus.write(Addr::new(spec::CART_CTRL), card | slot2);
        assert_eq!(read(&mut bus, spec::CART_WIN), b'P', "the directory of the card in slot 2");
        bus.write(Addr::new(spec::CART_WIN + 0x2000), 0x5A);
        assert_eq!(read(&mut bus, spec::CART_WIN + 0x2000), 0xFF, "no writes without WREN");
        bus.write(Addr::new(spec::CART_CTRL), card | slot2 | wren);
        bus.write(Addr::new(spec::CART_SBANK), 15);
        bus.write(Addr::new(spec::CART_WIN + 0xFFFF), 0x5A);
        let data = &bus.cart.cards[1].as_ref().unwrap().data;
        assert_eq!(data[spec::CARD_SIZE as usize - 1], 0x5A, "bank 15 is the card's last 64 KB");
        assert!(bus.cart.cards[1].as_ref().unwrap().dirty);
        bus.write(Addr::new(spec::CART_SBANK), 16);
        assert_eq!(read(&mut bus, spec::CART_WIN), 0xFF, "past the end of the card");
        bus.write(Addr::new(spec::CART_CTRL), 0);
        assert_eq!(read(&mut bus, spec::CART_WIN), b'P', "back to the cart's ROM");
        assert_eq!(read(&mut bus, spec::CART_WIN + 1), b'6');

        assert!(bus.cart.eject_card(1).is_some());
        assert_eq!(read(&mut bus, spec::CART_STAT), (spec::CARTS_CART | spec::CARTS_NEW2) as u8, "taking it out is news too");
    }

    #[test]
    fn the_card_library_keeps_the_standard_layout() {
        let games = std::path::PathBuf::from("../games");
        let program = p63_asm::assemble_file_in("tests/card_lib.p63", &[games]).unwrap();
        let mut m = Machine::new();
        for (i, b) in program.image_at(0x100).unwrap().into_iter().enumerate() {
            m.cpu.bus.ram[0x100 + i] = b;
        }
        m.cpu.bus.cart.insert(p63_cart::pack(&[1, 2], &p63_cart::PackOptions::default()).unwrap());
        m.cpu.bus.cart.insert_card(0, p63_cart::card::blank());
        let r = &mut m.cpu.registers;
        r.set_pc(0x100);
        r.set_sp(0x8000);
        r.set_cc(Flags::F | Flags::I);
        for _ in 0..30 {
            m.run_frame().unwrap();
        }
        // opened, saved, 124 free, found at block 3, loaded, page 0 changed, page 39 kept,
        // 125 free after the delete, gone, and the cart's ROM back in the window
        assert_eq!(&m.cpu.bus.ram[0x10..0x1A], &[1, 1, 124, 3, 1, 0x77, 39, 125, 0, b'P']);
        let card = &m.cpu.bus.cart.cards[0].as_ref().unwrap().data;
        let saves = p63_cart::card::saves(card).unwrap();
        assert_eq!(saves.len(), 1, "what the console wrote, read back by the reference reader");
        let s = &saves[0];
        assert_eq!((s.game.as_str(), s.name.as_str(), s.title.as_str(), s.seq), ("CARDTEST", "SLOT1", "Card test", 1));
        assert_eq!(s.blocks, vec![3, 4]);
        assert_eq!(card[4 * p63_cart::card::BLOCK + 8 * 256], 39, "data page 39 is page 8 of the second block");
    }

    const VM_EQUS: &str = "
VM_SLOT  EQU $FFFE44
VM_GO    EQU $FFFE45
VM_WHY   EQU $FFFE46
VM_ARG   EQU $FFFE48
VM_TABLE EQU $FFFE4C
TABLE    EQU $002000
RESULT   EQU $003000
";

    /// A machine with `host` at $000100 and `guest` at guest $000100, which is real page
    /// 1 ($040000). The guest's table is at $002000 with only page 0 mapped; it starts at
    /// $000100 with interrupts masked.
    fn vm_machine(host: &str, guest: &str) -> Machine {
        let mut m = Machine::new();
        let load = |m: &mut Machine, src: &str, at: u32| {
            let p = p63_asm::assemble("t", &format!("{VM_EQUS}\n    ORG $000100\n{src}")).unwrap();
            for (i, b) in p.image_at(0x100).unwrap().into_iter().enumerate() {
                m.cpu.bus.ram[at as usize + i] = b;
            }
        };
        load(&mut m, host, 0x100);
        load(&mut m, guest, 0x04_0100);
        m.cpu.bus.ram[0x2000] = 0x81;
        let state = &mut m.cpu.bus.ram[0x2000 + spec::VMT_STATE as usize..];
        state[0] = 0x50;
        state[16] = 0x00;
        state[17] = 0x01;
        let r = &mut m.cpu.registers;
        r.set_pc(0x100);
        r.set_sp(0x8000);
        r.set_cc(Flags::F | Flags::I);
        m
    }

    fn host_result(m: &mut Machine, n: usize) -> Vec<u8> {
        m.cpu.bus.ram[0x3000..0x3000 + n].to_vec()
    }

    const RUN_GUEST: &str = "
    CLR A
    ST A,VM_SLOT
    LD Q,#TABLE
    ST Q,VM_TABLE
    LD A,#$80
    ST A,VM_GO
    LD A,VM_WHY
    ST A,RESULT
    LD Q,VM_ARG
    ST Q,RESULT+1
";

    #[test]
    fn a_missing_page_stops_the_guest_and_its_instruction_runs_again() {
        let host = format!(
            "{RUN_GUEST}
    LD A,$0BFFFF                    ; the ST D's first byte, undone
    ST A,RESULT+10
    LD A,#$83                       ; map guest page 2 on real page 3, run on
    ST A,TABLE+2
    CLR A
    ST A,VM_GO
    LD A,VM_WHY
    ST A,RESULT+5
    LD Q,VM_ARG
    ST Q,RESULT+6
DONE:
    BRA DONE"
        );
        let guest = "
    LD A,#$5A
    ST A,$001000
    LD D,#$1234
    ST D,$07FFFF                    ; one byte on page 1, the other on page 2
    ST A,$080000
    ST A,$FFFED1";
        let mut m = vm_machine(&host, guest);
        m.cpu.bus.ram[0x2001] = 0x82;
        m.run_frame().unwrap();
        let r = host_result(&mut m, 11);
        assert_eq!(m.cpu.bus.ram[0x04_1000], 0x5A, "guest RAM is on its page");
        assert_eq!(&r[..5], &[vm::WHY_PAGE, 0x00, 0x00, 0x08, 0x00], "the ST D stopped at $080000");
        assert_eq!(r[10], 0, "and its first byte was undone");
        assert_eq!(&r[5..10], &[vm::WHY_RESET, 0x12, 0, 0, 0], "ran again, then wrote A to SYS_RESET");
        assert_eq!(m.cpu.bus.ram[0x0B_FFFF], 0x34, "page 1 is real page 2");
        assert_eq!(m.cpu.bus.ram[0x0C_0000], 0x12, "page 2 is real page 3");
        assert!(!m.cpu.bus.reset_requested, "the console didn't reset");
        let at = 0x2000 + spec::VMT_STATE as usize;
        assert_eq!(&m.cpu.bus.ram[at + 3..at + 5], &[0x34, 0x12], "the guest's registers are in its table (D)");
    }

    #[test]
    fn a_chord_stops_a_busy_guest_and_hides_its_buttons() {
        let host = format!("{RUN_GUEST}\nDONE:\n    BRA DONE");
        let guest = "
SPIN:
    LD A,$FFFFDC
    ST A,$002000
    BRA SPIN";
        let mut m = vm_machine(&host, guest);
        m.cpu.bus.keyboard.pad[0] = 0x30;
        m.run_frame().unwrap();
        assert_eq!(m.cpu.bus.ram[0x04_2000], 0x30, "Select+Start alone reach the game");
        assert_eq!(m.cpu.bus.vm.guest, Some(0));
        m.cpu.bus.keyboard.pad[0] = 0x38;
        m.run_frame().unwrap();
        assert_eq!(&host_result(&mut m, 2), &[vm::WHY_CHORD, 0x08]);
        assert_eq!(m.cpu.bus.vm.guest, None);
        assert_eq!(m.cpu.bus.vm.mask, 0x38);
        m.cpu.bus.vm.guest = Some(0);
        assert_eq!(m.cpu.bus.read(Addr::new(spec::KB_PAD)), 0, "the chord is hidden from the guest");
    }

    #[test]
    fn guests_cant_stop_the_console() {
        let host = format!("{RUN_GUEST}\nDONE:\n    BRA DONE");
        let mut m = vm_machine(&host, "    LD A,#1\n    ST A,$FFFED2\n    LD A,#2\n    ST A,$FFFE80\n");
        m.run_frame().unwrap();
        assert_eq!(&host_result(&mut m, 5), &[vm::WHY_SYSTEM, 0x80, 0xFE, 0xFF, 0x02], "the settings RAM, value 2");
        assert_eq!(m.cpu.bus.vm.slots[0].speed, SPEED_TURBO, "turbo for itself, from vblank");
        assert_eq!(m.cpu.bus.speed, SPEED_NORMAL, "not for the console");
        assert_eq!(m.cpu.bus.nvram[0], 0, "and the settings RAM is untouched");

        let mut m = vm_machine(&host, "    .BYTE $FD, $00\n");
        m.run_frame().unwrap();
        assert_eq!(&host_result(&mut m, 4), &[vm::WHY_ILLEGAL, 0x00, 0x01, 0x00]);

        let mut m = vm_machine(&host, "    LD A,$FFFE44\n    ST A,$001000\n    RESET\n");
        m.run_frame().unwrap();
        assert_eq!(m.cpu.bus.ram[0x04_1000], 0xFF, "the VM registers read $FF");
        assert_eq!(host_result(&mut m, 1)[0], vm::WHY_RESET);
        assert_eq!(m.cpu.registers.pc() & 0xFF_0000, 0, "the host is still running");
    }

    const CONTEXT: u32 = 0x10_0000;

    /// The guest gets a video context at $100000 and the flags and slice given.
    fn run_with_context(flags: u8, slice: u16) -> String {
        format!(
            "
    CLR A
    ST A,VM_SLOT
    LD Q,#TABLE
    ST Q,VM_TABLE
    LD Q,#{CONTEXT}
    ST Q,$FFFE55                    ; VM_VRAM, and VM_FLAGS after it
    LD A,#{flags}
    ST A,$FFFE58
    LD D,#{slice}
    ST D,$FFFE59
    LD A,#$80
    ST A,VM_GO
    LD A,VM_WHY
    ST A,RESULT
DONE:
    BRA DONE"
        )
    }

    #[test]
    fn a_guest_draws_into_its_own_context_and_shows_full_or_windowed() {
        let guest = "
    LD A,#$E0                       ; backdrop red, display on: in its own context
    ST A,$FFE600
    LD A,#1
    ST A,$FFFF80
    WAITV
    BRA *";
        let mut m = vm_machine(&run_with_context(vm::VMF_WAITV, 0), guest);
        m.run_frame().unwrap();
        assert_eq!(host_result(&mut m, 1)[0], vm::WHY_WAITV, "stopped at its WAITV");
        assert_eq!(m.cpu.bus.ram[spec::VRAM_PALETTE as usize], 0, "the real palette is untouched");
        assert_eq!(m.cpu.bus.ram[(CONTEXT + spec::VRAM_PALETTE - spec::OAM_BASE) as usize], 0xE0);
        assert_eq!(m.cpu.bus.vm.slots[0].video.register(spec::V_CTRL), 1, "its own registers");
        assert_eq!(m.cpu.bus.video.register(spec::V_CTRL), 0);
        let red = p63_ppu::rgb(0xE0, 0);

        m.cpu.bus.vm.show = vm::SHOW_FULL;
        m.run_frame().unwrap();
        assert_eq!(m.frame[0], red, "fullscreen: the guest's picture");

        m.cpu.bus.video.write(spec::V_CTRL - spec::VIDEO_IO_BASE, 1);
        m.cpu.bus.vm.show = 0;
        let s = &mut m.cpu.bus.vm.slots[0];
        s.flags |= vm::VMF_WINDOW;
        (s.wx, s.wy) = (8, 4);
        m.run_frame().unwrap();
        m.run_frame().unwrap();
        let px = |m: &Machine, x: usize, y: usize| m.frame[y * p63_ppu::WIDTH + x];
        assert_eq!(px(&m, 8, 4), red, "the window's corner");
        assert_eq!(px(&m, 167, 123), red, "and its far corner");
        assert_eq!(px(&m, 7, 4), 0, "the host's black backdrop around it");
        assert_eq!(px(&m, 168, 4), 0);
        assert_eq!(px(&m, 8, 124), 0);
    }

    #[test]
    fn a_slice_stops_a_guest_that_never_waits() {
        let mut m = vm_machine(&run_with_context(0, 10), "SPIN:\n    BRA SPIN");
        m.run_frame().unwrap();
        assert_eq!(host_result(&mut m, 1)[0], vm::WHY_SLICE);
    }

    #[test]
    fn half_size_averages_each_2x2_block_and_deeper_windows_cover() {
        let mut m = Machine::new();
        let w = p63_ppu::WIDTH;
        let pic = &mut m.guest_shown[0];
        pic[0] = 0x00FF_0000;
        pic[1] = 0x0000_FF00;
        pic[w] = 0x0000_00FF;
        pic[w + 1] = 0x00FF_FFFF;
        m.guest_shown[1].fill(0x0000_1234);
        for (i, (x, depth)) in [(0, 1), (100, 0)].into_iter().enumerate() {
            let s = &mut m.cpu.bus.vm.slots[i];
            (s.vram, s.flags, s.wx, s.depth) = (CONTEXT, vm::VMF_WINDOW, x, depth);
        }
        let line = m.window_line(0);
        assert_eq!(line[0], Some(0x007F_7F7F));
        assert_eq!(line[100], Some(0), "slot 0 is deeper, so it covers slot 1");
        assert_eq!(line[160], Some(0x0000_1234), "slot 1 past slot 0's 160");
        assert_eq!(line[260], None, "and 160 wide");
        assert!(m.window_line(120).iter().all(Option::is_none), "120 tall");
    }

    #[test]
    fn a_guest_still_waiting_hands_back_at_once() {
        let guest = "    WAITV\n    BRA *";
        let host = format!("{RUN_GUEST}\n    CLR A\n    ST A,VM_GO\n    LD A,VM_WHY\n    ST A,RESULT+5\n    LD Q,VM_ARG\n    ST Q,RESULT+6\nDONE:\n    BRA DONE");
        let mut m = vm_machine(&host, guest);
        m.cpu.bus.vm.slots[0].flags = vm::VMF_WAITV;
        m.run_frame().unwrap();
        let r = host_result(&mut m, 7);
        assert_eq!((r[0], r[1]), (vm::WHY_WAITV, 0), "ran its WAITV");
        assert_eq!((r[5], r[6]), (vm::WHY_WAITV, 1), "then was still waiting");

        let mut m = vm_machine(&host, "SPIN:\n    WAITV\n    BRA SPIN");
        m.cpu.bus.vm.slots[0].flags = vm::VMF_WAITV | vm::VMF_SLEEP;
        m.run_frame().unwrap();
        m.run_frame().unwrap();
        let r = host_result(&mut m, 7);
        assert_eq!((r[5], r[6]), (vm::WHY_WAITV, 0), "with VMF_SLEEP it slept through to its next WAITV");
    }

    #[test]
    fn each_guest_has_its_own_sound_in_the_mix() {
        let mut bus = SystemBus::default();
        let s = &mut bus.vm.slots[1];
        (s.aram, s.vol) = (0x20_0000, 255);
        bus.vm.guest = Some(1);
        bus.write(Addr::new(spec::ARAM_BASE), 0x40);
        bus.write(Addr::new(spec::AUD_MVOL_L), 0x7F);
        assert_eq!(bus.ram[0x20_0000], 0x40, "its Audio RAM is in host RAM");
        assert_eq!(bus.ram[spec::ARAM_BASE as usize], 0, "the real one is untouched");
        assert_eq!(bus.vm.slots[1].apu.read(spec::AUD_MVOL_L - spec::AUDIO_IO_BASE), 0x7F);
        assert_eq!(bus.audio.read(spec::AUD_MVOL_L - spec::AUDIO_IO_BASE), 0, "and its own registers");
        bus.vm.guest = None;
        let quiet = bus.audio_tick();
        assert_eq!(quiet, (0, 0), "nothing playing yet");
    }

    #[test]
    fn usage_counters_count_lines() {
        let mut m = vm_machine(&run_with_context(0, 50), "SPIN:\n    BRA SPIN");
        m.run_frame().unwrap();
        let rd = |m: &mut Machine, a: u32| (0..4).fold(0u32, |v, i| v | u32::from(m.cpu.bus.peek(Addr::new(a + i))) << (8 * i));
        assert_eq!(rd(&mut m, spec::VM_TICKS), 262, "every line of the frame");
        let used = rd(&mut m, spec::VM_USED);
        assert!((49..=51).contains(&used), "the guest ran its 50-line slice: {used}");
        assert_eq!(rd(&mut m, spec::VM_IDLE), 0, "nobody waited");
    }

    #[test]
    fn ram_in_use_counts_marked_and_written_kb() {
        let host = "
    CLR A
    ST A,VM_SLOT
    LD D,#3                             ; the image: blocks 0-2
    ST D,$FFFE6D
    LD Q,#TABLE
    ST Q,VM_TABLE
    LD A,#$80
    ST A,VM_GO
    LD D,$FFFE6F
    ST D,RESULT
DONE:
    BRA DONE";
        let guest = "
    LD A,#1
    ST A,$000500                        ; in the image: already counted
    ST A,$001400                        ; block 5
    ST A,$0017FF                        ; block 5 again
    ST A,$002000                        ; block 8
    ST A,$FFFED1";
        let mut m = vm_machine(host, guest);
        m.run_frame().unwrap();
        assert_eq!(&host_result(&mut m, 2), &[5, 0], "3 marked + 2 written = 5 KB");
    }

    /// Frames of vblank interrupts a stopped guest handles while the host never gives it
    /// another turn, with or without `VMF_LIVE`.
    fn stopped_guest_interrupts(flags: u8) -> u8 {
        let host = format!(
            "{RUN_GUEST_SETUP}
    LD Q,#${flags:02X}100000              ; its own screen at $100000, and its flags
    ST Q,${vram:06X}
    LD D,#100                           ; a short turn
    ST D,${slice:06X}
    LD A,#$80
    ST A,VM_GO
    LD A,VM_WHY
    ST A,RESULT
DONE:
    BRA DONE",
            vram = spec::VM_VRAM,
            slice = spec::VM_SLICE,
        );
        let guest = format!(
            "
    LD X,#$008000
    MOV SP,X
    LD Q,#HANDLER
    ST W,${vec:06X}
    ST B,${vec:06X}+2
    LD A,#$01
    ST A,${irqst:06X}
    ST A,${irqen:06X}                   ; vblank
    CLI
LOOP:
    BRA LOOP
HANDLER:
    LD A,#$01
    ST A,${irqst:06X}
    LD A,$001000
    INC A
    ST A,$001000
    RTI",
            vec = spec::VEC_IRQ,
            irqst = spec::V_IRQST,
            irqen = spec::V_IRQEN,
        );
        let mut m = vm_machine(&host, &guest);
        for _ in 0..6 {
            m.run_frame().unwrap();
        }
        assert_eq!(host_result(&mut m, 1)[0], vm::WHY_SLICE, "its turn ran out and the host kept the CPU");
        m.cpu.bus.ram[0x04_1000]
    }

    const RUN_GUEST_SETUP: &str = "
    CLR A
    ST A,VM_SLOT
    LD Q,#TABLE
    ST Q,VM_TABLE
";

    #[test]
    fn a_live_guest_takes_its_interrupts_while_stopped() {
        let taken = stopped_guest_interrupts(vm::VMF_LIVE);
        assert!(taken >= 5, "one a frame while the host had the CPU, got {taken}");
        assert_eq!(stopped_guest_interrupts(0), 0, "without VMF_LIVE they wait for its turn");
    }

    #[test]
    fn a_live_guest_takes_its_interrupts_while_another_guest_runs() {
        let mut m = vm_machine(&format!("{RUN_GUEST}\nDONE:\n    BRA DONE"), "LOOP:\n    BRA LOOP");
        // slot 1: stopped, live, a vblank handler that counts at its $001000 (real $0C1000)
        m.cpu.bus.ram[0x2100] = 0x83;
        let handler = p63_asm::assemble(
            "t",
            "    ORG $000100\n    LD A,#$01\n    ST A,$FFFF90\n    LD A,$001000\n    INC A\n    ST A,$001000\n    RTI",
        )
        .unwrap();
        for (i, b) in handler.image_at(0x100).unwrap().into_iter().enumerate() {
            m.cpu.bus.ram[0x0C_0100 + i] = b;
        }
        let s = &mut m.cpu.bus.vm.slots[1];
        s.table = 0x2100;
        s.vram = 0x10_0000;
        s.flags = vm::VMF_LIVE;
        let mut regs = vm::registers_from(&[0; vm::STATE_LEN]);
        regs.set_sp(0x8000);
        regs.set_pc(0x200);
        let mut cpu = p63_core::CpuState::default();
        cpu.registers = regs.clone();
        s.cpu = Some(cpu);
        let at = (0x2100 + spec::VMT_STATE) as usize;
        m.cpu.bus.ram[at..at + vm::STATE_LEN].copy_from_slice(&vm::state_bytes(&regs));
        let s = &mut m.cpu.bus.vm.slots[1];
        s.video.write(spec::V_IRQEN - spec::VIDEO_IO_BASE, 1);
        m.cpu.bus.ram[(0x2100 + spec::VMT_VECS + (spec::VEC_IRQ - vm::VECTORS)) as usize..][..3].copy_from_slice(&[0x00, 0x01, 0x00]);
        for _ in 0..6 {
            m.run_frame().unwrap();
        }
        assert_eq!(m.cpu.bus.vm.guest, Some(0), "slot 0 still has the CPU");
        assert!(m.cpu.bus.ram[0x0C_1000] >= 5, "slot 1 took each vblank, got {}", m.cpu.bus.ram[0x0C_1000]);
    }

    #[test]
    fn each_program_has_its_own_pad_mirror() {
        let host = format!(
            "
    LD A,#$01                           ; the host's mirror off
    ST A,$FFFFC0
{RUN_GUEST}
    LD A,$FFFFDC
    ST A,RESULT+8
DONE:
    BRA DONE"
        );
        let guest = "
    LD A,$FFFFDC
    ST A,$001000
    ST A,$FFFED1";
        let mut m = vm_machine(&host, guest);
        m.cpu.bus.keyboard.keys[usize::from(input::scan::Z)] = true;
        m.run_frame().unwrap();
        assert_eq!(m.cpu.bus.ram[0x04_1000], spec::BTN_A as u8, "a new program has the mirror on: Z is A");
        assert_eq!(host_result(&mut m, 9)[8], 0, "the host turned its own off");
    }

    #[test]
    fn video_in_use_counts_written_kb() {
        let mut s = vm::Slot::default();
        s.touch_video(spec::OAM_BASE);
        s.touch_video(spec::OAM_BASE + 0x3FF);
        s.touch_video(spec::VRAM_END);
        assert_eq!(s.vkb, 2, "the first OAM KB and the last VRAM KB");
        s.mark(3);
        assert_eq!(s.vkb, 0, "a new program starts from none");
    }

    #[test]
    fn speed_switch_reads_back() {
        let mut bus = SystemBus::default();
        assert_eq!(read(&mut bus, SYS_SWITCH), 0, "Auto");
        bus.speed_forced = Some(SPEED_LOW);
        assert_eq!(read(&mut bus, SYS_SWITCH), SWITCH_FORCED | SPEED_LOW);
        bus.speed_forced = Some(SPEED_NORMAL);
        assert_eq!(read(&mut bus, SYS_SWITCH), SWITCH_FORCED, "forced Normal is still forced");
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
