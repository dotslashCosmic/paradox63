//! JMP, JSR/RTS, BSR, LBRA, LBSR, run through `step()`.

use p63_core::{Addr, Bus, Ram, ReferenceInterpreter};

const PC: u32 = 0x1000;
const SP: u32 = 0x8000;

fn cpu() -> ReferenceInterpreter<Ram> {
    let mut cpu = ReferenceInterpreter::new(Ram::new(0x0100_0000));
    cpu.registers.set_pc(PC);
    cpu.registers.set_sp(SP);
    cpu
}

fn load(cpu: &mut ReferenceInterpreter<Ram>, at: u32, code: &[u8]) {
    for (i, b) in code.iter().enumerate() {
        cpu.bus.write(Addr::new(at + i as u32), *b);
    }
}

fn step(cpu: &mut ReferenceInterpreter<Ram>) -> u32 {
    cpu.step().expect("step").0
}

#[test]
fn jmp_extended() {
    let mut c = cpu();
    load(&mut c, PC, &[0x40, 0x56, 0x34, 0x12]);
    assert_eq!(step(&mut c), 2);
    assert_eq!(c.registers.pc(), 0x12_3456);
    assert_eq!(c.registers.sp(), SP, "JMP doesn't touch the stack");
}

#[test]
fn jsr_pushes_next_instruction_and_rts_returns() {
    // JSR $002000 at $1000, RTS at $2000
    let mut c = cpu();
    load(&mut c, PC, &[0x41, 0x00, 0x20, 0x00]);
    load(&mut c, 0x2000, &[0x42, 0x00]);

    assert_eq!(step(&mut c), 3);
    assert_eq!(c.registers.pc(), 0x2000);
    assert_eq!(c.registers.sp(), SP - 3);
    assert_eq!(c.bus.read(Addr::new(SP - 3)), 0x04);
    assert_eq!(c.bus.read(Addr::new(SP - 2)), 0x10);
    assert_eq!(c.bus.read(Addr::new(SP - 1)), 0x00);

    assert_eq!(step(&mut c), 3);
    assert_eq!(c.registers.pc(), PC + 4);
    assert_eq!(c.registers.sp(), SP);
}

#[test]
fn bsr_backwards_returns_past_itself() {
    let mut c = cpu();
    load(&mut c, PC, &[0x44, 0xF0]); // BSR -16
    assert_eq!(step(&mut c), 2);
    assert_eq!(c.registers.pc(), PC + 2 - 16);
    assert_eq!(c.bus.read_u24_le(Addr::new(SP - 3)), PC + 2);
}

#[test]
fn lbra_both_directions() {
    let mut c = cpu();
    load(&mut c, PC, &[0x45, 0x00, 0x10]);
    assert_eq!(step(&mut c), 2);
    assert_eq!(c.registers.pc(), PC + 3 + 0x1000);

    let mut c = cpu();
    load(&mut c, PC, &[0x45, 0xFD, 0xFF]); // -3 = loop to itself
    step(&mut c);
    assert_eq!(c.registers.pc(), PC);
}

#[test]
fn lbsr_pushes_address_after_three_bytes() {
    let mut c = cpu();
    load(&mut c, PC, &[0x4D, 0x00, 0x01]);
    assert_eq!(step(&mut c), 3);
    assert_eq!(c.registers.pc(), PC + 3 + 0x100);
    assert_eq!(c.bus.read_u24_le(Addr::new(SP - 3)), PC + 3);
}

#[test]
fn nested_calls_unwind_in_order() {
    let mut c = cpu();
    load(&mut c, PC, &[0x41, 0x00, 0x20, 0x00]); // JSR $2000
    load(&mut c, 0x2000, &[0x41, 0x00, 0x30, 0x00]); // JSR $3000
    load(&mut c, 0x3000, &[0x42, 0x00]); // RTS
    load(&mut c, 0x2004, &[0x42, 0x00]); // RTS
    for _ in 0..4 {
        step(&mut c);
    }
    assert_eq!(c.registers.pc(), PC + 4);
    assert_eq!(c.registers.sp(), SP);
}

#[test]
fn stack_wraps_below_zero() {
    let mut c = cpu();
    c.registers.set_sp(0x00_0001);
    load(&mut c, PC, &[0x44, 0x00]);
    step(&mut c);
    assert_eq!(c.registers.sp(), 0xFF_FFFE);
}
