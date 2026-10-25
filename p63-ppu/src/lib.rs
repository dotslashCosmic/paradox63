//! p63-ppu: draws a Paradox63 frame from VRAM, OAM and the video registers.
//!
//! Sprites over the backdrop for now; the BG tile layers come later.

use p63_spec as spec;

pub const WIDTH: usize = spec::SCREEN_W as usize;
pub const HEIGHT: usize = spec::SCREEN_H as usize;
/// Sprites past this many on one line are dropped and set the overflow flag.
pub const SPRITES_PER_LINE: usize = 32;

const CTRL_ENABLE: u8 = 0x01;
const CTRL_SPRITES: u8 = 0x20;
pub const STAT_OVERFLOW: u8 = 0x04;
const TILE_BYTES: u32 = 32;

/// Register values the renderer needs.
#[derive(Debug, Clone, Copy, Default)]
pub struct Regs {
    pub ctrl: u8,
    /// Fade amount: 0 = full brightness, 31 = black.
    pub bright: u8,
}

/// RGB332 to 0x00RRGGBB, with the fade applied.
#[must_use]
pub fn rgb(c: u8, bright: u8) -> u32 {
    let level = 31 - u32::from(bright.min(31));
    let r = u32::from(c >> 5) * 255 / 7 * level / 31;
    let g = u32::from((c >> 2) & 7) * 255 / 7 * level / 31;
    let b = u32::from(c & 3) * 255 / 3 * level / 31;
    (r << 16) | (g << 8) | b
}

/// A decoded OAM entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sprite {
    x: i32,
    y: i32,
    tile: u32,
    palette: u8,
    flip_h: bool,
    flip_v: bool,
    /// width and height in tiles
    w: u32,
    h: u32,
}

fn sprite(oam: &[u8]) -> Option<Sprite> {
    let attr = oam[spec::SPR_ATTR as usize];
    let code = (attr >> 3) & 3;
    let (w, h) = if oam[spec::SPR_EXT as usize] & 1 == 0 {
        [(1, 1), (1, 2), (2, 2), (4, 4)][code as usize]
    } else {
        match code {
            0 => (2, 1),
            1 => (3, 3),
            _ => return None, // reserved sizes aren't drawn
        }
    };
    let y = oam[spec::SPR_Y as usize];
    if y == 255 {
        return None;
    }
    Some(Sprite {
        x: i32::from(oam[spec::SPR_X as usize]) | i32::from(oam[spec::SPR_XH as usize] & 1) << 8,
        y: i32::from(y),
        tile: u32::from(oam[spec::SPR_TILE as usize]) | u32::from(attr & 7) << 8,
        palette: oam[spec::SPR_PAL as usize],
        flip_h: attr & spec::SPR_FLIPH as u8 != 0,
        flip_v: attr & spec::SPR_FLIPV as u8 != 0,
        w,
        h,
    })
}

/// Colour index (0-15) of one sprite pixel; 0 is transparent.
/// In a 4bpp tile row the high nibble is the left pixel.
fn pixel(mem: &[u8], s: &Sprite, sx: u32, sy: u32) -> u8 {
    let (pw, ph) = (s.w * 8, s.h * 8);
    let sx = if s.flip_h { pw - 1 - sx } else { sx };
    let sy = if s.flip_v { ph - 1 - sy } else { sy };
    let tile = (s.tile + (sy / 8) * s.w + sx / 8) & 0x7FF;
    let at = spec::VRAM_PATTERN + tile * TILE_BYTES + (sy % 8) * 4 + (sx % 8) / 2;
    let byte = mem.get(at as usize).copied().unwrap_or(0);
    if sx % 2 == 0 { byte >> 4 } else { byte & 0x0F }
}

/// Draws one frame into `frame` (`WIDTH * HEIGHT` pixels of 0x00RRGGBB).
/// `mem` is the full 16 MB address space. Returns true if any line had too many sprites.
pub fn render(mem: &[u8], regs: Regs, frame: &mut [u32]) -> bool {
    let palette = |p: u8, c: u8| mem[(spec::VRAM_PALETTE + u32::from(p) * 16 + u32::from(c)) as usize];
    if regs.ctrl & CTRL_ENABLE == 0 {
        frame.fill(0);
        return false;
    }
    frame.fill(rgb(palette(0, 0), regs.bright));
    if regs.ctrl & CTRL_SPRITES == 0 {
        return false;
    }

    let oam = &mem[spec::OAM_BASE as usize..=spec::OAM_END as usize];
    let sprites: Vec<Sprite> = oam.chunks(spec::SPR_SIZE as usize).filter_map(sprite).collect();
    let mut overflow = false;
    for line in 0..HEIGHT as i32 {
        let on_line: Vec<&Sprite> =
            sprites.iter().filter(|s| (s.y..s.y + s.h as i32 * 8).contains(&line)).collect();
        overflow |= on_line.len() > SPRITES_PER_LINE;
        // lowest OAM index ends up on top
        for s in on_line.iter().take(SPRITES_PER_LINE).rev() {
            let sy = (line - s.y) as u32;
            for sx in 0..s.w * 8 {
                let x = s.x + sx as i32;
                if !(0..WIDTH as i32).contains(&x) {
                    continue;
                }
                let c = pixel(mem, s, sx, sy);
                if c != 0 {
                    frame[line as usize * WIDTH + x as usize] = rgb(palette(s.palette, c), regs.bright);
                }
            }
        }
    }
    overflow
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory() -> Vec<u8> {
        vec![0; 0x0100_0000]
    }

    fn put_sprite(mem: &mut [u8], n: u32, x: u32, y: u8, tile: u16, attr: u8, pal: u8) {
        let at = (spec::OAM_BASE + n * spec::SPR_SIZE) as usize;
        mem[at] = x as u8;
        mem[at + 1] = y;
        mem[at + 2] = tile as u8;
        mem[at + 3] = attr | (tile >> 8) as u8;
        mem[at + 4] = pal;
        mem[at + 5] = (x >> 8) as u8;
    }

    fn hide_all(mem: &mut [u8]) {
        for n in 0..spec::SPR_COUNT {
            mem[(spec::OAM_BASE + n * spec::SPR_SIZE + 1) as usize] = 255;
        }
    }

    const ON: Regs = Regs { ctrl: CTRL_ENABLE | CTRL_SPRITES, bright: 0 };

    #[test]
    fn rgb332_expands_to_full_range() {
        assert_eq!(rgb(0xFF, 0), 0x00FF_FFFF);
        assert_eq!(rgb(0xE0, 0), 0x00FF_0000);
        assert_eq!(rgb(0x03, 0), 0x0000_00FF);
        assert_eq!(rgb(0xFF, 31), 0);
    }

    #[test]
    fn solid_8x16_sprite_lands_where_oam_says() {
        let mut mem = memory();
        hide_all(&mut mem);
        let tile1 = (spec::VRAM_PATTERN + 32) as usize;
        mem[tile1..tile1 + 64].fill(0x11); // tiles 1 and 2 solid colour 1
        mem[(spec::VRAM_PALETTE + 2 * 16 + 1) as usize] = 0x1C; // palette 2, colour 1: green
        put_sprite(&mut mem, 0, 300, 50, 1, spec::SPR_SZ_816 as u8, 2);
        let mut frame = vec![0; WIDTH * HEIGHT];
        render(&mem, ON, &mut frame);
        let green = rgb(0x1C, 0);
        assert_eq!(frame[50 * WIDTH + 300], green, "top-left");
        assert_eq!(frame[65 * WIDTH + 307], green, "bottom-right of 8x16");
        assert_eq!(frame[66 * WIDTH + 300], 0, "below the sprite");
        assert_eq!(frame[50 * WIDTH + 308], 0, "right of the sprite");
    }

    #[test]
    fn nibbles_flips_and_transparency() {
        let mut mem = memory();
        hide_all(&mut mem);
        let tile = spec::VRAM_PATTERN as usize + 32;
        mem[tile] = 0x12; // row 0: pixel 0 = colour 1, pixel 1 = colour 2
        mem[(spec::VRAM_PALETTE + 1) as usize] = 0xE0;
        mem[(spec::VRAM_PALETTE + 2) as usize] = 0x03;
        put_sprite(&mut mem, 0, 10, 10, 1, 0, 0);
        put_sprite(&mut mem, 1, 30, 10, 1, spec::SPR_FLIPH as u8, 0);
        let mut frame = vec![0; WIDTH * HEIGHT];
        render(&mem, ON, &mut frame);
        assert_eq!(frame[10 * WIDTH + 10], rgb(0xE0, 0), "high nibble is the left pixel");
        assert_eq!(frame[10 * WIDTH + 11], rgb(0x03, 0));
        assert_eq!(frame[10 * WIDTH + 12], 0, "colour 0 is transparent");
        assert_eq!(frame[10 * WIDTH + 37], rgb(0xE0, 0), "flipped: pixel 0 on the right");
    }

    #[test]
    fn lower_index_on_top_and_32_per_line() {
        let mut mem = memory();
        hide_all(&mut mem);
        let tile = spec::VRAM_PATTERN as usize + 32;
        mem[tile..tile + 32].fill(0x11);
        mem[(spec::VRAM_PALETTE + 16 + 1) as usize] = 0xFF;
        mem[(spec::VRAM_PALETTE + 2 * 16 + 1) as usize] = 0x1C;
        put_sprite(&mut mem, 0, 0, 0, 1, 0, 1);
        put_sprite(&mut mem, 1, 4, 0, 1, 0, 2);
        let mut frame = vec![0; WIDTH * HEIGHT];
        assert!(!render(&mem, ON, &mut frame));
        assert_eq!(frame[5], rgb(0xFF, 0), "sprite 0 wins the overlap");
        for n in 0..33 {
            put_sprite(&mut mem, n, n * 9, 100, 1, 0, 1);
        }
        assert!(render(&mem, ON, &mut frame), "33 sprites on a line overflow");
        assert_eq!(frame[100 * WIDTH + 32 * 9], 0, "the 33rd isn't drawn");
    }

    #[test]
    fn display_off_is_black() {
        let mut mem = memory();
        mem[spec::VRAM_PALETTE as usize] = 0xFF;
        let mut frame = vec![1; WIDTH * HEIGHT];
        render(&mem, Regs { ctrl: 0, bright: 0 }, &mut frame);
        assert!(frame.iter().all(|&p| p == 0));
        render(&mem, Regs { ctrl: CTRL_ENABLE, bright: 0 }, &mut frame);
        assert!(frame.iter().all(|&p| p == 0x00FF_FFFF), "backdrop is palette 0 colour 0");
    }
}
