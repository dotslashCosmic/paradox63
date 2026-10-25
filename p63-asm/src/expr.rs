//! Tokens and expressions.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Num(i64),
    Str(String),
    Punct(&'static str),
}

const PUNCT: [&str; 18] = [
    "<<", ">>", "#", ",", "(", ")", "+", "-", "*", "/", "%", "<", ">", "&", "|", "^", "~", ":",
];
const BRACKETS: [&str; 2] = ["[", "]"];

/// Splits one line (comment already removed or not) into tokens.
pub fn lex(line: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = line.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == ';' {
            break;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' || c == '.' {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.') {
                i += 1;
            }
            toks.push(Tok::Ident(chars[start..i].iter().collect()));
            continue;
        }
        if c.is_ascii_digit() || ((c == '$' || c == '%') && i + 1 < chars.len()) {
            let (radix, start) = match c {
                '$' => (16, i + 1),
                '%' => (2, i + 1),
                _ => (10, i),
            };
            let mut j = start;
            while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                j += 1;
            }
            let digits: String = chars[start..j].iter().filter(|&&d| d != '_').collect();
            if radix == 2 && digits.is_empty() {
                // a lone % is the modulo operator
                toks.push(Tok::Punct("%"));
                i += 1;
                continue;
            }
            let v = i64::from_str_radix(&digits, radix)
                .map_err(|_| format!("bad number `{}`", chars[i..j].iter().collect::<String>()))?;
            toks.push(Tok::Num(v));
            i = j;
            continue;
        }
        if c == '\'' {
            if i + 2 < chars.len() && chars[i + 2] == '\'' {
                toks.push(Tok::Num(i64::from(chars[i + 1] as u32)));
                i += 3;
                continue;
            }
            return Err("bad character literal".into());
        }
        if c == '"' {
            let mut j = i + 1;
            let mut s = String::new();
            while j < chars.len() && chars[j] != '"' {
                s.push(chars[j]);
                j += 1;
            }
            if j >= chars.len() {
                return Err("unterminated string".into());
            }
            toks.push(Tok::Str(s));
            i = j + 1;
            continue;
        }
        let rest: String = chars[i..].iter().take(2).collect();
        if let Some(p) = PUNCT.iter().chain(BRACKETS.iter()).find(|p| rest.starts_with(**p)) {
            toks.push(Tok::Punct(p));
            i += p.len();
            continue;
        }
        return Err(format!("unexpected character `{c}`"));
    }
    Ok(toks)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(i64),
    Sym(String),
    Here,
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Bin(&'static str, Box<Expr>, Box<Expr>),
}

/// Why an expression couldn't be evaluated.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    Undefined(String),
    Other(String),
}

pub struct Env<'a> {
    pub symbols: &'a HashMap<String, i64>,
    pub here: i64,
}

impl Expr {
    pub fn eval(&self, env: &Env) -> Result<i64, EvalError> {
        Ok(match self {
            Expr::Num(n) => *n,
            Expr::Here => env.here,
            Expr::Sym(s) => *env.symbols.get(s).ok_or_else(|| EvalError::Undefined(s.clone()))?,
            Expr::Neg(e) => e.eval(env)?.wrapping_neg(),
            Expr::Not(e) => !e.eval(env)?,
            Expr::Bin(op, a, b) => {
                let (a, b) = (a.eval(env)?, b.eval(env)?);
                match *op {
                    "+" => a.wrapping_add(b),
                    "-" => a.wrapping_sub(b),
                    "*" => a.wrapping_mul(b),
                    "/" | "%" if b == 0 => return Err(EvalError::Other("division by zero".into())),
                    "/" => a.wrapping_div(b),
                    "%" => a.wrapping_rem(b),
                    "<<" => a << (b & 63),
                    ">>" => a >> (b & 63),
                    "&" => a & b,
                    "|" => a | b,
                    _ => a ^ b,
                }
            }
        })
    }
}

/// Recursive-descent expression parser over a token slice.
pub struct ExprParser<'a> {
    toks: &'a [Tok],
    pos: usize,
    /// Prefix for `.local` names (the enclosing global label).
    scope: &'a str,
    /// Brackets and prefix operators open right now
    depth: usize,
    /// Binary operators seen so far
    ops: usize,
}

/// Deeper nesting than this is an error rather than a stack overflow.
const MAX_DEPTH: usize = 256;
/// Most binary operators in one expression.
const MAX_OPS: usize = 1000;

const LEVELS: [&[&str]; 5] = [&["|"], &["^"], &["&"], &["<<", ">>"], &["+", "-"]];

impl<'a> ExprParser<'a> {
    pub fn new(toks: &'a [Tok], scope: &'a str) -> Self {
        Self { toks, pos: 0, scope, depth: 0, ops: 0 }
    }

    pub fn done(&self) -> bool {
        self.pos >= self.toks.len()
    }

    fn peek_punct(&self) -> Option<&'static str> {
        match self.toks.get(self.pos) {
            Some(Tok::Punct(p)) => Some(p),
            _ => None,
        }
    }

    pub fn parse(&mut self) -> Result<Expr, String> {
        self.level(0)
    }

    fn level(&mut self, n: usize) -> Result<Expr, String> {
        if n == LEVELS.len() {
            return self.term();
        }
        let mut lhs = self.level(n + 1)?;
        while let Some(p) = self.peek_punct().filter(|p| LEVELS[n].contains(p)) {
            self.pos += 1;
            self.count_op()?;
            let rhs = self.level(n + 1)?;
            lhs = Expr::Bin(p, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn term(&mut self) -> Result<Expr, String> {
        let mut lhs = self.unary()?;
        while let Some(p) = self.peek_punct().filter(|p| ["*", "/", "%"].contains(p)) {
            self.pos += 1;
            self.count_op()?;
            let rhs = self.unary()?;
            lhs = Expr::Bin(p, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// Each binary operator nests the tree one level deeper.
    fn count_op(&mut self) -> Result<(), String> {
        self.ops += 1;
        if self.ops > MAX_OPS {
            return Err("expression too long".into());
        }
        Ok(())
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.depth >= MAX_DEPTH {
            return Err("expression nested too deeply".into());
        }
        self.depth += 1;
        let e = self.unary_inner();
        self.depth -= 1;
        e
    }

    fn unary_inner(&mut self) -> Result<Expr, String> {
        match self.peek_punct() {
            Some("-") => {
                self.pos += 1;
                Ok(Expr::Neg(Box::new(self.unary()?)))
            }
            Some("~") => {
                self.pos += 1;
                Ok(Expr::Not(Box::new(self.unary()?)))
            }
            Some("+") => {
                self.pos += 1;
                self.unary()
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> Result<Expr, String> {
        let tok = self.toks.get(self.pos).cloned().ok_or("expected a value")?;
        self.pos += 1;
        match tok {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Ident(name) => Ok(Expr::Sym(qualify(&name, self.scope))),
            Tok::Punct("*") => Ok(Expr::Here),
            Tok::Punct("(") => {
                let e = self.parse()?;
                if self.peek_punct() != Some(")") {
                    return Err("missing `)`".into());
                }
                self.pos += 1;
                Ok(e)
            }
            Tok::Str(s) if s.chars().count() == 1 => Ok(Expr::Num(i64::from(s.chars().next().unwrap_or('\0') as u32))),
            other => Err(format!("unexpected `{}`", describe(&other))),
        }
    }
}

/// Turns `.LOOP` into `SCOPE.LOOP`; global names are upper-cased as written.
pub fn qualify(name: &str, scope: &str) -> String {
    let upper = name.to_ascii_uppercase();
    if upper.starts_with('.') {
        format!("{scope}{upper}")
    } else {
        upper
    }
}

pub fn describe(t: &Tok) -> String {
    match t {
        Tok::Ident(s) => s.clone(),
        Tok::Num(n) => n.to_string(),
        Tok::Str(s) => format!("\"{s}\""),
        Tok::Punct(p) => (*p).to_string(),
    }
}

/// Parses a whole token slice as one expression.
pub fn parse_expr(toks: &[Tok], scope: &str) -> Result<Expr, String> {
    let mut p = ExprParser::new(toks, scope);
    let e = p.parse()?;
    if !p.done() {
        return Err(format!("unexpected `{}` after expression", describe(&toks[p.pos])));
    }
    Ok(e)
}
