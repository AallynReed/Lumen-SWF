//! Port of `macromedia/asc/parser/Scanner.java` (with `States.java` as the `states` module).

#![allow(non_upper_case_globals)]

use std::cell::{RefCell, RefMut};
use std::rc::Rc;
use std::sync::atomic::Ordering;

use self::states::*;
use super::chars::*;
use super::input::InputBuffer;
use super::tokens::*;
use crate::asc::util::context::{java_length, java_substring, Context, USE_VERBOSE_ERRORS};
use crate::asc::util::errors::*;

pub mod states {
    pub const start_state: i32 = 0;
    pub const error_state: i32 = start_state - 1;

    pub const dot_state: i32 = start_state + 1;
    pub const slashequal_state: i32 = dot_state + 1;
    pub const slash_state: i32 = slashequal_state + 1;
    pub const A_state: i32 = slash_state + 1;
    pub const zero_state: i32 = A_state + 1;
    pub const decimalinteger_state: i32 = zero_state + 1;
    pub const decimal_state: i32 = decimalinteger_state + 1;
    pub const exponentstart_state: i32 = decimal_state + 1;
    pub const exponent_state: i32 = exponentstart_state + 1;
    pub const hexinteger_state: i32 = exponent_state + 1;
    pub const slashregexp_state: i32 = hexinteger_state + 1;
    pub const regexp_state: i32 = slashregexp_state + 1;

    pub const blockcommentstart_state: i32 = regexp_state + 1;
    pub const blockcomment_state: i32 = blockcommentstart_state + 1;
    pub const blockcommentstar_state: i32 = blockcomment_state + 1;

    pub const doccomment_state: i32 = blockcommentstar_state + 1;
    pub const doccommentstar_state: i32 = doccomment_state + 1;
    pub const doccommenttag_state: i32 = doccommentstar_state + 1;
    pub const doccommentvalue_state: i32 = doccommenttag_state + 1;

    pub const startxml_state: i32 = doccommentvalue_state + 1;
    pub const startxmlname_state: i32 = startxml_state + 1;
    pub const xmlliteral_state: i32 = startxmlname_state + 1;
    pub const endxml_state: i32 = xmlliteral_state + 1;
    pub const endxmlname_state: i32 = endxml_state + 1;
    pub const xmlcommentorcdatastart_state: i32 = endxmlname_state + 1;
    pub const xmlcdata_state: i32 = xmlcommentorcdatastart_state + 1;
    pub const xmlcomment_state: i32 = xmlcdata_state + 1;
    pub const xmlpi_state: i32 = xmlcomment_state + 1;
    pub const xmltext_state: i32 = xmlpi_state + 1;
}

const DEBUG: bool = false;

// Features.HAS_HASHPRAGMAS
const HAS_HASHPRAGMAS: bool = false;

#[derive(Default)]
struct Tok {
    id: i32,
    lookback: i32,
    text: Option<String>,
}

pub struct Scanner {
    current_token: Tok,
    is_first_token_on_line: bool,
    save_comments: bool,
    ctx: Rc<RefCell<Context>>,
    pub input: Option<Rc<RefCell<InputBuffer>>>,

    pub state: i32,

    level: i32,
    in_xml: i32,
    states: Vec<i32>,
    levels: Vec<i32>,
}

fn reserved_word(s: &str) -> Option<i32> {
    Some(match s {
        "as" => AS_TOKEN,
        "break" => BREAK_TOKEN,
        "case" => CASE_TOKEN,
        "catch" => CATCH_TOKEN,
        "class" => CLASS_TOKEN,
        "const" => CONST_TOKEN,
        "continue" => CONTINUE_TOKEN,
        "default" => DEFAULT_TOKEN,
        "delete" => DELETE_TOKEN,
        "do" => DO_TOKEN,
        "else" => ELSE_TOKEN,
        "extends" => EXTENDS_TOKEN,
        "false" => FALSE_TOKEN,
        "finally" => FINALLY_TOKEN,
        "for" => FOR_TOKEN,
        "function" => FUNCTION_TOKEN,
        "get" => GET_TOKEN,
        "if" => IF_TOKEN,
        "implements" => IMPLEMENTS_TOKEN,
        "import" => IMPORT_TOKEN,
        "in" => IN_TOKEN,
        "include" => INCLUDE_TOKEN,
        "instanceof" => INSTANCEOF_TOKEN,
        "interface" => INTERFACE_TOKEN,
        "is" => IS_TOKEN,
        "namespace" => NAMESPACE_TOKEN,
        "new" => NEW_TOKEN,
        "null" => NULL_TOKEN,
        "package" => PACKAGE_TOKEN,
        "private" => PRIVATE_TOKEN,
        "protected" => PROTECTED_TOKEN,
        "public" => PUBLIC_TOKEN,
        "return" => RETURN_TOKEN,
        "set" => SET_TOKEN,
        "super" => SUPER_TOKEN,
        "switch" => SWITCH_TOKEN,
        "this" => THIS_TOKEN,
        "throw" => THROW_TOKEN,
        "true" => TRUE_TOKEN,
        "try" => TRY_TOKEN,
        "typeof" => TYPEOF_TOKEN,
        "use" => USE_TOKEN,
        "var" => VAR_TOKEN,
        "void" => VOID_TOKEN,
        "while" => WHILE_TOKEN,
        "with" => WITH_TOKEN,
        _ => return None,
    })
}

const fn ch(c: u8) -> u16 {
    c as u16
}

fn is_hex_digit(c: u16) -> bool {
    (ch(b'0')..=ch(b'9')).contains(&c) || (ch(b'a')..=ch(b'f')).contains(&c) || (ch(b'A')..=ch(b'F')).contains(&c)
}

fn is_digit(c: u16) -> bool {
    (ch(b'0')..=ch(b'9')).contains(&c)
}

fn push_str(buf: &mut Vec<u16>, s: &str) {
    buf.extend(s.encode_utf16());
}

impl Scanner {
    fn init(cx: Rc<RefCell<Context>>, save_comments: bool, input: Rc<RefCell<InputBuffer>>) -> Scanner {
        cx.borrow_mut().input = Some(input.clone());
        Scanner {
            current_token: Tok::default(),
            is_first_token_on_line: false,
            save_comments,
            ctx: cx,
            input: Some(input),
            state: start_state,
            level: 0,
            in_xml: 0,
            states: Vec::new(),
            levels: Vec::new(),
        }
    }

    pub fn new_stream(cx: Rc<RefCell<Context>>, in_: &[u8], encoding: Option<&str>, origin: Option<String>) -> Scanner {
        Self::new_stream_save(cx, in_, encoding, origin, true)
    }

    pub fn new_stream_save(
        cx: Rc<RefCell<Context>>,
        in_: &[u8],
        encoding: Option<&str>,
        origin: Option<String>,
        save_comments: bool,
    ) -> Scanner {
        let input = Rc::new(RefCell::new(InputBuffer::new_from_stream(in_, encoding, origin)));
        Self::init(cx, save_comments, input)
    }

    pub fn new(cx: Rc<RefCell<Context>>, in_: &str, origin: Option<String>) -> Scanner {
        Self::new_save(cx, in_, origin, true)
    }

    pub fn new_save(cx: Rc<RefCell<Context>>, in_: &str, origin: Option<String>, save_comments: bool) -> Scanner {
        let input = Rc::new(RefCell::new(InputBuffer::new(in_, origin)));
        Self::init(cx, save_comments, input)
    }

    pub fn new_input(cx: Rc<RefCell<Context>>, input: Rc<RefCell<InputBuffer>>) -> Scanner {
        Self::init(cx, true, input)
    }

    /// `scanner.input`; never hold the guard across another scanner call.
    pub fn input(&self) -> RefMut<'_, InputBuffer> {
        self.input.as_ref().unwrap().borrow_mut()
    }

    fn nextchar(&self) -> u16 {
        self.input().nextchar() as u16
    }

    pub fn retract(&self) {
        self.input().retract();
    }

    fn pos(&self) -> i32 {
        self.input().text_pos()
    }

    fn mark(&self) {
        self.input().text_mark();
    }

    fn make_comment_token(&mut self, id: i32, text: Option<String>) -> i32 {
        self.current_token.id = id;
        self.current_token.text = text;
        id
    }

    fn make_token_text(&mut self, id: i32, text: Option<String>) -> i32 {
        self.current_token.id = id;
        self.current_token.lookback = id;
        self.current_token.text = text;
        id
    }

    fn make_token(&mut self, id: i32) -> i32 {
        self.current_token.id = id;
        self.current_token.lookback = id;
        self.current_token.text = None;
        id
    }

    pub fn get_current_token_text(&mut self) -> Option<String> {
        if self.current_token.text.is_none() {
            self.error_msg("Scanner internal: current token not a pseudo-terminal");
        }
        self.current_token.text.clone()
    }

    pub fn get_current_token_text_or_type_text(&self, id: i32) -> String {
        if let Some(text) = &self.current_token.text {
            return text.clone();
        }
        TOKEN_CLASS_NAMES[(-id) as usize].to_string()
    }

    pub fn get_current_string_token_text(&mut self, is_single_quoted: &mut bool) -> String {
        if self.current_token.id != STRINGLITERAL_TOKEN || self.current_token.text.is_none() {
            self.error_msg("internal: string token expected.");
        }
        let fulltext = self.current_token.text.clone().unwrap();
        *is_single_quoted = fulltext.starts_with('\'');
        java_substring(&fulltext, 1, java_length(&fulltext) - 1)
    }

    fn error_arg(&mut self, kind: i32, arg: &str) {
        let mut out = String::new();

        let (origin, err_pos, ln, col) = {
            let mut input = self.input();
            let origin = input.origin.clone();
            let err_pos = input.position_of_mark();
            let ln = input.get_ln_num(err_pos);
            let col = input.get_col_pos(err_pos);
            (origin, err_pos, ln, col)
        };

        let prefix = if USE_VERBOSE_ERRORS.load(Ordering::Relaxed) {
            format!("[Compiler] Error #{}: ", kind)
        } else {
            String::new()
        };
        let error_string = self.ctx.borrow().error_string(kind);
        let mut msg = prefix + error_string.as_deref().unwrap_or("null");

        if DEBUG {
            msg = "[Scanner] ".to_string() + &msg;
        }

        let next_loc = Context::replace_string_arg(&mut out, &msg, 0, arg);
        if next_loc != -1 {
            out.push_str(&java_substring(&msg, next_loc, java_length(&msg)));
        }

        let line_text = self.input().get_line_text(err_pos);
        // A null Java origin reaches the handler, which prints it as "null".
        let origin = origin.unwrap_or_else(|| "null".to_string());
        self.ctx.borrow().localized_error_ln_col_code(&origin, ln, col, &out, &line_text, kind);
        self.skiperror_kind(kind);
    }

    fn error_msg(&mut self, msg: &str) {
        self.ctx.borrow().internal_error(msg);
        self.error_arg(kError_Lexical_General, msg);
    }

    fn error(&mut self, kind: i32) {
        self.error_arg(kind, "");
    }

    fn skiperror(&mut self) {
        self.skiperror_kind(kError_Lexical_General);
    }

    fn skiperror_kind(&mut self, kind: i32) {
        match kind {
            kError_Lexical_General => {}

            kError_Lexical_LineTerminatorInSingleQuotedStringLiteral
            | kError_Lexical_LineTerminatorInDoubleQuotedStringLiteral => loop {
                let nc = self.nextchar();
                if nc == ch(b'\'') || nc == 0 {
                    return;
                }
            },

            _ => loop {
                let nc = self.nextchar();
                if nc == ch(b';') || nc == ch(b'\n') || nc == ch(b'\r') || nc == 0 {
                    return;
                }
            },
        }
    }

    pub fn follows_line_terminator(&self) -> bool {
        if DEBUG {
            println!("isFirstTokenOnLine = {}", self.is_first_token_on_line);
        }
        self.is_first_token_on_line
    }

    pub fn push_state(&mut self) {
        self.states.push(self.state);
        self.levels.push(self.level);
        self.state = start_state;
        self.level = 0;
        self.in_xml += 1;
    }

    pub fn pop_state(&mut self) {
        self.state = self.states.pop().unwrap_or(0);
        self.level = self.levels.pop().unwrap_or(0);
        if self.in_xml > 0 {
            self.in_xml -= 1;
        }
    }

    fn get_doc_text_buffer(doctagname: &[u16]) -> Vec<u16> {
        let mut doctextbuf = Vec::new();
        push_str(&mut doctextbuf, "<");
        doctextbuf.extend_from_slice(doctagname);
        push_str(&mut doctextbuf, "><![CDATA[");
        doctextbuf
    }

    pub fn clear_unused_buffers(&mut self) {
        self.input().clear_unused_buffers();
        self.input = None;
    }

    fn es4_numerics(&self) -> bool {
        self.ctx.borrow().statics.borrow().es4_numerics
    }

    pub fn nexttoken(&mut self, reset_state: bool) -> i32 {
        let mut doctagname: Vec<u16> = "description".encode_utf16().collect();
        let mut doctextbuf: Option<Vec<u16>> = None;
        let startofxml = self.pos();
        let mut blockcommentbuf: Option<Vec<u16>> = None;
        let mut regexp_flags: u16 = 0;
        let mut maybe_reserved = false;
        let mut c: u16 = 0;

        if reset_state {
            self.is_first_token_on_line = false;
        }

        loop {
            if DEBUG {
                println!("state = {}, next = {}", self.state, self.pos());
            }

            match self.state {
                start_state => {
                    c = self.nextchar();
                    self.mark();

                    match c {
                        0x61..=0x7a => {
                            maybe_reserved = true;
                            self.state = A_state;
                            continue;
                        }
                        0x41..=0x5a | 0x5f | 0x24 => {
                            self.state = A_state;
                            continue;
                        }

                        0xffef => {
                            // A char never equals these negative ints, so this always errors after one read.
                            if self.nextchar() as i32 == 0xffffffbbu32 as i32 && self.nextchar() as i32 == 0xffffffbfu32 as i32 {
                                self.state = start_state;
                            } else {
                                self.state = error_state;
                            }
                            continue;
                        }

                        0x40 => return self.make_token(ATSIGN_TOKEN),

                        0x27 | 0x22 => {
                            let startquote = c;
                            let mut needs_escape = false;

                            loop {
                                c = self.nextchar();
                                if c == startquote {
                                    break;
                                }
                                if c == ch(b'\\') {
                                    needs_escape = true;
                                    c = self.nextchar();

                                    if c == ch(b'\r') {
                                        c = self.nextchar();
                                    }
                                    if c == ch(b'\n') {
                                        continue;
                                    }
                                } else if c == ch(b'\r') || c == ch(b'\n') {
                                    if startquote == ch(b'\'') {
                                        self.error(kError_Lexical_LineTerminatorInSingleQuotedStringLiteral);
                                    } else {
                                        self.error(kError_Lexical_LineTerminatorInDoubleQuotedStringLiteral);
                                    }
                                    break;
                                } else if c == 0 {
                                    self.error(kError_Lexical_EndOfStreamInStringLiteral);
                                    return self.make_token(EOS_TOKEN);
                                }
                            }
                            let text = self.input().copy_replace_string_escapes(needs_escape);
                            return self.make_token_text(STRINGLITERAL_TOKEN, Some(text));
                        }

                        0x2d => {
                            return match self.nextchar() {
                                0x2d => self.make_token(MINUSMINUS_TOKEN),
                                0x3d => self.make_token(MINUSASSIGN_TOKEN),
                                _ => {
                                    self.retract();
                                    self.make_token(MINUS_TOKEN)
                                }
                            };
                        }

                        0x21 => {
                            if self.nextchar() == ch(b'=') {
                                if self.nextchar() == ch(b'=') {
                                    return self.make_token(STRICTNOTEQUALS_TOKEN);
                                }
                                self.retract();
                                return self.make_token(NOTEQUALS_TOKEN);
                            }
                            self.retract();
                            return self.make_token(NOT_TOKEN);
                        }

                        0x25 => {
                            return match self.nextchar() {
                                0x3d => self.make_token(MODULUSASSIGN_TOKEN),
                                _ => {
                                    self.retract();
                                    self.make_token(MODULUS_TOKEN)
                                }
                            };
                        }

                        0x26 => {
                            c = self.nextchar();
                            if c == ch(b'=') {
                                return self.make_token(BITWISEANDASSIGN_TOKEN);
                            }
                            if c == ch(b'&') {
                                if self.nextchar() == ch(b'=') {
                                    return self.make_token(LOGICALANDASSIGN_TOKEN);
                                }
                                self.retract();
                                return self.make_token(LOGICALAND_TOKEN);
                            }
                            self.retract();
                            return self.make_token(BITWISEAND_TOKEN);
                        }

                        0x23 => {
                            if HAS_HASHPRAGMAS {
                                return self.make_token(USE_TOKEN);
                            }
                            self.state = error_state;
                            continue;
                        }

                        0x28 => return self.make_token(LEFTPAREN_TOKEN),

                        0x29 => return self.make_token(RIGHTPAREN_TOKEN),

                        0x2a => {
                            if self.nextchar() == ch(b'=') {
                                return self.make_token(MULTASSIGN_TOKEN);
                            }
                            self.retract();
                            return self.make_token(MULT_TOKEN);
                        }

                        0x2c => return self.make_token(COMMA_TOKEN),

                        0x2e => {
                            self.state = dot_state;
                            continue;
                        }

                        0x2f => {
                            self.state = slash_state;
                            continue;
                        }

                        0x3a => {
                            if self.nextchar() == ch(b':') {
                                return self.make_token(DOUBLECOLON_TOKEN);
                            }
                            self.retract();
                            return self.make_token(COLON_TOKEN);
                        }

                        0x3b => return self.make_token(SEMICOLON_TOKEN),

                        0x3f => return self.make_token(QUESTIONMARK_TOKEN),

                        0x5b => return self.make_token(LEFTBRACKET_TOKEN),

                        0x5d => return self.make_token(RIGHTBRACKET_TOKEN),

                        0x5e => {
                            if self.nextchar() == ch(b'=') {
                                return self.make_token(BITWISEXORASSIGN_TOKEN);
                            }
                            self.retract();
                            return self.make_token(BITWISEXOR_TOKEN);
                        }

                        0x7b => return self.make_token(LEFTBRACE_TOKEN),

                        0x7c => {
                            c = self.nextchar();
                            if c == ch(b'=') {
                                return self.make_token(BITWISEORASSIGN_TOKEN);
                            }
                            if c == ch(b'|') {
                                if self.nextchar() == ch(b'=') {
                                    return self.make_token(LOGICALORASSIGN_TOKEN);
                                }
                                self.retract();
                                return self.make_token(LOGICALOR_TOKEN);
                            }
                            self.retract();
                            return self.make_token(BITWISEOR_TOKEN);
                        }

                        0x7d => return self.make_token(RIGHTBRACE_TOKEN),

                        0x7e => return self.make_token(BITWISENOT_TOKEN),

                        0x2b => {
                            c = self.nextchar();
                            if c == ch(b'+') {
                                return self.make_token(PLUSPLUS_TOKEN);
                            }
                            if c == ch(b'=') {
                                return self.make_token(PLUSASSIGN_TOKEN);
                            }
                            self.retract();
                            return self.make_token(PLUS_TOKEN);
                        }

                        0x3c => {
                            match self.nextchar() {
                                0x3c => {
                                    if self.nextchar() == ch(b'=') {
                                        return self.make_token(LEFTSHIFTASSIGN_TOKEN);
                                    }
                                    self.retract();
                                    return self.make_token(LEFTSHIFT_TOKEN);
                                }

                                0x3d => return self.make_token(LESSTHANOREQUALS_TOKEN),

                                0x2f => return self.make_token(XMLTAGSTARTEND_TOKEN),
                                0x21 => {
                                    self.state = xmlcommentorcdatastart_state;
                                    continue;
                                }
                                0x3f => {
                                    self.state = xmlpi_state;
                                    continue;
                                }
                                _ => {}
                            }
                            self.retract();
                            return self.make_token(LESSTHAN_TOKEN);
                        }

                        0x3d => {
                            if self.nextchar() == ch(b'=') {
                                if self.nextchar() == ch(b'=') {
                                    return self.make_token(STRICTEQUALS_TOKEN);
                                }
                                self.retract();
                                return self.make_token(EQUALS_TOKEN);
                            }
                            self.retract();
                            return self.make_token(ASSIGN_TOKEN);
                        }

                        0x3e => {
                            self.state = start_state;

                            match self.nextchar() {
                                0x3e => {
                                    return match self.nextchar() {
                                        0x3e => {
                                            if self.nextchar() == ch(b'=') {
                                                return self.make_token(UNSIGNEDRIGHTSHIFTASSIGN_TOKEN);
                                            }
                                            self.retract();
                                            self.make_token(UNSIGNEDRIGHTSHIFT_TOKEN)
                                        }
                                        0x3d => self.make_token(RIGHTSHIFTASSIGN_TOKEN),
                                        _ => {
                                            self.retract();
                                            self.make_token(RIGHTSHIFT_TOKEN)
                                        }
                                    };
                                }

                                0x3d => return self.make_token(GREATERTHANOREQUALS_TOKEN),
                                _ => {}
                            }
                            self.retract();
                            return self.make_token(GREATERTHAN_TOKEN);
                        }

                        0x30 => {
                            self.state = zero_state;
                            continue;
                        }

                        0x31..=0x39 => {
                            self.state = decimalinteger_state;
                            continue;
                        }

                        0x20 | 0x09 | 0x000b | 0x000c | 0x0085 | 0x00a0 => continue,

                        0x0a | 0x0d => {
                            self.is_first_token_on_line = true;
                            continue;
                        }

                        0 => return self.make_token(EOS_TOKEN),

                        _ => {
                            let class = self.input().next_char_class(c, true);
                            match class {
                                Lu | Ll | Lt | Lm | Lo | Nl => {
                                    maybe_reserved = false;
                                    self.state = A_state;
                                    continue;
                                }

                                Zs | Cc | Cf => continue,

                                Zl | Zp => {
                                    self.is_first_token_on_line = true;
                                    continue;
                                }

                                _ => {
                                    self.state = error_state;
                                    continue;
                                }
                            }
                        }
                    }
                }

                A_state => {
                    let mut needs_escape = c == ch(b'\\');

                    loop {
                        c = self.nextchar();
                        if (ch(b'a')..=ch(b'z')).contains(&c) {
                            continue;
                        }
                        if (ch(b'A')..=ch(b'Z')).contains(&c) || is_digit(c) || c == ch(b'$') || c == ch(b'_') {
                            maybe_reserved = false;
                            continue;
                        }
                        if c <= 0x7f {
                            if c == ch(b'\\') {
                                needs_escape = true;
                            } else {
                                self.retract();
                                break;
                            }
                        }

                        let class = self.input().next_char_class(c, false);
                        match class {
                            Lu | Ll | Lt | Lm | Lo | Nl | Mn | Mc | Nd | Pc => {
                                maybe_reserved = false;
                                self.input().next_char_class(c, true);
                                continue;
                            }
                            _ => {}
                        }

                        self.retract();
                        break;
                    }

                    self.state = start_state;
                    let s = self.input().copy_replace_unicode_escapes_if(needs_escape);
                    if maybe_reserved {
                        if let Some(i) = reserved_word(&s) {
                            return self.make_token(i);
                        }
                    }
                    return self.make_token_text(IDENTIFIER_TOKEN, Some(s));
                }

                zero_state => match self.nextchar() {
                    0x78 | 0x58 => {
                        let n = self.nextchar();
                        if is_hex_digit(n) {
                            self.state = hexinteger_state;
                        } else {
                            self.state = start_state;
                            self.error(kError_Lexical_General);
                        }
                        continue;
                    }

                    0x2e => {
                        self.state = decimal_state;
                        continue;
                    }

                    0x30..=0x39 => {
                        self.state = decimalinteger_state;
                        continue;
                    }
                    0x45 | 0x65 => {
                        self.state = exponentstart_state;
                        continue;
                    }
                    0x64 | 0x6d | 0x69 | 0x75 => {
                        if !self.es4_numerics() {
                            self.retract();
                        }
                        self.state = start_state;
                        let text = self.input().copy();
                        return self.make_token_text(NUMBERLITERAL_TOKEN, Some(text));
                    }
                    _ => {
                        self.retract();
                        self.state = start_state;
                        let text = self.input().copy();
                        return self.make_token_text(NUMBERLITERAL_TOKEN, Some(text));
                    }
                },

                hexinteger_state => match self.nextchar() {
                    n if is_hex_digit(n) => {
                        self.state = hexinteger_state;
                        continue;
                    }
                    0x75 | 0x69 => {
                        if !self.es4_numerics() {
                            self.retract();
                        }
                        self.state = start_state;
                        let text = self.input().copy();
                        return self.make_token_text(NUMBERLITERAL_TOKEN, Some(text));
                    }
                    _ => {
                        self.retract();
                        self.state = start_state;
                        let text = self.input().copy();
                        return self.make_token_text(NUMBERLITERAL_TOKEN, Some(text));
                    }
                },

                dot_state => match self.nextchar() {
                    0x30..=0x39 => {
                        self.state = decimal_state;
                        continue;
                    }

                    0x2e => {
                        self.state = start_state;
                        if self.nextchar() == ch(b'.') {
                            return self.make_token(TRIPLEDOT_TOKEN);
                        }
                        self.retract();
                        return self.make_token(DOUBLEDOT_TOKEN);
                    }

                    0x3c => {
                        self.state = start_state;
                        return self.make_token(DOTLESSTHAN_TOKEN);
                    }

                    _ => {
                        self.retract();
                        self.state = start_state;
                        return self.make_token(DOT_TOKEN);
                    }
                },

                decimalinteger_state => match self.nextchar() {
                    0x30..=0x39 => {
                        self.state = decimalinteger_state;
                        continue;
                    }
                    0x2e => {
                        self.state = decimal_state;
                        continue;
                    }
                    0x64 | 0x6d | 0x75 | 0x69 => {
                        if !self.es4_numerics() {
                            self.retract();
                        }
                        self.state = start_state;
                        let text = self.input().copy();
                        return self.make_token_text(NUMBERLITERAL_TOKEN, Some(text));
                    }
                    0x45 | 0x65 => {
                        self.state = exponentstart_state;
                        continue;
                    }
                    _ => {
                        self.retract();
                        self.state = start_state;
                        let text = self.input().copy();
                        return self.make_token_text(NUMBERLITERAL_TOKEN, Some(text));
                    }
                },

                decimal_state => match self.nextchar() {
                    0x30..=0x39 => {
                        self.state = decimal_state;
                        continue;
                    }
                    0x64 | 0x6d => {
                        if !self.es4_numerics() {
                            self.retract();
                        }
                        self.state = start_state;
                        let text = self.input().copy();
                        return self.make_token_text(NUMBERLITERAL_TOKEN, Some(text));
                    }
                    0x45 | 0x65 => {
                        self.state = exponentstart_state;
                        continue;
                    }
                    _ => {
                        self.retract();
                        self.state = start_state;
                        let text = self.input().copy();
                        return self.make_token_text(NUMBERLITERAL_TOKEN, Some(text));
                    }
                },

                exponentstart_state => match self.nextchar() {
                    0x30..=0x39 | 0x2b | 0x2d => {
                        self.state = exponent_state;
                        continue;
                    }
                    _ => {
                        self.error(kError_Lexical_General);
                        self.state = start_state;
                        continue;
                    }
                },

                exponent_state => match self.nextchar() {
                    0x30..=0x39 => {
                        self.state = exponent_state;
                        continue;
                    }
                    0x64 | 0x6d => {
                        if !self.es4_numerics() {
                            self.retract();
                        }
                        self.state = start_state;
                        let text = self.input().copy();
                        return self.make_token_text(NUMBERLITERAL_TOKEN, Some(text));
                    }
                    _ => {
                        self.retract();
                        self.state = start_state;
                        let text = self.input().copy();
                        return self.make_token_text(NUMBERLITERAL_TOKEN, Some(text));
                    }
                },

                slash_state => {
                    c = self.nextchar();

                    match c {
                        0x2f => {
                            self.state = start_state;
                            loop {
                                c = self.nextchar();
                                if c == 0 {
                                    break;
                                }
                                if c == ch(b'\r') || c == ch(b'\n') {
                                    self.is_first_token_on_line = true;
                                    if !self.save_comments {
                                        break;
                                    }
                                    self.retract();
                                    let text = self.input().copy_replace_unicode_escapes();
                                    return self.make_comment_token(SLASHSLASHCOMMENT_TOKEN, Some(text));
                                }
                            }
                            continue;
                        }

                        0x2a => {
                            if !self.save_comments {
                                loop {
                                    c = self.nextchar();
                                    if c == 0 {
                                        break;
                                    }
                                    if c == ch(b'\r') || c == ch(b'\n') {
                                        self.is_first_token_on_line = true;
                                    }

                                    if c == ch(b'*') {
                                        c = self.nextchar();
                                        if c == ch(b'/') {
                                            break;
                                        }
                                        self.retract();
                                    }
                                }
                                self.state = start_state;
                            } else {
                                push_str(blockcommentbuf.get_or_insert_with(Vec::new), "/*");
                                self.state = blockcommentstart_state;
                            }
                            continue;
                        }

                        _ => {
                            if c == ch(b'>') && self.in_xml > 0 {
                                self.state = start_state;
                                return self.make_token(XMLTAGENDEND_TOKEN);
                            }

                            let lb = self.current_token.lookback;

                            if lb == IDENTIFIER_TOKEN
                                || lb == NUMBERLITERAL_TOKEN
                                || lb == RIGHTPAREN_TOKEN
                                || lb == RIGHTBRACE_TOKEN
                                || lb == RIGHTBRACKET_TOKEN
                            {
                                self.state = start_state;
                                if c == ch(b'=') {
                                    return self.make_token(DIVASSIGN_TOKEN);
                                }
                                self.retract();
                                return self.make_token(DIV_TOKEN);
                            }
                            self.state = slashregexp_state;
                            self.retract();
                            continue;
                        }
                    }
                }

                slashregexp_state => match self.nextchar() {
                    0x5c => {
                        self.nextchar();
                        continue;
                    }
                    0x2f => {
                        regexp_flags = 0;
                        self.state = regexp_state;
                        continue;
                    }
                    0 | 0x0a | 0x0d => {
                        self.error(kError_Lexical_General);
                        self.state = start_state;
                        continue;
                    }
                    _ => {
                        self.state = slashregexp_state;
                        continue;
                    }
                },

                regexp_state => {
                    c = self.nextchar();
                    let flag = match c {
                        0x67 => 0x01,
                        0x69 => 0x02,
                        0x6d => 0x04,
                        0x73 => 0x08,
                        0x78 => 0x10,
                        _ => 0,
                    };
                    if flag != 0 {
                        if (regexp_flags & flag) == 0 {
                            regexp_flags |= flag;
                            continue;
                        }
                        self.error(kError_Lexical_General);
                        self.state = start_state;
                        continue;
                    }
                    if java_is_java_identifier_part(c) {
                        self.error(kError_Lexical_General);
                        self.state = start_state;
                        continue;
                    }
                    self.retract();
                    self.state = start_state;
                    let text = self.input().copy_replace_unicode_escapes();
                    return self.make_token_text(REGEXPLITERAL_TOKEN, Some(text));
                }

                xmlcommentorcdatastart_state => {
                    match self.nextchar() {
                        0x5b => {
                            if self.nextchar() == ch(b'C')
                                && self.nextchar() == ch(b'D')
                                && self.nextchar() == ch(b'A')
                                && self.nextchar() == ch(b'T')
                                && self.nextchar() == ch(b'A')
                                && self.nextchar() == ch(b'[')
                            {
                                self.state = xmlcdata_state;
                                continue;
                            }
                        }

                        0x2d => {
                            if self.nextchar() == ch(b'-') {
                                self.state = xmlcomment_state;
                                continue;
                            }
                        }
                        _ => {}
                    }
                    self.error(kError_Lexical_General);
                    self.state = start_state;
                    continue;
                }

                xmlcdata_state => {
                    match self.nextchar() {
                        0x5d => {
                            if self.nextchar() == ch(b']') && self.nextchar() == ch(b'>') {
                                self.state = start_state;
                                let end = self.pos();
                                let text = self.input().substring_replace_unicode_escapes(startofxml, end);
                                return self.make_token_text(XMLMARKUP_TOKEN, text);
                            }
                            continue;
                        }

                        0 => {
                            self.error(kError_Lexical_General);
                            self.state = start_state;
                        }
                        _ => {}
                    }
                    continue;
                }

                xmlcomment_state => {
                    loop {
                        c = self.nextchar();
                        if !(c != ch(b'-') && c != 0) {
                            break;
                        }
                    }

                    if c == ch(b'-') && self.nextchar() != ch(b'-') {
                        continue;
                    }

                    if self.nextchar() == ch(b'>') {
                        self.state = start_state;
                        let end = self.pos();
                        let text = self.input().substring_replace_unicode_escapes(startofxml, end);
                        return self.make_token_text(XMLMARKUP_TOKEN, text);
                    }

                    self.error(kError_Lexical_General);
                    self.state = start_state;
                    continue;
                }

                xmlpi_state => {
                    loop {
                        c = self.nextchar();
                        if !(c != ch(b'?') && c != 0) {
                            break;
                        }
                    }

                    if c == ch(b'?') && self.nextchar() == ch(b'>') {
                        self.state = start_state;
                        let end = self.pos();
                        let text = self.input().substring_replace_unicode_escapes(startofxml, end);
                        return self.make_token_text(XMLMARKUP_TOKEN, text);
                    }

                    if c == 0 {
                        self.error(kError_Lexical_General);
                        self.state = start_state;
                    }
                    continue;
                }

                xmltext_state => {
                    match self.nextchar() {
                        0x3c | 0x7b => {
                            self.retract();
                            let end = self.pos();
                            let xmltext = self.input().substring_replace_unicode_escapes(startofxml, end);
                            if xmltext.is_some() {
                                self.state = start_state;
                                return self.make_token_text(XMLTEXT_TOKEN, xmltext);
                            } else {
                                match self.nextchar() {
                                    0x3c => match self.nextchar() {
                                        0x2f => {
                                            self.state = start_state;
                                            return self.make_token(XMLTAGSTARTEND_TOKEN);
                                        }
                                        0x21 => {
                                            self.state = xmlcommentorcdatastart_state;
                                            continue;
                                        }
                                        0x3f => {
                                            self.state = xmlpi_state;
                                            continue;
                                        }
                                        _ => {
                                            self.retract();
                                            self.state = start_state;
                                            return self.make_token(LESSTHAN_TOKEN);
                                        }
                                    },
                                    0x7b => {
                                        self.state = start_state;
                                        return self.make_token(LEFTBRACE_TOKEN);
                                    }
                                    _ => {}
                                }
                            }
                            // Java falls through into `case 0`.
                            self.state = start_state;
                            return self.make_token(EOS_TOKEN);
                        }
                        0 => {
                            self.state = start_state;
                            return self.make_token(EOS_TOKEN);
                        }
                        _ => {}
                    }
                    continue;
                }

                xmlliteral_state => match self.nextchar() {
                    0x7b => {
                        let end = self.pos() - 1;
                        let text = self.input().substring_replace_unicode_escapes(startofxml, end);
                        return self.make_token_text(XMLPART_TOKEN, text);
                    }

                    0x3c => {
                        if self.nextchar() == ch(b'/') {
                            self.level -= 1;
                            self.nextchar();
                            self.mark();
                            self.retract();
                            self.state = endxmlname_state;
                        } else {
                            self.level += 1;
                            self.state = xmlliteral_state;
                        }
                        continue;
                    }

                    0x2f => {
                        if self.nextchar() == ch(b'>') {
                            self.level -= 1;
                            if self.level == 0 {
                                self.state = start_state;
                                let end = self.pos() + 1;
                                let text = self.input().substring_replace_unicode_escapes(startofxml, end);
                                return self.make_token_text(XMLLITERAL_TOKEN, text);
                            }
                        }
                        continue;
                    }

                    0 => {
                        self.retract();
                        self.error(kError_Lexical_NoMatchingTag);
                        self.state = start_state;
                        continue;
                    }

                    _ => continue,
                },

                endxmlname_state => {
                    c = self.nextchar();
                    // Java's isJavaIdentifierPart('\0') is true, so EOF here loops forever, as in ASC.
                    if java_is_java_identifier_part(c) || c == ch(b':') {
                        continue;
                    }

                    match c {
                        0x7b => {
                            let end = self.pos() - 1;
                            let xmltext = self.input().substring_replace_unicode_escapes(startofxml, end);
                            return self.make_token_text(XMLPART_TOKEN, xmltext);
                        }
                        0x3e => {
                            self.retract();
                            self.nextchar();
                            if self.level == 0 {
                                let end = self.pos() + 1;
                                let xmltext = self.input().substring_replace_unicode_escapes(startofxml, end);
                                self.state = start_state;
                                return self.make_token_text(XMLLITERAL_TOKEN, xmltext);
                            }
                            self.state = xmlliteral_state;
                            continue;
                        }

                        _ => {
                            self.state = xmlliteral_state;
                            continue;
                        }
                    }
                }

                blockcommentstart_state => {
                    c = self.nextchar();
                    blockcommentbuf.as_mut().unwrap().push(c);
                    match c {
                        0x2a => {
                            if self.nextchar() == ch(b'/') {
                                self.state = start_state;
                                return self.make_comment_token(BLOCKCOMMENT_TOKEN, Some(String::new()));
                            }
                            self.retract();
                            self.state = doccomment_state;
                            continue;
                        }

                        0 => {
                            self.error(kError_BlockCommentNotTerminated);
                            self.state = start_state;
                            continue;
                        }

                        0x0a | 0x0d => {
                            self.is_first_token_on_line = true;
                            self.state = blockcomment_state;
                            continue;
                        }
                        _ => {
                            self.state = blockcomment_state;
                            continue;
                        }
                    }
                }

                doccomment_state => {
                    c = self.nextchar();
                    blockcommentbuf.as_mut().unwrap().push(c);
                    match c {
                        0x2a => {
                            self.state = doccommentstar_state;
                            continue;
                        }

                        0x40 => {
                            let buf = doctextbuf.get_or_insert_with(|| Self::get_doc_text_buffer(&doctagname));
                            if !doctagname.is_empty() {
                                push_str(buf, "]]></");
                                buf.extend_from_slice(&doctagname);
                                push_str(buf, ">");
                            }
                            doctagname = Vec::new();
                            self.state = doccommenttag_state;
                            continue;
                        }

                        0x0d | 0x0a => {
                            self.is_first_token_on_line = true;
                            doctextbuf.get_or_insert_with(|| Self::get_doc_text_buffer(&doctagname)).push(ch(b'\n'));
                            continue;
                        }

                        0 => {
                            self.error(kError_BlockCommentNotTerminated);
                            self.state = start_state;
                            continue;
                        }

                        _ => {
                            doctextbuf.get_or_insert_with(|| Self::get_doc_text_buffer(&doctagname)).push(c);
                            continue;
                        }
                    }
                }

                doccommentstar_state => {
                    c = self.nextchar();
                    blockcommentbuf.as_mut().unwrap().push(c);
                    match c {
                        0x2f => {
                            let buf = doctextbuf.get_or_insert_with(|| Self::get_doc_text_buffer(&doctagname));
                            if !doctagname.is_empty() {
                                push_str(buf, "]]></");
                                buf.extend_from_slice(&doctagname);
                                push_str(buf, ">");
                            }
                            let doctext = String::from_utf16_lossy(buf);
                            self.state = start_state;
                            return self.make_comment_token(DOCCOMMENT_TOKEN, Some(doctext));
                        }

                        0x2a => continue,

                        0 => {
                            self.error(kError_BlockCommentNotTerminated);
                            self.state = start_state;
                            continue;
                        }

                        _ => {
                            self.state = doccomment_state;
                            continue;
                        }
                    }
                }

                doccommenttag_state => {
                    c = self.nextchar();
                    match c {
                        0x2a => {
                            self.state = doccommentstar_state;
                            continue;
                        }

                        0x20 | 0x09 | 0x0d | 0x0a => {
                            doctextbuf.get_or_insert_with(|| Self::get_doc_text_buffer(&doctagname));

                            loop {
                                c = self.nextchar();
                                if !(c == ch(b' ') || c == ch(b'\t')) {
                                    break;
                                }
                            }
                            self.retract();

                            if !doctagname.is_empty() {
                                let buf = doctextbuf.as_mut().unwrap();
                                push_str(buf, "\n<");
                                buf.extend_from_slice(&doctagname);
                                push_str(buf, "><![CDATA[");
                            }
                            self.state = doccomment_state;
                            continue;
                        }

                        0 => {
                            self.error(kError_BlockCommentNotTerminated);
                            self.state = start_state;
                            continue;
                        }

                        _ => {
                            doctagname.push(c);
                            continue;
                        }
                    }
                }

                doccommentvalue_state => match self.nextchar() {
                    0x2a => {
                        self.state = doccommentstar_state;
                        continue;
                    }

                    0x40 => {
                        self.state = doccommenttag_state;
                        continue;
                    }

                    0 => {
                        self.error(kError_BlockCommentNotTerminated);
                        self.state = start_state;
                        continue;
                    }

                    _ => {
                        self.state = doccomment_state;
                        continue;
                    }
                },

                blockcomment_state => {
                    c = self.nextchar();
                    blockcommentbuf.as_mut().unwrap().push(c);
                    match c {
                        0x2a => {
                            c = self.nextchar();
                            if c == ch(b'/') {
                                self.state = start_state;
                                let buf = blockcommentbuf.as_mut().unwrap();
                                buf.push(c);
                                let blocktext = String::from_utf16_lossy(buf);
                                return self.make_comment_token(BLOCKCOMMENT_TOKEN, Some(blocktext));
                            }
                            self.retract();
                        }

                        0x0d | 0x0a => {
                            self.is_first_token_on_line = true;
                        }

                        0 => {
                            self.error(kError_BlockCommentNotTerminated);
                            self.state = start_state;
                        }
                        _ => {}
                    }
                    continue;
                }

                error_state => {
                    self.error(kError_Lexical_General);
                    self.skiperror();
                    self.state = start_state;
                    continue;
                }

                _ => {
                    self.error_msg("invalid scanner state");
                    self.state = start_state;
                    return self.make_token(EOS_TOKEN);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asc::util::context::{CompilerHandler, ContextStatics};

    struct Collect(Rc<RefCell<Vec<String>>>);

    impl CompilerHandler for Collect {
        fn error(&mut self, _filename: &str, ln: i32, col: i32, msg: &str, _source: &str, code: i32) {
            self.0.borrow_mut().push(format!("{}:{}:{}:{}", code, ln, col, msg));
        }
    }

    fn scan_with(src: &str, save_comments: bool) -> (Vec<(i32, Option<String>)>, Vec<String>) {
        let statics = Rc::new(RefCell::new(ContextStatics::new()));
        let ctx = Rc::new(RefCell::new(Context::new(statics)));
        let errors = Rc::new(RefCell::new(Vec::new()));
        ctx.borrow_mut().set_handler(Rc::new(RefCell::new(Collect(errors.clone()))));
        let mut scanner = Scanner::new_save(ctx, src, Some("test.as".to_string()), save_comments);
        let mut out = Vec::new();
        loop {
            let t = scanner.nexttoken(true);
            if t == EOS_TOKEN {
                break;
            }
            out.push((t, scanner.current_token.text.clone()));
        }
        let errors = errors.borrow().clone();
        (out, errors)
    }

    fn scan(src: &str) -> Vec<(i32, Option<String>)> {
        let (tokens, errors) = scan_with(src, true);
        assert!(errors.is_empty(), "{:?}", errors);
        tokens
    }

    fn t(id: i32) -> (i32, Option<String>) {
        (id, None)
    }

    fn tt(id: i32, text: &str) -> (i32, Option<String>) {
        (id, Some(text.to_string()))
    }

    #[test]
    fn token_tables_line_up() {
        assert_eq!(TOKEN_CLASS_NAMES[(-IDENTIFIER_TOKEN) as usize], "identifier");
        assert_eq!(TOKEN_CLASS_NAMES[(-ERROR_TOKEN) as usize], "<error>");
        assert_eq!(TOKEN_TO_STRING[(-DOTLESSTHAN_TOKEN) as usize], ".<");
    }

    #[test]
    fn numbers() {
        assert_eq!(
            scan("0 12 0x1F 0XaB 1.5 .5 1e10 2E+3 4e-2 0.5e1 007"),
            vec![
                tt(NUMBERLITERAL_TOKEN, "0"),
                tt(NUMBERLITERAL_TOKEN, "12"),
                tt(NUMBERLITERAL_TOKEN, "0x1F"),
                tt(NUMBERLITERAL_TOKEN, "0XaB"),
                tt(NUMBERLITERAL_TOKEN, "1.5"),
                tt(NUMBERLITERAL_TOKEN, ".5"),
                tt(NUMBERLITERAL_TOKEN, "1e10"),
                tt(NUMBERLITERAL_TOKEN, "2E+3"),
                tt(NUMBERLITERAL_TOKEN, "4e-2"),
                tt(NUMBERLITERAL_TOKEN, "0.5e1"),
                tt(NUMBERLITERAL_TOKEN, "007"),
            ]
        );
        // Without es4 numerics the suffix is retracted and scanned as an identifier.
        assert_eq!(scan("1d"), vec![tt(NUMBERLITERAL_TOKEN, "1"), tt(IDENTIFIER_TOKEN, "d")]);
        assert_eq!(scan("a..b"), vec![tt(IDENTIFIER_TOKEN, "a"), t(DOUBLEDOT_TOKEN), tt(IDENTIFIER_TOKEN, "b")]);
    }

    #[test]
    fn bad_exponent_reports_and_drops_number() {
        let (tokens, errors) = scan_with("1e;", true);
        assert_eq!(tokens, vec![]);
        assert_eq!(errors, vec!["1093:1:1:[Compiler] Error #1093: Syntax error.".to_string()]);
    }

    #[test]
    fn strings() {
        assert_eq!(
            scan(r#"'a' "b\n\t" 'it\'s' "q\"" "\x41B\\" 'x\
y'"#),
            vec![
                tt(STRINGLITERAL_TOKEN, "'a'"),
                tt(STRINGLITERAL_TOKEN, "\"b\n\t\""),
                tt(STRINGLITERAL_TOKEN, "'it's'"),
                tt(STRINGLITERAL_TOKEN, "\"q\"\""),
                tt(STRINGLITERAL_TOKEN, "\"AB\\\""),
                tt(STRINGLITERAL_TOKEN, "'xy'"),
            ]
        );
        let (tokens, errors) = scan_with("'abc\nd'", true);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("1094:1:1:"), "{:?}", errors);
        assert_eq!(tokens, vec![tt(STRINGLITERAL_TOKEN, "'abc\nd'")]);
    }

    #[test]
    fn string_token_text_strips_quotes() {
        let statics = Rc::new(RefCell::new(ContextStatics::new()));
        let ctx = Rc::new(RefCell::new(Context::new(statics)));
        let mut scanner = Scanner::new(ctx, "'h\u{e9}llo'", None);
        assert_eq!(scanner.nexttoken(true), STRINGLITERAL_TOKEN);
        let mut single = false;
        assert_eq!(scanner.get_current_string_token_text(&mut single), "h\u{e9}llo");
        assert!(single);
    }

    #[test]
    fn regexp_versus_divide() {
        assert_eq!(
            scan("a / b /= c; x = /ab+\\/c/gi; (1) / 2; [] /x/"),
            vec![
                tt(IDENTIFIER_TOKEN, "a"),
                t(DIV_TOKEN),
                tt(IDENTIFIER_TOKEN, "b"),
                t(DIVASSIGN_TOKEN),
                tt(IDENTIFIER_TOKEN, "c"),
                t(SEMICOLON_TOKEN),
                tt(IDENTIFIER_TOKEN, "x"),
                t(ASSIGN_TOKEN),
                tt(REGEXPLITERAL_TOKEN, "/ab+\\/c/gi"),
                t(SEMICOLON_TOKEN),
                t(LEFTPAREN_TOKEN),
                tt(NUMBERLITERAL_TOKEN, "1"),
                t(RIGHTPAREN_TOKEN),
                t(DIV_TOKEN),
                tt(NUMBERLITERAL_TOKEN, "2"),
                t(SEMICOLON_TOKEN),
                t(LEFTBRACKET_TOKEN),
                t(RIGHTBRACKET_TOKEN),
                t(DIV_TOKEN),
                tt(IDENTIFIER_TOKEN, "x"),
                t(DIV_TOKEN),
            ]
        );
    }

    #[test]
    fn xml_literal_start() {
        assert_eq!(
            scan("x = <a b='1'>t</a>;"),
            vec![
                tt(IDENTIFIER_TOKEN, "x"),
                t(ASSIGN_TOKEN),
                t(LESSTHAN_TOKEN),
                tt(IDENTIFIER_TOKEN, "a"),
                tt(IDENTIFIER_TOKEN, "b"),
                t(ASSIGN_TOKEN),
                tt(STRINGLITERAL_TOKEN, "'1'"),
                t(GREATERTHAN_TOKEN),
                tt(IDENTIFIER_TOKEN, "t"),
                t(XMLTAGSTARTEND_TOKEN),
                tt(IDENTIFIER_TOKEN, "a"),
                t(GREATERTHAN_TOKEN),
                t(SEMICOLON_TOKEN),
            ]
        );
        assert_eq!(scan("<!-- c -->"), vec![tt(XMLMARKUP_TOKEN, "<!-- c -->")]);
        assert_eq!(scan("<![CDATA[x]]>"), vec![tt(XMLMARKUP_TOKEN, "<![CDATA[x]]>")]);
        assert_eq!(scan("<?pi x?>"), vec![tt(XMLMARKUP_TOKEN, "<?pi x?>")]);
    }

    #[test]
    fn xml_literal_state_driven_by_parser() {
        let statics = Rc::new(RefCell::new(ContextStatics::new()));
        let ctx = Rc::new(RefCell::new(Context::new(statics)));
        let mut scanner = Scanner::new(ctx, "<a><b/>{x}</a>;", None);
        scanner.push_state();
        scanner.state = xmlliteral_state;
        assert_eq!(scanner.nexttoken(true), XMLPART_TOKEN);
        assert_eq!(scanner.current_token.text.as_deref(), Some("<a><b/>"));
        scanner.state = start_state;
        assert_eq!(scanner.nexttoken(true), IDENTIFIER_TOKEN);
        assert_eq!(scanner.nexttoken(true), RIGHTBRACE_TOKEN);
        scanner.state = xmlliteral_state;
        assert_eq!(scanner.nexttoken(true), XMLLITERAL_TOKEN);
        assert_eq!(scanner.current_token.text.as_deref(), Some("</a>;"));
        scanner.pop_state();
        assert_eq!(scanner.state, start_state);
    }

    #[test]
    fn comments() {
        assert_eq!(
            scan("a // line\n/* block */ /** doc @param x the x */ /**/ b"),
            vec![
                tt(IDENTIFIER_TOKEN, "a"),
                tt(SLASHSLASHCOMMENT_TOKEN, "// line"),
                tt(BLOCKCOMMENT_TOKEN, "/* block */"),
                tt(
                    DOCCOMMENT_TOKEN,
                    "<description><![CDATA[ doc ]]></description>\n<param><![CDATA[x the x ]]></param>"
                ),
                tt(BLOCKCOMMENT_TOKEN, ""),
                tt(IDENTIFIER_TOKEN, "b"),
            ]
        );
        let (tokens, errors) = scan_with("a // line\n/* block */ b", false);
        assert!(errors.is_empty());
        assert_eq!(tokens, vec![tt(IDENTIFIER_TOKEN, "a"), tt(IDENTIFIER_TOKEN, "b")]);
        let (_, errors) = scan_with("/* open", true);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("1204:"), "{:?}", errors);
    }

    #[test]
    fn comment_does_not_change_divide_lookback() {
        assert_eq!(
            scan("a /* c */ / b"),
            vec![tt(IDENTIFIER_TOKEN, "a"), tt(BLOCKCOMMENT_TOKEN, "/* c */"), t(DIV_TOKEN), tt(IDENTIFIER_TOKEN, "b")]
        );
    }

    #[test]
    fn keywords_and_identifiers() {
        assert_eq!(
            scan("var Var break3 function _x $y \u{e9}t\u{e9} \\u0069f if get include"),
            vec![
                t(VAR_TOKEN),
                tt(IDENTIFIER_TOKEN, "Var"),
                tt(IDENTIFIER_TOKEN, "break3"),
                t(FUNCTION_TOKEN),
                tt(IDENTIFIER_TOKEN, "_x"),
                tt(IDENTIFIER_TOKEN, "$y"),
                tt(IDENTIFIER_TOKEN, "\u{e9}t\u{e9}"),
                tt(IDENTIFIER_TOKEN, "if"),
                t(IF_TOKEN),
                t(GET_TOKEN),
                t(INCLUDE_TOKEN),
            ]
        );
        // Not reserved words in the ASC scanner: they reach the parser as identifiers.
        assert_eq!(
            scan("override dynamic final native static each"),
            vec![
                tt(IDENTIFIER_TOKEN, "override"),
                tt(IDENTIFIER_TOKEN, "dynamic"),
                tt(IDENTIFIER_TOKEN, "final"),
                tt(IDENTIFIER_TOKEN, "native"),
                tt(IDENTIFIER_TOKEN, "static"),
                tt(IDENTIFIER_TOKEN, "each"),
            ]
        );
    }

    #[test]
    fn punctuators_and_line_terminators() {
        let statics = Rc::new(RefCell::new(ContextStatics::new()));
        let ctx = Rc::new(RefCell::new(Context::new(statics)));
        let mut scanner = Scanner::new(ctx, ">>>= !== .< :: ... &&= ||= @\nx", None);
        let mut ids = Vec::new();
        loop {
            let t = scanner.nexttoken(true);
            if t == IDENTIFIER_TOKEN {
                assert!(scanner.follows_line_terminator());
            }
            if t == EOS_TOKEN {
                break;
            }
            ids.push(t);
        }
        assert_eq!(
            ids,
            vec![
                UNSIGNEDRIGHTSHIFTASSIGN_TOKEN,
                STRICTNOTEQUALS_TOKEN,
                DOTLESSTHAN_TOKEN,
                DOUBLECOLON_TOKEN,
                TRIPLEDOT_TOKEN,
                LOGICALANDASSIGN_TOKEN,
                LOGICALORASSIGN_TOKEN,
                ATSIGN_TOKEN,
                IDENTIFIER_TOKEN,
            ]
        );
    }

    #[test]
    fn error_position_is_line_and_column() {
        let (_, errors) = scan_with("a\n  #", true);
        assert_eq!(errors, vec!["1093:2:3:[Compiler] Error #1093: Syntax error.".to_string()]);
    }
}
