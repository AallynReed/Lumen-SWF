//! Port of `macromedia/asc/parser/Parser.java`.

#![allow(non_upper_case_globals)]

use std::cell::{Ref, RefCell, RefMut};
use std::collections::HashSet;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, MAIN_SEPARATOR, MAIN_SEPARATOR_STR};
use std::rc::{Rc, Weak};
use std::sync::atomic::Ordering;

use super::factory::NodeFactory;
use super::input::InputBuffer;
use super::node::*;
use super::scanner::states::*;
use super::scanner::Scanner;
use super::tokens::*;
use crate::asc::semantics::reference_value::ReferenceValue;
use crate::asc::util::context::{
    java_length, java_substring, Context, ContextRef, FileInclude, USE_SANITY_STYLE_ERRORS,
};
use crate::asc::util::errors::*;

// `embedding.avmplus.Features` with mxmlc's default dialect (AS3, which turns on ES4 and E4X).
const HAS_NONIDENTFIELDNAMES: bool = true;
const HAS_QUALIFIEDIDENTIFIERS: bool = true;
const HAS_DESCENDOPERATORS: bool = true;
const HAS_ATTRIBUTEIDENTIFIERS: bool = true;
const HAS_WILDCARDSELECTOR: bool = true;
const HAS_XMLLITERALS: bool = true;
const HAS_EXPRESSIONQUALIFIEDIDS: bool = true;
const HAS_REGULAREXPRESSIONS: bool = true;
const HAS_LOGICALASSIGNMENT: bool = true;
const HAS_SQUAREBRACKETATTRS: bool = true;
const HAS_CONSTPARAMETERS: bool = false;
const HAS_COMPOUNDCLASSNAMES: bool = true;
const HAS_INTERFACEDEFINITIONS: bool = true;
// `embedding.avmplus.RuntimeConstants`.
const AVMPLUS: bool = true;

const PUBLIC: &str = "public";
const PRIVATE: &str = "private";
const PROTECTED: &str = "protected";
const ASTERISK: &str = "*";
const DEFAULT: &str = "default";
const AS3: &str = "AS3";
pub const CONFIG: &str = "CONFIG";
const GET: &str = "get";
const NAMESPACE: &str = "namespace";
const SET: &str = "set";
const VEC: &str = "vec";
const VECTOR: &str = "Vector";
const __AS3__: &str = "__AS3__";

const abbrevIfElse_mode: i32 = ELSE_TOKEN;
const abbrevDoWhile_mode: i32 = WHILE_TOKEN;
const abbrevFunction_mode: i32 = FUNCTION_TOKEN;
const abbrev_mode: i32 = LAST_TOKEN - 1;
const full_mode: i32 = abbrev_mode - 1;
const allowIn_mode: i32 = full_mode - 1;
const noIn_mode: i32 = allowIn_mode - 1;

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum ParseError {
    CatchParameter,
    Syntax,
    Expression,
    Eos,
    Xml,
}

/// `-LAST_TOKEN`, spelled out because the token constants chain too deeply for array lengths.
const TOKEN_SET_SIZE: usize = 134;
const _: () = assert!(TOKEN_SET_SIZE == (-LAST_TOKEN) as usize);

const fn token_set(ids: &[i32]) -> [bool; TOKEN_SET_SIZE] {
    let mut set = [false; TOKEN_SET_SIZE];
    let mut i = 0;
    while i < ids.len() {
        set[(-ids[i]) as usize] = true;
        i += 1;
    }
    set
}

static StatementTokenSet: [bool; TOKEN_SET_SIZE] = token_set(&[
    SUPER_TOKEN, LEFTBRACE_TOKEN, IF_TOKEN, SWITCH_TOKEN, DO_TOKEN, WHILE_TOKEN, FOR_TOKEN, WITH_TOKEN, CONTINUE_TOKEN,
    BREAK_TOKEN, RETURN_TOKEN, THROW_TOKEN, TRY_TOKEN, LEFTBRACKET_TOKEN, DOCCOMMENT_TOKEN,
]);

static XMLTokenSet: [bool; TOKEN_SET_SIZE] = token_set(&[
    IDENTIFIER_TOKEN, ABSTRACT_TOKEN, AS_TOKEN, BREAK_TOKEN, CASE_TOKEN, CATCH_TOKEN, CLASS_TOKEN, CONST_TOKEN,
    CONTINUE_TOKEN, DEBUGGER_TOKEN, DEFAULT_TOKEN, DELETE_TOKEN, DO_TOKEN, ELSE_TOKEN, ENUM_TOKEN, EXTENDS_TOKEN,
    FALSE_TOKEN, FINAL_TOKEN, FINALLY_TOKEN, FOR_TOKEN, FUNCTION_TOKEN, GET_TOKEN, GOTO_TOKEN, IF_TOKEN,
    IMPLEMENTS_TOKEN, IMPORT_TOKEN, IN_TOKEN, INCLUDE_TOKEN, INSTANCEOF_TOKEN, INTERFACE_TOKEN, IS_TOKEN,
    NAMESPACE_TOKEN, CONFIG_TOKEN, NATIVE_TOKEN, NEW_TOKEN, NULL_TOKEN, PACKAGE_TOKEN, PRIVATE_TOKEN, PROTECTED_TOKEN,
    PUBLIC_TOKEN, RETURN_TOKEN, SET_TOKEN, STATIC_TOKEN, SUPER_TOKEN, SWITCH_TOKEN, SYNCHRONIZED_TOKEN, THIS_TOKEN,
    THROW_TOKEN, THROWS_TOKEN, TRANSIENT_TOKEN, TRUE_TOKEN, TRY_TOKEN, TYPEOF_TOKEN, USE_TOKEN, VAR_TOKEN, VOID_TOKEN,
    VOLATILE_TOKEN, WHILE_TOKEN, WITH_TOKEN,
]);

const fn init_binary_precedence() -> [i32; TOKEN_SET_SIZE] {
    let mut p = [0; TOKEN_SET_SIZE];
    p[(-COMMA_TOKEN) as usize] = 1;
    p[(-ASSIGN_TOKEN) as usize] = 2;
    p[(-BITWISEANDASSIGN_TOKEN) as usize] = 2;
    p[(-BITWISEORASSIGN_TOKEN) as usize] = 2;
    p[(-BITWISEXORASSIGN_TOKEN) as usize] = 2;
    p[(-DIVASSIGN_TOKEN) as usize] = 2;
    p[(-LEFTSHIFTASSIGN_TOKEN) as usize] = 2;
    p[(-LOGICALANDASSIGN_TOKEN) as usize] = 2;
    p[(-LOGICALORASSIGN_TOKEN) as usize] = 2;
    p[(-LOGICALXORASSIGN_TOKEN) as usize] = 2;
    p[(-MINUSASSIGN_TOKEN) as usize] = 2;
    p[(-MULTASSIGN_TOKEN) as usize] = 2;
    p[(-MODULUSASSIGN_TOKEN) as usize] = 2;
    p[(-PLUSASSIGN_TOKEN) as usize] = 2;
    p[(-RIGHTSHIFTASSIGN_TOKEN) as usize] = 2;
    p[(-UNSIGNEDRIGHTSHIFTASSIGN_TOKEN) as usize] = 2;
    p[(-QUESTIONMARK_TOKEN) as usize] = 3;
    p[(-LOGICALOR_TOKEN) as usize] = 4;
    p[(-LOGICALAND_TOKEN) as usize] = 5;
    p[(-BITWISEOR_TOKEN) as usize] = 6;
    p[(-BITWISEXOR_TOKEN) as usize] = 7;
    p[(-BITWISEAND_TOKEN) as usize] = 8;
    p[(-EQUALS_TOKEN) as usize] = 9;
    p[(-NOTEQUALS_TOKEN) as usize] = 9;
    p[(-STRICTEQUALS_TOKEN) as usize] = 9;
    p[(-STRICTNOTEQUALS_TOKEN) as usize] = 9;
    p[(-GREATERTHAN_TOKEN) as usize] = 10;
    p[(-GREATERTHANOREQUALS_TOKEN) as usize] = 10;
    p[(-LESSTHAN_TOKEN) as usize] = 10;
    p[(-LESSTHANOREQUALS_TOKEN) as usize] = 10;
    p[(-INSTANCEOF_TOKEN) as usize] = 10;
    p[(-IS_TOKEN) as usize] = 10;
    p[(-AS_TOKEN) as usize] = 10;
    p[(-IN_TOKEN) as usize] = 10;
    p[(-LEFTSHIFT_TOKEN) as usize] = 11;
    p[(-RIGHTSHIFT_TOKEN) as usize] = 11;
    p[(-UNSIGNEDRIGHTSHIFT_TOKEN) as usize] = 11;
    p[(-MINUS_TOKEN) as usize] = 12;
    p[(-PLUS_TOKEN) as usize] = 12;
    p[(-DIV_TOKEN) as usize] = 13;
    p[(-MODULUS_TOKEN) as usize] = 13;
    p[(-MULT_TOKEN) as usize] = 13;
    p
}

static binary_precedence: [i32; TOKEN_SET_SIZE] = init_binary_precedence();

fn rb(n: &Option<NodeRef>) -> Ref<'_, Node> {
    n.as_ref().expect("NullPointerException").borrow()
}

fn rbm(n: &Option<NodeRef>) -> RefMut<'_, Node> {
    n.as_ref().expect("NullPointerException").borrow_mut()
}

fn is_kind(n: &Option<NodeRef>, f: impl Fn(&Node) -> bool) -> bool {
    n.as_ref().map_or(false, |n| f(&n.borrow()))
}

/// `java.lang.String.trim()`.
fn java_trim(s: &str) -> String {
    s.trim_matches(|c: char| c <= ' ').to_string()
}

/// `new File(path).getAbsolutePath()`.
fn java_absolute_path(path: &str) -> String {
    std::path::absolute(path).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| path.to_string())
}

/// `new File(path).getCanonicalPath()`, which also normalises paths that do not exist.
fn java_canonical_path(path: &str) -> String {
    let canonical = match std::fs::canonicalize(path) {
        Ok(p) => p.to_string_lossy().into_owned(),
        Err(_) => java_absolute_path(path),
    };
    match canonical.strip_prefix(r"\\?\UNC\") {
        Some(rest) => format!(r"\\{}", rest),
        None => canonical.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(canonical),
    }
}

/// `Parser.error(kind, errCode, arg1, arg2, pos)`, shared with [`ParserRef`].
fn report_error(
    ctx: &ContextRef,
    input: &Rc<RefCell<InputBuffer>>,
    kind: ParseError,
    err_code: i32,
    arg1: &str,
    arg2: &str,
    mut pos: i32,
) {
    let origin = input.borrow().origin.clone();
    let mut out = String::new();

    if USE_SANITY_STYLE_ERRORS.load(Ordering::Relaxed) {
        out.push_str(&format!("code={}; arg1={}; arg2={}", err_code, arg1, arg2));
    } else {
        let msg = ctx.borrow().shell_error_string(err_code);
        let mut next_loc = Context::replace_string_arg(&mut out, &msg, 0, arg1);
        next_loc = Context::replace_string_arg(&mut out, &msg, next_loc, arg2);
        if next_loc != -1 {
            out.push_str(&java_substring(&msg, next_loc, java_length(&msg)));
        }
    }

    if pos < 0 {
        pos = input.borrow().position_of_mark();
    }

    let lineno = input.borrow_mut().get_ln_num(pos);
    let column = input.borrow_mut().get_col_pos(pos);
    let origin = origin.unwrap_or_else(|| "null".to_string());

    if kind == ParseError::Syntax || kind == ParseError::Eos {
        let line_text = input.borrow_mut().get_line_text(pos);
        ctx.borrow().localized_error_ln_col_code(&origin, lineno, column, &out, &line_text, err_code);
    } else {
        ctx.borrow().localized_error_ln_col_code(&origin, lineno, column, &out, "", err_code);
    }
}

/// What `cx.parser` refers to: the parts of a `Parser` that its `error(int)` reads.
pub struct ParserRef {
    ctx: Weak<RefCell<Context>>,
    input: RefCell<Option<Rc<RefCell<InputBuffer>>>>,
}

impl ParserRef {
    pub fn error(&self, err_code: i32) -> Option<NodeRef> {
        let ctx = self.ctx.upgrade().expect("NullPointerException");
        let input = self.input.borrow().clone().expect("NullPointerException");
        report_error(&ctx, &input, ParseError::Syntax, err_code, "", "", -1);
        None
    }
}

pub struct Parser {
    next_token: i32,
    ctx: ContextRef,
    node_factory: Rc<RefCell<NodeFactory>>,
    create_doc_info: bool,
    save_comment_nodes: bool,
    pub scanner: Option<Scanner>,

    encoding: Option<String>,
    pub comments: Option<Vec<NodeRef>>,
    pub block_kind_stack: Vec<i32>,
    pub current_class_name: Option<String>,
    within_package: bool,
    parsing_include: bool,
    pub config_namespaces: Rc<RefCell<Vec<HashSet<String>>>>,
    is_xmllist: bool,
    this_ref: Rc<ParserRef>,
}

impl Parser {
    fn in_xml_token_set(id: i32) -> bool {
        XMLTokenSet.get((-id) as usize).copied().unwrap_or(false)
    }

    fn in_statement_token_set(id: i32) -> bool {
        StatementTokenSet.get((-id) as usize).copied().unwrap_or(false)
    }

    fn nf(&self) -> RefMut<'_, NodeFactory> {
        self.node_factory.borrow_mut()
    }

    fn sc(&mut self) -> &mut Scanner {
        self.scanner.as_mut().expect("NullPointerException")
    }

    fn position_of_mark(&self) -> i32 {
        self.scanner.as_ref().expect("NullPointerException").input().position_of_mark()
    }

    fn set_scanner(&mut self, scanner: Option<Scanner>) {
        *self.this_ref.input.borrow_mut() = scanner.as_ref().and_then(|s| s.input.clone());
        self.scanner = scanner;
    }

    fn clear_unused_buffers(&mut self) {
        self.comments.as_mut().expect("NullPointerException").clear();
        self.sc().clear_unused_buffers();
        self.set_scanner(None);
        self.comments = None;
    }

    pub fn error(&mut self, err_code: i32) -> Option<NodeRef> {
        self.error_pos(ParseError::Syntax, err_code, "", "", -1)
    }

    fn error1(&mut self, kind: ParseError, err_code: i32, arg1: &str) -> Option<NodeRef> {
        self.error_pos(kind, err_code, arg1, "", -1)
    }

    fn error2(&mut self, kind: ParseError, err_code: i32, arg1: &str, arg2: &str) -> Option<NodeRef> {
        self.error_pos(kind, err_code, arg1, arg2, -1)
    }

    fn error_pos(&mut self, kind: ParseError, err_code: i32, arg1: &str, arg2: &str, pos: i32) -> Option<NodeRef> {
        let input = self.sc().input.clone().expect("NullPointerException");
        report_error(&self.ctx, &input, kind, err_code, arg1, arg2, pos);
        None
    }

    fn skiperror_kind(&mut self, e: ParseError) {
        match e {
            ParseError::CatchParameter => loop {
                self.get_next_token();
                if self.next_token == RIGHTPAREN_TOKEN || self.next_token == EOS_TOKEN {
                    break;
                }
            },

            ParseError::Xml | ParseError::Eos => loop {
                self.get_next_token();
                if self.next_token == EOS_TOKEN {
                    break;
                }
            },

            ParseError::Syntax | ParseError::Expression => loop {
                let t = self.next_token;
                if t == LEFTPAREN_TOKEN
                    || t == RIGHTPAREN_TOKEN
                    || t == LEFTBRACE_TOKEN
                    || t == RIGHTBRACE_TOKEN
                    || t == LEFTBRACKET_TOKEN
                    || t == RIGHTBRACKET_TOKEN
                    || t == COMMA_TOKEN
                    || t == SEMICOLON_TOKEN
                    || t == EOS_TOKEN
                {
                    break;
                }
                self.get_next_token();
            },
        }
    }

    fn skiperror(&mut self, token_id: i32) {
        loop {
            if self.next_token == token_id {
                if token_id == RIGHTBRACE_TOKEN {
                    let mut size = self.block_kind_stack.len();
                    self.block_kind_stack.clear();
                    while size > 0 {
                        self.block_kind_stack.push(ERROR_TOKEN);
                        size -= 1;
                    }
                }
                break;
            } else if self.next_token == EOS_TOKEN {
                break;
            } else if self.next_token == SEMICOLON_TOKEN && token_id != RIGHTBRACE_TOKEN {
                break;
            } else if self.next_token == LEFTBRACE_TOKEN && token_id != RIGHTBRACE_TOKEN {
                self.sc().retract();
                break;
            } else if self.next_token == RIGHTBRACE_TOKEN {
                self.sc().retract();
                break;
            }
            self.get_next_token();
        }
    }

    fn init(
        cx: &ContextRef,
        origin: &str,
        emit_doc_info: bool,
        save_comment_nodes: bool,
        block_kind_stack: Option<&[i32]>,
    ) -> Parser {
        let node_factory = Context::get_node_factory(cx);
        node_factory.borrow_mut().create_default_doc_comments(emit_doc_info);
        let mut stack = Vec::new();
        match block_kind_stack {
            Some(b) => stack.extend_from_slice(b),
            None => stack.push(EMPTY_TOKEN),
        }
        let this_ref = Rc::new(ParserRef { ctx: Rc::downgrade(cx), input: RefCell::new(None) });
        cx.borrow_mut().parser = Some(this_ref.clone());
        cx.borrow_mut().set_origin(origin);
        Parser {
            next_token: EMPTY_TOKEN,
            ctx: cx.clone(),
            node_factory,
            create_doc_info: emit_doc_info,
            save_comment_nodes,
            scanner: None,
            encoding: None,
            comments: Some(Vec::new()),
            block_kind_stack: stack,
            current_class_name: None,
            within_package: false,
            parsing_include: false,
            config_namespaces: Rc::new(RefCell::new(Vec::new())),
            is_xmllist: false,
            this_ref,
        }
    }

    /// `Parser(Context, InputStream, String)`.
    pub fn new_stream(cx: &ContextRef, in_: &[u8], origin: &str) -> Parser {
        Self::new_stream_encoding(cx, in_, origin, None)
    }

    /// `Parser(Context, InputStream, String, String)`.
    pub fn new_stream_encoding(cx: &ContextRef, in_: &[u8], origin: &str, encoding: Option<&str>) -> Parser {
        let mut p = Self::init(cx, origin, false, false, None);
        p.set_scanner(Some(Scanner::new_stream(cx.clone(), in_, encoding, Some(origin.to_string()))));
        p.encoding = encoding.map(str::to_string);
        p
    }

    /// `Parser(Context, InputStream, String, boolean, boolean)`.
    pub fn new_stream_doc(cx: &ContextRef, in_: &[u8], origin: &str, emit_doc_info: bool, save_comment_nodes: bool) -> Parser {
        Self::new_stream_blocks(cx, in_, origin, None, emit_doc_info, save_comment_nodes, None)
    }

    /// `Parser(Context, InputStream, String, String, boolean, boolean)`.
    pub fn new_stream_encoding_doc(
        cx: &ContextRef,
        in_: &[u8],
        origin: &str,
        encoding: Option<&str>,
        emit_doc_info: bool,
        save_comment_nodes: bool,
    ) -> Parser {
        Self::new_stream_blocks(cx, in_, origin, encoding, emit_doc_info, save_comment_nodes, None)
    }

    /// `Parser(Context, InputStream, String, String, boolean, boolean, IntList)`.
    pub fn new_stream_blocks(
        cx: &ContextRef,
        in_: &[u8],
        origin: &str,
        encoding: Option<&str>,
        emit_doc_info: bool,
        save_comment_nodes: bool,
        block_kind_stack: Option<&[i32]>,
    ) -> Parser {
        let mut p = Self::init(cx, origin, emit_doc_info, save_comment_nodes, block_kind_stack);
        p.set_scanner(Some(Scanner::new_stream_save(
            cx.clone(),
            in_,
            encoding,
            Some(origin.to_string()),
            save_comment_nodes | emit_doc_info,
        )));
        p.encoding = encoding.map(str::to_string);
        p
    }

    /// `Parser(Context, InputStream, String, String, boolean, boolean, IntList, boolean)`.
    #[allow(clippy::too_many_arguments)]
    pub fn new_stream_include(
        cx: &ContextRef,
        in_: &[u8],
        origin: &str,
        encoding: Option<&str>,
        emit_doc_info: bool,
        save_comment_nodes: bool,
        block_kind_stack: Option<&[i32]>,
        is_include: bool,
    ) -> Parser {
        let mut p = Self::new_stream_blocks(cx, in_, origin, encoding, emit_doc_info, save_comment_nodes, block_kind_stack);
        p.parsing_include = is_include;
        p
    }

    /// `Parser(Context, String, String)`.
    pub fn new(cx: &ContextRef, in_: &str, origin: &str) -> Parser {
        let mut p = Self::init(cx, origin, false, false, None);
        p.set_scanner(Some(Scanner::new_save(cx.clone(), in_, Some(origin.to_string()), false)));
        p
    }

    /// `Parser(Context, String, String, boolean, boolean)`.
    pub fn new_doc(cx: &ContextRef, in_: &str, origin: &str, emit_doc_info: bool, save_comment_nodes: bool) -> Parser {
        Self::new_blocks(cx, in_, origin, emit_doc_info, save_comment_nodes, None)
    }

    /// `Parser(Context, String, String, boolean, boolean, IntList)`.
    pub fn new_blocks(
        cx: &ContextRef,
        in_: &str,
        origin: &str,
        emit_doc_info: bool,
        save_comment_nodes: bool,
        block_kind_stack: Option<&[i32]>,
    ) -> Parser {
        let mut p = Self::init(cx, origin, emit_doc_info, save_comment_nodes, block_kind_stack);
        p.set_scanner(Some(Scanner::new_save(cx.clone(), in_, Some(origin.to_string()), save_comment_nodes | emit_doc_info)));
        p
    }

    /// `Parser(Context, String, String, boolean, boolean, IntList, boolean)`.
    pub fn new_include(
        cx: &ContextRef,
        in_: &str,
        origin: &str,
        emit_doc_info: bool,
        save_comment_nodes: bool,
        block_kind_stack: Option<&[i32]>,
        is_include: bool,
    ) -> Parser {
        let mut p = Self::new_blocks(cx, in_, origin, emit_doc_info, save_comment_nodes, block_kind_stack);
        p.parsing_include = is_include;
        p
    }

    /// `Parser(Context, InputBuffer, String, boolean)`.
    pub fn new_input(cx: &ContextRef, input_buffer: Rc<RefCell<InputBuffer>>, origin: &str, emit_doc_info: bool) -> Parser {
        let mut p = Self::init(cx, origin, emit_doc_info, false, None);
        p.set_scanner(Some(Scanner::new_input(cx.clone(), input_buffer)));
        p
    }

    fn shift(&mut self) {
        self.next_token = EMPTY_TOKEN;
    }

    pub fn r#match(&mut self, expected_token_class: i32) -> i32 {
        let result;
        let lt = self.lookahead();

        if lt == expected_token_class {
            self.shift();
            return lt;
        }

        if lt == ERROR_TOKEN {
            let result = self.next_token;
            self.shift();
            return result;
        }

        if expected_token_class == EOS_TOKEN {
            if self.ctx.borrow().error_count() == 0 {
                self.error(kError_Parser_ExtraCharsAfterEndOfProgram);
            }
            self.skiperror(EOS_TOKEN);
            result = self.next_token;
            self.shift();
        } else {
            let next_token = self.next_token;
            let text = self.sc().get_current_token_text_or_type_text(next_token);
            self.error2(ParseError::Eos, kError_Parser_ExpectedToken, get_token_class_name(expected_token_class), &text);

            self.skiperror(expected_token_class);

            result = self.next_token;
            if self.next_token != EOS_TOKEN {
                self.shift();
            }
        }
        result
    }

    fn lookahead_semicolon(&mut self, mode: i32) -> bool {
        let lt = self.lookahead();

        lt == SEMICOLON_TOKEN
            || lt == EOS_TOKEN
            || lt == RIGHTBRACE_TOKEN
            || lt == mode
            || self.sc().follows_line_terminator()
    }

    fn match_semicolon(&mut self, mode: i32) -> i32 {
        let mut result = ERROR_TOKEN;
        let lt = self.lookahead();

        if lt == SEMICOLON_TOKEN {
            self.shift();
            result = SEMICOLON_TOKEN;
        } else if lt == EOS_TOKEN || lt == RIGHTBRACE_TOKEN || self.sc().follows_line_terminator() {
            result = SEMICOLON_TOKEN;
        } else if (mode == abbrevIfElse_mode || mode == abbrevDoWhile_mode) && lt == mode {
            result = SEMICOLON_TOKEN;
        } else if lt != ERROR_TOKEN {
            if mode == abbrevFunction_mode {
                self.error(kError_Parser_ExpectedLeftBrace);
                self.skiperror(LEFTBRACE_TOKEN);
            } else {
                let next_token = self.next_token;
                let text = self.sc().get_current_token_text_or_type_text(next_token);
                self.error1(ParseError::Syntax, kError_Parser_ExpectedSemicolon, &text);
                self.skiperror(SEMICOLON_TOKEN);
            }
        }
        result
    }

    fn match_noninsertable_semicolon(&mut self, mode: i32) -> i32 {
        match self.lookahead() {
            SEMICOLON_TOKEN => {
                self.shift();
                return SEMICOLON_TOKEN;
            }
            EOS_TOKEN | RIGHTBRACE_TOKEN => return SEMICOLON_TOKEN,
            _ => {}
        }

        if (mode == abbrevIfElse_mode || mode == abbrevDoWhile_mode) && self.lookahead() == mode {
            SEMICOLON_TOKEN
        } else {
            let next_token = self.next_token;
            let text = self.sc().get_current_token_text_or_type_text(next_token);
            self.error1(ParseError::Syntax, kError_Parser_ExpectedSemicolon, &text);
            self.skiperror(SEMICOLON_TOKEN);
            ERROR_TOKEN
        }
    }

    fn get_next_token(&mut self) {
        let mut tok = self.sc().nexttoken(true);

        while tok == SLASHSLASHCOMMENT_TOKEN || tok == BLOCKCOMMENT_TOKEN || tok == DOCCOMMENT_TOKEN {
            if self.save_comment_nodes && (!self.ctx.borrow().script_assist_parsing || tok != DOCCOMMENT_TOKEN) {
                let text = self.sc().get_current_token_text().expect("NullPointerException");
                let pos = self.position_of_mark();
                let new_comment = self.nf().comment(&text, tok, pos);
                self.comments.as_mut().expect("NullPointerException").push(new_comment);
            }

            if tok == DOCCOMMENT_TOKEN && self.create_doc_info {
                break;
            }

            tok = self.sc().nexttoken(false);
        }
        self.next_token = tok;
    }

    fn change_lookahead(&mut self, tok: i32) {
        self.next_token = tok;
    }

    fn lookahead(&mut self) -> i32 {
        if self.next_token == EMPTY_TOKEN {
            self.get_next_token();
        }
        self.next_token
    }

    fn current_token_text(&mut self) -> String {
        self.sc().get_current_token_text().expect("NullPointerException")
    }

    fn current_string_token_text(&mut self, is_single_quoted: &mut bool) -> String {
        self.sc().get_current_string_token_text(is_single_quoted)
    }

    fn parse_identifier(&mut self) -> NodeRef {
        let name = self.parse_identifier_string();
        let pos = self.position_of_mark();
        self.nf().identifier_intern_pos(&name, false, pos)
    }

    pub fn parse_identifier_string(&mut self) -> String {
        let lt = self.lookahead();

        match lt {
            GET_TOKEN => {
                self.shift();
                GET.to_string()
            }
            SET_TOKEN => {
                self.shift();
                SET.to_string()
            }
            NAMESPACE_TOKEN => {
                self.shift();
                NAMESPACE.to_string()
            }
            IDENTIFIER_TOKEN => {
                self.shift();
                self.current_token_text()
            }
            _ => {
                if IDENTIFIER_TOKEN == self.r#match(IDENTIFIER_TOKEN) {
                    self.current_token_text()
                } else {
                    "expected identifer".to_string()
                }
            }
        }
    }

    fn parse_property_identifier(&mut self) -> NodeRef {
        let pos = self.position_of_mark();
        let name = self.parse_property_identifier_string();
        self.nf().identifier_intern_pos(&name, false, pos)
    }

    fn parse_property_identifier_string(&mut self) -> String {
        let lt = self.lookahead();

        match lt {
            DEFAULT_TOKEN => {
                self.shift();
                DEFAULT.to_string()
            }
            GET_TOKEN => {
                self.shift();
                GET.to_string()
            }
            SET_TOKEN => {
                self.shift();
                SET.to_string()
            }
            NAMESPACE_TOKEN => {
                self.shift();
                NAMESPACE.to_string()
            }
            IDENTIFIER_TOKEN => {
                self.shift();
                self.current_token_text()
            }
            _ => {
                if HAS_WILDCARDSELECTOR && (lt == MULT_TOKEN || lt == MULTASSIGN_TOKEN) {
                    if lt == MULT_TOKEN {
                        self.shift();
                    } else {
                        self.change_lookahead(ASSIGN_TOKEN);
                    }
                    ASTERISK.to_string()
                } else if IDENTIFIER_TOKEN == self.r#match(IDENTIFIER_TOKEN) {
                    self.current_token_text()
                } else {
                    "expected property identifier".to_string()
                }
            }
        }
    }

    fn parse_qualifier(&mut self) -> NodeRef {
        match self.lookahead() {
            PUBLIC_TOKEN => {
                self.shift();
                let pos = self.position_of_mark();
                self.nf().identifier_intern_pos(PUBLIC, false, pos)
            }
            PRIVATE_TOKEN => {
                self.shift();
                let pos = self.position_of_mark();
                self.nf().identifier_intern_pos(PRIVATE, false, pos)
            }
            PROTECTED_TOKEN => {
                self.shift();
                let pos = self.position_of_mark();
                self.nf().identifier_intern_pos(PROTECTED, false, pos)
            }
            _ => self.parse_property_identifier(),
        }
    }

    /// `men.selector`'s expression, read through the Java `(GetExpressionNode)` view.
    fn bracket_get_expr(men: &NodeRef) -> Option<NodeRef> {
        let m = men.borrow();
        let selector = &m.as_member_expression().expect("ClassCastException").selector;
        let s = rb(selector);
        s.as_get_expression().expect("NullPointerException").sel.expr.clone()
    }

    fn parse_simple_qualified_identifier(&mut self) -> NodeRef {
        let result: NodeRef;
        let mut is_attr = false;

        if HAS_ATTRIBUTEIDENTIFIERS && self.lookahead() == ATSIGN_TOKEN {
            self.shift();
            is_attr = true;
        }

        if is_attr && self.lookahead() == LEFTBRACKET_TOKEN {
            let men = self.parse_brackets(None);
            let expr = Self::bracket_get_expr(&men);
            let pos = rb(&expr).pos();
            result = self.nf().qualified_expression(None, expr, pos);
        } else {
            let first = self.parse_qualifier();

            if HAS_QUALIFIEDIDENTIFIERS && self.lookahead() == DOUBLECOLON_TOKEN {
                self.shift();
                let get = self.nf().get_expression(Some(first.clone()));
                let temp = self.nf().member_expression(None, Some(get));

                if self.lookahead() == LEFTBRACKET_TOKEN {
                    let men = self.parse_brackets(None);
                    let expr = Self::bracket_get_expr(&men);
                    let pos = rb(&expr).pos();
                    result = self.nf().qualified_expression(Some(temp), expr, pos);
                } else {
                    let name = self.parse_property_identifier_string();
                    let pos = self.position_of_mark();
                    let qualid = self.nf().qualified_identifier_pos(Some(temp), Some(&name), pos);
                    let first_name = first.borrow().identifier().expect("ClassCastException").name.clone();
                    if self.config_namespaces.borrow().last().expect("IndexOutOfBoundsException").contains(&first_name) {
                        qualid.borrow_mut().qualified_identifier_mut().unwrap().is_config_name = true;
                    }
                    qualid.borrow_mut().identifier_mut().unwrap().set_orig_type_token(DOUBLECOLON_TOKEN);
                    result = qualid;
                }
            } else {
                result = first;
            }
        }

        result.borrow_mut().set_attr(is_attr);
        result
    }

    fn parse_expression_qualified_identifier(&mut self) -> NodeRef {
        let mut is_attr = false;

        if HAS_ATTRIBUTEIDENTIFIERS && self.lookahead() == ATSIGN_TOKEN {
            self.shift();
            is_attr = true;
        }

        let first = self.parse_paren_expression();
        self.r#match(DOUBLECOLON_TOKEN);
        let name = self.parse_property_identifier_string();
        let pos = self.position_of_mark();
        let result = self.nf().qualified_identifier_pos(Some(first), Some(&name), pos);
        result.borrow_mut().set_attr(is_attr);
        result
    }

    fn parse_qualified_identifier(&mut self) -> NodeRef {
        if HAS_EXPRESSIONQUALIFIEDIDS && self.lookahead() == LEFTPAREN_TOKEN {
            self.parse_expression_qualified_identifier()
        } else {
            self.parse_simple_qualified_identifier()
        }
    }

    fn parse_primary_expression(&mut self) -> Option<NodeRef> {
        let result: Option<NodeRef>;
        let lt = self.lookahead();
        let pos = self.position_of_mark();

        match lt {
            NULL_TOKEN => {
                self.shift();
                result = Some(self.nf().literal_null_pos(pos));
            }
            TRUE_TOKEN => {
                self.shift();
                result = Some(self.nf().literal_boolean_pos(true, pos));
            }
            FALSE_TOKEN => {
                self.shift();
                result = Some(self.nf().literal_boolean_pos(false, pos));
            }
            PRIVATE_TOKEN => {
                self.shift();
                result = Some(self.nf().identifier_intern_pos(PRIVATE, false, pos));
            }
            PUBLIC_TOKEN => {
                self.shift();
                result = Some(self.nf().identifier_intern_pos(PUBLIC, false, pos));
            }
            PROTECTED_TOKEN => {
                self.shift();
                result = Some(self.nf().identifier_intern_pos(PROTECTED, false, pos));
            }
            NUMBERLITERAL_TOKEN => {
                self.shift();
                let text = self.current_token_text();
                result = Some(self.nf().literal_number_pos(&text, pos));
            }
            STRINGLITERAL_TOKEN => {
                let mut is_single_quoted = false;
                self.shift();
                let enclosed_text = self.current_string_token_text(&mut is_single_quoted);
                result = Some(self.nf().literal_string_quoted(&enclosed_text, pos, is_single_quoted));
            }
            THIS_TOKEN => {
                self.shift();
                result = Some(self.nf().this_expression(pos));
            }
            LEFTPAREN_TOKEN => {
                result = self.parse_paren_expression_list();
            }
            LEFTBRACKET_TOKEN => {
                result = Some(self.parse_array_literal());
            }
            LEFTBRACE_TOKEN => {
                result = Some(self.parse_object_literal());
            }
            FUNCTION_TOKEN => {
                self.shift();
                let mut first = None;
                if self.lookahead() == IDENTIFIER_TOKEN {
                    first = Some(self.parse_identifier());
                }
                result = Some(self.parse_function_common(first));
            }
            PACKAGE_TOKEN => {
                if self.within_package {
                    self.error(kError_NestedPackage);
                    result = Some(self.nf().error(pos, kError_NestedPackage));
                } else {
                    let next_token = self.next_token;
                    let text = self.sc().get_current_token_text_or_type_text(next_token);
                    self.error1(ParseError::Syntax, kError_Parser_ExpectedPrimaryExprBefore, &text);
                    result = Some(self.nf().error(pos, kError_Parser_ExpectedPrimaryExprBefore));
                }
                self.skiperror(LEFTBRACE_TOKEN);
                self.skiperror(RIGHTBRACE_TOKEN);
            }
            CATCH_TOKEN | FINALLY_TOKEN | ELSE_TOKEN => {
                let next_token = self.next_token;
                let text = self.sc().get_current_token_text_or_type_text(next_token);
                self.error1(ParseError::Syntax, kError_Parser_ExpectedPrimaryExprBefore, &text);
                self.skiperror(LEFTBRACE_TOKEN);
                self.skiperror(RIGHTBRACE_TOKEN);
                let p = self.position_of_mark();
                result = Some(self.nf().error(p, kError_Parser_ExpectedPrimaryExprBefore));
            }
            XMLMARKUP_TOKEN | LESSTHAN_TOKEN => {
                result = if HAS_XMLLITERALS { self.parse_xml_literal() } else { self.parse_typename() };
            }
            REGEXPLITERAL_TOKEN => {
                if HAS_REGULAREXPRESSIONS {
                    self.shift();
                    let text = self.current_token_text();
                    result = Some(self.nf().literal_reg_exp(&text, pos));
                } else {
                    result = self.parse_typename();
                }
            }
            _ => {
                result = self.parse_typename();
            }
        }

        result
    }

    fn parse_xml_literal(&mut self) -> Option<NodeRef> {
        self.is_xmllist = false;
        let mut result = None;
        let first: Option<NodeRef>;
        let pos = self.position_of_mark();

        if self.lookahead() == XMLMARKUP_TOKEN {
            self.shift();
            let text = self.current_token_text();
            let s = self.nf().literal_string_pos(&text, pos);
            first = Some(self.nf().list(None, Some(s)));
        } else {
            self.sc().push_state();
            first = self.parse_xml_element();
            self.sc().pop_state();
        }

        if first.is_some() {
            let list = self.nf().list_pos(None, first, pos);
            let p = self.position_of_mark();
            let is_xmllist = self.is_xmllist;
            result = Some(self.nf().literal_xml(Some(list), is_xmllist, p));
        }
        result
    }

    fn parse_xml_element(&mut self) -> Option<NodeRef> {
        let mut result: Option<NodeRef>;
        let mut is_xmllist = false;

        self.r#match(LESSTHAN_TOKEN);
        if self.lookahead() == GREATERTHAN_TOKEN {
            is_xmllist = true;
            let s = self.nf().literal_string_pos("", 0);
            result = Some(self.nf().list(None, Some(s)));
        } else {
            let s = self.nf().literal_string_pos("<", 0);
            result = Some(self.nf().list(None, Some(s)));
            result = self.parse_xml_name(result);
            result = self.parse_xml_attributes(result);
        }

        if self.lookahead() == GREATERTHAN_TOKEN {
            self.shift();
            if !is_xmllist {
                let s = self.nf().literal_string_pos(">", 0);
                result = self.concat_xml(result, Some(s));
            }
            result = self.parse_xml_element_content(result);
            if self.lookahead() == EOS_TOKEN {
                self.error(kError_Lexical_NoMatchingTag);
                let p = self.position_of_mark();
                return Some(self.nf().error(p, kError_Lexical_NoMatchingTag));
            }
            self.r#match(XMLTAGSTARTEND_TOKEN);
            if self.lookahead() == GREATERTHAN_TOKEN {
                if !is_xmllist {
                    let p = self.position_of_mark();
                    self.ctx.borrow().error(p, kError_MissingXMLTagName);
                }
            } else {
                let s = self.nf().literal_string_pos("</", 0);
                result = self.concat_xml(result, Some(s));
                result = self.parse_xml_name(result);
                let s = self.nf().literal_string_pos(">", 0);
                result = self.concat_xml(result, Some(s));
            }
            self.r#match(GREATERTHAN_TOKEN);
        } else {
            self.r#match(XMLTAGENDEND_TOKEN);
            let s = self.nf().literal_string_pos("/>", 0);
            result = self.concat_xml(result, Some(s));
        }
        self.is_xmllist = is_xmllist;
        result
    }

    fn concat_xml(&mut self, left: Option<NodeRef>, right: Option<NodeRef>) -> Option<NodeRef> {
        let mut tmp_left = left.clone();

        if is_kind(&left, |n| n.list().map_or(false, |l| l.size() == 1)) {
            tmp_left = rb(&left).list().unwrap().items.first().cloned().flatten();
        }

        if is_kind(&tmp_left, |n| n.as_literal_string().is_some()) && is_kind(&right, |n| n.as_literal_string().is_some()) {
            let value = rb(&tmp_left).as_literal_string().unwrap().value.clone() + &rb(&right).as_literal_string().unwrap().value;
            let pos = rb(&left).pos();
            return Some(self.nf().literal_string_pos(&value, pos));
        }

        if is_kind(&left, |n| n.as_binary_expression().is_some()) && is_kind(&right, |n| n.as_literal_string().is_some()) {
            let (op, lhs, rhs, left_pos) = {
                let l = rb(&left);
                let b = l.as_binary_expression().unwrap();
                (b.op, b.lhs.clone(), b.rhs.clone(), l.pos())
            };
            if op == PLUS_TOKEN && is_kind(&rhs, |n| n.as_literal_string().is_some()) {
                let value = rb(&rhs).as_literal_string().unwrap().value.clone() + &rb(&right).as_literal_string().unwrap().value;
                let s = self.nf().literal_string(&value);
                return Some(self.nf().binary_expression_pos(PLUS_TOKEN, lhs, Some(s), left_pos));
            }
        }

        Some(self.nf().binary_expression(PLUS_TOKEN, left, right))
    }

    fn parse_xml_name(&mut self, first: Option<NodeRef>) -> Option<NodeRef> {
        let mut result;
        let mut lt = self.lookahead();

        if lt == LEFTBRACE_TOKEN {
            self.shift();
            self.sc().push_state();
            let expr = self.parse_expression_list(allowIn_mode);
            result = self.concat_xml(first, expr);
            self.r#match(RIGHTBRACE_TOKEN);
            self.sc().pop_state();
        } else {
            if Self::in_xml_token_set(lt) || lt == COLON_TOKEN {
                self.shift();
                let text = self.sc().get_current_token_text_or_type_text(lt);
                let p = self.position_of_mark();
                let s = self.nf().literal_string_pos(&text, p);
                result = self.concat_xml(first, Some(s));
            } else {
                self.error1(ParseError::Syntax, kError_ErrorNodeError, "invalid xml name");
                self.skiperror_kind(ParseError::Xml);
                let p = self.position_of_mark();
                result = Some(self.nf().error(p, kError_MissingXMLTagName));
            }

            lt = self.lookahead();
            loop {
                let separator_text = if lt == DOT_TOKEN {
                    "."
                } else if lt == MINUS_TOKEN {
                    "-"
                } else if lt == COLON_TOKEN {
                    ":"
                } else {
                    break;
                };

                self.shift();
                lt = self.lookahead();

                if Self::in_xml_token_set(lt) {
                    let s = self.nf().literal_string_pos(separator_text, 0);
                    result = self.concat_xml(result, Some(s));
                    let text = self.sc().get_current_token_text_or_type_text(lt);
                    let p = self.position_of_mark();
                    let s = self.nf().literal_string_pos(&text, p);
                    result = self.concat_xml(result, Some(s));
                    self.shift();
                    lt = self.lookahead();
                } else {
                    self.error(kError_MissingXMLTagName);
                    self.skiperror_kind(ParseError::Xml);
                    let p = self.position_of_mark();
                    result = Some(self.nf().error(p, kError_MissingXMLTagName));
                }
            }
        }
        result
    }

    fn parse_xml_attributes(&mut self, first: Option<NodeRef>) -> Option<NodeRef> {
        let mut result = first;
        let mut lt = self.lookahead();

        while !(lt == GREATERTHAN_TOKEN || lt == XMLTAGENDEND_TOKEN || lt == EOS_TOKEN) {
            let s = self.nf().literal_string_pos(" ", 0);
            result = self.concat_xml(result, Some(s));
            result = self.parse_xml_attribute(result);
            lt = self.lookahead();
        }
        result
    }

    fn parse_xml_attribute(&mut self, first: Option<NodeRef>) -> Option<NodeRef> {
        let mut result = self.parse_xml_name(first);

        if self.lookahead() == ASSIGN_TOKEN {
            self.shift();

            let mut value = None;
            let mut single_quote = false;

            if self.lookahead() == STRINGLITERAL_TOKEN {
                let mut is_single_quoted = false;
                self.shift();
                let enclosed_text = self.current_string_token_text(&mut is_single_quoted);
                let p = self.position_of_mark();
                value = Some(self.nf().literal_string_quoted(&enclosed_text, p, is_single_quoted));
                single_quote = is_single_quoted;
            } else if self.lookahead() == LEFTBRACE_TOKEN {
                self.shift();
                self.sc().push_state();
                let expr = self.parse_expression_list(allowIn_mode);
                let args = self.nf().argument_list(None, expr);
                value = Some(self.nf().invoke("[[ToXMLAttrString]]", Some(args), 0));
                self.r#match(RIGHTBRACE_TOKEN);
                self.sc().pop_state();
            } else {
                self.error(kError_ParserExpectingLeftBraceOrStringLiteral);
            }

            let s = self.nf().literal_string_pos(if single_quote { "='" } else { "=\"" }, 0);
            result = self.concat_xml(result, Some(s));

            result = self.concat_xml(result, value);

            let s = self.nf().literal_string_pos(if single_quote { "'" } else { "\"" }, 0);
            result = self.concat_xml(result, Some(s));
        }

        result
    }

    fn parse_xml_element_content(&mut self, first: Option<NodeRef>) -> Option<NodeRef> {
        let mut result = first;

        self.sc().push_state();

        self.sc().state = xmltext_state;
        while !(self.lookahead() == XMLTAGSTARTEND_TOKEN || self.lookahead() == EOS_TOKEN) {
            let lt = self.lookahead();

            if lt == LEFTBRACE_TOKEN {
                self.shift();
                self.sc().push_state();
                let expr = self.parse_expression_list(allowIn_mode);
                let args = self.nf().argument_list(None, expr);
                let expr = self.nf().invoke("[[ToXMLString]]", Some(args), 0);
                result = self.concat_xml(result, Some(expr));
                self.r#match(RIGHTBRACE_TOKEN);
                self.sc().pop_state();
            } else if lt == LESSTHAN_TOKEN {
                self.sc().state = start_state;
                let e = self.parse_xml_element();
                result = self.concat_xml(result, e);
            } else if lt == XMLMARKUP_TOKEN {
                let t = self.r#match(XMLMARKUP_TOKEN);
                let text = self.sc().get_current_token_text_or_type_text(t);
                let p = self.position_of_mark();
                let s = self.nf().literal_string_pos(&text, p);
                result = self.concat_xml(result, Some(s));
            } else if lt == XMLTEXT_TOKEN {
                let t = self.r#match(XMLTEXT_TOKEN);
                let text = self.sc().get_current_token_text_or_type_text(t);
                let p = self.position_of_mark();
                let s = self.nf().literal_string_pos(&text, p);
                result = self.concat_xml(result, Some(s));
            } else {
                self.error(kError_MissingXMLTagName);
                self.skiperror(RIGHTBRACE_TOKEN);
                self.sc().pop_state();
                return result;
            }
            self.sc().state = xmltext_state;
        }
        self.sc().pop_state();
        result
    }

    fn parse_paren_expression(&mut self) -> NodeRef {
        self.r#match(LEFTPAREN_TOKEN);
        let mark = self.position_of_mark();
        let result = self.parse_assignment_expression(allowIn_mode).expect("NullPointerException");
        result.borrow_mut().set_position(mark);
        self.r#match(RIGHTPAREN_TOKEN);
        result
    }

    fn parse_paren_expression_list(&mut self) -> Option<NodeRef> {
        self.r#match(LEFTPAREN_TOKEN);
        let result = self.parse_expression_list(allowIn_mode);
        self.r#match(RIGHTPAREN_TOKEN);
        result
    }

    fn parse_primary_expression_or_expression_qualified_identifier(&mut self) -> Option<NodeRef> {
        let mut result;

        if self.lookahead() == LEFTPAREN_TOKEN {
            let first = self.parse_paren_expression_list();
            result = first.clone();

            if HAS_EXPRESSIONQUALIFIEDIDS && self.lookahead() == DOUBLECOLON_TOKEN {
                self.shift();
                if first.is_none() || rb(&first).list().expect("ClassCastException").size() != 1 {
                    result = self.error(kError_Parser_ExpectingASingleExpressionWithinParenthesis);
                    self.skiperror_kind(ParseError::Expression);
                } else {
                    let last = rb(&first).list().unwrap().items.last().cloned().flatten();
                    let name = self.parse_identifier_string();
                    let p = self.position_of_mark();
                    let q = self.nf().qualified_identifier_pos(last, Some(&name), p);
                    let get = self.nf().get_expression(Some(q));
                    result = Some(self.nf().member_expression(None, Some(get)));
                }
            }
        } else {
            result = self.parse_primary_expression();
        }

        result
    }

    fn parse_function_common(&mut self, first: Option<NodeRef>) -> NodeRef {
        let mut third = None;

        let (saved_has_arguments, saved_has_rest, saved_has_dxns) = {
            let nf = self.nf();
            (nf.has_arguments, nf.has_rest, nf.has_dxns)
        };

        {
            let mut nf = self.nf();
            nf.has_arguments = false;
            nf.has_rest = false;
            nf.has_dxns = false;
        }

        let mut is_ctor = false;
        if self.ctx.borrow().statics.borrow().es4_nullability
            && *self.block_kind_stack.last().expect("ArrayIndexOutOfBoundsException") == CLASS_TOKEN
            && first.is_some()
            && Some(rb(&first).identifier().expect("ClassCastException").name.as_str()) == self.current_class_name.as_deref()
        {
            is_ctor = true;
        }

        self.block_kind_stack.push(FUNCTION_TOKEN);

        let second = if is_ctor { self.parse_constructor_signature() } else { self.parse_function_signature() };

        let saved_dxns = self.nf().dxns.clone();

        if self.lookahead() == LEFTBRACE_TOKEN {
            third = self.parse_block();

            if !AVMPLUS {
                let p = self.position_of_mark();
                let ret = self.nf().return_statement_pos(None, p);
                third = Some(self.nf().statement_list(third, Some(ret)));
            }

            if third.is_none() {
                third = Some(self.nf().statement_list_concat(None, None));
            }
        } else {
            self.match_semicolon(abbrevFunction_mode);
        }

        let pos = if first.is_some() { rb(&first).pos() } else { second.borrow().pos() };
        let ctx = self.ctx.clone();
        let result = self.nf().function_common_pos(&ctx, first, Some(second), third, pos);
        let has_dxns = self.nf().has_dxns;
        result.borrow_mut().as_function_common_mut().unwrap().default_dxns = if has_dxns { None } else { saved_dxns };

        {
            let mut nf = self.nf();
            nf.has_arguments = saved_has_arguments;
            nf.has_rest = saved_has_rest;
            nf.has_dxns = saved_has_dxns;
        }

        self.block_kind_stack.pop();

        result
    }

    fn parse_object_literal(&mut self) -> NodeRef {
        let mut first = None;

        let pos = self.position_of_mark();

        self.r#match(LEFTBRACE_TOKEN);

        if self.lookahead() == RIGHTBRACE_TOKEN {
            self.shift();
        } else {
            loop {
                let t = self.parse_literal_field();

                if t.is_some() {
                    first = Some(self.nf().argument_list(first, t));
                }

                if self.lookahead() != COMMA_TOKEN {
                    break;
                }

                self.shift();
            }
            self.r#match(RIGHTBRACE_TOKEN);
        }

        self.nf().literal_object_pos(first, pos)
    }

    fn parse_literal_field(&mut self) -> Option<NodeRef> {
        let mut l = None;

        let mut first = self.parse_field_or_config_name();
        if first.borrow().is_configuration_name() && self.lookahead() != COLON_TOKEN {
            l = Some(self.nf().list(None, Some(first)));
            first = self.parse_field_or_config_name();
        }
        self.r#match(COLON_TOKEN);

        let second = self.parse_assignment_expression(allowIn_mode);
        let mut result = self.nf().literal_field(Some(first), second);

        if l.is_some() {
            result = self.nf().list(l, Some(result));
        }

        Some(result)
    }

    fn parse_field_or_config_name(&mut self) -> NodeRef {
        let result;
        let lt = self.lookahead();

        if HAS_NONIDENTFIELDNAMES && lt == STRINGLITERAL_TOKEN {
            let mut is_single_quoted = false;
            self.shift();
            let enclosed_text = self.current_string_token_text(&mut is_single_quoted);
            let p = self.position_of_mark();
            result = self.nf().literal_string_quoted(&enclosed_text, p, is_single_quoted);
        } else if HAS_NONIDENTFIELDNAMES && lt == NUMBERLITERAL_TOKEN {
            self.shift();
            let text = self.current_token_text();
            let p = self.position_of_mark();
            result = self.nf().literal_number_pos(&text, p);
        } else if HAS_NONIDENTFIELDNAMES && lt == LEFTPAREN_TOKEN {
            result = self.parse_paren_expression();
        } else {
            let ident = self.parse_identifier();
            let name = ident.borrow().identifier().unwrap().name.clone();
            if self.config_namespaces.borrow().last().expect("IndexOutOfBoundsException").contains(&name)
                && self.lookahead() == DOUBLECOLON_TOKEN
            {
                self.shift();
                let s = self.parse_identifier_string();
                let p = self.position_of_mark();
                let qualid = self.nf().qualified_identifier_pos(Some(ident), Some(&s), p);
                qualid.borrow_mut().qualified_identifier_mut().unwrap().is_config_name = true;
                result = qualid;
            } else {
                result = ident;
            }
        }

        result
    }

    fn parse_array_literal(&mut self) -> NodeRef {
        let mut initializer_list = None;
        let pos = self.position_of_mark();

        self.r#match(LEFTBRACKET_TOKEN);
        if RIGHTBRACKET_TOKEN != self.lookahead() {
            initializer_list = self.parse_element_list(true);
        }
        let result = self.nf().literal_array_pos(initializer_list, pos);
        self.r#match(RIGHTBRACKET_TOKEN);

        result
    }

    fn parse_vector_literal(&mut self) -> NodeRef {
        let mut initializer_list = None;
        let pos = self.position_of_mark();

        self.r#match(LESSTHAN_TOKEN);
        let type_list = self.parse_type_expression_list();
        let id = self.nf().identifier_intern_pos("Vector", false, pos);
        let vector_type = self.nf().apply_type_expr(Some(id), type_list, pos);
        self.r#match(GREATERTHAN_TOKEN);

        self.r#match(LEFTBRACKET_TOKEN);
        if RIGHTBRACKET_TOKEN != self.lookahead() {
            initializer_list = self.parse_element_list(false);
        }
        self.r#match(RIGHTBRACKET_TOKEN);

        self.nf().literal_vector(Some(vector_type), initializer_list, pos)
    }

    fn parse_element_list(&mut self, allow_empty: bool) -> Option<NodeRef> {
        let mut result = None;

        loop {
            let t = self.parse_literal_element(allow_empty);

            if t.is_some() {
                result = Some(self.nf().argument_list(result, t));
            }

            if self.lookahead() != COMMA_TOKEN {
                break;
            }

            self.shift();
        }

        result
    }

    fn parse_literal_element(&mut self, allow_empty: bool) -> Option<NodeRef> {
        let result;
        let mut lt = self.lookahead();

        if lt == COMMA_TOKEN && allow_empty {
            let p = self.position_of_mark();
            result = Some(self.nf().empty_element_pos(p));
        } else if lt == RIGHTBRACKET_TOKEN {
            result = None;
        } else {
            let first = self.parse_assignment_expression(allowIn_mode);
            lt = self.lookahead();
            if rb(&first).is_configuration_name() && lt != COMMA_TOKEN && lt != RIGHTBRACKET_TOKEN {
                let list = self.nf().list(None, first);
                let e = self.parse_assignment_expression(allowIn_mode);
                result = Some(self.nf().list(Some(list), e));
            } else {
                result = first;
            }
        }

        result
    }

    fn parse_super_expression(&mut self) -> NodeRef {
        let pos = self.position_of_mark();
        self.shift();
        let mut first = None;
        if self.lookahead() == LEFTPAREN_TOKEN {
            first = Some(self.parse_paren_expression());
        }
        self.nf().super_expression(first, pos)
    }

    fn precedence(&self, token: i32, mode: i32) -> i32 {
        if token == IN_TOKEN && mode == noIn_mode {
            return 0;
        }

        binary_precedence[(-token) as usize]
    }

    fn parse_binary_expression(&mut self, mode: i32, prec: i32) -> Option<NodeRef> {
        let mut result = self.parse_unary_expression();

        let mut op = self.lookahead();
        let mut p1 = self.precedence(op, mode);
        let mut p2 = p1;
        while p1 >= prec {
            while p2 == p1 {
                self.shift();

                let t = self.parse_binary_expression(mode, p1 + 1);

                result = Some(self.nf().binary_expression(op, result, t));
                op = self.lookahead();
                p2 = self.precedence(op, mode);
            }
            p1 -= 1;
        }
        result
    }

    fn parse_postfix_expression(&mut self) -> Option<NodeRef> {
        let mut first;
        let mut lt = self.lookahead();

        match lt {
            PUBLIC_TOKEN | PRIVATE_TOKEN | PROTECTED_TOKEN | DEFAULT_TOKEN | GET_TOKEN | SET_TOKEN | IDENTIFIER_TOKEN
            | NAMESPACE_TOKEN | MULT_TOKEN | ATSIGN_TOKEN => {
                first = self.parse_attribute_expression();
                lt = self.lookahead();
                if (lt == PLUSPLUS_TOKEN || lt == MINUSMINUS_TOKEN) && !self.lookahead_semicolon(full_mode) {
                    first = self.parse_postfix_increment(first);
                }
            }

            NULL_TOKEN | TRUE_TOKEN | FALSE_TOKEN | NUMBERLITERAL_TOKEN | STRINGLITERAL_TOKEN | THIS_TOKEN
            | REGEXPLITERAL_TOKEN | LEFTPAREN_TOKEN | LEFTBRACKET_TOKEN | LEFTBRACE_TOKEN | FUNCTION_TOKEN => {
                first = self.parse_primary_expression_or_expression_qualified_identifier();
            }

            SUPER_TOKEN => {
                first = Some(self.parse_super_expression());
            }

            NEW_TOKEN => {
                first = self.parse_short_new_expression();

                if !is_kind(&first, |n| n.as_literal_vector().is_some()) {
                    if self.lookahead() == LEFTPAREN_TOKEN {
                        first = self.parse_arguments(first);
                    } else {
                        first = Some(self.nf().call_expression(first, None));
                    }

                    first = Some(self.nf().new_expression(first));
                    if !self.lookahead_semicolon(full_mode)
                        && (self.lookahead() == PLUSPLUS_TOKEN || self.lookahead() == MINUSMINUS_TOKEN)
                    {
                        first = self.parse_postfix_increment(first);
                    }
                }
            }

            _ => {
                first = self.parse_full_new_subexpression();
                if self.lookahead() == LEFTPAREN_TOKEN {
                    first = self.parse_arguments(first);
                }
            }
        }

        self.parse_full_postfix_expression_prime(first)
    }

    fn parse_postfix_increment(&mut self, first: Option<NodeRef>) -> Option<NodeRef> {
        let lt = self.lookahead();

        if lt == PLUSPLUS_TOKEN || lt == MINUSMINUS_TOKEN {
            self.shift();
            let p = self.position_of_mark();
            Some(self.nf().postfix_expression(lt, first, p))
        } else {
            let result = self.error(kError_Parser_ExpectingIncrOrDecrOperator);
            self.skiperror_kind(ParseError::Syntax);
            result
        }
    }

    fn parse_full_postfix_expression_prime(&mut self, mut first: Option<NodeRef>) -> Option<NodeRef> {
        let mut result = first.clone();
        let lt = self.lookahead();

        if lt == DOT_TOKEN || lt == LEFTBRACKET_TOKEN || lt == DOTLESSTHAN_TOKEN || (HAS_DESCENDOPERATORS && lt == DOUBLEDOT_TOKEN)
        {
            first = self.parse_property_operator(first);
            result = self.parse_full_postfix_expression_prime(first);
        } else if lt == LEFTPAREN_TOKEN {
            first = self.parse_arguments(first);
            result = self.parse_full_postfix_expression_prime(first);
        } else if !self.lookahead_semicolon(full_mode) && (lt == PLUSPLUS_TOKEN || lt == MINUSMINUS_TOKEN) {
            first = self.parse_postfix_increment(first);
            result = self.parse_full_postfix_expression_prime(first);
        }

        result
    }

    fn parse_attribute_expression(&mut self) -> Option<NodeRef> {
        let ident = self.parse_simple_qualified_identifier();
        let get = self.nf().get_expression(Some(ident));
        let first = self.nf().member_expression(None, Some(get));

        let lt = self.lookahead();

        if lt == DOT_TOKEN || lt == LEFTBRACKET_TOKEN || lt == DOUBLEDOT_TOKEN || lt == LEFTPAREN_TOKEN || lt == ATSIGN_TOKEN {
            self.parse_attribute_expression_prime(Some(first))
        } else {
            Some(first)
        }
    }

    fn parse_attribute_expression_prime(&mut self, first: Option<NodeRef>) -> Option<NodeRef> {
        let lt = self.lookahead();

        if lt == DOT_TOKEN || lt == DOUBLEDOT_TOKEN || lt == LEFTBRACKET_TOKEN || lt == DOTLESSTHAN_TOKEN {
            let p = self.parse_property_operator(first);
            self.parse_attribute_expression_prime(p)
        } else if lt == LEFTPAREN_TOKEN {
            let a = self.parse_arguments(first);
            self.parse_attribute_expression_prime(a)
        } else {
            first
        }
    }

    fn parse_full_new_expression(&mut self) -> Option<NodeRef> {
        self.shift();
        let first = self.parse_full_new_subexpression();
        let args = self.parse_arguments(first);
        Some(self.nf().new_expression(args))
    }

    fn parse_full_new_subexpression(&mut self) -> Option<NodeRef> {
        let result;
        let mut first;

        match self.lookahead() {
            NEW_TOKEN => {
                first = self.parse_full_new_expression();
                if is_kind(&first, |n| n.as_literal_vector().is_some()) {
                    result = first;
                } else {
                    result = self.parse_full_new_subexpression_prime(first);
                }
            }

            SUPER_TOKEN => {
                first = Some(self.parse_super_expression());
                first = self.parse_property_operator(first);
                result = self.parse_full_new_subexpression_prime(first);
            }

            LEFTPAREN_TOKEN => {
                first = self.parse_primary_expression_or_expression_qualified_identifier();
                result = self.parse_full_new_subexpression_prime(first);
            }

            PUBLIC_TOKEN | PRIVATE_TOKEN | PROTECTED_TOKEN | DEFAULT_TOKEN | GET_TOKEN | SET_TOKEN | NAMESPACE_TOKEN
            | MULT_TOKEN | IDENTIFIER_TOKEN | ATSIGN_TOKEN => {
                let q = self.parse_qualified_identifier();
                let get = self.nf().get_expression(Some(q));
                first = Some(self.nf().member_expression(None, Some(get)));
                result = self.parse_full_new_subexpression_prime(first);
            }

            _ => {
                first = self.parse_primary_expression();
                result = self.parse_full_new_subexpression_prime(first);
            }
        }

        result
    }

    fn parse_full_new_subexpression_prime(&mut self, mut first: Option<NodeRef>) -> Option<NodeRef> {
        let mut result = first.clone();
        let lt = self.lookahead();

        if lt == DOT_TOKEN
            || lt == LEFTBRACKET_TOKEN
            || lt == DOTLESSTHAN_TOKEN
            || (HAS_DESCENDOPERATORS && lt == DOUBLEDOT_TOKEN)
            || lt == ATSIGN_TOKEN
        {
            if lt == ATSIGN_TOKEN {
                // parsePropertyOperator consumes nothing for '@', so Java recurses until the stack overflows
                panic!("StackOverflowError");
            }
            first = self.parse_property_operator(first);
            result = self.parse_full_new_subexpression_prime(first);
        }

        result
    }

    fn parse_short_new_expression(&mut self) -> Option<NodeRef> {
        self.r#match(NEW_TOKEN);
        if LESSTHAN_TOKEN == self.lookahead() {
            Some(self.parse_vector_literal())
        } else {
            self.parse_short_new_subexpression()
        }
    }

    fn parse_short_new_subexpression(&mut self) -> Option<NodeRef> {
        if self.lookahead() == NEW_TOKEN {
            let mut result = self.parse_short_new_expression();
            if self.lookahead() == LEFTPAREN_TOKEN {
                result = self.parse_arguments(result);
            }
            Some(self.nf().new_expression(result))
        } else {
            self.parse_full_new_subexpression()
        }
    }

    fn parse_property_operator(&mut self, first: Option<NodeRef>) -> Option<NodeRef> {
        let mut result = None;
        let lt = self.lookahead();

        if lt == DOT_TOKEN {
            self.shift();
            if self.lookahead() == LEFTPAREN_TOKEN {
                self.shift();
                let list = self.parse_expression_list(allowIn_mode);
                let pos = rb(&first).pos();
                result = Some(self.nf().filter_operator(first, list, pos));
                self.r#match(RIGHTPAREN_TOKEN);
            } else {
                let q = self.parse_qualified_identifier();
                let get = self.nf().get_expression(Some(q));
                result = Some(self.nf().member_expression(first, Some(get)));
            }
        } else if lt == LEFTBRACKET_TOKEN {
            result = Some(self.parse_brackets(first));
        } else if lt == DOUBLEDOT_TOKEN {
            self.shift();
            let q = self.parse_qualified_identifier();
            let selector = self.nf().get_expression(Some(q));
            selector.borrow_mut().selector_mut().unwrap().set_mode(DOUBLEDOT_TOKEN);
            result = Some(self.nf().member_expression(first, Some(selector)));
        } else if lt == DOTLESSTHAN_TOKEN {
            result = self.parse_templatized_type_expression(first);
        }

        result
    }

    fn parse_brackets(&mut self, first: Option<NodeRef>) -> NodeRef {
        let second;

        self.r#match(LEFTBRACKET_TOKEN);

        if self.lookahead() == RIGHTBRACKET_TOKEN {
            self.error(kError_Lexical_SyntaxError);
            second = None;
        } else {
            second = self.parse_arguments_with_rest(allowIn_mode);
            rbm(&second).as_argument_list_mut().expect("ClassCastException").is_bracket_selector = true;
        }

        self.r#match(RIGHTBRACKET_TOKEN);
        let get = self.nf().get_expression(second);
        self.nf().indexed_member_expression(first, Some(get))
    }

    fn parse_arguments(&mut self, first: Option<NodeRef>) -> Option<NodeRef> {
        self.r#match(LEFTPAREN_TOKEN);
        let args = if self.lookahead() == RIGHTPAREN_TOKEN { None } else { self.parse_arguments_with_rest(allowIn_mode) };
        self.r#match(RIGHTPAREN_TOKEN);

        Some(self.nf().call_expression(first, args))
    }

    fn parse_arguments_with_rest(&mut self, mode: i32) -> Option<NodeRef> {
        let mut result = None;

        loop {
            let t;

            if self.lookahead() == TRIPLEDOT_TOKEN {
                self.shift();
                let e = self.parse_assignment_expression(allowIn_mode);
                t = Some(self.nf().rest_expression(e));
            } else {
                t = self.parse_assignment_expression(mode);
            }

            if t.is_some() {
                result = Some(self.nf().argument_list(result, t));
            }

            if self.lookahead() != COMMA_TOKEN {
                break;
            }

            self.shift();
        }

        result
    }

    fn parse_unary_expression(&mut self) -> Option<NodeRef> {
        let result;
        let pos;
        let op = self.lookahead();

        match op {
            DELETE_TOKEN => {
                self.shift();
                self.lookahead();
                let e = self.parse_postfix_expression();
                let p = self.position_of_mark();
                result = Some(self.nf().unary_expression_pos(op, e, p));
            }

            TYPEOF_TOKEN | PLUSPLUS_TOKEN | MINUSMINUS_TOKEN | PLUS_TOKEN | BITWISENOT_TOKEN | NOT_TOKEN => {
                self.shift();
                self.lookahead();
                let e = self.parse_unary_expression();
                let p = self.position_of_mark();
                result = Some(self.nf().unary_expression_pos(op, e, p));
            }

            VOID_TOKEN => {
                self.shift();
                let la = self.lookahead();
                pos = self.position_of_mark();
                result = if la == COMMA_TOKEN || la == SEMICOLON_TOKEN || la == RIGHTPAREN_TOKEN {
                    let zero = self.nf().literal_number_pos("0", pos);
                    Some(self.nf().unary_expression_pos(op, Some(zero), pos))
                } else {
                    let e = self.parse_unary_expression();
                    Some(self.nf().unary_expression_pos(op, e, pos))
                };
            }

            MINUS_TOKEN => {
                self.shift();
                if self.lookahead() == NEGMINLONGLITERAL_TOKEN {
                    self.shift();
                    let text = self.current_token_text();
                    let p = self.position_of_mark();
                    let lit = self.nf().literal_number_pos(&text, p);
                    let p = self.position_of_mark();
                    result = Some(self.nf().unary_expression_pos(op, Some(lit), p));
                } else {
                    let e = self.parse_unary_expression();
                    let p = self.position_of_mark();
                    result = Some(self.nf().unary_expression_pos(op, e, p));
                }
            }

            _ => {
                result = self.parse_postfix_expression();
            }
        }

        result
    }

    fn parse_conditional_expression(&mut self, mode: i32) -> Option<NodeRef> {
        let first = self.parse_binary_expression(mode, 4);
        let mut result = first.clone();

        if self.lookahead() == QUESTIONMARK_TOKEN {
            self.shift();
            let second = self.parse_assignment_expression(mode);
            self.r#match(COLON_TOKEN);
            let third = self.parse_assignment_expression(mode);
            result = Some(self.nf().conditional_expression(first, second, third));
        }

        result
    }

    fn parse_non_assignment_expression(&mut self, mode: i32) -> Option<NodeRef> {
        let first = self.parse_binary_expression(mode, 4);
        let mut result = first.clone();

        if self.lookahead() == QUESTIONMARK_TOKEN {
            self.shift();
            let second = self.parse_non_assignment_expression(mode);
            self.r#match(COLON_TOKEN);
            let third = self.parse_non_assignment_expression(mode);
            result = Some(self.nf().conditional_expression(first, second, third));
        }

        result
    }

    fn parse_assignment_expression(&mut self, mode: i32) -> Option<NodeRef> {
        let first = self.parse_conditional_expression(mode);
        self.parse_assignment_expression_suffix(mode, first)
    }

    fn parse_assignment_expression_suffix(&mut self, mode: i32, first: Option<NodeRef>) -> Option<NodeRef> {
        let mut result = first.clone();
        let lt = self.lookahead();

        let is_logical_assign = matches!(lt, LOGICALANDASSIGN_TOKEN | LOGICALXORASSIGN_TOKEN | LOGICALORASSIGN_TOKEN);
        let is_assign = matches!(
            lt,
            ASSIGN_TOKEN
                | MULTASSIGN_TOKEN
                | DIVASSIGN_TOKEN
                | MODULUSASSIGN_TOKEN
                | PLUSASSIGN_TOKEN
                | MINUSASSIGN_TOKEN
                | LEFTSHIFTASSIGN_TOKEN
                | RIGHTSHIFTASSIGN_TOKEN
                | UNSIGNEDRIGHTSHIFTASSIGN_TOKEN
                | BITWISEANDASSIGN_TOKEN
                | BITWISEXORASSIGN_TOKEN
                | BITWISEORASSIGN_TOKEN
        );

        if (is_logical_assign && HAS_LOGICALASSIGNMENT) || is_assign {
            self.shift();
            let third = self.parse_assignment_expression(mode);
            result = Some(self.nf().assignment_expression(first, lt, third));
        }

        result
    }

    fn parse_expression_list(&mut self, mode: i32) -> Option<NodeRef> {
        let mut result = None;

        loop {
            let t = self.parse_assignment_expression(mode);

            if t.is_some() {
                result = Some(self.nf().list(result, t));
            }

            if self.lookahead() != COMMA_TOKEN {
                break;
            }

            self.shift();
        }

        result
    }

    fn parse_list_expression_prime(&mut self, mode: i32, list: Option<NodeRef>) -> Option<NodeRef> {
        let result = list.clone();

        while self.lookahead() == COMMA_TOKEN {
            self.shift();
            let t = self.parse_assignment_expression(mode);
            self.nf().list(list.clone(), t);
        }

        result
    }

    fn parse_non_attribute_qualified_expression(&mut self) -> NodeRef {
        if self.lookahead() == LEFTPAREN_TOKEN {
            self.parse_expression_qualified_identifier()
        } else {
            self.parse_simple_qualified_identifier()
        }
    }

    fn parse_simple_type_identifier(&mut self) -> Option<NodeRef> {
        let mut result = None;

        loop {
            let t = self.parse_non_attribute_qualified_expression();
            let pos = t.borrow().pos();
            let get = self.nf().get_expression_pos(Some(t), pos);
            result = Some(self.nf().member_expression_pos(result, Some(get), pos));

            if self.lookahead() != DOT_TOKEN {
                break;
            }

            self.shift();
        }

        result
    }

    fn parse_templatized_type_expression(&mut self, first: Option<NodeRef>) -> Option<NodeRef> {
        self.shift();
        let type_list = self.parse_type_expression_list();
        let p = self.position_of_mark();
        let result = self.nf().apply_type_expr(first, type_list, p);

        let lt = self.lookahead();

        match lt {
            UNSIGNEDRIGHTSHIFTASSIGN_TOKEN => self.change_lookahead(RIGHTSHIFTASSIGN_TOKEN),
            UNSIGNEDRIGHTSHIFT_TOKEN => self.change_lookahead(RIGHTSHIFT_TOKEN),
            RIGHTSHIFT_TOKEN => self.change_lookahead(GREATERTHAN_TOKEN),
            RIGHTSHIFTASSIGN_TOKEN => self.change_lookahead(GREATERTHANOREQUALS_TOKEN),
            GREATERTHANOREQUALS_TOKEN => self.change_lookahead(ASSIGN_TOKEN),
            _ => {
                self.r#match(GREATERTHAN_TOKEN);
            }
        }

        Some(result)
    }

    fn parse_typename(&mut self) -> Option<NodeRef> {
        let first = self.parse_simple_type_identifier();
        let mut result = first.clone();

        if self.lookahead() == DOTLESSTHAN_TOKEN {
            result = self.parse_templatized_type_expression(first);
        }

        result
    }

    fn parse_typename_list(&mut self) -> Option<NodeRef> {
        let mut result = None;

        loop {
            let t = self.parse_typename();

            if t.is_some() {
                result = Some(self.nf().list(result, t));
            }

            if self.lookahead() != COMMA_TOKEN {
                break;
            }

            self.shift();
        }

        result
    }

    fn parse_type_expression(&mut self, _mode: i32) -> Option<NodeRef> {
        let result;

        if self.ctx.borrow().dialect(7) && self.next_token == IDENTIFIER_TOKEN && self.current_token_text() == "Object" {
            self.r#match(IDENTIFIER_TOKEN);
            result = None;
        } else {
            let first = self.parse_typename();
            let mut is_nullable = true;
            let mut is_explicit = false;
            let lt = self.lookahead();

            if lt == NOT_TOKEN {
                self.shift();
                is_nullable = false;
                is_explicit = true;
            } else if lt == QUESTIONMARK_TOKEN {
                self.shift();
                is_nullable = true;
                is_explicit = true;
            }

            result = Some(self.nf().type_expression(first, is_nullable, is_explicit, -1));
        }

        result
    }

    fn parse_statement(&mut self, mode: i32) -> Option<NodeRef> {
        let result;
        let lt = self.lookahead();

        match lt {
            SUPER_TOKEN => {
                result = Some(self.parse_super_statement(mode));
            }

            LEFTBRACE_TOKEN => {
                result = self.parse_block();
            }

            LEFTBRACKET_TOKEN | XMLLITERAL_TOKEN | DOCCOMMENT_TOKEN => {
                result = Some(self.parse_meta_data());
            }

            IF_TOKEN => {
                result = Some(self.parse_if_statement(mode));
            }

            SWITCH_TOKEN => {
                result = Some(self.parse_switch_statement());
            }

            DO_TOKEN => {
                result = Some(self.parse_do_statement());
                self.match_semicolon(mode);
            }

            WHILE_TOKEN => {
                result = Some(self.parse_while_statement(mode));
            }

            FOR_TOKEN => {
                result = self.parse_for_statement(mode);
            }

            WITH_TOKEN => {
                result = Some(self.parse_with_statement(mode));
            }

            CONTINUE_TOKEN => {
                result = Some(self.parse_continue_statement());
                self.match_semicolon(mode);
            }

            BREAK_TOKEN => {
                result = Some(self.parse_break_statement());
                self.match_semicolon(mode);
            }

            RETURN_TOKEN => {
                result = Some(self.parse_return_statement());
                self.match_semicolon(mode);
            }

            THROW_TOKEN => {
                result = Some(self.parse_throw_statement());
                self.match_semicolon(mode);
            }

            TRY_TOKEN => {
                result = self.parse_try_statement();
            }

            _ => {
                let r = self.parse_labeled_or_expression_statement(mode);
                if !r.borrow().is_labeled_statement() {
                    self.match_semicolon(mode);
                }
                result = Some(r);
            }
        }

        result
    }

    fn parse_substatement(&mut self, mode: i32) -> Option<NodeRef> {
        let lt = self.lookahead();

        if lt == SEMICOLON_TOKEN {
            self.shift();
            Some(self.nf().empty_statement())
        } else if lt == VAR_TOKEN {
            let result = self.parse_variable_definition(None, mode);
            self.match_semicolon(mode);
            Some(result)
        } else {
            self.parse_annotated_substatements_or_statement(mode)
        }
    }

    fn parse_super_statement(&mut self, mode: i32) -> NodeRef {
        let result;

        self.shift();
        let p = self.position_of_mark();
        let first = self.nf().super_expression(None, p);

        if self.lookahead() == LEFTPAREN_TOKEN {
            let call = self.parse_arguments(Some(first.clone()));
            if !is_kind(&call, |n| n.as_call_expression().is_some()) && call.is_some() {
                panic!("ClassCastException");
            }

            if self.lookahead_semicolon(mode) {
                result = self.nf().super_statement(call);
                self.match_semicolon(mode);
            } else {
                let arg0 = {
                    let c = call.as_ref().map(|c| c.borrow());
                    let args = c.as_ref().and_then(|c| c.as_call_expression().unwrap().args.clone());
                    match &args {
                        Some(a) if a.borrow().as_argument_list().expect("ClassCastException").size() == 1 => {
                            Some(a.borrow().as_argument_list().unwrap().items[0].clone())
                        }
                        _ => None,
                    }
                };
                match arg0 {
                    None => {
                        self.error(kError_Parser_WrongNumberOfSuperArgs);
                    }
                    Some(arg0) => {
                        first.borrow_mut().as_super_expression_mut().unwrap().expr = arg0;
                    }
                }
                let t = self.parse_full_postfix_expression_prime(Some(first));
                let l = self.nf().list(None, t);
                let lp = self.parse_list_expression_prime(mode, Some(l));
                result = self.nf().expression_statement(lp);
                self.match_semicolon(mode);
            }
        } else if self.lookahead() == DOT_TOKEN || self.lookahead() == LEFTBRACKET_TOKEN {
            let mut t = self.parse_full_postfix_expression_prime(Some(first));
            if self.lookahead() == ASSIGN_TOKEN {
                t = self.parse_assignment_expression_suffix(mode, t);
            }
            let l = self.nf().list(None, t);
            let lp = self.parse_list_expression_prime(mode, Some(l));
            result = self.nf().expression_statement(lp);
            self.match_semicolon(mode);
        } else {
            let la = self.lookahead();
            let text = self.sc().get_current_token_text_or_type_text(la);
            self.error2(ParseError::Eos, kError_Parser_ExpectedToken, get_token_class_name(LEFTPAREN_TOKEN), &text);
            result = self.nf().error_code(kError_Parser_ExpectedToken);
            self.skiperror(SEMICOLON_TOKEN);
        }

        result
    }

    fn parse_labeled_or_expression_statement(&mut self, mode: i32) -> NodeRef {
        let first = self.parse_expression_list(allowIn_mode);

        if self.lookahead() == COLON_TOKEN {
            if !rb(&first).is_label() {
                self.error(kError_Parser_LabelIsNotIdentifier);
            }
            self.shift();
            let s = self.parse_substatement(mode);
            self.nf().labeled_statement(first, s)
        } else {
            self.nf().expression_statement(first)
        }
    }

    fn parse_block(&mut self) -> Option<NodeRef> {
        self.parse_block_with(None)
    }

    /// `parseBlock(AttributeListNode first)`.
    fn parse_block_with(&mut self, first: Option<NodeRef>) -> Option<NodeRef> {
        self.r#match(LEFTBRACE_TOKEN);
        let result = self.parse_directives(first, None);
        self.r#match(RIGHTBRACE_TOKEN);
        result
    }

    pub fn parse_meta_data(&mut self) -> NodeRef {
        let pos = self.position_of_mark();

        if self.lookahead() == DOCCOMMENT_TOKEN {
            self.shift();
            let text = self.current_token_text();
            let mut nf = self.nf();
            let s = nf.literal_string_pos(&text, pos);
            let list = nf.list(None, Some(s));
            let first = nf.literal_xml(Some(list), false, pos);
            let get = nf.get_expression(Some(first));
            let x = nf.member_expression_pos(None, Some(get), pos);
            let args = nf.argument_list(None, Some(x));
            let array = nf.literal_array_pos(Some(args), pos);
            nf.doc_comment(Some(array), pos)
        } else if self.lookahead() == XMLLITERAL_TOKEN {
            let xml = self.parse_xml_literal();
            let mut nf = self.nf();
            let args = nf.argument_list(None, xml);
            let array = nf.literal_array(Some(args));
            nf.meta_data(Some(array), pos)
        } else {
            let array = self.parse_array_literal();
            self.nf().meta_data(Some(array), pos)
        }
    }

    fn parse_if_statement(&mut self, mode: i32) -> NodeRef {
        self.shift();

        let first = self.parse_paren_expression_list();
        let second = self.parse_substatement(abbrevIfElse_mode);
        let mut third = None;

        if self.lookahead() == ELSE_TOKEN {
            self.shift();
            third = self.parse_substatement(mode);
        }

        self.nf().if_statement(first, second, third)
    }

    fn parse_switch_statement(&mut self) -> NodeRef {
        self.shift();

        let first = self.parse_paren_expression_list();

        self.r#match(LEFTBRACE_TOKEN);
        let second = self.parse_case_statements();
        self.r#match(RIGHTBRACE_TOKEN);

        self.nf().switch_statement(first, second)
    }

    fn parse_case_label(&mut self) -> Option<NodeRef> {
        let mut result = None;
        let lt = self.lookahead();

        if lt == CASE_TOKEN {
            self.shift();
            let e = self.parse_expression_list(allowIn_mode);
            let p = self.position_of_mark();
            result = Some(self.nf().case_label_pos(e, p));
        } else if lt == DEFAULT_TOKEN {
            self.shift();
            let p = self.position_of_mark();
            result = Some(self.nf().case_label_pos(None, p));
        } else {
            self.error(kError_Parser_ExpectedCaseLabel);
            self.skiperror(RIGHTBRACE_TOKEN);
        }
        self.r#match(COLON_TOKEN);

        result
    }

    fn parse_case_statements(&mut self) -> Option<NodeRef> {
        let mut result = None;
        let mut lt = self.lookahead();

        if lt != RIGHTBRACE_TOKEN && lt != EOS_TOKEN {
            let label = self.parse_case_label();
            result = Some(self.nf().statement_list(None, label));

            lt = self.lookahead();
            while lt != RIGHTBRACE_TOKEN && lt != EOS_TOKEN {
                let t = if lt == CASE_TOKEN || lt == DEFAULT_TOKEN {
                    self.parse_case_label()
                } else {
                    self.parse_directive(None, full_mode)
                };

                result = Some(self.nf().statement_list(result, t));
                lt = self.lookahead();
            }
        }

        result
    }

    fn parse_do_statement(&mut self) -> NodeRef {
        self.shift();
        let first = self.parse_substatement(abbrevDoWhile_mode);
        self.r#match(WHILE_TOKEN);
        let expr = self.parse_paren_expression_list();
        self.nf().do_statement(first, expr)
    }

    fn parse_while_statement(&mut self, mode: i32) -> NodeRef {
        self.shift();
        let first = self.parse_paren_expression_list();
        let second = self.parse_substatement(mode);
        self.nf().while_statement(first, second)
    }

    fn parse_for_statement(&mut self, mode: i32) -> Option<NodeRef> {
        let result;
        let mut is_each = false;

        self.shift();

        if self.lookahead() == IDENTIFIER_TOKEN {
            self.shift();
            if self.current_token_text() != "each" {
                self.error(kError_Parser_ExpectedLeftParen);
            }
            is_each = true;
        }

        self.r#match(LEFTPAREN_TOKEN);

        let first;
        let lt = self.lookahead();

        if lt == SEMICOLON_TOKEN {
            first = None;
        } else if lt == CONST_TOKEN || lt == VAR_TOKEN {
            first = Some(self.parse_variable_definition(None, noIn_mode));
        } else if is_each {
            let e = self.parse_postfix_expression();
            first = Some(self.nf().list(None, e));
        } else {
            first = self.parse_expression_list(noIn_mode);
        }

        let second;
        if self.lookahead() == IN_TOKEN {
            let var_def_list = first.as_ref().and_then(|f| f.borrow().as_variable_definition().map(|vd| vd.list.clone()));
            let var_def_size = var_def_list.map(|l| rb(&l).list().expect("ClassCastException").size());
            let list_size = first.as_ref().and_then(|f| f.borrow().list().map(|l| l.size()));
            if var_def_size.map_or(false, |s| s > 1) {
                self.error(kError_ParserInvalidForInInitializer);
            } else if list_size.map_or(false, |s| s > 1) {
                self.error(kError_ParserInvalidForInInitializer);
            }

            self.r#match(IN_TOKEN);
            second = self.parse_expression_list(allowIn_mode);
            self.r#match(RIGHTPAREN_TOKEN);

            let pos = self.position_of_mark();
            let sub = self.parse_substatement(mode);
            result = Some(self.nf().for_in_statement_pos(is_each, first, second, sub, pos));
        } else if self.lookahead() == COLON_TOKEN {
            self.r#match(IN_TOKEN);
            self.skiperror(LEFTBRACE_TOKEN);
            result = self.parse_substatement(mode);
        } else {
            if is_each {
                self.error(kError_Parser_EachWithoutIn);
            }

            self.r#match(SEMICOLON_TOKEN);

            second = if self.lookahead() == SEMICOLON_TOKEN { None } else { self.parse_expression_list(allowIn_mode) };
            self.r#match(SEMICOLON_TOKEN);

            let third = if self.lookahead() == RIGHTPAREN_TOKEN { None } else { self.parse_expression_list(allowIn_mode) };
            self.r#match(RIGHTPAREN_TOKEN);

            let pos = self.position_of_mark();
            let sub = self.parse_substatement(mode);
            result = Some(self.nf().for_statement_pos(first, second, third, sub, false, pos));
        }
        result
    }

    fn parse_with_statement(&mut self, mode: i32) -> NodeRef {
        let pos = self.position_of_mark();
        self.shift();
        let first = self.parse_paren_expression_list();
        let sub = self.parse_substatement(mode);
        self.nf().with_statement_pos(first, sub, pos)
    }

    fn parse_continue_statement(&mut self) -> NodeRef {
        let mut first = None;
        self.shift();

        if !self.lookahead_semicolon(full_mode) {
            first = Some(self.parse_identifier());
        }

        let p = self.position_of_mark();
        self.nf().continue_statement_pos(first, p)
    }

    fn parse_break_statement(&mut self) -> NodeRef {
        let mut first = None;
        self.shift();

        if !self.lookahead_semicolon(full_mode) {
            first = Some(self.parse_identifier());
        }

        let p = self.position_of_mark();
        self.nf().break_statement_pos(first, p)
    }

    fn parse_return_statement(&mut self) -> NodeRef {
        let mut first = None;
        self.shift();

        if !self.lookahead_semicolon(full_mode) {
            first = self.parse_expression_list(allowIn_mode);
        }

        let p = self.position_of_mark();
        self.nf().return_statement_pos(first, p)
    }

    fn parse_throw_statement(&mut self) -> NodeRef {
        let pos = self.position_of_mark();
        self.shift();

        self.lookahead();

        if self.sc().follows_line_terminator() {
            self.error_pos(ParseError::Syntax, kError_Parser_ThrowWithoutExpression, "", "", pos);
            self.nf().throw_statement(None, pos)
        } else {
            let e = self.parse_expression_list(allowIn_mode);
            self.nf().throw_statement(e, pos)
        }
    }

    fn parse_try_statement(&mut self) -> Option<NodeRef> {
        let result;

        self.shift();
        let first = self.parse_block();
        if self.lookahead() == CATCH_TOKEN {
            let second = self.parse_catch_clauses();
            if self.lookahead() == FINALLY_TOKEN {
                let f = self.parse_finally_clause();
                result = Some(self.nf().try_statement(first, Some(second), Some(f)));
            } else {
                result = Some(self.nf().try_statement(first, Some(second), None));
            }
        } else if self.lookahead() == FINALLY_TOKEN {
            let catch_clause = {
                let mut nf = self.nf();
                let throw = nf.throw_statement(None, 0);
                let statements = nf.statement_list(None, Some(throw));
                let clause = nf.catch_clause(None, Some(statements));
                nf.statement_list(None, Some(clause))
            };

            let f = self.parse_finally_clause();
            result = Some(self.nf().try_statement(first, Some(catch_clause), Some(f)));
        } else {
            self.error(kError_Parser_ExpectingCatchOrFinally);
            self.skiperror(SEMICOLON_TOKEN);
            result = None;
        }

        result
    }

    fn parse_catch_clauses(&mut self) -> NodeRef {
        let clause = self.parse_catch_clause();
        let mut result = self.nf().statement_list(None, Some(clause));
        while self.lookahead() == CATCH_TOKEN {
            let clause = self.parse_catch_clause();
            result = self.nf().statement_list(Some(result), Some(clause));
        }

        result
    }

    fn parse_catch_clause(&mut self) -> NodeRef {
        self.shift();

        self.r#match(LEFTPAREN_TOKEN);
        let first = self.parse_parameter();
        self.r#match(RIGHTPAREN_TOKEN);

        let block = self.parse_block();
        self.nf().catch_clause(Some(first), block)
    }

    fn parse_finally_clause(&mut self) -> NodeRef {
        self.shift();

        let block = self.parse_block();
        self.nf().finally_clause(block)
    }

    fn parse_directive(&mut self, first: Option<NodeRef>, mode: i32) -> Option<NodeRef> {
        // Java catches every Exception here and yields null.
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| -> Option<NodeRef> {
            let result;
            let lt = self.lookahead();

            match lt {
                SEMICOLON_TOKEN => {
                    self.match_noninsertable_semicolon(mode);
                    result = Some(self.nf().empty_statement());
                }

                INTERFACE_TOKEN | VAR_TOKEN | CONST_TOKEN | FUNCTION_TOKEN | CLASS_TOKEN | NAMESPACE_TOKEN | IMPORT_TOKEN
                | INCLUDE_TOKEN | USE_TOKEN => {
                    if lt == INTERFACE_TOKEN && !HAS_INTERFACEDEFINITIONS {
                        result = self.parse_annotated_directive_or_statement(mode);
                    } else {
                        result = self.parse_annotatable_directive_or_pragma_or_include(first, mode);
                    }
                }

                DEFAULT_TOKEN => {
                    self.shift();
                    if self.r#match(IDENTIFIER_TOKEN) == IDENTIFIER_TOKEN
                        && self.current_token_text() != "xml"
                        && self.lookahead() == NAMESPACE_TOKEN
                    {
                        self.error(kError_Parser_ExpectedXMLBeforeNameSpace);
                    }
                    self.r#match(NAMESPACE_TOKEN);
                    self.r#match(ASSIGN_TOKEN);
                    let e = self.parse_non_assignment_expression(allowIn_mode);
                    result = Some(self.nf().default_xml_namespace(e, 0));
                }

                _ => {
                    result = self.parse_annotated_directive_or_statement(mode);
                }
            }

            result
        }));

        match outcome {
            Ok(r) => r,
            // an Error such as StackOverflowError is not an Exception, so Java's catch lets it through
            Err(e) if e.downcast_ref::<&str>() == Some(&"StackOverflowError") => panic::resume_unwind(e),
            Err(_) => None,
        }
    }

    fn parse_annotated_directive_or_statement(&mut self, mode: i32) -> Option<NodeRef> {
        let mut result = None;
        let mut lt = self.lookahead();

        if Self::in_statement_token_set(lt) {
            result = self.parse_statement(mode);
        } else {
            let mut temp;
            if HAS_SQUAREBRACKETATTRS && lt == LEFTBRACKET_TOKEN {
                self.shift();
                temp = self.parse_assignment_expression(allowIn_mode);
                self.r#match(RIGHTBRACKET_TOKEN);
            } else {
                temp = Some(self.parse_labeled_or_expression_statement(mode));
            }

            if rb(&temp).is_labeled_statement() {
                result = temp;
            } else {
                let next_token = self.next_token;
                let directive_string = self.sc().get_current_token_text_or_type_text(next_token);

                if self.lookahead_semicolon(mode) && !rb(&temp).is_configuration_name() {
                    self.match_semicolon(mode);

                    result = temp.clone();

                    if is_kind(&temp, |n| n.as_expression_statement().is_some()) {
                        let expr = rb(&temp).as_expression_statement().unwrap().expr.clone();
                        temp = expr;
                    }

                    let is_attribute_keyword = self.check_attribute(&temp);

                    if is_attribute_keyword {
                        let which = self.which_attribute(&temp);
                        self.error2(
                            ParseError::Syntax,
                            kError_Parser_ExpectingAnnotatableDirectiveAfterAttributes,
                            &which,
                            &directive_string,
                        );
                    }
                } else if rb(&temp).is_attribute() {
                    let mut estmt = None;
                    if is_kind(&temp, |n| n.as_expression_statement().is_some()) {
                        estmt = temp.clone();
                        let expr = rb(&temp).as_expression_statement().unwrap().expr.clone();
                        temp = expr;
                    }

                    let is_attribute_keyword = self.check_attribute(&temp);
                    let first;

                    match self.lookahead() {
                        TRUE_TOKEN | FALSE_TOKEN | PRIVATE_TOKEN | PUBLIC_TOKEN | PROTECTED_TOKEN | IDENTIFIER_TOKEN => {
                            let list = self.parse_attribute_list();
                            first = self.nf().attribute_list(temp.clone(), list);
                        }
                        _ => {
                            first = self.nf().attribute_list(temp.clone(), None);
                        }
                    }

                    lt = self.lookahead();
                    match lt {
                        VAR_TOKEN | CONST_TOKEN | FUNCTION_TOKEN | CLASS_TOKEN | NAMESPACE_TOKEN | IMPORT_TOKEN | USE_TOKEN
                        | INTERFACE_TOKEN | DOCCOMMENT_TOKEN => {
                            result = self.parse_annotatable_directive_or_pragma_or_include(Some(first), mode);
                        }
                        _ => {
                            if rb(&temp).is_configuration_name() && lt == LEFTBRACE_TOKEN {
                                result = self.parse_annotatable_directive_or_pragma_or_include(Some(first), mode);
                            } else if lt == PACKAGE_TOKEN {
                                self.error(kError_AttributesOnPackage);
                                result = self.parse_package_definition();
                            } else if is_attribute_keyword || first.borrow().as_attribute_list().unwrap().size() > 1 {
                                let which = self.which_attribute(&temp);
                                self.error2(
                                    ParseError::Syntax,
                                    kError_Parser_ExpectingAnnotatableDirectiveAfterAttributes,
                                    &which,
                                    &directive_string,
                                );
                                self.skiperror(SEMICOLON_TOKEN);
                            } else if rb(&temp).is_configuration_name() {
                                result = estmt;
                            } else {
                                result = estmt;
                            }
                        }
                    }
                } else if self.ctx.borrow().error_count() == 0 {
                    self.match_semicolon(mode);
                }
            }
        }

        result
    }

    fn parse_annotated_substatements_or_statement(&mut self, mode: i32) -> Option<NodeRef> {
        let mut result = None;

        let lt = self.lookahead();
        if Self::in_statement_token_set(lt) {
            result = self.parse_statement(mode);
        } else {
            let t = self.parse_labeled_or_expression_statement(mode);

            if t.borrow().is_labeled_statement() {
                result = Some(t);
                if self.lookahead() == SEMICOLON_TOKEN {
                    self.shift();
                }
            } else if self.lookahead_semicolon(mode) {
                self.match_semicolon(mode);
                result = Some(t);
            } else if t.borrow().is_attribute() {
                self.error(kError_InvalidAttribute);
            } else {
                self.r#match(SEMICOLON_TOKEN);
            }
        }

        result
    }

    fn parse_annotatable_directive_or_pragma_or_include(&mut self, first: Option<NodeRef>, mode: i32) -> Option<NodeRef> {
        let mut result = None;
        let lt = self.lookahead();

        match lt {
            CONST_TOKEN | VAR_TOKEN => {
                result = Some(self.parse_variable_definition(first, mode));
                self.match_semicolon(mode);
            }

            FUNCTION_TOKEN => {
                result = Some(self.parse_function_definition(first));
            }

            CLASS_TOKEN => {
                result = Some(self.parse_class_definition(first, mode));
            }

            INTERFACE_TOKEN => {
                result = Some(self.parse_interface_definition(first, mode));
            }

            NAMESPACE_TOKEN => {
                result = Some(self.parse_namespace_definition(first));
                self.match_semicolon(mode);
            }

            IMPORT_TOKEN => {
                result = Some(self.parse_import_directive(first));
                self.match_semicolon(mode);
            }

            USE_TOKEN => {
                result = self.parse_use_directive(first);
                self.match_semicolon(mode);
            }

            INCLUDE_TOKEN => {
                result = self.parse_include_directive();
                self.match_semicolon(mode);
            }

            LEFTBRACE_TOKEN => {
                let stmts = self.parse_block();
                rbm(&stmts).as_statement_list_mut().expect("ClassCastException").config_attrs = first;
                result = stmts;
                self.match_semicolon(mode);
            }

            _ => {
                self.error(kError_Parser_DefinitionOrDirectiveExpected);
                self.skiperror(SEMICOLON_TOKEN);
            }
        }

        result
    }

    pub fn parse_directives(&mut self, first: Option<NodeRef>, mut list: Option<NodeRef>) -> Option<NodeRef> {
        loop {
            let lt = self.lookahead();
            if lt == RIGHTBRACE_TOKEN || lt == EOS_TOKEN {
                break;
            }

            let t = self.parse_directive(first.clone(), abbrev_mode);

            if t.is_none() {
                break;
            }

            if is_kind(&t, |n| n.as_statement_list().is_some()) {
                let (collapse, items) = {
                    let n = rb(&t);
                    let sln = n.as_statement_list().unwrap();
                    (sln.config_attrs.is_none() && !sln.has_pragma, sln.items.clone())
                };
                if collapse {
                    if list.is_none() {
                        list = t;
                    } else {
                        rbm(&list).as_statement_list_mut().expect("ClassCastException").items.extend(items);
                    }
                } else {
                    list = Some(self.nf().statement_list(list, t));
                }
            } else {
                list = Some(self.nf().statement_list(list, t));
            }
        }

        list
    }

    fn parse_use_directive(&mut self, first: Option<NodeRef>) -> Option<NodeRef> {
        let mut result = None;

        self.shift();

        if self.lookahead() == NAMESPACE_TOKEN {
            self.shift();
            let e = self.parse_non_assignment_expression(allowIn_mode);
            result = Some(self.nf().use_directive(first, e));
        } else if self.lookahead() == INCLUDE_TOKEN {
            result = self.parse_include_directive();
        } else if self.ctx.borrow().statics.borrow().es4_numerics {
            let list = self.parse_pragma_item_list(allowIn_mode);
            let p = self.position_of_mark();
            result = Some(self.nf().pragma(list, p));
        } else {
            self.error(kError_UndefinedNamespace);
        }

        result
    }

    fn parse_pragma_item(&mut self, mode: i32) -> Option<NodeRef> {
        let argument;

        let id = self.parse_identifier();
        let lt = self.lookahead();
        let pos = self.position_of_mark();

        if lt == COMMA_TOKEN || self.lookahead_semicolon(mode) {
            argument = None;
        } else if lt == TRUE_TOKEN {
            self.shift();
            argument = Some(self.nf().literal_boolean_pos(true, pos));
        } else if lt == FALSE_TOKEN {
            self.shift();
            argument = Some(self.nf().literal_boolean_pos(false, pos));
        } else if lt == NUMBERLITERAL_TOKEN {
            self.shift();
            let text = self.current_token_text();
            argument = Some(self.nf().literal_number_pos(&text, pos));
        } else if lt == STRINGLITERAL_TOKEN {
            let mut is_single_quoted = false;
            self.shift();
            let enclosed_text = self.current_string_token_text(&mut is_single_quoted);
            argument = Some(self.nf().literal_string_quoted(&enclosed_text, pos, is_single_quoted));
        } else {
            argument = Some(self.parse_identifier());
        }

        let p = self.position_of_mark();
        Some(self.nf().use_pragma_pos(Some(id), argument, p))
    }

    fn parse_pragma_item_list(&mut self, mode: i32) -> Option<NodeRef> {
        let mut result = None;

        loop {
            let t = self.parse_pragma_item(mode);
            if t.is_some() {
                result = Some(self.nf().list(result, t));
            }

            if self.lookahead() != COMMA_TOKEN {
                break;
            }
            self.shift();
        }

        result
    }

    fn parse_include_directive(&mut self) -> Option<NodeRef> {
        self.shift();

        let mut is_single_quoted = false;
        let mut filespec = None;

        if self.lookahead() == STRINGLITERAL_TOKEN {
            self.shift();
            filespec = Some(java_trim(&self.current_string_token_text(&mut is_single_quoted)));
        } else {
            self.r#match(STRINGLITERAL_TOKEN);
        }

        let mut incl: Option<FileInclude> = None;
        let handler = self.ctx.borrow().handler.clone();
        if let Some(handler) = handler {
            let path = self.ctx.borrow().path();
            incl = handler.borrow_mut().find_file_include(path.as_deref(), filespec.as_deref());
        }

        let mut in_: Option<Vec<u8>> = None;
        let mut text: Option<String> = None;

        let fixed_filespec;
        let parent_path;
        match incl {
            None => {
                let filespec = filespec.expect("NullPointerException").replace('/', MAIN_SEPARATOR_STR);

                let mut fixed = if Path::new(&filespec).is_absolute() {
                    java_absolute_path(&filespec)
                } else {
                    let path = self.ctx.borrow().path();
                    format!("{}{}{}", path.unwrap_or_default(), MAIN_SEPARATOR, filespec)
                };

                fixed = java_canonical_path(&fixed);

                let last_separator = fixed.rfind(MAIN_SEPARATOR).map_or(-1, |i| java_length(&fixed[..i]));
                parent_path = java_substring(&fixed, 0, last_separator);
                if !self.ctx.borrow().script_assist_parsing {
                    match std::fs::read(&fixed) {
                        Ok(bytes) => in_ = Some(bytes),
                        Err(_) => {
                            self.error1(ParseError::Syntax, kError_Parser_UnableToOpenFile, &fixed);
                            return None;
                        }
                    }
                }
                fixed_filespec = fixed;
            }
            Some(incl) => {
                fixed_filespec = incl.fixed_filespec;
                parent_path = incl.parent_path;
                in_ = incl.in_;
                text = incl.text;
            }
        }

        let statics = self.ctx.borrow().statics.clone();
        if statics.borrow().include_paths.contains(&fixed_filespec) {
            self.error1(ParseError::Syntax, kError_Parser_FileIncludesItself, &fixed_filespec);
            in_.as_ref().expect("NullPointerException");
            return None;
        } else {
            statics.borrow_mut().include_paths.push(fixed_filespec.clone());
        }

        let old_ctx_path_spec = self.ctx.borrow().path();
        self.ctx.borrow().set_path(Some(parent_path));

        let p = self.position_of_mark();
        let first = self.nf().literal_string_quoted(&fixed_filespec, p, is_single_quoted);

        let mut second = None;
        let result;
        if !self.ctx.borrow().script_assist_parsing {
            let cx = Rc::new(RefCell::new(Context::new(statics.clone())));
            let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
                let mut p;
                if let Some(bytes) = &in_ {
                    p = Parser::new_stream_include(
                        &cx,
                        bytes,
                        &fixed_filespec,
                        self.encoding.as_deref(),
                        self.create_doc_info,
                        self.save_comment_nodes,
                        Some(&self.block_kind_stack),
                        true,
                    );
                } else {
                    p = Parser::new_include(
                        &cx,
                        text.as_deref().expect("NullPointerException"),
                        &fixed_filespec,
                        self.create_doc_info,
                        self.save_comment_nodes,
                        Some(&self.block_kind_stack),
                        true,
                    );
                }
                p.config_namespaces = self.config_namespaces.clone();
                p.parse_program()
            }));

            self.ctx.borrow().set_path(old_ctx_path_spec);
            statics.borrow_mut().include_paths.pop();

            match outcome {
                Ok(program) => second = program,
                Err(e) => panic::resume_unwind(e),
            }
            result = self.nf().include_directive(&cx, Some(first), second);
        } else {
            let ctx = self.ctx.clone();
            result = self.nf().include_directive(&ctx, Some(first), second);
        }

        Some(result)
    }

    fn check_attribute(&mut self, node: &Option<NodeRef>) -> bool {
        let token_id = *self.block_kind_stack.last().expect("ArrayIndexOutOfBoundsException");
        let n = rb(node);
        let pos = n.pos();

        if token_id != ERROR_TOKEN && token_id != CLASS_TOKEN && token_id != INTERFACE_TOKEN {
            if n.has_attribute("private") {
                self.ctx.borrow().error(pos, kError_InvalidPrivate);
            } else if n.has_attribute("protected") {
                self.ctx.borrow().error(pos, kError_InvalidProtected);
            } else if n.has_attribute("static") {
                self.ctx.borrow().error(pos, kError_InvalidStatic);
            } else if token_id != PACKAGE_TOKEN && token_id != EMPTY_TOKEN && n.has_attribute("internal") {
                self.ctx.borrow().error(pos, kError_InvalidInternal);
            } else if token_id != PACKAGE_TOKEN && n.has_attribute("public") {
                self.ctx.borrow().error(pos, kError_InvalidPublic);
            }
        }

        if n.has_attribute("prototype") {
            self.ctx.borrow().error(pos, kError_PrototypeIsAnInvalidAttribute);
        }

        n.has_attribute("static")
            || n.has_attribute("public")
            || n.has_attribute("private")
            || n.has_attribute("protected")
            || n.has_attribute("internal")
            || n.has_attribute("native")
            || n.has_attribute("final")
            || n.has_attribute("override")
            || n.has_attribute("prototype")
    }

    fn which_attribute(&self, t: &Option<NodeRef>) -> String {
        let t = rb(t);
        if t.is_list() {
            let t1 = t.list().expect("ClassCastException").items[0].clone();
            if rb(&t1).is_member_expression() {
                let t2 = {
                    let m1 = rb(&t1);
                    let selector = &m1.as_member_expression().unwrap().selector;
                    let identifier = rb(selector).selector().expect("ClassCastException").get_identifier();
                    identifier
                };
                if let Some(t2) = t2 {
                    return t2.borrow().identifier().expect("ClassCastException").to_identifier_string();
                }
            }
        }

        for name in ["static", "public", "private", "protected", "internal", "native", "final", "override", "prototype"] {
            if t.has_attribute(name) {
                return name.to_string();
            }
        }

        "<unknown>".to_string()
    }

    fn parse_attribute_list(&mut self) -> Option<NodeRef> {
        let mut result = None;

        loop {
            let lt = self.lookahead();
            let mut t;
            let pos = self.position_of_mark();

            match lt {
                TRUE_TOKEN => {
                    self.shift();
                    t = Some(self.nf().literal_boolean_pos(true, pos));
                }

                FALSE_TOKEN => {
                    self.shift();
                    t = Some(self.nf().literal_boolean_pos(false, pos));
                }

                PRIVATE_TOKEN | PROTECTED_TOKEN | PUBLIC_TOKEN => {
                    self.shift();
                    let name = if lt == PRIVATE_TOKEN {
                        PRIVATE
                    } else if lt == PROTECTED_TOKEN {
                        PROTECTED
                    } else {
                        PUBLIC
                    };
                    t = Some(self.nf().identifier_intern_pos(name, false, pos));
                    if self.lookahead() == DOUBLECOLON_TOKEN {
                        self.shift();
                        let s = self.parse_identifier_string();
                        let p = self.position_of_mark();
                        t = Some(self.nf().qualified_identifier_pos(t, Some(&s), p));
                    }
                }

                LEFTBRACKET_TOKEN => {
                    t = Some(self.parse_array_literal());
                }

                IDENTIFIER_TOKEN => {
                    t = self.parse_simple_type_identifier();
                }

                _ => {
                    t = None;
                }
            }

            if t.is_none() {
                break;
            }

            self.check_attribute(&t);
            result = Some(self.nf().attribute_list(t, result));
        }

        result
    }

    fn parse_import_directive(&mut self, first: Option<NodeRef>) -> NodeRef {
        let mut third = None;
        let stmt_pos = self.position_of_mark();

        self.shift();
        let second = self.parse_package_name(true);
        if self.lookahead() == ASSIGN_TOKEN {
            third = Some(self.parse_package_name(true));
        }

        let ctx = self.ctx.clone();
        self.nf().import_directive_pos(first, Some(second), third, stmt_pos, &ctx)
    }

    fn parse_variable_definition(&mut self, first: Option<NodeRef>, mode: i32) -> NodeRef {
        let lt = self.lookahead();
        let mut kind = VAR_TOKEN;

        if lt == CONST_TOKEN {
            kind = CONST_TOKEN;
            self.shift();
        } else if lt == VAR_TOKEN {
            self.shift();
        }

        let t = self.parse_variable_binding_list(first.clone(), kind, mode);
        self.nf().variable_definition(first, kind, t)
    }

    fn parse_variable_binding_list(&mut self, attrs: Option<NodeRef>, kind: i32, mode: i32) -> Option<NodeRef> {
        let mut result = None;

        loop {
            let t = self.parse_variable_binding(attrs.clone(), kind, mode);

            result = Some(self.nf().list(result, Some(t)));

            if self.lookahead() != COMMA_TOKEN {
                break;
            }

            self.shift();
        }

        result
    }

    fn parse_variable_binding(&mut self, attrs: Option<NodeRef>, kind: i32, mode: i32) -> NodeRef {
        let first = self.parse_typed_identifier(mode);
        let second = self.parse_variable_initialization(mode);
        self.nf().variable_binding(attrs, kind, Some(first), second)
    }

    fn parse_variable_initialization(&mut self, mode: i32) -> Option<NodeRef> {
        let mut result = None;

        if self.lookahead() == ASSIGN_TOKEN {
            self.shift();
            let t = self.parse_assignment_expression(mode);

            if self.lookahead() == COMMA_TOKEN
                || self.lookahead_semicolon(mode)
                || (self.ctx.borrow().script_assist_parsing && self.lookahead() == IN_TOKEN)
            {
                result = t;
            }
        }

        result
    }

    fn parse_typed_identifier(&mut self, mode: i32) -> NodeRef {
        let first = self.parse_identifier();
        let mut second = None;
        let mut no_anno = true;

        if self.lookahead() == COLON_TOKEN {
            self.shift();
            no_anno = false;

            let lt = self.lookahead();

            if lt == MULT_TOKEN || lt == MULTASSIGN_TOKEN {
                if lt == MULT_TOKEN {
                    self.shift();
                } else {
                    self.change_lookahead(ASSIGN_TOKEN);
                }

                if self.ctx.borrow().script_assist_parsing {
                    second = Some(self.nf().identifier_intern(ASTERISK, false));
                }
            } else if !self.error_if_next_token_is_keyword_instead_of_type_expression() {
                second = self.parse_type_expression(mode);
            }
        }

        let result = self.nf().typed_identifier(Some(first), second);
        result.borrow_mut().as_typed_identifier_mut().unwrap().no_anno = no_anno;
        result
    }

    fn parse_function_definition(&mut self, first: Option<NodeRef>) -> NodeRef {
        self.shift();

        let second = self.parse_function_name();
        let identifier = second.borrow().as_function_name().unwrap().identifier.clone();
        let third = self.parse_function_common(identifier);
        let ctx = self.ctx.clone();
        self.nf().function_definition(&ctx, first, Some(second), Some(third))
    }

    fn parse_function_name(&mut self) -> NodeRef {
        let lt = self.lookahead();

        if lt == GET_TOKEN || lt == SET_TOKEN {
            self.shift();
            let (name, kind) = if lt == GET_TOKEN { (GET, GET_TOKEN) } else { (SET, SET_TOKEN) };
            if self.lookahead() == LEFTPAREN_TOKEN {
                let p = self.position_of_mark();
                let id = self.nf().identifier_intern_pos(name, false, p);
                self.nf().function_name(EMPTY_TOKEN, Some(id))
            } else {
                let id = self.parse_identifier();
                self.nf().function_name(kind, Some(id))
            }
        } else {
            let id = self.parse_identifier();
            self.nf().function_name(EMPTY_TOKEN, Some(id))
        }
    }

    fn parse_result_signature(&mut self, no_anno: &mut bool, void_anno: &mut bool) -> Option<NodeRef> {
        let mut result = None;

        *no_anno = true;
        *void_anno = false;

        if self.lookahead() == COLON_TOKEN {
            *no_anno = false;
            self.shift();
            let lt = self.lookahead();

            if lt == MULT_TOKEN {
                self.shift();
                if self.ctx.borrow().script_assist_parsing {
                    let p = self.position_of_mark();
                    result = Some(self.nf().identifier_intern_pos(ASTERISK, false, p));
                }
            } else if lt == MULTASSIGN_TOKEN {
                self.change_lookahead(ASSIGN_TOKEN);
            } else if lt == VOID_TOKEN {
                self.shift();
                *void_anno = true;
            } else {
                self.error_if_next_token_is_keyword_instead_of_type_expression();
                result = self.parse_type_expression(allowIn_mode);
            }
        }

        result
    }

    fn parse_function_signature(&mut self) -> NodeRef {
        let mut no_anno = false;
        let mut void_anno = false;

        self.r#match(LEFTPAREN_TOKEN);
        let first = self.parse_parameters();
        self.r#match(RIGHTPAREN_TOKEN);

        let second = self.parse_result_signature(&mut no_anno, &mut void_anno);

        let p = self.position_of_mark();
        let result = self.nf().function_signature_pos(first, second, p);
        {
            let mut r = result.borrow_mut();
            let sig = r.as_function_signature_mut().unwrap();
            sig.no_anno = no_anno;
            sig.void_anno = void_anno;
        }
        result
    }

    fn parse_constructor_signature(&mut self) -> NodeRef {
        let result;
        let mut no_anno = false;
        let mut void_anno = false;

        self.r#match(LEFTPAREN_TOKEN);
        let first = self.parse_parameters();
        self.r#match(RIGHTPAREN_TOKEN);

        let second = self.parse_constructor_initializer(&mut no_anno, &mut void_anno);

        let p = self.position_of_mark();
        if void_anno {
            result = self.nf().function_signature_pos(first, None, p);
            let mut r = result.borrow_mut();
            let sig = r.as_function_signature_mut().unwrap();
            sig.no_anno = false;
            sig.void_anno = true;
        } else {
            result = self.nf().constructor_signature(first, second, p);
            let mut r = result.borrow_mut();
            let sig = r.as_function_signature_mut().unwrap();
            sig.no_anno = true;
            sig.void_anno = false;
        }

        result
    }

    fn parse_constructor_initializer(&mut self, no_anno: &mut bool, void_anno: &mut bool) -> Option<NodeRef> {
        let mut result = None;

        *no_anno = true;
        *void_anno = false;

        if self.lookahead() == COLON_TOKEN {
            self.shift();
            if self.lookahead() == VOID_TOKEN {
                self.shift();
                *no_anno = false;
                *void_anno = true;
                return None;
            }
            result = self.parse_initializer_list();
            if self.lookahead() == SUPER_TOKEN {
                let s = self.parse_super_initializer();
                result = Some(self.nf().list(result, s));
            }
        }

        result
    }

    fn parse_super_initializer(&mut self) -> Option<NodeRef> {
        self.shift();

        let p = self.position_of_mark();
        let first = self.nf().super_expression(None, p);
        let n = self.parse_arguments(Some(first));

        if !is_kind(&n, |n| n.as_call_expression().is_some()) {
            self.ctx.borrow().internal_error("Internal error in parseSuperInitializer()");
            if n.is_some() {
                panic!("ClassCastException");
            }
        }

        Some(self.nf().super_statement(n))
    }

    fn parse_initializer_list(&mut self) -> Option<NodeRef> {
        let mut result = None;

        loop {
            if self.lookahead() == SUPER_TOKEN {
                break;
            }

            let t = self.parse_initializer();

            if t.is_some() {
                result = Some(self.nf().list(result, t));
            }

            if self.lookahead() != COMMA_TOKEN {
                break;
            }

            self.shift();
        }

        result
    }

    fn parse_initializer(&mut self) -> Option<NodeRef> {
        let first = self.parse_identifier();

        if self.lookahead() != ASSIGN_TOKEN {
            let pos = first.borrow().pos();
            self.ctx.borrow().error(pos, kError_Parser_DefinitionOrDirectiveExpected);
        }

        let second = self.parse_variable_initialization(allowIn_mode);

        Some(self.nf().assignment_expression(Some(first), ASSIGN_TOKEN, second))
    }

    fn parse_parameters(&mut self) -> Option<NodeRef> {
        let mut result = None;

        if self.lookahead() != RIGHTPAREN_TOKEN {
            result = self.parse_parameter_list();
        }

        result
    }

    fn parse_parameter_list(&mut self) -> Option<NodeRef> {
        let mut result = None;

        loop {
            if self.lookahead() == TRIPLEDOT_TOKEN {
                let rest = self.parse_rest_parameter();
                result = Some(self.nf().parameter_list(result, rest));
                break;
            }

            let t = self.parse_parameter();
            if self.lookahead() == ASSIGN_TOKEN {
                self.shift();
                let init = self.parse_non_assignment_expression(allowIn_mode);
                t.borrow_mut().parameter_mut().unwrap().init = init;
            }
            result = Some(self.nf().parameter_list(result, Some(t)));

            if self.lookahead() != COMMA_TOKEN {
                break;
            }

            self.shift();
        }

        result
    }

    fn parse_rest_parameter(&mut self) -> Option<NodeRef> {
        let mut first = None;

        self.shift();

        let lt = self.lookahead();
        if lt == CONST_TOKEN || lt == IDENTIFIER_TOKEN || lt == GET_TOKEN || lt == SET_TOKEN {
            first = Some(self.parse_parameter());
        }

        let p = self.position_of_mark();
        self.nf().rest_parameter_pos(first, p)
    }

    fn parse_parameter(&mut self) -> NodeRef {
        let mut third = None;
        let mut no_anno = true;

        let mut first = VAR_TOKEN;

        if HAS_CONSTPARAMETERS && self.lookahead() == CONST_TOKEN {
            first = CONST_TOKEN;
            self.shift();
        }

        let second = self.parse_identifier();

        if self.lookahead() == COLON_TOKEN {
            no_anno = false;
            self.shift();
            if self.lookahead() == MULT_TOKEN {
                self.shift();
                second.borrow_mut().identifier_mut().unwrap().set_orig_type_token(MULT_TOKEN);
            } else if self.lookahead() == MULTASSIGN_TOKEN {
                self.change_lookahead(ASSIGN_TOKEN);
                second.borrow_mut().identifier_mut().unwrap().set_orig_type_token(MULTASSIGN_TOKEN);
            } else if !self.error_if_next_token_is_keyword_instead_of_type_expression() {
                third = self.parse_type_expression(allowIn_mode);
            }
        }

        let result = self.nf().parameter(first, Some(second), third);
        result.borrow_mut().parameter_mut().unwrap().no_anno = no_anno;
        result
    }

    fn error_if_next_token_is_keyword_instead_of_type_expression(&mut self) -> bool {
        let mut lt = self.lookahead();

        if lt == VOID_TOKEN {
            self.error1(ParseError::Syntax, kError_Parser_keywordInsteadOfTypeExpr, get_token_class_name(lt));
            self.r#match(VOID_TOKEN);
            true
        } else if lt != IDENTIFIER_TOKEN && Self::in_xml_token_set(lt) {
            self.error1(ParseError::Syntax, kError_Parser_keywordInsteadOfTypeExpr, get_token_class_name(lt));
            lt = self.lookahead();
            if Self::in_xml_token_set(lt) {
                self.shift();
            } else {
                self.r#match(lt);
            }
            true
        } else if self.ctx.borrow().dialect(8) && lt == IDENTIFIER_TOKEN && self.current_token_text() == "Object" {
            self.error(kError_ColonObjectAnnoOutOfService);
            true
        } else {
            false
        }
    }

    fn parse_class_definition(&mut self, attrs: Option<NodeRef>, _mode: i32) -> NodeRef {
        self.shift();

        let last = *self.block_kind_stack.last().expect("ArrayIndexOutOfBoundsException");
        if last != PACKAGE_TOKEN && last != EMPTY_TOKEN {
            self.error(kError_InvalidClassNesting);
        }

        self.block_kind_stack.push(CLASS_TOKEN);

        let first = self.parse_class_name();
        let (pkgname, ident, non_nullable) = {
            let f = first.borrow();
            let c = f.as_class_name().unwrap();
            (c.pkgname.clone(), c.ident.clone(), c.non_nullable)
        };

        let temp_class_name = self.current_class_name.take();
        self.current_class_name = Some(rb(&ident).identifier().expect("ClassCastException").name.clone());

        let ctx = self.ctx.clone();
        if pkgname.is_some() {
            self.nf().start_package(&ctx, None, pkgname.clone());
        }

        self.nf().start_class_defs();
        let second = self.parse_inheritance();
        let mut third = self.parse_block();

        if third.is_none() {
            third = Some(self.nf().statement_list_concat(None, None));
        }

        let result = self.nf().class_definition_non_nullable(&ctx, attrs, ident, second, third, non_nullable);
        self.block_kind_stack.pop();
        self.current_class_name = temp_class_name;

        if pkgname.is_some() {
            self.nf().finish_package(&ctx, None);
        }

        result
    }

    fn parse_class_name(&mut self) -> NodeRef {
        let result;
        let mut first = self.parse_identifier();

        if HAS_COMPOUNDCLASSNAMES {
            let mut list = None;

            while self.lookahead() == DOT_TOKEN {
                self.shift();
                list = Some(self.nf().package_identifiers(list, Some(first), true));
                first = self.parse_identifier();
            }

            let pkgname = if list.is_some() { Some(self.nf().package_name(list)) } else { None };
            result = self.nf().class_name(pkgname, Some(first));
        } else {
            result = self.nf().class_name(None, Some(first));
        }

        if self.ctx.borrow().statics.borrow().es4_nullability {
            if self.lookahead() == NOT_TOKEN {
                self.shift();
                result.borrow_mut().as_class_name_mut().unwrap().non_nullable = true;
            } else if self.lookahead() == QUESTIONMARK_TOKEN {
                self.shift();
            }
        }

        result
    }

    fn parse_inheritance(&mut self) -> Option<NodeRef> {
        let mut result = None;
        let mut first = None;
        let mut second = None;

        if self.lookahead() == EXTENDS_TOKEN {
            self.shift();
            first = self.parse_typename();
        }

        if self.lookahead() == IMPLEMENTS_TOKEN {
            self.shift();
            second = self.parse_typename_list();
        }

        if first.is_some() || second.is_some() {
            result = Some(self.nf().inheritance(first, second));
        }

        result
    }

    fn parse_type_expression_list(&mut self) -> Option<NodeRef> {
        let mut result = None;

        loop {
            let t = self.parse_type_expression(allowIn_mode);

            if t.is_some() {
                result = Some(self.nf().list(result, t));
            }

            if self.lookahead() != COMMA_TOKEN {
                break;
            }

            self.shift();
        }

        result
    }

    fn parse_interface_definition(&mut self, attrs: Option<NodeRef>, _mode: i32) -> NodeRef {
        self.shift();

        self.block_kind_stack.push(INTERFACE_TOKEN);

        let first = self.parse_class_name();
        let (pkgname, ident) = {
            let f = first.borrow();
            let c = f.as_class_name().unwrap();
            (c.pkgname.clone(), c.ident.clone())
        };

        let ctx = self.ctx.clone();
        if pkgname.is_some() {
            self.nf().start_package(&ctx, None, pkgname.clone());
        }

        let mut second = None;
        if self.lookahead() == EXTENDS_TOKEN {
            self.shift();
            second = self.parse_typename_list();
        }

        let third = self.parse_block();
        let result = self.nf().interface_definition(&ctx, attrs, ident, second, third);

        self.block_kind_stack.pop();

        if pkgname.is_some() {
            self.nf().finish_package(&ctx, None);
        }

        result
    }

    fn parse_namespace_definition(&mut self, first: Option<NodeRef>) -> NodeRef {
        self.shift();

        let second = self.parse_identifier();
        let result;

        let is_config = first.as_ref().map_or(false, |f| {
            let f = f.borrow();
            f.as_attribute_list().expect("ClassCastException").items.len() == 1 && f.has_attribute("config")
        });

        if is_config {
            result = self.nf().config_namespace_definition(None, Some(second), -1);

            let name = {
                let r = result.borrow();
                let name = &r.namespace_definition().unwrap().name;
                let name = rb(name).identifier().expect("ClassCastException").name.clone();
                name
            };
            self.config_namespaces.borrow_mut().last_mut().expect("IndexOutOfBoundsException").insert(name);
        } else {
            let mut third = None;

            if self.lookahead() == ASSIGN_TOKEN {
                self.shift();
                if self.ctx.borrow().statics.borrow().es4_nullability {
                    if self.lookahead() == STRINGLITERAL_TOKEN {
                        let mut is_single_quoted = false;
                        self.shift();
                        let enclosed_text = self.current_string_token_text(&mut is_single_quoted);
                        let p = self.position_of_mark();
                        third = Some(self.nf().literal_string_quoted(&enclosed_text, p, is_single_quoted));
                    } else {
                        third = self.parse_simple_type_identifier();
                    }
                } else {
                    third = self.parse_assignment_expression(allowIn_mode);
                }
            }
            result = self.nf().namespace_definition(first, Some(second), third);
        }

        result
    }

    pub fn generate_as3_use_directive(ctx: &ContextRef) -> NodeRef {
        let node_factory = Context::get_node_factory(ctx);
        let as3_identifier = node_factory.borrow_mut().identifier_intern(AS3, false);

        let reference_value = {
            let cx = ctx.borrow();
            let as3_namespace = cx.as3_namespace();
            ReferenceValue::new(&cx, None, AS3, Some(&as3_namespace))
        };
        reference_value.set_is_attribute_identifier(false);
        as3_identifier.borrow_mut().identifier_mut().unwrap().ref_ = Some(reference_value);
        let mut nf = node_factory.borrow_mut();
        let get = nf.get_expression(Some(as3_identifier));
        let member = nf.member_expression(None, Some(get));
        nf.use_directive(None, Some(member))
    }

    fn vector_import_directive(&mut self) -> NodeRef {
        let ctx = self.ctx.clone();
        let mut nf = self.nf();
        let id = nf.identifier_intern(__AS3__, false);
        let mut pin = nf.package_identifiers(None, Some(id), true);
        let id = nf.identifier_intern(VEC, false);
        pin = nf.package_identifiers(Some(pin), Some(id), true);
        let id = nf.identifier_intern(VECTOR, false);
        pin = nf.package_identifiers(Some(pin), Some(id), true);
        let name = nf.package_name(Some(pin));
        nf.import_directive(None, Some(name), None, &ctx)
    }

    fn use_namespace_directive(&mut self, name: &str) -> NodeRef {
        let mut nf = self.nf();
        let id = nf.identifier(name);
        let get = nf.get_expression(Some(id));
        let member = nf.member_expression(None, Some(get));
        nf.use_directive(None, Some(member))
    }

    fn statements_of(node: &Option<NodeRef>) -> Option<NodeRef> {
        let n = rb(node);
        n.as_package_definition().map(|p| p.statements.clone()).unwrap_or_else(|| n.program().unwrap().statements.clone())
    }

    fn parse_package_definition(&mut self) -> Option<NodeRef> {
        if self.within_package {
            self.error(kError_NestedPackage);
        }

        self.within_package = true;

        self.nf().dxns = None;

        self.block_kind_stack.push(PACKAGE_TOKEN);

        self.shift();

        {
            let mut config_namespaces = self.config_namespaces.borrow_mut();
            let conf_ns = config_namespaces.last().expect("IndexOutOfBoundsException").clone();
            config_namespaces.push(conf_ns);
        }

        let mut first = None;
        let has_packagename = self.lookahead() != LEFTBRACE_TOKEN;

        if has_packagename {
            first = Some(self.parse_package_name(false));
        }

        let ctx = self.ctx.clone();
        self.nf().start_package(&ctx, None, first);

        let as3_use_directive = Self::generate_as3_use_directive(&ctx);
        let mut use_directives = None;

        let use_namespaces = ctx.borrow().statics.borrow().use_namespaces.clone();
        if !use_namespaces.is_empty() {
            let mut list = Vec::new();
            for u in &use_namespaces {
                list.push(self.use_namespace_directive(u));
            }
            use_directives = Some(list);
        }

        let es4_vectors = ctx.borrow().statics.borrow().es4_vectors;
        let mut import_directive = None;
        if es4_vectors {
            import_directive = Some(self.vector_import_directive());
        }

        let block = self.parse_block();
        let result = self.nf().finish_package(&ctx, block);

        let insert = |result: &Option<NodeRef>, node: Option<NodeRef>| {
            let statements = Self::statements_of(result);
            rbm(&statements).as_statement_list_mut().expect("ClassCastException").items.insert(1, node);
        };

        if ctx.borrow().dialect(10) && result.is_some() {
            insert(&result, Some(as3_use_directive));
        }

        if !has_packagename && es4_vectors && result.is_some() {
            insert(&result, import_directive.clone());
        }

        if let (Some(use_directives), Some(_)) = (&use_directives, &result) {
            for u in use_directives {
                insert(&result, Some(u.clone()));
            }
        }

        if has_packagename && es4_vectors && result.is_some() {
            insert(&result, import_directive);
        }

        self.block_kind_stack.pop();
        self.config_namespaces.borrow_mut().pop();

        self.within_package = false;

        result
    }

    pub fn parse_config_values(&mut self) -> Option<NodeRef> {
        let mut configs = None;
        let config_code = self.ctx.borrow().get_config_var_code();

        if let Some(config_code) = config_code {
            let orig = self.scanner.take();
            let orig_input = orig.as_ref().expect("NullPointerException").input.clone();
            let orig_next_token = self.next_token;

            let scanner = Scanner::new(self.ctx.clone(), &config_code, Some(String::new()));
            scanner.input().report_pos = false;
            self.set_scanner(Some(scanner));

            configs = self.parse_directives(None, None);

            self.set_scanner(orig);
            self.sc().input = orig_input.clone();
            *self.this_ref.input.borrow_mut() = orig_input.clone();
            self.ctx.borrow_mut().input = orig_input;
            self.next_token = orig_next_token;
        }
        configs
    }

    fn parse_package_name(&mut self, is_definition: bool) -> NodeRef {
        if self.lookahead() == STRINGLITERAL_TOKEN {
            let mut is_single_quoted = false;
            self.shift();
            let enclosed_text = self.current_string_token_text(&mut is_single_quoted);
            let p = self.position_of_mark();
            let first = self.nf().literal_string_quoted(&enclosed_text, p, is_single_quoted);
            self.nf().package_name_url(Some(first))
        } else {
            let id = self.parse_identifier();
            let mut first = self.nf().package_identifiers(None, Some(id), is_definition);

            while self.lookahead() == DOT_TOKEN {
                self.shift();
                let id = self.parse_property_identifier();
                first = self.nf().package_identifiers(Some(first), Some(id), is_definition);
            }
            self.nf().package_name(Some(first))
        }
    }

    /// Clears unused buffers before returning.
    pub fn parse_program(&mut self) -> Option<NodeRef> {
        let mut first = None;
        let mut second = None;
        let mut configs = None;

        if !self.parsing_include {
            {
                let mut config_namespaces = self.config_namespaces.borrow_mut();
                config_namespaces.push(HashSet::new());
                config_namespaces.last_mut().unwrap().insert(CONFIG.to_string());
            }
            configs = self.parse_config_values();
        }

        while self.lookahead() == PACKAGE_TOKEN || self.lookahead() == DOCCOMMENT_TOKEN {
            let mut meta = None;
            if self.lookahead() == DOCCOMMENT_TOKEN
                || self.lookahead() == LEFTBRACKET_TOKEN
                || self.lookahead() == XMLLITERAL_TOKEN
            {
                meta = Some(self.parse_meta_data());
                second = Some(self.nf().statement_list(second, meta.clone()));
            }

            if self.lookahead() == PACKAGE_TOKEN {
                let pkgdef = self.parse_package_definition();
                first = Some(self.nf().statement_list(first, pkgdef.clone()));
                if let Some(meta) = &meta {
                    meta.borrow_mut().meta_data_mut().unwrap().def = pkgdef.clone();
                    rbm(&pkgdef).definition_mut().unwrap().add_meta_data_node(Some(meta.clone()));
                }

                if pkgdef.is_some() {
                    let statements = Self::statements_of(&pkgdef);
                    second = Some(self.nf().statement_list_concat(second, statements));
                }
            }
        }

        second = self.parse_directives(None, second);

        let ctx = self.ctx.clone();
        if ctx.borrow().dialect(10) && !self.parsing_include && second.is_some() {
            let udn = Self::generate_as3_use_directive(&ctx);
            rbm(&second).as_statement_list_mut().unwrap().items.insert(0, Some(udn));
        }

        let use_namespaces = ctx.borrow().statics.borrow().use_namespaces.clone();
        if !use_namespaces.is_empty() && !self.parsing_include && second.is_some() {
            for use_name in &use_namespaces {
                let udn2 = self.use_namespace_directive(use_name);
                rbm(&second).as_statement_list_mut().unwrap().items.insert(0, Some(udn2));
            }
        }

        if ctx.borrow().statics.borrow().es4_vectors && !self.parsing_include && second.is_some() {
            let idn = self.vector_import_directive();
            rbm(&second).as_statement_list_mut().unwrap().items.insert(0, Some(idn));
        }

        let p = self.position_of_mark();
        let result = Some(self.nf().program_pos(&ctx, second, p));
        self.r#match(EOS_TOKEN);

        if ctx.borrow().script_assist_parsing {
            let comments = self.comments.clone().expect("NullPointerException");
            ctx.borrow_mut().comments.extend(comments);
        }

        self.clear_unused_buffers();

        if !self.parsing_include {
            if result.is_some() {
                let statements = Self::statements_of(&result);
                if let Some(configs) = &configs {
                    let items = configs.borrow().as_statement_list().unwrap().items.clone();
                    rbm(&statements).as_statement_list_mut().unwrap().items.splice(0..0, items);
                }
                let configdef = {
                    let mut nf = self.nf();
                    let id = nf.identifier_intern(CONFIG, false);
                    nf.config_namespace_definition(None, Some(id), -1)
                };
                rbm(&statements).as_statement_list_mut().unwrap().items.insert(0, Some(configdef));
            }
            self.config_namespaces.borrow_mut().pop();
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asc::parser::evaluator::evaluate;
    use crate::asc::parser::printer::NodePrinter;
    use crate::asc::util::context::{CompilerHandler, ContextStatics};

    struct Errors(Rc<RefCell<String>>);

    impl CompilerHandler for Errors {
        fn error(&mut self, _filename: &str, ln: i32, col: i32, msg: &str, source: &str, code: i32) {
            self.0.borrow_mut().push_str(&format!("{} {} {} {} | {}\n", code, ln, col, msg, source));
        }
    }

    /// The tree as `NodePrinter` prints it with positions, and the reported errors; expectations come from Java ASC.
    fn parse(src: &str) -> (String, String) {
        let statics = Rc::new(RefCell::new(ContextStatics::new()));
        let cx = Rc::new(RefCell::new(Context::new(statics)));
        let errors = Rc::new(RefCell::new(String::new()));
        cx.borrow_mut().set_handler(Rc::new(RefCell::new(Errors(errors.clone()))));
        let program = Parser::new_stream(&cx, src.as_bytes(), "t.as").parse_program().unwrap();
        let mut printer = NodePrinter::new_out(Vec::new());
        printer.show_positions = true;
        evaluate(&program, &cx, &mut printer);
        let tree = String::from_utf8(printer.into_inner()).unwrap();
        let errors = errors.borrow().clone();
        (tree, errors)
    }

    const PROLOGUE: &str = "
  statementlist @0
    config namespace  @0
      identifier CONFIG @0
    import  @0
      packagename @0
        packageidentifiers @0
          identifier __AS3__@0
          identifier vec@0
          identifier Vector@";

    #[test]
    fn operator_precedence_and_typed_binding() {
        let (tree, errors) = parse("var x:int = 1 + 2 * 3;\n");
        let expected = format!(
            "@23\nprogram @4{PROLOGUE}4
    var  @4
      list @4
        variablebinding @4
          typedidentifier @4
            qualifiedidentifier x@6
            typeexpr @6
              member @6
                get lexical @6
                  identifier int@12
          binary plus @12
            literalnumber:1@16
            binary mult @16
              literalnumber:2@20
              literalnumber:3
"
        );
        assert_eq!(tree, expected);
        assert_eq!(errors, "");
    }

    #[test]
    fn syntax_errors_recover_like_java() {
        let (tree, errors) = parse("var x = ;\nfoo(\n");
        assert_eq!(
            errors,
            "1084 1 9 [Compiler] Error #1084: Syntax error: expecting identifier before semicolon. | var x = ;
1084 3 1 [Compiler] Error #1084: Syntax error: expecting identifier before end of program. | \n\
1084 3 1 [Compiler] Error #1084: Syntax error: expecting rightparen before end of program. | \n"
        );
        let expected = format!(
            "@15\nprogram @4{PROLOGUE}4
    var  @4
      list @4
        variablebinding @4
          typedidentifier @4
            qualifiedidentifier x@8
          member @8
            get lexical @8
              identifier expected property identifier@10
    expression @10
      list @10
        member @10
          call lexical @10
            identifier foo@15
            argumentlist @15
              member @15
                get lexical @15
                  identifier expected property identifier
"
        );
        assert_eq!(tree, expected);
    }

    #[test]
    fn xml_literal_concatenation() {
        let (tree, errors) = parse("var x = <a b=\"1\">{y}</a>;\n");
        let expected = format!(
            "@26\nprogram @4{PROLOGUE}4
    var  @4
      list @4
        variablebinding @4
          typedidentifier @4
            qualifiedidentifier x@23
          literalxml @8
            list @0
              binary plus @0
                binary plus @0
                  literalstring:<a b=\"1\">@0
                  invoke dot [[ToXMLString]]@18
                    argumentlist @18
                      list @18
                        member @18
                          get lexical @18
                            identifier y@0
                literalstring:</a>
"
        );
        assert_eq!(tree, expected);
        assert_eq!(errors, "");
    }

    #[test]
    fn config_block_and_labeled_loop() {
        let (tree, errors) = parse("CONFIG::debug {\n  outer: while (true) { break outer; }\n}\n");
        let expected = format!(
            "@57\nprogram @18{PROLOGUE}18
    statementlist @18
      labeled @18
        list @18
          member @18
            get lexical @18
              identifier outer @32
        while @32
          list @32
            literalboolean 1 @46
          statementlist @46
            break @46
              identifier outer
"
        );
        assert_eq!(tree, expected);
        assert_eq!(errors, "");
    }
}
