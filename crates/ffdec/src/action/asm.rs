//! AS1/2 p-code assembler.

use std::sync::Arc;

use super::{simple_name, Action, ActionList, Charset, DefineFunction2, Op, Try, Value};

/// A p-code syntax error on a 1-based line.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

type R<T> = Result<T, ParseError>;

#[derive(Debug, Clone, PartialEq)]
enum Sym {
    Label(String),
    Instr(String),
    BlockStart,
    BlockEnd,
    Comma,
    Eol,
    Eof,
    Comment,
    Str(String),
    Int(i64),
    Float(f32),
    Double(f64),
    Bool(bool),
    Null,
    Undefined,
    Register(u8),
    Constant(u16),
    Ident(String),
}

fn is_letter(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

fn is_letter_digit(c: char) -> bool {
    is_letter(c) || c.is_ascii_digit()
}

struct Lexer {
    s: Vec<char>,
    pos: usize,
    line: usize,
    params: bool,
    pushed: Vec<Sym>,
}

impl Lexer {
    fn new(text: &str) -> Lexer {
        Lexer { s: text.chars().collect(), pos: 0, line: 1, params: false, pushed: Vec::new() }
    }

    fn err<T>(&self, message: impl Into<String>) -> R<T> {
        Err(ParseError { line: self.line, message: message.into() })
    }

    fn at(&self, i: usize) -> Option<char> {
        self.s.get(i).copied()
    }

    fn digits(&self, i: usize) -> usize {
        self.s[i.min(self.s.len())..].iter().take_while(|c| c.is_ascii_digit()).count()
    }

    fn ident_len(&self, i: usize) -> usize {
        match self.at(i) {
            Some(c) if is_letter(c) => 1 + self.s[i + 1..].iter().take_while(|&&c| is_letter_digit(c)).count(),
            _ => 0,
        }
    }

    /// `PositiveNumberLiteral`: `0` or a digit run without a leading zero.
    fn positive(&self, i: usize) -> usize {
        match self.at(i) {
            Some('0') => 1,
            Some(c) if c.is_ascii_digit() => self.digits(i),
            _ => 0,
        }
    }

    fn word(&self, i: usize, w: &str) -> usize {
        let n = w.chars().count();
        if self.s.len() >= i + n && self.s[i..i + n].iter().copied().eq(w.chars()) { n } else { 0 }
    }

    /// `FloatLiteral`.
    fn float(&self, i: usize) -> usize {
        if self.word(i, "NaN") > 0 {
            return 3;
        }
        let neg = (self.at(i) == Some('-')) as usize;
        if self.word(i + neg, "Infinity") > 0 {
            return neg + 8;
        }
        let j = i + neg;
        let int = self.digits(j);
        let mut n = if int > 0 {
            if self.at(j + int) == Some('.') { int + 1 + self.digits(j + int + 1) } else { int }
        } else if self.at(j) == Some('.') && self.digits(j + 1) > 0 {
            1 + self.digits(j + 1)
        } else {
            return 0;
        };
        if matches!(self.at(j + n), Some('e' | 'E')) {
            let sign = matches!(self.at(j + n + 1), Some('+' | '-')) as usize;
            let e = self.digits(j + n + 1 + sign);
            if e > 0 {
                n += 1 + sign + e;
            }
        }
        neg + n
    }

    fn text(&self, i: usize, n: usize) -> String {
        self.s[i..i + n].iter().collect()
    }

    fn lex(&mut self) -> R<Sym> {
        if let Some(s) = self.pushed.pop() {
            return Ok(s);
        }
        loop {
            let i = self.pos;
            let Some(c) = self.at(i) else { return Ok(Sym::Eof) };
            if !self.params {
                if matches!(c, ' ' | '\t' | '\u{c}') {
                    self.pos += 1;
                    continue;
                }
                let n = self.ident_len(i);
                if n > 0 {
                    let name = self.text(i, n);
                    if self.at(i + n) == Some(':') {
                        self.pos += n + 1;
                        return Ok(Sym::Label(name));
                    }
                    self.pos += n;
                    self.params = true;
                    return Ok(Sym::Instr(name));
                }
                self.pos += 1;
                if c == '}' {
                    return Ok(Sym::BlockEnd);
                }
                if c == '\n' || (c == '\r' && self.at(self.pos) != Some('\n')) {
                    self.line += 1;
                }
                continue;
            }
            if c == '"' {
                self.pos += 1;
                return self.string();
            }
            // Longest match wins; on a tie the earlier rule does, as in JFlex.
            let hex = if c == '0' && matches!(self.at(i + 1), Some('x' | 'X')) {
                let h = self.s[i + 2..].iter().take_while(|c| c.is_ascii_hexdigit()).count();
                if h > 0 { 2 + h } else { 0 }
            } else {
                0
            };
            let neg = (c == '-') as usize;
            let num = match self.positive(i + neg) {
                0 => 0,
                p => neg + p,
            };
            let flt = self.float(i);
            let flt_f = if flt > 0 && self.at(i + flt) == Some('f') { flt + 1 } else { 0 };
            let eol = match c {
                '\r' if self.at(i + 1) == Some('\n') => 2,
                '\r' | '\n' => 1,
                _ => 0,
            };
            let comment = if c == ';' { 1 + self.s[i + 1..].iter().take_while(|&&c| c != '\r' && c != '\n').count() } else { 0 };
            let reg = match self.word(i, "register") {
                0 => 0,
                n => match self.positive(i + n) {
                    0 => 0,
                    p => n + p,
                },
            };
            let cst = match self.word(i, "constant") {
                0 => 0,
                n => match self.positive(i + n) {
                    0 => 0,
                    p => n + p,
                },
            };
            let rules = [
                (1, (c == ',') as usize),
                (2, hex),
                (3, num),
                (4, flt_f),
                (5, flt),
                (6, eol),
                (7, comment),
                (8, (c == '{') as usize),
                (9, self.word(i, "true")),
                (10, self.word(i, "false")),
                (11, self.word(i, "null")),
                (12, self.word(i, "undefined")),
                (13, reg),
                (14, cst),
                (15, self.ident_len(i)),
            ];
            let (rule, n) = rules.iter().fold((0, 0), |best, &(r, n)| if n > best.1 { (r, n) } else { best });
            if n == 0 {
                self.pos += 1;
                continue;
            }
            let t = self.text(i, n);
            self.pos += n;
            return Ok(match rule {
                1 => Sym::Comma,
                2 => match i64::from_str_radix(&t[2..], 16) {
                    Ok(v) => Sym::Int(v),
                    Err(_) => return self.err("Hexadecimal integer is too large"),
                },
                3 => match t.parse() {
                    Ok(v) => Sym::Int(v),
                    Err(_) => return self.err(format!("Integer is too large: {t}")),
                },
                4 => Sym::Float(t[..t.len() - 1].parse().unwrap_or(f32::NAN)),
                5 => Sym::Double(t.parse().unwrap_or(f64::NAN)),
                6 => {
                    self.params = false;
                    self.line += 1;
                    Sym::Eol
                }
                7 => Sym::Comment,
                8 => {
                    self.params = false;
                    Sym::BlockStart
                }
                9 => Sym::Bool(true),
                10 => Sym::Bool(false),
                11 => Sym::Null,
                12 => Sym::Undefined,
                13 => match t[8..].parse() {
                    Ok(v) => Sym::Register(v),
                    Err(_) => return self.err(format!("Register number is too large: {t}")),
                },
                14 => match t[8..].parse() {
                    Ok(v) => Sym::Constant(v),
                    Err(_) => return self.err(format!("Constant index is too large: {t}")),
                },
                _ => Sym::Ident(t),
            });
        }
    }

    fn string(&mut self) -> R<Sym> {
        let mut out = String::new();
        let mut repeat = 1;
        loop {
            let Some(c) = self.at(self.pos) else { return Ok(Sym::Eof) };
            self.pos += 1;
            let ch = match c {
                '"' => return Ok(Sym::Str(out)),
                '\r' | '\n' => return self.err("Unterminated string at end of line"),
                '\\' => {
                    let Some(e) = self.at(self.pos) else { return self.err("Illegal escape sequence \"\\\"") };
                    self.pos += 1;
                    match e {
                        'b' => '\u{8}',
                        't' => '\t',
                        'n' => '\n',
                        'f' => '\u{c}',
                        '\u{a7}' => '\u{a7}',
                        'r' => '\r',
                        '"' => '"',
                        '\'' => '\'',
                        '\\' => '\\',
                        'x' | 'u' => {
                            let n = if e == 'x' { 2 } else { 4 };
                            let hex = self.s.get(self.pos..self.pos + n).filter(|h| h.iter().all(char::is_ascii_hexdigit));
                            let Some(h) = hex else { return self.err(format!("Illegal escape sequence \"\\{e}\"")) };
                            let v = u32::from_str_radix(&h.iter().collect::<String>(), 16).unwrap();
                            self.pos += n;
                            char::from_u32(v).unwrap_or('\u{fffd}')
                        }
                        '{' => {
                            let d = self.positive(self.pos);
                            if d == 0 || self.at(self.pos + d) != Some('}') {
                                return self.err("Illegal escape sequence \"\\{\"");
                            }
                            repeat = self.text(self.pos, d).parse().unwrap_or(1);
                            self.pos += d + 1;
                            continue;
                        }
                        other => return self.err(format!("Illegal escape sequence \"\\{other}\"")),
                    }
                }
                c => c,
            };
            for _ in 0..repeat {
                out.push(ch);
            }
            repeat = 1;
        }
    }

    fn pushback(&mut self, s: Sym) {
        self.pushed.push(s);
    }

    fn string_arg(&mut self) -> R<String> {
        match self.lex()? {
            Sym::Str(s) => Ok(s),
            _ => self.err("String expected"),
        }
    }

    fn long(&mut self) -> R<i64> {
        match self.lex()? {
            Sym::Int(v) => Ok(v),
            _ => self.err("Integer expected"),
        }
    }

    fn boolean(&mut self) -> R<bool> {
        match self.lex()? {
            Sym::Bool(v) => Ok(v),
            _ => self.err("Boolean expected"),
        }
    }

    fn ident(&mut self) -> R<String> {
        match self.lex()? {
            Sym::Ident(v) => Ok(v),
            _ => self.err("Identifier expected"),
        }
    }

    fn comma(&mut self) -> R<()> {
        match self.lex()? {
            Sym::Comma => {}
            s => self.pushback(s),
        }
        Ok(())
    }

    fn block_open(&mut self) -> R<()> {
        match self.lex()? {
            Sym::BlockStart => Ok(()),
            _ => self.err("Block start expected"),
        }
    }
}

const NAMED: &[(&str, u8)] = &[
    ("GotoFrame", 0x81),
    ("GetURL", 0x83),
    ("StoreRegister", 0x87),
    ("ConstantPool", 0x88),
    ("StrictMode", 0x89),
    ("WaitForFrame", 0x8a),
    ("SetTarget", 0x8b),
    ("GoToLabel", 0x8c),
    ("WaitForFrame2", 0x8d),
    ("DefineFunction2", 0x8e),
    ("Try", 0x8f),
    ("With", 0x94),
    ("Push", 0x96),
    ("Jump", 0x99),
    ("GetURL2", 0x9a),
    ("DefineFunction", 0x9b),
    ("If", 0x9d),
    ("GotoFrame2", 0x9f),
    ("FFDec_DeobfuscateJump", 0x99),
];

/// A parsed action plus the label its jump names.
struct Parsed {
    action: Action,
    label: Option<String>,
}

fn int<T: TryFrom<i64>>(lx: &Lexer, v: i64, what: &str) -> R<T> {
    T::try_from(v).or_else(|_| lx.err(format!("{what} out of range: {v}")))
}

fn push_value(lx: &mut Lexer, pool: &[String], expected: bool, count: usize) -> R<Option<Value>> {
    Ok(Some(match lx.lex()? {
        Sym::Str(s) => match pool.iter().position(|p| *p == s) {
            Some(i) => Value::Constant(i as u16),
            None => Value::Str(s),
        },
        Sym::Float(f) => Value::Float(f),
        Sym::Double(d) => Value::Double(d),
        Sym::Null => Value::Null,
        Sym::Undefined => Value::Undefined,
        Sym::Register(r) => Value::Register(r),
        Sym::Bool(b) => Value::Bool(b),
        Sym::Int(i) => Value::Int(i),
        Sym::Constant(c) => Value::Constant(c),
        Sym::Eol | Sym::Eof if expected => return lx.err("Value expected"),
        Sym::Eol | Sym::Eof if count == 0 => return lx.err("Arguments expected"),
        Sym::Eol | Sym::Eof => return Ok(None),
        Sym::Comment => return push_value(lx, pool, expected, count),
        s => return lx.err(format!("Arguments expected, {s:?} found")),
    }))
}

fn parse_action(name: &str, lx: &mut Lexer, pool: &[String]) -> R<Parsed> {
    let lower = name.to_ascii_lowercase();
    let simple = (0..=0xffu8).find(|&c| simple_name(c).is_some_and(|n| n.eq_ignore_ascii_case(name)));
    let named = NAMED.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|&(_, c)| c);
    let mut label = None;
    let (code, op) = match (simple, named) {
        (Some(c), _) => (c, Op::Simple),
        _ if lower == "end" => (0, Op::End),
        (None, Some(code)) => {
            let op = match code {
                0x81 => {
                    let v = lx.long()?;
                    Op::GotoFrame(int(lx, v, "frame")?)
                }
                0x83 => {
                    let url = lx.string_arg()?;
                    lx.comma()?;
                    Op::GetUrl { url, target: lx.string_arg()? }
                }
                0x87 => {
                    let v = lx.long()?;
                    Op::StoreRegister(int(lx, v, "register")?)
                }
                0x88 => {
                    let mut out = Vec::new();
                    loop {
                        let mut s = lx.lex()?;
                        let required = !out.is_empty() && s == Sym::Comma;
                        if required {
                            s = lx.lex()?;
                        }
                        match s {
                            Sym::Str(v) => out.push(v),
                            _ if required => return lx.err("String expected"),
                            s => {
                                lx.pushback(s);
                                break;
                            }
                        }
                    }
                    Op::ConstantPool(out)
                }
                0x89 => {
                    let v = lx.long()?;
                    Op::StrictMode(int(lx, v, "mode")?)
                }
                0x8a => {
                    let f = lx.long()?;
                    lx.comma()?;
                    let skip = lx.long()?;
                    Op::WaitForFrame { frame: int(lx, f, "frame")?, skip: int(lx, skip, "skip count")? }
                }
                0x8b => Op::SetTarget(lx.string_arg()?),
                0x8c => Op::GoToLabel(lx.string_arg()?),
                0x8d => {
                    let skip = lx.long()?;
                    Op::WaitForFrame2 { skip: int(lx, skip, "skip count")? }
                }
                0x8e => {
                    let name = lx.string_arg()?;
                    lx.comma()?;
                    let n = lx.long()?;
                    lx.comma()?;
                    let rc = lx.long()?;
                    let mut flags = [false; 9];
                    for f in &mut flags {
                        lx.comma()?;
                        *f = lx.boolean()?;
                    }
                    let mut params = Vec::new();
                    for _ in 0..n {
                        lx.comma()?;
                        let r = lx.long()?;
                        lx.comma()?;
                        params.push((int(lx, r, "register")?, lx.string_arg()?));
                    }
                    lx.block_open()?;
                    let [preload_parent, preload_root, suppress_super, preload_super, suppress_arguments, preload_arguments, suppress_this, preload_this, preload_global] = flags;
                    Op::DefineFunction2(Box::new(DefineFunction2 {
                        name,
                        params,
                        register_count: int(lx, rc, "register count")?,
                        preload_parent,
                        preload_root,
                        suppress_super,
                        preload_super,
                        suppress_arguments,
                        preload_arguments,
                        suppress_this,
                        preload_this,
                        preload_global,
                        code_size: 0,
                    }))
                }
                0x8f => {
                    let mut t = Try { catch_in_register: false, finally_flag: false, catch_flag: false, catch_name: None, catch_register: 0, try_size: 0, catch_size: 0, finally_size: 0 };
                    match lx.lex()? {
                        Sym::Str(s) => {
                            t.catch_name = Some(s);
                            lx.block_open()?;
                        }
                        Sym::Register(r) => {
                            t.catch_register = r;
                            t.catch_in_register = true;
                            lx.block_open()?;
                        }
                        Sym::BlockStart => {}
                        _ => return lx.err("Unknown symbol after Try"),
                    }
                    Op::Try(Box::new(t))
                }
                0x94 => {
                    lx.block_open()?;
                    Op::With { code_size: 0 }
                }
                0x96 => {
                    let mut values = Vec::new();
                    loop {
                        let expected = match lx.lex()? {
                            Sym::Comma => true,
                            s => {
                                lx.pushback(s);
                                false
                            }
                        };
                        match push_value(lx, pool, expected, values.len())? {
                            Some(v) => values.push(v),
                            None => break,
                        }
                    }
                    let pool = (!pool.is_empty()).then(|| Arc::new(pool.to_vec()));
                    Op::Push { values, pool }
                }
                0x99 | 0x9d => {
                    label = Some(lx.ident()?);
                    if code == 0x99 { Op::Jump { offset: 0, deobfuscate: lower.starts_with("ffdec_") } } else { Op::If { offset: 0 } }
                }
                0x9a => {
                    let s = lx.lex()?;
                    let last = matches!(s, Sym::Bool(_));
                    lx.pushback(s);
                    let mut method = 0;
                    if !last {
                        method = lx.long()?;
                        lx.comma()?;
                    }
                    let load_variables = lx.boolean()?;
                    lx.comma()?;
                    let load_target = lx.boolean()?;
                    if last {
                        lx.comma()?;
                        method = lx.long()?;
                    }
                    Op::GetUrl2 { load_variables, load_target, method: int(lx, method, "send vars method")? }
                }
                0x9b => {
                    let name = lx.string_arg()?;
                    lx.comma()?;
                    let n = lx.long()?;
                    let mut params = Vec::new();
                    for _ in 0..n {
                        lx.comma()?;
                        params.push(lx.string_arg()?);
                    }
                    lx.block_open()?;
                    Op::DefineFunction { name, params, code_size: 0 }
                }
                _ => {
                    let scene_bias_flag = lx.boolean()?;
                    lx.comma()?;
                    let play = lx.boolean()?;
                    let mut scene_bias = 0;
                    if scene_bias_flag {
                        lx.comma()?;
                        let v = lx.long()?;
                        scene_bias = int(lx, v, "scene bias")?;
                    }
                    Op::GotoFrame2 { scene_bias_flag, play, scene_bias }
                }
            };
            (code, op)
        }
        _ if lower.len() == 10 && lower.starts_with("unknown_") && u8::from_str_radix(&lower[8..], 16).is_ok() => {
            let code = u8::from_str_radix(&lower[8..], 16).unwrap();
            let mut data = Vec::new();
            let mut s = lx.lex()?;
            while let Sym::Int(v) = s {
                if code < 0x80 {
                    return lx.err("Short actions cannot have data");
                }
                data.push(u8::try_from(v).or_else(|_| lx.err("Byte value expected"))?);
                s = lx.lex()?;
                if s == Sym::Comma {
                    s = lx.lex()?;
                    if !matches!(s, Sym::Int(_)) {
                        return lx.err("Byte value expected");
                    }
                }
            }
            lx.pushback(s);
            (code, Op::Unknown(data))
        }
        _ => return lx.err(format!("Unknown instruction name: {name}")),
    };
    Ok(Parsed { action: Action::new(code, op), label })
}

/// `GraphSourceItemContainer.parseDivision`: sets the size of the block just closed; true
/// when another block of the same container follows.
fn parse_division(a: &mut Action, size: u64, lx: &mut Lexer, cs: Charset) -> R<bool> {
    let header = a.bytes_len(cs);
    let body = size.saturating_sub(header) as u32;
    let Op::Try(t) = &mut a.op else {
        a.set_container_size(0, body as u64);
        return Ok(false);
    };
    match lx.lex()? {
        Sym::Instr(n) if n.eq_ignore_ascii_case("catch") => {
            t.try_size = body;
            t.catch_flag = true;
            lx.block_open()?;
            return Ok(true);
        }
        Sym::Instr(n) if n.eq_ignore_ascii_case("finally") => {
            if t.catch_flag {
                t.catch_size = body.saturating_sub(t.try_size);
            } else {
                t.try_size = body;
            }
            t.finally_flag = true;
            lx.block_open()?;
            return Ok(true);
        }
        s => lx.pushback(s),
    }
    if t.finally_flag {
        t.finally_size = body.saturating_sub(t.try_size + t.catch_size);
    } else if t.catch_flag {
        t.catch_size = body.saturating_sub(t.try_size);
    }
    Ok(false)
}

/// `ASMParser.parse(0, true, text, version, true, ...)`: assembles p-code, dropping `Nop`s.
pub fn parse_pcode(text: &str, version: u8) -> Result<ActionList, ParseError> {
    let cs = Charset::for_version(version);
    let mut pool: Vec<String> = Vec::new();
    {
        let mut lx = Lexer::new(text);
        let mut open: Vec<Action> = Vec::new();
        loop {
            match lx.lex()? {
                Sym::BlockEnd => {
                    let Some(a) = open.last_mut() else { return lx.err("Block end without start") };
                    if !parse_division(a, 0, &mut lx, cs)? {
                        open.pop();
                    }
                }
                Sym::Instr(n) if n.eq_ignore_ascii_case("nop") => {}
                Sym::Instr(n) => {
                    let a = parse_action(&n, &mut lx, &[])?.action;
                    match a.op {
                        Op::ConstantPool(c) => pool.extend(c),
                        _ if a.container_sizes().is_some() => open.push(a),
                        _ => {}
                    }
                }
                Sym::Eof => break,
                _ => {}
            }
        }
    }
    let mut lx = Lexer::new(text);
    let mut actions: Vec<Action> = Vec::new();
    let mut jumps: Vec<(usize, String, usize)> = Vec::new();
    let mut labels: Vec<(String, u64)> = Vec::new();
    let mut containers: Vec<usize> = Vec::new();
    let mut address = 0u64;
    if !pool.is_empty() {
        let mut a = Action::new(0x88, Op::ConstantPool(pool.clone()));
        a.length = a.content_len(cs);
        address += a.total_len();
        actions.push(a);
    }
    loop {
        match lx.lex()? {
            Sym::Label(n) => labels.push((n, address)),
            Sym::Comment | Sym::Eol => {}
            Sym::BlockEnd => {
                let Some(&c) = containers.last() else { return lx.err("Block end without start") };
                let size = address - actions[c].address;
                if !parse_division(&mut actions[c], size, &mut lx, cs)? {
                    containers.pop();
                }
            }
            Sym::Instr(n) if n.eq_ignore_ascii_case("nop") => {}
            Sym::Instr(n) => {
                let line = lx.line;
                let p = parse_action(&n, &mut lx, &pool)?;
                let mut a = p.action;
                if matches!(a.op, Op::ConstantPool(_)) {
                    continue;
                }
                a.length = a.content_len(cs);
                a.address = address;
                address += a.total_len();
                if a.container_sizes().is_some() {
                    containers.push(actions.len());
                }
                if let Some(l) = p.label {
                    jumps.push((actions.len(), l, line));
                }
                actions.push(a);
            }
            Sym::Eof => break,
            s => return lx.err(format!("Label or Instruction name expected, found {s:?}")),
        }
    }
    for (i, name, line) in jumps {
        let Some(&(_, to)) = labels.iter().find(|(n, _)| *n == name) else {
            return Err(ParseError { line, message: format!("Target not found: {name}") });
        };
        let a = &mut actions[i];
        let offset = to as i64 - (a.address + a.total_len()) as i64;
        if !(-0x8000..=0x7fff).contains(&offset) {
            return Err(ParseError { line, message: format!("Jump offset is too large: {offset}") });
        }
        a.set_offset(offset as i32);
    }
    if !actions.last().is_some_and(|a| matches!(a.op, Op::End)) {
        let mut e = Action::new(0, Op::End);
        e.address = address;
        actions.push(e);
    }
    Ok(ActionList { actions, charset: cs, version })
}
