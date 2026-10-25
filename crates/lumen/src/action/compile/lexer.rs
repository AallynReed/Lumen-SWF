//! AS1/2 lexer.

use super::CompileError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Keyword,
    GlobalFunc,
    GlobalConst,
    Operator,
    Directive,
    Preprocessor,
    String,
    Integer,
    Double,
    Identifier,
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum T {
    In, Super, Function, False, Null, This, True, Break, Case, Continue, Default, Do, While, Else, For, Each,
    If, Return, Switch, Throw, Try, Catch, Finally, With, Dynamic, Private, Public, Static, Class, Extends,
    Get, Implements, Interface, Set, Var, Import, IfFrameLoaded,
    ParentOpen, ParentClose, CurlyOpen, CurlyClose, BracketOpen, BracketClose, Semicolon, Comma, Rest, Dot,
    Assign, GreaterThan, LowerThan, Not, Negate, Ternar, Colon, Equals, StrictEquals, LowerEqual,
    GreaterEqual, NotEqual, StrictNotEqual, And, Or, FullAnd, FullOr, Increment, Decrement, Plus, Minus,
    Multiply, Divide, BitAnd, BitOr, Xor, Modulo, ShiftLeft, ShiftRight, UShiftRight, AssignPlus,
    AssignMinus, AssignMultiply, AssignDivide, AssignBitAnd, AssignBitOr, AssignXor, AssignModulo,
    AssignShiftLeft, AssignShiftRight, AssignUShiftRight, Delete, InstanceOf, New, TypeOf, Void, Attribute,
    String, Identifier, Integer, Double, Trace, GetUrl, GotoAndStop, NextFrame, Play, PrevFrame, TellTarget,
    Stop, StopAllSounds, ToggleHighQuality, Ord, Chr, DuplicateMovieClip, StopDrag, GetTimer,
    LoadVariables, LoadMovie, GotoAndPlay, MbOrd, MbChr, MbLength, MbSubstring, Random, RemoveMovieClip,
    StartDrag, Substring, Length, Int, TargetPath, NumberOp, StringOp, Eval, Undefined, Newline,
    GetVersion, Call, LoadMovieNum, LoadVariablesNum, Print, PrintNum, PrintAsBitmap, PrintAsBitmapNum,
    UnloadMovie, UnloadMovieNum, FsCommand, Preprocessor, FsCommand2, Directive, Eof,
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
    /// `SymbolType` (precedence, binary, right associative).
    pub fn info(self) -> (i32, bool, bool) {
        use T::*;
        match self {
            In => (RELATIONAL, true, false),
            Comma => (COMMA, false, false),
            Assign | AssignPlus | AssignMinus | AssignMultiply | AssignDivide | AssignBitAnd | AssignBitOr | AssignXor | AssignModulo
            | AssignShiftLeft | AssignShiftRight | AssignUShiftRight => (ASSIGNMENT, true, true),
            GreaterThan | LowerThan | LowerEqual | GreaterEqual | InstanceOf => (RELATIONAL, true, false),
            Not | Negate | Delete | TypeOf => (UNARY, false, false),
            Ternar => (CONDITIONAL, true, true),
            Colon => (CONDITIONAL, false, false),
            Equals | StrictEquals | NotEqual | StrictNotEqual => (EQUALITY, true, false),
            And | FullAnd => (LOGICALAND, true, false),
            Or | FullOr => (LOGICALOR, true, false),
            Increment | Decrement => (POSTFIX, false, false),
            Plus | Minus => (ADDITIVE, true, false),
            Multiply | Divide | Modulo => (MULTIPLICATIVE, true, false),
            BitAnd => (BITAND, true, false),
            BitOr => (BITOR, true, false),
            Xor => (BITXOR, true, false),
            ShiftLeft | ShiftRight | UShiftRight => (SHIFT, true, false),
            Break | Case | Continue | Default | Do | While | Else | For | Each | If | Return | Switch | Throw | Try | Catch | Finally
            | With | Dynamic | Private | Public | Static | Class | Extends | Get | Implements | Interface | Set | Var | Import
            | IfFrameLoaded | Rest | Void | Attribute | Eof => (NOPRECEDENCE, false, false),
            _ => (PRIMARY, false, false),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum V {
    None,
    Str(String),
    Int(i64),
    Double(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sym {
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
    ("else", Group::Keyword, T::Else), ("for", Group::Keyword, T::For), ("each", Group::Keyword, T::Each),
    ("in", Group::Keyword, T::In), ("if", Group::Keyword, T::If), ("return", Group::Keyword, T::Return),
    ("super", Group::Keyword, T::Super), ("switch", Group::Keyword, T::Switch), ("throw", Group::Keyword, T::Throw),
    ("try", Group::Keyword, T::Try), ("catch", Group::Keyword, T::Catch), ("finally", Group::Keyword, T::Finally),
    ("with", Group::Keyword, T::With), ("dynamic", Group::Keyword, T::Dynamic), ("private", Group::Keyword, T::Private),
    ("public", Group::Keyword, T::Public), ("static", Group::Keyword, T::Static), ("class", Group::Keyword, T::Class),
    ("extends", Group::Keyword, T::Extends), ("function", Group::Keyword, T::Function), ("get", Group::Keyword, T::Get),
    ("implements", Group::Keyword, T::Implements), ("interface", Group::Keyword, T::Interface), ("set", Group::Keyword, T::Set),
    ("var", Group::Keyword, T::Var), ("import", Group::Keyword, T::Import), ("false", Group::Keyword, T::False),
    ("null", Group::GlobalConst, T::Null), ("this", Group::Keyword, T::This), ("true", Group::Keyword, T::True),
    ("tellTarget", Group::GlobalFunc, T::TellTarget), ("ifFrameLoaded", Group::Keyword, T::IfFrameLoaded),
    ("delete", Group::Operator, T::Delete), ("instanceof", Group::Operator, T::InstanceOf), ("new", Group::Operator, T::New),
    ("typeof", Group::Operator, T::TypeOf), ("void", Group::Operator, T::Void), ("and", Group::Operator, T::FullAnd),
    ("or", Group::Operator, T::FullOr),
];

/// Global functions, matched case-insensitively.
const GLOBAL_FUNCS: &[(&str, T)] = &[
    ("geturl", T::GetUrl), ("trace", T::Trace), ("gotoandstop", T::GotoAndStop), ("nextframe", T::NextFrame), ("play", T::Play),
    ("prevframe", T::PrevFrame), ("stop", T::Stop), ("stopallsounds", T::StopAllSounds), ("togglehighquality", T::ToggleHighQuality),
    ("ord", T::Ord), ("chr", T::Chr), ("duplicatemovieclip", T::DuplicateMovieClip), ("stopdrag", T::StopDrag), ("gettimer", T::GetTimer),
    ("loadvariables", T::LoadVariables), ("loadmovie", T::LoadMovie), ("gotoandplay", T::GotoAndPlay), ("mbord", T::MbOrd),
    ("mbchr", T::MbChr), ("mblength", T::MbLength), ("mbsubstring", T::MbSubstring), ("random", T::Random),
    ("removemovieclip", T::RemoveMovieClip), ("startdrag", T::StartDrag), ("substring", T::Substring), ("length", T::Length),
    ("int", T::Int), ("targetpath", T::TargetPath), ("number", T::NumberOp), ("string", T::StringOp), ("eval", T::Eval),
    ("getversion", T::GetVersion), ("call", T::Call), ("loadmovienum", T::LoadMovieNum), ("loadvariablesnum", T::LoadVariablesNum),
    ("printasbitmapnum", T::PrintAsBitmapNum), ("printnum", T::PrintNum), ("printasbitmap", T::PrintAsBitmap), ("print", T::Print),
    ("unloadmovie", T::UnloadMovie), ("unloadmovienum", T::UnloadMovieNum), ("fscommand2", T::FsCommand2), ("fscommand", T::FsCommand),
];

const OPERATORS: &[(&str, T)] = &[
    (">>>=", T::AssignUShiftRight), ("...", T::Rest), ("===", T::StrictEquals), ("!==", T::StrictNotEqual), (">>>", T::UShiftRight),
    ("<<=", T::AssignShiftLeft), (">>=", T::AssignShiftRight), ("==", T::Equals), ("<=", T::LowerEqual), (">=", T::GreaterEqual),
    ("!=", T::NotEqual), ("<>", T::NotEqual), ("&&", T::And), ("||", T::Or), ("++", T::Increment), ("--", T::Decrement),
    ("<<", T::ShiftLeft), (">>", T::ShiftRight), ("+=", T::AssignPlus), ("-=", T::AssignMinus), ("*=", T::AssignMultiply),
    ("/=", T::AssignDivide), ("&=", T::AssignBitAnd), ("|=", T::AssignBitOr), ("^=", T::AssignXor), ("%=", T::AssignModulo),
    ("(", T::ParentOpen), (")", T::ParentClose), ("{", T::CurlyOpen), ("}", T::CurlyClose), ("[", T::BracketOpen),
    ("]", T::BracketClose), (";", T::Semicolon), (",", T::Comma), (".", T::Dot), ("=", T::Assign), (">", T::GreaterThan),
    ("<", T::LowerThan), ("!", T::Not), ("~", T::Negate), ("?", T::Ternar), (":", T::Colon), ("+", T::Plus), ("-", T::Minus),
    ("*", T::Multiply), ("/", T::Divide), ("&", T::BitAnd), ("|", T::BitOr), ("^", T::Xor), ("%", T::Modulo), ("@", T::Attribute),
];

fn is_letter(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

fn is_letter_digit(c: char) -> bool {
    is_letter(c) || c.is_ascii_digit()
}

pub struct Lexer {
    s: Vec<char>,
    pos: usize,
    pub line: usize,
    case_sensitive: bool,
    pub pushed: Vec<Sym>,
    /// Symbols lexed since `mark`, for `LexBufferer`.
    pub recorded: Option<Vec<Sym>>,
}

impl Lexer {
    pub fn new(text: &str, case_sensitive: bool) -> Lexer {
        Lexer { s: text.chars().collect(), pos: 0, line: 1, case_sensitive, pushed: Vec::new(), recorded: None }
    }

    fn at(&self, i: usize) -> Option<char> {
        self.s.get(i).copied()
    }

    fn starts(&self, i: usize, w: &str) -> bool {
        let n = w.chars().count();
        self.s.len() >= i + n && self.s[i..i + n].iter().copied().eq(w.chars())
    }

    fn sym(group: Group, t: T, v: V) -> Sym {
        Sym { group, t, v }
    }

    pub fn lex(&mut self) -> Result<Sym, CompileError> {
        let s = match self.pushed.pop() {
            Some(s) => s,
            None => self.next()?,
        };
        if let Some(r) = &mut self.recorded {
            r.push(s.clone());
        }
        Ok(s)
    }

    pub fn pushback(&mut self, s: Sym) {
        if let Some(r) = &mut self.recorded {
            if r.last() == Some(&s) {
                r.pop();
            }
        }
        self.pushed.push(s);
    }

    fn count_lines(&mut self, from: usize, to: usize) {
        self.line += self.s[from..to].iter().filter(|&&c| c == '\n').count();
    }

    fn next(&mut self) -> Result<Sym, CompileError> {
        loop {
            let i = self.pos;
            let Some(c) = self.at(i) else { return Ok(Self::sym(Group::Eof, T::Eof, V::None)) };
            if c == '\n' {
                self.line += 1;
                self.pos += 1;
                continue;
            }
            if matches!(c, ' ' | '\t' | '\u{c}' | '\r') {
                self.pos += 1;
                continue;
            }
            if self.starts(i, "/*") {
                let end = (i + 2..self.s.len().saturating_sub(1)).find(|&j| self.s[j] == '*' && self.s[j + 1] == '/').map_or(self.s.len(), |j| j + 2);
                self.count_lines(i, end);
                self.pos = end;
                continue;
            }
            if self.starts(i, "//") {
                let end = (i..self.s.len()).find(|&j| self.s[j] == '\r' || self.s[j] == '\n').unwrap_or(self.s.len());
                self.pos = end;
                continue;
            }
            if c == '"' || c == '\'' {
                self.pos += 1;
                return self.string(c);
            }
            if c == '\u{a7}' {
                if self.at(i + 1) == Some('\u{a7}') {
                    let n = self.ident_len(i + 2);
                    if n > 0 {
                        self.pos = i + 2 + n;
                        return Ok(Self::sym(Group::Preprocessor, T::Preprocessor, V::Str(self.s[i + 2..i + 2 + n].iter().collect())));
                    }
                }
                self.pos += 1;
                return self.oidentifier();
            }
            if c == '#' {
                let n = self.ident_len(i + 1);
                if n > 0 {
                    self.pos = i + 1 + n;
                    return Ok(Self::sym(Group::Directive, T::Directive, V::Str(self.s[i + 1..i + 1 + n].iter().collect())));
                }
            }
            if c.is_ascii_digit() || (c == '.' && self.at(i + 1).is_some_and(|d| d.is_ascii_digit())) {
                return Ok(self.number());
            }
            let n = self.ident_len(i);
            if n > 0 {
                let w: String = self.s[i..i + n].iter().collect();
                self.pos += n;
                return Ok(self.word(w));
            }
            if let Some(&(op, t)) = OPERATORS.iter().find(|(op, _)| self.starts(i, op)) {
                self.pos += op.chars().count();
                return Ok(Self::sym(Group::Operator, t, V::Str(op.into())));
            }
            self.pos += 1;
        }
    }

    fn ident_len(&self, i: usize) -> usize {
        match self.at(i) {
            Some(c) if is_letter(c) => 1 + self.s[i + 1..].iter().take_while(|&&c| is_letter_digit(c)).count(),
            _ => 0,
        }
    }

    fn word(&self, w: String) -> Sym {
        if let Some(&(_, g, t)) = KEYWORDS.iter().find(|(k, _, _)| *k == w) {
            return Self::sym(g, t, V::Str(w));
        }
        let lower = w.to_ascii_lowercase();
        if lower == "undefined" || lower == "newline" {
            if self.case_sensitive && w != lower {
                return Self::sym(Group::Identifier, T::Identifier, V::Str(w));
            }
            let t = if lower == "undefined" { T::Undefined } else { T::Newline };
            return Self::sym(Group::GlobalConst, t, V::Str(w));
        }
        if let Some(&(_, t)) = GLOBAL_FUNCS.iter().find(|(k, _)| *k == lower) {
            return Self::sym(Group::GlobalFunc, t, V::Str(w));
        }
        Self::sym(Group::Identifier, T::Identifier, V::Str(w))
    }

    /// `DecIntegerLiteral`, `HexIntegerLiteral`, `OctIntegerLiteral` and `DoubleLiteral`, by longest match.
    fn number(&mut self) -> Sym {
        let i = self.pos;
        let digits = |j: usize| self.s[j.min(self.s.len())..].iter().take_while(|c| c.is_ascii_digit()).count();
        let hex = if self.at(i) == Some('0') && matches!(self.at(i + 1), Some('x' | 'X')) {
            let h = self.s[i + 2..].iter().take_while(|c| c.is_ascii_hexdigit()).count();
            if h > 0 { 2 + h } else { 0 }
        } else {
            0
        };
        let oct = if self.at(i) == Some('0') {
            let z = self.s[i..].iter().take_while(|&&c| c == '0').count();
            let mut j = i + z;
            if matches!(self.at(j), Some('1'..='3')) && self.at(j + 1).is_some_and(|c| ('0'..='7').contains(&c)) {
                j += 1;
            }
            let o = self.s[j..].iter().take_while(|&&c| ('0'..='7').contains(&c)).count();
            if o > 0 { j + o - i } else { 0 }
        } else {
            0
        };
        let dec = if self.at(i) == Some('0') { 1 } else { digits(i) };
        let d = digits(i);
        let mut dbl = if d > 0 {
            if self.at(i + d) == Some('.') { d + 1 + digits(i + d + 1) } else { d }
        } else if self.at(i) == Some('.') {
            1 + digits(i + 1)
        } else {
            0
        };
        if dbl > 0 && matches!(self.at(i + dbl), Some('e' | 'E')) {
            let sign = matches!(self.at(i + dbl + 1), Some('+' | '-')) as usize;
            let e = digits(i + dbl + 1 + sign);
            if e > 0 {
                dbl += 1 + sign + e;
            }
        }
        let best = dec.max(hex).max(oct).max(dbl);
        let text: String = self.s[i..i + best].iter().collect();
        self.pos = i + best;
        if best == dec && dec >= hex && dec >= oct {
            return match text.parse::<i64>() {
                Ok(v) => Self::sym(Group::Integer, T::Integer, V::Int(v)),
                Err(_) => Self::sym(Group::Double, T::Double, V::Double(text.parse().unwrap_or(f64::INFINITY))),
            };
        }
        if best == hex {
            return Self::sym(Group::Integer, T::Integer, V::Int(i64::from_str_radix(&text[2..], 16).unwrap_or(i64::MAX)));
        }
        if best == oct {
            return Self::sym(Group::Integer, T::Integer, V::Int(i64::from_str_radix(&text, 8).unwrap_or(i64::MAX)));
        }
        Self::sym(Group::Double, T::Double, V::Double(text.parse().unwrap_or(0.0)))
    }

    fn hex_escape(&mut self, n: usize) -> Option<char> {
        let h = self.s.get(self.pos..self.pos + n)?;
        if !h.iter().all(char::is_ascii_hexdigit) {
            return None;
        }
        self.pos += n;
        char::from_u32(u32::from_str_radix(&h.iter().collect::<String>(), 16).ok()?)
    }

    /// `STRING` and `CHARLITERAL`: an unknown escape keeps only its backslash.
    fn string(&mut self, quote: char) -> Result<Sym, CompileError> {
        let mut out = String::new();
        loop {
            let Some(c) = self.at(self.pos) else { return Ok(Self::sym(Group::Eof, T::Eof, V::None)) };
            self.pos += 1;
            match c {
                c if c == quote => return Ok(Self::sym(Group::String, T::String, V::Str(out))),
                '\r' | '\n' => {
                    if c == '\n' || self.at(self.pos) != Some('\n') {
                        self.line += 1;
                    }
                    if c == '\r' && self.at(self.pos) == Some('\n') {
                        self.pos += 1;
                        self.line += 1;
                    }
                    return self.next();
                }
                '\\' => {
                    let Some(e) = self.at(self.pos) else {
                        out.push('\\');
                        continue;
                    };
                    self.pos += 1;
                    match e {
                        'b' => out.push('\u{8}'),
                        't' => out.push('\t'),
                        'n' => out.push('\n'),
                        'f' => out.push('\u{c}'),
                        'r' => out.push('\r'),
                        '"' => out.push('"'),
                        '\'' => out.push('\''),
                        '\\' => out.push('\\'),
                        'x' | 'u' => match self.hex_escape(if e == 'x' { 2 } else { 4 }) {
                            Some(ch) => out.push(ch),
                            None => out.push('\\'),
                        },
                        '0'..='7' if quote == '"' && self.s.get(self.pos..self.pos + 2).is_some_and(|o| o.iter().all(|c| ('0'..='7').contains(c))) => {
                            let o: String = self.s[self.pos - 1..self.pos + 2].iter().collect();
                            self.pos += 2;
                            out.push(char::from_u32(u32::from_str_radix(&o, 8).unwrap()).unwrap_or('\u{fffd}'));
                        }
                        '\r' | '\n' => {
                            self.pos -= 1;
                            out.push('\\');
                        }
                        _ => out.push('\\'),
                    }
                }
                c => out.push(c),
            }
        }
    }

    /// `§name§` identifiers, with escapes and `\{n}` repeats.
    fn oidentifier(&mut self) -> Result<Sym, CompileError> {
        let mut out = String::new();
        let mut repeat = 1;
        loop {
            let Some(c) = self.at(self.pos) else { return Ok(Self::sym(Group::Eof, T::Eof, V::None)) };
            self.pos += 1;
            let ch = match c {
                '\u{a7}' => return Ok(Self::sym(Group::Identifier, T::Identifier, V::Str(out))),
                '\r' | '\n' => {
                    if c == '\n' {
                        self.line += 1;
                    }
                    return self.next();
                }
                '\\' => {
                    let Some(e) = self.at(self.pos) else {
                        return Err(CompileError { line: self.line, message: "Illegal escape sequence".into() });
                    };
                    self.pos += 1;
                    match e {
                        'b' => '\u{8}',
                        't' => '\t',
                        'n' => '\n',
                        'f' => '\u{c}',
                        '\u{a7}' => '\u{a7}',
                        'r' => '\r',
                        '\\' => '\\',
                        'x' | 'u' => match self.hex_escape(if e == 'x' { 2 } else { 4 }) {
                            Some(ch) => ch,
                            None => return Err(CompileError { line: self.line, message: format!("Illegal escape sequence \"\\{e}\"") }),
                        },
                        '{' => {
                            let d = self.s[self.pos..].iter().take_while(|c| c.is_ascii_digit()).count();
                            if d == 0 || self.at(self.pos + d) != Some('}') {
                                return Err(CompileError { line: self.line, message: "Illegal escape sequence \"\\{\"".into() });
                            }
                            repeat = self.s[self.pos..self.pos + d].iter().collect::<String>().parse().unwrap_or(1);
                            self.pos += d + 1;
                            continue;
                        }
                        other => return Err(CompileError { line: self.line, message: format!("Illegal escape sequence \"\\{other}\"") }),
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
}
