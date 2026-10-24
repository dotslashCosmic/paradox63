//! Table-driven conformance: every opcode in `p63_spec::OPCODES` is checked
//! for legality and base cycle count against the reference interpreter.

use p63_core::{Addr, Bus, CpuError, Ram, ReferenceInterpreter};
use p63_spec::{base_cycles, OPCODES};

const PC: u32 = 0x1000;
const SP: u32 = 0x8000;

#[rustfmt::skip]
const UNIMPLEMENTED: &[u8] = &[
    0x43,
    0x50, 0x51, 0x52, 0x53, 0x54, 0x5A, 0x5B, 0x5D,
    0x60, 0x61, 0x62, 0x63, 0x64,
    0x70, 0x71, 0x72, 0x73, 0x74, 0x75,
    0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98,
    0xB0, 0xB1,
    0xCE, 0xCF,
    0xF0, 0xF1, 0xF2, 0xF3, 0xF4, 0xFE, 0xFF,
];

/// Runs one instruction `op` with an all-zero postbyte and operands.
fn run(op: u8) -> Result<u32, CpuError> {
    let mut ram = Ram::new(0x0100_0000);
    ram.write(Addr::new(PC), op);
    let mut cpu = ReferenceInterpreter::new(ram);
    cpu.registers.set_pc(PC);
    cpu.registers.set_sp(SP);
    cpu.step().map(|c| c.0)
}

/// True when the table's cycle text has a variable part ("1+n", "3+E", "1/byte").
fn is_variable(op: u8) -> bool {
    let t = OPCODES[op as usize].cycles;
    t.contains('+') || t.contains('/')
}

#[test]
fn every_opcode_matches_the_table() {
    let mut failures = Vec::new();
    for info in &OPCODES {
        let op = info.opcode;
        let skipped = UNIMPLEMENTED.contains(&op);
        let res = run(op);
        let name = format!("{op:#04X} {} {}", info.mnemonic, info.operands);

        match (info.legal, skipped, res) {
            // Illegal opcodes must be reported as such.
            (false, _, Err(CpuError::IllegalOpcode(_))) => {}
            (false, _, r) => failures.push(format!("{name}: illegal opcode executed: {r:?}")),

            // Skipped legal opcodes must still be missing; remove them from the list once done.
            (true, true, Err(CpuError::IllegalOpcode(_))) => {}
            (true, true, r) => failures.push(format!("{name}: now implemented ({r:?}); remove from UNIMPLEMENTED")),

            (true, false, Err(e)) => failures.push(format!("{name}: {e}")),
            (true, false, Ok(c)) => {
                let (lo, hi) = base_cycles(op);
                let ok = if is_variable(op) { c >= lo } else { (lo..=hi).contains(&c) };
                if !ok {
                    failures.push(format!("{name}: {c} cycles, table says {:?}", info.cycles));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} conformance failures:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn skip_list_only_names_legal_opcodes() {
    for &op in UNIMPLEMENTED {
        assert!(OPCODES[op as usize].legal, "{op:#04X} is illegal; it doesn't belong in UNIMPLEMENTED");
    }
}
