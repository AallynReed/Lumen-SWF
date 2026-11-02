//! Port of `flex2.compiler.mxml.dom.MxmlScanner`: the JavaCC token manager that runs the whole SAX parse up front and
//! hands out its tokens and errors. Attribute lines are the element's begin line, as the stock Xerces 2.9.1 parser
//! `SAXParserFactory` picks under Java 17 has no `AttributesProxy.getLineNumber`.

use std::collections::HashMap;

use super::node::{Node, NodeClass, NodeRef, PrimitiveClass};
use super::scanner_error::ScannerError;
use super::xerces::{self, Locator, ParseFailure, SaxAttr, SaxHandler};
use crate::asc::flex2::compiler::mxml::attribute::AttrValue;
use crate::asc::flex2::compiler::mxml::lang::standard_defs::{SPARK_TEXT_ATTRIBUTE, SPARK_TEXT_CONTENT_PROPERTIES, SPARK_TEXT_TAGS};
use crate::asc::flex2::compiler::mxml::mxml_namespaces::{FXG_2008_NAMESPACE, MXML_2006_NAMESPACE, MXML_2009_NAMESPACE, SPARK_NAMESPACE};
use crate::asc::flex2::compiler::mxml::parser::{self as pc, find_element_type, PResult, ParserError, TokenManager};
use crate::asc::flex2::compiler::mxml::token::{Tok, Token};
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};

pub const MARKUP_NOT_RECOGNIZED_IN_CONTENT: &str = "The content of elements must consist of well-formed character data or markup.";
pub const RESERVED_PI_TARGET: &str = "The processing instruction target matching \"[xX][mM][lL]\" is not allowed.";
pub const MARKUP_NOT_RECOGNIZED_IN_MISC: &str = "The markup in the document following the root element must be well-formed.";
pub const ETAG_REQUIRED: &str = "The element type \"{0}\" must be terminated by the matching end-tag \"</{0}>\".";

const LEX_FLEX2: i32 = 0;
const LEX_FLEX1_WEBSERVICE: i32 = 1;
const LEX_FLEX1_HTTPSERVICE: i32 = 2;
const LEX_FLEX1_REMOTEOBJECT: i32 = 3;
const LEX_FLEX2_PRIVATE: i32 = 4;

enum SaxEvent {
    Token(Tok),
    Error(ScannerError),
}

/// Why `new MxmlScanner(in, ...)` threw.
pub enum ScannerFailure {
    Scanner(ScannerError),
    /// A Java runtime exception out of a SAX callback (`EmptyStackException` has no message).
    Runtime(Option<String>),
}

pub struct MxmlScanner {
    language_namespace: Option<String>,
    language_version: i32,
    sax_events: Vec<SaxEvent>,
    element_stack: Vec<NodeRef>,
    in_cdata: bool,
    comment: Option<String>,
    process_comments: bool,
    begin_line: i32,
    begin_column: i32,
    pos: usize,
    kind: i32,
    process_design_layers: bool,
    state: i32,
    text_depth: i32,
    locator: Option<Locator>,
    prefix_mappings: Option<HashMap<String, String>>,
}

impl MxmlScanner {
    pub fn new(input: Vec<u8>, process_design_layers: bool) -> Result<MxmlScanner, ScannerFailure> {
        MxmlScanner::with_comments(input, process_design_layers, false)
    }

    pub fn with_comments(input: Vec<u8>, process_design_layers: bool, process_comments: bool) -> Result<MxmlScanner, ScannerFailure> {
        let mut s = MxmlScanner {
            language_namespace: None,
            language_version: 0,
            sax_events: Vec::with_capacity(100),
            element_stack: Vec::new(),
            in_cdata: false,
            comment: None,
            process_comments,
            begin_line: 0,
            begin_column: 0,
            pos: 0,
            kind: 0,
            process_design_layers,
            state: LEX_FLEX2,
            text_depth: 0,
            locator: None,
            prefix_mappings: None,
        };
        match xerces::parse(input, &mut s) {
            Ok(()) | Err(ParseFailure::Fatal(_)) | Err(ParseFailure::Io(_)) => Ok(s),
            Err(ParseFailure::Handler(e)) => Err(e),
        }
    }

    fn find_element_type(&mut self, uri: &str, local_name: &str, start: bool) -> i32 {
        let mut kind = find_element_type(uri, local_name, start);
        match self.state {
            LEX_FLEX2 => match kind {
                pc::START_WEBSERVICE => self.state = LEX_FLEX1_WEBSERVICE,
                pc::START_HTTPSERVICE => self.state = LEX_FLEX1_HTTPSERVICE,
                pc::START_REMOTEOBJECT => self.state = LEX_FLEX1_REMOTEOBJECT,
                pc::START_PRIVATE => self.state = LEX_FLEX2_PRIVATE,
                pc::START_OPERATION | pc::START_REQUEST | pc::START_METHOD | pc::START_ARGUMENTS => kind = pc::START_ELEMENT,
                pc::END_OPERATION | pc::END_REQUEST | pc::END_METHOD | pc::END_ARGUMENTS => kind = pc::END_ELEMENT,
                _ => {}
            },
            LEX_FLEX2_PRIVATE => match kind {
                pc::END_PRIVATE => self.state = LEX_FLEX2,
                _ => kind = pc::SKIP_ELEMENT,
            },
            LEX_FLEX1_WEBSERVICE => match kind {
                pc::END_WEBSERVICE => self.state = LEX_FLEX2,
                pc::START_METHOD | pc::START_ARGUMENTS => kind = pc::START_ELEMENT,
                pc::END_METHOD | pc::END_ARGUMENTS => kind = pc::END_ELEMENT,
                _ => {}
            },
            LEX_FLEX1_HTTPSERVICE => match kind {
                pc::END_HTTPSERVICE => self.state = LEX_FLEX2,
                pc::START_OPERATION | pc::START_METHOD | pc::START_ARGUMENTS => kind = pc::START_ELEMENT,
                pc::END_OPERATION | pc::END_METHOD | pc::END_ARGUMENTS => kind = pc::END_ELEMENT,
                _ => {}
            },
            LEX_FLEX1_REMOTEOBJECT => match kind {
                pc::END_REMOTEOBJECT => self.state = LEX_FLEX2,
                pc::START_OPERATION | pc::START_REQUEST => kind = pc::START_ELEMENT,
                pc::END_OPERATION | pc::END_REQUEST => kind = pc::END_ELEMENT,
                _ => {}
            },
            _ => {}
        }
        kind
    }

    fn parent_design_layer(&self) -> Result<Option<NodeRef>, ScannerFailure> {
        match self.element_stack.last() {
            None => Err(ScannerFailure::Runtime(None)),
            Some(p) => Ok(if self.process_design_layers && p.borrow().is_design_layer_node() { Some(p.clone()) } else { None }),
        }
    }

    fn create_node(&self, kind: i32, uri: &str, local_name: &str, attributes: &[SaxAttr]) -> Result<Node, ScannerFailure> {
        let n = attributes.len();
        let prim = |c: PrimitiveClass| Node::with_class(NodeClass::Primitive { class: c, source_file: None }, uri, local_name, n);
        let plain = |c: NodeClass| Node::with_class(c, uri, local_name, n);
        Ok(match kind {
            pc::START_STYLE => plain(NodeClass::Style { style_sheet: None }),
            pc::START_SCRIPT => plain(NodeClass::Script { source_file: None }),
            pc::START_METADATA => plain(NodeClass::MetaData),
            pc::START_MODEL => plain(NodeClass::Model { source_file: None }),
            pc::START_XML => plain(NodeClass::Xml { source_file: None }),
            pc::START_XMLLIST => plain(NodeClass::XmlList),
            pc::START_ARRAY => plain(NodeClass::Array),
            pc::START_VECTOR => plain(NodeClass::Vector),
            pc::START_STRING => prim(PrimitiveClass::String),
            pc::START_NUMBER => prim(PrimitiveClass::Number),
            pc::START_INT => prim(PrimitiveClass::Int),
            pc::START_UINT => prim(PrimitiveClass::UInt),
            pc::START_BOOLEAN => prim(PrimitiveClass::Boolean),
            pc::START_CLASS => prim(PrimitiveClass::Class),
            pc::START_FUNCTION => prim(PrimitiveClass::Function),
            pc::START_WEBSERVICE => plain(NodeClass::WebService),
            pc::START_HTTPSERVICE => plain(NodeClass::HttpService),
            pc::START_REMOTEOBJECT => plain(NodeClass::RemoteObject),
            pc::START_OPERATION => plain(NodeClass::Operation),
            pc::START_REQUEST => {
                if attributes.iter().any(|a| a.uri.is_empty() && a.local == "format" && a.value == "xml") {
                    plain(NodeClass::Xml { source_file: None })
                } else {
                    plain(NodeClass::Request)
                }
            }
            pc::START_METHOD => plain(NodeClass::Method),
            pc::START_ARGUMENTS => plain(NodeClass::Arguments),
            pc::START_BINDING => plain(NodeClass::Binding),
            pc::START_ELEMENT => {
                if self.sax_events.is_empty() {
                    let mut d = Node::new_document(uri, local_name, n);
                    d.set_language_namespace(self.language_namespace.as_deref());
                    d.set_version(self.language_version);
                    d
                } else {
                    match self.parent_design_layer()? {
                        Some(p) => plain(NodeClass::Layered { layer_parent: Some(p) }),
                        None => Node::new(uri, local_name, n),
                    }
                }
            }
            pc::START_COMPONENT => plain(NodeClass::InlineComponent { class_qname: None }),
            pc::START_DESIGNLAYER => {
                let parent = self.parent_design_layer()?;
                plain(NodeClass::DesignLayer { layer_parent: parent })
            }
            pc::START_DECLARATIONS => plain(NodeClass::Declarations),
            pc::START_REPARENT => plain(NodeClass::Reparent),
            pc::START_STATE => plain(NodeClass::State),
            pc::START_PRIVATE => plain(NodeClass::Private),
            pc::START_LIBRARY => plain(NodeClass::Library),
            pc::START_DEFINITION => plain(NodeClass::Definition { name: None }),
            _ => panic!("NullPointerException: no node for token kind {kind}"),
        })
    }

    fn loc(&self) -> Locator {
        self.locator.unwrap_or(Locator { available: false, line: 1, column: 1 })
    }

    /// `assignTokenPosition(t)`: returns (beginLine, beginColumn, endLine, endColumn).
    fn token_position(&mut self) -> (i32, i32, i32, i32) {
        let loc = self.loc();
        if self.sax_events.is_empty() {
            self.begin_line = loc.line;
            self.begin_column = loc.column;
        }
        (self.begin_line, self.begin_column, loc.line, loc.column)
    }

    fn after_event(&mut self) {
        let loc = self.loc();
        self.begin_line = loc.line;
        self.begin_column = loc.column;
    }

    pub fn close(&mut self) {
        self.sax_events.clear();
    }
}

fn language_version(uri: &str) -> i32 {
    if uri == MXML_2009_NAMESPACE || uri == FXG_2008_NAMESPACE {
        4
    } else if uri == MXML_2006_NAMESPACE {
        3
    } else {
        -1
    }
}

fn java_trim_is_empty(s: &str) -> bool {
    s.chars().all(|c| c <= ' ')
}

impl SaxHandler for MxmlScanner {
    type Error = ScannerFailure;

    fn start_document(&mut self, loc: Locator) -> Result<(), ScannerFailure> {
        self.locator = Some(loc);
        self.prefix_mappings = Some(HashMap::new());
        self.text_depth = 0;
        Ok(())
    }

    fn end_document(&mut self, loc: Locator) -> Result<(), ScannerFailure> {
        self.locator = Some(loc);
        let (bl, bc, el, ec) = self.token_position();
        self.sax_events.push(SaxEvent::Token(Tok::plain(Token { kind: pc::EOF, begin_line: bl, begin_column: bc, end_line: el, end_column: ec, image: String::new() })));
        self.kind = pc::EOF;
        self.locator = None;
        self.prefix_mappings = None;
        Ok(())
    }

    fn start_prefix_mapping(&mut self, prefix: &str, uri: &str, loc: Locator) -> Result<(), ScannerFailure> {
        self.locator = Some(loc);
        self.prefix_mappings.as_mut().expect("NullPointerException").insert(uri.to_string(), prefix.to_string());
        let version = language_version(uri);
        if version != -1 {
            if let Some(ns) = &self.language_namespace {
                if ns != uri {
                    let msg = format!("Only one language namespace must be specified per document. Found '{ns}' and '{uri}'.");
                    return Err(ScannerFailure::Scanner(ScannerError::new(loc.line, loc.column, &msg)));
                }
            }
            self.language_namespace = Some(uri.to_string());
            self.language_version = version;
        }
        Ok(())
    }

    fn end_prefix_mapping(&mut self, prefix: &str, loc: Locator) -> Result<(), ScannerFailure> {
        self.locator = Some(loc);
        if let Some(m) = self.prefix_mappings.as_mut() {
            m.remove(prefix);
        }
        Ok(())
    }

    fn start_element(&mut self, uri: &str, local_name: &str, qname: &str, attributes: &[SaxAttr], loc: Locator) -> Result<(), ScannerFailure> {
        self.locator = Some(loc);
        self.kind = self.find_element_type(uri, local_name, true);
        if self.kind == pc::SKIP_ELEMENT {
            return Ok(());
        }
        let mut n = self.create_node(self.kind, uri, local_name, attributes)?;
        let prefix = self.prefix_mappings.as_ref().and_then(|m| m.get(uri).cloned());
        n.add_prefix_mapping(uri, prefix.as_deref());
        let (bl, bc, el, ec) = self.token_position();
        n.begin_line = bl;
        n.begin_column = bc;
        n.end_line = el;
        n.end_column = ec;
        for a in attributes {
            let line = n.begin_line;
            n.add_attribute(&a.uri, &a.local, AttrValue::Str(a.value.clone()), line);
            let prefix = self.prefix_mappings.as_ref().and_then(|m| m.get(&a.uri).cloned());
            n.add_prefix_mapping(&a.uri, prefix.as_deref());
        }
        n.kind = self.kind;
        n.image = format!("<{qname}>");
        if self.comment.is_some() {
            n.comment = self.comment.take();
        }
        if uri == SPARK_NAMESPACE {
            if SPARK_TEXT_TAGS.contains(&local_name) {
                self.text_depth += 1;
                n.process_children_individually = true;
                n.preserve_whitespace = true;
                if attributes.iter().any(|a| (a.uri.is_empty() || a.uri == uri) && a.local == SPARK_TEXT_ATTRIBUTE) {
                    n.preserve_whitespace = false;
                }
            } else if self.text_depth > 0 {
                if SPARK_TEXT_CONTENT_PROPERTIES.contains(&local_name) {
                    n.process_children_individually = true;
                    n.preserve_whitespace = true;
                }
                if let Some(parent) = self.element_stack.last() {
                    parent.borrow_mut().preserve_whitespace = false;
                }
            }
        }
        let n = n.into_ref();
        self.sax_events.push(SaxEvent::Token(Tok::Node(n.clone())));
        self.element_stack.push(n);
        self.after_event();
        Ok(())
    }

    fn end_element(&mut self, uri: &str, local_name: &str, qname: &str, loc: Locator) -> Result<(), ScannerFailure> {
        self.locator = Some(loc);
        let kind = self.find_element_type(uri, local_name, false);
        if kind == pc::SKIP_ELEMENT {
            return Ok(());
        }
        let (bl, bc, el, ec) = self.token_position();
        self.sax_events.push(SaxEvent::Token(Tok::plain(Token { kind, begin_line: bl, begin_column: bc, end_line: el, end_column: ec, image: format!("</{qname}>") })));
        if self.text_depth > 0 && uri == SPARK_NAMESPACE && SPARK_TEXT_TAGS.contains(&local_name) {
            self.text_depth -= 1;
        }
        if let Some(prev) = self.element_stack.last() {
            let matches = {
                let p = prev.borrow();
                p.get_namespace() == uri && p.get_local_part() == local_name
            };
            if matches {
                self.element_stack.pop();
            }
        }
        self.kind = kind;
        self.after_event();
        Ok(())
    }

    fn characters(&mut self, image: &str, loc: Locator) -> Result<(), ScannerFailure> {
        self.locator = Some(loc);
        let last = match self.sax_events.last() {
            Some(SaxEvent::Token(Tok::Node(n))) if n.borrow().is_cdata_node() => Some(n.clone()),
            Some(_) => None,
            None => return Err(ScannerFailure::Runtime(Some("Index -1 out of bounds for length 0".to_string()))),
        };
        let was_in_cdata = last.as_ref().is_some_and(|n| n.borrow().in_cdata());
        let mut cdata = last;
        let mut skip = false;
        if was_in_cdata && self.in_cdata {
        } else if was_in_cdata && !self.in_cdata {
            if !java_trim_is_empty(image) {
                cdata = None;
            } else {
                skip = true;
            }
        } else if !was_in_cdata && self.in_cdata {
            cdata = None;
        } else if self.kind != pc::CDATA && java_trim_is_empty(image) {
            if self.text_depth > 0 {
                if let Some(parent) = self.element_stack.last() {
                    if !parent.borrow().preserve_whitespace {
                        skip = true;
                    }
                }
            } else {
                skip = true;
            }
        }
        if !skip && cdata.is_none() {
            let mut c = Node::new_cdata();
            c.kind = pc::CDATA;
            let (bl, bc, el, ec) = self.token_position();
            c.begin_line = bl;
            c.begin_column = bc;
            c.end_line = el;
            c.end_column = ec;
            c.image = image.to_string();
            c.set_in_cdata(self.in_cdata);
            self.sax_events.push(SaxEvent::Token(Tok::Node(c.into_ref())));
        } else if !skip {
            let c = cdata.unwrap();
            let mut c = c.borrow_mut();
            c.image.push_str(image);
            c.end_line = loc.line;
            c.end_column = loc.column;
        }
        if !skip {
            self.kind = pc::CDATA;
        }
        self.after_event();
        Ok(())
    }

    fn processing_instruction(&mut self, _target: &str, data: &str, loc: Locator) -> Result<(), ScannerFailure> {
        self.locator = Some(loc);
        toolkit::log(CompilerMessage::warning(format!("Unexpected processing instruction encountered and will be ignored. Found: {data}")), None, -1, -1, None);
        Ok(())
    }

    fn start_cdata(&mut self, loc: Locator) -> Result<(), ScannerFailure> {
        self.locator = Some(loc);
        self.in_cdata = true;
        Ok(())
    }

    fn end_cdata(&mut self, loc: Locator) -> Result<(), ScannerFailure> {
        self.locator = Some(loc);
        self.in_cdata = false;
        Ok(())
    }

    fn comment(&mut self, text: &str, loc: Locator) -> Result<(), ScannerFailure> {
        self.locator = Some(loc);
        if !self.process_comments {
            return Ok(());
        }
        if let Some(rest) = text.strip_prefix('-') {
            let mut temp = rest.trim_matches(|c: char| c <= ' ').to_string();
            if let Some(t) = temp.strip_prefix("/**") {
                temp = t.to_string();
            }
            if let Some(t) = temp.strip_suffix("*/") {
                temp = t.to_string();
            }
            let re = regex::Regex::new("(?m)^[ \t\n\x0B\x0C\r]*-").unwrap();
            self.comment = Some(re.replace_all(&temp, "").into_owned());
        }
        Ok(())
    }

    fn fatal_error(&mut self, message: &str, loc: Locator) -> Result<(), ScannerFailure> {
        let (line, col) = if self.locator.is_some() { (loc.line, loc.column) } else { (1, 1) };
        self.sax_events.push(SaxEvent::Error(ScannerError::new(line, col, message)));
        Ok(())
    }
}

impl TokenManager for MxmlScanner {
    fn get_next_token(&mut self) -> PResult<Tok> {
        if self.pos >= self.sax_events.len() {
            return match self.sax_events.last() {
                None => Err(ParserError::Runtime("Index -1 out of bounds for length 0".to_string())),
                Some(SaxEvent::Token(t)) => {
                    let msg = format!("Unexpected end of token stream. The last token was: {}.", t.image());
                    Err(ParserError::Scanner(ScannerError::new(t.begin_line(), t.begin_column(), &msg)))
                }
                Some(SaxEvent::Error(e)) => {
                    let msg = format!("Unexpected end of SAX event stream. The last event was: {}.", e.get_reason());
                    Err(ParserError::Scanner(ScannerError::new(e.get_line_number(), e.get_column_number(), &msg)))
                }
            };
        }
        self.pos += 1;
        match &self.sax_events[self.pos - 1] {
            SaxEvent::Token(t) => Ok(t.clone()),
            SaxEvent::Error(e) => Err(ParserError::Scanner(e.clone())),
        }
    }
}

