//! p63-asm: two-pass assembler for Paradox63 assembly.
//!
//! Lays the program out repeatedly until every label and every size choice
//! (short/long branch, indexed displacement) is stable, then emits bytes.

pub mod encode;
pub mod expr;
pub mod parse;

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};

use encode::{encode, Ctx, Hint};
use expr::{Env, EvalError, Expr, Tok};
use p63_core::{Addr, Bus};
use parse::{parse_line, Kind, Stmt};

/// Where code goes when the source has no ORG.
pub const DEFAULT_ORIGIN: u32 = p63_spec::MAIN_BASE;

const MAX_PASSES: usize = 32;
const MAX_INCLUDE_DEPTH: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsmError {
    pub file: String,
    pub line: usize,
    pub message: String,
}

impl fmt::Display for AsmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.file, self.line, self.message)
    }
}

impl std::error::Error for AsmError {}

/// Assembled output: one segment per ORG, plus the symbol table.
#[derive(Debug, Clone, Default)]
pub struct Program {
    pub segments: Vec<(u32, Vec<u8>)>,
    pub symbols: BTreeMap<String, u32>,
}

impl Program {
    /// Address of a label or EQU, by name (case-insensitive).
    #[must_use]
    pub fn symbol(&self, name: &str) -> Option<u32> {
        self.symbols.get(&name.to_ascii_uppercase()).copied()
    }

    /// One flat image starting at `base`, gaps filled with zeros.
    /// Fails if any code lies below `base`.
    pub fn image_at(&self, base: u32) -> Result<Vec<u8>, String> {
        let mut image = Vec::new();
        for (addr, bytes) in &self.segments {
            if *addr < base {
                return Err(format!("code at ${addr:06X} is below the load address ${base:06X}"));
            }
            let at = (addr - base) as usize;
            if image.len() < at + bytes.len() {
                image.resize(at + bytes.len(), 0);
            }
            image[at..at + bytes.len()].copy_from_slice(bytes);
        }
        Ok(image)
    }

    /// Writes every segment into memory.
    pub fn load(&self, bus: &mut impl Bus) {
        for (addr, bytes) in &self.segments {
            for (i, b) in bytes.iter().enumerate() {
                bus.write(Addr::new(addr + i as u32), *b);
            }
        }
    }
}

struct Line {
    file: String,
    number: usize,
    stmt: Stmt,
}

/// Assembles a file on disk; INCLUDE paths are relative to the including file.
pub fn assemble_file(path: impl AsRef<Path>) -> Result<Program, Vec<AsmError>> {
    let path = path.as_ref();
    let source = std::fs::read_to_string(path).map_err(|e| {
        vec![AsmError { file: path.display().to_string(), line: 0, message: e.to_string() }]
    })?;
    let mut errors = Vec::new();
    let mut lines = Vec::new();
    let mut scope = String::new();
    read_lines(&path.display().to_string(), &source, path.parent(), 0, &mut scope, &mut lines, &mut errors);
    finish(&lines, errors)
}

/// Assembles source text. INCLUDE is resolved relative to the current directory.
pub fn assemble(name: &str, source: &str) -> Result<Program, Vec<AsmError>> {
    let mut errors = Vec::new();
    let mut lines = Vec::new();
    let mut scope = String::new();
    read_lines(name, source, None, 0, &mut scope, &mut lines, &mut errors);
    finish(&lines, errors)
}

fn finish(lines: &[Line], errors: Vec<AsmError>) -> Result<Program, Vec<AsmError>> {
    if !errors.is_empty() {
        return Err(errors);
    }
    let program = run_passes(lines)?;
    Ok(program)
}

#[allow(clippy::too_many_arguments)]
fn read_lines(
    file: &str,
    source: &str,
    dir: Option<&Path>,
    depth: usize,
    scope: &mut String,
    out: &mut Vec<Line>,
    errors: &mut Vec<AsmError>,
) {
    for (i, text) in source.lines().enumerate() {
        let number = i + 1;
        let err = |message: String| AsmError { file: file.to_string(), line: number, message };
        match parse_line(text, scope) {
            Err(m) => errors.push(err(m)),
            Ok(stmt) => {
                scope.clone_from(&stmt.scope);
                if let Kind::Include(name) = &stmt.kind {
                    if depth >= MAX_INCLUDE_DEPTH {
                        errors.push(err("INCLUDE nested too deeply".into()));
                        continue;
                    }
                    let path: PathBuf = dir.map_or_else(|| PathBuf::from(name), |d| d.join(name));
                    match std::fs::read_to_string(&path) {
                        Ok(text) => {
                            let label = path.display().to_string();
                            read_lines(&label, &text, path.parent(), depth + 1, scope, out, errors);
                        }
                        Err(e) => errors.push(err(format!("can't read `{}`: {e}", path.display()))),
                    }
                }
                out.push(Line { file: file.to_string(), number, stmt });
            }
        }
    }
}

struct Layout {
    symbols: HashMap<String, i64>,
    segments: Vec<(u32, Vec<u8>)>,
    errors: Vec<AsmError>,
}

fn run_passes(lines: &[Line]) -> Result<Program, Vec<AsmError>> {
    let mut hints = vec![Hint::default(); lines.len()];
    let mut symbols = HashMap::new();
    let mut settled = false;
    for _ in 0..MAX_PASSES {
        let before: Vec<(bool, u8)> = hints.iter().map(|h| (h.long_branch, h.disp_class)).collect();
        let layout = lay_out(lines, &mut hints, &symbols, false);
        let after: Vec<(bool, u8)> = hints.iter().map(|h| (h.long_branch, h.disp_class)).collect();
        settled = layout.symbols == symbols && before == after;
        symbols = layout.symbols;
        if settled {
            break;
        }
    }
    let layout = lay_out(lines, &mut hints, &symbols, true);
    if !layout.errors.is_empty() {
        return Err(layout.errors);
    }
    if !settled || layout.symbols != symbols {
        // labels would point at the wrong places
        let (file, line) = lines.first().map_or((String::new(), 0), |l| (l.file.clone(), l.number));
        return Err(vec![AsmError { file, line, message: format!("layout didn't settle in {MAX_PASSES} passes") }]);
    }
    let symbols = layout.symbols.into_iter().map(|(k, v)| (k, (v & 0xFF_FFFF) as u32)).collect();
    Ok(Program { segments: layout.segments.into_iter().filter(|(_, b)| !b.is_empty()).collect(), symbols })
}

/// One pass over the program. `known` holds values from the previous pass.
fn lay_out(lines: &[Line], hints: &mut [Hint], known: &HashMap<String, i64>, final_pass: bool) -> Layout {
    let mut symbols = known.clone();
    let mut defined: HashMap<String, (String, usize)> = HashMap::new();
    let mut segments: Vec<(u32, Vec<u8>)> = vec![(DEFAULT_ORIGIN, Vec::new())];
    let mut pc = i64::from(DEFAULT_ORIGIN);
    let mut errors = Vec::new();

    for (line, hint) in lines.iter().zip(hints.iter_mut()) {
        let mut fail = |message: String| {
            if final_pass {
                errors.push(AsmError { file: line.file.clone(), line: line.number, message });
            }
        };
        if let Some(label) = &line.stmt.label {
            if let Some(m) = define(&mut symbols, &mut defined, line, label, pc) {
                fail(m);
            }
        }
        let env_symbols = symbols.clone();
        let env = Env { symbols: &env_symbols, here: pc };
        let eval = |e: &Expr| -> Result<i64, String> {
            match e.eval(&env) {
                Ok(v) => Ok(v),
                Err(EvalError::Undefined(s)) => Err(format!("undefined symbol `{s}`")),
                Err(EvalError::Other(m)) => Err(m),
            }
        };

        let bytes: Vec<u8> = match &line.stmt.kind {
            Kind::Empty | Kind::Include(_) => continue,
            Kind::Equ(name, e) => {
                match eval(e) {
                    Ok(v) => {
                        if let Some(m) = define(&mut symbols, &mut defined, line, name, v) {
                            fail(m);
                        }
                    }
                    Err(m) => fail(m),
                }
                continue;
            }
            Kind::Directive(name, args) if name == ".ORG" => {
                match args.first().map(|a| expr::parse_expr(a, &line.stmt.scope)).transpose() {
                    Ok(Some(e)) => match eval(&e) {
                        Ok(v) => {
                            pc = v;
                            segments.push(((v & 0xFF_FFFF) as u32, Vec::new()));
                        }
                        Err(m) => fail(m),
                    },
                    Ok(None) => fail("ORG needs an address".into()),
                    Err(m) => fail(m),
                }
                continue;
            }
            Kind::Directive(name, args) => match data(name, args, &line.stmt.scope, &eval, final_pass) {
                Ok(b) => b,
                Err(m) => {
                    fail(m);
                    continue;
                }
            },
            Kind::Instr(mn, ops) => {
                let ctx = Ctx { env: Env { symbols: &env_symbols, here: pc }, final_pass };
                match encode(mn, ops, &ctx, hint) {
                    Ok(b) => b,
                    Err(m) => {
                        fail(m);
                        continue;
                    }
                }
            }
        };
        pc += bytes.len() as i64;
        segments.last_mut().expect("always a segment").1.extend(bytes);
    }
    Layout { symbols, segments, errors }
}

/// Records a symbol; returns an error message if it was already defined this pass.
fn define(
    symbols: &mut HashMap<String, i64>,
    defined: &mut HashMap<String, (String, usize)>,
    line: &Line,
    name: &str,
    value: i64,
) -> Option<String> {
    let dup = defined.get(name).map(|(file, n)| format!("`{name}` is already defined at {file}:{n}"));
    defined.insert(name.to_string(), (line.file.clone(), line.number));
    symbols.insert(name.to_string(), value);
    dup
}

/// Data directives.
fn data(
    name: &str,
    args: &[Vec<Tok>],
    scope: &str,
    eval: &dyn Fn(&Expr) -> Result<i64, String>,
    final_pass: bool,
) -> Result<Vec<u8>, String> {
    let width = match name {
        ".BYTE" | ".DB" => 1,
        ".WORD" | ".DW" => 2,
        ".ADDR" => 3,
        ".LONG" | ".DL" => 4,
        ".DS" | ".RES" => {
            let count = args.first().ok_or(".DS needs a size")?;
            let n = value(&expr::parse_expr(count, scope)?, eval, final_pass)?;
            let fill = match args.get(1) {
                Some(f) => value(&expr::parse_expr(f, scope)?, eval, final_pass)? as u8,
                None => 0,
            };
            if !(0..=0x100_0000).contains(&n) {
                return Err(format!(".DS size {n} is out of range"));
            }
            return Ok(vec![fill; n as usize]);
        }
        _ => return Err(format!("unknown directive `{name}`")),
    };
    let mut out = Vec::new();
    for arg in args {
        if let [Tok::Str(s)] = arg.as_slice() {
            if width != 1 {
                return Err(format!("strings only go in .BYTE, not {name}"));
            }
            out.extend(s.bytes());
            continue;
        }
        let v = value(&expr::parse_expr(arg, scope)?, eval, final_pass)?;
        let bits = width * 8;
        if final_pass && !(-(1i64 << (bits - 1))..(1i64 << bits)).contains(&v) {
            return Err(format!("{v} doesn't fit in {name}"));
        }
        out.extend((0..width).map(|i| (v >> (8 * i)) as u8));
    }
    Ok(out)
}

fn value(e: &Expr, eval: &dyn Fn(&Expr) -> Result<i64, String>, final_pass: bool) -> Result<i64, String> {
    match eval(e) {
        Ok(v) => Ok(v),
        Err(_) if !final_pass => Ok(0),
        Err(m) => Err(m),
    }
}
