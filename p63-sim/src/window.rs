//! The `play` window: the screen, the PC's input devices, and the console's front
//! panel as menus (cartridge slot, power, reset, the CPU speed switch).

use std::path::{Path, PathBuf};
use std::time::Instant;

use minifb::{Key, Menu, MenuHandle, Scale, Window, WindowOptions, MENU_KEY_CTRL};
use p63_bus::{Machine, SPEED_LOW, SPEED_NORMAL, SPEED_TURBO};
use p63_spec::{RESET_PANEL, RESET_SLOT};

use super::input::{Ports, CONTROLS_TEXT};
use super::{audio, find_bios, is_source, load_bios, nvram_path, read_card, write_card, FineTimer, FrameClock, Stats};

// menu item ids
const INSERT: usize = 1;
const EJECT: usize = 2;
const RESET: usize = 3;
const POWER: usize = 4;
const QUIT: usize = 5;
const SPEED_AUTO: usize = 10;
const SPEED_IDS: [(usize, u8); 3] = [(11, SPEED_LOW), (12, SPEED_NORMAL), (13, SPEED_TURBO)];
const CONTROLS: usize = 20;
const ABOUT: usize = 21;
// + the slot, 0 or 1
const CARD_INSERT: usize = 30;
const CARD_NEW: usize = 32;
const CARD_EJECT: usize = 34;
const CARD_LABEL: usize = 36;
/// Frames a card must go unwritten before its file is saved, so a save in progress is
/// written once, at its end.
const CARD_SETTLE: u32 = 30;
const RECENT: usize = 100;
const RECENT_MAX: usize = 5;

fn mhz(speed: u8) -> &'static str {
    match speed {
        SPEED_TURBO => "14.32 MHz",
        SPEED_LOW => "3.58 MHz",
        _ => "7.16 MHz",
    }
}

/// A cartridge in the slot.
struct Slot {
    path: PathBuf,
    data: Vec<u8>,
    title: String,
    kind: &'static str,
}

impl Slot {
    fn read(path: &Path) -> Result<Self, String> {
        if is_source(path) {
            return Err(format!(
                "{} is assembly source, not a cartridge. Build one first:\n  p63-sim asm {} -c {}",
                path.display(),
                path.display(),
                path.with_extension("p63c").display()
            ));
        }
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let kind = match p63_cart::crypt::Layout::of(&data) {
            Ok(l) if l.encrypted && l.sealed => "encrypted and sealed",
            Ok(l) if l.sealed => "sealed",
            Ok(l) if l.encrypted => "encrypted",
            Ok(_) => "plain",
            Err(_) => "not a cartridge",
        };
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_ascii_uppercase()).unwrap_or_default();
        let title = p63_cart::Cart::parse(data.clone()).map(|c| c.title).ok().filter(|t| !t.is_empty()).unwrap_or(stem);
        Ok(Self { path: path.to_path_buf(), data, title, kind })
    }
}

/// The console: its BIOS, the slot, and the machine while the power is on.
struct Console {
    bios: PathBuf,
    rom: Vec<u8>,
    slot: Option<Slot>,
    machine: Option<Machine>,
    /// The speed switch: `None` lets the program pick.
    forced: Option<u8>,
    recent: Vec<PathBuf>,
    /// The memory card files in the two card slots.
    cards: [Option<PathBuf>; 2],
    /// Frames until a written card is saved to its file (0: nothing waiting).
    card_wait: [u32; 2],
}

impl Console {
    fn power_on(&mut self) -> Result<(), String> {
        let mut m = Machine::new();
        let cart = self.slot.as_ref().map(|s| s.data.clone()).unwrap_or_default();
        m.boot_with_rom(self.rom.clone(), cart)?;
        for (slot, path) in self.cards.iter().enumerate() {
            if let Some(path) = path {
                m.cpu.bus.cart.insert_card(slot, read_card(path)?);
            }
        }
        if let Ok(saved) = std::fs::read(nvram_path(&self.bios)) {
            let n = saved.len().min(p63_bus::NVRAM_LEN);
            m.cpu.bus.nvram[..n].copy_from_slice(&saved[..n]);
        }
        m.cpu.bus.speed_forced = self.forced;
        m.cpu.bus.speed = self.forced.unwrap_or(SPEED_NORMAL);
        self.machine = Some(m);
        Ok(())
    }

    /// While the BIOS is up (intro or Home) it watches the slot itself. Once a cart is
    /// running, changing it resets the console to Home, so no program sees its ROM change.
    fn swap_cart(&mut self) {
        let data = self.slot.as_ref().map(|s| s.data.clone()).unwrap_or_default();
        if let Some(mut m) = self.machine.take() {
            m.cpu.bus.cart.insert(data);
            if m.cpu.bus.boot_locked {
                panel_reset(&mut m, RESET_SLOT);
            }
            self.machine = Some(m);
        }
    }

    /// Saves card `slot` to its file now if it has changes waiting.
    fn flush_card(&mut self, slot: usize) -> Result<(), String> {
        if self.card_wait[slot] == 0 {
            return Ok(());
        }
        self.card_wait[slot] = 0;
        match (&self.cards[slot], self.machine.as_ref().and_then(|m| m.cpu.bus.cart.cards[slot].as_ref())) {
            (Some(path), Some(card)) => write_card(path, &card.data),
            _ => Ok(()),
        }
    }

    fn flush_cards(&mut self) -> Result<(), String> {
        self.flush_card(0)?;
        self.flush_card(1)
    }

    /// After a frame: a card the program wrote is saved once it has been left alone a while.
    fn settle_cards(&mut self) -> Result<(), String> {
        for slot in 0..2 {
            if let Some(card) = self.machine.as_mut().and_then(|m| m.cpu.bus.cart.cards[slot].as_mut()) {
                if std::mem::take(&mut card.dirty) {
                    self.card_wait[slot] = CARD_SETTLE;
                    continue;
                }
            }
            if self.card_wait[slot] == 1 {
                self.flush_card(slot)?;
            } else {
                self.card_wait[slot] = self.card_wait[slot].saturating_sub(1);
            }
        }
        Ok(())
    }

    /// Puts a card file in a slot, taking out whatever was there. A running console
    /// sees it go in at once.
    fn insert_card(&mut self, slot: usize, path: PathBuf) -> Result<(), String> {
        let data = read_card(&path)?;
        self.eject_card(slot)?;
        if !path.exists() {
            write_card(&path, &data)?;
        }
        if let Some(m) = &mut self.machine {
            m.cpu.bus.cart.insert_card(slot, data);
        }
        self.cards[slot] = Some(path);
        save_cards(&self.bios, &self.cards);
        Ok(())
    }

    fn eject_card(&mut self, slot: usize) -> Result<(), String> {
        self.flush_card(slot)?;
        if let Some(m) = &mut self.machine {
            m.cpu.bus.cart.eject_card(slot);
        }
        self.cards[slot] = None;
        save_cards(&self.bios, &self.cards);
        Ok(())
    }

    fn insert(&mut self, slot: Slot) {
        self.recent.retain(|p| p != &slot.path);
        self.recent.insert(0, slot.path.clone());
        self.recent.truncate(RECENT_MAX);
        save_recent(&self.bios, &self.recent);
        self.slot = Some(slot);
        self.swap_cart();
    }

    fn eject(&mut self) {
        self.slot = None;
        self.swap_cart();
    }

    fn set_speed(&mut self, forced: Option<u8>) {
        self.forced = forced;
        if let Some(m) = &mut self.machine {
            m.cpu.bus.speed_forced = forced;
        }
    }

    fn title(&self) -> String {
        let cart = self.slot.as_ref().map_or("no cartridge", |s| s.title.as_str());
        match &self.machine {
            Some(m) => format!("Paradox63 - {cart} - {}", mhz(m.cpu.bus.speed)),
            None => format!("Paradox63 - {cart} - power off"),
        }
    }

    fn about(&self) -> String {
        let cart = self.slot.as_ref().map_or("none".to_string(), |s| format!("{} ({})", s.title, s.kind));
        let speed = match (&self.machine, self.forced) {
            (None, _) => "power off".to_string(),
            (Some(m), _) => mhz(m.cpu.bus.speed).to_string(),
        };
        let bios = self.bios.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        format!(
            "Paradox63 simulator {}\nHardware spec {}\n\nBIOS: {bios} (signature verified)\nCartridge: {cart}\n\
             Memory card 1: {}\nMemory card 2: {}\nCPU: {speed}\n\n\
             The simulator is free software under the GNU GPL v3.\nThe BIOS is proprietary; see BIOS_LICENSE.",
            env!("CARGO_PKG_VERSION"),
            p63_spec::SPEC_VERSION,
            self.card_name(0),
            self.card_name(1),
        )
    }

    fn card_name(&self, slot: usize) -> String {
        self.cards[slot].as_ref().and_then(|p| p.file_name()).map_or("none".into(), |n| n.to_string_lossy().into_owned())
    }

    fn card_menu(&self) -> Menu {
        let mut cards = Menu::new("Memory Cards").expect("menu");
        for slot in 0..2 {
            if slot == 1 {
                cards.add_separator();
            }
            let n = slot + 1;
            cards.add_item(&format!("Slot {n}: {}", self.card_name(slot)), CARD_LABEL + slot).enabled(false).build();
            cards.add_item(&format!("Insert Card in Slot {n}..."), CARD_INSERT + slot).build();
            cards.add_item(&format!("New Card in Slot {n}..."), CARD_NEW + slot).build();
            cards.add_item(&format!("Eject Card {n}"), CARD_EJECT + slot).enabled(self.cards[slot].is_some()).build();
        }
        cards
    }

    fn menus(&self) -> (Menu, Menu, Menu) {
        let mut console = Menu::new("Console").expect("menu");
        console.add_item("Insert Cartridge...", INSERT).shortcut(Key::O, MENU_KEY_CTRL).build();
        if !self.recent.is_empty() {
            let mut recent = Menu::new("Recent").expect("menu");
            for (i, p) in self.recent.iter().enumerate() {
                recent.add_item(&p.display().to_string(), RECENT + i).build();
            }
            console.add_sub_menu("Recent", &recent);
        }
        console.add_item("Eject", EJECT).shortcut(Key::E, MENU_KEY_CTRL).enabled(self.slot.is_some()).build();
        console.add_separator();
        console.add_item("Reset", RESET).shortcut(Key::R, MENU_KEY_CTRL).enabled(self.machine.is_some()).build();
        let power = if self.machine.is_some() { "Power Off" } else { "Power On" };
        console.add_item(power, POWER).shortcut(Key::P, MENU_KEY_CTRL).build();
        console.add_separator();
        let mut speed = Menu::new("CPU Speed").expect("menu");
        let mark = |on: bool, label: &str| if on { format!("> {label}") } else { format!("   {label}") };
        speed.add_item(&mark(self.forced.is_none(), "Auto"), SPEED_AUTO).build();
        for (id, s) in SPEED_IDS {
            let name = match s {
                SPEED_LOW => "Low power, 3.58 MHz",
                SPEED_TURBO => "Turbo, 14.32 MHz",
                _ => "Normal, 7.16 MHz",
            };
            speed.add_item(&mark(self.forced == Some(s), name), id).build();
        }
        console.add_sub_menu("CPU Speed", &speed);
        console.add_separator();
        console.add_item("Quit", QUIT).build();
        let mut help = Menu::new("Help").expect("menu");
        help.add_item("Controls", CONTROLS).build();
        help.add_item("About Paradox63", ABOUT).build();
        (console, self.card_menu(), help)
    }
}

/// The reset line, with the front panel's reason: the BIOS skips the intro for these.
fn panel_reset(m: &mut Machine, reason: u32) {
    m.cpu.bus.reset_reason = reason as u8;
    m.reset();
}

/// Which card files were in the slots, kept beside the BIOS: one line per slot.
fn cards_path(bios: &Path) -> PathBuf {
    bios.with_extension("cards")
}

/// The cards from last time. The very first time, a new card goes in slot 1.
fn load_cards(bios: &Path) -> [Option<PathBuf>; 2] {
    match std::fs::read_to_string(cards_path(bios)) {
        Ok(text) => {
            let mut lines = text.lines().map(|l| (!l.is_empty()).then(|| PathBuf::from(l)));
            [lines.next().flatten(), lines.next().flatten()]
        }
        Err(_) => [Some(bios.with_file_name("card1.p63m")), None],
    }
}

fn save_cards(bios: &Path, cards: &[Option<PathBuf>; 2]) {
    let line = |p: &Option<PathBuf>| p.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
    // losing the list isn't worth stopping for
    let _ = std::fs::write(cards_path(bios), format!("{}\n{}\n", line(&cards[0]), line(&cards[1])));
}

fn card_dialog(cards: &[Option<PathBuf>; 2], bios: &Path) -> rfd::FileDialog {
    let dir = cards.iter().flatten().find_map(|p| p.parent()).or_else(|| bios.parent());
    let dialog = rfd::FileDialog::new().add_filter("Paradox63 memory card", &["p63m"]);
    match dir {
        Some(d) => dialog.set_directory(d),
        None => dialog,
    }
}

fn recent_path(bios: &Path) -> PathBuf {
    bios.with_extension("recent")
}

fn load_recent(bios: &Path) -> Vec<PathBuf> {
    std::fs::read_to_string(recent_path(bios))
        .map(|t| t.lines().filter(|l| !l.is_empty()).map(PathBuf::from).take(RECENT_MAX).collect())
        .unwrap_or_default()
}

fn save_recent(bios: &Path, recent: &[PathBuf]) {
    let text: String = recent.iter().map(|p| format!("{}\n", p.display())).collect();
    // losing the list isn't worth stopping for
    let _ = std::fs::write(recent_path(bios), text);
}

fn message(title: &str, text: &str, level: rfd::MessageLevel) {
    rfd::MessageDialog::new().set_title(title).set_description(text).set_level(level).show();
}

pub(super) fn play(file: Option<&Path>, bios: Option<&Path>, stats: bool, cards: [Option<PathBuf>; 2]) -> Result<(), String> {
    let bios = find_bios(bios)?;
    let rom = load_bios(&bios)?;
    let recent = load_recent(&bios);
    let last = load_cards(&bios);
    let mut console = Console { bios, rom, slot: None, machine: None, forced: None, recent, cards: [None, None], card_wait: [0; 2] };
    for (slot, path) in cards.into_iter().zip(last).enumerate().filter_map(|(i, (given, last))| Some((i, given.or(last)?))) {
        console.insert_card(slot, path)?;
    }
    if let Some(path) = file {
        console.insert(Slot::read(path)?);
    }
    console.power_on()?;

    let (w, h) = (p63_ppu::WIDTH, p63_ppu::HEIGHT);
    let options = WindowOptions { scale: Scale::X2, resize: true, ..WindowOptions::default() };
    let mut window = Window::new(&console.title(), w, h, options).map_err(|e| e.to_string())?;
    let mut handles: Vec<MenuHandle> = Vec::new();
    let mut rebuild = true;
    let mut shown_title = console.title();
    let black = vec![0u32; w * h];
    let speaker = audio::Speaker::open().map_err(|e| eprintln!("no sound: {e}")).ok();
    // frames run on their own steady clock; the sound follows it (see audio.rs)
    window.set_target_fps(0);
    let _timer = FineTimer::new();
    let mut clock = FrameClock::new();
    let mut timing = Stats::default();
    let mut ports = Ports::new();
    while window.is_open() {
        if rebuild {
            for h in handles.drain(..) {
                window.remove_menu(h);
            }
            let (c, cards, help) = console.menus();
            handles.push(window.add_menu(&c));
            handles.push(window.add_menu(&cards));
            handles.push(window.add_menu(&help));
            rebuild = false;
        }
        if let Some(id) = window.is_menu_pressed() {
            rebuild = true;
            ports.free_mouse(&mut window);
            match id {
                INSERT => {
                    let mut dialog = rfd::FileDialog::new().add_filter("Paradox63 cartridge", &["p63c"]);
                    if let Some(dir) = console.slot.as_ref().and_then(|s| s.path.parent()) {
                        dialog = dialog.set_directory(dir);
                    }
                    if let Some(path) = dialog.pick_file() {
                        match Slot::read(&path) {
                            Ok(slot) => console.insert(slot),
                            Err(e) => message("Insert Cartridge", &e, rfd::MessageLevel::Error),
                        }
                    }
                }
                EJECT => console.eject(),
                RESET => {
                    if let Some(m) = &mut console.machine {
                        panel_reset(m, RESET_PANEL);
                    }
                }
                POWER => {
                    console.flush_cards()?;
                    if console.machine.take().is_none() {
                        console.power_on()?;
                    }
                }
                QUIT => break,
                id if id == CARD_INSERT || id == CARD_INSERT + 1 => {
                    if let Some(path) = card_dialog(&console.cards, &console.bios).pick_file() {
                        if let Err(e) = console.insert_card(id - CARD_INSERT, path) {
                            message("Insert Memory Card", &e, rfd::MessageLevel::Error);
                        }
                    }
                }
                id if id == CARD_NEW || id == CARD_NEW + 1 => {
                    let dialog = card_dialog(&console.cards, &console.bios).set_file_name("card.p63m");
                    if let Some(path) = dialog.save_file() {
                        let made = write_card(&path, &p63_cart::card::blank()).and_then(|()| console.insert_card(id - CARD_NEW, path));
                        if let Err(e) = made {
                            message("New Memory Card", &e, rfd::MessageLevel::Error);
                        }
                    }
                }
                id if id == CARD_EJECT || id == CARD_EJECT + 1 => console.eject_card(id - CARD_EJECT)?,
                SPEED_AUTO => console.set_speed(None),
                CONTROLS => message("Controls", CONTROLS_TEXT, rfd::MessageLevel::Info),
                ABOUT => message("About Paradox63", &console.about(), rfd::MessageLevel::Info),
                id if (RECENT..RECENT + RECENT_MAX).contains(&id) => {
                    if let Some(path) = console.recent.get(id - RECENT).cloned() {
                        match Slot::read(&path) {
                            Ok(slot) => console.insert(slot),
                            Err(e) => message("Insert Cartridge", &e, rfd::MessageLevel::Error),
                        }
                    }
                }
                id => {
                    if let Some(&(_, s)) = SPEED_IDS.iter().find(|(i, _)| *i == id) {
                        console.set_speed(Some(s));
                    }
                }
            }
        }

        let t0 = Instant::now();
        clock.wait();
        let t1 = Instant::now();
        let mut crashed = None;
        if let Some(m) = &mut console.machine {
            let bus = &mut m.cpu.bus;
            ports.update(&mut window, &mut bus.keyboard, &mut bus.pointer);
            if let Err(e) = m.run_frame() {
                crashed = Some(format!("The console stopped at frame {}, ${:06X}:\n{e}", m.frames, m.cpu.registers.pc()));
            }
            if m.cpu.bus.nvram_dirty {
                m.cpu.bus.nvram_dirty = false;
                let path = nvram_path(&console.bios);
                std::fs::write(&path, m.cpu.bus.nvram).map_err(|e| format!("{}: {e}", path.display()))?;
            }
            if let Some(speaker) = &speaker {
                speaker.push(&m.audio);
            }
        }
        console.settle_cards()?;
        if let Some(text) = crashed {
            console.flush_cards()?;
            console.machine = None;
            rebuild = true;
            ports.free_mouse(&mut window);
            message("Paradox63", &text, rfd::MessageLevel::Error);
        }
        let t2 = Instant::now();
        let picture = console.machine.as_ref().map_or(&black, |m| &m.frame);
        window.update_with_buffer(picture, w, h).map_err(|e| e.to_string())?;
        let mouse = if ports.mouse_held() { "Ctrl+Alt frees the mouse" } else { "click to use the mouse" };
        let title = format!("{} - {mouse}", console.title());
        if title != shown_title {
            window.set_title(&title);
            shown_title = title;
        }
        if stats {
            let queued = speaker.as_ref().map_or(0, audio::Speaker::queued);
            timing.frame(t1 - t0, t2 - t1, t2.elapsed(), queued);
        }
    }
    console.flush_cards()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The console with the release BIOS and pong in the slot, or `None` without a BIOS.
    fn console(boot_to_cart: bool) -> Option<Console> {
        let bios = PathBuf::from("../target/release/bios.bin");
        let rom = load_bios(&bios).ok()?;
        let program = p63_asm::assemble_file("../games/pong.p63").ok()?;
        let data = super::super::make_cart(&program, Path::new("pong.p63")).ok()?;
        let slot = Slot { path: PathBuf::from("pong.p63c"), data, title: "PONG".into(), kind: "plain" };
        let mut c = Console { bios, rom, slot: Some(slot), machine: None, forced: None, recent: Vec::new(), cards: [None, None], card_wait: [0; 2] };
        c.power_on().ok()?;
        let m = c.machine.as_mut()?;
        m.cpu.bus.nvram[..2].copy_from_slice(&[0x63, u8::from(boot_to_cart)]);
        Some(c)
    }

    /// Frames until `done` holds, up to `limit`.
    fn frames_until(c: &mut Console, limit: u32, done: impl Fn(&mut Machine) -> bool) -> Option<u32> {
        let m = c.machine.as_mut()?;
        (1..=limit).find(|_| {
            m.run_frame().unwrap();
            done(m)
        })
    }

    fn running(m: &mut Machine) -> bool {
        m.cpu.bus.boot_locked
    }

    fn home(m: &mut Machine) -> bool {
        m.cpu.bus.peek(p63_core::Addr::new(p63_spec::V_CTRL)) == 0x39
    }

    fn cart_state(m: &mut Machine) -> u8 {
        m.cpu.bus.peek(p63_core::Addr::new(0x90))
    }

    /// The text on BG0's row `row`, as the screen shows it.
    fn screen_row(m: &mut Machine, row: u32) -> String {
        (0..40).map(|c| char::from(m.cpu.bus.peek(p63_core::Addr::new(p63_spec::VRAM_BG0 + (row * 40 + c) * 2)))).collect()
    }

    /// Holds `button` for one frame, then lets it go for `settle` frames.
    fn press(m: &mut Machine, button: u32, settle: u32) {
        m.cpu.bus.keyboard.pad[0] = button as u8;
        m.run_frame().unwrap();
        m.cpu.bus.keyboard.pad[0] = 0;
        for _ in 0..settle {
            m.run_frame().unwrap();
        }
    }

    #[test]
    fn home_lists_a_cards_saves_and_copies_one() {
        let Some(mut c) = console(false) else { return };
        let mut card = p63_cart::card::blank();
        let mut save = vec![0u8; 300];
        save[..12].copy_from_slice(b"PlayerName, day 3");
        save[p63_spec::SH_ICON as usize..p63_spec::SH_PAL as usize].fill(0x12);
        save[p63_spec::SH_PAL as usize + 1] = 0xE0;
        save[p63_spec::SH_PAL as usize + 2] = 0xFC;
        p63_cart::card::write_save(&mut card, "GAMETITLE", "SLOT1", &save).unwrap();
        for n in 2..=9 {
            save[..12].copy_from_slice(format!("Save {n}      ").as_bytes());
            p63_cart::card::write_save(&mut card, "PONG", &format!("S{n}"), &save).unwrap();
        }
        let m = c.machine.as_mut().unwrap();
        m.cpu.bus.cart.insert_card(0, card);
        m.cpu.bus.cart.insert_card(1, p63_cart::card::blank());
        frames_until(&mut c, 2000, home).expect("Home comes up");
        let m = c.machine.as_mut().unwrap();
        for _ in 0..60 {
            m.run_frame().unwrap();
        }
        press(m, p63_spec::BTN_RIGHT, 40);
        let shots = std::env::var("P63_SHOTS").ok();
        if let Some(dir) = &shots {
            super::super::save_bmp(&Path::new(dir).join("memory.bmp"), &m.frame, p63_ppu::WIDTH, p63_ppu::HEIGHT).unwrap();
        }
        assert!(screen_row(m, 7).contains("CARD 1") && screen_row(m, 7).contains("118 BLOCKS FREE"), "{}", screen_row(m, 7));
        assert!(screen_row(m, 11).contains("PlayerName, DAY 3"), "{}", screen_row(m, 11));
        assert!(screen_row(m, 12).contains("GAMETITLE") && screen_row(m, 12).contains("1 BLOCK"), "{}", screen_row(m, 12));
        press(m, p63_spec::BTN_A, 5);
        assert!(screen_row(m, 26).contains("A: COPY TO CARD 2"), "{}", screen_row(m, 26));
        press(m, p63_spec::BTN_A, 5);
        assert!(screen_row(m, 26).contains("COPIED TO CARD 2"), "{}", screen_row(m, 26));
        let copy = p63_cart::card::saves(&m.cpu.bus.cart.cards[1].as_ref().unwrap().data).unwrap();
        assert_eq!(copy.len(), 1);
        assert_eq!((copy[0].game.as_str(), copy[0].name.as_str(), copy[0].title.as_str()), ("GAMETITLE", "SLOT1", "PlayerName, day 3"));
        press(m, p63_spec::BTN_A, 5);
        press(m, p63_spec::BTN_A, 5);
        assert!(screen_row(m, 26).contains("IT IS ALREADY ON CARD 2"), "{}", screen_row(m, 26));
        press(m, p63_spec::BTN_SELECT, 5);
        assert!(screen_row(m, 11).contains("PlayerName, DAY 3"), "card 2 lists the copy: {}", screen_row(m, 11));
        if let Some(dir) = &shots {
            super::super::save_bmp(&Path::new(dir).join("memory2.bmp"), &m.frame, p63_ppu::WIDTH, p63_ppu::HEIGHT).unwrap();
        }
        m.cpu.bus.cart.eject_card(1);
        for _ in 0..5 {
            m.run_frame().unwrap();
        }
        assert!(screen_row(m, 12).contains("NO MEMORY CARD IN THIS SLOT"), "taking it out shows at once: {}", screen_row(m, 12));
        press(m, p63_spec::BTN_SELECT, 5);
        for _ in 0..8 {
            press(m, p63_spec::BTN_DOWN, 3);
        }
        assert!(screen_row(m, 23).contains("SAVE 9"), "the list scrolls to the ninth: {}", screen_row(m, 23));
        assert!(screen_row(m, 11).contains("SAVE 3"), "{}", screen_row(m, 11));
        if let Some(dir) = &shots {
            super::super::save_bmp(&Path::new(dir).join("memory3.bmp"), &m.frame, p63_ppu::WIDTH, p63_ppu::HEIGHT).unwrap();
        }
    }

    #[test]
    fn intro_only_at_power_on() {
        let Some(mut c) = console(true) else { return };
        let cold = frames_until(&mut c, 2000, running).expect("the cart starts");
        panel_reset(c.machine.as_mut().unwrap(), RESET_PANEL);
        let warm = frames_until(&mut c, 2000, running).expect("the cart starts again");
        assert!(cold > 120, "the intro plays at power-on ({cold} frames)");
        assert!(warm < 30, "the reset button skips it ({warm} frames)");
    }

    #[test]
    fn eject_goes_home_and_insert_needs_no_reset() {
        let Some(mut c) = console(true) else { return };
        frames_until(&mut c, 2000, running).expect("the cart starts");
        let slot = c.slot.take();
        c.swap_cart();
        let to_home = frames_until(&mut c, 2000, home).expect("Home comes up");
        assert!(to_home < 30, "no intro on the way ({to_home} frames)");
        assert_eq!(cart_state(c.machine.as_mut().unwrap()), p63_spec::ERR_NOCART as u8);
        let frames = c.machine.as_ref().unwrap().frames;
        c.slot = slot;
        c.swap_cart();
        let seen = frames_until(&mut c, 10, |m| cart_state(m) == 0).expect("Home sees the cart");
        let m = c.machine.as_ref().unwrap();
        assert!(!m.cpu.bus.boot_locked && m.frames == frames + u64::from(seen), "no reset, still on Home");
    }
}
