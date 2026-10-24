//! Short and long branches, run through `step()`.

use p63_core::{Addr, Bus, Flags, Ram, ReferenceInterpreter};

const PC: u32 = 0x1000;

fn cpu(code: &[u8], flags: Flags) -> ReferenceInterpreter<Ram> {
    let mut ram = Ram::new(0x0100_0000);
    for (i, b) in code.iter().enumerate() {
        ram.write(Addr::new(PC + i as u32), *b);
    }
    let mut cpu = ReferenceInterpreter::new(ram);
    cpu.registers.set_pc(PC);
    cpu.registers.set_cc(flags);
    cpu
}

/// Runs one branch; returns (cycles, new PC).
fn run(code: &[u8], flags: Flags) -> (u32, u32) {
    let mut c = cpu(code, flags);
    let cycles = c.step().expect("step").0;
    (cycles, c.registers.pc())
}

#[test]
fn bra_forward_and_back() {
    assert_eq!(run(&[0x30, 0x10], Flags::empty()), (2, PC + 2 + 0x10));
    assert_eq!(run(&[0x30, 0xFE], Flags::empty()), (2, PC)); // BRA * (loop to itself)
}

#[test]
fn brn_never_branches() {
    assert_eq!(run(&[0x31, 0x10], Flags::all()), (1, PC + 2));
}

#[test]
fn beq_bne() {
    assert_eq!(run(&[0x32, 0x08], Flags::Z), (2, PC + 10));
    assert_eq!(run(&[0x32, 0x08], Flags::empty()), (1, PC + 2));
    assert_eq!(run(&[0x33, 0x08], Flags::empty()), (2, PC + 10));
    assert_eq!(run(&[0x33, 0x08], Flags::Z), (1, PC + 2));
}

#[test]
fn unsigned_conditions() {
    assert_eq!(run(&[0x34, 4], Flags::empty()).1, PC + 6); // BCC
    assert_eq!(run(&[0x35, 4], Flags::C).1, PC + 6); // BCS
    assert_eq!(run(&[0x36, 4], Flags::empty()).1, PC + 6); // BHI
    assert_eq!(run(&[0x36, 4], Flags::Z).1, PC + 2);
    assert_eq!(run(&[0x37, 4], Flags::C).1, PC + 6); // BLS
    assert_eq!(run(&[0x37, 4], Flags::empty()).1, PC + 2);
}

#[test]
fn signed_conditions() {
    // N xor V decides less-than
    assert_eq!(run(&[0x38, 4], Flags::N | Flags::V).1, PC + 6); // BGE
    assert_eq!(run(&[0x39, 4], Flags::N).1, PC + 6); // BLT
    assert_eq!(run(&[0x39, 4], Flags::V).1, PC + 6);
    assert_eq!(run(&[0x3A, 4], Flags::empty()).1, PC + 6); // BGT
    assert_eq!(run(&[0x3A, 4], Flags::Z).1, PC + 2);
    assert_eq!(run(&[0x3B, 4], Flags::Z).1, PC + 6); // BLE
    assert_eq!(run(&[0x3B, 4], Flags::N).1, PC + 6);
}

#[test]
fn sign_and_overflow_conditions() {
    assert_eq!(run(&[0x3C, 4], Flags::N).1, PC + 6); // BMI
    assert_eq!(run(&[0x3D, 4], Flags::empty()).1, PC + 6); // BPL
    assert_eq!(run(&[0x3E, 4], Flags::V).1, PC + 6); // BVS
    assert_eq!(run(&[0x3F, 4], Flags::empty()).1, PC + 6); // BVC
}

#[test]
fn long_branch_taken_and_not() {
    // LBEQ +$1000 / -$10, offset counted from PC+3
    assert_eq!(run(&[0x80, 0x00, 0x10], Flags::Z), (3, PC + 3 + 0x1000));
    assert_eq!(run(&[0x80, 0xF0, 0xFF], Flags::Z), (3, PC + 3 - 0x10));
    assert_eq!(run(&[0x80, 0x00, 0x10], Flags::empty()), (2, PC + 3));
}

#[test]
fn long_conditions_match_short_ones() {
    // 0x81..0x8D use the same conditions as 0x33..0x3F
    for op in 0x81u8..=0x8D {
        for flags in [Flags::empty(), Flags::Z, Flags::C, Flags::N, Flags::V, Flags::N | Flags::V] {
            let short = run(&[op - 0x80 + 0x32, 4], flags).1 != PC + 2;
            let long = run(&[op, 4, 0], flags).1 != PC + 3;
            assert_eq!(short, long, "{op:#04X} with {flags:?}");
        }
    }
}

#[test]
fn branch_wraps_at_24_bits() {
    let mut c = cpu(&[], Flags::empty());
    c.bus.write(Addr::new(0xFF_FFFE), 0x30);
    c.bus.write(Addr::new(0xFF_FFFF), 0x04);
    c.registers.set_pc(0xFF_FFFE);
    c.step().unwrap();
    assert_eq!(c.registers.pc(), 0x00_0004);
}
