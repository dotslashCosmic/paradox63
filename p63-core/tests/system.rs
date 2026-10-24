//! Flag ops, LD X/Y extended, WAIT/WAITV, RESET and the prefix bytes.

use p63_core::{Addr, Bus, Flags, Halt, Ram, ReferenceInterpreter};

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
fn flag_ops_set_and_clear_one_flag() {
    for (op, flag, on) in [
        (0x70, Flags::C, false),
        (0x71, Flags::C, true),
        (0x72, Flags::V, false),
        (0x73, Flags::V, true),
        (0x74, Flags::I, false),
        (0x75, Flags::I, true),
    ] {
        let start = if on { Flags::N } else { Flags::all() };
        let mut c = cpu(&[op, 0x00]);
        c.registers.set_cc(start);
        assert_eq!(step(&mut c), 1);
        let mut want = start;
        want.set(flag, on);
        assert_eq!(c.registers.cc(), want, "{op:#04X}");
    }
}

#[test]
fn ld_x_and_y_extended() {
    let mut c = cpu(&[0xCE, 0x00, 0x20, 0x00, 0xCF, 0x03, 0x20, 0x00]);
    c.bus.write_u24_le(Addr::new(0x2000), 0x80_1234);
    c.bus.write_u24_le(Addr::new(0x2003), 0);
    assert_eq!(step(&mut c), 4);
    assert_eq!(c.registers.x(), 0x80_1234);
    assert!(c.registers.cc().contains(Flags::N));
    assert_eq!(step(&mut c), 4);
    assert_eq!(c.registers.y(), 0);
    assert!(c.registers.cc().contains(Flags::Z));
    assert_eq!(c.registers.pc(), PC + 8);
}

#[test]
fn wait_idles_until_an_interrupt_is_pending() {
    let mut c = cpu(&[0xB0, 0x00, 0x71, 0x00]); // WAIT, SEC
    c.registers.set_cc(Flags::I); // masked IRQ wakes WAIT without being taken
    assert_eq!(step(&mut c), 2);
    assert_eq!(c.halt(), Halt::Interrupt);
    for _ in 0..5 {
        assert_eq!(step(&mut c), 1);
        assert_eq!(c.registers.pc(), PC + 2, "halted CPU doesn't fetch");
    }
    c.set_irq_pending(true);
    step(&mut c);
    assert_eq!(c.halt(), Halt::Running);
    assert!(c.registers.cc().contains(Flags::C), "SEC ran after waking");
}

#[test]
fn waitv_ignores_interrupts_and_wakes_on_vblank() {
    let mut c = cpu(&[0xB1, 0x00, 0x71, 0x00]); // WAITV, SEC
    c.registers.set_cc(Flags::I);
    step(&mut c);
    c.set_irq_pending(true);
    step(&mut c);
    assert_eq!(c.halt(), Halt::VBlank);
    c.signal_vblank();
    step(&mut c);
    assert!(c.registers.cc().contains(Flags::C));
}

#[test]
fn reset_instruction_loads_reset_vector() {
    let mut c = cpu(&[0xF4, 0x00]);
    for (i, b) in p63_spec::LOW_VECTOR_ROM.iter().enumerate() {
        c.bus.write(Addr::new(i as u32), *b);
    }
    c.registers.set_a(0x55);
    assert_eq!(step(&mut c), 2);
    assert_eq!(c.registers.a(), 0);
    assert_eq!(c.registers.sp(), p63_spec::SP_RESET);
    assert_eq!(c.registers.cc(), Flags::F | Flags::I);
    assert_eq!(c.registers.pc(), c.bus.read_u24_le(Addr::new(p63_spec::VEC_RESET)));
}

#[test]
fn prefix_skips_three_bytes_as_a_nop() {
    for prefix in [0xFE, 0xFF] {
        let mut c = cpu(&[prefix, 0x10, 0x00]);
        c.registers.set_a(7);
        assert_eq!(step(&mut c), 1);
        assert_eq!(c.registers.pc(), PC + 3);
        assert_eq!(c.registers.a(), 7);
    }
}
