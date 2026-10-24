//! PACK UNP BEXT BINS LEA LDI MOVQ DAA, run through `step()`.

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

/// Steps once; checks the instruction length and returns cycles.
fn step(cpu: &mut ReferenceInterpreter<Ram>, len: u32) -> u32 {
    let c = cpu.step().expect("step").0;
    assert_eq!(cpu.registers.pc(), PC + len, "instruction length");
    c
}

#[test]
fn pack_gp_and_e_into_w() {
    let mut c = cpu(&[0x50, 0x73, 0x05]); // PACK W,GP,E
    c.registers.set_gp(0x12);
    c.registers.set_e(0x34);
    assert_eq!(step(&mut c, 3), 1);
    assert_eq!(c.registers.w(), 0x1234);
}

#[test]
fn unp_b_a_d_swaps_halves() {
    let mut c = cpu(&[0x51, 0x10, 0x02]); // UNP B,A,D
    c.registers.set_d(0xAA55);
    step(&mut c, 3);
    assert_eq!(c.registers.b(), 0xAA);
    assert_eq!(c.registers.a(), 0x55);
}

#[test]
fn pack_into_an_8_bit_register_is_a_nop() {
    let mut c = cpu(&[0x50, 0x01, 0x00]);
    c.registers.set_a(7);
    assert_eq!(step(&mut c, 3), 1);
    assert_eq!(c.registers.a(), 7);
}

#[test]
fn bext_a_from_d() {
    // BEXT A,D,#4,#3: $1234 >> 4 = $123, low 3 bits = 3
    let mut c = cpu(&[0x52, 0x20, 0x42]);
    c.registers.set_d(0x1234);
    assert_eq!(step(&mut c, 3), 2);
    assert_eq!(c.registers.a(), 3);
}

#[test]
fn bins_builds_rgb332() {
    // BINS A,GP,#5,#3 / #2,#3 / #0,#2 with R=3 G=2 B=2
    let mut c = cpu(&[0x53, 0x70, 0x52, 0x53, 0x70, 0x22, 0x53, 0x70, 0x01]);
    c.registers.set_a(0);
    c.registers.set_cc(Flags::C);
    c.registers.set_gp(3);
    c.step().unwrap();
    c.registers.set_gp(2);
    c.step().unwrap();
    c.registers.set_gp(2);
    c.step().unwrap();
    assert_eq!(c.registers.a(), 0b0110_1010);
    assert_eq!(c.registers.cc(), Flags::C, "BINS changes no flags");
}

#[test]
fn bins_field_past_the_register_is_a_nop() {
    let mut c = cpu(&[0x53, 0x00, 0x72]); // pos 7, len 3 in an 8-bit register
    c.registers.set_a(0x0F);
    assert_eq!(step(&mut c, 3), 1);
    assert_eq!(c.registers.a(), 0x0F);
}

#[test]
fn lea_steps_x_and_y() {
    let mut c = cpu(&[0x54, 0x09, 0x54, 0x8F, 0x54, 0x02, 0x00, 0xFF]);
    c.registers.set_x(0x2000);
    c.registers.set_y(0x3000);
    assert_eq!(c.step().unwrap().0, 1);
    assert_eq!(c.registers.x(), 0x2001); // LEA X,1,X
    c.step().unwrap();
    assert_eq!(c.registers.y(), 0x2FFF); // LEA Y,-1,Y
    assert_eq!(c.step().unwrap().0, 3);
    assert_eq!(c.registers.x(), 0x2001 - 256); // LEA X,-256,X
    assert_eq!(c.registers.pc(), PC + 8);
}

#[test]
fn ldi_uses_the_register_width() {
    let mut c = cpu(&[0x5A, 0x08, 0x56, 0x34, 0x12]); // LDI X,#$123456
    assert_eq!(step(&mut c, 5), 2);
    assert_eq!(c.registers.x(), 0x12_3456);

    let mut c = cpu(&[0x5A, 0x07, 0x80]); // LDI GP,#$80
    assert_eq!(step(&mut c, 3), 1);
    assert_eq!(c.registers.gp(), 0x80);
    assert!(c.registers.cc().contains(Flags::N));
}

#[test]
fn movq_from_register_immediate_and_memory() {
    let mut c = cpu(&[0x5B, 0x50]); // MOVQ Q,W
    c.registers.set_w(0xBEEF);
    assert_eq!(step(&mut c, 2), 3);
    assert_eq!(c.registers.q(), 0xBEEF);

    let mut c = cpu(&[0x5B, 0xE0, 0x78, 0x56, 0x34, 0x12]); // MOVQ Q,#$12345678
    step(&mut c, 6);
    assert_eq!(c.registers.q(), 0x1234_5678);

    let mut c = cpu(&[0x5B, 0xF0, 0x01, 0x00, 0x20, 0x00]); // MOVQ Q,$002000
    c.bus.write_u32_le(Addr::new(0x2000), 0x8000_0001);
    step(&mut c, 6);
    assert_eq!(c.registers.q(), 0x8000_0001);
    assert!(c.registers.cc().contains(Flags::N));
}

#[test]
fn daa_after_bcd_add() {
    // 0x45 + 0x38 = 0x7D binary -> 0x83 BCD
    let mut c = cpu(&[0x10, 0x10, 0x5D, 0x00]); // ADD A,B ; DAA
    c.registers.set_a(0x45);
    c.registers.set_b(0x38);
    c.step().unwrap();
    c.step().unwrap();
    assert_eq!(c.registers.a(), 0x83);
    assert!(!c.registers.cc().contains(Flags::C));

    // 0x99 + 0x01 = 0x9A -> 0x00 with carry
    let mut c = cpu(&[0x10, 0x10, 0x5D, 0x00]);
    c.registers.set_a(0x99);
    c.registers.set_b(0x01);
    c.step().unwrap();
    c.step().unwrap();
    assert_eq!(c.registers.a(), 0x00);
    assert!(c.registers.cc().contains(Flags::C | Flags::Z));
}

#[test]
fn daa_after_bcd_subtract() {
    // 0x42 - 0x15 = 0x2D binary (half borrow) -> 0x27 BCD
    let mut c = cpu(&[0x12, 0x10, 0x5D, 0x01]); // SUB A,B ; DAA (subtract form)
    c.registers.set_a(0x42);
    c.registers.set_b(0x15);
    c.step().unwrap();
    c.step().unwrap();
    assert_eq!(c.registers.a(), 0x27);
}
