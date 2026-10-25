//! Port of the JavaCC/JJTree-generated `Parser` and `JJTParserState` (Flex's `Parser.java`, which differs from `Parser.jjt`: an
//! unknown `#word` always parses as a line directive), with the parse-time `jjtOpen`/`jjtClose` hooks of the node classes.

use super::runtime::RuntimeInstance;
use super::token_manager::*;
use crate::asc::util::jstr;

pub const JJTPROCESS: i32 = 0;
pub const JJTESCAPEDDIRECTIVE: i32 = 2;
pub const JJTESCAPE: i32 = 3;
pub const JJTCOMMENT: i32 = 4;
pub const JJTNUMBERLITERAL: i32 = 5;
pub const JJTSTRINGLITERAL: i32 = 6;
pub const JJTIDENTIFIER: i32 = 7;
pub const JJTWORD: i32 = 8;
pub const JJTDIRECTIVE: i32 = 9;
pub const JJTBLOCK: i32 = 10;
pub const JJTOBJECTARRAY: i32 = 11;
pub const JJTINTEGERRANGE: i32 = 12;
pub const JJTMETHOD: i32 = 13;
pub const JJTREFERENCE: i32 = 14;
pub const JJTTRUE: i32 = 15;
pub const JJTFALSE: i32 = 16;
pub const JJTTEXT: i32 = 17;
pub const JJTIFSTATEMENT: i32 = 18;
pub const JJTELSESTATEMENT: i32 = 19;
pub const JJTELSEIFSTATEMENT: i32 = 20;
pub const JJTSETDIRECTIVE: i32 = 21;
pub const JJTEXPRESSION: i32 = 22;
pub const JJTORNODE: i32 = 24;
pub const JJTANDNODE: i32 = 25;
pub const JJTEQNODE: i32 = 26;
pub const JJTNENODE: i32 = 27;
pub const JJTLTNODE: i32 = 28;
pub const JJTGTNODE: i32 = 29;
pub const JJTLENODE: i32 = 30;
pub const JJTGENODE: i32 = 31;
pub const JJTADDNODE: i32 = 32;
pub const JJTSUBTRACTNODE: i32 = 33;
pub const JJTMULNODE: i32 = 34;
pub const JJTDIVNODE: i32 = 35;
pub const JJTMODNODE: i32 = 36;
pub const JJTNOTNODE: i32 = 37;

pub const JJT_NODE_NAME: [&str; 38] = [
    "process",
    "void",
    "EscapedDirective",
    "Escape",
    "Comment",
    "NumberLiteral",
    "StringLiteral",
    "Identifier",
    "Word",
    "Directive",
    "Block",
    "ObjectArray",
    "IntegerRange",
    "Method",
    "Reference",
    "True",
    "False",
    "Text",
    "IfStatement",
    "ElseStatement",
    "ElseIfStatement",
    "SetDirective",
    "Expression",
    "Assignment",
    "OrNode",
    "AndNode",
    "EQNode",
    "NENode",
    "LTNode",
    "GTNode",
    "LENode",
    "GENode",
    "AddNode",
    "SubtractNode",
    "MulNode",
    "DivNode",
    "ModNode",
    "NotNode",
];

/// `Directive.BLOCK` / `Directive.LINE`.
pub const BLOCK: i32 = 1;
pub const LINE: i32 = 2;

/// ASTReference's parse-time state (`jjtClose` and `getRoot`).
#[derive(Clone, Debug, Default)]
pub struct RefInfo {
    pub reference_type: i32,
    pub null_string: String,
    pub root_string: String,
    pub escaped: bool,
    pub computable_reference: bool,
    pub esc_prefix: String,
    pub more_prefix: String,
    pub identifier: Option<String>,
    pub literal: Option<String>,
    pub num_children: i32,
}

#[derive(Clone, Debug)]
pub enum PData {
    None,
    Text(String),
    Comment(String),
    Escape(String),
    Identifier(String),
    Method(String),
    NumberLiteral(i32),
    StringLiteral { image: String, interpolate: bool },
    Directive(String),
    Reference(RefInfo),
    SetDirective(String),
}

#[derive(Clone, Debug)]
pub struct PNode {
    pub id: i32,
    pub children: Vec<usize>,
    pub first: Option<usize>,
    pub last: Option<usize>,
    pub first_token_image: Option<String>,
    pub begin_line: i32,
    pub begin_column: i32,
    pub data: PData,
}

/// `ParseException` (and the lexical and `#macro` errors the parser rethrows as one).
#[derive(Clone, Debug)]
pub struct ParseException(pub String);

type PResult<T> = Result<T, ParseException>;

pub struct ParseResult {
    pub nodes: Vec<PNode>,
    pub tokens: Vec<Token>,
    pub root: usize,
}

const STATEMENT_START: [i32; 19] = [
    LPAREN,
    RPAREN,
    ESCAPE_DIRECTIVE,
    SET_DIRECTIVE,
    DOUBLE_ESCAPE,
    ESCAPE,
    TEXT,
    SINGLE_LINE_COMMENT,
    FORMAL_COMMENT,
    MULTI_LINE_COMMENT,
    STRING_LITERAL,
    IF_DIRECTIVE,
    STOP_DIRECTIVE,
    NUMBER_LITERAL,
    WORD,
    IDENTIFIER,
    DOT,
    LCURLY,
    RCURLY,
];

const PARAMETER_START: [i32; 8] =
    [LBRACKET, WHITESPACE, STRING_LITERAL, TRUE, FALSE, NUMBER_LITERAL, IDENTIFIER, LCURLY];

pub struct Parser<'a> {
    token_source: ParserTokenManager,
    token: usize,
    jj_ntk: i32,
    jj_scanpos: usize,
    jj_lastpos: usize,
    jj_la: i32,
    lex_error: Option<String>,
    nodes: Vec<PNode>,
    stack: Vec<usize>,
    marks: Vec<usize>,
    mk: usize,
    current_template_name: String,
    rsvc: &'a RuntimeInstance,
}

/// `Parser.parse(reader, templateName)`, with `rsvc` the parse-time runtime (directives, and Velocimacros seen so far).
pub fn parse(input: &str, template_name: &str, rsvc: &RuntimeInstance) -> PResult<ParseResult> {
    let units = jstr::to_utf16(input);
    let mut p = Parser {
        token_source: ParserTokenManager::new(&units),
        token: 0,
        jj_ntk: -1,
        jj_scanpos: 0,
        jj_lastpos: 0,
        jj_la: 0,
        lex_error: None,
        nodes: Vec::new(),
        stack: Vec::new(),
        marks: Vec::new(),
        mk: 0,
        current_template_name: template_name.to_string(),
        rsvc,
    };
    p.token_source.clear_state_vars();
    match p.process() {
        Ok(root) => Ok(ParseResult { nodes: p.nodes, tokens: p.token_source.tokens, root }),
        Err(e) => match p.lex_error {
            Some(l) => Err(ParseException(format!("Lexical error: {l}"))),
            None => Err(e),
        },
    }
}

impl Parser<'_> {
    fn next_from_source(&mut self) -> PResult<usize> {
        match self.token_source.get_next_token() {
            Ok(t) => Ok(t),
            Err(TokenMgrError(m)) => {
                self.lex_error = Some(format!("org.apache.flex.forks.velocity.runtime.parser.TokenMgrError: {m}"));
                Err(ParseException(m))
            }
        }
    }

    fn tok(&self, t: usize) -> &Token {
        &self.token_source.tokens[t]
    }

    fn is_directive(&self, name: &str) -> bool {
        self.rsvc.directive_type(name).is_some()
    }

    // ---- JJTree node scopes

    fn new_node(&mut self, id: i32) -> usize {
        self.nodes.push(PNode {
            id,
            children: Vec::new(),
            first: None,
            last: None,
            first_token_image: None,
            begin_line: 0,
            begin_column: 0,
            data: PData::None,
        });
        self.nodes.len() - 1
    }

    fn open_node_scope(&mut self, n: usize) -> PResult<()> {
        self.marks.push(self.mk);
        self.mk = self.stack.len();
        self.jjt_open(n)
    }

    fn pop_node(&mut self) -> usize {
        if self.stack.len() - 1 < self.mk {
            self.mk = self.marks.pop().unwrap();
        }
        self.stack.pop().unwrap()
    }

    fn close_node_scope_num(&mut self, n: usize, num: usize) -> PResult<()> {
        self.mk = self.marks.pop().unwrap();
        let mut children = Vec::with_capacity(num);
        for _ in 0..num {
            children.push(self.pop_node());
        }
        children.reverse();
        self.nodes[n].children = children;
        self.jjt_close(n)?;
        self.stack.push(n);
        Ok(())
    }

    fn close_node_scope(&mut self, n: usize) -> PResult<()> {
        let a = self.stack.len() - self.mk;
        self.close_node_scope_num(n, a)
    }

    // ---- node hooks

    fn simple_open(&mut self, n: usize) -> PResult<()> {
        let first = self.get_token(1)?;
        let t = self.tok(first).clone();
        let node = &mut self.nodes[n];
        node.first = Some(first);
        node.first_token_image = Some(t.image);
        node.begin_line = t.begin_line;
        node.begin_column = t.begin_column;
        Ok(())
    }

    fn jjt_open(&mut self, n: usize) -> PResult<()> {
        match self.nodes[n].id {
            JJTTEXT => {
                let t = self.get_token(1)?;
                self.nodes[n].data = PData::Text(token_literal(&self.token_source.tokens, t));
            }
            JJTCOMMENT => {
                let t = self.get_token(1)?;
                let comment = self.tok(t).image.clone();
                let loc1 = jstr::index_of(&comment, "##", 0);
                let loc2 = jstr::index_of(&comment, "#*", 0);
                let c = if loc1 == -1 && loc2 == -1 {
                    String::new()
                } else {
                    jstr::substring(&comment, 0, if loc1 == -1 { loc2 } else { loc1 })
                };
                self.nodes[n].data = PData::Comment(c);
            }
            JJTIDENTIFIER => {
                let t = self.get_token(1)?;
                self.nodes[n].data = PData::Identifier(self.tok(t).image.clone());
            }
            JJTMETHOD => {
                let t = self.get_token(1)?;
                self.nodes[n].data = PData::Method(self.tok(t).image.clone());
            }
            JJTNUMBERLITERAL => {
                let t = self.get_token(1)?;
                let image = self.tok(t).image.clone();
                let v = java_parse_int(&image).ok_or_else(|| {
                    ParseException(format!("java.lang.NumberFormatException: For input string: \"{image}\""))
                })?;
                self.nodes[n].data = PData::NumberLiteral(v);
            }
            JJTSTRINGLITERAL => {
                self.simple_open(n)?;
                let t = self.tok(self.nodes[n].first.unwrap()).image.clone();
                let image = jstr::substring(&t, 1, jstr::utf16_len(&t) - 1);
                let interpolate = t.starts_with('"') && (image.contains('$') || image.contains('#'));
                self.nodes[n].data = PData::StringLiteral { image, interpolate };
            }
            _ => self.simple_open(n)?,
        }
        Ok(())
    }

    fn jjt_close(&mut self, n: usize) -> PResult<()> {
        match self.nodes[n].id {
            JJTDIRECTIVE => {}
            JJTSETDIRECTIVE => {
                let left = self.nodes[n].children[0];
                let image = self.nodes[left].first_token_image.clone().unwrap_or_default();
                self.nodes[n].data = PData::SetDirective(jstr::substring_from(&image, 1));
            }
            JJTREFERENCE => {
                self.nodes[n].last = Some(self.get_token(0)?);
                let mut info =
                    RefInfo { computable_reference: true, identifier: Some(String::new()), ..RefInfo::default() };
                info.root_string = self.get_root(n, &mut info);
                info.num_children = self.nodes[n].children.len() as i32;
                info.literal = simple_literal(&self.nodes[n], &self.token_source.tokens);
                if info.num_children > 0 {
                    let last = *self.nodes[n].children.last().unwrap();
                    info.identifier = self.nodes[last].first_token_image.clone();
                }
                self.nodes[n].data = PData::Reference(info);
            }
            _ => self.nodes[n].last = Some(self.get_token(0)?),
        }
        Ok(())
    }

    fn get_root(&mut self, n: usize, info: &mut RefInfo) -> String {
        let t = self.nodes[n].first.unwrap();
        let image = self.tok(t).image.clone();
        let slashbang = jstr::index_of(&image, "\\!", 0);
        if slashbang != -1 {
            let len = jstr::utf16_len(&image);
            let mut i = jstr::index_of_unit(&image, b'$' as u16, 0);
            if i == -1 {
                self.rsvc.error("ASTReference.getRoot() : internal error : no $ found for slashbang.");
                info.computable_reference = false;
                info.null_string = image.clone();
                return image;
            }
            while i < len && jstr::char_at(&image, i) != b'\\' as u16 {
                i += 1;
            }
            let start = i;
            let mut count = 0;
            while i < len && {
                let c = jstr::char_at(&image, i);
                i += 1;
                c == b'\\' as u16
            } {
                count += 1;
            }
            let mut ns = jstr::substring(&image, 0, start);
            ns.push_str(&jstr::substring(&image, start, start + count - 1));
            ns.push_str(&jstr::substring_from(&image, start + count));
            info.computable_reference = false;
            info.null_string = ns.clone();
            return ns;
        }
        info.escaped = false;
        let mut image = image;
        if image.starts_with('\\') {
            let len = jstr::utf16_len(&image);
            let mut i = 0;
            while i < len && jstr::char_at(&image, i) == b'\\' as u16 {
                i += 1;
            }
            if i % 2 != 0 {
                info.escaped = true;
            }
            if i > 0 {
                info.esc_prefix = jstr::substring(&image, 0, i / 2);
            }
            image = jstr::substring_from(&image, i);
        }
        let loc1 = jstr::last_index_of_unit(&image, b'$' as u16);
        if loc1 > 0 {
            info.more_prefix.push_str(&jstr::substring(&image, 0, loc1));
            image = jstr::substring_from(&image, loc1);
        }
        self.token_source.tokens[t].image = image.clone();
        info.null_string = simple_literal(&self.nodes[n], &self.token_source.tokens).unwrap_or_default();
        let next_image = || self.tok(self.tok(t).next.unwrap()).image.clone();
        if image.starts_with("$!") {
            info.reference_type = QUIET_REFERENCE;
            if !info.escaped {
                info.null_string = String::new();
            }
            if image.starts_with("$!{") {
                next_image()
            } else {
                jstr::substring_from(&image, 2)
            }
        } else if image == "${" {
            info.reference_type = FORMAL_REFERENCE;
            next_image()
        } else if image.starts_with('$') {
            info.reference_type = NORMAL_REFERENCE;
            jstr::substring_from(&image, 1)
        } else {
            info.reference_type = RUNT;
            image
        }
    }

    fn escaped_directive(&self, str_image: &str) -> String {
        let i_last = jstr::last_index_of(str_image, "\\");
        let str_directive = jstr::substring_from(str_image, i_last + 1);
        let name = jstr::substring_from(&str_directive, 1);
        let recognized = self.is_directive(&name)
            || self.rsvc.is_velocimacro(&name, &self.current_template_name)
            || matches!(name.as_str(), "if" | "end" | "set" | "else" | "elseif" | "stop");
        if recognized {
            let mut s = jstr::substring(str_image, 0, i_last / 2);
            s.push_str(&str_directive);
            s
        } else {
            str_image.to_string()
        }
    }

    // ---- JavaCC runtime

    fn jj_consume_token(&mut self, kind: i32) -> PResult<usize> {
        let old_token = self.token;
        match self.tok(self.token).next {
            Some(n) => self.token = n,
            None => {
                let n = self.next_from_source()?;
                self.token_source.tokens[self.token].next = Some(n);
                self.token = n;
            }
        }
        self.jj_ntk = -1;
        if self.tok(self.token).kind == kind {
            return Ok(self.token);
        }
        self.token = old_token;
        Err(self.generate_parse_exception())
    }

    fn generate_parse_exception(&self) -> ParseException {
        let t = match self.tok(self.token).next {
            Some(n) => self.tok(n),
            None => self.tok(self.token),
        };
        let image = if t.kind == EOF { "<EOF>".to_string() } else { add_escapes(&t.image) };
        ParseException(format!("Encountered \"{}\" at line {}, column {}.", image, t.begin_line, t.begin_column))
    }

    fn jj_scan_token(&mut self, kind: i32) -> bool {
        if self.jj_scanpos == self.jj_lastpos {
            self.jj_la -= 1;
            match self.tok(self.jj_scanpos).next {
                Some(n) => {
                    self.jj_scanpos = n;
                    self.jj_lastpos = n;
                }
                None => match self.next_from_source() {
                    Ok(n) => {
                        self.token_source.tokens[self.jj_scanpos].next = Some(n);
                        self.jj_scanpos = n;
                        self.jj_lastpos = n;
                    }
                    Err(_) => return true,
                },
            }
        } else {
            self.jj_scanpos = self.tok(self.jj_scanpos).next.unwrap();
        }
        self.tok(self.jj_scanpos).kind != kind
    }

    fn get_token(&mut self, index: usize) -> PResult<usize> {
        let mut t = self.token;
        for _ in 0..index {
            t = match self.tok(t).next {
                Some(n) => n,
                None => {
                    let n = self.next_from_source()?;
                    self.token_source.tokens[t].next = Some(n);
                    n
                }
            };
        }
        Ok(t)
    }

    fn ntk(&mut self) -> PResult<i32> {
        if self.jj_ntk != -1 {
            return Ok(self.jj_ntk);
        }
        let n = match self.tok(self.token).next {
            Some(n) => n,
            None => {
                let n = self.next_from_source()?;
                self.token_source.tokens[self.token].next = Some(n);
                n
            }
        };
        self.jj_ntk = self.tok(n).kind;
        Ok(self.jj_ntk)
    }

    fn jj_2(&mut self, xla: i32, f: fn(&mut Self) -> bool) -> PResult<bool> {
        self.jj_la = xla;
        self.jj_scanpos = self.token;
        self.jj_lastpos = self.token;
        let r = !f(self);
        match &self.lex_error {
            Some(e) => Err(ParseException(e.clone())),
            None => Ok(r),
        }
    }

    fn opt_whitespace(&mut self) -> PResult<()> {
        if self.ntk()? == WHITESPACE {
            self.jj_consume_token(WHITESPACE)?;
        }
        Ok(())
    }

    // ---- productions

    fn process(&mut self) -> PResult<usize> {
        let n = self.new_node(JJTPROCESS);
        self.open_node_scope(n)?;
        while STATEMENT_START.contains(&self.ntk()?) {
            self.statement()?;
        }
        self.jj_consume_token(EOF)?;
        self.close_node_scope(n)?;
        Ok(n)
    }

    fn statement(&mut self) -> PResult<()> {
        match self.ntk()? {
            IF_DIRECTIVE => self.if_statement(),
            STOP_DIRECTIVE => self.stop_statement(),
            _ => {
                if self.jj_2(2, Self::jj_3_1)? {
                    self.reference()
                } else {
                    match self.ntk()? {
                        SINGLE_LINE_COMMENT | FORMAL_COMMENT | MULTI_LINE_COMMENT => self.comment(),
                        SET_DIRECTIVE => self.set_directive(),
                        ESCAPE_DIRECTIVE => self.escaped_directive_node(),
                        DOUBLE_ESCAPE => self.escape(),
                        WORD => self.directive().map(|_| ()),
                        LPAREN | RPAREN | ESCAPE | TEXT | STRING_LITERAL | NUMBER_LITERAL | DOT | LCURLY | RCURLY => {
                            self.text()
                        }
                        _ => {
                            self.jj_consume_token(-1)?;
                            Err(self.generate_parse_exception())
                        }
                    }
                }
            }
        }
    }

    fn block(&mut self) -> PResult<()> {
        let b = self.new_node(JJTBLOCK);
        self.open_node_scope(b)?;
        loop {
            self.statement()?;
            if !STATEMENT_START.contains(&self.ntk()?) {
                break;
            }
        }
        self.close_node_scope(b)
    }

    fn escaped_directive_node(&mut self) -> PResult<()> {
        let n = self.new_node(JJTESCAPEDDIRECTIVE);
        self.open_node_scope(n)?;
        let t = self.jj_consume_token(ESCAPE_DIRECTIVE)?;
        self.close_node_scope(n)?;
        let image = self.escaped_directive(&self.tok(t).image.clone());
        self.token_source.tokens[t].image = image;
        Ok(())
    }

    fn escape(&mut self) -> PResult<()> {
        let n = self.new_node(JJTESCAPE);
        self.open_node_scope(n)?;
        let mut count = 0;
        let mut control = false;
        let mut t;
        loop {
            t = self.jj_consume_token(DOUBLE_ESCAPE)?;
            count += 1;
            if !self.jj_2(2, Self::jj_3_2)? {
                break;
            }
        }
        self.close_node_scope(n)?;
        let next = self.tok(t).next.unwrap();
        if matches!(self.tok(next).kind, IF_DIRECTIVE | ELSE_DIRECTIVE | ELSEIF_DIRECTIVE | END | STOP_DIRECTIVE) {
            control = true;
        }
        let name = jstr::substring_from(&self.tok(next).image, 1);
        if self.is_directive(&name) || self.rsvc.is_velocimacro(&name, &self.current_template_name) {
            control = true;
        }
        let mut val = String::new();
        for _ in 0..count {
            val.push_str(if control { "\\" } else { "\\\\" });
        }
        self.nodes[n].data = PData::Escape(val);
        Ok(())
    }

    fn comment(&mut self) -> PResult<()> {
        let n = self.new_node(JJTCOMMENT);
        self.open_node_scope(n)?;
        let k = self.ntk()?;
        match k {
            SINGLE_LINE_COMMENT | MULTI_LINE_COMMENT | FORMAL_COMMENT => {
                self.jj_consume_token(k)?;
            }
            _ => {
                self.jj_consume_token(-1)?;
                return Err(self.generate_parse_exception());
            }
        }
        self.close_node_scope(n)
    }

    fn leaf(&mut self, id: i32, kind: i32) -> PResult<()> {
        let n = self.new_node(id);
        self.open_node_scope(n)?;
        self.jj_consume_token(kind)?;
        self.close_node_scope(n)
    }

    fn number_literal(&mut self) -> PResult<()> {
        self.leaf(JJTNUMBERLITERAL, NUMBER_LITERAL)
    }

    fn string_literal(&mut self) -> PResult<()> {
        self.leaf(JJTSTRINGLITERAL, STRING_LITERAL)
    }

    fn identifier(&mut self) -> PResult<()> {
        self.leaf(JJTIDENTIFIER, IDENTIFIER)
    }

    fn word(&mut self) -> PResult<()> {
        self.leaf(JJTWORD, WORD)
    }

    fn directive_arg(&mut self) -> PResult<i32> {
        match self.ntk()? {
            IDENTIFIER | LCURLY => {
                self.reference()?;
                Ok(JJTREFERENCE)
            }
            WORD => {
                self.word()?;
                Ok(JJTWORD)
            }
            STRING_LITERAL => {
                self.string_literal()?;
                Ok(JJTSTRINGLITERAL)
            }
            NUMBER_LITERAL => {
                self.number_literal()?;
                Ok(JJTNUMBERLITERAL)
            }
            _ => {
                if self.jj_2(i32::MAX, Self::jj_3_3)? {
                    self.integer_range()?;
                    return Ok(JJTINTEGERRANGE);
                }
                match self.ntk()? {
                    LBRACKET => {
                        self.object_array()?;
                        Ok(JJTOBJECTARRAY)
                    }
                    TRUE => {
                        self.leaf(JJTTRUE, TRUE)?;
                        Ok(JJTTRUE)
                    }
                    FALSE => {
                        self.leaf(JJTFALSE, FALSE)?;
                        Ok(JJTFALSE)
                    }
                    _ => {
                        self.jj_consume_token(-1)?;
                        Err(self.generate_parse_exception())
                    }
                }
            }
        }
    }

    fn directive(&mut self) -> PResult<usize> {
        let n = self.new_node(JJTDIRECTIVE);
        self.open_node_scope(n)?;
        let t = self.jj_consume_token(WORD)?;
        let t_image = self.tok(t).image.clone();
        let directive_name = jstr::substring_from(&t_image, 1);
        let do_it_now = directive_name == "macro";
        let directive_type = self.rsvc.directive_type(&directive_name).unwrap_or(LINE);
        self.nodes[n].data = PData::Directive(directive_name);
        self.token_source.switch_to(DIRECTIVE);
        let mut arg_pos = 0;
        self.opt_whitespace()?;
        self.jj_consume_token(LPAREN)?;
        while self.jj_2(2, Self::jj_3_4)? {
            self.opt_whitespace()?;
            let arg_type = self.directive_arg()?;
            let (bl, bc) = (self.tok(t).begin_line, self.tok(t).begin_column);
            if arg_type == JJTWORD {
                if !(do_it_now && arg_pos == 0) && !(t_image == "#foreach" && arg_pos == 1) {
                    return Err(ParseException(format!(
                        "Invalid arg #{} in directive {} at line {}, column {} in template {}",
                        arg_pos, t_image, bl, bc, self.current_template_name
                    )));
                }
            } else if do_it_now && arg_pos == 0 {
                return Err(ParseException(format!(
                    "Invalid first arg  in #macro() directive - must be a word token (no ' or \" surrounding) at line {}, column {} in template {}",
                    bl, bc, self.current_template_name
                )));
            }
            arg_pos += 1;
        }
        self.opt_whitespace()?;
        self.jj_consume_token(RPAREN)?;
        if directive_type == LINE {
            self.close_node_scope(n)?;
            return Ok(n);
        }
        self.block()?;
        self.jj_consume_token(END)?;
        self.close_node_scope(n)?;
        if do_it_now {
            self.process_and_register(n)?;
        }
        Ok(n)
    }

    // Macro.processAndRegister at parse time: only the name matters to later parses.
    fn process_and_register(&mut self, n: usize) -> PResult<()> {
        let num_args = self.nodes[n].children.len();
        if num_args < 2 {
            self.rsvc.error(&format!(
                "#macro error : Velocimacro must have name as 1st argument to #macro(). #args = {num_args}"
            ));
            return Err(ParseException("First argument to #macro() must be  macro name.".to_string()));
        }
        let first_type = self.nodes[self.nodes[n].children[0]].id;
        if first_type != JJTWORD {
            return Err(ParseException(format!(
                "First argument to #macro() must be a token without surrounding ' or \", which specifies the macro name.  Currently it is a {}",
                JJT_NODE_NAME[first_type as usize]
            )));
        }
        let mut args: Vec<String> = Vec::new();
        for i in 0..num_args - 1 {
            let mut a = self.nodes[self.nodes[n].children[i]].first_token_image.clone().unwrap_or_default();
            if i > 0 && a.starts_with('$') {
                a = jstr::substring_from(&a, 1);
            }
            args.push(a);
        }
        let name = args.remove(0);
        self.rsvc.add_velocimacro(&name, None, args, &self.current_template_name);
        Ok(())
    }

    fn object_array(&mut self) -> PResult<()> {
        let n = self.new_node(JJTOBJECTARRAY);
        self.open_node_scope(n)?;
        self.jj_consume_token(LBRACKET)?;
        if PARAMETER_START.contains(&self.ntk()?) {
            self.parameter()?;
            while self.ntk()? == COMMA {
                self.jj_consume_token(COMMA)?;
                self.parameter()?;
            }
        }
        self.jj_consume_token(RBRACKET)?;
        self.close_node_scope(n)
    }

    fn range_operand(&mut self) -> PResult<()> {
        match self.ntk()? {
            IDENTIFIER | LCURLY => self.reference(),
            NUMBER_LITERAL => self.number_literal(),
            _ => {
                self.jj_consume_token(-1)?;
                Err(self.generate_parse_exception())
            }
        }
    }

    fn integer_range(&mut self) -> PResult<()> {
        let n = self.new_node(JJTINTEGERRANGE);
        self.open_node_scope(n)?;
        self.jj_consume_token(LBRACKET)?;
        self.opt_whitespace()?;
        self.range_operand()?;
        self.opt_whitespace()?;
        self.jj_consume_token(DOUBLEDOT)?;
        self.opt_whitespace()?;
        self.range_operand()?;
        self.opt_whitespace()?;
        self.jj_consume_token(RBRACKET)?;
        self.close_node_scope(n)
    }

    fn parameter(&mut self) -> PResult<()> {
        self.opt_whitespace()?;
        if self.ntk()? == STRING_LITERAL {
            self.string_literal()?;
        } else if self.jj_2(i32::MAX, Self::jj_3_5)? {
            self.integer_range()?;
        } else {
            match self.ntk()? {
                LBRACKET => self.object_array()?,
                TRUE => self.leaf(JJTTRUE, TRUE)?,
                FALSE => self.leaf(JJTFALSE, FALSE)?,
                IDENTIFIER | LCURLY => self.reference()?,
                NUMBER_LITERAL => self.number_literal()?,
                _ => {
                    self.jj_consume_token(-1)?;
                    return Err(self.generate_parse_exception());
                }
            }
        }
        self.opt_whitespace()
    }

    fn method(&mut self) -> PResult<()> {
        let n = self.new_node(JJTMETHOD);
        self.open_node_scope(n)?;
        self.identifier()?;
        self.jj_consume_token(LPAREN)?;
        if PARAMETER_START.contains(&self.ntk()?) {
            self.parameter()?;
            while self.ntk()? == COMMA {
                self.jj_consume_token(COMMA)?;
                self.parameter()?;
            }
        }
        self.jj_consume_token(REFMOD2_RPAREN)?;
        self.close_node_scope(n)
    }

    fn reference_tail(&mut self, more: fn(&mut Self) -> bool, method: fn(&mut Self) -> bool) -> PResult<()> {
        while self.jj_2(2, more)? {
            self.jj_consume_token(DOT)?;
            if self.jj_2(3, method)? {
                self.method()?;
            } else if self.ntk()? == IDENTIFIER {
                self.identifier()?;
            } else {
                self.jj_consume_token(-1)?;
                return Err(self.generate_parse_exception());
            }
        }
        Ok(())
    }

    fn reference(&mut self) -> PResult<()> {
        let n = self.new_node(JJTREFERENCE);
        self.open_node_scope(n)?;
        match self.ntk()? {
            IDENTIFIER => {
                self.jj_consume_token(IDENTIFIER)?;
                self.reference_tail(Self::jj_3_6, Self::jj_3_7)?;
            }
            LCURLY => {
                self.jj_consume_token(LCURLY)?;
                self.jj_consume_token(IDENTIFIER)?;
                self.reference_tail(Self::jj_3_8, Self::jj_3_9)?;
                self.jj_consume_token(RCURLY)?;
            }
            _ => {
                self.jj_consume_token(-1)?;
                return Err(self.generate_parse_exception());
            }
        }
        self.close_node_scope(n)
    }

    fn text(&mut self) -> PResult<()> {
        let n = self.new_node(JJTTEXT);
        self.open_node_scope(n)?;
        let k = self.ntk()?;
        match k {
            TEXT | DOT | RPAREN | LPAREN | NUMBER_LITERAL | STRING_LITERAL | ESCAPE | LCURLY | RCURLY => {
                self.jj_consume_token(k)?;
            }
            _ => {
                self.jj_consume_token(-1)?;
                return Err(self.generate_parse_exception());
            }
        }
        self.close_node_scope(n)
    }

    fn if_statement(&mut self) -> PResult<()> {
        let n = self.new_node(JJTIFSTATEMENT);
        self.open_node_scope(n)?;
        self.jj_consume_token(IF_DIRECTIVE)?;
        self.opt_whitespace()?;
        self.jj_consume_token(LPAREN)?;
        self.expression()?;
        self.jj_consume_token(RPAREN)?;
        self.block()?;
        if self.ntk()? == ELSEIF_DIRECTIVE {
            loop {
                self.else_if_statement()?;
                if self.ntk()? != ELSEIF_DIRECTIVE {
                    break;
                }
            }
        }
        if self.ntk()? == ELSE_DIRECTIVE {
            self.else_statement()?;
        }
        self.jj_consume_token(END)?;
        self.close_node_scope(n)
    }

    fn else_statement(&mut self) -> PResult<()> {
        let n = self.new_node(JJTELSESTATEMENT);
        self.open_node_scope(n)?;
        self.jj_consume_token(ELSE_DIRECTIVE)?;
        self.block()?;
        self.close_node_scope(n)
    }

    fn else_if_statement(&mut self) -> PResult<()> {
        let n = self.new_node(JJTELSEIFSTATEMENT);
        self.open_node_scope(n)?;
        self.jj_consume_token(ELSEIF_DIRECTIVE)?;
        self.opt_whitespace()?;
        self.jj_consume_token(LPAREN)?;
        self.expression()?;
        self.jj_consume_token(RPAREN)?;
        self.block()?;
        self.close_node_scope(n)
    }

    fn set_directive(&mut self) -> PResult<()> {
        let n = self.new_node(JJTSETDIRECTIVE);
        self.open_node_scope(n)?;
        self.jj_consume_token(SET_DIRECTIVE)?;
        self.opt_whitespace()?;
        self.reference()?;
        self.opt_whitespace()?;
        self.jj_consume_token(EQUALS)?;
        self.expression()?;
        self.jj_consume_token(RPAREN)?;
        self.token_source.in_set = false;
        if self.ntk()? == NEWLINE {
            self.jj_consume_token(NEWLINE)?;
        }
        self.close_node_scope(n)
    }

    fn stop_statement(&mut self) -> PResult<()> {
        self.jj_consume_token(STOP_DIRECTIVE).map(|_| ())
    }

    fn expression(&mut self) -> PResult<()> {
        let n = self.new_node(JJTEXPRESSION);
        self.open_node_scope(n)?;
        self.conditional_or_expression()?;
        self.close_node_scope(n)
    }

    fn binary(&mut self, id: i32, operand: fn(&mut Self) -> PResult<()>) -> PResult<()> {
        let n = self.new_node(id);
        self.open_node_scope(n)?;
        operand(self)?;
        self.close_node_scope_num(n, 2)
    }

    fn conditional_or_expression(&mut self) -> PResult<()> {
        self.conditional_and_expression()?;
        while self.ntk()? == LOGICAL_OR {
            self.jj_consume_token(LOGICAL_OR)?;
            self.binary(JJTORNODE, Self::conditional_and_expression)?;
        }
        Ok(())
    }

    fn conditional_and_expression(&mut self) -> PResult<()> {
        self.equality_expression()?;
        while self.ntk()? == LOGICAL_AND {
            self.jj_consume_token(LOGICAL_AND)?;
            self.binary(JJTANDNODE, Self::equality_expression)?;
        }
        Ok(())
    }

    fn equality_expression(&mut self) -> PResult<()> {
        self.relational_expression()?;
        loop {
            let k = self.ntk()?;
            let id = match k {
                LOGICAL_EQUALS => JJTEQNODE,
                LOGICAL_NOT_EQUALS => JJTNENODE,
                _ => break,
            };
            self.jj_consume_token(k)?;
            self.binary(id, Self::relational_expression)?;
        }
        Ok(())
    }

    fn relational_expression(&mut self) -> PResult<()> {
        self.additive_expression()?;
        loop {
            let k = self.ntk()?;
            let id = match k {
                LOGICAL_LT => JJTLTNODE,
                LOGICAL_GT => JJTGTNODE,
                LOGICAL_LE => JJTLENODE,
                LOGICAL_GE => JJTGENODE,
                _ => break,
            };
            self.jj_consume_token(k)?;
            self.binary(id, Self::additive_expression)?;
        }
        Ok(())
    }

    fn additive_expression(&mut self) -> PResult<()> {
        self.multiplicative_expression()?;
        loop {
            let k = self.ntk()?;
            let id = match k {
                PLUS => JJTADDNODE,
                MINUS => JJTSUBTRACTNODE,
                _ => break,
            };
            self.jj_consume_token(k)?;
            self.binary(id, Self::multiplicative_expression)?;
        }
        Ok(())
    }

    fn multiplicative_expression(&mut self) -> PResult<()> {
        self.unary_expression()?;
        loop {
            let k = self.ntk()?;
            let id = match k {
                MULTIPLY => JJTMULNODE,
                DIVIDE => JJTDIVNODE,
                MODULUS => JJTMODNODE,
                _ => break,
            };
            self.jj_consume_token(k)?;
            self.binary(id, Self::unary_expression)?;
        }
        Ok(())
    }

    fn unary_expression(&mut self) -> PResult<()> {
        if self.jj_2(2, Self::jj_3_10)? {
            self.opt_whitespace()?;
            self.jj_consume_token(LOGICAL_NOT)?;
            let n = self.new_node(JJTNOTNODE);
            self.open_node_scope(n)?;
            self.unary_expression()?;
            self.close_node_scope_num(n, 1)
        } else {
            match self.ntk()? {
                LBRACKET | LPAREN | WHITESPACE | STRING_LITERAL | TRUE | FALSE | NUMBER_LITERAL | IDENTIFIER
                | LCURLY => self.primary_expression(),
                _ => {
                    self.jj_consume_token(-1)?;
                    Err(self.generate_parse_exception())
                }
            }
        }
    }

    fn primary_expression(&mut self) -> PResult<()> {
        self.opt_whitespace()?;
        match self.ntk()? {
            STRING_LITERAL => self.string_literal()?,
            NUMBER_LITERAL => self.number_literal()?,
            IDENTIFIER | LCURLY => self.reference()?,
            _ => {
                if self.jj_2(i32::MAX, Self::jj_3_11)? {
                    self.integer_range()?;
                } else {
                    match self.ntk()? {
                        LBRACKET => self.object_array()?,
                        TRUE => self.leaf(JJTTRUE, TRUE)?,
                        FALSE => self.leaf(JJTFALSE, FALSE)?,
                        LPAREN => {
                            self.jj_consume_token(LPAREN)?;
                            self.expression()?;
                            self.jj_consume_token(RPAREN)?;
                        }
                        _ => {
                            self.jj_consume_token(-1)?;
                            return Err(self.generate_parse_exception());
                        }
                    }
                }
            }
        }
        self.opt_whitespace()
    }
}

pub const NORMAL_REFERENCE: i32 = 1;
pub const FORMAL_REFERENCE: i32 = 2;
pub const QUIET_REFERENCE: i32 = 3;
pub const RUNT: i32 = 4;

/// `Integer.parseInt` of a NUMBER_LITERAL image.
fn java_parse_int(s: &str) -> Option<i32> {
    s.parse::<i32>().ok()
}

/// `SimpleNode.literal()` while the tokens exist: `None` where Java throws a NullPointerException.
pub fn simple_literal(n: &PNode, tokens: &[Token]) -> Option<String> {
    let mut t = n.first?;
    let mut sb = tokens[t].image.clone();
    while Some(t) != n.last {
        t = tokens[t].next?;
        sb.push_str(&tokens[t].image);
    }
    Some(sb)
}

/// `NodeUtils.specialText`.
pub fn special_text(tokens: &[Token], t: usize) -> String {
    let mut special_text = String::new();
    let Some(st0) = tokens[t].special_token else { return special_text };
    if tokens[st0].image.starts_with("##") {
        return special_text;
    }
    let mut tmp = st0;
    while let Some(p) = tokens[tmp].special_token {
        tmp = p;
    }
    let mut cur = Some(tmp);
    while let Some(c) = cur {
        let st: Vec<u16> = jstr::to_utf16(&tokens[c].image);
        let mut sb: Vec<u16> = Vec::new();
        let mut i = 0usize;
        while i < st.len() {
            let ch = st[i];
            if ch == b'#' as u16 || ch == b'$' as u16 {
                sb.push(ch);
            }
            if ch == b'\\' as u16 {
                let mut ok = true;
                let mut term = false;
                let mut j = i;
                while ok && j < st.len() {
                    let cc = st[j];
                    if cc == b'\\' as u16 {
                    } else if cc == b'$' as u16 {
                        term = true;
                        ok = false;
                    } else {
                        ok = false;
                    }
                    j += 1;
                }
                if term {
                    sb.extend_from_slice(&st[i..j]);
                    i = j;
                }
            }
            i += 1;
        }
        special_text.push_str(&jstr::from_utf16(&sb));
        cur = tokens[c].next;
    }
    special_text
}

/// `NodeUtils.tokenLiteral`.
pub fn token_literal(tokens: &[Token], t: usize) -> String {
    let mut s = special_text(tokens, t);
    s.push_str(&tokens[t].image);
    s
}

#[allow(non_snake_case, unused_assignments, unused_mut, clippy::needless_return)]
impl Parser<'_> {
    fn jj_3R_54(&mut self) -> bool {
        if self.jj_scan_token(STRING_LITERAL) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_24(&mut self) -> bool {
        if self.jj_scan_token(WHITESPACE) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_41(&mut self) -> bool {
        if self.jj_scan_token(NUMBER_LITERAL) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3_4(&mut self) -> bool {
        let mut xsp: usize;
        xsp = self.jj_scanpos;
        if self.jj_3R_24() {
            self.jj_scanpos = xsp;
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        if self.jj_3R_25() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_38(&mut self) -> bool {
        if self.jj_scan_token(WHITESPACE) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_29(&mut self) -> bool {
        if self.jj_scan_token(WHITESPACE) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_32(&mut self) -> bool {
        if self.jj_3R_50() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_30(&mut self) -> bool {
        if self.jj_3R_50() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_37(&mut self) -> bool {
        if self.jj_3R_41() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3_2(&mut self) -> bool {
        if self.jj_scan_token(DOUBLE_ESCAPE) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_28(&mut self) -> bool {
        if self.jj_3R_41() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_36(&mut self) -> bool {
        if self.jj_3R_19() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3_9(&mut self) -> bool {
        if self.jj_3R_31() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_82(&mut self) -> bool {
        if self.jj_scan_token(COMMA) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        if self.jj_3R_59() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_23(&mut self) -> bool {
        if self.jj_scan_token(WHITESPACE) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3_7(&mut self) -> bool {
        if self.jj_3R_31() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_27(&mut self) -> bool {
        if self.jj_3R_19() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_58(&mut self) -> bool {
        if self.jj_scan_token(FALSE) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_35(&mut self) -> bool {
        if self.jj_scan_token(WHITESPACE) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_57(&mut self) -> bool {
        if self.jj_scan_token(TRUE) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3_8(&mut self) -> bool {
        if self.jj_scan_token(DOT) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        let mut xsp: usize;
        xsp = self.jj_scanpos;
        if self.jj_3_9() {
            self.jj_scanpos = xsp;
            if self.jj_3R_32() {
                return true;
            }
            if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                return false;
            }
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_51(&mut self) -> bool {
        if self.jj_3R_59() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        let mut xsp: usize;
        loop {
            xsp = self.jj_scanpos;
            if self.jj_3R_82() {
                self.jj_scanpos = xsp;
                break;
            }
            if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                return false;
            }
        }
        return false;
    }
    fn jj_3_6(&mut self) -> bool {
        if self.jj_scan_token(DOT) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        let mut xsp: usize;
        xsp = self.jj_scanpos;
        if self.jj_3_7() {
            self.jj_scanpos = xsp;
            if self.jj_3R_30() {
                return true;
            }
            if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                return false;
            }
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_40(&mut self) -> bool {
        if self.jj_scan_token(LCURLY) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        if self.jj_scan_token(IDENTIFIER) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        let mut xsp: usize;
        loop {
            xsp = self.jj_scanpos;
            if self.jj_3_8() {
                self.jj_scanpos = xsp;
                break;
            }
            if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                return false;
            }
        }
        if self.jj_scan_token(RCURLY) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_22(&mut self) -> bool {
        if self.jj_3R_41() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3_11(&mut self) -> bool {
        if self.jj_scan_token(LBRACKET) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        let mut xsp: usize;
        xsp = self.jj_scanpos;
        if self.jj_3R_35() {
            self.jj_scanpos = xsp;
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        xsp = self.jj_scanpos;
        if self.jj_3R_36() {
            self.jj_scanpos = xsp;
            if self.jj_3R_37() {
                return true;
            }
            if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                return false;
            }
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        xsp = self.jj_scanpos;
        if self.jj_3R_38() {
            self.jj_scanpos = xsp;
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        if self.jj_scan_token(DOUBLEDOT) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_26(&mut self) -> bool {
        if self.jj_scan_token(WHITESPACE) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_39(&mut self) -> bool {
        if self.jj_scan_token(IDENTIFIER) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        let mut xsp: usize;
        loop {
            xsp = self.jj_scanpos;
            if self.jj_3_6() {
                self.jj_scanpos = xsp;
                break;
            }
            if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                return false;
            }
        }
        return false;
    }
    fn jj_3_1(&mut self) -> bool {
        if self.jj_3R_19() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_81(&mut self) -> bool {
        if self.jj_scan_token(LPAREN) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_19(&mut self) -> bool {
        let mut xsp: usize;
        xsp = self.jj_scanpos;
        if self.jj_3R_39() {
            self.jj_scanpos = xsp;
            if self.jj_3R_40() {
                return true;
            }
            if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                return false;
            }
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_80(&mut self) -> bool {
        if self.jj_3R_58() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_33(&mut self) -> bool {
        if self.jj_scan_token(WHITESPACE) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_79(&mut self) -> bool {
        if self.jj_3R_57() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_78(&mut self) -> bool {
        if self.jj_3R_56() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_77(&mut self) -> bool {
        if self.jj_3R_55() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_76(&mut self) -> bool {
        if self.jj_3R_19() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_21(&mut self) -> bool {
        if self.jj_3R_19() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3_5(&mut self) -> bool {
        if self.jj_scan_token(LBRACKET) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        let mut xsp: usize;
        xsp = self.jj_scanpos;
        if self.jj_3R_26() {
            self.jj_scanpos = xsp;
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        xsp = self.jj_scanpos;
        if self.jj_3R_27() {
            self.jj_scanpos = xsp;
            if self.jj_3R_28() {
                return true;
            }
            if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                return false;
            }
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        xsp = self.jj_scanpos;
        if self.jj_3R_29() {
            self.jj_scanpos = xsp;
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        if self.jj_scan_token(DOUBLEDOT) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_75(&mut self) -> bool {
        if self.jj_3R_41() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_31(&mut self) -> bool {
        if self.jj_3R_50() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        if self.jj_scan_token(LPAREN) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        let mut xsp: usize;
        xsp = self.jj_scanpos;
        if self.jj_3R_51() {
            self.jj_scanpos = xsp;
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        if self.jj_scan_token(REFMOD2_RPAREN) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_74(&mut self) -> bool {
        if self.jj_3R_54() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_73(&mut self) -> bool {
        if self.jj_scan_token(WHITESPACE) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_60(&mut self) -> bool {
        let mut xsp: usize;
        xsp = self.jj_scanpos;
        if self.jj_3R_73() {
            self.jj_scanpos = xsp;
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        xsp = self.jj_scanpos;
        if self.jj_3R_74() {
            self.jj_scanpos = xsp;
            if self.jj_3R_75() {
                self.jj_scanpos = xsp;
                if self.jj_3R_76() {
                    self.jj_scanpos = xsp;
                    if self.jj_3R_77() {
                        self.jj_scanpos = xsp;
                        if self.jj_3R_78() {
                            self.jj_scanpos = xsp;
                            if self.jj_3R_79() {
                                self.jj_scanpos = xsp;
                                if self.jj_3R_80() {
                                    self.jj_scanpos = xsp;
                                    if self.jj_3R_81() {
                                        return true;
                                    }
                                    if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                                        return false;
                                    }
                                } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                                    return false;
                                }
                            } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                                return false;
                            }
                        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                            return false;
                        }
                    } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                        return false;
                    }
                } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                    return false;
                }
            } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                return false;
            }
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_85(&mut self) -> bool {
        if self.jj_scan_token(WHITESPACE) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_72(&mut self) -> bool {
        if self.jj_3R_41() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_71(&mut self) -> bool {
        if self.jj_3R_19() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_83(&mut self) -> bool {
        if self.jj_scan_token(WHITESPACE) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_70(&mut self) -> bool {
        if self.jj_3R_58() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_69(&mut self) -> bool {
        if self.jj_3R_57() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_34(&mut self) -> bool {
        let mut xsp: usize;
        xsp = self.jj_scanpos;
        if self.jj_3_10() {
            self.jj_scanpos = xsp;
            if self.jj_3R_52() {
                return true;
            }
            if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                return false;
            }
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3_10(&mut self) -> bool {
        let mut xsp: usize;
        xsp = self.jj_scanpos;
        if self.jj_3R_33() {
            self.jj_scanpos = xsp;
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        if self.jj_scan_token(LOGICAL_NOT) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        if self.jj_3R_34() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_52(&mut self) -> bool {
        if self.jj_3R_60() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_68(&mut self) -> bool {
        if self.jj_3R_56() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_67(&mut self) -> bool {
        if self.jj_3R_55() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_66(&mut self) -> bool {
        if self.jj_3R_54() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_20(&mut self) -> bool {
        if self.jj_scan_token(WHITESPACE) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_87(&mut self) -> bool {
        if self.jj_3R_41() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_89(&mut self) -> bool {
        if self.jj_scan_token(COMMA) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        if self.jj_3R_59() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_63(&mut self) -> bool {
        if self.jj_3R_41() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_65(&mut self) -> bool {
        if self.jj_scan_token(WHITESPACE) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_59(&mut self) -> bool {
        let mut xsp: usize;
        xsp = self.jj_scanpos;
        if self.jj_3R_65() {
            self.jj_scanpos = xsp;
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        xsp = self.jj_scanpos;
        if self.jj_3R_66() {
            self.jj_scanpos = xsp;
            if self.jj_3R_67() {
                self.jj_scanpos = xsp;
                if self.jj_3R_68() {
                    self.jj_scanpos = xsp;
                    if self.jj_3R_69() {
                        self.jj_scanpos = xsp;
                        if self.jj_3R_70() {
                            self.jj_scanpos = xsp;
                            if self.jj_3R_71() {
                                self.jj_scanpos = xsp;
                                if self.jj_3R_72() {
                                    return true;
                                }
                                if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                                    return false;
                                }
                            } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                                return false;
                            }
                        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                            return false;
                        }
                    } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                        return false;
                    }
                } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                    return false;
                }
            } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                return false;
            }
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        xsp = self.jj_scanpos;
        if self.jj_3R_83() {
            self.jj_scanpos = xsp;
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_61(&mut self) -> bool {
        if self.jj_scan_token(WHITESPACE) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3_3(&mut self) -> bool {
        if self.jj_scan_token(LBRACKET) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        let mut xsp: usize;
        xsp = self.jj_scanpos;
        if self.jj_3R_20() {
            self.jj_scanpos = xsp;
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        xsp = self.jj_scanpos;
        if self.jj_3R_21() {
            self.jj_scanpos = xsp;
            if self.jj_3R_22() {
                return true;
            }
            if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                return false;
            }
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        xsp = self.jj_scanpos;
        if self.jj_3R_23() {
            self.jj_scanpos = xsp;
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        if self.jj_scan_token(DOUBLEDOT) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_49(&mut self) -> bool {
        if self.jj_3R_58() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_88(&mut self) -> bool {
        if self.jj_scan_token(WHITESPACE) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_86(&mut self) -> bool {
        if self.jj_3R_19() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_48(&mut self) -> bool {
        if self.jj_3R_57() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_84(&mut self) -> bool {
        if self.jj_scan_token(WHITESPACE) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_62(&mut self) -> bool {
        if self.jj_3R_19() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_64(&mut self) -> bool {
        if self.jj_3R_59() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        let mut xsp: usize;
        loop {
            xsp = self.jj_scanpos;
            if self.jj_3R_89() {
                self.jj_scanpos = xsp;
                break;
            }
            if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                return false;
            }
        }
        return false;
    }
    fn jj_3R_47(&mut self) -> bool {
        if self.jj_3R_56() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_55(&mut self) -> bool {
        if self.jj_scan_token(LBRACKET) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        let mut xsp: usize;
        xsp = self.jj_scanpos;
        if self.jj_3R_61() {
            self.jj_scanpos = xsp;
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        xsp = self.jj_scanpos;
        if self.jj_3R_62() {
            self.jj_scanpos = xsp;
            if self.jj_3R_63() {
                return true;
            }
            if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                return false;
            }
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        xsp = self.jj_scanpos;
        if self.jj_3R_84() {
            self.jj_scanpos = xsp;
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        if self.jj_scan_token(DOUBLEDOT) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        xsp = self.jj_scanpos;
        if self.jj_3R_85() {
            self.jj_scanpos = xsp;
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        xsp = self.jj_scanpos;
        if self.jj_3R_86() {
            self.jj_scanpos = xsp;
            if self.jj_3R_87() {
                return true;
            }
            if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                return false;
            }
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        xsp = self.jj_scanpos;
        if self.jj_3R_88() {
            self.jj_scanpos = xsp;
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        if self.jj_scan_token(RBRACKET) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_46(&mut self) -> bool {
        if self.jj_3R_55() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_45(&mut self) -> bool {
        if self.jj_3R_41() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_56(&mut self) -> bool {
        if self.jj_scan_token(LBRACKET) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        let mut xsp: usize;
        xsp = self.jj_scanpos;
        if self.jj_3R_64() {
            self.jj_scanpos = xsp;
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        if self.jj_scan_token(RBRACKET) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_44(&mut self) -> bool {
        if self.jj_3R_54() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_43(&mut self) -> bool {
        if self.jj_3R_53() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_42(&mut self) -> bool {
        if self.jj_3R_19() {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_25(&mut self) -> bool {
        let mut xsp: usize;
        xsp = self.jj_scanpos;
        if self.jj_3R_42() {
            self.jj_scanpos = xsp;
            if self.jj_3R_43() {
                self.jj_scanpos = xsp;
                if self.jj_3R_44() {
                    self.jj_scanpos = xsp;
                    if self.jj_3R_45() {
                        self.jj_scanpos = xsp;
                        if self.jj_3R_46() {
                            self.jj_scanpos = xsp;
                            if self.jj_3R_47() {
                                self.jj_scanpos = xsp;
                                if self.jj_3R_48() {
                                    self.jj_scanpos = xsp;
                                    if self.jj_3R_49() {
                                        return true;
                                    }
                                    if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                                        return false;
                                    }
                                } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                                    return false;
                                }
                            } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                                return false;
                            }
                        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                            return false;
                        }
                    } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                        return false;
                    }
                } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                    return false;
                }
            } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
                return false;
            }
        } else if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_53(&mut self) -> bool {
        if self.jj_scan_token(WORD) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
    fn jj_3R_50(&mut self) -> bool {
        if self.jj_scan_token(IDENTIFIER) {
            return true;
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return false;
        }
        return false;
    }
}
