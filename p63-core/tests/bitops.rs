//! BSET BCLR BCHG BTST JBS JBC DBNZ TBZ TBNZ, run through `step()`.

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

fn step(cpu: &mut ReferenceInterpreter<Ram>) -> u32 {
    cpu.step().expect("step").0
}

#[test]
fn bset_bclr_bchg_on_extended_memory() {
    // BSET $2000,#5 / BCLR $2000,#3 / BCHG $2000,#7
    let mut c = cpu(&[
        0x90, 0xF5, 0x01, 0x00, 0x20, 0x00,
        0x91, 0xF3, 0x01, 0x00, 0x20, 0x00,
        0x92, 0xF7, 0x01, 0x00, 0x20, 0x00,
    ]);
    c.bus.write(Addr::new(0x2000), 0x08);
    c.registers.set_cc(Flags::Z | Flags::C);
    assert_eq!(step(&mut c), 4);
    assert_eq!(c.bus.read(Addr::new(0x2000)), 0x28);
    step(&mut c);
    assert_eq!(c.bus.read(Addr::new(0x2000)), 0x20);
    step(&mut c);
    assert_eq!(c.bus.read(Addr::new(0x2000)), 0xA0);
    assert_eq!(c.registers.cc(), Flags::Z | Flags::C, "no flags changed");
    assert_eq!(c.registers.pc(), PC + 18);
}

#[test]
fn btst_register_sets_n_and_z() {
    // BTST E,#3 on a set bit, then on a clear bit
    let mut c = cpu(&[0x93, 0x33, 0x93, 0x34]);
    c.registers.set_e(0x08);
    assert_eq!(step(&mut c), 3);
    assert!(c.registers.cc().contains(Flags::N));
    assert!(!c.registers.cc().contains(Flags::Z));
    step(&mut c);
    assert!(!c.registers.cc().contains(Flags::N));
    assert!(c.registers.cc().contains(Flags::Z));
}

#[test]
fn bit_ops_on_direct_page_and_indexed() {
    // BSET <$10,#0 with DP=$40, then BCLR 1,X,#0 with the 3-bit offset form
    let mut c = cpu(&[0x90, 0xF0, 0x00, 0x10, 0x91, 0xF0, 0x02, 0x09]);
    c.registers.set_dp(0x40);
    c.registers.set_x(0x400F);
    c.bus.write(Addr::new(0x4010), 0xFE);
    step(&mut c);
    assert_eq!(c.bus.read(Addr::new(0x4010)), 0xFF);
    step(&mut c);
    assert_eq!(c.bus.read(Addr::new(0x4010)), 0xFE);
}

#[test]
fn bset_on_a_16_bit_register() {
    let mut c = cpu(&[0x90, 0x2F]); // BSET D,#15
    step(&mut c);
    assert_eq!(c.registers.d(), 0x8000);
}

#[test]
fn bit_number_past_the_operand_width_is_a_nop() {
    let mut c = cpu(&[0x90, 0x09]); // BSET A,#9
    assert_eq!(step(&mut c), 1);
    assert_eq!(c.registers.a(), 0);
}

#[test]
fn jbs_and_jbc_branch_on_a_memory_bit() {
    // JBS $2000,#4,+6
    let mut c = cpu(&[0x94, 0xF4, 0x01, 0x00, 0x20, 0x00, 0x06]);
    c.bus.write(Addr::new(0x2000), 0x10);
    assert_eq!(step(&mut c), 4);
    assert_eq!(c.registers.pc(), PC + 7 + 6);

    // JBC on the same set bit falls through
    let mut c = cpu(&[0x95, 0xF4, 0x01, 0x00, 0x20, 0x00, 0x06]);
    c.bus.write(Addr::new(0x2000), 0x10);
    step(&mut c);
    assert_eq!(c.registers.pc(), PC + 7);
}

#[test]
fn dbnz_counts_down_a_loop() {
    // LOOP: DBNZ B,LOOP  (rel = -3, back to itself)
    let mut c = cpu(&[0x96, 0x01, 0xFD]);
    c.registers.set_b(3);
    for left in [2u8, 1] {
        assert_eq!(step(&mut c), 2);
        assert_eq!(c.registers.b(), left);
        assert_eq!(c.registers.pc(), PC);
    }
    step(&mut c);
    assert_eq!(c.registers.b(), 0);
    assert_eq!(c.registers.pc(), PC + 3);
    assert!(c.registers.cc().contains(Flags::Z));
}

#[test]
fn dbnz_on_w_is_16_bit() {
    let mut c = cpu(&[0x96, 0x05, 0x10]);
    c.registers.set_w(0x0100);
    step(&mut c);
    assert_eq!(c.registers.w(), 0x00FF);
    assert_eq!(c.registers.pc(), PC + 3 + 0x10);
}

#[test]
fn tbz_and_tbnz() {
    // TBZ A,#6,+4 with bit 6 clear: taken
    let mut c = cpu(&[0x97, 0x06, 0x04]);
    c.registers.set_a(0x00);
    assert_eq!(step(&mut c), 2);
    assert_eq!(c.registers.pc(), PC + 3 + 4);

    // TBNZ A,#6,+4 with bit 6 clear: not taken
    let mut c = cpu(&[0x98, 0x06, 0x04]);
    step(&mut c);
    assert_eq!(c.registers.pc(), PC + 3);

    // TBNZ X,#15,-8 with that bit set: taken backwards
    let mut c = cpu(&[0x98, 0x8F, 0xF8]);
    c.registers.set_x(0x8000);
    step(&mut c);
    assert_eq!(c.registers.pc(), PC + 3 - 8);
}
