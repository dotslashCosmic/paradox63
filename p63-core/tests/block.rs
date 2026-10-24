//! MOVB MOVW FILL CPY SCNB, including interrupt-and-resume equivalence.

use p63_core::{Addr, Bus, Flags, Ram, ReferenceInterpreter};
use p63_spec::VEC_FIRQ;

const PC: u32 = 0x1000;
const SRC: u32 = 0x2000;
const DST: u32 = 0x3000;
const HANDLER: u32 = 0x4000;

fn cpu(code: &[u8]) -> ReferenceInterpreter<Ram> {
    let mut ram = Ram::new(0x0100_0000);
    for (i, b) in code.iter().enumerate() {
        ram.write(Addr::new(PC + i as u32), *b);
    }
    for i in 0..256u32 {
        ram.write(Addr::new(SRC + i), (i * 37 + 11) as u8);
        ram.write(Addr::new(DST + i), (i * 37 + 11) as u8);
    }
    // FIRQ handler trashes X and Y (FIRQ doesn't stack them), then RTI
    ram.write_u24_le(Addr::new(VEC_FIRQ), HANDLER);
    let handler = [0x5A, 0x08, 0, 0, 0, 0x5A, 0x09, 0, 0, 0, 0x43, 0x00];
    for (i, b) in handler.iter().enumerate() {
        ram.write(Addr::new(HANDLER + i as u32), *b);
    }
    let mut cpu = ReferenceInterpreter::new(ram);
    cpu.registers.set_pc(PC);
    cpu.registers.set_sp(0x8000);
    cpu.registers.set_x(SRC);
    cpu.registers.set_y(DST);
    cpu
}

/// Steps until PC is past the instruction; returns total cycles.
fn finish(c: &mut ReferenceInterpreter<Ram>, len: u32) -> u32 {
    let mut cycles = 0;
    for _ in 0..100_000 {
        if c.registers.pc() == PC + len {
            return cycles;
        }
        cycles += c.step().unwrap().0;
    }
    panic!("block op never finished");
}

fn run(code: &[u8]) -> (ReferenceInterpreter<Ram>, u32) {
    let mut c = cpu(code);
    let cycles = finish(&mut c, code.len() as u32);
    (c, cycles)
}

/// Runs the instruction, raising FIRQ after each of its first `n` units.
fn run_interrupted(mut c: ReferenceInterpreter<Ram>, len: u32, n: u32) -> ReferenceInterpreter<Ram> {
    let mut units = 0;
    for _ in 0..100_000 {
        if c.registers.pc() == PC + len {
            return c;
        }
        let in_block = c.registers.pc() == PC;
        c.step().unwrap();
        if c.registers.pc() == HANDLER {
            c.set_firq_pending(false); // the handler acknowledges the source
        } else if in_block {
            units += 1;
            if units <= n {
                c.set_firq_pending(true);
            }
        }
    }
    panic!("block op never finished");
}

fn assert_same(a: &mut ReferenceInterpreter<Ram>, b: &mut ReferenceInterpreter<Ram>, what: &str) {
    assert_eq!(a.registers.x(), b.registers.x(), "{what}: X");
    assert_eq!(a.registers.y(), b.registers.y(), "{what}: Y");
    assert_eq!(a.registers.cc(), b.registers.cc(), "{what}: CC");
    assert_eq!(a.registers.sp(), b.registers.sp(), "{what}: SP");
    for i in 0..256 {
        assert_eq!(a.bus.read(Addr::new(SRC + i)), b.bus.read(Addr::new(SRC + i)), "{what}: src+{i}");
        assert_eq!(a.bus.read(Addr::new(DST + i)), b.bus.read(Addr::new(DST + i)), "{what}: dst+{i}");
    }
    assert!(b.block_continuation().is_none(), "{what}: continuation left behind");
}

#[test]
fn movb_copies_and_advances() {
    let mut c = cpu(&[0x60, 0x05]);
    c.bus.write(Addr::new(SRC), 0xAB);
    assert_eq!(finish(&mut c, 2), 5);
    assert_eq!(c.bus.read(Addr::new(DST)), 0xAB);
    assert_eq!(c.registers.x(), SRC + 5);
    assert_eq!(c.registers.y(), DST + 5);
}

#[test]
fn each_step_is_one_unit_with_pc_held() {
    let mut c = cpu(&[0x60, 0x03]);
    assert_eq!(c.step().unwrap().0, 1);
    assert_eq!(c.registers.pc(), PC, "PC stays on the block instruction");
    assert_eq!(c.block_continuation().unwrap().remaining, 2);
}

#[test]
fn movw_long_count_costs_one_cycle_per_word() {
    let mut c = cpu(&[0x61, 0x80, 0xF4, 0x01]); // MOVW, 500 words
    for i in 0..1000u32 {
        c.bus.write(Addr::new(0x10_0000 + i), i as u8);
    }
    c.registers.set_x(0x10_0000);
    c.registers.set_y(0x20_0000);
    assert_eq!(finish(&mut c, 4), 500);
    assert_eq!(c.registers.x(), 0x10_0000 + 1000);
    for i in 0..1000u32 {
        assert_eq!(c.bus.read(Addr::new(0x20_0000 + i)), i as u8);
    }
}

#[test]
fn fill_writes_two_words_per_cycle() {
    let (mut c, cycles) = run(&[0x62, 0x80, 0xF4, 0x01, 0xCD, 0xAB]); // FILL #$ABCD, 500 words
    assert_eq!(cycles, 250);
    assert_eq!(c.registers.x(), SRC + 1000);
    assert_eq!(c.bus.read_u16_le(Addr::new(SRC)), 0xABCD);
    assert_eq!(c.bus.read_u16_le(Addr::new(SRC + 998)), 0xABCD);
}

#[test]
fn cpy_equal_blocks_set_z() {
    let (c, cycles) = run(&[0x63, 0x10]); // 16 words, SRC and DST hold the same bytes
    assert_eq!(cycles, 16);
    assert!(c.registers.cc().contains(Flags::Z));
    assert_eq!(c.registers.x(), SRC + 32);
}

#[test]
fn cpy_stops_at_first_difference() {
    let mut c = cpu(&[0x63, 0x10]);
    c.bus.write(Addr::new(DST + 6), 0x00); // word 3 differs
    c.bus.write(Addr::new(SRC + 6), 0x01);
    finish(&mut c, 2);
    assert!(!c.registers.cc().contains(Flags::Z));
    assert_eq!(c.registers.x(), SRC + 6, "X left on the differing word");
    assert_eq!(c.registers.y(), DST + 6);
}

#[test]
fn scnb_finds_a_byte_or_runs_off_the_end() {
    let target = (10 * 37 + 11) as u8;
    let (c, _) = run(&[0x64, 0x40, target]);
    assert!(c.registers.cc().contains(Flags::Z | Flags::C));
    assert_eq!(c.registers.x(), SRC + 10);

    let (c, cycles) = run(&[0x64, 0x05, target]);
    assert_eq!(cycles, 5);
    assert!(!c.registers.cc().intersects(Flags::Z | Flags::C));
    assert_eq!(c.registers.x(), SRC + 5);
}

#[test]
fn zero_count_is_a_one_cycle_nop() {
    let (c, cycles) = run(&[0x60, 0x00]);
    assert_eq!(cycles, 1);
    assert_eq!(c.registers.x(), SRC);
}

#[test]
fn interrupted_runs_match_uninterrupted_ones() {
    let target = (40 * 37 + 11) as u8;
    let cases: [(&str, Vec<u8>, u32); 6] = [
        ("MOVB", vec![0x60, 0x50], 80),
        ("MOVW", vec![0x61, 0x29], 21),
        ("FILL", vec![0x62, 0x29, 0x34, 0x12], 21),
        ("CPY", vec![0x63, 0x29], 21),
        ("SCNB found", vec![0x64, 0x50, target], 41),
        ("SCNB missing", vec![0x64, 0x30, 0xFF], 48),
    ];
    for (name, code, units) in cases {
        let len = code.len() as u32;
        let (mut plain, _) = run(&code);
        for n in 1..units {
            let mut broken = run_interrupted(cpu(&code), len, n);
            assert_same(&mut plain, &mut broken, &format!("{name} interrupted {n}x"));
        }
    }
}

#[test]
fn interrupted_cpy_with_a_difference_matches() {
    let code = [0x63, 0x20];
    let with_difference = || {
        let mut c = cpu(&code);
        c.bus.write(Addr::new(DST + 40), 0x00);
        c.bus.write(Addr::new(SRC + 40), 0x01);
        c
    };
    let mut plain = with_difference();
    finish(&mut plain, 2);
    for n in 1..12 {
        let mut broken = run_interrupted(with_difference(), 2, n);
        assert_same(&mut plain, &mut broken, &format!("CPY mismatch interrupted {n}x"));
    }
}
