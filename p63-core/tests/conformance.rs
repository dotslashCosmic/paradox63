//! Table-driven conformance: every opcode in `p63_spec::OPCODES` is checked
//! for legality and base cycle count against the reference interpreter.

use p63_core::{Addr, Bus, CpuError, Ram, ReferenceInterpreter};
use p63_spec::{base_cycles, OPCODES};

const PC: u32 = 0x1000;
const SP: u32 = 0x8000;

/// Runs one instruction `op` with an all-zero postbyte and operands.
fn run(op: u8) -> Result<u32, CpuError> {
    let mut ram = Ram::new(0x0100_0000);
    ram.write(Addr::new(PC), op);
    let mut cpu = ReferenceInterpreter::new(ram);
    cpu.registers.set_pc(PC);
    cpu.registers.set_sp(SP);
    cpu.step().map(|c| c.0)
}

/// True when the table's cycle text has a variable part ("1+n", "3+E", "1/byte"),
/// or is a prefix ("0"), whose cost is the extended instruction's.
fn is_variable(op: u8) -> bool {
    let t = OPCODES[op as usize].cycles;
    t.contains('+') || t.contains('/') || t == "0"
}

#[test]
fn every_opcode_matches_the_table() {
    let mut failures = Vec::new();
    for info in &OPCODES {
        let op = info.opcode;
        let name = format!("{op:#04X} {} {}", info.mnemonic, info.operands);
        match (info.legal, run(op)) {
            (false, Err(CpuError::IllegalOpcode(_))) => {}
            (false, r) => failures.push(format!("{name}: illegal opcode executed: {r:?}")),
            (true, Err(e)) => failures.push(format!("{name}: {e}")),
            (true, Ok(c)) => {
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
