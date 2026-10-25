//! p63-ppu: draws a Paradox63 frame from VRAM, OAM and the video registers.
//!
//! Three BG tile layers and the sprites, composited per pixel.

use p63_spec as spec;

pub const WIDTH: usize = spec::SCREEN_W as usize;
pub const HEIGHT: usize = spec::SCREEN_H as usize;
/// Sprites past this many on one line are dropped and set the overflow flag.
pub const SPRITES_PER_LINE: usize = 32;

const CTRL_ENABLE: u8 = 0x01;
const CTRL_BG0: u8 = 0x08;
const CTRL_BG1: u8 = 0x10;
const CTRL_SPRITES: u8 = 0x20;
pub const STAT_OVERFLOW: u8 = 0x04;
const TILE_BYTES: u32 = 32;
/// Tiles per nametable row.
const MAP_W: u32 = WIDTH as u32 / 8;

/// Register values the renderer needs.
#[derive(Debug, Clone, Copy, Default)]
pub struct Regs {
    pub ctrl: u8,
    /// Fade amount: 0 = full brightness, 31 = black.
    pub bright: u8,
    pub scroll_x0: u16,
    pub scroll_y0: u16,
    pub scroll_x1: u16,
    pub scroll_y1: u16,
    /// Pattern bank for the BG layers (bits 3-0).
    pub bank: u8,
    /// BG palette groups: bits 3-0 BG0, bits 7-4 BG1.
    pub palgrp: u8,
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
    /// drawn behind BG0 priority tiles
    low: bool,
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
        low: attr & spec::SPR_PRIO as u8 != 0,
        w,
        h,
    })
}

/// Colour index (0-15) at (x, y) inside an 8x8 tile.
/// In a 4bpp tile row the high nibble is the left pixel.
fn tile_pixel(mem: &[u8], tile: u32, x: u32, y: u32) -> u8 {
    let at = spec::VRAM_PATTERN + tile * TILE_BYTES + y * 4 + x / 2;
    let byte = mem.get(at as usize).copied().unwrap_or(0);
    if x % 2 == 0 { byte >> 4 } else { byte & 0x0F }
}

/// Colour index (0-15) of one sprite pixel; 0 is transparent.
fn pixel(mem: &[u8], s: &Sprite, sx: u32, sy: u32) -> u8 {
    let (pw, ph) = (s.w * 8, s.h * 8);
    let sx = if s.flip_h { pw - 1 - sx } else { sx };
    let sy = if s.flip_v { ph - 1 - sy } else { sy };
    let tile = (s.tile + (sy / 8) * s.w + sx / 8) & 0x7FF;
    tile_pixel(mem, tile, sx % 8, sy % 8)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bg {
    Bg0,
    Bg1,
    Bg2,
}

/// One BG pixel at screen (x, y): colour index, global palette and priority bit.
fn bg_pixel(mem: &[u8], regs: &Regs, bg: Bg, x: u32, y: u32) -> (u8, u8, bool) {
    let (map, scroll_x, scroll_y) = match bg {
        Bg::Bg0 => (spec::VRAM_BG0, regs.scroll_x0, regs.scroll_y0),
        Bg::Bg1 => (spec::VRAM_BG1, regs.scroll_x1, regs.scroll_y1),
        Bg::Bg2 => (spec::VRAM_BG2, 0, 0),
    };
    let x = (x + u32::from(scroll_x)) % WIDTH as u32;
    let y = (y + u32::from(scroll_y)) % HEIGHT as u32;
    let entry = (map + ((y / 8) * MAP_W + x / 8) * 2) as usize;
    let (index, attr) = (mem[entry], mem[entry + 1]);
    let (palette, flip_v, flip_h, priority) = match bg {
        Bg::Bg0 => ((regs.palgrp & 15) * 16 + ((attr >> 3) & 15), attr & 4 != 0, attr & 2 != 0, attr & 1 != 0),
        Bg::Bg1 => ((regs.palgrp >> 4) * 16 + ((attr >> 2) & 15), attr & 2 != 0, attr & 1 != 0, false),
        Bg::Bg2 => (0, false, false, false),
    };
    let tx = if flip_h { 7 - x % 8 } else { x % 8 };
    let ty = if flip_v { 7 - y % 8 } else { y % 8 };
    let tile = u32::from(regs.bank & 15) * 256 + u32::from(index);
    (tile_pixel(mem, tile, tx, ty), palette, priority)
}

/// Draws one frame into `frame` (`WIDTH * HEIGHT` pixels of 0x00RRGGBB).
/// `mem` is the full 16 MB address space. Returns true if any line had too many sprites.
pub fn render(mem: &[u8], regs: Regs, frame: &mut [u32]) -> bool {
    let palette = |p: u8, c: u8| mem[(spec::VRAM_PALETTE + u32::from(p) * 16 + u32::from(c)) as usize];
    if regs.ctrl & CTRL_ENABLE == 0 {
        frame.fill(0);
        return false;
    }
    let sprites: Vec<Sprite> = if regs.ctrl & CTRL_SPRITES == 0 {
        Vec::new()
    } else {
        let oam = &mem[spec::OAM_BASE as usize..=spec::OAM_END as usize];
        oam.chunks(spec::SPR_SIZE as usize).filter_map(sprite).collect()
    };

    let mut overflow = false;
    // sprite line buffers as RGB332, split by priority
    let mut low = [None::<u8>; WIDTH];
    let mut high = [None::<u8>; WIDTH];
    for line in 0..HEIGHT as i32 {
        low.fill(None);
        high.fill(None);
        let on_line: Vec<&Sprite> =
            sprites.iter().filter(|s| (s.y..s.y + s.h as i32 * 8).contains(&line)).collect();
        overflow |= on_line.len() > SPRITES_PER_LINE;
        // lowest OAM index ends up on top
        for s in on_line.iter().take(SPRITES_PER_LINE).rev() {
            let buf = if s.low { &mut low } else { &mut high };
            let sy = (line - s.y) as u32;
            for sx in 0..s.w * 8 {
                let x = s.x + sx as i32;
                if !(0..WIDTH as i32).contains(&x) {
                    continue;
                }
                let c = pixel(mem, s, sx, sy);
                if c != 0 {
                    buf[x as usize] = Some(palette(s.palette, c));
                }
            }
        }

        let y = line as u32;
        for x in 0..WIDTH {
            let (c, p, _) = bg_pixel(mem, &regs, Bg::Bg2, x as u32, y);
            let mut out = palette(p, c);
            if regs.ctrl & CTRL_BG1 != 0 {
                let (c, p, _) = bg_pixel(mem, &regs, Bg::Bg1, x as u32, y);
                if c != 0 {
                    out = palette(p, c);
                }
            }
            let bg0 = if regs.ctrl & CTRL_BG0 == 0 {
                None
            } else {
                let (c, p, prio) = bg_pixel(mem, &regs, Bg::Bg0, x as u32, y);
                (c != 0).then(|| (palette(p, c), prio))
            };
            if let Some((c, false)) = bg0 {
                out = c;
            }
            if let Some(c) = low[x] {
                out = c;
            }
            if let Some((c, true)) = bg0 {
                out = c;
            }
            if let Some(c) = high[x] {
                out = c;
            }
            frame[line as usize * WIDTH + x] = rgb(out, regs.bright);
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

    const ON: Regs = Regs { ctrl: CTRL_ENABLE | CTRL_SPRITES, ..ZERO };

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
        render(&mem, Regs::default(), &mut frame);
        assert!(frame.iter().all(|&p| p == 0));
        render(&mem, Regs { ctrl: CTRL_ENABLE, ..Regs::default() }, &mut frame);
        assert!(frame.iter().all(|&p| p == 0x00FF_FFFF), "backdrop is palette 0 colour 0");
    }

    const BGS: Regs = Regs { ctrl: CTRL_ENABLE | CTRL_BG0 | CTRL_BG1 | CTRL_SPRITES, ..ZERO };
    const ZERO: Regs =
        Regs { ctrl: 0, bright: 0, scroll_x0: 0, scroll_y0: 0, scroll_x1: 0, scroll_y1: 0, bank: 0, palgrp: 0 };

    fn put_tile(mem: &mut [u8], map: u32, col: u32, row: u32, index: u8, attr: u8) {
        let at = (map + (row * MAP_W + col) * 2) as usize;
        mem[at] = index;
        mem[at + 1] = attr;
    }

    fn set_colour(mem: &mut [u8], palette: u32, colour: u32, value: u8) {
        mem[(spec::VRAM_PALETTE + palette * 16 + colour) as usize] = value;
    }

    /// Tile `n` filled with colour index `c`.
    fn solid_tile(mem: &mut [u8], n: u32, c: u8) {
        let at = (spec::VRAM_PATTERN + n * TILE_BYTES) as usize;
        mem[at..at + 32].fill(c << 4 | c);
    }

    fn at(frame: &[u32], x: usize, y: usize) -> u32 {
        frame[y * WIDTH + x]
    }

    #[test]
    fn bg0_tile_lands_on_its_cell() {
        let mut mem = memory();
        hide_all(&mut mem);
        solid_tile(&mut mem, 3, 1);
        set_colour(&mut mem, 0, 1, 0xE0);
        put_tile(&mut mem, spec::VRAM_BG0, 5, 2, 3, 0);
        let mut frame = vec![0; WIDTH * HEIGHT];
        render(&mem, BGS, &mut frame);
        assert_eq!(at(&frame, 40, 16), rgb(0xE0, 0), "top-left of cell (5, 2)");
        assert_eq!(at(&frame, 47, 23), rgb(0xE0, 0), "bottom-right");
        assert_eq!(at(&frame, 48, 16), 0, "next cell is backdrop");
        assert_eq!(at(&frame, 39, 16), 0);
    }

    #[test]
    fn scrolling_moves_the_tile_and_wraps() {
        let mut mem = memory();
        hide_all(&mut mem);
        solid_tile(&mut mem, 3, 1);
        set_colour(&mut mem, 0, 1, 0xE0);
        put_tile(&mut mem, spec::VRAM_BG0, 5, 2, 3, 0);
        let mut frame = vec![0; WIDTH * HEIGHT];
        render(&mem, Regs { scroll_x0: 3, scroll_y0: 10, ..BGS }, &mut frame);
        assert_eq!(at(&frame, 37, 6), rgb(0xE0, 0), "moved left 3, up 10");
        assert_eq!(at(&frame, 45, 6), 0);
        assert_eq!(at(&frame, 36, 6), 0);
        // the map wraps at 320x240
        render(&mem, Regs { scroll_x0: 320 + 48, scroll_y0: 240 + 24, ..BGS }, &mut frame);
        assert_eq!(at(&frame, 319, 239), rgb(0xE0, 0), "wrapped to the far corner");
        // BG1 scrolls on its own registers
        put_tile(&mut mem, spec::VRAM_BG0, 5, 2, 0, 0);
        put_tile(&mut mem, spec::VRAM_BG1, 5, 2, 3, 0);
        render(&mem, Regs { scroll_x0: 100, scroll_x1: 8, ..BGS }, &mut frame);
        assert_eq!(at(&frame, 32, 16), rgb(0xE0, 0));
    }

    #[test]
    fn bg_flips() {
        let mut mem = memory();
        hide_all(&mut mem);
        let tile = (spec::VRAM_PATTERN + 4 * TILE_BYTES) as usize;
        mem[tile] = 0x10; // top-left pixel only
        set_colour(&mut mem, 0, 1, 0xE0);
        put_tile(&mut mem, spec::VRAM_BG0, 0, 0, 4, 0);
        put_tile(&mut mem, spec::VRAM_BG0, 1, 0, 4, 0x02);
        put_tile(&mut mem, spec::VRAM_BG0, 2, 0, 4, 0x04);
        put_tile(&mut mem, spec::VRAM_BG1, 3, 0, 4, 0x01);
        put_tile(&mut mem, spec::VRAM_BG1, 4, 0, 4, 0x02);
        let mut frame = vec![0; WIDTH * HEIGHT];
        render(&mem, BGS, &mut frame);
        let red = rgb(0xE0, 0);
        assert_eq!(at(&frame, 0, 0), red);
        assert_eq!(at(&frame, 15, 0), red, "BG0 flip H");
        assert_eq!(at(&frame, 16, 7), red, "BG0 flip V");
        assert_eq!(at(&frame, 31, 0), red, "BG1 flip H");
        assert_eq!(at(&frame, 32, 7), red, "BG1 flip V");
        assert_eq!(at(&frame, 8, 0), 0);
    }

    #[test]
    fn palette_groups_and_bank() {
        let mut mem = memory();
        hide_all(&mut mem);
        solid_tile(&mut mem, 2 * 256 + 1, 1);
        set_colour(&mut mem, 3 * 16 + 5, 1, 0xE0); // BG0: group 3, palette 5
        set_colour(&mut mem, 7 * 16 + 9, 1, 0x1C); // BG1: group 7, palette 9
        put_tile(&mut mem, spec::VRAM_BG0, 0, 0, 1, 5 << 3);
        put_tile(&mut mem, spec::VRAM_BG1, 1, 0, 1, 9 << 2);
        let mut frame = vec![0; WIDTH * HEIGHT];
        render(&mem, Regs { bank: 2, palgrp: 0x73, ..BGS }, &mut frame);
        assert_eq!(at(&frame, 0, 0), rgb(0xE0, 0));
        assert_eq!(at(&frame, 8, 0), rgb(0x1C, 0));
        render(&mem, Regs { bank: 0, palgrp: 0x73, ..BGS }, &mut frame);
        assert_eq!(at(&frame, 0, 0), 0, "bank 0 tile 1 is blank");
    }

    #[test]
    fn layers_stack_and_colour_zero_is_transparent() {
        let mut mem = memory();
        hide_all(&mut mem);
        solid_tile(&mut mem, 1, 1);
        solid_tile(&mut mem, 2, 2);
        let half = (spec::VRAM_PATTERN + 3 * TILE_BYTES) as usize;
        mem[half] = 0x30; // left pixel colour 3, right pixel transparent
        set_colour(&mut mem, 0, 0, 0x03); // backdrop
        set_colour(&mut mem, 0, 1, 0xE0);
        set_colour(&mut mem, 0, 2, 0x1C);
        set_colour(&mut mem, 0, 3, 0xFF);
        put_tile(&mut mem, spec::VRAM_BG2, 0, 0, 1, 0);
        put_tile(&mut mem, spec::VRAM_BG1, 0, 0, 2, 0);
        put_tile(&mut mem, spec::VRAM_BG0, 0, 0, 3, 0);
        put_tile(&mut mem, spec::VRAM_BG2, 1, 0, 1, 0);
        put_tile(&mut mem, spec::VRAM_BG0, 1, 0, 3, 0);
        let mut frame = vec![0; WIDTH * HEIGHT];
        render(&mem, BGS, &mut frame);
        assert_eq!(at(&frame, 0, 0), rgb(0xFF, 0), "BG0 over BG1");
        assert_eq!(at(&frame, 1, 0), rgb(0x1C, 0), "BG0 colour 0 shows BG1");
        assert_eq!(at(&frame, 9, 0), rgb(0xE0, 0), "BG0 and BG1 clear show BG2");
        assert_eq!(at(&frame, 16, 0), rgb(0x03, 0), "BG2 colour 0 is the backdrop");
        render(&mem, Regs { ctrl: CTRL_ENABLE | CTRL_BG1, ..ZERO }, &mut frame);
        assert_eq!(at(&frame, 0, 0), rgb(0x1C, 0), "BG0 disabled");
        render(&mem, Regs { ctrl: CTRL_ENABLE, ..ZERO }, &mut frame);
        assert_eq!(at(&frame, 0, 0), rgb(0xE0, 0), "BG1 disabled, BG2 always on");
    }

    #[test]
    fn sprite_priority_against_bg0() {
        let mut mem = memory();
        hide_all(&mut mem);
        solid_tile(&mut mem, 1, 1);
        solid_tile(&mut mem, 2, 2);
        set_colour(&mut mem, 0, 1, 0xE0); // BG0 tiles
        set_colour(&mut mem, 1, 2, 0x1C); // sprites
        put_tile(&mut mem, spec::VRAM_BG0, 0, 0, 1, 0); // normal tile
        put_tile(&mut mem, spec::VRAM_BG0, 1, 0, 1, 1); // priority tile
        put_tile(&mut mem, spec::VRAM_BG1, 2, 0, 1, 0);
        let prio = spec::SPR_PRIO as u8;
        put_sprite(&mut mem, 0, 0, 0, 2, 0, 1);
        put_sprite(&mut mem, 1, 8, 0, 2, prio, 1);
        put_sprite(&mut mem, 2, 0, 8, 2, prio, 1);
        put_sprite(&mut mem, 3, 16, 0, 2, prio, 1);
        put_tile(&mut mem, spec::VRAM_BG0, 0, 1, 1, 0);
        let mut frame = vec![0; WIDTH * HEIGHT];
        render(&mem, BGS, &mut frame);
        let (sprite, tile) = (rgb(0x1C, 0), rgb(0xE0, 0));
        assert_eq!(at(&frame, 0, 0), sprite, "high-priority sprite over a normal BG0 tile");
        assert_eq!(at(&frame, 8, 0), tile, "BG0 priority tile over a low-priority sprite");
        assert_eq!(at(&frame, 0, 8), sprite, "low-priority sprite over a normal BG0 tile");
        assert_eq!(at(&frame, 16, 0), sprite, "low-priority sprite over BG1");
    }
}
