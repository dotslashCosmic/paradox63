//! Source lines to statements.

use crate::expr::{describe, lex, parse_expr, qualify, Expr, Tok};

/// Register names and their 4-bit codes.
pub const REGISTERS: [(&str, u8); 14] = [
    ("A", 0x0), ("B", 0x1), ("D", 0x2), ("E", 0x3), ("F", 0x4), ("W", 0x5), ("Q", 0x6),
    ("GP", 0x7), ("X", 0x8), ("Y", 0x9), ("SP", 0xA), ("PC", 0xB), ("DP", 0xC), ("CC", 0xD),
];

pub fn register(name: &str) -> Option<u8> {
    let upper = name.to_ascii_uppercase();
    REGISTERS.iter().find(|(n, _)| *n == upper).map(|&(_, c)| c)
}

/// Width in bits of a register code.
pub fn reg_bits(code: u8) -> u32 {
    match code {
        0x2 | 0x5 => 16,
        0x6 => 32,
        0x8..=0xB => 24,
        _ => 8,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Reg(u8),
    Imm(Expr),
    /// `<expr`: direct page offset
    Dp(Expr),
    /// a bare address (extended, or a branch target)
    Addr(Expr),
    /// `offset,X` / `offset,Y`
    Indexed(Expr, u8),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Empty,
    Instr(String, Vec<Operand>),
    Directive(String, Vec<Vec<Tok>>),
    Equ(String, Expr),
    Include(String),
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub label: Option<String>,
    pub kind: Kind,
    /// Global label in force, for `.local` names.
    pub scope: String,
}

/// Splits tokens on top-level commas (not inside parentheses).
fn split_commas(toks: &[Tok]) -> Vec<Vec<Tok>> {
    let mut parts = vec![Vec::new()];
    let mut depth = 0;
    for t in toks {
        match t {
            Tok::Punct("(") => depth += 1,
            Tok::Punct(")") => depth -= 1,
            Tok::Punct(",") if depth == 0 => {
                parts.push(Vec::new());
                continue;
            }
            _ => {}
        }
        parts.last_mut().expect("never empty").push(t.clone());
    }
    if parts.len() == 1 && parts[0].is_empty() {
        parts.clear();
    }
    parts
}

fn operand(toks: &[Tok], scope: &str) -> Result<Operand, String> {
    match toks {
        [] => Err("missing operand".into()),
        [Tok::Ident(name)] if register(name).is_some() => Ok(Operand::Reg(register(name).unwrap_or(0))),
        [Tok::Punct("["), Tok::Ident(name), Tok::Punct("]")] if register(name).is_some() => {
            Ok(Operand::Reg(register(name).unwrap_or(0)))
        }
        [Tok::Punct("#"), rest @ ..] => Ok(Operand::Imm(parse_expr(rest, scope)?)),
        [Tok::Punct("<"), rest @ ..] => Ok(Operand::Dp(parse_expr(rest, scope)?)),
        [Tok::Punct(">"), rest @ ..] => Ok(Operand::Addr(parse_expr(rest, scope)?)),
        _ => Ok(Operand::Addr(parse_expr(toks, scope)?)),
    }
}

/// Parses operands, folding `expr,X` / `expr,Y` into indexed operands.
pub fn operands(toks: &[Tok], scope: &str) -> Result<Vec<Operand>, String> {
    let mut out: Vec<Operand> = Vec::new();
    for part in split_commas(toks) {
        let op = operand(&part, scope)?;
        if let Operand::Reg(base @ (0x8 | 0x9)) = op {
            if let Some(Operand::Addr(_)) = out.last() {
                if let Some(Operand::Addr(off)) = out.pop() {
                    out.push(Operand::Indexed(off, base));
                    continue;
                }
            }
        }
        out.push(op);
    }
    Ok(out)
}

/// Parses one line. `scope` is the global label in force before this line.
pub fn parse_line(line: &str, scope: &str) -> Result<Stmt, String> {
    let toks = lex(line)?;
    let mut rest: &[Tok] = &toks;
    let mut label = None;
    let mut scope = scope.to_string();

    // `NAME:` label
    if let [Tok::Ident(name), Tok::Punct(":"), tail @ ..] = rest {
        let full = qualify(name, &scope);
        if !name.starts_with('.') {
            scope.clone_from(&full);
        }
        label = Some(full);
        rest = tail;
    }

    let kind = match rest {
        [] => Kind::Empty,
        [Tok::Ident(name), Tok::Ident(eq), tail @ ..] if eq.eq_ignore_ascii_case("EQU") => {
            Kind::Equ(qualify(name, &scope), parse_expr(tail, &scope)?)
        }
        [Tok::Ident(word), tail @ ..] => {
            let upper = word.to_ascii_uppercase();
            match upper.as_str() {
                "INCLUDE" | ".INCLUDE" => match tail {
                    [Tok::Str(path)] => Kind::Include(path.clone()),
                    _ => return Err("INCLUDE needs a quoted file name".into()),
                },
                "ORG" => Kind::Directive(".ORG".into(), split_commas(tail)),
                d if d.starts_with('.') => Kind::Directive(upper, split_commas(tail)),
                _ => Kind::Instr(upper, operands(tail, &scope)?),
            }
        }
        [t, ..] => return Err(format!("unexpected `{}` at start of statement", describe(t))),
    };
    Ok(Stmt { label, kind, scope })
}
