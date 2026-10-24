//! ALU operand encoding and indexed sub-modes,
//! run through `step()` from raw bytes.

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

/// Steps once; returns cycles and checks PC moved past exactly `code.len()` bytes.
fn step(cpu: &mut ReferenceInterpreter<Ram>, len: u32) -> u32 {
    let c = cpu.step().expect("step").0;
    assert_eq!(cpu.registers.pc(), PC + len, "instruction length");
    c
}

#[test]
fn add_a_a_doubles() {
    let mut c = cpu(&[0x10, 0x00]);
    c.registers.set_a(21);
    assert_eq!(step(&mut c, 2), 1);
    assert_eq!(c.registers.a(), 42);
}

#[test]
fn add_d_w_is_16_bit() {
    let mut c = cpu(&[0x10, 0x52]);
    c.registers.set_d(0x80FF);
    c.registers.set_w(0x8001);
    assert_eq!(step(&mut c, 2), 1);
    assert_eq!(c.registers.d(), 0x0100);
    let f = c.registers.cc();
    assert!(f.contains(Flags::C) && f.contains(Flags::V) && f.contains(Flags::H));
}

#[test]
fn cmp_a_immediate() {
    let mut c = cpu(&[0x1B, 0xE0, 0x05]);
    c.registers.set_a(5);
    assert_eq!(step(&mut c, 3), 2);
    assert_eq!(c.registers.a(), 5, "CMP doesn't write");
    assert!(c.registers.cc().contains(Flags::Z));
}

#[test]
fn cmp_x_immediate_is_24_bit() {
    let mut c = cpu(&[0x1B, 0xE8, 0x00, 0x10, 0x00]);
    c.registers.set_x(0x0FFF);
    assert_eq!(step(&mut c, 5), 2);
    assert!(c.registers.cc().contains(Flags::C), "0x0FFF < 0x1000 borrows");
    assert!(c.registers.cc().contains(Flags::N), "24-bit result is negative");
}

#[test]
fn add_gp_direct_page() {
    let mut c = cpu(&[0x10, 0xF7, 0x00, 0x20]);
    c.registers.set_dp(0x30);
    c.registers.set_gp(1);
    c.bus.write(Addr::new(0x3020), 9);
    assert_eq!(step(&mut c, 4), 2);
    assert_eq!(c.registers.gp(), 10);
}

#[test]
fn sub_d_extended() {
    let mut c = cpu(&[0x12, 0xF2, 0x01, 0x45, 0x23, 0x01]);
    c.registers.set_d(0x1000);
    c.bus.write_u16_le(Addr::new(0x01_2345), 0x0001);
    assert_eq!(step(&mut c, 6), 2);
    assert_eq!(c.registers.d(), 0x0FFF);
}

#[test]
fn or_a_indexed_3bit() {
    let mut c = cpu(&[0x15, 0xF0, 0x02, 0x0B]);
    c.registers.set_x(0x2000);
    c.registers.set_a(0x0F);
    c.bus.write(Addr::new(0x2003), 0xF0);
    assert_eq!(step(&mut c, 4), 2);
    assert_eq!(c.registers.a(), 0xFF);
}

#[test]
fn indexed_8bit_offset_uses_0x01() {
    // LD A,-2,Y with the 8-bit form: D0 81 FE
    let mut c = cpu(&[0xD0, 0x81, 0xFE]);
    c.registers.set_y(0x2002);
    c.bus.write(Addr::new(0x2000), 0x77);
    step(&mut c, 3);
    assert_eq!(c.registers.a(), 0x77);
}

#[test]
fn indexed_6bit_offset_covers_0x40() {
    // 0x40-0x7F is now only the 6-bit form; 0x60 = offset -32
    let mut c = cpu(&[0xD0, 0x60]);
    c.registers.set_x(0x2020);
    c.bus.write(Addr::new(0x2000), 0x55);
    step(&mut c, 2);
    assert_eq!(c.registers.a(), 0x55);
}

#[test]
fn bit_e_immediate_sets_flags_only() {
    let mut c = cpu(&[0x29, 0xE3, 0x80]);
    c.registers.set_e(0x7F);
    assert_eq!(step(&mut c, 3), 2);
    assert_eq!(c.registers.e(), 0x7F, "BIT doesn't write");
    assert!(c.registers.cc().contains(Flags::Z));
}

#[test]
fn div_d_by_e() {
    let mut c = cpu(&[0x1D, 0x00]);
    c.registers.set_d(1000);
    c.registers.set_e(7);
    step(&mut c, 2);
    assert_eq!(c.registers.a(), 142);
    assert_eq!(c.registers.b(), 6);
}

#[test]
fn inc_with_count() {
    let mut c = cpu(&[0x18, 0xE0, 0x05]);
    c.registers.set_a(10);
    assert_eq!(step(&mut c, 3), 6);
    assert_eq!(c.registers.a(), 15);
}

#[test]
fn inc_y_by_one() {
    let mut c = cpu(&[0x18, 0x09]);
    c.registers.set_y(0xFF_FFFF);
    assert_eq!(step(&mut c, 2), 1);
    assert_eq!(c.registers.y(), 0);
    assert!(c.registers.cc().contains(Flags::Z));
}

#[test]
fn clr_d() {
    let mut c = cpu(&[0x1F, 0x02]);
    c.registers.set_d(0xBEEF);
    step(&mut c, 2);
    assert_eq!(c.registers.d(), 0);
}

#[test]
fn illegal_postbytes_are_one_cycle_nops() {
    for code in [
        &[0x10, 0x0B][..], // dest PC
        &[0x10, 0x0D],     // dest CC
        &[0x17, 0x10],     // unary with non-zero high nibble
        &[0x18, 0x50],     // bad count nibble
        &[0x1C, 0x02],     // MUL mode
        &[0x55, 0x09],     // SMODX dest not X
    ] {
        let mut c = cpu(code);
        c.registers.set_a(1);
        let before = c.registers.clone();
        assert_eq!(step(&mut c, 2), 1, "{code:02X?}");
        let mut after = c.registers.clone();
        after.set_pc(before.pc());
        assert_eq!(after, before, "{code:02X?} changed state");
    }
}

#[test]
fn mul_signed() {
    let mut c = cpu(&[0x1C, 0x01]);
    c.registers.set_a(0xFE); // -2
    c.registers.set_b(3);
    step(&mut c, 2);
    assert_eq!(c.registers.d(), 0xFFFA); // -6
    assert!(c.registers.cc().contains(Flags::N));
    assert!(!c.registers.cc().contains(Flags::C));
}

#[test]
fn div_signed() {
    let mut c = cpu(&[0x1D, 0x01]);
    c.registers.set_d(0xFF03); // -253
    c.registers.set_e(3);
    step(&mut c, 2);
    assert_eq!(c.registers.a(), 0xAC); // -84 (truncates toward zero)
    assert_eq!(c.registers.b(), 0xFF); // -1
}

#[test]
fn smodx_immediate_negative() {
    let mut c = cpu(&[0x55, 0xE8, 0xFF, 0xFF, 0xFF]);
    c.registers.set_x(0x10);
    assert_eq!(step(&mut c, 5), 2);
    assert_eq!(c.registers.x(), 0x0F);
}

#[test]
fn smodx_d_sign_extends() {
    let mut c = cpu(&[0x55, 0x28]);
    c.registers.set_x(0x1_0000);
    c.registers.set_d(0xFFFF); // -1
    step(&mut c, 2);
    assert_eq!(c.registers.x(), 0xFFFF);
}
