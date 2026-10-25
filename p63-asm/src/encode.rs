//! Instruction encoding.

use crate::expr::{Env, EvalError, Expr};
use crate::parse::{reg_bits, Operand};

/// Per-statement size choices that only ever grow between passes.
#[derive(Debug, Clone, Copy, Default)]
pub struct Hint {
    pub long_branch: bool,
    pub disp_class: u8,
}

pub struct Ctx<'a> {
    pub env: Env<'a>,
    /// Undefined symbols and range problems are errors only on the final pass.
    pub final_pass: bool,
}

impl Ctx<'_> {
    fn pc(&self) -> i64 {
        self.env.here
    }

    fn val(&self, e: &Expr) -> Result<i64, String> {
        match e.eval(&self.env) {
            Ok(v) => Ok(v),
            Err(EvalError::Undefined(s)) if !self.final_pass => {
                let _ = s;
                Ok(0)
            }
            Err(EvalError::Undefined(s)) => Err(format!("undefined symbol `{s}`")),
            Err(EvalError::Other(m)) => Err(m),
        }
    }

    /// Checks that `v` fits `bits` (signed or unsigned) on the final pass.
    fn fit(&self, v: i64, bits: u32, what: &str) -> Result<i64, String> {
        let lo = -(1i64 << (bits - 1));
        let hi = (1i64 << bits) - 1;
        if self.final_pass && !(lo..=hi).contains(&v) {
            return Err(format!("{what} {v} doesn't fit in {bits} bits"));
        }
        Ok(v)
    }
}

fn le(v: i64, bytes: u32) -> impl Iterator<Item = u8> {
    (0..bytes).map(move |i| (v >> (8 * i)) as u8)
}

type R = Result<Vec<u8>, String>;

/// Index 0-6 of A B D E F W Q, used by the LD/ST opcode rows.
fn ldst_index(code: u8) -> Option<u8> {
    match code {
        0x0..=0x6 => Some(code),
        _ => None,
    }
}

fn reg(op: &Operand, what: &str) -> Result<u8, String> {
    match op {
        Operand::Reg(c) => Ok(*c),
        _ => Err(format!("{what} must be a register")),
    }
}

/// A register that ALU instructions may write (not PC or CC).
fn dest_reg(op: &Operand) -> Result<u8, String> {
    match reg(op, "destination")? {
        0xB | 0xD => Err("PC and CC can't be an ALU destination".into()),
        c => Ok(c),
    }
}

fn imm(op: &Operand, what: &str) -> Result<Expr, String> {
    match op {
        Operand::Imm(e) => Ok(e.clone()),
        _ => Err(format!("{what} must be an immediate (`#value`)")),
    }
}

fn arity(ops: &[Operand], n: usize, mn: &str) -> Result<(), String> {
    if ops.len() == n {
        Ok(())
    } else {
        Err(format!("{mn} takes {n} operand(s), got {}", ops.len()))
    }
}

/// Indexed postbyte plus displacement, using the smallest form the offset and hint allow.
fn indexed(ctx: &Ctx, off: &Expr, base: u8, hint: &mut Hint) -> R {
    let v = ctx.val(off)?;
    let needed = match v {
        -32..=31 => 0,
        -128..=127 => 1,
        -32768..=32767 => 2,
        _ => 3,
    };
    let class = needed.max(hint.disp_class);
    hint.disp_class = class;
    let r = if base == 0x9 { 0x80 } else { 0x00 };
    Ok(match class {
        0 if v == 0 => vec![r],
        0 => vec![r | 0x40 | (v as u8 & 0x3F)],
        1 => vec![r | 0x01, v as u8],
        2 => [r | 0x02].into_iter().chain(le(v, 2)).collect(),
        _ => [r | 0x03].into_iter().chain(le(v & 0xFF_FFFF, 3)).collect(),
    })
}

/// Memory operand after a 0xF source nibble: mode byte and address bytes.
fn mem_mode(ctx: &Ctx, op: &Operand, hint: &mut Hint) -> R {
    match op {
        Operand::Dp(e) => {
            let off = ctx.val(e)?;
            if ctx.final_pass && !(0..=63).contains(&off) {
                return Err(format!("direct page offset {off} must be 0-63"));
            }
            Ok(vec![0x00, off as u8])
        }
        Operand::Addr(e) => Ok([0x01].into_iter().chain(le(ctx.val(e)?, 3)).collect()),
        Operand::Indexed(off, base) => {
            Ok([0x02].into_iter().chain(indexed(ctx, off, *base, hint)?).collect())
        }
        _ => Err("expected a memory operand".into()),
    }
}

/// ALU source operand: (nibble, bytes after the postbyte).
fn alu_src(ctx: &Ctx, op: &Operand, dest_bits: u32, hint: &mut Hint) -> Result<(u8, Vec<u8>), String> {
    match op {
        Operand::Reg(c) => Ok((*c, vec![])),
        Operand::Imm(e) => {
            let v = ctx.fit(ctx.val(e)?, dest_bits, "immediate")?;
            Ok((0xE, le(v, dest_bits / 8).collect()))
        }
        _ => Ok((0xF, mem_mode(ctx, op, hint)?)),
    }
}

fn rel8(ctx: &Ctx, target: &Expr, end: i64) -> Result<u8, String> {
    let off = ctx.val(target)? - end;
    if ctx.final_pass && !(-128..=127).contains(&off) {
        return Err(format!("branch target is {off} bytes away (limit -128..127)"));
    }
    Ok(off as u8)
}

fn short_branch_opcode(mn: &str) -> Option<u8> {
    let names = [
        "BRA", "BRN", "BEQ", "BNE", "BCC", "BCS", "BHI", "BLS", "BGE", "BLT", "BGT", "BLE", "BMI", "BPL", "BVS",
        "BVC",
    ];
    let mn = match mn {
        "BHS" => "BCC",
        "BLO" => "BCS",
        m => m,
    };
    names.iter().position(|n| *n == mn).map(|i| 0x30 + i as u8)
}

/// Relaxable branch: short form unless the target is out of range.
fn branch(ctx: &Ctx, short_op: u8, long_op: Option<u8>, force_long: bool, target: &Expr, hint: &mut Hint) -> R {
    let pc = ctx.pc();
    let t = ctx.val(target)?;
    let short_off = t - (pc + 2);
    let fits = (-128..=127).contains(&short_off);
    let long = force_long || hint.long_branch || (!fits && long_op.is_some());
    if long {
        let Some(op) = long_op else { return Err("this branch has no long form".into()) };
        hint.long_branch = true;
        let off = t - (pc + 3);
        if ctx.final_pass && !(-32768..=32767).contains(&off) {
            return Err(format!("branch target is {off} bytes away; use JMP"));
        }
        return Ok([op].into_iter().chain(le(off, 2)).collect());
    }
    if ctx.final_pass && !fits {
        return Err(format!("branch target is {short_off} bytes away (limit -128..127)"));
    }
    Ok(vec![short_op, short_off as u8])
}

fn signed_flag(ops: &[Operand], fixed: &[u8], mn: &str, form: &str) -> Result<u8, String> {
    let (regs, signed) = match ops.last() {
        Some(Operand::Addr(Expr::Sym(s))) if s == "S" => (&ops[..ops.len() - 1], 1),
        _ => (ops, 0),
    };
    if regs.is_empty() {
        return Ok(signed);
    }
    let codes: Vec<u8> = regs.iter().map(|o| reg(o, "operand")).collect::<Result<_, _>>()?;
    if codes != fixed {
        return Err(format!("{mn} only works as `{form}`"));
    }
    Ok(signed)
}

/// Block op count: 7-bit short form or 0x80 + 16-bit count.
fn block_count(ctx: &Ctx, op: &Operand) -> R {
    let n = ctx.val(&imm(op, "count")?)?;
    if ctx.final_pass && !(0..=0xFFFF).contains(&n) {
        return Err(format!("block count {n} must be 0-65535"));
    }
    Ok(if (0..=127).contains(&n) { vec![n as u8] } else { [0x80].into_iter().chain(le(n, 2)).collect() })
}

/// Encodes one instruction at `ctx.pc()`.
#[allow(clippy::too_many_lines)] // one arm per mnemonic group
pub fn encode(mn: &str, ops: &[Operand], ctx: &Ctx, hint: &mut Hint) -> R {
    let none = |op: u8, post: u8| -> R {
        arity(ops, 0, mn)?;
        Ok(vec![op, post])
    };
    match mn {
        "NOP" => none(0x00, 0),
        "RTS" => none(0x42, 0),
        "RTI" => none(0x43, 0),
        "CLC" => none(0x70, 0),
        "SEC" => none(0x71, 0),
        "CLV" => none(0x72, 0),
        "SEV" => none(0x73, 0),
        "CLI" => none(0x74, 0),
        "SEI" => none(0x75, 0),
        "WAIT" => none(0xB0, 0),
        "WAITV" => none(0xB1, 0),
        "NMI" => none(0xF0, 0),
        "IRQ" => none(0xF1, 0),
        "FIRQ" => none(0xF2, 0),
        "SWI" => none(0xF3, 0),
        "RESET" => none(0xF4, 0),
        "DAA" => match ops {
            [] => Ok(vec![0x5D, 0x00]),
            [Operand::Addr(Expr::Sym(s))] if s == "SUB" => Ok(vec![0x5D, 0x01]),
            _ => Err("DAA takes no operand, or `SUB` after a subtraction".into()),
        },

        "LD" => {
            arity(ops, 2, mn)?;
            let r = reg(&ops[0], "LD destination")?;
            let bits = reg_bits(r);
            match (&ops[1], ldst_index(r)) {
                (Operand::Imm(e), Some(i)) => {
                    let v = ctx.fit(ctx.val(e)?, bits, "immediate")?;
                    Ok([0x01 + i].into_iter().chain(le(v, bits / 8)).collect())
                }
                (Operand::Imm(_), None) => encode("LDI", ops, ctx, hint),
                (Operand::Reg(_), _) => encode("MOV", ops, ctx, hint),
                (Operand::Dp(e), Some(i)) => Ok(vec![0x46 + i, dp_offset(ctx, e)?]),
                (Operand::Addr(e), Some(i)) => Ok([0xC0 + i].into_iter().chain(le(ctx.val(e)?, 3)).collect()),
                (Operand::Addr(e), None) if r == 0x8 || r == 0x9 => {
                    Ok([0xCE + (r - 0x8)].into_iter().chain(le(ctx.val(e)?, 3)).collect())
                }
                (Operand::Indexed(off, base), Some(i)) => {
                    Ok([0xD0 + i].into_iter().chain(indexed(ctx, off, *base, hint)?).collect())
                }
                _ => Err(format!("no load of that form into {}", reg_name(r))),
            }
        }
        "ST" => {
            arity(ops, 2, mn)?;
            let r = reg(&ops[0], "ST source")?;
            let Some(i) = ldst_index(r) else {
                return Err(format!("ST only stores A B D E F W Q, not {}", reg_name(r)));
            };
            match &ops[1] {
                Operand::Dp(e) => Ok(vec![0x09 + i, dp_offset(ctx, e)?]),
                Operand::Addr(e) => Ok([0xC7 + i].into_iter().chain(le(ctx.val(e)?, 3)).collect()),
                Operand::Indexed(off, base) => {
                    Ok([0xD7 + i].into_iter().chain(indexed(ctx, off, *base, hint)?).collect())
                }
                _ => Err("ST needs a memory destination".into()),
            }
        }
        "LDI" => {
            arity(ops, 2, mn)?;
            let r = dest_reg(&ops[0])?;
            let bits = reg_bits(r);
            let v = ctx.fit(ctx.val(&imm(&ops[1], "LDI value")?)?, bits, "immediate")?;
            Ok([0x5A, r].into_iter().chain(le(v, bits / 8)).collect())
        }
        "MOV" => {
            arity(ops, 2, mn)?;
            let d = reg(&ops[0], "MOV destination")?;
            match &ops[1] {
                Operand::Reg(s) => Ok(vec![0x08, (s << 4) | d]),
                Operand::Imm(_) => encode("LDI", ops, ctx, hint),
                _ => Err("MOV copies a register or an immediate; use LD for memory".into()),
            }
        }
        "EXG" => {
            arity(ops, 2, mn)?;
            Ok(vec![0xA0, (reg(&ops[0], "EXG operand")? << 4) | reg(&ops[1], "EXG operand")?])
        }

        "ADD" | "ADC" | "SUB" | "SBC" | "AND" | "OR" | "XOR" | "CMP" | "BIT" => {
            arity(ops, 2, mn)?;
            let op = match mn {
                "ADD" => 0x10,
                "ADC" => 0x11,
                "SUB" => 0x12,
                "SBC" => 0x13,
                "AND" => 0x14,
                "OR" => 0x15,
                "XOR" => 0x16,
                "CMP" => 0x1B,
                _ => 0x29,
            };
            let d = dest_reg(&ops[0])?;
            let (nib, extra) = alu_src(ctx, &ops[1], reg_bits(d), hint)?;
            Ok([op, (nib << 4) | d].into_iter().chain(extra).collect())
        }
        "NOT" | "NEG" | "CLR" | "TST" | "COM" | "SWAP" => {
            arity(ops, 1, mn)?;
            let op = match mn {
                "NOT" => 0x17,
                "NEG" => 0x1A,
                "CLR" => 0x1F,
                "TST" => 0x28,
                "COM" => 0x2A,
                _ => 0x2B,
            };
            Ok(vec![op, dest_reg(&ops[0])?])
        }
        "INC" | "DEC" | "ASL" | "ASR" | "LSL" | "LSR" | "ROL" | "ROR" | "RCL" | "RCR" => {
            let op = match mn {
                "INC" => 0x18,
                "DEC" => 0x19,
                "ASL" => 0x20,
                "ASR" => 0x21,
                "LSL" => 0x22,
                "LSR" => 0x23,
                "ROL" => 0x24,
                "ROR" => 0x25,
                "RCL" => 0x26,
                _ => 0x27,
            };
            match ops {
                [r] => Ok(vec![op, dest_reg(r)?]),
                [r, n] => {
                    let count = ctx.val(&imm(n, "count")?)?;
                    if ctx.final_pass && !(1..=255).contains(&count) {
                        return Err(format!("count {count} must be 1-255"));
                    }
                    Ok(vec![op, 0xE0 | dest_reg(r)?, count as u8])
                }
                _ => Err(format!("{mn} takes a register and an optional #count")),
            }
        }
        "MUL" => Ok(vec![0x1C, signed_flag(ops, &[0x0, 0x1], mn, "MUL A,B[,S]")?]),
        "DIV" => Ok(vec![0x1D, signed_flag(ops, &[0x2, 0x3], mn, "DIV D,E[,S]")?]),
        "MOD" => Ok(vec![0x1E, signed_flag(ops, &[0x2, 0x3], mn, "MOD D,E[,S]")?]),
        "MULD" => Ok(vec![0x5C, signed_flag(ops, &[0x2, 0x5], mn, "MULD D,W[,S]")?]),
        "MAQ" => {
            if !ops.is_empty() && signed_flag(ops, &[0x2, 0x5], mn, "MAQ D,W")? != 0 {
                return Err("MAQ has no signed form".into());
            }
            Ok(vec![0x2E, 0x00])
        }
        "EXT" | "ZEXT" => {
            arity(ops, 2, mn)?;
            let op = if mn == "EXT" { 0x2C } else { 0x2D };
            Ok(vec![op, (reg(&ops[0], "source")? << 4) | dest_reg(&ops[1])?])
        }
        "ADDZ" => {
            arity(ops, 2, mn)?;
            let Operand::Addr(a) = &ops[0] else { return Err("ADDZ needs an address".into()) };
            let v = ctx.fit(ctx.val(&imm(&ops[1], "ADDZ value")?)?, 8, "immediate")?;
            Ok([0x2F].into_iter().chain(le(ctx.val(a)?, 3)).chain([v as u8]).collect())
        }
        "SMODX" => {
            arity(ops, 2, mn)?;
            if reg(&ops[0], "SMODX destination")? != 0x8 {
                return Err("SMODX always adds to X".into());
            }
            match &ops[1] {
                Operand::Reg(s @ 0x0..=0x7) => Ok(vec![0x55, (s << 4) | 0x8]),
                Operand::Imm(e) => {
                    let v = ctx.fit(ctx.val(e)?, 24, "immediate")?;
                    Ok([0x55, 0xE8].into_iter().chain(le(v, 3)).collect())
                }
                _ => Err("SMODX adds A B D E F W Q GP or #imm to X".into()),
            }
        }
        "PUSH" | "POP" => {
            arity(ops, 1, mn)?;
            Ok(vec![if mn == "PUSH" { 0x56 } else { 0x58 }, reg(&ops[0], "operand")?])
        }
        "PUSH2" | "POP2" => {
            arity(ops, 2, mn)?;
            let op = if mn == "PUSH2" { 0x57 } else { 0x59 };
            Ok(vec![op, (reg(&ops[0], "operand")? << 4) | reg(&ops[1], "operand")?])
        }

        "JMP" | "JSR" => {
            arity(ops, 1, mn)?;
            let Operand::Addr(a) = &ops[0] else { return Err(format!("{mn} needs an address")) };
            Ok([if mn == "JMP" { 0x40 } else { 0x41 }].into_iter().chain(le(ctx.val(a)?, 3)).collect())
        }
        "BSR" | "LBSR" => {
            arity(ops, 1, mn)?;
            let Operand::Addr(t) = &ops[0] else { return Err(format!("{mn} needs a target")) };
            branch(ctx, 0x44, Some(0x4D), mn == "LBSR", t, hint)
        }
        _ if short_branch_opcode(mn).is_some() || mn.starts_with('L') && short_branch_opcode(&mn[1..]).is_some() => {
            arity(ops, 1, mn)?;
            let Operand::Addr(t) = &ops[0] else { return Err(format!("{mn} needs a target")) };
            let force_long = short_branch_opcode(mn).is_none();
            let short = short_branch_opcode(if force_long { &mn[1..] } else { mn }).unwrap_or(0x30);
            let long = match short {
                0x30 => Some(0x45),
                0x31 => None,
                s => Some(0x80 + (s - 0x32)),
            };
            branch(ctx, short, long, force_long, t, hint)
        }

        "BSET" | "BCLR" | "BCHG" | "BTST" | "JBS" | "JBC" => {
            let op = match mn {
                "BSET" => 0x90,
                "BCLR" => 0x91,
                "BCHG" => 0x92,
                "BTST" => 0x93,
                "JBS" => 0x94,
                _ => 0x95,
            };
            let jump = op >= 0x94;
            arity(ops, if jump { 3 } else { 2 }, mn)?;
            let bit = ctx.val(&imm(&ops[1], "bit number")?)?;
            if ctx.final_pass && !(0..=15).contains(&bit) {
                return Err(format!("bit number {bit} must be 0-15"));
            }
            let mut out = match &ops[0] {
                Operand::Reg(r) => vec![op, (dest_reg(&Operand::Reg(*r))? << 4) | bit as u8],
                other => [op, 0xF0 | bit as u8].into_iter().chain(mem_mode(ctx, other, hint)?).collect(),
            };
            if jump {
                let Operand::Addr(t) = &ops[2] else { return Err(format!("{mn} needs a target")) };
                let end = ctx.pc() + out.len() as i64 + 1;
                out.push(rel8(ctx, t, end)?);
            }
            Ok(out)
        }
        "DBNZ" => {
            arity(ops, 2, mn)?;
            let Operand::Addr(t) = &ops[1] else { return Err("DBNZ needs a target".into()) };
            Ok(vec![0x96, dest_reg(&ops[0])?, rel8(ctx, t, ctx.pc() + 3)?])
        }
        "TBZ" | "TBNZ" => {
            arity(ops, 3, mn)?;
            let bit = ctx.val(&imm(&ops[1], "bit number")?)?;
            let Operand::Addr(t) = &ops[2] else { return Err(format!("{mn} needs a target")) };
            let op = if mn == "TBZ" { 0x97 } else { 0x98 };
            Ok(vec![op, (reg(&ops[0], "operand")? << 4) | (bit as u8 & 0x0F), rel8(ctx, t, ctx.pc() + 3)?])
        }

        "PACK" => {
            arity(ops, 3, mn)?;
            let (d, hi, lo) = (reg(&ops[0], "destination")?, reg(&ops[1], "high byte")?, reg(&ops[2], "low byte")?);
            Ok(vec![0x50, (hi << 4) | lo, d])
        }
        "UNP" => {
            arity(ops, 3, mn)?;
            let (hi, lo, s) = (reg(&ops[0], "high byte")?, reg(&ops[1], "low byte")?, reg(&ops[2], "source")?);
            Ok(vec![0x51, (hi << 4) | lo, s])
        }
        "BEXT" | "BINS" => {
            arity(ops, 4, mn)?;
            let (d, s) = (dest_reg(&ops[0])?, reg(&ops[1], "source")?);
            let pos = ctx.val(&imm(&ops[2], "position")?)?;
            let len = ctx.val(&imm(&ops[3], "length")?)?;
            if ctx.final_pass && (!(0..=15).contains(&pos) || !(1..=16).contains(&len)) {
                return Err("field position must be 0-15 and length 1-16".into());
            }
            Ok(vec![if mn == "BEXT" { 0x52 } else { 0x53 }, (s << 4) | d, ((pos as u8) << 4) | (len as u8 - 1)])
        }
        "LEA" => {
            arity(ops, 2, mn)?;
            let d = reg(&ops[0], "LEA destination")?;
            match &ops[1] {
                Operand::Indexed(off, base) if *base == d => {
                    Ok([0x54].into_iter().chain(indexed(ctx, off, *base, hint)?).collect())
                }
                Operand::Indexed(..) => Err("LEA writes back to its base register (`LEA X,n,X`)".into()),
                _ => Err("LEA needs an indexed operand (`LEA X,n,X`)".into()),
            }
        }
        "MOVQ" => {
            arity(ops, 2, mn)?;
            if reg(&ops[0], "MOVQ destination")? != 0x6 {
                return Err("MOVQ always loads Q".into());
            }
            let (nib, extra) = alu_src(ctx, &ops[1], 32, hint)?;
            Ok([0x5B, nib << 4].into_iter().chain(extra).collect())
        }

        "MOVB" | "MOVW" | "CPY" => {
            arity(ops, 3, mn)?;
            if reg(&ops[0], "source")? != 0x8 || reg(&ops[1], "destination")? != 0x9 {
                return Err(format!("{mn} is `{mn} [X],[Y],#count`"));
            }
            let op = match mn {
                "MOVB" => 0x60,
                "MOVW" => 0x61,
                _ => 0x63,
            };
            Ok([op].into_iter().chain(block_count(ctx, &ops[2])?).collect())
        }
        "FILL" | "SCNB" => {
            arity(ops, 3, mn)?;
            if reg(&ops[0], "pointer")? != 0x8 {
                return Err(format!("{mn} is `{mn} [X],#value,#count`"));
            }
            let bits = if mn == "FILL" { 16 } else { 8 };
            let v = ctx.fit(ctx.val(&imm(&ops[1], "value")?)?, bits, "value")?;
            let op = if mn == "FILL" { 0x62 } else { 0x64 };
            Ok([op].into_iter().chain(block_count(ctx, &ops[2])?).chain(le(v, bits / 8)).collect())
        }

        _ => Err(format!("unknown instruction `{mn}`")),
    }
}

fn dp_offset(ctx: &Ctx, e: &Expr) -> Result<u8, String> {
    let off = ctx.val(e)?;
    if ctx.final_pass && !(0..=63).contains(&off) {
        return Err(format!("direct page offset {off} must be 0-63"));
    }
    Ok(off as u8)
}

fn reg_name(code: u8) -> &'static str {
    crate::parse::REGISTERS.iter().find(|(_, c)| *c == code).map_or("?", |(n, _)| n)
}
