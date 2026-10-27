//! TST/COM/SWAP/BIT and the shift/rotate group, run through `step()`.

use p63_core::{Addr, Bus, Flags, Ram, ReferenceInterpreter};

const PC: u32 = 0x1000;

fn cpu(code: &[u8]) -> ReferenceInterpreter<Ram> {
    let mut ram = Ram::new(0x0100_0000);
    for (i, b) in code.iter().enumerate() {
        ram.write(Addr::new(PC + i as u32), *b);
    }
    let mut cpu = ReferenceInterpreter::new(ram);
    cpu.registers.set_pc(PC);
    cpu
}

fn step(cpu: &mut ReferenceInterpreter<Ram>, len: u32) -> u32 {
    let c = cpu.step().expect("step").0;
    assert_eq!(cpu.registers.pc(), PC + len, "instruction length");
    c
}

fn set_carry(cpu: &mut ReferenceInterpreter<Ram>, on: bool) {
    let mut f = cpu.registers.cc();
    f.set(Flags::C, on);
    cpu.registers.set_cc(f);
}

fn flag(cpu: &ReferenceInterpreter<Ram>, f: Flags) -> bool {
    cpu.registers.cc().contains(f)
}

#[test]
fn bit_leaves_carry() {
    let mut c = cpu(&[0x29, 0xE0, 0x01]);
    c.registers.set_a(0x01);
    set_carry(&mut c, true);
    step(&mut c, 3);
    assert!(flag(&c, Flags::C));
    assert!(!flag(&c, Flags::Z));
}

#[test]
fn tst_d_sets_flags_without_writing() {
    let mut c = cpu(&[0x28, 0x02]);
    c.registers.set_d(0x8000);
    assert_eq!(step(&mut c, 2), 1);
    assert_eq!(c.registers.d(), 0x8000);
    assert!(flag(&c, Flags::N));
    assert!(!flag(&c, Flags::Z));
}

#[test]
fn com_sets_carry() {
    let mut c = cpu(&[0x2A, 0x00]);
    c.registers.set_a(0x0F);
    step(&mut c, 2);
    assert_eq!(c.registers.a(), 0xF0);
    assert!(flag(&c, Flags::C) && flag(&c, Flags::N));
}

#[test]
fn swap_every_byte_of_d() {
    let mut c = cpu(&[0x2B, 0x02]);
    c.registers.set_d(0x12AB);
    step(&mut c, 2);
    assert_eq!(c.registers.d(), 0x21BA);
}

#[test]
fn asl_a_sets_carry_and_overflow() {
    let mut c = cpu(&[0x20, 0x00]);
    c.registers.set_a(0x81);
    assert_eq!(step(&mut c, 2), 1);
    assert_eq!(c.registers.a(), 0x02);
    assert!(flag(&c, Flags::C));
    assert!(flag(&c, Flags::V)); // N=0, C=1
}

#[test]
fn lsl_is_asl() {
    let mut c = cpu(&[0x22, 0xE0, 0x03]);
    c.registers.set_a(0x05);
    assert_eq!(step(&mut c, 3), 2);
    assert_eq!(c.registers.a(), 0x28);
}

#[test]
fn asr_keeps_sign() {
    let mut c = cpu(&[0x21, 0xE0, 0x02]);
    c.registers.set_a(0x80);
    step(&mut c, 3);
    assert_eq!(c.registers.a(), 0xE0);
    assert!(!flag(&c, Flags::C));
    assert!(!flag(&c, Flags::V));
}

#[test]
fn lsr_q_by_17() {
    let mut c = cpu(&[0x23, 0xE6, 17]);
    c.registers.set_q(0x0003_0000);
    assert_eq!(step(&mut c, 3), 2); // a count of any size is 2 cycles
    assert_eq!(c.registers.q(), 1);
    assert!(flag(&c, Flags::C)); // bit 16 was the last one out
}

#[test]
fn asl_x_is_24_bit() {
    let mut c = cpu(&[0x20, 0x08]);
    c.registers.set_x(0x80_0000);
    step(&mut c, 2);
    assert_eq!(c.registers.x(), 0);
    assert!(flag(&c, Flags::Z) && flag(&c, Flags::C));
}

#[test]
fn rol_wraps_top_bit() {
    let mut c = cpu(&[0x24, 0x00]);
    c.registers.set_a(0x81);
    step(&mut c, 2);
    assert_eq!(c.registers.a(), 0x03);
    assert!(flag(&c, Flags::C));
}

#[test]
fn ror_wraps_low_bit_and_keeps_v() {
    let mut c = cpu(&[0x25, 0x00]);
    c.registers.set_a(0x01);
    let mut f = c.registers.cc();
    f.insert(Flags::V);
    c.registers.set_cc(f);
    step(&mut c, 2);
    assert_eq!(c.registers.a(), 0x80);
    assert!(flag(&c, Flags::C) && flag(&c, Flags::V));
}

#[test]
fn rcl_through_carry() {
    let mut c = cpu(&[0x26, 0x00]);
    c.registers.set_a(0x00);
    set_carry(&mut c, true);
    step(&mut c, 2);
    assert_eq!(c.registers.a(), 0x01);
    assert!(!flag(&c, Flags::C));
}

#[test]
fn rcr_through_carry() {
    let mut c = cpu(&[0x27, 0x00]);
    c.registers.set_a(0x00);
    set_carry(&mut c, true);
    step(&mut c, 2);
    assert_eq!(c.registers.a(), 0x80);
    assert!(!flag(&c, Flags::C));
}

#[test]
fn shift_with_bad_count_nibble_is_nop() {
    let mut c = cpu(&[0x20, 0x30]);
    c.registers.set_a(0x01);
    assert_eq!(step(&mut c, 2), 1);
    assert_eq!(c.registers.a(), 0x01);
}
