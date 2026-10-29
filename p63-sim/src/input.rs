//! The PC's keyboard, mouse and game controllers, plugged into the console's ports.

use minifb::{Key, MouseButton, MouseMode, Window};
use p63_bus::input::{scan, Keyboard, Pointer};
use p63_spec as spec;

/// PC keys and the P63 scan codes they press.
const KEYS: &[(Key, u8)] = &[
    (Key::Escape, scan::ESC),
    (Key::Key1, 0x02), (Key::Key2, 0x03), (Key::Key3, 0x04), (Key::Key4, 0x05), (Key::Key5, 0x06),
    (Key::Key6, 0x07), (Key::Key7, 0x08), (Key::Key8, 0x09), (Key::Key9, 0x0A), (Key::Key0, 0x0B),
    (Key::Minus, 0x0C), (Key::Equal, 0x0D), (Key::Backspace, scan::BACKSPACE), (Key::Tab, scan::TAB),
    (Key::Q, 0x10), (Key::W, 0x11), (Key::E, 0x12), (Key::R, 0x13), (Key::T, 0x14),
    (Key::Y, 0x15), (Key::U, 0x16), (Key::I, 0x17), (Key::O, 0x18), (Key::P, 0x19),
    (Key::LeftBracket, 0x1A), (Key::RightBracket, 0x1B), (Key::Enter, scan::ENTER), (Key::NumPadEnter, scan::ENTER),
    (Key::LeftCtrl, scan::LCTRL),
    (Key::A, 0x1E), (Key::S, 0x1F), (Key::D, 0x20), (Key::F, 0x21), (Key::G, 0x22),
    (Key::H, 0x23), (Key::J, 0x24), (Key::K, 0x25), (Key::L, 0x26),
    (Key::Semicolon, 0x27), (Key::Apostrophe, 0x28), (Key::Backquote, 0x29), (Key::LeftShift, scan::LSHIFT),
    (Key::Backslash, 0x2B),
    (Key::Z, scan::Z), (Key::X, scan::X), (Key::C, 0x2E), (Key::V, 0x2F), (Key::B, 0x30),
    (Key::N, 0x31), (Key::M, 0x32), (Key::Comma, 0x33), (Key::Period, 0x34), (Key::Slash, 0x35),
    (Key::RightShift, scan::RSHIFT), (Key::LeftAlt, scan::ALT), (Key::Space, 0x39), (Key::CapsLock, scan::CAPS),
    (Key::F1, 0x3B), (Key::F2, 0x3C), (Key::F3, 0x3D), (Key::F4, scan::F4), (Key::F5, 0x3F),
    (Key::F6, 0x40), (Key::F7, 0x41), (Key::F8, 0x42), (Key::F9, 0x43), (Key::F10, 0x44),
    (Key::F11, scan::F11), (Key::F12, scan::F12),
    (Key::Home, scan::HOME), (Key::Up, scan::UP), (Key::PageUp, scan::PGUP), (Key::Left, scan::LEFT),
    (Key::Right, scan::RIGHT), (Key::End, scan::END), (Key::Down, scan::DOWN), (Key::PageDown, scan::PGDN),
    (Key::Insert, scan::INSERT), (Key::Delete, scan::DELETE), (Key::NumLock, scan::NUMLOCK),
    (Key::ScrollLock, scan::SCROLL), (Key::RightCtrl, scan::RCTRL), (Key::RightAlt, scan::ALTGR),
    (Key::NumPadSlash, 0x35), (Key::NumPadAsterisk, 0x09), (Key::NumPadMinus, 0x0C), (Key::NumPadPlus, 0x0D),
];

/// Windows keeps Ctrl+Alt+Del, Alt+Tab and Ctrl+Esc for itself, so the window never
/// sees them. These stand in: (PC keys held, P63 keys pressed instead).
const STAND_INS: [(&[u8], &[u8]); 3] = [
    (&[scan::LCTRL, scan::ALT, scan::END], &[scan::LCTRL, scan::ALT, scan::DELETE]),
    (&[scan::LCTRL, scan::TAB], &[scan::ALT, scan::TAB]),
    (&[scan::ALT, scan::HOME], &[scan::LCTRL, scan::ESC]),
];

pub const CONTROLS_TEXT: &str = "\
Gamepads 1 and 2: game controllers
Keyboard: the PC keyboard
  Arrows = D-pad, Z/X = A/B, Enter = Start, Backspace = Select
Mouse: click in the window to use it, Ctrl+Alt to let go

Ctrl+O insert, Ctrl+E eject, Ctrl+R reset, Ctrl+P power";

/// Gamepad buttons, by controller button.
const BUTTONS: [(gilrs::Button, u8); 8] = [
    (gilrs::Button::South, spec::BTN_A as u8),
    (gilrs::Button::East, spec::BTN_B as u8),
    (gilrs::Button::Start, spec::BTN_START as u8),
    (gilrs::Button::Select, spec::BTN_SELECT as u8),
    (gilrs::Button::DPadUp, spec::BTN_UP as u8),
    (gilrs::Button::DPadDown, spec::BTN_DOWN as u8),
    (gilrs::Button::DPadLeft, spec::BTN_LEFT as u8),
    (gilrs::Button::DPadRight, spec::BTN_RIGHT as u8),
];

/// Everything the PC plugs into the console.
pub struct Ports {
    pads: Option<gilrs::Gilrs>,
    mouse: Mouse,
}

impl Ports {
    pub fn new() -> Self {
        let pads = gilrs::Gilrs::new().map_err(|e| eprintln!("no game controllers: {e}")).ok();
        Self { pads, mouse: Mouse::default() }
    }

    /// True while the mouse is plugged in (the PC cursor is hidden and held).
    pub fn mouse_held(&self) -> bool {
        self.mouse.held
    }

    /// Gives the PC cursor back, for a menu or a dialog.
    pub fn free_mouse(&mut self, window: &mut Window) {
        if self.mouse.held {
            self.mouse.release(window);
        }
    }

    /// Reads the PC's devices into the console's ports, once a frame.
    pub fn update(&mut self, window: &mut Window, kb: &mut Keyboard, pointer: &mut Pointer) {
        let mut held = window.get_keys();
        if window.is_active() {
            held.extend(alt_keys());
        }
        let mut keys = [false; 128];
        for &(k, s) in KEYS {
            if held.contains(&k) {
                keys[usize::from(s)] = true;
            }
        }
        for (pc, p63) in STAND_INS {
            // the same modifiers exactly, so Ctrl+Alt+Tab isn't taken for Ctrl+Tab
            let same = |s: u8| keys[usize::from(s)] == pc.contains(&s);
            if pc.iter().all(|&s| keys[usize::from(s)]) && same(scan::LCTRL) && same(scan::ALT) {
                for &s in pc {
                    keys[usize::from(s)] = false;
                }
                for &s in p63 {
                    keys[usize::from(s)] = true;
                }
            }
        }
        kb.keys = keys;
        let mut ports = spec::PORT_KEYBOARD as u8;
        kb.pad = [0; 2];
        if let Some(gilrs) = &mut self.pads {
            while gilrs.next_event().is_some() {}
            let mut ids: Vec<_> = gilrs.gamepads().filter(|(_, g)| g.is_connected()).map(|(id, _)| id).collect();
            ids.sort_by_key(|&id| usize::from(id));
            for (port, id) in ids.into_iter().take(2).enumerate() {
                kb.pad[port] = pad_buttons(&gilrs.gamepad(id));
                ports |= [spec::PORT_PAD1, spec::PORT_PAD2][port] as u8;
            }
        }
        self.mouse.update(window, &held, pointer);
        if self.mouse.held {
            ports |= spec::PORT_POINTER as u8;
        }
        kb.ports = ports;
    }
}

fn pad_buttons(pad: &gilrs::Gamepad) -> u8 {
    let mut buttons = BUTTONS.iter().filter(|(b, _)| pad.is_pressed(*b)).fold(0, |a, (_, bit)| a | bit);
    let (x, y) = (pad.value(gilrs::Axis::LeftStickX), pad.value(gilrs::Axis::LeftStickY));
    for (on, bit) in [(x < -0.5, spec::BTN_LEFT), (x > 0.5, spec::BTN_RIGHT), (y > 0.5, spec::BTN_UP), (y < -0.5, spec::BTN_DOWN)] {
        if on {
            buttons |= bit as u8;
        }
    }
    buttons
}

/// The PC mouse as the console's pointer. A click in the window plugs it in: the PC
/// cursor hides and is held in the middle of the window, and its movement goes to the
/// console. Ctrl+Alt (pressed and let go on their own) or leaving the window unplugs it.
#[derive(Default)]
struct Mouse {
    held: bool,
    was_down: bool,
    /// Ctrl+Alt are down with nothing else so far.
    release_armed: bool,
    /// Movement smaller than a console pixel, kept for next time.
    rest: (f32, f32),
}

impl Mouse {
    fn update(&mut self, window: &mut Window, keys: &[Key], pointer: &mut Pointer) {
        let down = window.get_mouse_down(MouseButton::Left);
        if !self.held {
            pointer.buttons = 0;
            let inside = window.get_mouse_pos(MouseMode::Discard).is_some();
            // plug in on the click's release, so it doesn't reach the console
            if self.was_down && !down && inside && window.is_active() {
                self.grab(window);
            }
            self.was_down = down;
            return;
        }
        let ctrl = keys.iter().any(|k| matches!(k, Key::LeftCtrl | Key::RightCtrl));
        let alt = keys.iter().any(|k| matches!(k, Key::LeftAlt | Key::RightAlt));
        let only_those = keys.iter().all(|k| matches!(k, Key::LeftCtrl | Key::RightCtrl | Key::LeftAlt | Key::RightAlt));
        if !only_those {
            self.release_armed = false;
        } else if ctrl && alt {
            self.release_armed = true;
        } else if keys.is_empty() && self.release_armed {
            self.release(window);
            return;
        }
        if !window.is_active() {
            self.release(window);
            return;
        }
        let (dx, dy) = moved(window);
        let (w, _) = window.get_size();
        let scale = w as f32 / p63_ppu::WIDTH as f32;
        let (fx, fy) = (dx as f32 / scale + self.rest.0, dy as f32 / scale + self.rest.1);
        let (px, py) = (fx.trunc(), fy.trunc());
        self.rest = (fx - px, fy - py);
        let wheel = window.get_scroll_wheel().map_or(0, |(_, y)| i32::from(y > 0.0) - i32::from(y < 0.0));
        pointer.moved(px as i32, py as i32, wheel);
        pointer.buttons = (u8::from(down) * spec::MSB_LEFT as u8)
            | (u8::from(window.get_mouse_down(MouseButton::Right)) * spec::MSB_RIGHT as u8)
            | (u8::from(window.get_mouse_down(MouseButton::Middle)) * spec::MSB_MIDDLE as u8);
    }

    fn grab(&mut self, window: &mut Window) {
        self.held = true;
        self.release_armed = false;
        self.rest = (0.0, 0.0);
        window.set_cursor_visibility(false);
        moved(window);
    }

    fn release(&mut self, window: &mut Window) {
        self.held = false;
        self.was_down = false;
        window.set_cursor_visibility(true);
    }
}

/// The Alt keys held. minifb never reports Alt on Windows, so ask Windows directly.
#[cfg(windows)]
fn alt_keys() -> Vec<Key> {
    #[link(name = "user32")]
    extern "system" {
        fn GetAsyncKeyState(key: i32) -> i16;
    }
    const VK_LMENU: i32 = 0xA4;
    const VK_RMENU: i32 = 0xA5;
    // SAFETY: a plain query with no pointers
    let down = |vk| unsafe { GetAsyncKeyState(vk) } < 0;
    [(VK_LMENU, Key::LeftAlt), (VK_RMENU, Key::RightAlt)].into_iter().filter(|&(vk, _)| down(vk)).map(|(_, k)| k).collect()
}

#[cfg(not(windows))]
fn alt_keys() -> Vec<Key> {
    Vec::new()
}

/// How far the PC mouse moved since the last look, in screen pixels, putting it back in
/// the middle of the window.
#[cfg(windows)]
fn moved(window: &Window) -> (i32, i32) {
    #[repr(C)]
    #[derive(Default)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[link(name = "user32")]
    extern "system" {
        fn GetCursorPos(p: *mut Point) -> i32;
        fn SetCursorPos(x: i32, y: i32) -> i32;
        fn ClientToScreen(window: *mut std::ffi::c_void, p: *mut Point) -> i32;
    }
    let (w, h) = window.get_size();
    let mut middle = Point { x: w as i32 / 2, y: h as i32 / 2 };
    let mut now = Point::default();
    // SAFETY: the handle is this live window's; the points are valid locals
    unsafe {
        ClientToScreen(window.get_window_handle(), &mut middle);
        GetCursorPos(&mut now);
        SetCursorPos(middle.x, middle.y);
    }
    (now.x - middle.x, now.y - middle.y)
}

/// Elsewhere the cursor can't be held, so movement is followed while it's in the window.
#[cfg(not(windows))]
fn moved(window: &Window) -> (i32, i32) {
    use std::sync::atomic::{AtomicI64, Ordering};
    static LAST: AtomicI64 = AtomicI64::new(i64::MIN);
    let Some((x, y)) = window.get_unscaled_mouse_pos(MouseMode::Clamp) else { return (0, 0) };
    let now = (i64::from(x as i32) << 32) | i64::from(y as i32 as u32);
    let last = LAST.swap(now, Ordering::Relaxed);
    if last == i64::MIN {
        return (0, 0);
    }
    (x as i32 - (last >> 32) as i32, y as i32 - last as i32)
}
