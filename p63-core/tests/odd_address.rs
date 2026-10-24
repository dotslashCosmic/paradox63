//! Word accesses at odd addresses take one more bus transfer (+1 cycle).

use p63_core::{Addr, Bus, Ram, ReferenceInterpreter};

const PC: u32 = 0x1000;

fn cpu(code: &[u8]) -> ReferenceInterpreter<Ram> {
    let mut ram = Ram::new(0x0100_0000);
    for (i, b) in code.iter().enumerate() {
        ram.write(Addr::new(PC + i as u32), *b);
    }
    let mut cpu = ReferenceInterpreter::new(ram);
    cpu.registers.set_pc(PC);
    cpu.registers.set_sp(0x8000);
    cpu
}

fn cycles(code: &[u8], setup: impl Fn(&mut ReferenceInterpreter<Ram>)) -> u32 {
    let mut c = cpu(code);
    setup(&mut c);
    c.step().unwrap().0
}

#[test]
fn ld_and_st_extended() {
    // LD D,$002000 / $002001
    assert_eq!(cycles(&[0xC2, 0x00, 0x20, 0x00], |_| {}), 3);
    assert_eq!(cycles(&[0xC2, 0x01, 0x20, 0x00], |_| {}), 4);
    // ST Q,$002001: 4-byte value at an odd address
    assert_eq!(cycles(&[0xCD, 0x01, 0x20, 0x00], |_| {}), 5);
    // LD A,$002001: bytes never pay
    assert_eq!(cycles(&[0xC0, 0x01, 0x20, 0x00], |_| {}), 3);
}

#[test]
fn ld_x_extended_24_bit_never_pays() {
    // 3 bytes take a word and a byte whichever way they're aligned
    assert_eq!(cycles(&[0xCE, 0x00, 0x20, 0x00], |_| {}), 4);
    assert_eq!(cycles(&[0xCE, 0x01, 0x20, 0x00], |_| {}), 4);
}

#[test]
fn direct_page_and_indexed() {
    // LD W,<$11 (odd)
    assert_eq!(cycles(&[0x4B, 0x10], |c| c.registers.set_dp(0x20)), 3);
    assert_eq!(cycles(&[0x4B, 0x11], |c| c.registers.set_dp(0x20)), 4);
    // ST D,1,X with X even -> odd effective address
    assert_eq!(cycles(&[0xD9, 0x09], |c| c.registers.set_x(0x2000)), 4);
}

#[test]
fn alu_memory_operand() {
    // ADD D,$002001
    assert_eq!(cycles(&[0x10, 0xF2, 0x01, 0x01, 0x20, 0x00], |_| {}), 3);
    assert_eq!(cycles(&[0x10, 0xF2, 0x01, 0x00, 0x20, 0x00], |_| {}), 2);
}

#[test]
fn push_and_pop_with_odd_sp() {
    // PUSH D with SP=$8000 lands at $7FFE (even); with SP=$8001 at $7FFF (odd)
    assert_eq!(cycles(&[0x56, 0x02], |_| {}), 2);
    assert_eq!(cycles(&[0x56, 0x02], |c| c.registers.set_sp(0x8001)), 3);
    assert_eq!(cycles(&[0x58, 0x06], |c| c.registers.set_sp(0x7FFD)), 4); // POP Q
}

#[test]
fn block_words_at_odd_addresses() {
    let run = |code: &[u8], x: u32, y: u32| {
        let mut c = cpu(code);
        c.registers.set_x(x);
        c.registers.set_y(y);
        let mut total = 0;
        while c.registers.pc() != PC + code.len() as u32 {
            total += c.step().unwrap().0;
        }
        total
    };
    // MOVW 4 words: aligned 4 cycles; odd source adds 1 per word
    assert_eq!(run(&[0x61, 0x04], 0x2000, 0x3000), 4);
    assert_eq!(run(&[0x61, 0x04], 0x2001, 0x3000), 8);
    // FILL 4 words: aligned 2 cycles; odd destination adds 1 per word
    assert_eq!(run(&[0x62, 0x04, 0x00, 0x00], 0x2000, 0), 2);
    assert_eq!(run(&[0x62, 0x04, 0x00, 0x00], 0x2001, 0), 6);
}
