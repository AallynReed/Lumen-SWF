//! Port of the `flex2.compiler.mxml.Parser` JavaCC generates from `Grammar.jj` (with `ParserConstants`,
//! `ParseException`, `TokenManager` and `MxmlVisitor`); the tokens' `next` chain is a vector.

#![allow(unused_assignments, clippy::needless_return)]

use super::dom::scanner_error::ScannerError;
use super::mxml_namespaces::{FXG_2008_NAMESPACE, MXML_2006_NAMESPACE, MXML_2009_NAMESPACE, MX_NAMESPACE, SPARK_NAMESPACE};
use super::token::{Tok, Token};

pub const EOF: i32 = 0;
pub const START_DESIGNLAYER: i32 = 1;
pub const END_DESIGNLAYER: i32 = 2;
pub const START_DECLARATIONS: i32 = 3;
pub const END_DECLARATIONS: i32 = 4;
pub const START_STYLE: i32 = 5;
pub const END_STYLE: i32 = 6;
pub const START_SCRIPT: i32 = 7;
pub const END_SCRIPT: i32 = 8;
pub const START_METADATA: i32 = 9;
pub const END_METADATA: i32 = 10;
pub const START_STATE: i32 = 11;
pub const END_STATE: i32 = 12;
pub const START_MODEL: i32 = 13;
pub const END_MODEL: i32 = 14;
pub const START_XML: i32 = 15;
pub const END_XML: i32 = 16;
pub const START_XMLLIST: i32 = 17;
pub const END_XMLLIST: i32 = 18;
pub const START_ARRAY: i32 = 19;
pub const END_ARRAY: i32 = 20;
pub const START_VECTOR: i32 = 21;
pub const END_VECTOR: i32 = 22;
pub const START_BINDING: i32 = 23;
pub const END_BINDING: i32 = 24;
pub const START_STRING: i32 = 25;
pub const END_STRING: i32 = 26;
pub const START_NUMBER: i32 = 27;
pub const END_NUMBER: i32 = 28;
pub const START_INT: i32 = 29;
pub const END_INT: i32 = 30;
pub const START_UINT: i32 = 31;
pub const END_UINT: i32 = 32;
pub const START_BOOLEAN: i32 = 33;
pub const END_BOOLEAN: i32 = 34;
pub const START_CLASS: i32 = 35;
pub const END_CLASS: i32 = 36;
pub const START_FUNCTION: i32 = 37;
pub const END_FUNCTION: i32 = 38;
pub const START_REMOTEOBJECT: i32 = 39;
pub const END_REMOTEOBJECT: i32 = 40;
pub const START_METHOD: i32 = 41;
pub const END_METHOD: i32 = 42;
pub const START_ARGUMENTS: i32 = 43;
pub const END_ARGUMENTS: i32 = 44;
pub const START_WEBSERVICE: i32 = 45;
pub const END_WEBSERVICE: i32 = 46;
pub const START_OPERATION: i32 = 47;
pub const END_OPERATION: i32 = 48;
pub const START_REQUEST: i32 = 49;
pub const END_REQUEST: i32 = 50;
pub const START_HTTPSERVICE: i32 = 51;
pub const END_HTTPSERVICE: i32 = 52;
pub const START_COMPONENT: i32 = 53;
pub const END_COMPONENT: i32 = 54;
pub const START_REPARENT: i32 = 55;
pub const END_REPARENT: i32 = 56;
pub const START_PRIVATE: i32 = 57;
pub const END_PRIVATE: i32 = 58;
pub const START_LIBRARY: i32 = 59;
pub const END_LIBRARY: i32 = 60;
pub const START_DEFINITION: i32 = 61;
pub const END_DEFINITION: i32 = 62;
pub const START_ELEMENT: i32 = 63;
pub const CDATA: i32 = 64;
pub const END_ELEMENT: i32 = 65;
pub const SKIP_ELEMENT: i32 = 66;

/// `ParserConstants.tokenImage` after `Parser`'s static initializer has stripped quotes and swapped entries into kind order.
pub const TOKEN_IMAGE: [&str; 67] = [
    "<EOF>", "DesignLayer", "/DesignLayer", "Declarations", "/Declarations", "Style", "/Style", "Script", "/Script", "Metadata", "/Metadata", "State", "/State",
    "Model", "/Model", "XML", "/XML", "XMLList", "/XMLList", "Array", "/Array", "Vector", "/Vector", "Binding", "/Binding", "String", "/String", "Number",
    "/Number", "int", "/int", "uint", "/uint", "Boolean", "/Boolean", "Class", "/Class", "Function", "/Function", "RemoteObject", "/RemoteObject", "method",
    "/method", "arguments", "/arguments", "WebService", "/WebService", "operation", "/operation", "request", "/request", "HTTPService", "/HTTPService",
    "Component", "/Component", "Reparent", "/Reparent", "Private", "/Private", "Library", "/Library", "Definition", "/Definition",
    "Non-MXML language element", "<CDATA>", "Non-MXML language element", "<SKIP_ELEMENT>",
];

const SPECIAL_COMPONENT_TAGS: [&str; 8] = ["State", "HTTPService", "RemoteObject", "WebService", "method", "arguments", "operation", "request"];
const FXG_LANGUAGE_TAGS: [&str; 3] = ["Definition", "Library", "Private"];
const MXML2009_NEW_TAGS: [&str; 6] = ["Declarations", "Definition", "DesignLayer", "Library", "Private", "Reparent"];

/// `ConstantMappings.get(localPart)`: the kind whose image is `local_part`.
fn constant_mapping(local_part: &str) -> Option<i32> {
    if local_part == "Non-MXML language element" {
        return None;
    }
    TOKEN_IMAGE.iter().position(|i| *i == local_part).map(|i| i as i32)
}

/// `System.getProperty("line.separator")`.
pub const LINE_SEPARATOR: &str = if cfg!(windows) { "\r\n" } else { "\n" };

pub fn find_element_type(uri: &str, local_name: &str, is_start: bool) -> i32 {
    let lookup = || {
        let local_part = if is_start { local_name.to_string() } else { format!("/{local_name}") };
        constant_mapping(&local_part)
    };
    let mapped = if uri == MXML_2009_NAMESPACE && !SPECIAL_COMPONENT_TAGS.contains(&local_name) {
        lookup()
    } else if uri == MXML_2006_NAMESPACE && !MXML2009_NEW_TAGS.contains(&local_name) {
        lookup()
    } else if uri == FXG_2008_NAMESPACE && FXG_LANGUAGE_TAGS.contains(&local_name) {
        lookup()
    } else if (uri == SPARK_NAMESPACE || uri == MX_NAMESPACE) && SPECIAL_COMPONENT_TAGS.contains(&local_name) {
        lookup()
    } else {
        None
    };
    mapped.unwrap_or(if is_start { START_ELEMENT } else { END_ELEMENT })
}

/// `ParseException(currentToken, expectedTokenSequences, tokenImage)`.
#[derive(Clone)]
pub struct ParseException {
    pub current_token: Tok,
    /// `currentToken.next`.
    pub next: Tok,
    pub expected_token_sequences: Vec<Vec<i32>>,
    message: String,
}

impl ParseException {
    fn new(current_token: Tok, next: Tok, expected_token_sequences: Vec<Vec<i32>>) -> ParseException {
        let message = initialise(&next, &expected_token_sequences);
        ParseException { current_token, next, expected_token_sequences, message }
    }

    pub fn get_message(&self) -> &str {
        &self.message
    }
}

fn initialise(next: &Tok, expected_token_sequences: &[Vec<i32>]) -> String {
    let eol = LINE_SEPARATOR;
    let mut expected = String::new();
    let mut max_size = 0;
    for seq in expected_token_sequences {
        max_size = max_size.max(seq.len());
        for &j in seq {
            expected.push_str(TOKEN_IMAGE[j as usize]);
            expected.push(' ');
        }
        if seq[seq.len() - 1] != 0 {
            expected.push_str("...");
        }
        expected.push_str(eol);
        expected.push_str("    ");
    }
    let mut retval = String::from("Encountered \"");
    for i in 0..max_size {
        if i != 0 {
            retval.push(' ');
        }
        if next.kind() == 0 {
            retval.push_str(TOKEN_IMAGE[0]);
            break;
        }
        retval.push(' ');
        retval.push_str(TOKEN_IMAGE[next.kind() as usize]);
        retval.push_str(" \"");
        retval.push_str(&add_escapes(&next.image()));
        retval.push_str(" \"");
    }
    retval.push_str(&format!("\" at line {}, column {}", next.begin_line(), next.begin_column()));
    retval.push('.');
    retval.push_str(eol);
    if expected_token_sequences.len() == 1 {
        retval.push_str("Was expecting:");
    } else {
        retval.push_str("Was expecting one of:");
    }
    retval.push_str(eol);
    retval.push_str("    ");
    retval.push_str(&expected);
    retval
}

fn add_escapes(s: &str) -> String {
    let mut r = String::new();
    for c in s.encode_utf16() {
        match c {
            0 => {}
            8 => r.push_str("\\b"),
            9 => r.push_str("\\t"),
            10 => r.push_str("\\n"),
            12 => r.push_str("\\f"),
            13 => r.push_str("\\r"),
            34 => r.push_str("\\\""),
            39 => r.push_str("\\'"),
            92 => r.push_str("\\\\"),
            c if !(0x20..=0x7e).contains(&c) => r.push_str(&format!("\\u{:04x}", c)),
            c => r.push(c as u8 as char),
        }
    }
    r
}

/// What can escape the parser: its `ParseException`, the scanner's `ScannerError`, or a Java runtime exception.
#[derive(Clone)]
pub enum ParserError {
    Parse(ParseException),
    Scanner(ScannerError),
    /// `Exception.getMessage()` of a runtime exception (e.g. `ArrayIndexOutOfBoundsException`).
    Runtime(String),
}

pub type PResult<T> = Result<T, ParserError>;

pub trait TokenManager {
    fn get_next_token(&mut self) -> PResult<Tok>;
}

pub trait MxmlVisitor {
    fn parse_application(&mut self, app: &Tok, components: Option<Vec<Tok>>);
    fn parse_declarations(&mut self, t: &Tok, properties: Option<Vec<Tok>>);
    fn parse_component(&mut self, comp: &Tok, components: Option<Vec<Tok>>);
    fn parse_style(&mut self, style: &Tok, text: Option<&Tok>);
    fn parse_script(&mut self, script: &Tok, text: Option<&Tok>);
    fn parse_meta_data(&mut self, metadata: &Tok, text: Option<&Tok>);
    fn parse_model(&mut self, t: &Tok, objects: Option<Vec<Tok>>);
    fn parse_xml(&mut self, t: &Tok, objects: Option<Vec<Tok>>);
    fn parse_xml_list(&mut self, t: &Tok, objects: Option<Vec<Tok>>);
    fn parse_array(&mut self, t: &Tok, elements: Option<Vec<Tok>>);
    fn parse_vector(&mut self, t: &Tok, elements: Option<Vec<Tok>>);
    fn parse_binding(&mut self, t: &Tok);
    fn parse_anonymous_object(&mut self, t: &Tok, objects: Option<Vec<Tok>>);
    fn parse_web_service(&mut self, t: &Tok, children: Option<Vec<Tok>>);
    fn parse_http_service(&mut self, t: &Tok, children: Option<Vec<Tok>>);
    fn parse_remote_object(&mut self, t: &Tok, children: Option<Vec<Tok>>);
    fn parse_operation(&mut self, t: &Tok, children: Option<Vec<Tok>>);
    fn parse_request(&mut self, t: &Tok, children: Option<Vec<Tok>>);
    fn parse_method(&mut self, t: &Tok, children: Option<Vec<Tok>>);
    fn parse_arguments(&mut self, t: &Tok, children: Option<Vec<Tok>>);
    fn parse_string(&mut self, s: &Tok, data: Option<&Tok>);
    fn parse_number(&mut self, n: &Tok, data: Option<&Tok>);
    fn parse_int(&mut self, n: &Tok, data: Option<&Tok>);
    fn parse_uint(&mut self, n: &Tok, data: Option<&Tok>);
    fn parse_boolean(&mut self, b: &Tok, data: Option<&Tok>);
    fn parse_class(&mut self, b: &Tok, data: Option<&Tok>);
    fn parse_function(&mut self, b: &Tok, data: Option<&Tok>);
    fn parse_inline_component(&mut self, t: &Tok, child: Option<&Tok>);
    fn parse_reparent(&mut self, t: &Tok);
    fn parse_state(&mut self, t: &Tok, children: Option<Vec<Tok>>);
    fn parse_library(&mut self, t: &Tok, children: Option<Vec<Tok>>);
    fn parse_definition(&mut self, t: &Tok, child: Option<&Tok>);
    fn parse_design_layer(&mut self, t: &Tok, children: Option<Vec<Tok>>);
}

fn add_child(children: Option<Vec<Tok>>, child: Tok) -> Option<Vec<Tok>> {
    let mut c = children.unwrap_or_default();
    c.push(child);
    Some(c)
}

const JJ_LA1_0: [u32; 59] = [
    0xaaaaaaaa, 0xaaaaaaaa, 0xaaaaaaa2, 0xaaaaaaa2, 0xaaaaaaa2, 0xaaaaaaa2, 0x2a0, 0xaaaaa802, 0xaaaaa802, 0x0, 0x0, 0x0, 0x0, 0xaaaaa800, 0xaaaaa802,
    0xaaaaa802, 0xaa280000, 0xaa280000, 0x0, 0x0, 0x0, 0x0, 0x0, 0xaa280802, 0xaa280802, 0xaa280802, 0xaa280802, 0x0, 0x0, 0xaa000000, 0x0, 0x0, 0x0,
    0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0xaa080000, 0xaa080000, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, 0x2, 0x0, 0x0, 0x0, 0x2, 0x2,
];
const JJ_LA1_1: [u32; 59] = [
    0x8aa820aa, 0x8aa820aa, 0x80a0002a, 0x80a0002a, 0x80a0002a, 0x80a0002a, 0x0, 0x80a820aa, 0x80a820aa, 0x82080, 0x0, 0x0, 0x0, 0xa0002a, 0x80a0002a,
    0x80a0002a, 0x8000002a, 0x8000002a, 0x0, 0x80000000, 0x0, 0x80000000, 0x0, 0x8080002a, 0x8080002a, 0x8080002a, 0x8080002a, 0x80000000, 0x80000000,
    0x2a, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x80000200, 0x80000200, 0x80000800, 0x80000800, 0x8000002a, 0x8000002a, 0x0, 0x80008000, 0x80008000,
    0x80020000, 0x80020000, 0x80000000, 0x0, 0x80020000, 0x80020000, 0x80000000, 0x80000000, 0x0, 0x0, 0x20000000, 0x80000000, 0x80000000,
];
const JJ_LA1_2: [u32; 59] = [
    0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x0, 0x0, 0x0, 0x0, 0x1, 0x1, 0x1, 0x0, 0x0, 0x0, 0x0, 0x0, 0x1, 0x0, 0x1, 0x0, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1,
    0x0, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0x0, 0x5, 0x5, 0x0, 0x0,
    0x0,
];

pub struct Parser<'a> {
    token_source: &'a mut dyn TokenManager,
    /// `token` and the tokens after it; `tokens[i + 1]` is `tokens[i].next`.
    tokens: Vec<Tok>,
    token: usize,
    jj_ntk: i32,
    jj_gen: i32,
    jj_la1: [i32; 59],
    jj_kind: i32,
    visitor: Option<Box<dyn MxmlVisitor + 'a>>,
}

impl<'a> Parser<'a> {
    pub fn new(tm: &'a mut dyn TokenManager) -> Parser<'a> {
        Parser { token_source: tm, tokens: vec![Tok::plain(Token::default())], token: 0, jj_ntk: -1, jj_gen: 0, jj_la1: [-1; 59], jj_kind: -1, visitor: None }
    }

    pub fn set_visitor(&mut self, v: Box<dyn MxmlVisitor + 'a>) {
        self.visitor = Some(v);
    }

    fn next_of(&mut self, i: usize) -> PResult<usize> {
        if i + 1 >= self.tokens.len() {
            let t = self.token_source.get_next_token()?;
            self.tokens.push(t);
        }
        Ok(i + 1)
    }

    fn jj_consume_token(&mut self, kind: i32) -> PResult<Tok> {
        let old_token = self.token;
        self.token = self.next_of(old_token)?;
        self.jj_ntk = -1;
        if self.tokens[self.token].kind() == kind {
            self.jj_gen += 1;
            return Ok(self.tokens[self.token].clone());
        }
        self.token = old_token;
        self.jj_kind = kind;
        Err(ParserError::Parse(self.generate_parse_exception()))
    }

    /// `jj_consume_token(-1); throw new ParseException();`: the consume always throws first.
    fn fail(&mut self) -> ParserError {
        match self.jj_consume_token(-1) {
            Err(e) => e,
            Ok(_) => unreachable!(),
        }
    }

    pub fn get_next_token(&mut self) -> PResult<Tok> {
        self.token = self.next_of(self.token)?;
        self.jj_ntk = -1;
        self.jj_gen += 1;
        Ok(self.tokens[self.token].clone())
    }

    pub fn get_token(&mut self, index: usize) -> PResult<Tok> {
        let mut t = self.token;
        for _ in 0..index {
            t = self.next_of(t)?;
        }
        Ok(self.tokens[t].clone())
    }

    fn ntk(&mut self) -> PResult<i32> {
        if self.jj_ntk == -1 {
            let n = self.next_of(self.token)?;
            self.jj_ntk = self.tokens[n].kind();
        }
        Ok(self.jj_ntk)
    }

    pub fn generate_parse_exception(&mut self) -> ParseException {
        let mut la1tokens = [false; 67];
        if self.jj_kind >= 0 {
            la1tokens[self.jj_kind as usize] = true;
            self.jj_kind = -1;
        }
        for i in 0..59 {
            if self.jj_la1[i] == self.jj_gen {
                for j in 0..32 {
                    if JJ_LA1_0[i] & (1 << j) != 0 {
                        la1tokens[j] = true;
                    }
                    if JJ_LA1_1[i] & (1 << j) != 0 {
                        la1tokens[32 + j] = true;
                    }
                    if JJ_LA1_2[i] & (1 << j) != 0 && 64 + j < 67 {
                        la1tokens[64 + j] = true;
                    }
                }
            }
        }
        let exptokseq: Vec<Vec<i32>> = (0..67).filter(|&i| la1tokens[i]).map(|i| vec![i as i32]).collect();
        let current = self.tokens[self.token].clone();
        let next = self.tokens[self.token + 1].clone();
        ParseException::new(current, next, exptokseq)
    }

    pub fn parse_application(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        t = Some(self.parse_application_component()?);
        self.jj_consume_token(0)?;
        return Ok(t.unwrap());
    }

    pub fn parse_application_component(&mut self) -> PResult<Tok> {
        let mut app: Option<Tok> = None;
        let mut child: Option<Tok> = None;
        let mut children: Option<Vec<Tok>> = None;
        app = Some(self.jj_consume_token(START_ELEMENT)?);
        'label_1: loop {
            match self.ntk()? {
                START_DESIGNLAYER | START_DECLARATIONS | START_STYLE | START_SCRIPT | START_METADATA | START_STATE | START_MODEL | START_XML | START_XMLLIST | START_ARRAY | START_VECTOR | START_BINDING | START_STRING | START_NUMBER | START_INT | START_UINT | START_BOOLEAN | START_CLASS | START_FUNCTION | START_REMOTEOBJECT | START_WEBSERVICE | START_HTTPSERVICE | START_COMPONENT | START_REPARENT | START_PRIVATE | START_LIBRARY | START_ELEMENT | CDATA => {
                }
                _ => {
                    self.jj_la1[0] = self.jj_gen;
                    break 'label_1;
                }
            }
            match self.ntk()? {
                CDATA => {
                    child = Some(self.jj_consume_token(CDATA)?);
                }
                START_DESIGNLAYER => {
                    child = Some(self.parse_design_layer()?);
                }
                START_STYLE | START_SCRIPT | START_METADATA => {
                    child = Some(self.parse_language_component()?);
                }
                START_DECLARATIONS => {
                    child = Some(self.parse_declarations()?);
                }
                START_LIBRARY => {
                    child = Some(self.parse_library()?);
                }
                START_REMOTEOBJECT | START_WEBSERVICE | START_HTTPSERVICE => {
                    child = Some(self.parse_service_component()?);
                }
                START_STATE | START_MODEL | START_XML | START_XMLLIST | START_ARRAY | START_VECTOR | START_BINDING | START_STRING | START_NUMBER | START_INT | START_UINT | START_BOOLEAN | START_CLASS | START_FUNCTION | START_COMPONENT | START_REPARENT => {
                    child = Some(self.parse_faceless_component()?);
                }
                START_ELEMENT => {
                    child = Some(self.parse_ui_component()?);
                }
                START_PRIVATE => {
                    child = Some(self.parse_private()?);
                }
                _ => {
                    self.jj_la1[1] = self.jj_gen;
                    return Err(self.fail());
                }
            }
            children = add_child(children, child.clone().unwrap());
        }
        self.jj_consume_token(END_ELEMENT)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_application(app.as_ref().unwrap(), children.clone());
        }
        return Ok(app.unwrap());
    }

    pub fn parse_ui_component(&mut self) -> PResult<Tok> {
        let mut comp: Option<Tok> = None;
        let mut child: Option<Tok> = None;
        let mut children: Option<Vec<Tok>> = None;
        comp = Some(self.jj_consume_token(START_ELEMENT)?);
        'label_2: loop {
            match self.ntk()? {
                START_DESIGNLAYER | START_STYLE | START_SCRIPT | START_METADATA | START_STATE | START_MODEL | START_XML | START_XMLLIST | START_ARRAY | START_VECTOR | START_BINDING | START_STRING | START_NUMBER | START_INT | START_UINT | START_BOOLEAN | START_CLASS | START_FUNCTION | START_COMPONENT | START_REPARENT | START_ELEMENT | CDATA => {
                }
                _ => {
                    self.jj_la1[2] = self.jj_gen;
                    break 'label_2;
                }
            }
            match self.ntk()? {
                CDATA => {
                    child = Some(self.jj_consume_token(CDATA)?);
                }
                START_DESIGNLAYER => {
                    child = Some(self.parse_design_layer()?);
                }
                START_STYLE | START_SCRIPT | START_METADATA => {
                    child = Some(self.parse_language_component()?);
                }
                START_STATE | START_MODEL | START_XML | START_XMLLIST | START_ARRAY | START_VECTOR | START_BINDING | START_STRING | START_NUMBER | START_INT | START_UINT | START_BOOLEAN | START_CLASS | START_FUNCTION | START_COMPONENT | START_REPARENT => {
                    child = Some(self.parse_faceless_component()?);
                }
                START_ELEMENT => {
                    child = Some(self.parse_ui_component()?);
                }
                _ => {
                    self.jj_la1[3] = self.jj_gen;
                    return Err(self.fail());
                }
            }
            children = add_child(children, child.clone().unwrap());
        }
        self.jj_consume_token(END_ELEMENT)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_component(comp.as_ref().unwrap(), children.clone());
        }
        return Ok(comp.unwrap());
    }

    pub fn parse_design_layer(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut child: Option<Tok> = None;
        let mut children: Option<Vec<Tok>> = None;
        t = Some(self.jj_consume_token(START_DESIGNLAYER)?);
        'label_3: loop {
            match self.ntk()? {
                START_DESIGNLAYER | START_STYLE | START_SCRIPT | START_METADATA | START_STATE | START_MODEL | START_XML | START_XMLLIST | START_ARRAY | START_VECTOR | START_BINDING | START_STRING | START_NUMBER | START_INT | START_UINT | START_BOOLEAN | START_CLASS | START_FUNCTION | START_COMPONENT | START_REPARENT | START_ELEMENT | CDATA => {
                }
                _ => {
                    self.jj_la1[4] = self.jj_gen;
                    break 'label_3;
                }
            }
            match self.ntk()? {
                CDATA => {
                    child = Some(self.jj_consume_token(CDATA)?);
                }
                START_DESIGNLAYER => {
                    child = Some(self.parse_design_layer()?);
                }
                START_STYLE | START_SCRIPT | START_METADATA => {
                    child = Some(self.parse_language_component()?);
                }
                START_STATE | START_MODEL | START_XML | START_XMLLIST | START_ARRAY | START_VECTOR | START_BINDING | START_STRING | START_NUMBER | START_INT | START_UINT | START_BOOLEAN | START_CLASS | START_FUNCTION | START_COMPONENT | START_REPARENT => {
                    child = Some(self.parse_faceless_component()?);
                }
                START_ELEMENT => {
                    child = Some(self.parse_ui_component()?);
                }
                _ => {
                    self.jj_la1[5] = self.jj_gen;
                    return Err(self.fail());
                }
            }
            children = add_child(children, child.clone().unwrap());
        }
        self.jj_consume_token(END_DESIGNLAYER)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_design_layer(t.as_ref().unwrap(), children.clone());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_language_component(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        match self.ntk()? {
            START_METADATA => {
                t = Some(self.parse_meta_data()?);
            }
            START_SCRIPT => {
                t = Some(self.parse_script()?);
            }
            START_STYLE => {
                t = Some(self.parse_style()?);
            }
            _ => {
                self.jj_la1[6] = self.jj_gen;
                return Err(self.fail());
            }
        }
        return Ok(t.unwrap());
    }

    pub fn parse_declarations(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut child: Option<Tok> = None;
        let mut children: Option<Vec<Tok>> = None;
        t = Some(self.jj_consume_token(START_DECLARATIONS)?);
        'label_4: loop {
            match self.ntk()? {
                START_DESIGNLAYER | START_STATE | START_MODEL | START_XML | START_XMLLIST | START_ARRAY | START_VECTOR | START_BINDING | START_STRING | START_NUMBER | START_INT | START_UINT | START_BOOLEAN | START_CLASS | START_FUNCTION | START_REMOTEOBJECT | START_WEBSERVICE | START_HTTPSERVICE | START_COMPONENT | START_REPARENT | START_ELEMENT => {
                }
                _ => {
                    self.jj_la1[7] = self.jj_gen;
                    break 'label_4;
                }
            }
            match self.ntk()? {
                START_REMOTEOBJECT | START_WEBSERVICE | START_HTTPSERVICE => {
                    child = Some(self.parse_service_component()?);
                }
                START_STATE | START_MODEL | START_XML | START_XMLLIST | START_ARRAY | START_VECTOR | START_BINDING | START_STRING | START_NUMBER | START_INT | START_UINT | START_BOOLEAN | START_CLASS | START_FUNCTION | START_COMPONENT | START_REPARENT => {
                    child = Some(self.parse_faceless_component()?);
                }
                START_ELEMENT => {
                    child = Some(self.parse_ui_component()?);
                }
                START_DESIGNLAYER => {
                    child = Some(self.parse_design_layer()?);
                }
                _ => {
                    self.jj_la1[8] = self.jj_gen;
                    return Err(self.fail());
                }
            }
            children = add_child(children, child.clone().unwrap());
        }
        self.jj_consume_token(END_DECLARATIONS)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_declarations(t.as_ref().unwrap(), children.clone());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_service_component(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        match self.ntk()? {
            START_WEBSERVICE => {
                t = Some(self.parse_web_service()?);
            }
            START_HTTPSERVICE => {
                t = Some(self.parse_http_service()?);
            }
            START_REMOTEOBJECT => {
                t = Some(self.parse_remote_object()?);
            }
            _ => {
                self.jj_la1[9] = self.jj_gen;
                return Err(self.fail());
            }
        }
        return Ok(t.unwrap());
    }

    pub fn parse_style(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut d: Option<Tok> = None;
        t = Some(self.jj_consume_token(START_STYLE)?);
        match self.ntk()? {
            CDATA => {
                d = Some(self.jj_consume_token(CDATA)?);
            }
            _ => {
                self.jj_la1[10] = self.jj_gen;
            }
        }
        self.jj_consume_token(END_STYLE)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_style(t.as_ref().unwrap(), d.as_ref());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_script(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut d: Option<Tok> = None;
        t = Some(self.jj_consume_token(START_SCRIPT)?);
        match self.ntk()? {
            CDATA => {
                d = Some(self.jj_consume_token(CDATA)?);
            }
            _ => {
                self.jj_la1[11] = self.jj_gen;
            }
        }
        self.jj_consume_token(END_SCRIPT)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_script(t.as_ref().unwrap(), d.as_ref());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_meta_data(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut d: Option<Tok> = None;
        t = Some(self.jj_consume_token(START_METADATA)?);
        match self.ntk()? {
            CDATA => {
                d = Some(self.jj_consume_token(CDATA)?);
            }
            _ => {
                self.jj_la1[12] = self.jj_gen;
            }
        }
        self.jj_consume_token(END_METADATA)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_meta_data(t.as_ref().unwrap(), d.as_ref());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_faceless_component(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        match self.ntk()? {
            START_MODEL => {
                t = Some(self.parse_model()?);
            }
            START_XML => {
                t = Some(self.parse_xml()?);
            }
            START_XMLLIST => {
                t = Some(self.parse_xml_list()?);
            }
            START_ARRAY => {
                t = Some(self.parse_array()?);
            }
            START_VECTOR => {
                t = Some(self.parse_vector()?);
            }
            START_BINDING => {
                t = Some(self.parse_binding()?);
            }
            START_STRING | START_NUMBER | START_INT | START_UINT | START_BOOLEAN | START_CLASS | START_FUNCTION => {
                t = Some(self.parse_primitive()?);
            }
            START_COMPONENT => {
                t = Some(self.parse_inline_component()?);
            }
            START_STATE => {
                t = Some(self.parse_state()?);
            }
            START_REPARENT => {
                t = Some(self.parse_reparent()?);
            }
            _ => {
                self.jj_la1[13] = self.jj_gen;
                return Err(self.fail());
            }
        }
        return Ok(t.unwrap());
    }

    pub fn parse_state(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut child: Option<Tok> = None;
        let mut children: Option<Vec<Tok>> = None;
        t = Some(self.jj_consume_token(START_STATE)?);
        'label_5: loop {
            match self.ntk()? {
                START_DESIGNLAYER | START_STATE | START_MODEL | START_XML | START_XMLLIST | START_ARRAY | START_VECTOR | START_BINDING | START_STRING | START_NUMBER | START_INT | START_UINT | START_BOOLEAN | START_CLASS | START_FUNCTION | START_COMPONENT | START_REPARENT | START_ELEMENT => {
                }
                _ => {
                    self.jj_la1[14] = self.jj_gen;
                    break 'label_5;
                }
            }
            match self.ntk()? {
                START_DESIGNLAYER => {
                    child = Some(self.parse_design_layer()?);
                }
                START_STATE | START_MODEL | START_XML | START_XMLLIST | START_ARRAY | START_VECTOR | START_BINDING | START_STRING | START_NUMBER | START_INT | START_UINT | START_BOOLEAN | START_CLASS | START_FUNCTION | START_COMPONENT | START_REPARENT => {
                    child = Some(self.parse_faceless_component()?);
                }
                START_ELEMENT => {
                    child = Some(self.parse_ui_component()?);
                }
                _ => {
                    self.jj_la1[15] = self.jj_gen;
                    return Err(self.fail());
                }
            }
            children = add_child(children, child.clone().unwrap());
        }
        self.jj_consume_token(END_STATE)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_state(t.as_ref().unwrap(), children.clone());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_model(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut obj: Option<Tok> = None;
        let mut children: Option<Vec<Tok>> = None;
        t = Some(self.jj_consume_token(START_MODEL)?);
        match self.ntk()? {
            CDATA => {
                obj = Some(self.jj_consume_token(CDATA)?);
                children = add_child(children, obj.clone().unwrap());
            }
            _ => {
                self.jj_la1[18] = self.jj_gen;
                'label_6: loop {
                    match self.ntk()? {
                        START_ARRAY | START_VECTOR | START_STRING | START_NUMBER | START_INT | START_UINT | START_BOOLEAN | START_CLASS | START_FUNCTION | START_ELEMENT => {
                        }
                        _ => {
                            self.jj_la1[16] = self.jj_gen;
                            break 'label_6;
                        }
                    }
                    match self.ntk()? {
                        START_STRING | START_NUMBER | START_INT | START_UINT | START_BOOLEAN | START_CLASS | START_FUNCTION => {
                            obj = Some(self.parse_primitive()?);
                        }
                        START_ARRAY => {
                            obj = Some(self.parse_array()?);
                        }
                        START_VECTOR => {
                            obj = Some(self.parse_vector()?);
                        }
                        START_ELEMENT => {
                            obj = Some(self.parse_anonymous_object()?);
                        }
                        _ => {
                            self.jj_la1[17] = self.jj_gen;
                            return Err(self.fail());
                        }
                    }
                    children = add_child(children, obj.clone().unwrap());
                }
            }
        }
        self.jj_consume_token(END_MODEL)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_model(t.as_ref().unwrap(), children.clone());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_xml(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut obj: Option<Tok> = None;
        let mut children: Option<Vec<Tok>> = None;
        t = Some(self.jj_consume_token(START_XML)?);
        match self.ntk()? {
            CDATA => {
                obj = Some(self.jj_consume_token(CDATA)?);
                children = add_child(children, obj.clone().unwrap());
            }
            _ => {
                self.jj_la1[20] = self.jj_gen;
                'label_7: loop {
                    match self.ntk()? {
                        START_ELEMENT => {
                        }
                        _ => {
                            self.jj_la1[19] = self.jj_gen;
                            break 'label_7;
                        }
                    }
                    obj = Some(self.parse_anonymous_object()?);
                    children = add_child(children, obj.clone().unwrap());
                }
            }
        }
        self.jj_consume_token(END_XML)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_xml(t.as_ref().unwrap(), children.clone());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_xml_list(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut obj: Option<Tok> = None;
        let mut children: Option<Vec<Tok>> = None;
        t = Some(self.jj_consume_token(START_XMLLIST)?);
        match self.ntk()? {
            CDATA => {
                obj = Some(self.jj_consume_token(CDATA)?);
                children = add_child(children, obj.clone().unwrap());
            }
            _ => {
                self.jj_la1[22] = self.jj_gen;
                'label_8: loop {
                    match self.ntk()? {
                        START_ELEMENT => {
                        }
                        _ => {
                            self.jj_la1[21] = self.jj_gen;
                            break 'label_8;
                        }
                    }
                    obj = Some(self.parse_anonymous_object()?);
                    children = add_child(children, obj.clone().unwrap());
                }
            }
        }
        self.jj_consume_token(END_XMLLIST)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_xml_list(t.as_ref().unwrap(), children.clone());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_array(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut obj: Option<Tok> = None;
        let mut children: Option<Vec<Tok>> = None;
        t = Some(self.jj_consume_token(START_ARRAY)?);
        'label_9: loop {
            match self.ntk()? {
                START_DESIGNLAYER | START_STATE | START_ARRAY | START_VECTOR | START_STRING | START_NUMBER | START_INT | START_UINT | START_BOOLEAN | START_CLASS | START_FUNCTION | START_REPARENT | START_ELEMENT | CDATA => {
                }
                _ => {
                    self.jj_la1[23] = self.jj_gen;
                    break 'label_9;
                }
            }
            match self.ntk()? {
                CDATA => {
                    obj = Some(self.jj_consume_token(CDATA)?);
                }
                START_DESIGNLAYER => {
                    obj = Some(self.parse_design_layer()?);
                }
                START_STRING | START_NUMBER | START_INT | START_UINT | START_BOOLEAN | START_CLASS | START_FUNCTION => {
                    obj = Some(self.parse_primitive()?);
                }
                START_ARRAY => {
                    obj = Some(self.parse_array()?);
                }
                START_VECTOR => {
                    obj = Some(self.parse_vector()?);
                }
                START_ELEMENT => {
                    obj = Some(self.parse_ui_component()?);
                }
                START_REPARENT => {
                    obj = Some(self.parse_reparent()?);
                }
                START_STATE => {
                    obj = Some(self.parse_state()?);
                }
                _ => {
                    self.jj_la1[24] = self.jj_gen;
                    return Err(self.fail());
                }
            }
            children = add_child(children, obj.clone().unwrap());
        }
        self.jj_consume_token(END_ARRAY)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_array(t.as_ref().unwrap(), children.clone());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_vector(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut obj: Option<Tok> = None;
        let mut children: Option<Vec<Tok>> = None;
        t = Some(self.jj_consume_token(START_VECTOR)?);
        'label_10: loop {
            match self.ntk()? {
                START_DESIGNLAYER | START_STATE | START_ARRAY | START_VECTOR | START_STRING | START_NUMBER | START_INT | START_UINT | START_BOOLEAN | START_CLASS | START_FUNCTION | START_REPARENT | START_ELEMENT | CDATA => {
                }
                _ => {
                    self.jj_la1[25] = self.jj_gen;
                    break 'label_10;
                }
            }
            match self.ntk()? {
                CDATA => {
                    obj = Some(self.jj_consume_token(CDATA)?);
                }
                START_DESIGNLAYER => {
                    obj = Some(self.parse_design_layer()?);
                }
                START_STRING | START_NUMBER | START_INT | START_UINT | START_BOOLEAN | START_CLASS | START_FUNCTION => {
                    obj = Some(self.parse_primitive()?);
                }
                START_ARRAY => {
                    obj = Some(self.parse_array()?);
                }
                START_VECTOR => {
                    obj = Some(self.parse_vector()?);
                }
                START_ELEMENT => {
                    obj = Some(self.parse_ui_component()?);
                }
                START_REPARENT => {
                    obj = Some(self.parse_reparent()?);
                }
                START_STATE => {
                    obj = Some(self.parse_state()?);
                }
                _ => {
                    self.jj_la1[26] = self.jj_gen;
                    return Err(self.fail());
                }
            }
            children = add_child(children, obj.clone().unwrap());
        }
        self.jj_consume_token(END_VECTOR)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_vector(t.as_ref().unwrap(), children.clone());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_binding(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        t = Some(self.jj_consume_token(START_BINDING)?);
        self.jj_consume_token(END_BINDING)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_binding(t.as_ref().unwrap());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_anonymous_object(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut obj: Option<Tok> = None;
        let mut children: Option<Vec<Tok>> = None;
        t = Some(self.jj_consume_token(START_ELEMENT)?);
        'label_11: loop {
            match self.ntk()? {
                START_ELEMENT | CDATA => {
                }
                _ => {
                    self.jj_la1[27] = self.jj_gen;
                    break 'label_11;
                }
            }
            match self.ntk()? {
                CDATA => {
                    obj = Some(self.jj_consume_token(CDATA)?);
                }
                START_ELEMENT => {
                    obj = Some(self.parse_anonymous_object()?);
                }
                _ => {
                    self.jj_la1[28] = self.jj_gen;
                    return Err(self.fail());
                }
            }
            children = add_child(children, obj.clone().unwrap());
        }
        self.jj_consume_token(END_ELEMENT)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_anonymous_object(t.as_ref().unwrap(), children.clone());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_primitive(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        match self.ntk()? {
            START_STRING => {
                t = Some(self.parse_string()?);
            }
            START_NUMBER => {
                t = Some(self.parse_number()?);
            }
            START_INT => {
                t = Some(self.parse_int()?);
            }
            START_UINT => {
                t = Some(self.parse_uint()?);
            }
            START_BOOLEAN => {
                t = Some(self.parse_boolean()?);
            }
            START_CLASS => {
                t = Some(self.parse_class()?);
            }
            START_FUNCTION => {
                t = Some(self.parse_function()?);
            }
            _ => {
                self.jj_la1[29] = self.jj_gen;
                return Err(self.fail());
            }
        }
        return Ok(t.unwrap());
    }

    pub fn parse_string(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut data: Option<Tok> = None;
        t = Some(self.jj_consume_token(START_STRING)?);
        match self.ntk()? {
            CDATA => {
                data = Some(self.jj_consume_token(CDATA)?);
            }
            _ => {
                self.jj_la1[30] = self.jj_gen;
            }
        }
        self.jj_consume_token(END_STRING)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_string(t.as_ref().unwrap(), data.as_ref());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_number(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut data: Option<Tok> = None;
        t = Some(self.jj_consume_token(START_NUMBER)?);
        match self.ntk()? {
            CDATA => {
                data = Some(self.jj_consume_token(CDATA)?);
            }
            _ => {
                self.jj_la1[31] = self.jj_gen;
            }
        }
        self.jj_consume_token(END_NUMBER)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_number(t.as_ref().unwrap(), data.as_ref());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_int(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut data: Option<Tok> = None;
        t = Some(self.jj_consume_token(START_INT)?);
        match self.ntk()? {
            CDATA => {
                data = Some(self.jj_consume_token(CDATA)?);
            }
            _ => {
                self.jj_la1[32] = self.jj_gen;
            }
        }
        self.jj_consume_token(END_INT)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_int(t.as_ref().unwrap(), data.as_ref());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_uint(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut data: Option<Tok> = None;
        t = Some(self.jj_consume_token(START_UINT)?);
        match self.ntk()? {
            CDATA => {
                data = Some(self.jj_consume_token(CDATA)?);
            }
            _ => {
                self.jj_la1[33] = self.jj_gen;
            }
        }
        self.jj_consume_token(END_UINT)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_uint(t.as_ref().unwrap(), data.as_ref());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_boolean(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut data: Option<Tok> = None;
        t = Some(self.jj_consume_token(START_BOOLEAN)?);
        match self.ntk()? {
            CDATA => {
                data = Some(self.jj_consume_token(CDATA)?);
            }
            _ => {
                self.jj_la1[34] = self.jj_gen;
            }
        }
        self.jj_consume_token(END_BOOLEAN)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_boolean(t.as_ref().unwrap(), data.as_ref());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_class(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut data: Option<Tok> = None;
        t = Some(self.jj_consume_token(START_CLASS)?);
        match self.ntk()? {
            CDATA => {
                data = Some(self.jj_consume_token(CDATA)?);
            }
            _ => {
                self.jj_la1[35] = self.jj_gen;
            }
        }
        self.jj_consume_token(END_CLASS)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_class(t.as_ref().unwrap(), data.as_ref());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_function(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut data: Option<Tok> = None;
        t = Some(self.jj_consume_token(START_FUNCTION)?);
        match self.ntk()? {
            CDATA => {
                data = Some(self.jj_consume_token(CDATA)?);
            }
            _ => {
                self.jj_la1[36] = self.jj_gen;
            }
        }
        self.jj_consume_token(END_FUNCTION)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_function(t.as_ref().unwrap(), data.as_ref());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_remote_object(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut child: Option<Tok> = None;
        let mut children: Option<Vec<Tok>> = None;
        t = Some(self.jj_consume_token(START_REMOTEOBJECT)?);
        'label_12: loop {
            match self.ntk()? {
                START_METHOD | START_ELEMENT => {
                }
                _ => {
                    self.jj_la1[37] = self.jj_gen;
                    break 'label_12;
                }
            }
            match self.ntk()? {
                START_METHOD => {
                    child = Some(self.parse_method()?);
                }
                START_ELEMENT => {
                    child = Some(self.parse_ui_component()?);
                }
                _ => {
                    self.jj_la1[38] = self.jj_gen;
                    return Err(self.fail());
                }
            }
            children = add_child(children, child.clone().unwrap());
        }
        self.jj_consume_token(END_REMOTEOBJECT)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_remote_object(t.as_ref().unwrap(), children.clone());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_method(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut child: Option<Tok> = None;
        let mut children: Option<Vec<Tok>> = None;
        t = Some(self.jj_consume_token(START_METHOD)?);
        'label_13: loop {
            match self.ntk()? {
                START_ARGUMENTS | START_ELEMENT => {
                }
                _ => {
                    self.jj_la1[39] = self.jj_gen;
                    break 'label_13;
                }
            }
            match self.ntk()? {
                START_ARGUMENTS => {
                    child = Some(self.parse_arguments()?);
                }
                START_ELEMENT => {
                    child = Some(self.parse_ui_component()?);
                }
                _ => {
                    self.jj_la1[40] = self.jj_gen;
                    return Err(self.fail());
                }
            }
            children = add_child(children, child.clone().unwrap());
        }
        self.jj_consume_token(END_METHOD)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_method(t.as_ref().unwrap(), children.clone());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_arguments(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut child: Option<Tok> = None;
        let mut children: Option<Vec<Tok>> = None;
        t = Some(self.jj_consume_token(START_ARGUMENTS)?);
        match self.ntk()? {
            CDATA => {
                child = Some(self.jj_consume_token(CDATA)?);
                children = add_child(children, child.clone().unwrap());
            }
            _ => {
                self.jj_la1[43] = self.jj_gen;
                'label_14: loop {
                    match self.ntk()? {
                        START_ARRAY | START_STRING | START_NUMBER | START_INT | START_UINT | START_BOOLEAN | START_CLASS | START_FUNCTION | START_ELEMENT => {
                        }
                        _ => {
                            self.jj_la1[41] = self.jj_gen;
                            break 'label_14;
                        }
                    }
                    match self.ntk()? {
                        START_STRING | START_NUMBER | START_INT | START_UINT | START_BOOLEAN | START_CLASS | START_FUNCTION => {
                            child = Some(self.parse_primitive()?);
                        }
                        START_ARRAY => {
                            child = Some(self.parse_array()?);
                        }
                        START_ELEMENT => {
                            child = Some(self.parse_anonymous_object()?);
                        }
                        _ => {
                            self.jj_la1[42] = self.jj_gen;
                            return Err(self.fail());
                        }
                    }
                    children = add_child(children, child.clone().unwrap());
                }
            }
        }
        self.jj_consume_token(END_ARGUMENTS)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_arguments(t.as_ref().unwrap(), children.clone());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_web_service(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut child: Option<Tok> = None;
        let mut children: Option<Vec<Tok>> = None;
        t = Some(self.jj_consume_token(START_WEBSERVICE)?);
        'label_15: loop {
            match self.ntk()? {
                START_OPERATION | START_ELEMENT => {
                }
                _ => {
                    self.jj_la1[44] = self.jj_gen;
                    break 'label_15;
                }
            }
            match self.ntk()? {
                START_OPERATION => {
                    child = Some(self.parse_operation()?);
                }
                START_ELEMENT => {
                    child = Some(self.parse_ui_component()?);
                }
                _ => {
                    self.jj_la1[45] = self.jj_gen;
                    return Err(self.fail());
                }
            }
            children = add_child(children, child.clone().unwrap());
        }
        self.jj_consume_token(END_WEBSERVICE)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_web_service(t.as_ref().unwrap(), children.clone());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_operation(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut child: Option<Tok> = None;
        let mut children: Option<Vec<Tok>> = None;
        t = Some(self.jj_consume_token(START_OPERATION)?);
        'label_16: loop {
            match self.ntk()? {
                START_REQUEST | START_ELEMENT => {
                }
                _ => {
                    self.jj_la1[46] = self.jj_gen;
                    break 'label_16;
                }
            }
            match self.ntk()? {
                START_REQUEST => {
                    child = Some(self.parse_request()?);
                }
                START_ELEMENT => {
                    child = Some(self.parse_ui_component()?);
                }
                _ => {
                    self.jj_la1[47] = self.jj_gen;
                    return Err(self.fail());
                }
            }
            children = add_child(children, child.clone().unwrap());
        }
        self.jj_consume_token(END_OPERATION)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_operation(t.as_ref().unwrap(), children.clone());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_request(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut child: Option<Tok> = None;
        let mut children: Option<Vec<Tok>> = None;
        t = Some(self.jj_consume_token(START_REQUEST)?);
        match self.ntk()? {
            CDATA => {
                child = Some(self.jj_consume_token(CDATA)?);
                children = add_child(children, child.clone().unwrap());
            }
            _ => {
                self.jj_la1[49] = self.jj_gen;
                'label_17: loop {
                    match self.ntk()? {
                        START_ELEMENT => {
                        }
                        _ => {
                            self.jj_la1[48] = self.jj_gen;
                            break 'label_17;
                        }
                    }
                    child = Some(self.parse_anonymous_object()?);
                    children = add_child(children, child.clone().unwrap());
                }
            }
        }
        self.jj_consume_token(END_REQUEST)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_request(t.as_ref().unwrap(), children.clone());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_http_service(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        let mut child: Option<Tok> = None;
        let mut children: Option<Vec<Tok>> = None;
        t = Some(self.jj_consume_token(START_HTTPSERVICE)?);
        'label_18: loop {
            match self.ntk()? {
                START_REQUEST | START_ELEMENT => {
                }
                _ => {
                    self.jj_la1[50] = self.jj_gen;
                    break 'label_18;
                }
            }
            match self.ntk()? {
                START_REQUEST => {
                    child = Some(self.parse_request()?);
                }
                START_ELEMENT => {
                    child = Some(self.parse_ui_component()?);
                }
                _ => {
                    self.jj_la1[51] = self.jj_gen;
                    return Err(self.fail());
                }
            }
            children = add_child(children, child.clone().unwrap());
        }
        self.jj_consume_token(END_HTTPSERVICE)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_http_service(t.as_ref().unwrap(), children.clone());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_inline_component(&mut self) -> PResult<Tok> {
        let mut comp: Option<Tok> = None;
        let mut child: Option<Tok> = None;
        comp = Some(self.jj_consume_token(START_COMPONENT)?);
        match self.ntk()? {
            START_DESIGNLAYER | START_ELEMENT => {
                match self.ntk()? {
                    START_DESIGNLAYER => {
                        child = Some(self.parse_design_layer()?);
                    }
                    START_ELEMENT => {
                        child = Some(self.parse_application_component()?);
                    }
                    _ => {
                        self.jj_la1[52] = self.jj_gen;
                        return Err(self.fail());
                    }
                }
            }
            _ => {
                self.jj_la1[53] = self.jj_gen;
            }
        }
        self.jj_consume_token(END_COMPONENT)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_inline_component(comp.as_ref().unwrap(), child.as_ref());
        }
        return Ok(comp.unwrap());
    }

    pub fn parse_reparent(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        t = Some(self.jj_consume_token(START_REPARENT)?);
        self.jj_consume_token(END_REPARENT)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_reparent(t.as_ref().unwrap());
        }
        return Ok(t.unwrap());
    }

    pub fn parse_private(&mut self) -> PResult<Tok> {
        let mut t: Option<Tok> = None;
        t = Some(self.jj_consume_token(START_PRIVATE)?);
        'label_19: loop {
            match self.ntk()? {
                CDATA | SKIP_ELEMENT => {
                }
                _ => {
                    self.jj_la1[54] = self.jj_gen;
                    break 'label_19;
                }
            }
            match self.ntk()? {
                SKIP_ELEMENT => {
                    self.jj_consume_token(SKIP_ELEMENT)?;
                }
                CDATA => {
                    self.jj_consume_token(CDATA)?;
                }
                _ => {
                    self.jj_la1[55] = self.jj_gen;
                    return Err(self.fail());
                }
            }
        }
        self.jj_consume_token(END_PRIVATE)?;
        return Ok(t.unwrap());
    }

    pub fn parse_library(&mut self) -> PResult<Tok> {
        let mut lib: Option<Tok> = None;
        let mut child: Option<Tok> = None;
        let mut children: Option<Vec<Tok>> = None;
        lib = Some(self.jj_consume_token(START_LIBRARY)?);
        'label_20: loop {
            match self.ntk()? {
                START_DEFINITION => {
                }
                _ => {
                    self.jj_la1[56] = self.jj_gen;
                    break 'label_20;
                }
            }
            child = Some(self.parse_definition()?);
            children = add_child(children, child.clone().unwrap());
        }
        self.jj_consume_token(END_LIBRARY)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_library(lib.as_ref().unwrap(), children.clone());
        }
        return Ok(lib.unwrap());
    }

    pub fn parse_definition(&mut self) -> PResult<Tok> {
        let mut definition: Option<Tok> = None;
        let mut child: Option<Tok> = None;
        definition = Some(self.jj_consume_token(START_DEFINITION)?);
        match self.ntk()? {
            START_DESIGNLAYER | START_ELEMENT => {
                match self.ntk()? {
                    START_DESIGNLAYER => {
                        child = Some(self.parse_design_layer()?);
                    }
                    START_ELEMENT => {
                        child = Some(self.parse_application_component()?);
                    }
                    _ => {
                        self.jj_la1[57] = self.jj_gen;
                        return Err(self.fail());
                    }
                }
            }
            _ => {
                self.jj_la1[58] = self.jj_gen;
            }
        }
        self.jj_consume_token(END_DEFINITION)?;
        if let Some(v) = self.visitor.as_mut() {
            v.parse_definition(definition.as_ref().unwrap(), child.as_ref());
        }
        return Ok(definition.unwrap());
    }
}
