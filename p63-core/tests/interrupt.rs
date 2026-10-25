//! Interrupt priority, masking, stacking, RTI and the interrupt opcodes.

use p63_core::{Addr, Bus, Flags, Halt, Ram, ReferenceInterpreter};
use p63_spec::{VEC_FIRQ, VEC_IRQ, VEC_NMI, VEC_SWI};

const PC: u32 = 0x1000;
const SP: u32 = 0x8000;
const NMI_H: u32 = 0x3000;
const FIRQ_H: u32 = 0x4000;
const IRQ_H: u32 = 0x5000;
const SWI_H: u32 = 0x6000;

fn cpu(code: &[u8]) -> ReferenceInterpreter<Ram> {
    let mut ram = Ram::new(0x0100_0000);
    for (i, b) in code.iter().enumerate() {
        ram.write(Addr::new(PC + i as u32), *b);
    }
    ram.write_u24_le(Addr::new(VEC_NMI), NMI_H);
    ram.write_u24_le(Addr::new(VEC_IRQ), IRQ_H);
    ram.write_u24_le(Addr::new(VEC_SWI), SWI_H);
    // FIRQ's top byte is at $000000, which wraps
    ram.write_u24_le(Addr::new(VEC_FIRQ), FIRQ_H);
    for h in [NMI_H, FIRQ_H, IRQ_H, SWI_H] {
        ram.write(Addr::new(h), 0x43); // RTI
    }
    let mut cpu = ReferenceInterpreter::new(ram);
    cpu.registers.set_pc(PC);
    cpu.registers.set_sp(SP);
    cpu
}

fn step(cpu: &mut ReferenceInterpreter<Ram>) -> u32 {
    cpu.step().expect("step").0
}

fn fill_registers(c: &mut ReferenceInterpreter<Ram>) {
    c.registers.set_q(0x1122_3344);
    c.registers.set_gp(0x55);
    c.registers.set_dp(0x66);
    c.registers.set_x(0x77_8899);
    c.registers.set_y(0xAA_BBCC);
}

#[test]
fn irq_stacks_19_bytes_and_rti_restores_everything() {
    let mut c = cpu(&[0x00, 0x00]);
    fill_registers(&mut c);
    c.registers.set_cc(Flags::C | Flags::N);
    let before = c.registers.clone();
    c.set_irq_pending(true);

    assert_eq!(step(&mut c), 5);
    assert_eq!(c.registers.pc(), IRQ_H);
    assert_eq!(c.registers.sp(), SP - 19);
    let cc = c.registers.cc();
    assert!(cc.contains(Flags::I | Flags::E | Flags::C | Flags::N));

    // CC on top, saved with E=1 and the old I=0
    let saved_cc = Flags::from_bits_truncate(c.bus.read(Addr::new(SP - 19)));
    assert_eq!(saved_cc, Flags::E | Flags::C | Flags::N);
    assert_eq!(c.bus.read_u32_le(Addr::new(SP - 18)), 0x1122_3344);
    assert_eq!(c.bus.read_u24_le(Addr::new(SP - 3)), PC, "return address deepest");

    // handler clobbers registers, then RTI
    c.registers.set_q(0);
    c.registers.set_x(0);
    c.set_irq_pending(false);
    assert_eq!(step(&mut c), 5);
    let mut want = before;
    want.set_cc(Flags::E | Flags::C | Flags::N);
    assert_eq!(c.registers, want);
}

#[test]
fn firq_lean_frame_masks_f_and_i() {
    let mut c = cpu(&[0x00, 0x00]);
    fill_registers(&mut c);
    c.set_firq_pending(true);

    assert_eq!(step(&mut c), 3);
    assert_eq!(c.registers.pc(), FIRQ_H, "vector read wraps through $000000");
    assert_eq!(c.registers.sp(), SP - 4);
    assert!(c.registers.cc().contains(Flags::F | Flags::I));
    assert!(!c.registers.cc().contains(Flags::E));

    c.registers.set_x(0);
    c.set_firq_pending(false);
    assert_eq!(step(&mut c), 3);
    assert_eq!(c.registers.pc(), PC);
    assert_eq!(c.registers.sp(), SP);
    assert_eq!(c.registers.x(), 0, "lean RTI leaves other registers alone");
    assert!(!c.registers.cc().contains(Flags::F | Flags::I));
}

#[test]
fn masks_block_irq_and_firq_but_not_nmi() {
    let mut c = cpu(&[0x00, 0x00]);
    c.registers.set_cc(Flags::I | Flags::F);
    c.set_irq_pending(true);
    c.set_firq_pending(true);
    assert_eq!(step(&mut c), 1, "NOP runs, both masked");
    assert_eq!(c.registers.pc(), PC + 2);

    c.set_nmi_pending(true);
    step(&mut c);
    assert_eq!(c.registers.pc(), NMI_H);
    assert!(!c.nmi_pending(), "NMI is edge-triggered");
}

#[test]
fn priority_is_nmi_then_firq_then_irq() {
    let mut c = cpu(&[0x00, 0x00]);
    c.set_irq_pending(true);
    c.set_firq_pending(true);
    c.set_nmi_pending(true);
    step(&mut c);
    assert_eq!(c.registers.pc(), NMI_H);

    let mut c = cpu(&[0x00, 0x00]);
    c.set_irq_pending(true);
    c.set_firq_pending(true);
    step(&mut c);
    assert_eq!(c.registers.pc(), FIRQ_H);
}

#[test]
fn nmi_does_not_nest() {
    let mut c = cpu(&[0x00, 0x00]);
    c.bus.write(Addr::new(NMI_H), 0x00); // NOP at the handler
    c.set_nmi_pending(true);
    step(&mut c);
    c.set_nmi_pending(true);
    step(&mut c);
    assert_eq!(c.registers.pc(), NMI_H + 2, "second NMI waits for RTI");
}

#[test]
fn irq_is_level_triggered_and_refires_after_rti() {
    let mut c = cpu(&[0x00, 0x00]);
    c.set_irq_pending(true);
    step(&mut c); // enter
    step(&mut c); // RTI, line still asserted
    step(&mut c);
    assert_eq!(c.registers.pc(), IRQ_H);
}

#[test]
fn swi_and_interrupt_opcodes_ignore_masks() {
    for (op, handler, frame) in [(0xF3u8, SWI_H, 19), (0xF1, IRQ_H, 19), (0xF0, NMI_H, 19), (0xF2, FIRQ_H, 4)] {
        let mut c = cpu(&[op, 0x00]);
        c.registers.set_cc(Flags::I | Flags::F);
        step(&mut c);
        assert_eq!(c.registers.pc(), handler, "{op:#04X}");
        assert_eq!(c.registers.sp(), SP - frame);
        step(&mut c); // RTI
        assert_eq!(c.registers.pc(), PC + 2, "{op:#04X} returns past itself");
        assert_eq!(c.registers.sp(), SP);
    }
}

#[test]
fn rti_from_a_nested_swi_keeps_the_nmi_guard() {
    let mut c = cpu(&[0x00, 0x00]);
    for (i, b) in [0xF3u8, 0x00, 0x00, 0x00, 0x00, 0x00].iter().enumerate() {
        c.bus.write(Addr::new(NMI_H + i as u32), *b); // SWI, then NOPs
    }
    c.set_nmi_pending(true);
    step(&mut c); // into the NMI handler
    step(&mut c); // SWI
    step(&mut c); // the SWI handler's RTI
    assert_eq!(c.registers.pc(), NMI_H + 2);
    c.set_nmi_pending(true);
    step(&mut c);
    assert_eq!(c.registers.pc(), NMI_H + 4, "still inside the NMI handler: no nesting");
}

#[test]
fn a_handlers_block_op_leaves_the_interrupted_one_alone() {
    const SRC: u32 = 0x2000;
    const DST: u32 = 0x3000;
    let mut c = cpu(&[0x60, 0x08, 0x00, 0x00]); // MOVB 8 bytes
    for i in 0..16 {
        c.bus.write(Addr::new(SRC + i), 0x10 + i as u8);
        c.bus.write(Addr::new(DST + i), 0xEE);
    }
    // the IRQ handler copies 4 bytes of its own: LDI X,$5000 / LDI Y,$6000 / MOVB 4 / RTI
    let handler = [0x5A, 0x08, 0x00, 0x50, 0x00, 0x5A, 0x09, 0x00, 0x60, 0x00, 0x60, 0x04, 0x43, 0x00];
    for (i, b) in handler.iter().enumerate() {
        c.bus.write(Addr::new(IRQ_H + i as u32), *b);
    }
    c.registers.set_x(SRC);
    c.registers.set_y(DST);
    step(&mut c);
    step(&mut c); // two bytes copied
    c.set_irq_pending(true);
    step(&mut c);
    c.set_irq_pending(false);
    for _ in 0..100 {
        if c.registers.pc() == PC + 2 {
            break;
        }
        step(&mut c);
    }
    assert_eq!(c.registers.pc(), PC + 2);
    for i in 0..8 {
        assert_eq!(c.bus.read(Addr::new(DST + i)), 0x10 + i as u8, "byte {i}");
    }
    assert_eq!(c.bus.read(Addr::new(DST + 8)), 0xEE, "nothing copied past the 8 bytes");
}

#[test]
fn push_at_the_bottom_of_memory_wraps() {
    for (code, size) in [([0x56u8, 0x02], 2u32), ([0x56, 0x06], 4), ([0x57, 0x86], 7)] {
        let mut c = cpu(&code);
        c.registers.set_sp(1);
        let cycles = step(&mut c);
        assert!(cycles < 10, "{code:02X?}: {cycles} cycles");
        assert_eq!(c.registers.sp(), (1u32.wrapping_sub(size)) & 0x00FF_FFFF);
    }
}

#[test]
fn unmasked_irq_wakes_wait_into_the_handler() {
    let mut c = cpu(&[0xB0, 0x00]);
    step(&mut c);
    step(&mut c);
    assert_eq!(c.halt(), Halt::Interrupt);
    c.set_irq_pending(true);
    step(&mut c);
    assert_eq!(c.halt(), Halt::Running);
    assert_eq!(c.registers.pc(), IRQ_H);
    assert_eq!(c.bus.read_u24_le(Addr::new(SP - 3)), PC + 2, "returns after WAIT");
}
