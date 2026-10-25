//! Port of `org.apache.flex.forks.batik.css.parser.Parser` (Flex fork: namespaces, media queries, line numbers),
//! with the default handlers and factories, and its `Messages.properties` through `MessageFormat`.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;

use super::conditions::{ConditionRef, DefaultConditionFactory};
use super::css_lexical_unit::{self as clu, LexicalUnitRef};
use super::css_sac_media_list::{CSSSACMediaExpression, CSSSACMediaList, CSSSACMediaQuery};
use super::css_selector_list::CSSSelectorList;
use super::lexical_units as lu;
use super::scanner::{java_to_lower, JavaEqIc, Param, ParseException, Scanner};
use super::scanner_utilities::is_css_hexadecimal_character;
use super::selectors::{DefaultSelectorFactory, SelectorRef};
use crate::asc::flex2::batik::css::sac::lexical_unit::*;
use crate::asc::flex2::batik::css::sac::{CSSException, CSSParseException, CssError, CssResult, DocumentHandler, ErrorHandler};
use crate::asc::util::jstr;

/// `DefaultDocumentHandler`.
pub struct DefaultDocumentHandler;

impl DocumentHandler for DefaultDocumentHandler {
    fn start_document(&mut self) -> CssResult<()> {
        Ok(())
    }
    fn end_document(&mut self) -> CssResult<()> {
        Ok(())
    }
    fn comment(&mut self, _text: &str) -> CssResult<()> {
        Ok(())
    }
    fn ignorable_at_rule(&mut self, _at_rule: &str) -> CssResult<()> {
        Ok(())
    }
    fn namespace_declaration(&mut self, _prefix: Option<&str>, _uri: Option<&str>) -> CssResult<()> {
        Ok(())
    }
    fn import_style(&mut self, _uri: Option<&str>, _media: &CSSSACMediaList, _ns: Option<&str>) -> CssResult<()> {
        Ok(())
    }
    fn start_media(&mut self, _media: &CSSSACMediaList) -> CssResult<()> {
        Ok(())
    }
    fn end_media(&mut self, _media: &CSSSACMediaList) -> CssResult<()> {
        Ok(())
    }
    fn start_page(&mut self, _name: Option<&str>, _pseudo_page: Option<&str>) -> CssResult<()> {
        Ok(())
    }
    fn end_page(&mut self, _name: Option<&str>, _pseudo_page: Option<&str>) -> CssResult<()> {
        Ok(())
    }
    fn start_font_face(&mut self) -> CssResult<()> {
        Ok(())
    }
    fn end_font_face(&mut self) -> CssResult<()> {
        Ok(())
    }
    fn start_selector(&mut self, _selectors: &CSSSelectorList) -> CssResult<()> {
        Ok(())
    }
    fn end_selector(&mut self, _selectors: &CSSSelectorList) -> CssResult<()> {
        Ok(())
    }
    fn property(&mut self, _name: &str, _value: &LexicalUnitRef, _important: bool) -> CssResult<()> {
        Ok(())
    }
}

/// `DefaultErrorHandler`.
pub struct DefaultErrorHandler;

impl ErrorHandler for DefaultErrorHandler {
    fn warning(&mut self, _e: CSSParseException) -> CssResult<()> {
        Ok(())
    }
    fn error(&mut self, _e: CSSParseException) -> CssResult<()> {
        Ok(())
    }
    fn fatal_error(&mut self, e: CSSParseException) -> CssResult<()> {
        Err(CssError::Parse(e))
    }
}

/// `Messages.properties`.
fn bundle(key: &str) -> Option<&'static str> {
    Some(match key {
        "charset.string" => "A string was expected in a @charset rule.",
        "semicolon" => "';' expected.",
        "colon" => "':' expected.",
        "eof" => "Unexpected end of file.",
        "eof.expected" => "End of file expected.",
        "character" => "Unexpected character: {0}.",
        "string.or.uri" => "String or URI expected.",
        "prefix.or.namespace" => "String representing a namespace prefix or namespace URI expected.",
        "namespace" => "Namespace URI String expected.",
        "unresolved.namespace" => "Namespace not found.",
        "identifier" => "Identifier expected.",
        // The jar's properties continue this key into a comment line.
        "identifier.character" => "# Modified by Apache Flex",
        "identifier.or.string" => "Identifier or string expected.",
        "left.curly.brace" => "'{' expected.",
        "left.brace" => "'(' expected.",
        "right.curly.brace" => "'}' expected.",
        "right.brace" => "')' expected.",
        "right.bracket" => "']' expected.",
        "space" => "whitespace expected.",
        "token" => "Unexpected token: {0} (see LexicalUnits).",
        "rgb.color" => "Invalid RGB color: {0}.",
        "encoding" => "Invalid encoding: {0}.",
        "number.format" => "Invalid number.",
        "duplicate.pseudo.element" => "A pseudo element has already been defined.",
        "pseudo.function" => "Invalid pseudo-function.",
        "empty.source" => "Empty input source.",
        _ => return None,
    })
}

/// `NumberFormat.getInstance().format(int)` in an English locale.
fn grouped(v: i32) -> String {
    let digits = v.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if v < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// `MessageFormat.format(pattern, args)` for the simple `{n}` patterns of the bundle.
pub fn message_format(pattern: &str, args: Option<&[Param]>) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut quoted = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            if i + 1 < chars.len() && chars[i + 1] == '\'' {
                out.push('\'');
                i += 2;
                continue;
            }
            quoted = !quoted;
        } else if c == '{' && !quoted {
            let close = chars[i..].iter().position(|&x| x == '}').map(|p| p + i).unwrap_or(chars.len());
            let index: String = chars[i + 1..close].iter().collect();
            let n: usize = index.trim().parse().unwrap_or(usize::MAX);
            match args.and_then(|a| a.get(n)) {
                Some(Param::Char(u)) => out.push_str(&jstr::from_unit(*u)),
                Some(Param::Int(v)) => out.push_str(&grouped(*v)),
                Some(Param::Str(s)) => out.push_str(s),
                None => {
                    out.push('{');
                    out.push_str(&index);
                    out.push('}');
                }
            }
            i = close + 1;
            continue;
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

pub fn format_message(key: &str, args: Option<&[Param]>) -> CssResult<String> {
    match bundle(key) {
        Some(p) => Ok(message_format(p, args)),
        None => Err(CssError::Runtime(Some(format!("Can't find resource for bundle java.util.PropertyResourceBundle, key {key}")))),
    }
}

fn runtime(e: String) -> CssError {
    CssError::Runtime(Some(e))
}

/// Java's `finally`: the finally block's exception wins over the body's.
fn finally<T>(body: CssResult<T>, fin: CssResult<()>) -> CssResult<T> {
    match fin {
        Err(e) => Err(e),
        Ok(()) => body,
    }
}

/// `Float.parseFloat` on the scanner's digit strings.
fn parse_float(s: &str) -> Option<f32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit() || b == b'.') || s == "." {
        return None;
    }
    s.parse::<f32>().ok()
}

/// `Integer.parseInt`, with its `NumberFormatException` text.
fn parse_int(s: &str) -> CssResult<i32> {
    s.parse::<i32>().map_err(|_| runtime(format!("For input string: \"{s}\"")))
}

pub struct Parser {
    scanner: Option<Scanner>,
    current: i32,
    document_handler: Box<dyn DocumentHandler>,
    selector_factory: DefaultSelectorFactory,
    condition_factory: DefaultConditionFactory,
    error_handler: Box<dyn ErrorHandler>,
    pseudo_element: Option<String>,
    document_uri: Option<String>,
    namespaces: BTreeMap<String, String>,
    default_namespace: Option<String>,
    line_number_offset: i32,
    /// `getLineNumber()` as of the scanner's last move, for handlers that ask the parser for it.
    line: Rc<Cell<i32>>,
}

impl Default for Parser {
    fn default() -> Self {
        Parser::new()
    }
}

impl Parser {
    pub fn new() -> Parser {
        Parser {
            scanner: None,
            current: 0,
            document_handler: Box::new(DefaultDocumentHandler),
            selector_factory: DefaultSelectorFactory,
            condition_factory: DefaultConditionFactory,
            error_handler: Box::new(DefaultErrorHandler),
            pseudo_element: None,
            document_uri: None,
            namespaces: BTreeMap::new(),
            default_namespace: None,
            line_number_offset: 0,
            line: Rc::new(Cell::new(-1)),
        }
    }

    pub fn get_parser_version(&self) -> &'static str {
        "http://www.w3.org/TR/REC-CSS2"
    }

    pub fn set_document_handler(&mut self, handler: Box<dyn DocumentHandler>) {
        self.document_handler = handler;
    }

    pub fn take_document_handler(&mut self) -> Box<dyn DocumentHandler> {
        std::mem::replace(&mut self.document_handler, Box::new(DefaultDocumentHandler))
    }

    pub fn set_error_handler(&mut self, handler: Box<dyn ErrorHandler>) {
        self.error_handler = handler;
    }

    /// A shared view of `getLineNumber()`.
    pub fn line_number_cell(&self) -> Rc<Cell<i32>> {
        self.line.clone()
    }

    fn sync(&self) {
        self.line.set(self.get_line_number());
    }

    fn sc(&mut self) -> &mut Scanner {
        self.scanner.as_mut().expect("NullPointerException")
    }

    fn value(&self) -> CssResult<String> {
        self.scanner.as_ref().expect("NullPointerException").get_string_value().map_err(runtime)
    }

    fn create_scanner(&mut self, chars: Vec<u16>) -> Scanner {
        self.document_uri = Some(String::new());
        Scanner::new(chars)
    }

    fn set_scanner(&mut self, s: Option<Scanner>) {
        self.scanner = s;
        self.sync();
    }

    /// `parseStyleSheet(InputSource)` over a character stream.
    pub fn parse_style_sheet(&mut self, chars: Vec<u16>) -> CssResult<()> {
        let s = self.create_scanner(chars);
        self.set_scanner(Some(s));
        let body = self.parse_style_sheet_body();
        let fin = self.document_handler.end_document();
        self.set_scanner(None);
        finally(body, fin)
    }

    fn parse_style_sheet_body(&mut self) -> CssResult<()> {
        self.document_handler.start_document()?;
        let first = self.sc().next();
        self.sync();
        self.current = first.map_err(|e| CssError::Runtime(Some(e.message)))?;
        match self.current {
            lu::CHARSET_SYMBOL => {
                if self.next_ignore_spaces()? != lu::STRING {
                    self.report_error("charset.string")?;
                } else {
                    if self.next_ignore_spaces()? != lu::SEMI_COLON {
                        self.report_error("semicolon")?;
                    }
                    self.next()?;
                }
            }
            lu::COMMENT => {
                let v = self.value()?;
                self.document_handler.comment(&v)?;
            }
            _ => {}
        }
        self.skip_spaces_and_cdocdc()?;
        while self.current == lu::IMPORT_SYMBOL {
            self.next_ignore_spaces()?;
            self.parse_import_rule()?;
            self.next_ignore_spaces()?;
        }
        loop {
            match self.current {
                lu::NAMESPACE => {
                    self.next_ignore_spaces()?;
                    self.parse_namespace()?;
                }
                lu::PAGE_SYMBOL => {
                    self.next_ignore_spaces()?;
                    self.parse_page_rule()?;
                }
                lu::MEDIA_SYMBOL => {
                    self.next_ignore_spaces()?;
                    self.parse_media_rule()?;
                }
                lu::FONT_FACE_SYMBOL => {
                    self.next_ignore_spaces()?;
                    self.parse_font_face_rule()?;
                }
                lu::AT_KEYWORD => {
                    self.next_ignore_spaces()?;
                    self.parse_at_rule()?;
                }
                lu::EOF => break,
                _ => self.parse_rule_set()?,
            }
            self.skip_spaces_and_cdocdc()?;
        }
        Ok(())
    }

    pub fn get_column_number(&self) -> i32 {
        self.scanner.as_ref().map_or(-1, |s| s.get_column())
    }

    pub fn get_line_number(&self) -> i32 {
        self.scanner.as_ref().map_or(-1, |s| s.get_line()) + self.line_number_offset
    }

    pub fn get_line_number_offset(&self) -> i32 {
        self.line_number_offset
    }

    pub fn set_line_number_offset(&mut self, value: i32) {
        self.line_number_offset = value;
    }

    /// `parseStyleDeclaration(String)`.
    pub fn parse_style_declaration_string(&mut self, source: &str) -> CssResult<()> {
        let s = Scanner::new(jstr::to_utf16(source));
        self.set_scanner(Some(s));
        self.next_ignore_spaces()?;
        let r = match self.parse_style_declaration(false) {
            Err(CssError::Parse(e)) => self.report_style_declaration_error(e),
            r => r,
        };
        self.set_scanner(None);
        r
    }

    /// `parseRule(String)`.
    pub fn parse_rule_string(&mut self, source: &str) -> CssResult<()> {
        let s = Scanner::new(jstr::to_utf16(source));
        self.set_scanner(Some(s));
        self.next_ignore_spaces()?;
        self.parse_rule()?;
        self.set_scanner(None);
        Ok(())
    }

    /// `parseSelectors(String)`.
    pub fn parse_selectors_string(&mut self, source: &str) -> CssResult<CSSSelectorList> {
        let s = Scanner::new(jstr::to_utf16(source));
        self.set_scanner(Some(s));
        self.next_ignore_spaces()?;
        let ret = self.parse_selector_list()?;
        self.set_scanner(None);
        Ok(ret)
    }

    /// `parsePropertyValue(String)`.
    pub fn parse_property_value_string(&mut self, source: &str) -> CssResult<Option<LexicalUnitRef>> {
        let s = Scanner::new(jstr::to_utf16(source));
        self.set_scanner(Some(s));
        self.next_ignore_spaces()?;
        let exp = match self.parse_expression(false) {
            Err(CssError::Parse(e)) => {
                self.report_error_exception(e.clone())?;
                return Err(CssError::Parse(e));
            }
            r => r?,
        };
        let mut exception = None;
        if self.current != lu::EOF {
            exception = Some(self.create_css_parse_exception("eof.expected", None)?);
        }
        self.set_scanner(None);
        if let Some(e) = exception {
            self.error_handler.fatal_error(e)?;
        }
        Ok(exp)
    }

    /// `parsePriority(String)`.
    pub fn parse_priority_string(&mut self, source: &str) -> CssResult<bool> {
        let s = Scanner::new(jstr::to_utf16(source));
        self.set_scanner(Some(s));
        self.next_ignore_spaces()?;
        self.set_scanner(None);
        match self.current {
            lu::EOF => Ok(false),
            lu::IMPORT_SYMBOL => Ok(true),
            c => {
                self.report_error_with("token", Some(vec![Param::Int(c)]))?;
                Ok(false)
            }
        }
    }

    fn parse_rule(&mut self) -> CssResult<()> {
        match self.sc().get_type() {
            lu::IMPORT_SYMBOL => {
                self.next_ignore_spaces()?;
                self.parse_import_rule()
            }
            lu::NAMESPACE => {
                self.next_ignore_spaces()?;
                self.parse_namespace()
            }
            lu::AT_KEYWORD => {
                self.next_ignore_spaces()?;
                self.parse_at_rule()
            }
            lu::FONT_FACE_SYMBOL => {
                self.next_ignore_spaces()?;
                self.parse_font_face_rule()
            }
            lu::MEDIA_SYMBOL => {
                self.next_ignore_spaces()?;
                self.parse_media_rule()
            }
            lu::PAGE_SYMBOL => {
                self.next_ignore_spaces()?;
                self.parse_page_rule()
            }
            _ => self.parse_rule_set(),
        }
    }

    fn parse_at_rule(&mut self) -> CssResult<()> {
        self.sc().scan_at_rule();
        self.sync();
        let v = self.value()?;
        self.document_handler.ignorable_at_rule(&v)?;
        self.next_ignore_spaces()?;
        Ok(())
    }

    fn parse_import_rule(&mut self) -> CssResult<()> {
        let uri;
        match self.current {
            lu::STRING | lu::URI => {
                uri = Some(self.value()?);
                self.next_ignore_spaces()?;
            }
            _ => return self.report_error("string.or.uri"),
        }
        let ml = if self.current != lu::IDENTIFIER { CSSSACMediaList::all() } else { self.parse_media_list()? };
        self.document_handler.import_style(uri.as_deref(), &ml, None)?;
        if self.current != lu::SEMI_COLON {
            self.report_error("semicolon")?;
        } else {
            self.next()?;
        }
        Ok(())
    }

    fn parse_namespace(&mut self) -> CssResult<()> {
        let mut prefix = None;
        let mut uri = None;
        let line_number = self.get_line_number();
        match self.current {
            lu::IDENTIFIER => {
                prefix = Some(self.value()?);
                self.next_ignore_spaces()?;
            }
            lu::STRING => {
                uri = Some(self.value()?);
                self.next_ignore_spaces()?;
            }
            _ => return self.report_error("prefix.or.namespace"),
        }
        match self.current {
            lu::STRING => {
                uri = Some(self.value()?);
                self.next_ignore_spaces()?;
            }
            lu::SEMI_COLON => {}
            _ => return self.report_error("namespace"),
        }
        if let Some(uri) = uri {
            match prefix {
                Some(p) if !p.is_empty() => {
                    self.namespaces.insert(p, uri);
                }
                _ => self.default_namespace = Some(uri),
            }
        }
        if self.current != lu::SEMI_COLON {
            let e = self.create_css_parse_exception_at("semicolon", None, line_number)?;
            self.report_error_exception(e)?;
        } else {
            self.next()?;
        }
        Ok(())
    }

    fn parse_media_expression(&mut self) -> CssResult<Option<CSSSACMediaExpression>> {
        let media_feature = self.value()?;
        self.next_ignore_spaces()?;
        match self.current {
            lu::RIGHT_BRACE => return Ok(Some(CSSSACMediaExpression::new(media_feature, None))),
            lu::COLON => {
                self.next_ignore_spaces()?;
                let expr = self.parse_expression(true)?;
                return Ok(Some(CSSSACMediaExpression::new(media_feature, expr)));
            }
            _ => self.report_error("colon")?,
        }
        Ok(None)
    }

    fn parse_media_query(&mut self) -> CssResult<Option<CSSSACMediaQuery>> {
        let mut result = None;
        let mut expect_and = true;
        match self.current {
            lu::IDENTIFIER => {
                let next_token_string = self.value()?;
                let mut is_not = false;
                if next_token_string.java_eq_ic("not") {
                    self.next_ignore_spaces()?;
                    is_not = true;
                } else if next_token_string.java_eq_ic("only") {
                    self.next_ignore_spaces()?;
                }
                match self.current {
                    lu::IDENTIFIER => {
                        let media_type = self.value()?;
                        result = Some(CSSSACMediaQuery::new(is_not, Some(media_type)));
                        self.next_ignore_spaces()?;
                        expect_and = true;
                    }
                    _ => self.report_error("identifier")?,
                }
            }
            lu::LEFT_BRACE => {
                result = Some(CSSSACMediaQuery::new(false, None));
                expect_and = false;
            }
            _ => self.report_error("identifier")?,
        }
        while !expect_and || (self.current == lu::IDENTIFIER && self.value()?.java_eq_ic("and")) {
            if expect_and {
                self.next_ignore_spaces()?;
            }
            expect_and = true;
            match self.current {
                lu::LEFT_BRACE => {
                    self.next_ignore_spaces()?;
                    match self.current {
                        lu::IDENTIFIER => {
                            let e = self.parse_media_expression()?;
                            result.as_mut().ok_or(CssError::Runtime(None))?.append_expression(e);
                        }
                        _ => self.report_error("identifier")?,
                    }
                    match self.current {
                        lu::RIGHT_BRACE => {
                            self.next_ignore_spaces()?;
                        }
                        _ => self.report_error("right.brace")?,
                    }
                }
                _ => self.report_error("left.brace")?,
            }
        }
        Ok(result)
    }

    fn parse_media_list(&mut self) -> CssResult<CSSSACMediaList> {
        let mut result = CSSSACMediaList::new();
        let q = self.parse_media_query()?;
        result.append(q);
        while self.current == lu::COMMA {
            self.next_ignore_spaces()?;
            let q = self.parse_media_query()?;
            result.append(q);
        }
        Ok(result)
    }

    /// `{ ... parseStyleDeclaration(true) ... }` with `catch (CSSParseException e) { reportError(e); }`.
    fn parse_braced_declaration(&mut self) -> CssResult<()> {
        if self.current != lu::LEFT_CURLY_BRACE {
            self.report_error("left.curly.brace")
        } else {
            self.next_ignore_spaces()?;
            match self.parse_style_declaration(true) {
                Err(CssError::Parse(e)) => self.report_error_exception(e),
                r => r,
            }
        }
    }

    fn parse_font_face_rule(&mut self) -> CssResult<()> {
        let body = self.document_handler.start_font_face().and_then(|_| self.parse_braced_declaration());
        let fin = self.document_handler.end_font_face();
        finally(body, fin)
    }

    fn parse_page_rule(&mut self) -> CssResult<()> {
        let mut page = None;
        let mut ppage = None;
        if self.current == lu::IDENTIFIER {
            page = Some(self.value()?);
            self.next_ignore_spaces()?;
            if self.current == lu::COLON {
                self.next_ignore_spaces()?;
                if self.current != lu::IDENTIFIER {
                    return self.report_error("identifier");
                }
                ppage = Some(self.value()?);
                self.next_ignore_spaces()?;
            }
        }
        let body = self.document_handler.start_page(page.as_deref(), ppage.as_deref()).and_then(|_| self.parse_braced_declaration());
        let fin = self.document_handler.end_page(page.as_deref(), ppage.as_deref());
        finally(body, fin)
    }

    fn parse_media_rule(&mut self) -> CssResult<()> {
        let ml = self.parse_media_list()?;
        let body = (|| -> CssResult<()> {
            self.document_handler.start_media(&ml)?;
            if self.current != lu::LEFT_CURLY_BRACE {
                self.report_error("left.curly.brace")?;
            } else {
                self.next_ignore_spaces()?;
                loop {
                    match self.current {
                        lu::EOF => {
                            let e = self.create_css_parse_exception("eof", None)?;
                            self.report_style_declaration_error(e)?;
                            break;
                        }
                        lu::RIGHT_CURLY_BRACE => break,
                        _ => self.parse_rule_set()?,
                    }
                }
                self.next_ignore_spaces()?;
            }
            Ok(())
        })();
        let fin = self.document_handler.end_media(&ml);
        finally(body, fin)
    }

    fn parse_rule_set(&mut self) -> CssResult<()> {
        let sl = match self.parse_selector_list() {
            Ok(sl) => sl,
            Err(CssError::Parse(e)) => return self.report_error_exception(e),
            Err(e) => return Err(e),
        };
        let body = (|| -> CssResult<()> {
            self.document_handler.start_selector(&sl)?;
            if self.current != lu::LEFT_CURLY_BRACE {
                self.report_error("left.curly.brace")?;
                if self.current == lu::RIGHT_CURLY_BRACE {
                    self.next_ignore_spaces()?;
                }
            } else {
                self.next_ignore_spaces()?;
                match self.parse_style_declaration(true) {
                    Err(CssError::Parse(e)) => self.report_style_declaration_error(e)?,
                    r => r?,
                }
            }
            Ok(())
        })();
        let fin = self.document_handler.end_selector(&sl);
        finally(body, fin)
    }

    fn parse_selector_list(&mut self) -> CssResult<CSSSelectorList> {
        let mut result = CSSSelectorList::new();
        result.append(self.parse_selector()?);
        loop {
            if self.current != lu::COMMA {
                return Ok(result);
            }
            self.next_ignore_spaces()?;
            result.append(self.parse_selector()?);
        }
    }

    fn parse_selector(&mut self) -> CssResult<SelectorRef> {
        let ss = self.parse_simple_selector()?;
        let mut result = ss;
        self.pseudo_element = None;
        loop {
            match self.current {
                lu::IDENTIFIER | lu::ANY | lu::HASH | lu::DOT | lu::LEFT_BRACKET | lu::COLON => {
                    let current_line = self.get_line_number();
                    let simple = self.parse_simple_selector()?;
                    result = self.selector_factory.create_descendant_selector(result, simple)?;
                    result.set_line_number(current_line);
                }
                lu::PLUS => {
                    self.next_ignore_spaces()?;
                    let simple = self.parse_simple_selector()?;
                    result = self.selector_factory.create_direct_adjacent_selector(1, result, simple)?;
                }
                lu::PRECEDE => {
                    self.next_ignore_spaces()?;
                    let simple = self.parse_simple_selector()?;
                    result = self.selector_factory.create_child_selector(result, simple)?;
                }
                _ => break,
            }
        }
        if let Some(pe) = self.pseudo_element.clone() {
            let p = self.selector_factory.create_pseudo_element_selector(None, Some(pe))?;
            result = self.selector_factory.create_child_selector(result, p)?;
        }
        Ok(result)
    }

    fn parse_simple_selector(&mut self) -> CssResult<SelectorRef> {
        let mut uri = self.default_namespace.clone();
        let current_line_number = self.get_line_number();
        let mut result = match self.current {
            lu::IDENTIFIER => {
                let mut name = self.value()?;
                self.next()?;
                if self.current == lu::NAMESPACE_QUALIFIED {
                    if name.is_empty() {
                        uri = Some(String::new());
                    } else if name == "*" {
                        uri = None;
                    } else {
                        uri = self.namespaces.get(&name).cloned();
                        if uri.is_none() {
                            let e = self.create_css_parse_exception("unresolved.namespace", None)?;
                            self.error_handler.error(e)?;
                        }
                    }
                    self.next()?;
                    name = self.value()?;
                    self.next()?;
                }
                self.selector_factory.create_element_selector(uri, Some(name))?
            }
            c => {
                if c == lu::ANY {
                    self.next()?;
                }
                self.selector_factory.create_element_selector(uri, None)?
            }
        };
        result.set_line_number(current_line_number);
        let mut cond: Option<ConditionRef> = None;
        loop {
            let c: Option<ConditionRef>;
            match self.current {
                lu::HASH => {
                    let v = self.value()?;
                    c = Some(self.condition_factory.create_id_condition(Some(v))?);
                    self.next()?;
                }
                lu::DOT => {
                    if self.next()? != lu::IDENTIFIER {
                        return Err(CssError::Parse(self.create_css_parse_exception("identifier", None)?));
                    }
                    let v = self.value()?;
                    c = Some(self.condition_factory.create_class_condition(None, Some(v))?);
                    self.next()?;
                }
                lu::LEFT_BRACKET => {
                    if self.next_ignore_spaces()? != lu::IDENTIFIER {
                        return Err(CssError::Parse(self.create_css_parse_exception("identifier", None)?));
                    }
                    let name = self.value()?;
                    let op = self.next_ignore_spaces()?;
                    match op {
                        lu::RIGHT_BRACKET => {
                            self.next_ignore_spaces()?;
                            c = Some(self.condition_factory.create_attribute_condition(Some(name), None, false, None)?);
                        }
                        lu::EQUAL | lu::INCLUDES | lu::DASHMATCH => {
                            let val;
                            match self.next_ignore_spaces()? {
                                lu::STRING | lu::IDENTIFIER => {
                                    val = Some(self.value()?);
                                    self.next_ignore_spaces()?;
                                }
                                _ => return Err(CssError::Parse(self.create_css_parse_exception("identifier.or.string", None)?)),
                            }
                            if self.current != lu::RIGHT_BRACKET {
                                return Err(CssError::Parse(self.create_css_parse_exception("right.bracket", None)?));
                            }
                            self.next()?;
                            c = Some(match op {
                                lu::EQUAL => self.condition_factory.create_attribute_condition(Some(name), None, false, val)?,
                                lu::INCLUDES => self.condition_factory.create_one_of_attribute_condition(Some(name), None, false, val)?,
                                _ => self.condition_factory.create_begin_hyphen_attribute_condition(Some(name), None, false, val)?,
                            });
                        }
                        _ => return Err(CssError::Parse(self.create_css_parse_exception("right.bracket", None)?)),
                    }
                }
                lu::COLON => match self.next_ignore_spaces()? {
                    lu::IDENTIFIER => {
                        let val = self.value()?;
                        if is_pseudo_element(&val) {
                            if self.pseudo_element.is_some() {
                                return Err(CssError::Parse(self.create_css_parse_exception("duplicate.pseudo.element", None)?));
                            }
                            self.pseudo_element = Some(val);
                            c = None;
                        } else {
                            c = Some(self.condition_factory.create_pseudo_class_condition(None, Some(val))?);
                        }
                        self.next()?;
                    }
                    lu::FUNCTION => {
                        let func = self.value()?;
                        if self.next_ignore_spaces()? != lu::IDENTIFIER {
                            return Err(CssError::Parse(self.create_css_parse_exception("identifier", None)?));
                        }
                        let lang = self.value()?;
                        if self.next_ignore_spaces()? != lu::RIGHT_BRACE {
                            return Err(CssError::Parse(self.create_css_parse_exception("right.brace", None)?));
                        }
                        if !func.java_eq_ic("lang") {
                            return Err(CssError::Parse(self.create_css_parse_exception("pseudo.function", None)?));
                        }
                        c = Some(self.condition_factory.create_lang_condition(Some(lang))?);
                        self.next()?;
                    }
                    _ => return Err(CssError::Parse(self.create_css_parse_exception("identifier", None)?)),
                },
                _ => break,
            }
            if let Some(c) = c {
                cond = Some(match cond {
                    None => c,
                    Some(prev) => self.condition_factory.create_and_condition(prev, c)?,
                });
            }
        }
        self.skip_spaces()?;
        if let Some(cond) = cond {
            result = self.selector_factory.create_conditional_selector(result, cond)?;
        }
        Ok(result)
    }

    fn parse_style_declaration(&mut self, in_sheet: bool) -> CssResult<()> {
        loop {
            match self.current {
                lu::EOF => {
                    if in_sheet {
                        return Err(CssError::Parse(self.create_css_parse_exception("eof", None)?));
                    }
                    return Ok(());
                }
                lu::RIGHT_CURLY_BRACE => {
                    if !in_sheet {
                        return Err(CssError::Parse(self.create_css_parse_exception("eof.expected", None)?));
                    }
                    self.next_ignore_spaces()?;
                    return Ok(());
                }
                lu::SEMI_COLON => {
                    self.next_ignore_spaces()?;
                    continue;
                }
                lu::IDENTIFIER => {}
                _ => return Err(CssError::Parse(self.create_css_parse_exception("identifier", None)?)),
            }
            let name = self.value()?;
            if self.next_ignore_spaces()? != lu::COLON {
                return Err(CssError::Parse(self.create_css_parse_exception("colon", None)?));
            }
            self.next_ignore_spaces()?;
            let exp = match self.parse_expression(false) {
                Ok(e) => e,
                Err(CssError::Parse(e)) => {
                    self.report_error_exception(e)?;
                    None
                }
                Err(e) => return Err(e),
            };
            if let Some(exp) = exp {
                let mut important = false;
                if self.current == lu::IMPORTANT_SYMBOL {
                    important = true;
                    self.next_ignore_spaces()?;
                }
                self.document_handler.property(&name, &exp, important)?;
            }
        }
    }

    fn token_error(&mut self) -> CssResult<CssError> {
        let c = self.current;
        Ok(CssError::Parse(self.create_css_parse_exception("token", Some(vec![Param::Int(c)]))?))
    }

    fn parse_expression(&mut self, param: bool) -> CssResult<Option<LexicalUnitRef>> {
        let result = self.parse_term(None)?;
        let mut curr = result.clone();
        loop {
            let mut op = false;
            match self.current {
                lu::COMMA => {
                    op = true;
                    curr = clu::create_simple(SAC_OPERATOR_COMMA, Some(&curr), self.get_line_number());
                    self.next_ignore_spaces()?;
                }
                lu::EQUAL | lu::DIVIDE => {
                    op = true;
                    curr = clu::create_simple(SAC_OPERATOR_SLASH, Some(&curr), self.get_line_number());
                    self.next_ignore_spaces()?;
                }
                _ => {}
            }
            if param {
                if self.current == lu::RIGHT_BRACE {
                    if op {
                        return Err(self.token_error()?);
                    }
                    return Ok(Some(result));
                }
                curr = self.parse_term(Some(&curr))?;
            } else {
                match self.current {
                    lu::IMPORTANT_SYMBOL | lu::SEMI_COLON | lu::RIGHT_CURLY_BRACE | lu::EOF => {
                        if op {
                            return Err(self.token_error()?);
                        }
                        return Ok(Some(result));
                    }
                    _ => curr = self.parse_term(Some(&curr))?,
                }
            }
        }
    }

    fn float_unit(&mut self, t: i16, plus: bool, prev: Option<&LexicalUnitRef>) -> CssResult<LexicalUnitRef> {
        let v = self.number(plus)?;
        let line = self.get_line_number();
        Ok(clu::create_float(t, v, prev, line))
    }

    fn parse_term(&mut self, prev: Option<&LexicalUnitRef>) -> CssResult<LexicalUnitRef> {
        let mut plus = true;
        let mut sgn = false;
        if let lu::MINUS | lu::PLUS = self.current {
            if self.current == lu::MINUS {
                plus = false;
            }
            self.next()?;
            sgn = true;
        }
        match self.current {
            lu::INTEGER => {
                let mut sval = self.value()?;
                if !plus {
                    sval = format!("-{sval}");
                }
                let val = parse_int(&sval)?;
                let line = self.get_line_number();
                self.next_ignore_spaces()?;
                return Ok(clu::create_integer(val, prev, line));
            }
            lu::REAL => return self.float_unit(SAC_REAL, plus, prev),
            lu::PERCENTAGE => return self.float_unit(SAC_PERCENTAGE, plus, prev),
            lu::PT => return self.float_unit(SAC_POINT, plus, prev),
            lu::PC => return self.float_unit(SAC_PICA, plus, prev),
            lu::PX => return self.float_unit(SAC_PIXEL, plus, prev),
            lu::CM => return self.float_unit(SAC_CENTIMETER, plus, prev),
            lu::MM => return self.float_unit(SAC_MILLIMETER, plus, prev),
            lu::IN => return self.float_unit(SAC_INCH, plus, prev),
            lu::EM => return self.float_unit(SAC_EM, plus, prev),
            lu::EX => return self.float_unit(SAC_EX, plus, prev),
            lu::DEG => return self.float_unit(SAC_DEGREE, plus, prev),
            lu::DPI => return self.float_unit(SAC_DPI, plus, prev),
            lu::DPCM => return self.float_unit(SAC_DPCM, plus, prev),
            lu::RAD => return self.float_unit(SAC_RADIAN, plus, prev),
            lu::GRAD => return self.float_unit(SAC_GRADIAN, plus, prev),
            lu::S => return self.float_unit(SAC_SECOND, plus, prev),
            lu::MS => return self.float_unit(SAC_MILLISECOND, plus, prev),
            lu::HZ => return self.float_unit(SAC_HERTZ, plus, prev),
            lu::KHZ => return self.float_unit(SAC_KILOHERTZ, plus, prev),
            lu::DIMENSION => return self.dimension(plus, prev),
            lu::FUNCTION => return self.parse_function(plus, prev),
            _ => {}
        }
        if sgn {
            return Err(self.token_error()?);
        }
        match self.current {
            lu::STRING => {
                let val = self.value()?;
                let line = self.get_line_number();
                self.next_ignore_spaces()?;
                Ok(clu::create_string(SAC_STRING_VALUE, val, prev, line))
            }
            lu::IDENTIFIER => {
                let val = self.value()?;
                let line = self.get_line_number();
                self.next_ignore_spaces()?;
                if val.java_eq_ic("inherit") {
                    Ok(clu::create_simple(SAC_INHERIT, prev, line))
                } else {
                    Ok(clu::create_string(SAC_IDENT, val, prev, line))
                }
            }
            lu::URI => {
                let val = self.value()?;
                let line = self.get_line_number();
                self.next_ignore_spaces()?;
                Ok(clu::create_string(SAC_URI, val, prev, line))
            }
            lu::HASH => self.hexcolor(prev),
            lu::UNICODE_RANGE => {
                let val = self.value()?;
                let line = self.get_line_number();
                self.next_ignore_spaces()?;
                Ok(clu::create_string(SAC_UNICODERANGE, val, prev, line))
            }
            _ => Err(self.token_error()?),
        }
    }

    fn parse_function(&mut self, _positive: bool, prev: Option<&LexicalUnitRef>) -> CssResult<LexicalUnitRef> {
        let name = self.value()?;
        self.next_ignore_spaces()?;
        let params = self.parse_expression(true)?;
        if self.current != lu::RIGHT_BRACE {
            return Err(self.token_error()?);
        }
        let line = self.get_line_number();
        self.next_ignore_spaces()?;
        if let Some(t) = predefined_function(&name, params.as_ref()) {
            if t == SAC_ATTR {
                let v = params.as_ref().expect("NullPointerException").get_string_value().expect("IllegalStateException");
                return Ok(clu::create_string(SAC_ATTR, v, prev, line));
            }
            return Ok(clu::create_predefined_function(t, params, prev, line));
        }
        Ok(clu::create_function(name, params, prev, line))
    }

    fn hexcolor(&mut self, prev: Option<&LexicalUnitRef>) -> CssResult<LexicalUnitRef> {
        let val = self.value()?;
        let u = jstr::to_utf16(&val);
        let hex = |c: u16| -> i32 {
            if (b'0' as u16..=b'9' as u16).contains(&c) {
                (c - b'0' as u16) as i32
            } else {
                c as i32 - 'a' as i32 + 10
            }
        };
        let (r, g, b);
        match u.len() {
            3 => {
                let (rc, gc, bc) = (java_to_lower(u[0]), java_to_lower(u[1]), java_to_lower(u[2]));
                if !is_css_hexadecimal_character(rc) || !is_css_hexadecimal_character(gc) || !is_css_hexadecimal_character(bc) {
                    return Err(CssError::Parse(self.create_css_parse_exception("rgb.color", Some(vec![Param::Str(val)]))?));
                }
                r = hex(rc) | (hex(rc) << 4);
                g = hex(gc) | (hex(gc) << 4);
                b = hex(bc) | (hex(bc) << 4);
            }
            6 => {
                let l: Vec<u16> = u.iter().map(|&c| java_to_lower(c)).collect();
                if !l.iter().all(|&c| is_css_hexadecimal_character(c)) {
                    return Err(CssError::Parse(self.create_css_parse_exception("rgb.color", None)?));
                }
                r = (hex(l[0]) << 4) | hex(l[1]);
                g = (hex(l[2]) << 4) | hex(l[3]);
                b = (hex(l[4]) << 4) | hex(l[5]);
            }
            _ => return Err(CssError::Parse(self.create_css_parse_exception("rgb.color", Some(vec![Param::Str(val)]))?)),
        }
        let params = clu::create_integer(r, None, self.get_line_number());
        let tmp = clu::create_simple(SAC_OPERATOR_COMMA, Some(&params), self.get_line_number());
        let tmp = clu::create_integer(g, Some(&tmp), self.get_line_number());
        let tmp = clu::create_simple(SAC_OPERATOR_COMMA, Some(&tmp), self.get_line_number());
        clu::create_integer(b, Some(&tmp), self.get_line_number());
        let line = self.get_line_number();
        self.next_ignore_spaces()?;
        Ok(clu::create_predefined_function(SAC_RGBCOLOR, Some(params), prev, line))
    }

    fn skip_spaces(&mut self) -> CssResult<i32> {
        let mut lex = self.sc().get_type();
        while lex == lu::SPACE {
            lex = self.next()?;
        }
        Ok(lex)
    }

    fn skip_spaces_and_cdocdc(&mut self) -> CssResult<i32> {
        while let lu::COMMENT | lu::SPACE | lu::CDO | lu::CDC = self.current {
            self.sc().clear_buffer();
            self.next()?;
        }
        Ok(self.current)
    }

    fn number(&mut self, positive: bool) -> CssResult<f32> {
        let sgn: f32 = if positive { 1.0 } else { -1.0 };
        let val = self.value()?;
        self.next_ignore_spaces()?;
        match parse_float(&val) {
            Some(v) => Ok(sgn * v),
            None => Err(CssError::Parse(self.create_css_parse_exception("number.format", None)?)),
        }
    }

    fn dimension(&mut self, positive: bool, prev: Option<&LexicalUnitRef>) -> CssResult<LexicalUnitRef> {
        let sgn: f32 = if positive { 1.0 } else { -1.0 };
        let val = self.value()?;
        let u = jstr::to_utf16(&val);
        let i = u.iter().position(|&c| !(c == b'.' as u16 || (b'0' as u16..=b'9' as u16).contains(&c))).unwrap_or(u.len());
        let line = self.get_line_number();
        self.next_ignore_spaces()?;
        match parse_float(&jstr::from_utf16(&u[..i])) {
            Some(v) => Ok(clu::create_dimension(sgn * v, jstr::from_utf16(&u[i..]), prev, line)),
            None => Err(CssError::Parse(self.create_css_parse_exception("number.format", None)?)),
        }
    }

    fn scan_next(&mut self) -> Result<i32, ParseException> {
        let r = self.sc().next();
        self.sync();
        r
    }

    fn next(&mut self) -> CssResult<i32> {
        loop {
            self.sc().clear_buffer();
            match self.scan_next() {
                Ok(t) => self.current = t,
                Err(e) => {
                    self.report_error_with(&e.message, e.params)?;
                    return Ok(self.current);
                }
            }
            if self.current == lu::COMMENT {
                let v = self.value()?;
                self.document_handler.comment(&v)?;
            } else {
                break;
            }
        }
        Ok(self.current)
    }

    fn next_ignore_spaces(&mut self) -> CssResult<i32> {
        loop {
            self.sc().clear_buffer();
            match self.scan_next() {
                Ok(t) => self.current = t,
                Err(e) => {
                    let ex = self.create_css_parse_exception(&e.message, e.params)?;
                    self.error_handler.error(ex)?;
                    return Ok(self.current);
                }
            }
            match self.current {
                lu::COMMENT => {
                    let v = self.value()?;
                    self.document_handler.comment(&v)?;
                }
                lu::SPACE => {}
                _ => break,
            }
        }
        Ok(self.current)
    }

    fn report_error(&mut self, key: &str) -> CssResult<()> {
        self.report_error_with(key, None)
    }

    fn report_error_with(&mut self, key: &str, params: Option<Vec<Param>>) -> CssResult<()> {
        let e = self.create_css_parse_exception(key, params)?;
        self.report_error_exception(e)
    }

    fn report_error_exception(&mut self, e: CSSParseException) -> CssResult<()> {
        self.error_handler.error(e)?;
        let mut cbraces = 1;
        loop {
            match self.current {
                lu::EOF => return Ok(()),
                lu::SEMI_COLON | lu::RIGHT_CURLY_BRACE => {
                    cbraces -= 1;
                    if cbraces == 0 {
                        self.next_ignore_spaces()?;
                        return Ok(());
                    }
                    cbraces += 1;
                }
                lu::LEFT_CURLY_BRACE => cbraces += 1,
                _ => {}
            }
            self.next_ignore_spaces()?;
        }
    }

    fn report_style_declaration_error(&mut self, e: CSSParseException) -> CssResult<()> {
        self.error_handler.error(e)?;
        let mut cbraces = 1;
        loop {
            match self.current {
                lu::EOF => return Ok(()),
                lu::RIGHT_CURLY_BRACE => {
                    cbraces -= 1;
                    if cbraces == 0 {
                        self.next_ignore_spaces()?;
                        return Ok(());
                    }
                    cbraces += 1;
                }
                lu::LEFT_CURLY_BRACE => cbraces += 1,
                _ => {}
            }
            self.next_ignore_spaces()?;
        }
    }

    fn create_css_parse_exception(&self, key: &str, params: Option<Vec<Param>>) -> CssResult<CSSParseException> {
        self.create_css_parse_exception_at(key, params, self.get_line_number())
    }

    fn create_css_parse_exception_at(&self, key: &str, params: Option<Vec<Param>>, line_number: i32) -> CssResult<CSSParseException> {
        Ok(CSSParseException {
            message: format_message(key, params.as_deref())?,
            uri: self.document_uri.clone(),
            line_number,
            column_number: self.scanner.as_ref().expect("NullPointerException").get_column(),
        })
    }

    /// `parseMedia(String)`.
    pub fn parse_media(&self, media_text: &str) -> CSSSACMediaList {
        let mut result = CSSSACMediaList::new();
        if !media_text.java_eq_ic("all") {
            for t in media_text.split([' ', ',']).filter(|t| !t.is_empty()) {
                result.append_str(t);
            }
        }
        result
    }
}

/// The `predefined:` switch of `parseFunction`: the SAC type of a recognised `rgb`/`rect`/`counter`/`counters`/`attr`.
fn predefined_function(name: &str, params: Option<&LexicalUnitRef>) -> Option<i16> {
    let first = jstr::units(name).next()?;
    let next = |l: &Option<LexicalUnitRef>| l.as_ref().and_then(|x| x.get_next_lexical_unit());
    let steps = |kinds: &[&dyn Fn(&LexicalUnitRef) -> bool]| -> bool {
        let mut lu = params.cloned();
        for k in kinds {
            match &lu {
                None => return false,
                Some(x) => {
                    if !k(x) {
                        return false;
                    }
                }
            }
            lu = next(&lu);
        }
        lu.is_none()
    };
    let int_or_pct = |x: &LexicalUnitRef| matches!(x.get_lexical_unit_type(), SAC_INTEGER | SAC_PERCENTAGE);
    let comma = |x: &LexicalUnitRef| x.get_lexical_unit_type() == SAC_OPERATOR_COMMA;
    let ident = |x: &LexicalUnitRef| x.get_lexical_unit_type() == SAC_IDENT;
    let string = |x: &LexicalUnitRef| x.get_lexical_unit_type() == SAC_STRING_VALUE;
    let rect_part = |x: &LexicalUnitRef| match x.get_lexical_unit_type() {
        SAC_INTEGER => matches!(x.get_integer_value(), Ok(0)),
        SAC_IDENT => x.get_string_value().is_ok_and(|s| s.java_eq_ic("auto")),
        SAC_EM | SAC_EX | SAC_PIXEL | SAC_CENTIMETER | SAC_MILLIMETER | SAC_INCH | SAC_POINT | SAC_PICA | SAC_PERCENTAGE => true,
        _ => false,
    };
    match first {
        0x72 | 0x52 => {
            if name.java_eq_ic("rgb") {
                if steps(&[&int_or_pct, &comma, &int_or_pct, &comma, &int_or_pct]) {
                    return Some(SAC_RGBCOLOR);
                }
            } else if name.java_eq_ic("rect") && steps(&[&rect_part, &comma, &rect_part, &comma, &rect_part, &comma, &rect_part]) {
                return Some(SAC_RECT_FUNCTION);
            }
            None
        }
        0x63 | 0x43 => {
            if name.java_eq_ic("counter") {
                if steps(&[&ident, &comma, &ident]) {
                    return Some(SAC_COUNTER_FUNCTION);
                }
            } else if name.java_eq_ic("counters") && steps(&[&ident, &comma, &string, &comma, &ident]) {
                return Some(SAC_COUNTERS_FUNCTION);
            }
            None
        }
        0x61 | 0x41 => {
            if name.java_eq_ic("attr") && steps(&[&ident]) {
                return Some(SAC_ATTR);
            }
            None
        }
        _ => None,
    }
}

fn is_pseudo_element(s: &str) -> bool {
    match jstr::units(s).next() {
        Some(0x61 | 0x41) => s.java_eq_ic("after"),
        Some(0x62 | 0x42) => s.java_eq_ic("before"),
        Some(0x66 | 0x46) => s.java_eq_ic("first-letter") || s.java_eq_ic("first-line"),
        _ => false,
    }
}

impl From<CSSException> for CssError {
    fn from(e: CSSException) -> Self {
        CssError::Css(e)
    }
}
