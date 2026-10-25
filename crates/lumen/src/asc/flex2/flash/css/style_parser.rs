//! Port of `flash.css.StyleParser`: charset detection, then Batik's parser with mxmlc's error reporting.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::rule::RuleRef;
use super::style_document_handler::StyleDocumentHandler;
use super::style_parser_error_translator::get_user_friendly_errror;
use crate::asc::flex2::batik::css::parser::parser::Parser;
use crate::asc::flex2::batik::css::sac::{CSSException, CSSParseException, CssResult, ErrorHandler};
use crate::asc::flex2::compiler::toolkit::{self, Level};
use crate::asc::util::jstr;

pub struct StyleParser {
    rules: RefCell<Vec<RuleRef>>,
    css_path: Option<String>,
    errors_exist: Cell<bool>,
    mxml_path: Option<String>,
    mxml_line_number_offset: i32,
    line_number: RefCell<Rc<Cell<i32>>>,
    check_deprecation: bool,
}

fn msg(m: Option<String>) -> String {
    m.unwrap_or_else(|| "null".to_string())
}

/// The error handler `init` installs.
struct StyleErrorHandler {
    parser: Rc<StyleParser>,
}

impl StyleErrorHandler {
    fn report(&self, level: Level, exception: &CSSParseException) {
        let text = msg(get_user_friendly_errror(exception.get_message().as_deref()));
        toolkit::log_path_line(level, self.parser.get_path(), exception.get_line_number(), &text);
        if level == Level::Error {
            self.parser.errors_exist.set(true);
        }
    }
}

impl ErrorHandler for StyleErrorHandler {
    fn error(&mut self, exception: CSSParseException) -> CssResult<()> {
        self.report(Level::Error, &exception);
        Ok(())
    }
    fn fatal_error(&mut self, exception: CSSParseException) -> CssResult<()> {
        self.report(Level::Error, &exception);
        Ok(())
    }
    fn warning(&mut self, exception: CSSParseException) -> CssResult<()> {
        self.report(Level::Warning, &exception);
        Ok(())
    }
}

/// A Java charset name `InputStreamReader` accepts, canonicalized; `None` throws `UnsupportedEncodingException`.
fn charset(name: &str) -> Option<&'static str> {
    let n = name.to_ascii_uppercase().replace('_', "-");
    Some(match n.as_str() {
        "UTF-8" | "UTF8" => "UTF-8",
        "UTF-16" | "UTF16" | "UNICODE" => "UTF-16",
        "UTF-16BE" | "UNICODEBIGUNMARKED" | "X-UTF-16BE" => "UTF-16BE",
        "UTF-16LE" | "UNICODELITTLEUNMARKED" | "X-UTF-16LE" => "UTF-16LE",
        "ISO-8859-1" | "ISO8859-1" | "LATIN1" | "L1" | "ISO-LATIN-1" | "8859-1" | "CP819" | "IBM819" => "ISO-8859-1",
        "US-ASCII" | "ASCII" | "ASCII7" | "646" | "ISO646-US" => "US-ASCII",
        _ => return None,
    })
}

/// `new InputStreamReader(in, charsetName)` read to the end.
pub fn decode(bytes: &[u8], charset_name: &str) -> Vec<u16> {
    let be = |b: &[u8]| -> Vec<u16> {
        let mut v: Vec<u16> = b.chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        if b.len() % 2 == 1 {
            v.push(0xFFFD);
        }
        v
    };
    let le = |b: &[u8]| -> Vec<u16> {
        let mut v: Vec<u16> = b.chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        if b.len() % 2 == 1 {
            v.push(0xFFFD);
        }
        v
    };
    match charset(charset_name) {
        Some("UTF-16BE") => be(bytes),
        Some("UTF-16LE") => le(bytes),
        Some("UTF-16") => match bytes {
            [0xFE, 0xFF, rest @ ..] => be(rest),
            [0xFF, 0xFE, rest @ ..] => le(rest),
            _ => be(bytes),
        },
        Some("ISO-8859-1") => bytes.iter().map(|&b| b as u16).collect(),
        Some("US-ASCII") => bytes.iter().map(|&b| if b < 0x80 { b as u16 } else { 0xFFFD }).collect(),
        _ => jstr::decode_java_utf8(bytes),
    }
}

/// `StyleParser.readCSSCharset`: `Err(charsetName)` is `StyleSheetInvalidCharset`.
pub fn read_css_charset(input: &[u8]) -> Result<&'static str, String> {
    const LEADING: [&[u8]; 5] = [
        &[0xEF, 0xBB, 0xBF, 0x40, 0x63, 0x68, 0x61, 0x72, 0x73, 0x65, 0x74, 0x20, 0x22],
        &[0xEF, 0xBB, 0xBF],
        &[0x40, 0x63, 0x68, 0x61, 0x72, 0x73, 0x65, 0x74, 0x20, 0x22],
        &[0xFE, 0xFF],
        &[0xFF, 0xFE],
    ];
    const TABLE: [&str; 5] = ["specified", "UTF-8", "specified", "UTF-16BE", "UTF-16LE"];
    let max_bytes_to_read = 2 + (12 + 40);
    if input.is_empty() {
        return Ok("UTF-8");
    }
    let mut buffer = vec![0u8; max_bytes_to_read];
    let n = input.len().min(max_bytes_to_read);
    buffer[..n].copy_from_slice(&input[..n]);
    let found = LEADING.iter().position(|bytes| bytes.iter().enumerate().all(|(j, &b)| b == buffer[j]));
    let Some(found) = found else { return Ok("UTF-8") };
    if TABLE[found] != "specified" {
        return Ok(TABLE[found]);
    }
    let charset_index = LEADING[found].len();
    let mut charset_length = 0;
    let mut quote_position: i32 = -1;
    for (i, &b) in buffer.iter().enumerate().skip(charset_index) {
        if b == 0x22 {
            quote_position = i as i32;
            break;
        }
        charset_length += 1;
    }
    if quote_position > 0 && ((quote_position + 1) as usize) < buffer.len() && buffer[(quote_position + 1) as usize] == 0x3B {
        let test_charset: String = buffer[charset_index..charset_index + charset_length].iter().map(|&b| if b < 0x80 { b as char } else { '\u{FFFD}' }).collect();
        let Some(cs) = charset(&test_charset) else { return Err(test_charset) };
        let ch_buf: Vec<u16> = decode(input, cs).into_iter().take(2 + 12 + charset_length).collect();
        if !jstr::from_utf16(&ch_buf).contains(&test_charset) {
            return Err(test_charset);
        }
        return Ok(cs);
    }
    Ok("UTF-8")
}

impl StyleParser {
    fn new(css_path: Option<String>, mxml_path: Option<String>, mxml_line_number_offset: i32, check_deprecation: bool) -> Rc<StyleParser> {
        Rc::new(StyleParser {
            rules: RefCell::new(Vec::new()),
            css_path,
            errors_exist: Cell::new(false),
            mxml_path,
            mxml_line_number_offset,
            line_number: RefCell::new(Rc::new(Cell::new(-1))),
            check_deprecation,
        })
    }

    /// `StyleParser(cssPath, inputStream, handler, fontManager, checkDeprecation)`.
    pub fn from_stream(css_path: &str, input: &[u8], check_deprecation: bool) -> Rc<StyleParser> {
        let p = StyleParser::new(Some(css_path.to_string()), None, 0, check_deprecation);
        let charset_name = match read_css_charset(input) {
            Ok(c) => c,
            Err(name) => {
                toolkit::log_plain(Level::Error, &format!("@charset rule contains an invalid or unsupported charset encoding, {name}. Problem found in stylesheet {css_path}"));
                return p;
            }
        };
        let body = if input.starts_with(&[0xEF, 0xBB, 0xBF]) { &input[3..] } else { input };
        let chars = decode(body, charset_name);
        StyleParser::init(&p, chars);
        p
    }

    /// `StyleParser(mxmlPath, mxmlLineNumber, reader, ...)`: an `<fx:Style>` block's text.
    pub fn from_mxml(mxml_path: &str, mxml_line_number: i32, chars: Vec<u16>, check_deprecation: bool) -> Rc<StyleParser> {
        let p = StyleParser::new(None, Some(mxml_path.to_string()), mxml_line_number - 1, check_deprecation);
        StyleParser::init(&p, chars);
        p
    }

    /// `StyleParser(cssPath, reader, ...)`.
    pub fn from_reader(css_path: &str, chars: Vec<u16>, check_deprecation: bool) -> Rc<StyleParser> {
        let p = StyleParser::new(Some(css_path.to_string()), None, 0, check_deprecation);
        StyleParser::init(&p, chars);
        p
    }

    fn init(this: &Rc<StyleParser>, chars: Vec<u16>) {
        let mut parser = Parser::new();
        parser.set_line_number_offset(this.mxml_line_number_offset);
        *this.line_number.borrow_mut() = parser.line_number_cell();
        parser.set_document_handler(Box::new(StyleDocumentHandler::new(this.clone())));
        parser.set_error_handler(Box::new(StyleErrorHandler { parser: this.clone() }));
        if let Err(e) = parser.parse_style_sheet(chars) {
            let text = msg(get_user_friendly_errror(e.get_localized_message().as_deref()));
            toolkit::log_path_line(Level::Error, this.get_path(), -1, &text);
        }
    }

    pub fn errors_exist(&self) -> bool {
        self.errors_exist.get()
    }

    pub fn get_line_number(&self) -> i32 {
        self.line_number.borrow().get()
    }

    pub fn get_mxml_line_number(&self) -> i32 {
        self.mxml_line_number_offset
    }

    pub fn get_path(&self) -> &str {
        self.mxml_path.as_deref().or(self.css_path.as_deref()).unwrap_or("null")
    }

    pub fn add_rule(&self, rule: RuleRef) {
        self.rules.borrow_mut().push(rule);
    }

    pub fn get_rules(&self) -> Vec<RuleRef> {
        self.rules.borrow().clone()
    }

    pub fn warn_deprecation(&self, deprecated: &str, replacement: &str, line_number: i32) {
        if self.check_deprecation {
            let message = format!("'{deprecated}' has been deprecated since 3.0. Please use '{replacement}'.");
            toolkit::log_path_line(Level::Warning, self.get_path(), line_number, &message);
        }
    }

    pub fn warning(&self, css_exception: &CSSException) {
        let text = msg(get_user_friendly_errror(css_exception.get_message().as_deref()));
        toolkit::log_path_line(Level::Warning, self.get_path(), self.get_line_number(), &text);
    }

    pub fn error(&self, css_exception: &CSSException) {
        let text = msg(get_user_friendly_errror(css_exception.get_message().as_deref()));
        toolkit::log_path_line(Level::Error, self.get_path(), self.get_line_number(), &text);
    }
}
