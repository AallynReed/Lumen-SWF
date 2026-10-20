//! AVM2 p-code assembler: FFDec's `Flasm3Lexer` and `ASM3Parser.parse` for one method.

use std::collections::HashMap;

pub use crate::action::asm::ParseError;

use super::opcodes::{def_in, Dialect, Instruction, Operand};
use super::write::write_u32v;
use super::{method_flags, trait_attr, Abc, Exception, Metadata, Multiname, Namespace, Trait, TraitData, ValueKind};

type R<T> = Result<T, ParseError>;

#[derive(Debug, Clone, PartialEq)]
enum Sym {
    Label(String),
    Instr(String),
    Key(&'static str),
    Str(String),
    Num(String),
    Ident(String),
    Comment,
    Eof,
}

/// Keywords that start a line, and whether they switch to operand lexing.
const LINE_KEYS: &[(&str, bool)] = &[
    ("name", true), ("try", true), ("flag", true), ("param", true), ("paramname", true), ("optional", true),
    ("returns", true), ("body", true), ("maxstack", true), ("localcount", true), ("initscopedepth", true),
    ("maxscopedepth", true), ("code", true), ("trait", true), ("method", true), ("metadata", true), ("item", true),
    ("instance", true), ("extends", true), ("implements", true), ("protectedns", true), ("end", false), ("class", true),
    ("dispid", true), ("slotid", true), ("value", true), ("type", true),
];

const PARAM_KEYS: &[&str] = &[
    "QName", "QNameA", "RTQName", "RTQNameA", "RTQNameL", "RTQNameLA", "Multiname", "MultinameA", "MultinameL",
    "MultinameLA", "TypeName", "Unknown", "null", "(", ")", "[", "]", "<", ">", "Namespace", "PrivateNamespace",
    "PackageNamespace", "PackageInternalNs", "ProtectedNamespace", "ExplicitNamespace", "StaticProtectedNs", ",",
    "from", "to", "target", "name", "type", "end", "slot", "const", "method", "getter", "setter", "class", "function",
    "dispid", "slotid", "value", "flag", "EXPLICIT", "NATIVE", "HAS_OPTIONAL", "HAS_PARAM_NAMES", "IGNORE_REST",
    "NEED_ACTIVATION", "NEED_ARGUMENTS", "NEED_REST", "SET_DXNS", "Integer", "UInteger", "Double", "Decimal", "Float",
    "Float4", "Utf8", "True", "False", "Void", "Undefined", "FINAL", "OVERRIDE", "METADATA", "SEALED", "INTERFACE",
    "PROTECTEDNS", "NON_NULLABLE", "Number", "int", "uint", "NumberContext", "CEILING", "UP", "HALF_UP", "HALF_EVEN",
    "HALF_DOWN", "DOWN", "FLOOR",
];

fn is_letter(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

struct Lexer {
    s: Vec<char>,
    pos: usize,
    line: usize,
    params: bool,
    pushed: Vec<Sym>,
}

impl Lexer {
    fn err<T>(&self, message: impl Into<String>) -> R<T> {
        Err(ParseError { line: self.line, message: message.into() })
    }

    fn at(&self, i: usize) -> Option<char> {
        self.s.get(i).copied()
    }

    fn digits(&self, i: usize) -> usize {
        self.s.get(i..).map_or(0, |t| t.iter().take_while(|c| c.is_ascii_digit()).count())
    }

    fn ident(&self, i: usize) -> usize {
        match self.at(i) {
            Some(c) if is_letter(c) => 1 + self.s[i + 1..].iter().take_while(|&&c| is_letter(c) || c.is_ascii_digit()).count(),
            _ => 0,
        }
    }

    fn word(&self, i: usize, w: &str) -> bool {
        let n = w.chars().count();
        self.s.len() >= i + n && self.s[i..i + n].iter().copied().eq(w.chars())
    }

    fn positive(&self, i: usize) -> usize {
        match self.at(i) {
            Some('0') => 1,
            Some(c) if c.is_ascii_digit() => self.digits(i),
            _ => 0,
        }
    }

    /// `NumberLiteral` or `FloatLiteral`, whichever is longer.
    fn number(&self, i: usize) -> usize {
        let neg = (self.at(i) == Some('-')) as usize;
        let j = i + neg;
        let int = match self.at(j) {
            Some('0') if neg == 0 => 1,
            Some('1'..='9') => self.digits(j),
            _ => 0,
        };
        let int = if int > 0 { neg + int + matches!(self.at(j + int), Some('u' | 'i')) as usize } else { 0 };
        let d = self.digits(j);
        let mut f = if d > 0 {
            if self.at(j + d) == Some('.') { d + 1 + self.digits(j + d + 1) } else { d }
        } else if self.at(j) == Some('.') && self.digits(j + 1) > 0 {
            1 + self.digits(j + 1)
        } else {
            0
        };
        if f > 0 {
            if matches!(self.at(j + f), Some('e' | 'E')) {
                let sign = matches!(self.at(j + f + 1), Some('+' | '-')) as usize;
                let e = self.digits(j + f + 1 + sign);
                if e > 0 {
                    f += 1 + sign + e;
                }
            }
            f += matches!(self.at(j + f), Some('m' | 'd' | 'f')) as usize;
            f += neg;
        }
        int.max(f)
    }

    fn text(&self, i: usize, n: usize) -> String {
        self.s[i..i + n].iter().collect()
    }

    fn newline(&mut self, c: char) {
        if c == '\n' || (c == '\r' && self.at(self.pos) != Some('\n')) {
            self.line += 1;
        }
    }

    fn lex(&mut self) -> R<Sym> {
        if let Some(s) = self.pushed.pop() {
            return Ok(s);
        }
        loop {
            let i = self.pos;
            let Some(c) = self.at(i) else { return Ok(Sym::Eof) };
            if c == ';' {
                self.pos += 1 + self.s[i + 1..].iter().take_while(|&&c| c != '\r' && c != '\n').count();
                return Ok(Sym::Comment);
            }
            if !self.params {
                if matches!(c, ' ' | '\t' | '\u{c}') {
                    self.pos += 1;
                    continue;
                }
                for k in ["exceptionstart ", "exceptionend ", "exceptiontarget "] {
                    if self.word(i, k) {
                        let p = self.positive(i + k.len());
                        if p > 0 && self.at(i + k.len() + p) == Some(':') {
                            return self.err("Exception index syntax is not supported");
                        }
                    }
                }
                let n = self.ident(i);
                if n > 0 {
                    let w = self.text(i, n);
                    if self.at(i + n) == Some(':') {
                        self.pos += n + 1;
                        return Ok(Sym::Label(w));
                    }
                    self.pos += n;
                    if let Some(&(k, params)) = LINE_KEYS.iter().find(|(k, _)| *k == w) {
                        self.params = params;
                        return Ok(Sym::Key(k));
                    }
                    self.params = true;
                    return Ok(Sym::Instr(w));
                }
                self.pos += 1;
                self.newline(c);
                continue;
            }
            if c == '"' {
                self.pos += 1;
                return self.string();
            }
            if c == 'm' && self.at(i + 1) == Some('[') {
                let d = self.digits(i + 2);
                if d > 0 && self.at(i + 2 + d) == Some(']') && self.at(i + 3 + d) == Some('"') {
                    return self.err("Multiname index syntax is not supported");
                }
            }
            if matches!(c, '\r' | '\n') {
                self.pos += 1;
                self.newline(c);
                self.params = false;
                continue;
            }
            if self.word(i, "-Infinity") {
                self.pos += 9;
                return Ok(Sym::Ident("-Infinity".into()));
            }
            let id = self.ident(i);
            let num = self.number(i);
            let key = PARAM_KEYS.iter().filter(|k| self.word(i, k)).map(|k| (k.chars().count(), *k)).max_by_key(|(n, _)| *n);
            let best = id.max(num).max(key.map_or(0, |k| k.0));
            if best == 0 {
                self.pos += 1;
                continue;
            }
            self.pos += best;
            return Ok(match key {
                Some((n, k)) if n == best => Sym::Key(k),
                _ if num == best => Sym::Num(self.text(i, best)),
                _ => Sym::Ident(self.text(i, best)),
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
                '"' => {
                    return Ok(Sym::Str(out));
                }
                '\r' | '\n' => return self.err("Unterminated string at end of line"),
                '\\' => {
                    let Some(e) = self.at(self.pos) else { return self.err("Illegal escape sequence") };
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
                        // FFDec prints parameter names unescaped, so keep such backslashes.
                        other => {
                            out.push('\\');
                            other
                        }
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

    fn expect(&mut self, key: &'static str) -> R<()> {
        match self.lex()? {
            Sym::Key(k) if k == key => Ok(()),
            _ => self.err(format!("{key} expected")),
        }
    }

    /// `expectEnd`-style optional trailing comment.
    fn skip_comment(&mut self) -> R<()> {
        match self.lex()? {
            Sym::Comment => Ok(()),
            s => {
                self.pushback(s);
                Ok(())
            }
        }
    }
}

fn integer(lx: &Lexer, s: &Sym, what: &str) -> R<i64> {
    let Sym::Num(n) = s else { return lx.err(format!("{what} expected")) };
    if n.ends_with(['d', 'm', 'f']) || n.contains(['e', 'E', '.']) {
        return lx.err(format!("{what} expected"));
    }
    match n.trim_end_matches(['i', 'u']).parse::<i64>() {
        Ok(v) => Ok(v),
        Err(_) => lx.err(format!("{what} expected")),
    }
}

fn int32(lx: &Lexer, s: &Sym) -> R<i32> {
    let v = integer(lx, s, "integer")?;
    i32::try_from(v).or_else(|_| lx.err("integer expected"))
}

fn uint(lx: &Lexer, s: &Sym) -> R<u32> {
    let v = integer(lx, s, "unsigned integer")?;
    u32::try_from(v).or_else(|_| lx.err("unsigned integer expected"))
}

/// A number's text; also `NaN` and `Infinity`, which FFDec prints but cannot lex.
fn float_text(lx: &Lexer, s: &Sym, what: &str, strip: &[char]) -> R<String> {
    match s {
        Sym::Num(n) => Ok(n.trim_end_matches(strip).to_string()),
        Sym::Ident(n) if matches!(n.as_str(), "NaN" | "Infinity" | "-Infinity") => Ok(n.clone()),
        _ => lx.err(format!("{what} expected")),
    }
}

struct Pool<'a> {
    abc: &'a mut Abc,
}

impl Pool<'_> {
    fn string(&mut self, s: &str) -> usize {
        let c = &mut self.abc.constants;
        if let Some(i) = c.strings.iter().skip(1).position(|x| x == s) {
            return i + 1;
        }
        c.strings.push(s.to_string());
        c.strings.len() - 1
    }

    fn opt_string(&mut self, s: Option<&str>) -> usize {
        s.map_or(0, |s| self.string(s))
    }

    fn find_or_add<T: Clone>(v: &mut Vec<T>, x: T, eq: impl Fn(&T, &T) -> bool) -> usize {
        if v.is_empty() {
            v.push(x.clone());
        }
        if let Some(i) = v.iter().skip(1).position(|y| eq(y, &x)) {
            return i + 1;
        }
        v.push(x);
        v.len() - 1
    }

    fn int(&mut self, v: i32) -> usize {
        Self::find_or_add(&mut self.abc.constants.ints, v, |a, b| a == b)
    }

    fn uint(&mut self, v: u32) -> usize {
        Self::find_or_add(&mut self.abc.constants.uints, v, |a, b| a == b)
    }

    fn double(&mut self, v: f64) -> usize {
        Self::find_or_add(&mut self.abc.constants.doubles, v, |a, b| a.to_bits() == b.to_bits())
    }

    fn float(&mut self, v: f32) -> usize {
        Self::find_or_add(&mut self.abc.constants.floats, v, |a, b| a.to_bits() == b.to_bits())
    }

    fn namespace(&mut self, kind: u8, name: Option<&str>, mut sub: usize) -> usize {
        let name = self.opt_string(name);
        let c = &mut self.abc.constants;
        for (n, ns) in c.namespaces.iter().enumerate().skip(1) {
            if ns.kind == kind && ns.name == name {
                if sub == 0 {
                    return n;
                }
                sub -= 1;
            }
        }
        if c.namespaces.is_empty() {
            c.namespaces.push(Namespace { kind: 0, name: 0 });
        }
        c.namespaces.push(Namespace { kind, name });
        c.namespaces.len() - 1
    }

    fn ns_set(&mut self, set: Vec<usize>) -> usize {
        Self::find_or_add(&mut self.abc.constants.ns_sets, set, |a, b| a == b)
    }

    fn multiname(&mut self, m: Multiname) -> usize {
        let v = &mut self.abc.constants.multinames;
        if let Some(i) = v.iter().skip(1).position(|y| *y == m) {
            return i + 1;
        }
        if v.is_empty() {
            v.push(Multiname::RtqNameL { attr: false });
        }
        v.push(m);
        v.len() - 1
    }
}

fn opt_str(lx: &mut Lexer, what: &str) -> R<Option<String>> {
    match lx.lex()? {
        Sym::Key("null") => Ok(None),
        Sym::Str(s) => Ok(Some(s)),
        _ => lx.err(format!("{what} expected")),
    }
}

fn unknown_index(lx: &mut Lexer) -> R<usize> {
    lx.expect("(")?;
    let s = lx.lex()?;
    let v = uint(lx, &s)?;
    lx.expect(")")?;
    Ok(v as usize)
}

const NS_KINDS: &[(&str, u8)] = &[
    ("Namespace", 0x08),
    ("PrivateNamespace", 0x05),
    ("PackageNamespace", 0x16),
    ("PackageInternalNs", 0x17),
    ("ProtectedNamespace", 0x18),
    ("ExplicitNamespace", 0x19),
    ("StaticProtectedNs", 0x1a),
];

fn namespace(lx: &mut Lexer, p: &mut Pool) -> R<usize> {
    let kind = match lx.lex()? {
        Sym::Key("Unknown") => return unknown_index(lx),
        Sym::Key("null") => return Ok(0),
        Sym::Key(k) => match NS_KINDS.iter().find(|(n, _)| *n == k) {
            Some(&(_, kind)) => kind,
            None => return lx.err("Namespace kind expected"),
        },
        _ => return lx.err("Namespace kind expected"),
    };
    lx.expect("(")?;
    let name = opt_str(lx, "String or null")?;
    let mut sub = 0;
    match lx.lex()? {
        Sym::Key(",") => match lx.lex()? {
            Sym::Str(s) => sub = s.parse().or_else(|_| lx.err("Number expected"))?,
            _ => return lx.err("String expected"),
        },
        s => lx.pushback(s),
    }
    lx.expect(")")?;
    Ok(p.namespace(kind, name.as_deref(), sub))
}

fn namespace_set(lx: &mut Lexer, p: &mut Pool) -> R<usize> {
    match lx.lex()? {
        Sym::Key("Unknown") => return unknown_index(lx),
        Sym::Key("null") => return Ok(0),
        Sym::Key("[") => {}
        _ => return lx.err("[ expected"),
    }
    let mut set = Vec::new();
    let s = lx.lex()?;
    if s != Sym::Key("]") {
        lx.pushback(s);
        loop {
            set.push(namespace(lx, p)?);
            match lx.lex()? {
                Sym::Key(",") => {}
                Sym::Key("]") => break,
                _ => return lx.err("] expected"),
            }
        }
    }
    Ok(p.ns_set(set))
}

fn name_arg(lx: &mut Lexer, p: &mut Pool) -> R<usize> {
    Ok(p.opt_string(opt_str(lx, "String")?.as_deref()))
}

fn multiname(lx: &mut Lexer, p: &mut Pool) -> R<usize> {
    let k = match lx.lex()? {
        Sym::Key("Unknown") => return unknown_index(lx),
        Sym::Key("null") => return Ok(0),
        Sym::Key(k) => k,
        _ => return lx.err("Name expected"),
    };
    let attr = k.ends_with('A') && k != "QName";
    let m = match k {
        "QName" | "QNameA" => {
            lx.expect("(")?;
            let ns = namespace(lx, p)?;
            lx.expect(",")?;
            let name = name_arg(lx, p)?;
            lx.expect(")")?;
            Multiname::QName { attr: k == "QNameA", ns, name }
        }
        "RTQName" | "RTQNameA" => {
            lx.expect("(")?;
            let name = name_arg(lx, p)?;
            lx.expect(")")?;
            Multiname::RtqName { attr, name }
        }
        "RTQNameL" | "RTQNameLA" => {
            lx.expect("(")?;
            lx.expect(")")?;
            Multiname::RtqNameL { attr }
        }
        "Multiname" | "MultinameA" => {
            lx.expect("(")?;
            let name = name_arg(lx, p)?;
            lx.expect(",")?;
            let ns_set = namespace_set(lx, p)?;
            lx.expect(")")?;
            Multiname::Multiname { attr, name, ns_set }
        }
        "MultinameL" | "MultinameLA" => {
            lx.expect("(")?;
            let ns_set = namespace_set(lx, p)?;
            lx.expect(")")?;
            Multiname::MultinameL { attr, ns_set }
        }
        "TypeName" => {
            lx.expect("(")?;
            let name = multiname(lx, p)?;
            lx.expect("<")?;
            let mut params = vec![multiname(lx, p)?];
            loop {
                match lx.lex()? {
                    Sym::Key(",") => params.push(multiname(lx, p)?),
                    Sym::Key(">") => break,
                    _ => return lx.err("> expected"),
                }
            }
            lx.expect(")")?;
            Multiname::TypeName { name, params }
        }
        _ => return lx.err("Name expected"),
    };
    Ok(p.multiname(m))
}

fn empty_parens(lx: &mut Lexer) -> R<()> {
    match lx.lex()? {
        Sym::Key("(") => lx.expect(")"),
        s => {
            lx.pushback(s);
            Ok(())
        }
    }
}

/// `parseValue`.
fn value(lx: &mut Lexer, p: &mut Pool) -> R<ValueKind> {
    let t = lx.lex()?;
    let vk = |kind: u8, index: usize| Ok(ValueKind { index, kind });
    match t {
        Sym::Key(k @ ("Integer" | "UInteger" | "Double" | "Float")) => {
            lx.expect("(")?;
            let v = lx.lex()?;
            let index = if v == Sym::Key("null") {
                0
            } else {
                match k {
                    "Integer" => p.int(int32(lx, &v)?),
                    "UInteger" => p.uint(uint(lx, &v)?),
                    "Double" => p.double(float_text(lx, &v, "double", &['i', 'u', 'd'])?.parse().or_else(|_| lx.err("double expected"))?),
                    _ => p.float(float_text(lx, &v, "float", &['i', 'u', 'f'])?.parse().or_else(|_| lx.err("float expected"))?),
                }
            };
            lx.expect(")")?;
            vk(match k { "Integer" => 0x03, "UInteger" => 0x04, "Double" => 0x06, _ => 0x02 }, index)
        }
        Sym::Key("Decimal") => {
            lx.expect("(")?;
            let s = lx.lex()?;
            let index = uint(lx, &s)? as usize;
            lx.expect(")")?;
            vk(0x02, index)
        }
        Sym::Num(n) => {
            if n.ends_with('m') {
                return lx.err("Decimal values are not supported");
            }
            if n.ends_with('d') || n.contains(['e', 'E', '.']) {
                let d = n.trim_end_matches('d').parse().or_else(|_| lx.err("double expected"))?;
                return vk(0x06, p.double(d));
            }
            let i = n.trim_end_matches('u').trim_end_matches('i').parse().or_else(|_| lx.err("integer expected"))?;
            vk(0x03, p.int(i))
        }
        Sym::Str(s) => vk(0x01, p.string(&s)),
        Sym::Key("Utf8") => {
            lx.expect("(")?;
            let index = match lx.lex()? {
                Sym::Key("null") => 0,
                Sym::Str(s) => {
                    lx.expect(")")?;
                    p.string(&s)
                }
                _ => return lx.err("String or null expected"),
            };
            vk(0x01, index)
        }
        Sym::Key(k @ ("True" | "False" | "null" | "Void" | "Undefined")) => {
            empty_parens(lx)?;
            let kind = match k { "True" => 0x0b, "False" => 0x0a, "null" => 0x0c, _ => 0x00 };
            vk(kind, kind as usize)
        }
        // FFDec prints `Null()`, which its own lexer does not know.
        Sym::Ident(n) if n == "Null" => {
            empty_parens(lx)?;
            vk(0x0c, 0x0c)
        }
        Sym::Key(k) if NS_KINDS.iter().any(|(n, _)| *n == k) => {
            let kind = NS_KINDS.iter().find(|(n, _)| *n == k).unwrap().1;
            lx.expect("(")?;
            let index = namespace(lx, p)?;
            lx.expect(")")?;
            vk(kind, index)
        }
        _ => vk(0, 0),
    }
}

fn number_context(lx: &mut Lexer, s: Sym) -> R<i64> {
    if let Sym::Num(_) = s {
        return integer(lx, &s, "NumberContext");
    }
    if s != Sym::Key("NumberContext") {
        return lx.err("NumberContext expected");
    }
    lx.expect("(")?;
    let usage = match lx.lex()? {
        Sym::Key("Number") => 0,
        Sym::Key("Decimal") => 1,
        Sym::Key("Double") => 2,
        Sym::Key("int") => 3,
        Sym::Key("uint") => 4,
        _ => return lx.err("Usage expected - one of: Number, decimal, double, int or uint"),
    };
    let mut param = usage;
    if usage <= 1 {
        lx.expect(",")?;
        let rounding = ["CEILING", "UP", "HALF_UP", "HALF_EVEN", "HALF_DOWN", "DOWN", "FLOOR"];
        let r = match lx.lex()? {
            Sym::Key(k) => rounding.iter().position(|x| *x == k),
            _ => None,
        };
        let Some(r) = r else { return lx.err("Rounding expected - one of: CEILING, UP, HALF_UP, HALF_EVEN, HALF_DOWN, DOWN, FLOOR") };
        param |= (r as i64) << 3;
        match lx.lex()? {
            Sym::Key(",") => {
                let s = lx.lex()?;
                let precision = uint(lx, &s)? as i64;
                if precision > 34 {
                    return lx.err("Precision must not exceed 34");
                }
                if precision < 34 {
                    param |= precision << 6;
                }
            }
            s => lx.pushback(s),
        }
    }
    lx.expect(")")?;
    Ok(param)
}

/// Trait flags and metadata (`parseTraitParams`).
fn trait_params(lx: &mut Lexer, p: &mut Pool, t: &mut Trait) -> R<()> {
    let mut attrs = 0;
    let mut metadata = Vec::new();
    loop {
        match lx.lex()? {
            Sym::Key("flag") => {
                attrs |= match lx.lex()? {
                    Sym::Key("FINAL") => trait_attr::FINAL,
                    Sym::Key("OVERRIDE") => trait_attr::OVERRIDE,
                    Sym::Key("METADATA") => trait_attr::METADATA,
                    _ => return lx.err("Invalid trait flag"),
                }
            }
            Sym::Key("metadata") => {
                let Sym::Str(name) = lx.lex()? else { return lx.err("string metadata expected") };
                let mut items: Vec<(Option<String>, String)> = Vec::new();
                let mut s = lx.lex()?;
                while s == Sym::Key("item") {
                    let key = opt_str(lx, "string/null value")?;
                    let Sym::Str(v) = lx.lex()? else { return lx.err("string value expected") };
                    items.push((key, v));
                    s = lx.lex()?;
                }
                if s != Sym::Key("end") {
                    return lx.err("end expected");
                }
                lx.skip_comment()?;
                metadata.push((name, items));
            }
            s => {
                lx.pushback(s);
                break;
            }
        }
    }
    t.attrs = attrs;
    if attrs & trait_attr::METADATA != 0 {
        t.metadata = metadata
            .into_iter()
            .map(|(name, items)| {
                let md = Metadata {
                    name: p.string(&name),
                    keys: items.iter().map(|(k, _)| p.opt_string(k.as_deref())).collect(),
                    values: items.iter().map(|(_, v)| p.string(v)).collect(),
                };
                let all = &mut p.abc.metadata;
                match all.iter().position(|m| m.name == md.name && m.keys == md.keys && m.values == md.values) {
                    Some(i) => i,
                    None => {
                        all.push(md);
                        all.len() - 1
                    }
                }
            })
            .collect();
    }
    Ok(())
}

/// A body trait (`parseSlotConst`), after its `trait` keyword.
fn slot_const(lx: &mut Lexer, p: &mut Pool) -> R<Trait> {
    let is_const = match lx.lex()? {
        Sym::Key("slot") => false,
        Sym::Key("const") => true,
        _ => return lx.err("slot or const expected"),
    };
    let name = multiname(lx, p)?;
    let mut t = Trait { name, attrs: 0, data: TraitData::Slot { slot_id: 0, type_name: 0, value: None }, metadata: Vec::new() };
    trait_params(lx, p, &mut t)?;
    lx.expect("slotid")?;
    let s = lx.lex()?;
    let slot_id = int32(lx, &s)? as usize;
    lx.expect("type")?;
    let type_name = multiname(lx, p)?;
    let value = match lx.lex()? {
        Sym::Key("value") => Some(value(lx, p)?),
        s => {
            lx.pushback(s);
            None
        }
    };
    t.data = if is_const { TraitData::Const { slot_id, type_name, value } } else { TraitData::Slot { slot_id, type_name, value } };
    Ok(t)
}

/// A jump or switch operand waiting for its label's offset.
struct Fix {
    ins: usize,
    operand: usize,
    label: String,
    line: usize,
    case: bool,
}

/// Bytes of an instruction, as FFDec's `AVM2Instruction.getBytes`.
pub fn encode(ins: &Instruction) -> Vec<u8> {
    let mut w = crate::io::Writer::new();
    w.u8(ins.code);
    let s24 = |w: &mut crate::io::Writer, v: i64| {
        w.u8(v as u8);
        w.u8((v >> 8) as u8);
        w.u8((v >> 16) as u8);
    };
    for (i, kind) in ins.kinds.iter().enumerate() {
        let v = ins.operands.get(i).copied().unwrap_or(0);
        match kind {
            Operand::Offset | Operand::CaseBaseoffset => s24(&mut w, v),
            Operand::OptS8 | Operand::DebugType | Operand::RegisterIndex => w.u8(v as u8),
            Operand::OptCaseOffsets => {
                write_u32v(&mut w, v as i32 as u32);
                for &o in &ins.operands[i + 1..] {
                    s24(&mut w, o);
                }
                break;
            }
            _ => write_u32v(&mut w, v as i32 as u32),
        }
    }
    w.data
}

/// Where a trait lives, so it can be edited alongside its method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraitLoc {
    Script(usize, usize),
    Instance(usize, usize),
    Class(usize, usize),
}

impl Abc {
    pub fn trait_at(&self, loc: TraitLoc) -> Option<&Trait> {
        match loc {
            TraitLoc::Script(s, t) => self.scripts.get(s)?.traits.get(t),
            TraitLoc::Instance(c, t) => self.instances.get(c)?.traits.get(t),
            TraitLoc::Class(c, t) => self.classes.get(c)?.traits.get(t),
        }
    }

    fn trait_at_mut(&mut self, loc: TraitLoc) -> Option<&mut Trait> {
        match loc {
            TraitLoc::Script(s, t) => self.scripts.get_mut(s)?.traits.get_mut(t),
            TraitLoc::Instance(c, t) => self.instances.get_mut(c)?.traits.get_mut(t),
            TraitLoc::Class(c, t) => self.classes.get_mut(c)?.traits.get_mut(t),
        }
    }
}

/// Replaces `method` (and, given its location, the trait that owns it) with the p-code
/// `text`, adding constants it names that the ABC lacks. On error the ABC is left unchanged.
pub fn assemble_method(abc: &mut Abc, method: usize, owner: Option<TraitLoc>, text: &str) -> R<()> {
    let mut work = abc.clone();
    parse(&mut work, method, owner, text)?;
    *abc = work;
    Ok(())
}

fn parse(abc: &mut Abc, method: usize, owner: Option<TraitLoc>, text: &str) -> R<()> {
    let dialect = Dialect::of(abc.major, abc.minor);
    let body_index = abc.body_of.get(method).copied().flatten();
    let mut tr = owner.and_then(|l| abc.trait_at(l).cloned());
    let mut info = abc.methods.get(method).cloned().ok_or(ParseError { line: 1, message: "no such method".into() })?;
    let mut body = body_index.map(|b| abc.bodies[b].clone());
    info.flags = 0;
    info.name = 0;
    if let Some(b) = &mut body {
        b.traits.clear();
    }
    let mut p = Pool { abc };
    let mut lx = Lexer { s: text.chars().collect(), pos: 0, line: 1, params: false, pushed: Vec::new() };
    let mut code: Vec<Instruction> = Vec::new();
    let mut lines: Vec<usize> = Vec::new();
    let mut offset = 0usize;
    let mut labels: HashMap<String, usize> = HashMap::new();
    let mut fixes: Vec<Fix> = Vec::new();
    let mut tries: Vec<(Exception, [String; 3], usize)> = Vec::new();
    let (mut param_types, mut param_names, mut optional) = (Vec::new(), Vec::new(), Vec::new());
    let mut blocks: Vec<&'static str> = Vec::new();
    loop {
        let s = lx.lex()?;
        match s {
            Sym::Key(k @ ("body" | "code" | "method")) => {
                if body.is_none() && k == "body" {
                    return lx.err("This method cannot have a body.");
                }
                blocks.push(k);
            }
            Sym::Key("trait") => {
                blocks.push("trait");
                if blocks.contains(&"body") {
                    let t = slot_const(&mut lx, &mut p)?;
                    body.as_mut().unwrap().traits.push(t);
                    continue;
                }
                let Some(t) = tr.as_mut() else { return lx.err("No trait expected") };
                let kind = lx.lex()?;
                let name = multiname(&mut lx, &mut p)?;
                t.name = name;
                trait_params(&mut lx, &mut p, t)?;
                match (kind, &mut t.data) {
                    (Sym::Key(k @ ("method" | "getter" | "setter")), TraitData::Method { method, .. } | TraitData::Getter { method, .. } | TraitData::Setter { method, .. }) => {
                        lx.expect("dispid")?;
                        let s = lx.lex()?;
                        let d = uint(&lx, &s)? as usize;
                        let m = *method;
                        t.data = match k {
                            "method" => TraitData::Method { disp_id: d, method: m },
                            "getter" => TraitData::Getter { disp_id: d, method: m },
                            _ => TraitData::Setter { disp_id: d, method: m },
                        };
                    }
                    (Sym::Key("function"), TraitData::Function { slot_id, .. }) => {
                        lx.expect("slotid")?;
                        let s = lx.lex()?;
                        *slot_id = uint(&lx, &s)? as usize;
                    }
                    _ => return lx.err("Unexpected trait type"),
                }
            }
            Sym::Key("name") => info.name = name_arg(&mut lx, &mut p).or_else(|_| lx.err("String or null expected"))?,
            Sym::Key("param") => param_types.push(multiname(&mut lx, &mut p)?),
            Sym::Key("paramname") => param_names.push(p.opt_string(opt_str(&mut lx, "String or null")?.as_deref())),
            Sym::Key("optional") => optional.push(value(&mut lx, &mut p)?),
            Sym::Key(k @ ("maxstack" | "localcount" | "initscopedepth" | "maxscopedepth")) => {
                let s = lx.lex()?;
                let v = uint(&lx, &s)? as usize;
                let Some(b) = body.as_mut() else { return lx.err("This method cannot have a body.") };
                match k {
                    "maxstack" => b.max_stack = v,
                    "localcount" => b.max_regs = v,
                    "initscopedepth" => b.init_scope_depth = v,
                    _ => b.max_scope_depth = v,
                }
            }
            Sym::Key("returns") => info.return_type = multiname(&mut lx, &mut p)?,
            Sym::Key("flag") => {
                info.flags |= match lx.lex()? {
                    Sym::Key("NATIVE" | "EXPLICIT") => method_flags::EXPLICIT,
                    Sym::Key("HAS_OPTIONAL") => method_flags::HAS_OPTIONAL,
                    Sym::Key("HAS_PARAM_NAMES") => method_flags::HAS_PARAM_NAMES,
                    Sym::Key("IGNORE_REST") => method_flags::IGNORE_REST,
                    Sym::Key("NEED_ACTIVATION") => method_flags::NEED_ACTIVATION,
                    Sym::Key("NEED_ARGUMENTS") => method_flags::NEED_ARGUMENTS,
                    Sym::Key("NEED_REST") => method_flags::NEED_REST,
                    Sym::Key("SET_DXNS") => method_flags::SET_DXNS,
                    _ => 0,
                };
            }
            Sym::Key("try") => {
                let line = lx.line;
                let mut names: [String; 3] = Default::default();
                for (i, k) in ["from", "to", "target"].into_iter().enumerate() {
                    lx.expect(k)?;
                    match lx.lex()? {
                        Sym::Ident(n) => names[i] = n,
                        _ => return lx.err("Identifier expected"),
                    }
                }
                lx.expect("type")?;
                let type_name = multiname(&mut lx, &mut p)?;
                lx.expect("name")?;
                let var_name = multiname(&mut lx, &mut p)?;
                match lx.lex()? {
                    Sym::Key("end") => {}
                    s => lx.pushback(s),
                }
                tries.push((Exception { start: 0, end: 0, target: 0, type_name, var_name }, names, line));
            }
            Sym::Eof => break,
            Sym::Comment => {}
            Sym::Key("end") => {
                if blocks.pop().is_none() {
                    return lx.err("End block encountered but there is no block opened");
                }
            }
            Sym::Instr(name) => {
                lines.push(lx.line);
                let name = match name.as_str() {
                    "getlocal_0" => "getlocal0".to_string(),
                    "getlocal_1" => "getlocal1".to_string(),
                    "getlocal_2" => "getlocal2".to_string(),
                    "getlocal_3" => "getlocal3".to_string(),
                    "setlocal_0" => "setlocal0".to_string(),
                    "setlocal_1" => "setlocal1".to_string(),
                    "setlocal_2" => "setlocal2".to_string(),
                    "setlocal_3" => "setlocal3".to_string(),
                    _ => name,
                };
                let Some(d) = (0..=255u8).filter_map(|c| def_in(c, dialect)).find(|d| d.name == name) else {
                    return lx.err(format!("Invalid instruction name: {name}"));
                };
                let mut operands: Vec<i64> = Vec::new();
                for (i, kind) in d.operands.iter().enumerate() {
                    let mut s = lx.lex()?;
                    if i > 0 && s == Sym::Key(",") {
                        s = lx.lex()?;
                    }
                    let constant = matches!(
                        kind,
                        Operand::MultinameIndex | Operand::NamespaceIndex | Operand::StringIndex | Operand::IntIndex | Operand::UintIndex | Operand::DoubleIndex | Operand::DecimalIndex | Operand::FloatIndex | Operand::Float4Index
                    );
                    if constant && s == Sym::Key("Unknown") {
                        operands.push(unknown_index(&mut lx)? as i64);
                        continue;
                    }
                    let null = s == Sym::Key("null");
                    let v: i64 = match kind {
                        Operand::MultinameIndex => {
                            lx.pushback(s);
                            multiname(&mut lx, &mut p)? as i64
                        }
                        Operand::NamespaceIndex => {
                            lx.pushback(s);
                            namespace(&mut lx, &mut p)? as i64
                        }
                        _ if constant && null => 0,
                        Operand::StringIndex => match s {
                            Sym::Str(v) => p.string(&v) as i64,
                            _ => return lx.err("String or null expected"),
                        },
                        Operand::IntIndex => p.int(int32(&lx, &s)?) as i64,
                        Operand::UintIndex => p.uint(uint(&lx, &s)?) as i64,
                        Operand::DoubleIndex => {
                            let t = float_text(&lx, &s, "double", &['i', 'u', 'd'])?;
                            p.double(t.parse().or_else(|_| lx.err("double expected"))?) as i64
                        }
                        Operand::FloatIndex => {
                            let t = float_text(&lx, &s, "float", &['i', 'u', 'f'])?;
                            p.float(t.parse().or_else(|_| lx.err("float expected"))?) as i64
                        }
                        Operand::DecimalIndex | Operand::Float4Index => return lx.err("Decimal and float4 constants are not supported"),
                        Operand::NumberContext => number_context(&mut lx, s)?,
                        Operand::Offset | Operand::CaseBaseoffset => match s {
                            Sym::Ident(l) => {
                                fixes.push(Fix { ins: code.len(), operand: i, label: l, line: lx.line, case: *kind == Operand::CaseBaseoffset });
                                0
                            }
                            _ => return lx.err("Offset expected"),
                        },
                        Operand::OptCaseOffsets => {
                            if s != Sym::Key("[") {
                                return lx.err("Bracket open [ expected");
                            }
                            let mut c = 0;
                            let mut s = lx.lex()?;
                            while let Sym::Ident(l) = s {
                                fixes.push(Fix { ins: code.len(), operand: i + c + 1, label: l, line: lx.line, case: true });
                                c += 1;
                                s = lx.lex()?;
                                if s == Sym::Key("]") {
                                    break;
                                }
                                if s == Sym::Key(",") {
                                    s = lx.lex()?;
                                }
                            }
                            if s != Sym::Key("]") {
                                return lx.err("Bracket close ] expected");
                            }
                            if c == 0 {
                                return lx.err("At least single offset expected");
                            }
                            operands.push(c as i64 - 1);
                            operands.extend(std::iter::repeat(0).take(c));
                            continue;
                        }
                        Operand::OptS8 => {
                            let v = int32(&lx, &s)?;
                            if !(-128..=127).contains(&v) {
                                return lx.err("Byte value expected (-128 to 127). Use pushshort or pushint to push larger values");
                            }
                            v as i64
                        }
                        Operand::OptS16 => {
                            let v = int32(&lx, &s)?;
                            if !(-32768..=32767).contains(&v) {
                                return lx.err("Short value expected (-32768 to 32767). Use pushint to push larger values");
                            }
                            v as i64
                        }
                        _ => int32(&lx, &s)? as i64,
                    };
                    operands.push(v);
                }
                let mut ins = Instruction { code: d.code, name: d.name, offset, len: 0, operands, kinds: d.operands };
                ins.len = encode(&ins).len();
                offset += ins.len;
                code.push(ins);
            }
            Sym::Label(l) => {
                labels.insert(l, offset);
            }
            _ => return lx.err("Unexpected symbol"),
        }
    }
    let find = |l: &str, line: usize, what: &str| labels.get(l).copied().ok_or(ParseError { line, message: format!("Label {l} {what}not defined") });
    let mut exceptions = Vec::new();
    for (mut e, [from, to, target], line) in tries {
        e.start = find(&from, line, "for exception from ")?;
        e.end = find(&to, line, "for exception to ")?;
        e.target = find(&target, line, "for exception target ")?;
        exceptions.push(e);
    }
    for Fix { ins, operand, label, line, case } in fixes {
        let to = find(&label, line, "")? as i64;
        let i = &mut code[ins];
        i.operands[operand] = if case { to - i.offset as i64 } else { to - (i.offset + i.len) as i64 };
    }
    info.param_types = param_types;
    info.param_names = param_names;
    info.optional = if info.flags & method_flags::HAS_OPTIONAL != 0 { optional } else { Vec::new() };
    let abc = p.abc;
    abc.methods[method] = info;
    if let (Some(bi), Some(mut b)) = (body_index, body) {
        b.code = code.iter().flat_map(encode).collect();
        b.exceptions = exceptions;
        abc.bodies[bi] = b;
        super::stats::fit_body(abc, bi).map_err(|(at, message)| ParseError { line: lines[at], message })?;
    }
    if let (Some(loc), Some(t)) = (owner, tr) {
        *abc.trait_at_mut(loc).unwrap() = t;
    }
    Ok(())
}
