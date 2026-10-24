//! AS3 lexer with longest-match rules and XML states.

use super::CompileError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Keyword,
    Identifier,
    GlobalConst,
    Operator,
    String,
    Integer,
    Double,
    Decimal,
    Float,
    Float4,
    Preprocessor,
    TypeName,
    Xml,
    Regexp,
    NamespaceSuffix,
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum T {
    Break, Case, Continue, Default, Do, While, Else, For, Each, In, If, Return, Super, Switch, Throw, Try,
    Catch, Finally, With, Dynamic, Internal, Override, Private, Protected, Public, Static, Class, Const,
    Extends, Function, Get, Implements, Interface, Namespace, Package, Set, Var, Import, Use, False, Null,
    This, True, ParentOpen, ParentClose, CurlyOpen, CurlyClose, BracketOpen, BracketClose, Semicolon, Comma,
    Rest, Dot, Assign, GreaterThan, LowerThan, Not, Negate, Ternar, Colon, Equals, StrictEquals, LowerEqual,
    GreaterEqual, NotEqual, StrictNotEqual, And, Or, Increment, Decrement, Plus, Minus, Multiply, Divide,
    BitAnd, BitOr, Xor, Modulo, ShiftLeft, ShiftRight, UShiftRight, AssignPlus, AssignMinus, AssignMultiply,
    AssignDivide, AssignBitAnd, AssignBitOr, AssignXor, AssignModulo, AssignShiftLeft, AssignShiftRight,
    AssignUShiftRight, AssignAnd, AssignOr, As, Delete, InstanceOf, Is, NamespaceOp, New, TypeOf, Void,
    Attribute, String, Identifier, Integer, Double, Decimal, Float, Float4, TypeName, Eof, Infinity,
    Undefined, Final, XmlStartTagBegin, XmlStartVarTagBegin, XmlStartTagEnd, XmlFinishVarTagBegin,
    XmlFinishTag, XmlStartFinishTagEnd, XmlComment, XmlCData, XmlInstr, XmlVarBegin, XmlAttributeName,
    XmlAttributeValue, XmlAttributeValueSingleQuoted, XmlText, XmlAttrNameVarBegin, XmlAttrValVarBegin,
    XmlWhitespace, Filter, Descendants, Native, Preprocessor, Regexp, NamespaceSuffix, NullDot, NullCoalesce,
}

pub const PRIMARY: i32 = 0;
pub const POSTFIX: i32 = 1;
pub const UNARY: i32 = 2;
pub const MULTIPLICATIVE: i32 = 3;
pub const ADDITIVE: i32 = 4;
pub const SHIFT: i32 = 5;
pub const RELATIONAL: i32 = 6;
pub const EQUALITY: i32 = 7;
pub const BITAND: i32 = 8;
pub const BITXOR: i32 = 9;
pub const BITOR: i32 = 10;
pub const LOGICALAND: i32 = 11;
pub const LOGICALOR: i32 = 12;
pub const CONDITIONAL: i32 = 14;
pub const ASSIGNMENT: i32 = 15;
pub const COMMA: i32 = 16;
pub const NOPRECEDENCE: i32 = 17;

impl T {
    /// `SymbolType`: (precedence, binary, right associative).
    pub fn info(self) -> (i32, bool, bool) {
        use T::*;
        match self {
            In | GreaterThan | LowerThan | LowerEqual | GreaterEqual | As | InstanceOf | Is => (RELATIONAL, true, false),
            Comma => (COMMA, false, false),
            Assign | AssignPlus | AssignMinus | AssignMultiply | AssignDivide | AssignBitAnd | AssignBitOr | AssignXor | AssignModulo
            | AssignShiftLeft | AssignShiftRight | AssignUShiftRight | AssignAnd | AssignOr => (ASSIGNMENT, true, true),
            Not | Negate | Delete | TypeOf => (UNARY, false, false),
            Ternar => (CONDITIONAL, true, true),
            Colon => (CONDITIONAL, false, false),
            Equals | StrictEquals | NotEqual | StrictNotEqual => (EQUALITY, true, false),
            And => (LOGICALAND, true, false),
            Or => (LOGICALOR, true, false),
            Increment | Decrement => (POSTFIX, false, false),
            Plus | Minus => (ADDITIVE, true, false),
            Multiply | Divide | Modulo => (MULTIPLICATIVE, true, false),
            BitAnd => (BITAND, true, false),
            BitOr => (BITOR, true, false),
            Xor => (BITXOR, true, false),
            ShiftLeft | ShiftRight | UShiftRight => (SHIFT, true, false),
            NullCoalesce => (PRIMARY, true, true),
            Super | Function | False | Null | This | True | ParentOpen | ParentClose | CurlyOpen | CurlyClose | BracketOpen | BracketClose | Dot
            | NamespaceOp | New | String | Identifier | Integer | Double | Decimal | Float | Float4 | TypeName | Infinity | Undefined
            | XmlStartTagBegin | XmlStartVarTagBegin | XmlStartTagEnd | XmlFinishVarTagBegin | XmlFinishTag | XmlStartFinishTagEnd | XmlComment
            | XmlCData | XmlInstr | XmlVarBegin | XmlAttributeName | XmlAttributeValue | XmlAttributeValueSingleQuoted | XmlText
            | XmlAttrNameVarBegin | XmlAttrValVarBegin | XmlWhitespace | Filter | Descendants | Preprocessor | Regexp | NullDot => (PRIMARY, false, false),
            _ => (NOPRECEDENCE, false, false),
        }
    }

    pub fn precedence(self) -> i32 {
        self.info().0
    }

    pub fn is_binary(self) -> bool {
        self.info().1
    }

    pub fn is_right_associative(self) -> bool {
        self.info().2
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum V {
    None,
    Str(String),
    Int(i32),
    Double(f64),
    Float(f32),
    Float4([f32; 4]),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sym {
    pub pos: usize,
    pub group: Group,
    pub t: T,
    pub v: V,
}

impl Sym {
    pub fn text(&self) -> String {
        match &self.v {
            V::Str(s) => s.clone(),
            V::Int(i) => i.to_string(),
            V::Double(d) => d.to_string(),
            V::Float(f) => f.to_string(),
            V::Float4(f) => format!("{f:?}"),
            V::None => String::new(),
        }
    }

    pub fn is(&self, ts: &[T]) -> bool {
        ts.contains(&self.t)
    }
}

const KEYWORDS: &[(&str, Group, T)] = &[
    ("break", Group::Keyword, T::Break), ("case", Group::Keyword, T::Case), ("continue", Group::Keyword, T::Continue),
    ("default", Group::Keyword, T::Default), ("do", Group::Keyword, T::Do), ("while", Group::Keyword, T::While),
    ("else", Group::Keyword, T::Else), ("for", Group::Keyword, T::For), ("each", Group::Identifier, T::Each),
    ("in", Group::Keyword, T::In), ("if", Group::Keyword, T::If), ("return", Group::Keyword, T::Return),
    ("super", Group::Keyword, T::Super), ("switch", Group::Keyword, T::Switch), ("throw", Group::Keyword, T::Throw),
    ("try", Group::Keyword, T::Try), ("catch", Group::Keyword, T::Catch), ("finally", Group::Keyword, T::Finally),
    ("with", Group::Keyword, T::With), ("dynamic", Group::Identifier, T::Dynamic), ("internal", Group::Keyword, T::Internal),
    ("override", Group::Identifier, T::Override), ("private", Group::Keyword, T::Private), ("protected", Group::Keyword, T::Protected),
    ("public", Group::Keyword, T::Public), ("static", Group::Identifier, T::Static), ("class", Group::Keyword, T::Class),
    ("const", Group::Keyword, T::Const), ("extends", Group::Keyword, T::Extends), ("function", Group::Keyword, T::Function),
    ("get", Group::Identifier, T::Get), ("implements", Group::Keyword, T::Implements), ("interface", Group::Keyword, T::Interface),
    ("namespace", Group::Identifier, T::Namespace), ("package", Group::Keyword, T::Package), ("set", Group::Identifier, T::Set),
    ("var", Group::Keyword, T::Var), ("import", Group::Keyword, T::Import), ("use", Group::Keyword, T::Use),
    ("false", Group::Keyword, T::False), ("null", Group::GlobalConst, T::Null), ("this", Group::Keyword, T::This),
    ("true", Group::Keyword, T::True), ("undefined", Group::GlobalConst, T::Undefined), ("Infinity", Group::GlobalConst, T::Infinity),
    ("final", Group::Identifier, T::Final), ("native", Group::Identifier, T::Native),
    ("as", Group::Operator, T::As), ("delete", Group::Operator, T::Delete), ("instanceof", Group::Operator, T::InstanceOf),
    ("is", Group::Operator, T::Is), ("new", Group::Operator, T::New), ("typeof", Group::Operator, T::TypeOf), ("void", Group::Operator, T::Void),
];

/// Operators in rule order; the longest that matches wins.
const OPERATORS: &[(&str, T)] = &[
    ("(", T::ParentOpen), (")", T::ParentClose), ("{", T::CurlyOpen), ("}", T::CurlyClose), ("[", T::BracketOpen),
    ("]", T::BracketClose), (";", T::Semicolon), (",", T::Comma), ("...", T::Rest), (".", T::Dot), ("=", T::Assign),
    (">", T::GreaterThan), ("<", T::LowerThan), ("!", T::Not), ("~", T::Negate), ("?.", T::NullDot), ("??", T::NullCoalesce),
    ("?", T::Ternar), (":", T::Colon), ("===", T::StrictEquals), ("==", T::Equals), ("<=", T::LowerEqual), (">=", T::GreaterEqual),
    ("!==", T::StrictNotEqual), ("!=", T::NotEqual), ("&&=", T::AssignAnd), ("&&", T::And), ("||=", T::AssignOr), ("||", T::Or),
    ("++", T::Increment), ("--", T::Decrement), ("+", T::Plus), ("-", T::Minus), ("*", T::Multiply), ("/", T::Divide),
    ("&", T::BitAnd), ("|", T::BitOr), ("^", T::Xor), ("%", T::Modulo), ("<<", T::ShiftLeft), (">>", T::ShiftRight),
    (">>>", T::UShiftRight), ("+=", T::AssignPlus), ("-=", T::AssignMinus), ("*=", T::AssignMultiply), ("/=", T::AssignDivide),
    ("&=", T::AssignBitAnd), ("|=", T::AssignBitOr), ("^=", T::AssignXor), ("%=", T::AssignModulo), ("<<=", T::AssignShiftLeft),
    (">>=", T::AssignShiftRight), (">>>=", T::AssignUShiftRight), ("::", T::NamespaceOp), ("@", T::Attribute), (".(", T::Filter),
    ("..", T::Descendants),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Initial,
    XmlOpenTag,
    XmlCloseTagFinish,
    XmlOpenTagAttrib,
    Xml,
}

fn ident_first(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

fn ident_next(c: char) -> bool {
    ident_first(c) || c.is_numeric() || matches!(c, '\u{300}'..='\u{36f}' | '\u{203f}' | '\u{2040}')
}

fn xml_name_start(c: char) -> bool {
    c == ':' || c.is_ascii_alphabetic() || c == '_'
}

fn xml_name_char(c: char) -> bool {
    xml_name_start(c) || c == '-' || c == '.' || c.is_ascii_digit() || c == '\u{b7}'
}

pub struct Lexer {
    s: Vec<char>,
    pos: usize,
    /// Offset of `s[0]` in the original source, after `yypushbackstr`.
    base: usize,
    pub line: usize,
    pub state: State,
    pushed: Vec<Sym>,
    pub enable_white_space: bool,
    /// Symbols lexed since each open recording began, for `LexBufferer`.
    pub recorders: Vec<Vec<RecEvent>>,
    finished: bool,
    xml_text: String,
    xml_text_start: usize,
    /// Buffer position and length of the last token matched in the initial state.
    last_token: (usize, usize),
}

#[derive(Debug, Clone)]
pub enum RecEvent {
    Lex(Sym),
    Push(Sym),
}

impl Lexer {
    pub fn new(src: &str) -> Lexer {
        Lexer {
            s: src.chars().collect(),
            pos: 0,
            base: 0,
            line: 1,
            state: State::Initial,
            pushed: Vec::new(),
            enable_white_space: false,
            recorders: Vec::new(),
            finished: false,
            xml_text: String::new(),
            xml_text_start: 0,
            last_token: (0, 0),
        }
    }

    pub fn err<T>(&self, message: impl Into<String>) -> Result<T, CompileError> {
        Err(CompileError { line: self.line, message: message.into() })
    }

    pub fn yyline(&self) -> usize {
        self.line
    }

    fn at(&self, i: usize) -> Option<char> {
        self.s.get(i).copied()
    }

    fn starts(&self, i: usize, w: &str) -> bool {
        let mut k = i;
        for c in w.chars() {
            if self.at(k) != Some(c) {
                return false;
            }
            k += 1;
        }
        true
    }

    fn sym(&self, start: usize, group: Group, t: T, v: V) -> Sym {
        Sym { pos: self.base + start, group, t, v }
    }

    pub fn pushback(&mut self, s: Sym) {
        for r in self.recorders.iter_mut() {
            r.push(RecEvent::Push(s.clone()));
        }
        self.pushed.push(s);
    }

    pub fn lex(&mut self) -> Result<Sym, CompileError> {
        let s = match self.pushed.pop() {
            Some(s) => s,
            None => self.yylex()?,
        };
        for r in self.recorders.iter_mut() {
            r.push(RecEvent::Lex(s.clone()));
        }
        Ok(s)
    }

    /// `yypushbackstr`: lexes `s` again from `state` in place of the last token matched, whose
    /// first `skip` characters are kept.
    pub fn pushback_str(&mut self, s: &str, state: State, skip: usize) {
        let (token_start, token_len) = self.last_token;
        let lines = s.matches('\n').count();
        let rest: Vec<char> = self.s[(token_start + token_len).min(self.s.len())..].to_vec();
        self.base += token_start + skip;
        let mut new: Vec<char> = s.chars().collect();
        new.extend(rest);
        self.s = new;
        self.pos = 0;
        self.state = state;
        self.line -= lines;
        self.finished = false;
    }

    /// `begin`: switches state and drops any pending text.
    pub fn begin(&mut self, state: State) {
        self.xml_text.clear();
        self.state = state;
    }

    pub fn yytext(&self) -> String {
        let (start, len) = self.last_token;
        self.s[start.min(self.s.len())..(start + len).min(self.s.len())].iter().collect()
    }

    fn slice(&self, a: usize, b: usize) -> String {
        self.s[a..b].iter().collect()
    }

    fn count_lines(&mut self, from: usize, to: usize) {
        let mut i = from;
        while i < to {
            match self.s[i] {
                '\n' => self.line += 1,
                '\r' if self.at(i + 1) != Some('\n') => self.line += 1,
                _ => {}
            }
            i += 1;
        }
    }

    fn ident_len(&self, i: usize) -> usize {
        match self.at(i) {
            Some(c) if ident_first(c) => 1 + self.s[i + 1..].iter().take_while(|&&c| ident_next(c)).count(),
            _ => 0,
        }
    }

    fn xml_name_len(&self, i: usize) -> usize {
        match self.at(i) {
            Some(c) if xml_name_start(c) => 1 + self.s[i + 1..].iter().take_while(|&&c| xml_name_char(c)).count(),
            _ => 0,
        }
    }

    fn digits(&self, i: usize) -> usize {
        self.s[i.min(self.s.len())..].iter().take_while(|c| c.is_ascii_digit()).count()
    }

    /// FLit1 | FLit2 | FLit3, then an optional exponent.
    fn float_body(&self, i: usize) -> usize {
        let d = self.digits(i);
        let mut n = if d > 0 && self.at(i + d) == Some('.') {
            d + 1 + self.digits(i + d + 1)
        } else if d == 0 && self.at(i) == Some('.') && self.digits(i + 1) > 0 {
            1 + self.digits(i + 1)
        } else if d > 0 {
            d
        } else {
            return 0;
        };
        if matches!(self.at(i + n), Some('e' | 'E')) {
            let mut k = i + n + 1;
            if matches!(self.at(k), Some('+' | '-')) {
                k += 1;
            }
            let e = self.digits(k);
            if e > 0 {
                n = k + e - i;
            }
        }
        n
    }

    fn dec_int_len(&self, i: usize) -> usize {
        let n = match self.at(i) {
            Some('0') => 1,
            Some('1'..='9') => self.digits(i),
            _ => return 0,
        };
        n + usize::from(matches!(self.at(i + n), Some('u' | 'i')))
    }

    fn hex_len(&self, i: usize) -> usize {
        if self.at(i) == Some('0') && matches!(self.at(i + 1), Some('x' | 'X')) {
            let h = self.s[i + 2..].iter().take_while(|c| c.is_ascii_hexdigit()).count();
            if h > 0 {
                return 2 + h;
            }
        }
        0
    }

    /// `0+ [1-3]? {OctDigit}+`: any run of two or more octal digits that starts with 0.
    fn oct_len(&self, i: usize) -> usize {
        if self.at(i) != Some('0') {
            return 0;
        }
        let m = self.s[i..].iter().take_while(|c| ('0'..='7').contains(*c)).count();
        if m >= 2 { m } else { 0 }
    }

    /// A traditional, end-of-line or AsDoc comment at `i`.
    fn comment_len(&self, i: usize) -> usize {
        if self.starts(i, "//") {
            let mut k = i + 2;
            while k < self.s.len() && !matches!(self.s[k], '\r' | '\n') {
                k += 1;
            }
            if self.at(k) == Some('\r') && self.at(k + 1) == Some('\n') {
                k += 2;
            } else if matches!(self.at(k), Some('\r' | '\n')) {
                k += 1;
            }
            return k - i;
        }
        if self.starts(i, "/*") {
            if let Some(e) = (i + 2..self.s.len().saturating_sub(1)).find(|&k| self.s[k] == '*' && self.s[k + 1] == '/') {
                if e == i + 2 && self.at(i + 2) == Some('*') {
                    return 0;
                }
                return e + 2 - i;
            }
        }
        0
    }

    fn regexp_len(&self, i: usize) -> usize {
        if self.at(i) != Some('/') {
            return 0;
        }
        let mut k = i + 1;
        let mut n = 0;
        loop {
            match self.at(k) {
                Some('\\') if self.at(k + 1).is_some_and(|c| c != '\r' && c != '\n') => {
                    k += 2;
                    n += 1;
                }
                Some(c) if !matches!(c, '\r' | '\n' | '/' | '\\') => {
                    k += 1;
                    n += 1;
                }
                _ => break,
            }
        }
        if n == 0 || self.at(k) != Some('/') {
            return 0;
        }
        k += 1;
        k += self.s[k..].iter().take_while(|c| c.is_ascii_lowercase()).count();
        k - i
    }

    fn escape(&self, i: usize, allow_section: bool) -> Option<(char, usize)> {
        let c = self.at(i + 1)?;
        Some(match c {
            'b' => ('\u{8}', 2),
            't' => ('\t', 2),
            'n' => ('\n', 2),
            'f' => ('\u{c}', 2),
            'r' => ('\r', 2),
            '"' if !allow_section => ('"', 2),
            '\'' if !allow_section => ('\'', 2),
            '\u{a7}' if allow_section => ('\u{a7}', 2),
            '\\' => ('\\', 2),
            'x' => {
                let h: String = self.s.get(i + 2..i + 4)?.iter().collect();
                (char::from_u32(u32::from_str_radix(&h, 16).ok()?)?, 4)
            }
            'u' => {
                let h: String = self.s.get(i + 2..i + 6)?.iter().collect();
                (char::from_u32(u32::from_str_radix(&h, 16).ok()?).unwrap_or('\u{fffd}'), 6)
            }
            _ => return None,
        })
    }

    /// A string or character literal from its opening quote.
    fn quoted(&mut self, start: usize, quote: char) -> Option<Sym> {
        let mut out = String::new();
        let mut k = start + 1;
        loop {
            let Some(c) = self.at(k) else {
                self.pos = k;
                return None;
            };
            if c == quote {
                self.pos = k + 1;
                return Some(self.sym(start, Group::String, T::String, V::Str(out)));
            }
            if c == '\r' || c == '\n' {
                self.count_lines(k, k + 1 + usize::from(c == '\r' && self.at(k + 1) == Some('\n')));
                self.pos = k + 1 + usize::from(c == '\r' && self.at(k + 1) == Some('\n'));
                return None;
            }
            if c == '\\' {
                match self.escape(k, false) {
                    Some((v, n)) => {
                        out.push(v);
                        k += n;
                    }
                    None => k += if self.at(k + 1).is_some_and(|c| c != '\r' && c != '\n') { 2 } else { 1 },
                }
                continue;
            }
            out.push(c);
            k += 1;
        }
    }

    /// A §-quoted obfuscated identifier.
    fn oidentifier(&mut self, start: usize) -> Result<Option<Sym>, CompileError> {
        let mut out = String::new();
        let mut repeat = 1usize;
        let mut k = start + 1;
        loop {
            let Some(c) = self.at(k) else {
                self.pos = k;
                return Ok(None);
            };
            match c {
                '\u{a7}' => {
                    self.pos = k + 1;
                    return Ok(Some(self.sym(start, Group::Identifier, T::Identifier, V::Str(out))));
                }
                '\r' | '\n' => {
                    let n = 1 + usize::from(c == '\r' && self.at(k + 1) == Some('\n'));
                    self.line += 1;
                    self.pos = k + n;
                    return Ok(None);
                }
                '\\' => {
                    if self.at(k + 1) == Some('{') {
                        let n = self.dec_int_len(k + 2);
                        if n > 0 && self.at(k + 2 + n) == Some('}') {
                            let t: String = self.s[k + 2..k + 2 + n].iter().filter(|c| c.is_ascii_digit()).collect();
                            repeat = t.parse().unwrap_or(1);
                            k += n + 3;
                            continue;
                        }
                    }
                    match self.escape(k, true) {
                        Some((v, n)) => {
                            out.extend(std::iter::repeat_n(v, repeat));
                            repeat = 1;
                            k += n;
                        }
                        None => {
                            let t: String = self.s[k..(k + 2).min(self.s.len())].iter().collect();
                            return self.err(format!("Illegal escape sequence \"{t}\""));
                        }
                    }
                }
                c => {
                    out.extend(std::iter::repeat_n(c, repeat));
                    repeat = 1;
                    k += 1;
                }
            }
        }
    }

    fn number(&self, i: usize) -> Option<(usize, Group, T, V)> {
        let dec = self.dec_int_len(i);
        let hex = self.hex_len(i);
        let oct = self.oct_len(i);
        let fb = self.float_body(i);
        let dbl = if fb > 0 { fb + usize::from(matches!(self.at(i + fb), Some('m' | 'd'))) } else { 0 };
        let flt = if fb > 0 { fb + usize::from(self.at(i + fb) == Some('f')) } else { 0 };
        let best = dec.max(hex).max(oct).max(dbl).max(flt);
        if best == 0 {
            return None;
        }
        let text: String = self.s[i..i + best].iter().collect();
        if dec == best {
            let t = text.trim_end_matches(['i', 'u']);
            return Some(match t.parse::<i32>() {
                Ok(v) => (best, Group::Integer, T::Integer, V::Int(v)),
                Err(_) => (best, Group::Double, T::Double, V::Double(t.parse().unwrap_or(0.0))),
            });
        }
        if hex == best {
            let h = &text[2..];
            return Some(match i32::from_str_radix(h, 16) {
                Ok(v) => (best, Group::Integer, T::Integer, V::Int(v)),
                Err(_) => (best, Group::Double, T::Double, V::Double(h.chars().fold(0.0, |a, c| a * 16.0 + c.to_digit(16).unwrap() as f64))),
            });
        }
        if oct == best {
            return Some(match i32::from_str_radix(&text, 8) {
                Ok(v) => (best, Group::Integer, T::Integer, V::Int(v)),
                Err(_) => (best, Group::Double, T::Double, V::Double(text.chars().fold(0.0, |a, c| a * 8.0 + c.to_digit(8).unwrap() as f64))),
            });
        }
        if dbl == best {
            if let Some(d) = text.strip_suffix('m') {
                return Some((best, Group::Decimal, T::Decimal, V::Str(d.to_string())));
            }
            let d = text.strip_suffix('d').unwrap_or(&text);
            return Some((best, Group::Double, T::Double, V::Double(d.parse().unwrap_or(0.0))));
        }
        let f = text.strip_suffix('f').unwrap_or(&text);
        Some((best, Group::Float, T::Float, V::Float(f.parse().unwrap_or(0.0))))
    }

    /// float4(f, f, f, f).
    fn float4(&self, i: usize) -> Option<(usize, [f32; 4])> {
        if !self.starts(i, "float4") {
            return None;
        }
        let ws = |k: &mut usize| {
            while matches!(self.at(*k), Some(' ' | '\t' | '\u{c}' | '\r' | '\n')) {
                *k += 1;
            }
        };
        let mut k = i + 6;
        ws(&mut k);
        if self.at(k) != Some('(') {
            return None;
        }
        k += 1;
        let mut vals = [0f32; 4];
        for (n, v) in vals.iter_mut().enumerate() {
            ws(&mut k);
            let fb = self.float_body(k);
            if fb == 0 {
                return None;
            }
            let t: String = self.s[k..k + fb].iter().collect();
            *v = t.parse().ok()?;
            k += fb + usize::from(self.at(k + fb) == Some('f'));
            ws(&mut k);
            if self.at(k) != Some(if n == 3 { ')' } else { ',' }) {
                return None;
            }
            k += 1;
        }
        Some((k - i, vals))
    }

    fn yylex(&mut self) -> Result<Sym, CompileError> {
        loop {
            if self.pos >= self.s.len() {
                return Ok(self.sym(self.pos, Group::Eof, T::Eof, V::None));
            }
            let r = match self.state {
                State::Initial => self.initial()?,
                State::XmlCloseTagFinish => self.xml_close_finish()?,
                State::XmlOpenTag => self.xml_open_tag()?,
                State::XmlOpenTagAttrib => self.xml_open_tag_attrib(),
                State::Xml => self.xml()?,
            };
            if let Some(s) = r {
                return Ok(s);
            }
        }
    }

    fn initial(&mut self) -> Result<Option<Sym>, CompileError> {
        let i = self.pos;
        let c = self.s[i];
        // Candidate rules and their lengths; the longest wins, the earliest on ties.
        let ident = self.ident_len(i);
        let kw = KEYWORDS.iter().find(|(w, _, _)| w.chars().count() == ident && self.starts(i, w));
        let op = OPERATORS.iter().filter(|(w, _)| self.starts(i, w)).max_by_key(|(w, _)| w.len());
        let op_len = op.map_or(0, |(w, _)| w.len());
        let verbatim = if self.starts(i, "@\"") { (i + 2..self.s.len()).find(|&k| matches!(self.s[k], '"' | '\r' | '\n')).filter(|&k| self.s[k] == '"').map_or(0, |k| k + 1 - i) } else { 0 };
        let num = self.number(i);
        let num_len = num.as_ref().map_or(0, |n| n.0);
        let f4 = self.float4(i);
        let f4_len = f4.as_ref().map_or(0, |f| f.0);
        let comment = self.comment_len(i);
        let regexp = self.regexp_len(i);
        let preproc = if self.starts(i, "\u{a7}\u{a7}") { self.ident_len(i + 2).checked_sub(0).filter(|&n| n > 0).map_or(0, |n| n + 2) } else { 0 };
        let typename = if self.starts(i, ".<") { 2 } else { 0 };
        let xml_open = if c == '<' { self.xml_name_len(i + 1).checked_sub(0).filter(|&n| n > 0).map_or(0, |n| n + 1) } else { 0 };
        let ns_suffix = if c == '#' { self.dec_int_len(i + 1).checked_sub(0).filter(|&n| n > 0).map_or(0, |n| n + 1) } else { 0 };
        let ws = if matches!(c, ' ' | '\t' | '\u{c}') { self.s[i..].iter().take_while(|c| matches!(c, ' ' | '\t' | '\u{c}')).count() } else { 0 };
        let lt = if c == '\r' && self.at(i + 1) == Some('\n') { 2 } else if matches!(c, '\r' | '\n') { 1 } else { 0 };
        let xml_comment = if self.starts(i, "<!--") { 4 } else { 0 };
        let xml_cdata = if self.starts(i, "<![CDATA[") { 9 } else { 0 };
        let var_tag = if self.starts(i, "<{") { 2 } else { 0 };
        let quote = matches!(c, '"' | '\'' | '\u{a7}');

        let kw_len = kw.map_or(0, |(w, _, _)| w.chars().count());
        let best = [kw_len, verbatim, op_len, if quote { 1 } else { 0 }, preproc, num_len, f4_len, comment, lt, ws, typename, xml_open, xml_comment, xml_cdata, var_tag, ident, regexp, ns_suffix]
            .into_iter()
            .max()
            .unwrap_or(0);
        if best == 0 {
            self.pos += 1;
            return Ok(None);
        }
        self.last_token = (i, best);
        if kw_len == best {
            let (w, g, t) = kw.unwrap();
            self.pos += best;
            return Ok(Some(self.sym(i, *g, *t, V::Str(w.to_string()))));
        }
        if verbatim == best {
            self.pos += best;
            return Ok(Some(self.sym(i, Group::String, T::String, V::Str(self.slice(i + 2, i + best - 1)))));
        }
        if op_len == best {
            let (w, t) = op.unwrap();
            self.pos += best;
            return Ok(Some(self.sym(i, Group::Operator, *t, V::Str(w.to_string()))));
        }
        if quote && best == 1 {
            return Ok(match c {
                '\u{a7}' => self.oidentifier(i)?,
                q => self.quoted(i, q),
            });
        }
        if preproc == best {
            self.pos += best;
            return Ok(Some(self.sym(i, Group::Preprocessor, T::Preprocessor, V::Str(self.slice(i + 2, i + best)))));
        }
        if num_len == best && num_len >= f4_len {
            let (n, g, t, v) = num.unwrap();
            self.pos += n;
            return Ok(Some(self.sym(i, g, t, v)));
        }
        if f4_len == best {
            let (n, v) = f4.unwrap();
            self.count_lines(i, i + n);
            self.pos += n;
            return Ok(Some(self.sym(i, Group::Float4, T::Float4, V::Float4(v))));
        }
        if comment == best {
            self.count_lines(i, i + best);
            self.pos += best;
            return Ok(None);
        }
        if lt == best {
            self.line += 1;
            self.pos += best;
            if self.enable_white_space {
                return Ok(Some(self.sym(i, Group::Xml, T::XmlWhitespace, V::Str(self.slice(i, i + best)))));
            }
            return Ok(None);
        }
        if ws == best {
            self.pos += best;
            if self.enable_white_space {
                return Ok(Some(self.sym(i, Group::Xml, T::XmlWhitespace, V::Str(self.slice(i, i + best)))));
            }
            return Ok(None);
        }
        if typename == best {
            self.pos += 2;
            return Ok(Some(self.sym(i, Group::TypeName, T::TypeName, V::Str(".<".into()))));
        }
        if xml_open == best {
            self.pos += best;
            self.state = State::XmlOpenTag;
            self.xml_text_start = i;
            self.xml_text.clear();
            return Ok(Some(self.sym(i, Group::Xml, T::XmlStartTagBegin, V::Str(self.slice(i, i + best)))));
        }
        if xml_comment == best || xml_cdata == best {
            let (end, t) = if xml_comment == best { ("-->", T::XmlComment) } else { ("]]>", T::XmlCData) };
            let mut k = i + best;
            while k < self.s.len() && !self.starts(k, end) {
                k += 1;
            }
            let stop = (k + 3).min(self.s.len());
            self.count_lines(i, stop);
            self.pos = stop;
            if k >= self.s.len() {
                return Ok(None);
            }
            return Ok(Some(self.sym(i, Group::Xml, t, V::Str(self.slice(i, stop)))));
        }
        if var_tag == best {
            self.pos += 2;
            return Ok(Some(self.sym(i, Group::Xml, T::XmlStartVarTagBegin, V::Str("<{".into()))));
        }
        if ident == best {
            self.pos += best;
            return Ok(Some(self.sym(i, Group::Identifier, T::Identifier, V::Str(self.slice(i, i + best)))));
        }
        if regexp == best {
            self.pos += best;
            return Ok(Some(self.sym(i, Group::Regexp, T::Regexp, V::Str(self.slice(i, i + best)))));
        }
        let n: String = self.slice(i + 1, i + best).trim_end_matches(['i', 'u']).to_string();
        self.pos += best;
        Ok(Some(self.sym(i, Group::NamespaceSuffix, T::NamespaceSuffix, V::Int(n.parse().unwrap_or(0)))))
    }
}

/// XML literal states: the text collected so far and where it began.
impl Lexer {
    fn flush_text(&mut self) {
        if !self.xml_text.is_empty() {
            let t = std::mem::take(&mut self.xml_text);
            let s = Sym { pos: self.base + self.xml_text_start, group: Group::Xml, t: T::XmlText, v: V::Str(t) };
            self.pushback(s);
        }
    }

    /// Pushes `s` back, then the collected text before it, and lexes again.
    fn push_with_text(&mut self, s: Sym) -> Result<Option<Sym>, CompileError> {
        self.pushback(s);
        self.flush_text();
        self.lex().map(Some)
    }

    fn xml_close_finish(&mut self) -> Result<Option<Sym>, CompileError> {
        let i = self.pos;
        match self.s[i] {
            '>' => {
                self.pos += 1;
                self.state = State::Xml;
                let s = self.sym(i, Group::Operator, T::GreaterThan, V::Str(">".into()));
                self.push_with_text(s)
            }
            '\r' | '\n' => {
                let n = 1 + usize::from(self.s[i] == '\r' && self.at(i + 1) == Some('\n'));
                self.xml_text.extend(&self.s[i..i + n]);
                self.line += 1;
                self.pos += n;
                Ok(None)
            }
            ' ' | '\t' => {
                self.xml_text.push(self.s[i]);
                self.pos += 1;
                Ok(None)
            }
            _ => {
                self.pos += 1;
                Ok(None)
            }
        }
    }

    fn xml_open_tag(&mut self) -> Result<Option<Sym>, CompileError> {
        let i = self.pos;
        let name = self.xml_name_len(i);
        if name > 0 && self.at(i + name) == Some('=') {
            self.pos += name + 1;
            self.state = State::XmlOpenTagAttrib;
            let s = self.sym(i, Group::Xml, T::XmlAttributeName, V::Str(self.s[i..i + name + 1].iter().collect()));
            return self.push_with_text(s);
        }
        match self.s[i] {
            '{' => {
                self.pos += 1;
                self.state = State::Initial;
                let s = self.sym(i, Group::Xml, T::XmlAttrNameVarBegin, V::Str("{".into()));
                self.push_with_text(s)
            }
            '>' => {
                self.pos += 1;
                self.state = State::Xml;
                let s = Sym { pos: self.base + self.xml_text_start, group: Group::Xml, t: T::XmlStartTagEnd, v: V::Str(">".into()) };
                self.push_with_text(s)
            }
            '/' if self.at(i + 1) == Some('>') => {
                self.pos += 2;
                self.state = State::Xml;
                let s = self.sym(i, Group::Xml, T::XmlStartFinishTagEnd, V::Str("/>".into()));
                self.push_with_text(s)
            }
            '\r' | '\n' => {
                let n = 1 + usize::from(self.s[i] == '\r' && self.at(i + 1) == Some('\n'));
                self.xml_text.extend(&self.s[i..i + n]);
                self.line += 1;
                self.pos += n;
                Ok(None)
            }
            ' ' | '\t' | '\u{c}' => {
                let n = self.s[i..].iter().take_while(|c| matches!(c, ' ' | '\t' | '\u{c}')).count();
                self.xml_text.extend(&self.s[i..i + n]);
                self.pos += n;
                Ok(None)
            }
            _ => {
                self.pos += 1;
                Ok(None)
            }
        }
    }

    fn xml_open_tag_attrib(&mut self) -> Option<Sym> {
        let i = self.pos;
        match self.s[i] {
            q @ ('"' | '\'') => {
                let end = self.s[i + 1..].iter().position(|&c| c == q || c == '\r' || c == '\n').map(|e| i + 1 + e);
                if let Some(e) = end.filter(|&e| self.s[e] == q) {
                    self.pos = e + 1;
                    self.state = State::XmlOpenTag;
                    let t = if q == '"' { T::XmlAttributeValue } else { T::XmlAttributeValueSingleQuoted };
                    return Some(self.sym(i, Group::Xml, t, V::Str(self.s[i..=e].iter().collect())));
                }
                self.pos += 1;
                None
            }
            '{' => {
                self.pos += 1;
                self.state = State::Initial;
                Some(self.sym(i, Group::Xml, T::XmlAttrValVarBegin, V::Str("{".into())))
            }
            _ => {
                self.pos += 1;
                None
            }
        }
    }

    fn xml(&mut self) -> Result<Option<Sym>, CompileError> {
        let i = self.pos;
        let take_until = |me: &mut Lexer, start: usize, end: &str, t: T| -> Option<Sym> {
            let mut k = start;
            while k < me.s.len() && !me.starts(k, end) {
                k += 1;
            }
            let stop = (k + end.len()).min(me.s.len());
            me.count_lines(start, stop);
            me.pos = stop;
            let mut text = std::mem::take(&mut me.xml_text);
            text.extend(&me.s[start..stop]);
            (k < me.s.len()).then(|| Sym { pos: me.base + i, group: Group::Xml, t, v: V::Str(text) })
        };
        if self.starts(i, "<![CDATA[") {
            let before = std::mem::take(&mut self.xml_text);
            let s = take_until(self, i, "]]>", T::XmlCData);
            if !before.is_empty() {
                if let Some(s) = s {
                    self.pushback(s);
                }
                return Ok(Some(self.sym(i, Group::Xml, T::XmlText, V::Str(before))));
            }
            return Ok(s);
        }
        if self.starts(i, "<?") && self.xml_name_len(i + 2) > 0 {
            let before = std::mem::take(&mut self.xml_text);
            let s = take_until(self, i, "?>", T::XmlInstr);
            if !before.is_empty() {
                if let Some(s) = s {
                    self.pushback(s);
                }
                return Ok(Some(self.sym(i, Group::Xml, T::XmlText, V::Str(before))));
            }
            return Ok(s);
        }
        if self.starts(i, "<!--") {
            let before = std::mem::take(&mut self.xml_text);
            let s = take_until(self, i, "-->", T::XmlComment);
            if !before.is_empty() {
                if let Some(s) = s {
                    self.pushback(s);
                }
                return Ok(Some(self.sym(i, Group::Xml, T::XmlText, V::Str(before))));
            }
            return Ok(s);
        }
        if self.at(i) == Some('<') && self.xml_name_len(i + 1) > 0 {
            let n = 1 + self.xml_name_len(i + 1);
            self.pos += n;
            self.state = State::XmlOpenTag;
            let s = self.sym(i, Group::Xml, T::XmlStartTagBegin, V::Str(self.s[i..i + n].iter().collect()));
            return self.push_with_text(s);
        }
        if self.starts(i, "</") && self.xml_name_len(i + 2) > 0 {
            let mut k = i + 2 + self.xml_name_len(i + 2);
            k += self.s[k..].iter().take_while(|c| matches!(c, ' ' | '\t' | '\r' | '\n')).count();
            if self.at(k) == Some('>') {
                self.count_lines(i, k);
                self.pos = k + 1;
                let s = self.sym(i, Group::Xml, T::XmlFinishTag, V::Str(self.s[i..=k].iter().collect()));
                return self.push_with_text(s);
            }
        }
        if self.starts(i, "<{") {
            self.pos += 2;
            self.state = State::Initial;
            let s = self.sym(i, Group::Xml, T::XmlStartVarTagBegin, V::Str("<{".into()));
            return self.push_with_text(s);
        }
        if self.starts(i, "</{") {
            self.pos += 3;
            self.state = State::Initial;
            let s = self.sym(i, Group::Xml, T::XmlFinishVarTagBegin, V::Str("</{".into()));
            return self.push_with_text(s);
        }
        if self.at(i) == Some('{') {
            self.pos += 1;
            self.state = State::Initial;
            let s = self.sym(i, Group::Xml, T::XmlVarBegin, V::Str("{".into()));
            return self.push_with_text(s);
        }
        let c = self.s[i];
        if self.xml_text.is_empty() {
            self.xml_text_start = i;
        }
        if c == '\n' || (c == '\r' && self.at(i + 1) != Some('\n')) {
            self.line += 1;
        }
        self.xml_text.push(c);
        self.pos += 1;
        Ok(None)
    }
}
