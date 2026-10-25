//! Boots the sound demos in the games folder and checks what they play.

use p63_bus::Machine;
use p63_core::{Addr, Bus};
use p63_spec as spec;

const GAMES: &str = "../games";

/// Skips these tests in a checkout without the games folder.
fn have_games() -> bool {
    std::path::Path::new(GAMES).is_dir()
}

fn boot(file: &str) -> (Machine, p63_asm::Program) {
    let program = p63_asm::assemble_file(format!("{GAMES}/{file}")).unwrap_or_else(|e| panic!("{}", e[0]));
    let image = program.image_at(spec::FIXED_ENTRY).unwrap();
    let mut m = Machine::new();
    m.boot(p63_cart::pack(&image, &p63_cart::PackOptions::default()).unwrap()).unwrap();
    (m, program)
}

fn read(m: &mut Machine, a: u32) -> u8 {
    m.cpu.bus.read(Addr::new(a))
}

fn pitch(m: &mut Machine, voice: u32) -> u32 {
    let at = spec::AUD_V0_PITCH_L + voice * spec::AUD_STRIDE;
    u32::from(read(m, at)) | u32::from(read(m, at + 1)) << 8
}

#[test]
fn song_plays_all_eight_voices() {
    if !have_games() {
        return;
    }
    let (mut m, _) = boot("song.p63");
    let (mut loud, mut voices) = (0, 0);
    // past the start of the solo, where the twin guitar comes in
    for _ in 0..1700 {
        m.run_frame().unwrap();
        loud += m.audio.iter().filter(|&&(l, _)| l != 0).count();
        voices |= read(&mut m, spec::AUD_STAT);
        assert_eq!(m.cpu.halt(), p63_core::Halt::VBlank, "the player finishes inside the frame");
    }
    assert_eq!(voices, 0xFF, "every voice played");
    assert!(loud > 1700 * 400, "mostly sound, not silence ({loud} samples)");
}

#[test]
fn stream_jingle_changes_every_frame() {
    if !have_games() {
        return;
    }
    let (mut m, _) = boot("stream.p63");
    let (mut sweep, mut chord) = (Vec::new(), Vec::new());
    for _ in 0..30 {
        m.run_frame().unwrap();
        sweep.push(pitch(&mut m, 0));
        chord.push(pitch(&mut m, 1));
    }
    assert!(sweep.windows(2).all(|p| p[1] > p[0]), "the sweep rises every frame: {sweep:?}");
    let mut notes = chord.clone();
    notes.sort_unstable();
    notes.dedup();
    assert_eq!(notes.len(), 3, "the fake chord cycles three notes: {chord:?}");
    assert!(chord.windows(3).all(|w| w[0] != w[2] || w[0] == w[1]), "each note lasts 2 frames");
}

#[test]
fn scale_starts_on_c4() {
    if !have_games() {
        return;
    }
    let (mut m, program) = boot("scale.p63");
    for _ in 0..3 {
        m.run_frame().unwrap();
    }
    assert_eq!(read(&mut m, spec::AUD_STAT) & 1, 1);
    assert_eq!(pitch(&mut m, 0), program.symbol("N_C4").unwrap());
}
