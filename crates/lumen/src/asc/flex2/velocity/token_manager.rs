//! Port of the JavaCC-generated `ParserTokenManager` (from `Parser.jjt`): the same lexical states, token kinds, longest-match rule
//! (ties go to the lower kind), MORE/SPECIAL handling and lexical actions; `Token` and `TokenMgrError` too.

use super::char_stream::VelocityCharStream;

pub const EOF: i32 = 0;
pub const LBRACKET: i32 = 1;
pub const RBRACKET: i32 = 2;
pub const COMMA: i32 = 3;
pub const DOUBLEDOT: i32 = 4;
pub const LPAREN: i32 = 5;
pub const RPAREN: i32 = 6;
pub const REFMOD2_RPAREN: i32 = 7;
pub const ESCAPE_DIRECTIVE: i32 = 8;
pub const SET_DIRECTIVE: i32 = 9;
pub const DOLLAR: i32 = 10;
pub const DOLLARBANG: i32 = 11;
pub const HASH: i32 = 15;
pub const DOUBLE_ESCAPE: i32 = 16;
pub const ESCAPE: i32 = 17;
pub const TEXT: i32 = 18;
pub const SINGLE_LINE_COMMENT: i32 = 19;
pub const FORMAL_COMMENT: i32 = 20;
pub const MULTI_LINE_COMMENT: i32 = 21;
pub const WHITESPACE: i32 = 23;
pub const STRING_LITERAL: i32 = 24;
pub const TRUE: i32 = 25;
pub const FALSE: i32 = 26;
pub const NEWLINE: i32 = 27;
pub const MINUS: i32 = 28;
pub const PLUS: i32 = 29;
pub const MULTIPLY: i32 = 30;
pub const DIVIDE: i32 = 31;
pub const MODULUS: i32 = 32;
pub const LOGICAL_AND: i32 = 33;
pub const LOGICAL_OR: i32 = 34;
pub const LOGICAL_LT: i32 = 35;
pub const LOGICAL_LE: i32 = 36;
pub const LOGICAL_GT: i32 = 37;
pub const LOGICAL_GE: i32 = 38;
pub const LOGICAL_EQUALS: i32 = 39;
pub const LOGICAL_NOT_EQUALS: i32 = 40;
pub const LOGICAL_NOT: i32 = 41;
pub const EQUALS: i32 = 42;
pub const END: i32 = 43;
pub const IF_DIRECTIVE: i32 = 44;
pub const ELSEIF_DIRECTIVE: i32 = 45;
pub const ELSE_DIRECTIVE: i32 = 46;
pub const STOP_DIRECTIVE: i32 = 47;
pub const NUMBER_LITERAL: i32 = 49;
pub const WORD: i32 = 52;
pub const IDENTIFIER: i32 = 56;
pub const DOT: i32 = 57;
pub const LCURLY: i32 = 58;
pub const RCURLY: i32 = 59;
pub const REFERENCE_TERMINATOR: i32 = 60;
pub const DIRECTIVE_TERMINATOR: i32 = 61;

pub const DIRECTIVE: i32 = 0;
pub const REFMOD2: i32 = 1;
pub const REFMODIFIER: i32 = 2;
pub const DEFAULT: i32 = 3;
pub const PRE_DIRECTIVE: i32 = 4;
pub const REFERENCE: i32 = 5;
pub const IN_MULTI_LINE_COMMENT: i32 = 6;
pub const IN_FORMAL_COMMENT: i32 = 7;
pub const IN_SINGLE_LINE_COMMENT: i32 = 8;

#[derive(Clone, Debug, Default)]
pub struct Token {
    pub kind: i32,
    pub begin_line: i32,
    pub begin_column: i32,
    pub end_line: i32,
    pub end_column: i32,
    pub image: String,
    pub next: Option<usize>,
    pub special_token: Option<usize>,
}

/// `TokenMgrError`'s message.
#[derive(Debug)]
pub struct TokenMgrError(pub String);

// Token kinds tried in each lexical state, in kind order (the "<*>" MORE kinds 10..15 belong to every state).
const STATE_KINDS: [&[i32]; 9] = [
    &[
        1, 2, 3, 4, 5, 6, 10, 11, 12, 13, 14, 15, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39,
        40, 41, 42, 49, 52,
    ],
    &[1, 2, 3, 4, 7, 10, 11, 12, 13, 14, 15, 23, 24, 25, 26, 49, 56, 57, 58, 59, 60],
    &[5, 10, 11, 12, 13, 14, 15, 25, 26, 56, 57, 58, 59, 60],
    &[8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18],
    &[10, 11, 12, 13, 14, 15, 43, 44, 45, 46, 47, 49, 52, 61],
    &[10, 11, 12, 13, 14, 15, 25, 26, 56, 57, 58, 59, 60],
    &[10, 11, 12, 13, 14, 15, 21, 22],
    &[10, 11, 12, 13, 14, 15, 20, 22],
    &[10, 11, 12, 13, 14, 15, 19, 22],
];

const MORE_KINDS: u64 = 0x40fc00;
const SKIP_KINDS: u64 = 0x3000000000000000;

fn is_alpha(c: u16) -> bool {
    (b'a' as u16..=b'z' as u16).contains(&c) || (b'A' as u16..=b'Z' as u16).contains(&c)
}

fn is_digit(c: u16) -> bool {
    (b'0' as u16..=b'9' as u16).contains(&c)
}

fn ch(s: &[u16], i: usize) -> Option<u16> {
    s.get(i).copied()
}

fn is(s: &[u16], i: usize, c: u8) -> bool {
    ch(s, i) == Some(c as u16)
}

fn lit(s: &[u16], text: &str) -> Option<usize> {
    let b = text.as_bytes();
    if s.len() >= b.len() && b.iter().zip(s).all(|(&x, &y)| x as u16 == y) {
        Some(b.len())
    } else {
        None
    }
}

fn count(s: &[u16], from: usize, c: u8) -> usize {
    s[from.min(s.len())..].iter().take_while(|&&x| x == c as u16).count()
}

// ( (" "|"\t")* ("\n"|"\r"|"\r\n") )? after a prefix of length `n`.
fn opt_line_end(s: &[u16], n: usize) -> usize {
    let mut i = n;
    while is(s, i, b' ') || is(s, i, b'\t') {
        i += 1;
    }
    if is(s, i, b'\n') {
        i + 1
    } else if is(s, i, b'\r') {
        if is(s, i + 1, b'\n') {
            i + 2
        } else {
            i + 1
        }
    } else {
        n
    }
}

fn newline(s: &[u16]) -> Option<usize> {
    if is(s, 0, b'\n') {
        Some(1)
    } else if is(s, 0, b'\r') {
        Some(if is(s, 1, b'\n') { 2 } else { 1 })
    } else {
        None
    }
}

fn word(s: &[u16], i: usize) -> usize {
    match ch(s, i) {
        Some(c) if is_alpha(c) || c == b'_' as u16 => {
            1 + s[i + 1..].iter().take_while(|&&c| is_alpha(c) || is_digit(c) || c == b'_' as u16).count()
        }
        _ => 0,
    }
}

fn string_literal(s: &[u16]) -> Option<usize> {
    let q = ch(s, 0)?;
    let mut best = None;
    if q == b'"' as u16 {
        // NFA over positions inside the literal.
        let mut cur = vec![false; s.len() + 2];
        cur[1] = true;
        for i in 1..s.len() {
            if !cur[i] {
                continue;
            }
            let c = s[i];
            if c == b'"' as u16 {
                best = Some(i + 1);
                continue;
            }
            if c == b'\\' as u16 {
                match ch(s, i + 1) {
                    Some(e) if b"ntbrf\\'\"".iter().any(|&x| x as u16 == e) => cur[i + 2] = true,
                    Some(e) if (b'0' as u16..=b'7' as u16).contains(&e) => {
                        cur[i + 2] = true;
                        let o2 = ch(s, i + 2).is_some_and(|x| (b'0' as u16..=b'7' as u16).contains(&x));
                        if o2 {
                            cur[i + 3] = true;
                            if e <= b'3' as u16
                                && ch(s, i + 3).is_some_and(|x| (b'0' as u16..=b'7' as u16).contains(&x))
                            {
                                cur[i + 4] = true;
                            }
                        }
                    }
                    _ => {}
                }
                let sp = count(s, i + 1, b' ');
                if is(s, i + 1 + sp, b'\n') {
                    cur[i + 2 + sp] = true;
                }
            } else if c != b'\n' as u16 && c != b'\r' as u16 {
                cur[i + 1] = true;
            }
        }
    } else if q == b'\'' as u16 {
        let mut cur = vec![false; s.len() + 2];
        cur[1] = true;
        for i in 1..s.len() {
            if !cur[i] {
                continue;
            }
            let c = s[i];
            if c == b'\'' as u16 {
                best = Some(i + 1);
                continue;
            }
            if c != b'\n' as u16 && c != b'\r' as u16 {
                cur[i + 1] = true;
            }
            if c == b'\\' as u16 {
                let sp = count(s, i + 1, b' ');
                if is(s, i + 1 + sp, b'\n') {
                    cur[i + 2 + sp] = true;
                }
            }
        }
    }
    best
}

fn match_kind(kind: i32, s: &[u16]) -> Option<usize> {
    let c0 = ch(s, 0)?;
    match kind {
        1 => lit(s, "["),
        2 => lit(s, "]"),
        3 => lit(s, ","),
        4 => lit(s, ".."),
        5 => lit(s, "("),
        6 => lit(s, ")").map(|n| opt_line_end(s, n)),
        7 => lit(s, ")"),
        8 => {
            let n = count(s, 0, b'\\');
            if n % 2 == 1 && is(s, n, b'#') {
                let w = word(s, n + 1);
                if w > 0 {
                    Some(n + 1 + w)
                } else {
                    None
                }
            } else {
                None
            }
        }
        9 => {
            let ws = s.iter().take_while(|&&c| c == b' ' as u16 || c == b'\t' as u16).count();
            lit(&s[ws..], "#set").and_then(|_| {
                let sp = count(s, ws + 4, b' ');
                if is(s, ws + 4 + sp, b'(') {
                    Some(ws + 5 + sp)
                } else {
                    None
                }
            })
        }
        10 => {
            let n = count(s, 0, b'\\');
            if is(s, n, b'$') {
                Some(n + 1)
            } else {
                None
            }
        }
        11 => {
            let n = count(s, 0, b'\\');
            if is(s, n, b'$') {
                let m = count(s, n + 1, b'\\');
                if is(s, n + 1 + m, b'!') {
                    Some(n + m + 2)
                } else {
                    None
                }
            } else {
                None
            }
        }
        12 => lit(s, "##"),
        13 => lit(s, "#**").and_then(|_| match ch(s, 3) {
            Some(c) if c != b'#' as u16 => Some(4),
            _ => None,
        }),
        14 => lit(s, "#*"),
        15 => lit(s, "#"),
        16 => lit(s, "\\\\"),
        17 => lit(s, "\\"),
        18 => {
            let n = s.iter().take_while(|&&c| c != b'$' as u16 && c != b'#' as u16 && c != b'\\' as u16).count();
            if n > 0 {
                Some(n)
            } else {
                None
            }
        }
        19 | 27 => newline(s),
        20 | 21 => lit(s, "*#"),
        22 | 60 | 61 => Some(1),
        23 => {
            let n = s.iter().take_while(|&&c| c == b' ' as u16 || c == b'\t' as u16).count();
            if n > 0 {
                Some(n)
            } else {
                None
            }
        }
        24 => string_literal(s),
        25 => lit(s, "true"),
        26 => lit(s, "false"),
        28 => lit(s, "-"),
        29 => lit(s, "+"),
        30 => lit(s, "*"),
        31 => lit(s, "/"),
        32 => lit(s, "%"),
        33 => lit(s, "&&"),
        34 => lit(s, "||"),
        35 => lit(s, "<"),
        36 => lit(s, "<="),
        37 => lit(s, ">"),
        38 => lit(s, ">="),
        39 => lit(s, "=="),
        40 => lit(s, "!="),
        41 => lit(s, "!"),
        42 => lit(s, "="),
        43 => lit(s, "end").map(|n| opt_line_end(s, n)),
        44 => lit(s, "if"),
        45 => lit(s, "elseif"),
        46 => lit(s, "else").map(|n| opt_line_end(s, n)),
        47 => lit(s, "stop"),
        49 => {
            let m = if c0 == b'-' as u16 { 1 } else { 0 };
            let d = s[m.min(s.len())..].iter().take_while(|&&c| is_digit(c)).count();
            if d > 0 {
                Some(m + d)
            } else {
                None
            }
        }
        52 => {
            let w = word(s, 0);
            if w > 0 {
                Some(w)
            } else {
                None
            }
        }
        56 => {
            if is_alpha(c0) || c0 == b'_' as u16 {
                Some(
                    1 + s[1..]
                        .iter()
                        .take_while(|&&c| is_alpha(c) || is_digit(c) || c == b'-' as u16 || c == b'_' as u16)
                        .count(),
                )
            } else {
                None
            }
        }
        57 => {
            if c0 == b'.' as u16 && ch(s, 1).is_some_and(is_alpha) {
                Some(2)
            } else {
                None
            }
        }
        58 => lit(s, "{"),
        59 => lit(s, "}"),
        _ => None,
    }
}

pub struct ParserTokenManager {
    pub input_stream: VelocityCharStream,
    pub tokens: Vec<Token>,
    pub cur_lex_state: i32,
    state_stack: Vec<(i32, i32, i32)>,
    lparen: i32,
    rparen: i32,
    in_reference: bool,
    pub in_directive: bool,
    in_comment: bool,
    pub in_set: bool,
    file_depth: i32,
    jjmatched_kind: i32,
}

impl ParserTokenManager {
    pub fn new(input: &[u16]) -> ParserTokenManager {
        ParserTokenManager {
            input_stream: VelocityCharStream::new(input),
            tokens: vec![Token::default()],
            cur_lex_state: DEFAULT,
            state_stack: Vec::new(),
            lparen: 0,
            rparen: 0,
            in_reference: false,
            in_directive: false,
            in_comment: false,
            in_set: false,
            file_depth: 0,
            jjmatched_kind: 0,
        }
    }

    pub fn state_stack_pop(&mut self) -> bool {
        match self.state_stack.pop() {
            None => {
                self.lparen = 0;
                self.switch_to(DEFAULT);
                false
            }
            Some((state, lparen, rparen)) => {
                self.lparen = lparen;
                self.rparen = rparen;
                self.switch_to(state);
                true
            }
        }
    }

    pub fn state_stack_push(&mut self) -> bool {
        self.state_stack.push((self.cur_lex_state, self.lparen, self.rparen));
        self.lparen = 0;
        true
    }

    pub fn clear_state_vars(&mut self) {
        self.state_stack.clear();
        self.lparen = 0;
        self.rparen = 0;
        self.in_reference = false;
        self.in_directive = false;
        self.in_comment = false;
        self.in_set = false;
    }

    fn rparen_handler(&mut self) {
        let mut closed = false;
        if self.in_comment {
            closed = true;
        }
        while !closed {
            if self.lparen > 0 {
                if self.lparen == self.rparen + 1 {
                    self.state_stack_pop();
                } else {
                    self.rparen += 1;
                }
                closed = true;
            } else if !self.state_stack_pop() {
                break;
            }
        }
    }

    pub fn switch_to(&mut self, lex_state: i32) {
        self.cur_lex_state = lex_state;
    }

    fn fill_token(&mut self) -> usize {
        let s = &self.input_stream;
        let t = Token {
            kind: self.jjmatched_kind,
            image: s.get_image(),
            begin_line: s.get_begin_line(),
            begin_column: s.get_begin_column(),
            end_line: s.get_end_line(),
            end_column: s.get_end_column(),
            next: None,
            special_token: None,
        };
        self.tokens.push(t);
        self.tokens.len() - 1
    }

    // Longest match at the current character; ties go to the lowest kind, as in the generated DFA/NFA code.
    fn longest_match(&self) -> Option<(i32, usize)> {
        let s = self.input_stream.remaining();
        let mut best: Option<(i32, usize)> = None;
        for &kind in STATE_KINDS[self.cur_lex_state as usize] {
            if let Some(len) = match_kind(kind, s) {
                if best.is_none_or(|(_, l)| len > l) {
                    best = Some((kind, len));
                }
            }
        }
        best
    }

    pub fn get_next_token(&mut self) -> Result<usize, TokenMgrError> {
        let mut special_token: Option<usize> = None;
        'eof: loop {
            let mut cur_char = match self.input_stream.begin_token() {
                Ok(c) => c,
                Err(_) => {
                    self.jjmatched_kind = 0;
                    let t = self.fill_token();
                    self.tokens[t].special_token = special_token;
                    return Ok(t);
                }
            };
            loop {
                let Some((kind, len)) = self.longest_match() else {
                    return Err(self.lexical_error(cur_char));
                };
                self.jjmatched_kind = kind;
                self.input_stream.backup(-(len as isize - 1));
                let bit = 1u64 << kind;
                if bit & (MORE_KINDS | SKIP_KINDS) == 0 {
                    let t = self.fill_token();
                    self.tokens[t].special_token = special_token;
                    self.token_lexical_actions(t);
                    return Ok(t);
                } else if bit & SKIP_KINDS != 0 {
                    let t = self.fill_token();
                    match special_token {
                        None => special_token = Some(t),
                        Some(prev) => {
                            self.tokens[t].special_token = Some(prev);
                            self.tokens[prev].next = Some(t);
                            special_token = Some(t);
                        }
                    }
                    self.skip_lexical_actions();
                    continue 'eof;
                }
                self.more_lexical_actions();
                match self.input_stream.read_char() {
                    Ok(c) => cur_char = c,
                    Err(_) => return Err(self.lexical_error(cur_char)),
                }
            }
        }
    }

    fn lexical_error(&mut self, cur_char: u16) -> TokenMgrError {
        let s = &mut self.input_stream;
        let mut error_line = s.get_end_line();
        let mut error_column = s.get_end_column();
        let eof_seen = s.read_char().is_err();
        if !eof_seen {
            s.backup(1);
        }
        let error_after = s.get_image();
        if eof_seen {
            if cur_char == b'\n' as u16 || cur_char == b'\r' as u16 {
                error_line += 1;
                error_column = 0;
            } else {
                error_column += 1;
            }
        }
        let c = crate::asc::util::jstr::from_unit(cur_char);
        TokenMgrError(format!(
            "Lexical error at line {}, column {}.  Encountered: {}after : \"{}\"",
            error_line,
            error_column,
            if eof_seen { "<EOF> ".to_string() } else { format!("\"{}\" ({}), ", add_escapes(&c), cur_char) },
            add_escapes(&error_after)
        ))
    }

    fn skip_lexical_actions(&mut self) {
        match self.jjmatched_kind {
            60 => {
                self.input_stream.backup(1);
                self.in_reference = false;
                self.state_stack_pop();
            }
            61 => {
                self.input_stream.backup(1);
                self.in_directive = false;
                self.state_stack_pop();
            }
            _ => {}
        }
    }

    fn more_lexical_actions(&mut self) {
        match self.jjmatched_kind {
            10 | 11 => {
                if !self.in_comment {
                    if self.cur_lex_state == REFERENCE {
                        self.in_reference = false;
                        self.state_stack_pop();
                    }
                    self.in_reference = true;
                    self.state_stack_push();
                    self.switch_to(REFERENCE);
                }
            }
            12 => {
                if !self.in_comment {
                    if self.cur_lex_state == REFERENCE {
                        self.in_reference = false;
                        self.state_stack_pop();
                    }
                    self.in_comment = true;
                    self.state_stack_push();
                    self.switch_to(IN_SINGLE_LINE_COMMENT);
                }
            }
            13 => {
                self.input_stream.backup(1);
                self.in_comment = true;
                self.state_stack_push();
                self.switch_to(IN_FORMAL_COMMENT);
            }
            14 => {
                self.in_comment = true;
                self.state_stack_push();
                self.switch_to(IN_MULTI_LINE_COMMENT);
            }
            15 => {
                if !self.in_comment {
                    if self.cur_lex_state == REFERENCE || self.cur_lex_state == REFMODIFIER {
                        self.in_reference = false;
                        self.state_stack_pop();
                    }
                    self.in_directive = true;
                    self.state_stack_push();
                    self.switch_to(PRE_DIRECTIVE);
                }
            }
            _ => {}
        }
    }

    fn token_lexical_actions(&mut self, t: usize) {
        match self.jjmatched_kind {
            5 => {
                if !self.in_comment {
                    self.lparen += 1;
                }
                if self.cur_lex_state == REFMODIFIER {
                    self.switch_to(REFMOD2);
                }
            }
            6 => self.rparen_handler(),
            7 => self.switch_to(REFERENCE),
            9 => {
                if !self.in_comment {
                    self.in_directive = true;
                    self.state_stack_push();
                    self.in_set = true;
                    self.switch_to(DIRECTIVE);
                }
                if !self.in_comment {
                    self.lparen += 1;
                    if self.cur_lex_state == REFMODIFIER {
                        self.switch_to(REFMOD2);
                    }
                }
            }
            19..=21 => {
                self.in_comment = false;
                self.state_stack_pop();
            }
            24 => {
                if self.cur_lex_state == DIRECTIVE && !self.in_set && self.lparen == 0 {
                    self.state_stack_pop();
                }
            }
            27 => {
                self.state_stack_pop();
                if self.in_set {
                    self.in_set = false;
                }
                if self.in_directive {
                    self.in_directive = false;
                }
            }
            43 | 46 => {
                self.in_directive = false;
                self.state_stack_pop();
            }
            44 | 45 => self.switch_to(DIRECTIVE),
            47 => {
                self.tokens[t].kind = EOF;
                self.file_depth = 0;
            }
            49 => {
                if self.lparen == 0 && !self.in_set && self.cur_lex_state != REFMOD2 {
                    self.state_stack_pop();
                }
            }
            57 => {
                self.input_stream.backup(1);
                self.tokens[t].image = ".".to_string();
                self.switch_to(REFMODIFIER);
            }
            59 => {
                self.state_stack_pop();
            }
            _ => {}
        }
    }
}

pub fn add_escapes(s: &str) -> String {
    let mut r = String::new();
    for u in crate::asc::util::jstr::units(s) {
        match u {
            0 => {}
            8 => r.push_str("\\b"),
            9 => r.push_str("\\t"),
            10 => r.push_str("\\n"),
            12 => r.push_str("\\f"),
            13 => r.push_str("\\r"),
            0x22 => r.push_str("\\\""),
            0x27 => r.push_str("\\'"),
            0x5C => r.push_str("\\\\"),
            c if !(0x20..=0x7e).contains(&c) => r.push_str(&format!("\\u{:04x}", c)),
            c => r.push(c as u8 as char),
        }
    }
    r
}
