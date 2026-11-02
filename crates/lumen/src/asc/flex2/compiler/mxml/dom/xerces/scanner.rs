//! Port of the Xerces-J 2.9.1 pipeline `SAXParserFactory` gives `MxmlScanner` (namespace-aware, non-validating):
//! `XMLVersionDetector`, `XMLEntityManager`/`XMLEntityScanner` for the document entity, `XMLScanner`,
//! `XMLDocumentFragmentScannerImpl`, `XMLDocumentScannerImpl`, `XMLNSDocumentScannerImpl` and the SAX events
//! `AbstractSAXParser` sends. A fatal error stops the parse, as `continue-after-fatal-error` is off; DTDs are not supported.

use super::messages::format_message;
use super::reader::{ByteStream, ReadError, Reader, Rewindable};
use super::xml_char;
use super::{Locator, SaxAttr, SaxHandler};

const DEFAULT_BUFFER_SIZE: usize = 2048;
const XMLNS_URI: &str = "http://www.w3.org/2000/xmlns/";
const XML_URI: &str = "http://www.w3.org/XML/1998/namespace";

pub enum Stop<E> {
    /// A fatal error's message, not yet reported to the handler.
    Fatal(String),
    /// `END_OF_DOCUMENT_ENTITY`.
    Eof,
    /// An `IOException` escaping the parse (a reader error before the content dispatchers run).
    Io(String),
    Handler(E),
}

type R<T, E> = Result<T, Stop<E>>;

/// `reportFatalError(key, args)`; Java passes `null` arguments wherever `args` is empty here.
fn fatal<T, E>(key: &str, args: &[String]) -> R<T, E> {
    Err(Stop::Fatal(format_message(key, if args.is_empty() { None } else { Some(args) })))
}

/// Xerces `QName.toString()`, as messages print an attribute name.
fn qname_string(q: &QName) -> String {
    let mut parts = Vec::new();
    if let Some(p) = &q.prefix {
        parts.push(format!("prefix=\"{p}\""));
    }
    parts.push(format!("localpart=\"{}\"", q.localpart));
    parts.push(format!("rawname=\"{}\"", q.rawname));
    if let Some(u) = &q.uri {
        parts.push(format!("uri=\"{u}\""));
    }
    parts.join(",")
}

fn s(x: &str) -> String {
    x.to_string()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    XmlDecl,
    StartOfMarkup,
    Comment,
    Pi,
    Doctype,
    Prolog,
    RootElement,
    Content,
    Reference,
    TrailingMisc,
    Terminated,
    Cdata,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dispatcher {
    XmlDecl,
    Prolog,
    Content,
    TrailingMisc,
}

#[derive(Clone, Default, Debug)]
struct QName {
    prefix: Option<String>,
    localpart: String,
    rawname: String,
    uri: Option<String>,
}

struct Attr {
    name: QName,
    value: String,
}

struct Entity {
    ch: Vec<u16>,
    position: usize,
    count: usize,
    start_position: usize,
    base_char_offset: i64,
    line_number: i32,
    column_number: i32,
    may_read_chunks: bool,
    encoding: String,
    reader: Reader,
    stream: Rewindable,
}

/// `NamespaceSupport`.
#[derive(Default)]
struct NamespaceSupport {
    namespace: Vec<(String, Option<String>)>,
    context: Vec<usize>,
}

impl NamespaceSupport {
    fn reset(&mut self) {
        self.namespace = vec![(s("xml"), Some(s(XML_URI))), (s("xmlns"), Some(s(XMLNS_URI)))];
        self.context = vec![self.namespace.len()];
    }

    fn push_context(&mut self) {
        self.context.push(self.namespace.len());
    }

    fn pop_context(&mut self) {
        let start = self.context.pop().unwrap_or(0);
        self.namespace.truncate(start);
    }

    fn declare_prefix(&mut self, prefix: &str, uri: Option<String>) -> bool {
        if prefix == "xml" || prefix == "xmlns" {
            return false;
        }
        let start = *self.context.last().unwrap();
        for i in (start..self.namespace.len()).rev() {
            if self.namespace[i].0 == prefix {
                self.namespace[i].1 = uri;
                return true;
            }
        }
        self.namespace.push((prefix.to_string(), uri));
        true
    }

    fn get_uri(&self, prefix: &str) -> Option<String> {
        for (p, u) in self.namespace.iter().rev() {
            if p == prefix {
                return u.clone();
            }
        }
        None
    }

    fn declared(&self) -> Vec<(String, Option<String>)> {
        let start = *self.context.last().unwrap();
        self.namespace[start..].to_vec()
    }
}

pub struct Xerces<'h, H: SaxHandler> {
    h: &'h mut H,
    entity: Option<Entity>,
    entity_handler: bool,
    started: bool,
    state: State,
    dispatcher: Dispatcher,
    markup_depth: i32,
    entity_depth: usize,
    entity_stack: Vec<i32>,
    element_stack: Vec<QName>,
    current_element: QName,
    attributes: Vec<Attr>,
    in_scan_content: bool,
    string_buffer: Vec<u16>,
    seen_doctype_decl: bool,
    ns: NamespaceSupport,
    standalone: bool,
}

pub fn parse<H: SaxHandler>(bytes: Vec<u8>, h: &mut H) -> Result<(), Stop<H::Error>> {
    let mut x = Xerces {
        h,
        entity: None,
        entity_handler: false,
        started: false,
        state: State::XmlDecl,
        dispatcher: Dispatcher::XmlDecl,
        markup_depth: 0,
        entity_depth: 0,
        entity_stack: vec![0; 4],
        element_stack: Vec::new(),
        current_element: QName::default(),
        attributes: Vec::new(),
        in_scan_content: false,
        string_buffer: Vec::new(),
        seen_doctype_decl: false,
        ns: NamespaceSupport::default(),
        standalone: false,
    };
    x.ns.reset();
    match x.run(bytes) {
        Err(Stop::Fatal(msg)) => {
            let loc = x.locator();
            match x.h.fatal_error(&msg, loc) {
                Ok(()) => Err(Stop::Fatal(msg)),
                Err(e) => Err(Stop::Handler(e)),
            }
        }
        r => r,
    }
}

fn to_string(v: &[u16]) -> String {
    String::from_utf16_lossy(v)
}

impl<'h, H: SaxHandler> Xerces<'h, H> {
    fn locator(&self) -> Locator {
        match &self.entity {
            Some(e) => Locator { available: self.started, line: e.line_number, column: e.column_number },
            None => Locator { available: self.started, line: -1, column: -1 },
        }
    }

    fn handler(&mut self, r: Result<(), H::Error>) -> R<(), H::Error> {
        r.map_err(Stop::Handler)
    }

    // ---- XMLEntityManager / XMLVersionDetector ----

    fn run(&mut self, bytes: Vec<u8>) -> R<(), H::Error> {
        let encoding = self.setup_current_entity(bytes)?;
        let _ = encoding;
        self.determine_doc_version()?;
        self.entity_handler = true;
        self.start_entity_doc()?;
        loop {
            if !self.dispatch()? {
                return Ok(());
            }
        }
    }

    fn setup_current_entity(&mut self, bytes: Vec<u8>) -> R<String, H::Error> {
        let mut stream = Rewindable::new(ByteStream::new(bytes));
        let mut b4 = [0i32; 4];
        let mut count = 0;
        while count < 4 {
            b4[count] = stream.read1() as u8 as i32;
            count += 1;
        }
        let (encoding, big_endian) = if count == 4 { get_encoding_name(&b4) } else { (s("UTF-8"), None) };
        stream.reset();
        if count > 2 && encoding == "UTF-8" && b4[0] & 0xFF == 0xEF && b4[1] & 0xFF == 0xBB && b4[2] & 0xFF == 0xBF {
            stream.skip(3);
        }
        let reader = create_reader(&encoding, big_endian);
        self.entity = Some(Entity {
            ch: vec![0; DEFAULT_BUFFER_SIZE],
            position: 0,
            count: 0,
            start_position: 0,
            base_char_offset: 0,
            line_number: 1,
            column_number: 1,
            may_read_chunks: false,
            encoding: encoding.clone(),
            reader,
            stream,
        });
        Ok(encoding)
    }

    /// `XMLVersionDetector.determineDocVersion` (XML 1.1 documents are scanned as 1.0).
    fn determine_doc_version(&mut self) -> R<(), H::Error> {
        let mut expected: Vec<u16> = "<?xml version=     ".encode_utf16().collect();
        let r: R<(), H::Error> = (|| {
            if !self.skip_string("<?xml")? {
                return Ok(());
            }
            if !self.skip_decl_spaces()? {
                self.fixup_current_entity(&expected, 5);
                return Ok(());
            }
            if !self.skip_string("version")? {
                self.fixup_current_entity(&expected, 6);
                return Ok(());
            }
            self.skip_decl_spaces()?;
            if self.peek_char()? != '=' as i32 {
                self.fixup_current_entity(&expected, 13);
                return Ok(());
            }
            self.scan_char()?;
            self.skip_decl_spaces()?;
            let quote = self.scan_char()?;
            expected[14] = quote as u16;
            for p in 0..3 {
                expected[15 + p] = self.scan_char()? as u16;
            }
            expected[18] = self.scan_char()? as u16;
            self.fixup_current_entity(&expected, 19);
            Ok(())
        })();
        match r {
            Err(Stop::Eof) => fatal("PrematureEOF", &[]),
            Err(Stop::Fatal(m)) => Err(Stop::Io(m)),
            r => r,
        }
    }

    fn fixup_current_entity(&mut self, scanned: &[u16], length: usize) {
        let e = self.entity.as_mut().unwrap();
        if e.count - e.position + length > e.ch.len() {
            let mut ch = vec![0u16; length + e.count - e.position + 1];
            let n = e.ch.len().min(ch.len());
            ch[..n].copy_from_slice(&e.ch[..n]);
            e.ch = ch;
        }
        if e.position < length {
            e.ch.copy_within(e.position..e.count, length);
            e.count += length - e.position;
        } else {
            for i in length..e.position {
                e.ch[i] = ' ' as u16;
            }
        }
        e.ch[..length].copy_from_slice(&scanned[..length]);
        e.position = 0;
        e.base_char_offset = 0;
        e.start_position = 0;
        e.column_number = 1;
        e.line_number = 1;
    }

    /// `XMLDocumentScannerImpl.startEntity("[xml]")`.
    fn start_entity_doc(&mut self) -> R<(), H::Error> {
        self.entity_stack[self.entity_depth] = self.markup_depth;
        self.entity_depth += 1;
        self.started = true;
        let loc = self.locator();
        let r = self.h.start_document(loc);
        self.handler(r)
    }

    /// `XMLEntityManager.endEntity` for the document entity: the scanners' `endEntity`, then the entity is gone.
    fn end_entity_doc(&mut self) -> R<(), H::Error> {
        if self.entity_handler {
            if self.in_scan_content && !self.string_buffer.is_empty() {
                let text = to_string(&self.string_buffer);
                let loc = self.locator();
                let r = self.h.characters(&text, loc);
                self.handler(r)?;
                self.string_buffer.clear();
            }
            self.entity_depth -= 1;
            if self.markup_depth != self.entity_stack[self.entity_depth] {
                return fatal("MarkupEntityMismatch", &[]);
            }
            let loc = self.locator();
            let r = self.h.end_document(loc);
            self.handler(r)?;
        }
        self.entity = None;
        Ok(())
    }

    // ---- XMLEntityScanner ----

    fn e(&mut self) -> &mut Entity {
        self.entity.as_mut().expect("no current entity")
    }

    fn load(&mut self, offset: usize, change_entity: bool) -> R<bool, H::Error> {
        let e = self.entity.as_mut().expect("no current entity");
        e.base_char_offset += e.position as i64 - e.start_position as i64;
        let length = if e.may_read_chunks { e.ch.len() - offset } else { 64 };
        let chunks = e.may_read_chunks;
        let count = match e.reader.read(&mut e.ch, offset, length, &mut e.stream, chunks) {
            Ok(c) => c,
            Err(ReadError::Malformed { key, args }) => return Err(Stop::Fatal(format_message(key, Some(&args)))),
            Err(ReadError::CharConversion) => return Err(Stop::Fatal(format_message("CharConversionFailure", None))),
        };
        let mut entity_changed = false;
        if count != -1 {
            if count != 0 {
                e.count = count as usize + offset;
                e.position = offset;
                e.start_position = offset;
            }
        } else {
            e.count = offset;
            e.position = offset;
            e.start_position = offset;
            entity_changed = true;
            if change_entity {
                self.end_entity_doc()?;
                return Err(Stop::Eof);
            }
        }
        Ok(entity_changed)
    }

    fn peek_char(&mut self) -> R<i32, H::Error> {
        if self.e().position == self.e().count {
            self.load(0, true)?;
        }
        let e = self.e();
        let c = e.ch[e.position] as i32;
        Ok(if c == '\r' as i32 { '\n' as i32 } else { c })
    }

    fn scan_char(&mut self) -> R<i32, H::Error> {
        if self.e().position == self.e().count {
            self.load(0, true)?;
        }
        let e = self.e();
        let mut c = e.ch[e.position] as i32;
        e.position += 1;
        if c == '\n' as i32 || c == '\r' as i32 {
            e.line_number += 1;
            e.column_number = 1;
            if e.position == e.count {
                e.ch[0] = c as u16;
                self.load(1, false)?;
            }
            let e = self.e();
            if c == '\r' as i32 {
                let n = e.ch[e.position];
                e.position += 1;
                if n != '\n' as u16 {
                    e.position -= 1;
                }
                c = '\n' as i32;
            }
        }
        self.e().column_number += 1;
        Ok(c)
    }

    fn scan_name_impl(&mut self, start: fn(i32) -> bool, part: fn(i32) -> bool) -> R<Option<String>, H::Error> {
        if self.e().position == self.e().count {
            self.load(0, true)?;
        }
        let mut offset = self.e().position;
        if start(self.e().ch[offset] as i32) {
            self.e().position += 1;
            if self.e().position == self.e().count {
                let e = self.e();
                e.ch[0] = e.ch[offset];
                offset = 0;
                if self.load(1, false)? {
                    let e = self.e();
                    e.column_number += 1;
                    return Ok(Some(to_string(&e.ch[0..1])));
                }
            }
            loop {
                let e = self.e();
                if !part(e.ch[e.position] as i32) {
                    break;
                }
                e.position += 1;
                if e.position == e.count {
                    let length = e.position - offset;
                    if length == e.ch.len() {
                        let mut ch = vec![0u16; e.ch.len() << 1];
                        ch[..length].copy_from_slice(&e.ch[offset..offset + length]);
                        e.ch = ch;
                    } else {
                        e.ch.copy_within(offset..offset + length, 0);
                    }
                    offset = 0;
                    if self.load(length, false)? {
                        break;
                    }
                }
            }
        }
        let e = self.e();
        let length = e.position - offset;
        e.column_number += length as i32;
        Ok(if length > 0 { Some(to_string(&e.ch[offset..offset + length])) } else { None })
    }

    fn scan_name(&mut self) -> R<Option<String>, H::Error> {
        self.scan_name_impl(xml_char::is_name_start, xml_char::is_name)
    }

    fn scan_ncname(&mut self) -> R<Option<String>, H::Error> {
        self.scan_name_impl(xml_char::is_ncname_start, xml_char::is_ncname)
    }

    fn scan_qname(&mut self, qname: &mut QName) -> R<bool, H::Error> {
        if self.e().position == self.e().count {
            self.load(0, true)?;
        }
        let mut offset = self.e().position;
        if xml_char::is_ncname_start(self.e().ch[offset] as i32) {
            self.e().position += 1;
            if self.e().position == self.e().count {
                let e = self.e();
                e.ch[0] = e.ch[offset];
                offset = 0;
                if self.load(1, false)? {
                    let e = self.e();
                    e.column_number += 1;
                    let name = to_string(&e.ch[0..1]);
                    *qname = QName { prefix: None, localpart: name.clone(), rawname: name, uri: None };
                    return Ok(true);
                }
            }
            let mut index: i64 = -1;
            loop {
                let e = self.e();
                let c = e.ch[e.position];
                if !xml_char::is_name(c as i32) {
                    break;
                }
                if c == ':' as u16 {
                    if index != -1 {
                        break;
                    }
                    index = e.position as i64;
                }
                e.position += 1;
                if e.position == e.count {
                    let length = e.position - offset;
                    if length == e.ch.len() {
                        let mut ch = vec![0u16; e.ch.len() << 1];
                        ch[..length].copy_from_slice(&e.ch[offset..offset + length]);
                        e.ch = ch;
                    } else {
                        e.ch.copy_within(offset..offset + length, 0);
                    }
                    if index != -1 {
                        index -= offset as i64;
                    }
                    offset = 0;
                    if self.load(length, false)? {
                        break;
                    }
                }
            }
            let e = self.e();
            let length = e.position - offset;
            e.column_number += length as i32;
            if length > 0 {
                let rawname = to_string(&e.ch[offset..offset + length]);
                let (prefix, localpart) = if index != -1 {
                    let index = index as usize;
                    let prefix_length = index - offset;
                    let prefix = to_string(&e.ch[offset..offset + prefix_length]);
                    let len = length - prefix_length - 1;
                    let ok = xml_char::is_ncname_start(e.ch[index + 1] as i32);
                    let local = to_string(&e.ch[index + 1..index + 1 + len]);
                    if !ok {
                        return fatal("IllegalQName", &[]);
                    }
                    (Some(prefix), local)
                } else {
                    (None, rawname.clone())
                };
                *qname = QName { prefix, localpart, rawname, uri: None };
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The newline normalization `scanContent`, `scanLiteral` and `scanData` share; returns `(offset, newlines, done)`.
    fn normalize_newlines(&mut self, set_count: bool) -> R<(usize, i32, bool), H::Error> {
        let mut offset = self.e().position;
        let c = self.e().ch[offset];
        let mut newlines = 0;
        if c == '\n' as u16 || c == '\r' as u16 {
            loop {
                let e = self.e();
                let c = e.ch[e.position];
                e.position += 1;
                if c == '\r' as u16 {
                    newlines += 1;
                    e.line_number += 1;
                    e.column_number = 1;
                    if e.position == e.count {
                        offset = 0;
                        e.base_char_offset += e.position as i64 - e.start_position as i64;
                        e.position = newlines as usize;
                        e.start_position = newlines as usize;
                        if self.load(newlines as usize, false)? {
                            break;
                        }
                    }
                    let e = self.e();
                    if e.ch[e.position] == '\n' as u16 {
                        e.position += 1;
                        offset += 1;
                    } else {
                        newlines += 1;
                    }
                } else if c == '\n' as u16 {
                    newlines += 1;
                    e.line_number += 1;
                    e.column_number = 1;
                    if e.position == e.count {
                        offset = 0;
                        e.base_char_offset += e.position as i64 - e.start_position as i64;
                        e.position = newlines as usize;
                        e.start_position = newlines as usize;
                        if set_count {
                            e.count = newlines as usize;
                        }
                        if self.load(newlines as usize, false)? {
                            break;
                        }
                    }
                } else {
                    e.position -= 1;
                    break;
                }
                let e = self.e();
                if e.position >= e.count.saturating_sub(1) {
                    break;
                }
            }
            let e = self.e();
            for i in offset..e.position {
                e.ch[i] = '\n' as u16;
            }
            if e.position == e.count.wrapping_sub(1) {
                return Ok((offset, newlines, true));
            }
        }
        Ok((offset, newlines, false))
    }

    fn prepare_scan(&mut self) -> R<(), H::Error> {
        if self.e().position == self.e().count {
            self.load(0, true)?;
        } else if self.e().position == self.e().count - 1 {
            let e = self.e();
            e.ch[0] = e.ch[e.count - 1];
            self.load(1, false)?;
            let e = self.e();
            e.position = 0;
            e.start_position = 0;
        }
        Ok(())
    }

    /// `scanContent(content)`: the content and the next character (-1 at the buffer's end).
    fn scan_content(&mut self) -> R<(Vec<u16>, i32), H::Error> {
        self.prepare_scan()?;
        let (offset, newlines, done) = self.normalize_newlines(false)?;
        let e = self.e();
        if done {
            let length = e.position - offset;
            return Ok((e.ch[offset..offset + length].to_vec(), -1));
        }
        while e.position < e.count {
            let c = e.ch[e.position];
            e.position += 1;
            if !xml_char::is_content(c as i32) {
                e.position -= 1;
                break;
            }
        }
        let length = e.position - offset;
        e.column_number += length as i32 - newlines;
        let content = e.ch[offset..offset + length].to_vec();
        let c = if e.position != e.count {
            let c = e.ch[e.position] as i32;
            if c == '\r' as i32 { '\n' as i32 } else { c }
        } else {
            -1
        };
        Ok((content, c))
    }

    fn scan_literal(&mut self, quote: i32) -> R<(Vec<u16>, i32), H::Error> {
        self.prepare_scan()?;
        let (offset, newlines, done) = self.normalize_newlines(false)?;
        let e = self.e();
        if done {
            let length = e.position - offset;
            return Ok((e.ch[offset..offset + length].to_vec(), -1));
        }
        while e.position < e.count {
            let c = e.ch[e.position] as i32;
            e.position += 1;
            if c == quote || c == '%' as i32 || !xml_char::is_content(c) {
                e.position -= 1;
                break;
            }
        }
        let length = e.position - offset;
        e.column_number += length as i32 - newlines;
        let content = e.ch[offset..offset + length].to_vec();
        let c = if e.position != e.count { e.ch[e.position] as i32 } else { -1 };
        Ok((content, c))
    }

    /// `scanData(delimiter, buffer)`: true while the delimiter has not been found.
    fn scan_data(&mut self, delimiter: &str, buffer: &mut Vec<u16>) -> R<bool, H::Error> {
        let delim: Vec<u16> = delimiter.encode_utf16().collect();
        let delim_len = delim.len();
        let char_at0 = delim[0];
        let mut found = false;
        if self.e().position == self.e().count {
            self.load(0, true)?;
        }
        let mut next_entity = false;
        while self.e().position + delim_len > self.e().count && !next_entity {
            let e = self.e();
            let (p, c) = (e.position, e.count);
            e.ch.copy_within(p..c, 0);
            next_entity = self.load(c - p, false)?;
            let e = self.e();
            e.position = 0;
            e.start_position = 0;
        }
        if self.e().position + delim_len > self.e().count {
            let e = self.e();
            let length = e.count - e.position;
            buffer.extend_from_slice(&e.ch[e.position..e.position + length]);
            e.column_number += e.count as i32;
            e.base_char_offset += e.position as i64 - e.start_position as i64;
            e.position = e.count;
            e.start_position = e.count;
            self.load(0, true)?;
            return Ok(false);
        }
        let (offset, newlines, done) = self.normalize_newlines(true)?;
        if done {
            let e = self.e();
            buffer.extend_from_slice(&e.ch[offset..e.position]);
            return Ok(true);
        }
        let e = self.e();
        'outer: while e.position < e.count {
            let c = e.ch[e.position];
            e.position += 1;
            if c == char_at0 {
                let delim_offset = e.position - 1;
                for i in 1..delim_len {
                    if e.position == e.count {
                        e.position -= i;
                        break 'outer;
                    }
                    let c = e.ch[e.position];
                    e.position += 1;
                    if delim[i] != c {
                        e.position -= 1;
                        break;
                    }
                }
                if e.position == delim_offset + delim_len {
                    found = true;
                    break;
                }
            } else if c == '\n' as u16 || c == '\r' as u16 {
                e.position -= 1;
                break;
            } else if xml_char::is_invalid(c as i32) {
                e.position -= 1;
                let length = e.position - offset;
                e.column_number += length as i32 - newlines;
                buffer.extend_from_slice(&e.ch[offset..offset + length]);
                return Ok(true);
            }
        }
        let mut length = e.position - offset;
        e.column_number += length as i32 - newlines;
        if found {
            length -= delim_len;
        }
        buffer.extend_from_slice(&e.ch[offset..offset + length]);
        Ok(!found)
    }

    fn skip_char(&mut self, c: i32) -> R<bool, H::Error> {
        if self.e().position == self.e().count {
            self.load(0, true)?;
        }
        let e = self.e();
        let cc = e.ch[e.position] as i32;
        if cc == c {
            e.position += 1;
            if c == '\n' as i32 {
                e.line_number += 1;
                e.column_number = 1;
            } else {
                e.column_number += 1;
            }
            return Ok(true);
        } else if c == '\n' as i32 && cc == '\r' as i32 {
            if e.position == e.count {
                e.ch[0] = cc as u16;
                self.load(1, false)?;
            }
            let e = self.e();
            e.position += 1;
            if e.ch[e.position] == '\n' as u16 {
                e.position += 1;
            }
            e.line_number += 1;
            e.column_number = 1;
            return Ok(true);
        }
        Ok(false)
    }

    fn skip_spaces(&mut self) -> R<bool, H::Error> {
        if self.e().position == self.e().count {
            self.load(0, true)?;
        }
        let mut c = { let e = self.e(); e.ch[e.position] as i32 };
        if !xml_char::is_space(c) {
            return Ok(false);
        }
        loop {
            let mut entity_changed = false;
            if c == '\n' as i32 || c == '\r' as i32 {
                let e = self.e();
                e.line_number += 1;
                e.column_number = 1;
                if e.position == e.count - 1 {
                    e.ch[0] = c as u16;
                    entity_changed = self.load(1, true)?;
                    if !entity_changed {
                        let e = self.e();
                        e.position = 0;
                        e.start_position = 0;
                    }
                }
                if c == '\r' as i32 {
                    let e = self.e();
                    e.position += 1;
                    if e.ch[e.position] != '\n' as u16 {
                        e.position -= 1;
                    }
                }
            } else {
                self.e().column_number += 1;
            }
            if !entity_changed {
                self.e().position += 1;
            }
            if self.e().position == self.e().count {
                self.load(0, true)?;
            }
            c = { let e = self.e(); e.ch[e.position] as i32 };
            if !xml_char::is_space(c) {
                break;
            }
        }
        Ok(true)
    }

    fn skip_decl_spaces(&mut self) -> R<bool, H::Error> {
        self.skip_spaces()
    }

    fn skip_string(&mut self, string: &str) -> R<bool, H::Error> {
        if self.e().position == self.e().count {
            self.load(0, true)?;
        }
        let chars: Vec<u16> = string.encode_utf16().collect();
        let length = chars.len();
        for i in 0..length {
            let e = self.e();
            let c = e.ch[e.position];
            e.position += 1;
            if c != chars[i] {
                e.position -= i + 1;
                return Ok(false);
            }
            if i < length - 1 && e.position == e.count {
                let (cnt, i1) = (e.count, i + 1);
                e.ch.copy_within(cnt - i - 1..cnt, 0);
                if self.load(i1, false)? {
                    let e = self.e();
                    e.start_position -= i1;
                    e.position -= i1;
                    return Ok(false);
                }
            }
        }
        self.e().column_number += length as i32;
        Ok(true)
    }

    fn set_encoding(&mut self, encoding: &str) {
        let e = self.e();
        if e.encoding != encoding {
            if e.encoding.starts_with("UTF-16") {
                let enc = encoding.to_ascii_uppercase();
                if enc == "UTF-16" || enc == "ISO-10646-UCS-4" || enc == "ISO-10646-UCS-2" {
                    return;
                }
            }
            e.reader = create_reader(encoding, None);
            e.encoding = encoding.to_string();
        }
    }

    // ---- XMLScanner ----

    fn scan_xml_decl_or_text_decl(&mut self) -> R<(Option<String>, Option<String>, Option<String>), H::Error> {
        let (mut version, mut encoding, mut standalone) = (None, None, None);
        let mut state = 0;
        let mut data_found_for_target = false;
        let mut saw_space = self.skip_decl_spaces()?;
        while self.peek_char()? != '?' as i32 {
            data_found_for_target = true;
            let (name, value) = self.scan_pseudo_attribute()?;
            match state {
                0 => {
                    if name == "version" {
                        if !saw_space {
                            return fatal("SpaceRequiredBeforeVersionInXMLDecl", &[]);
                        }
                        state = 1;
                        if value != "1.0" {
                            return fatal("VersionNotSupported", &[value]);
                        }
                        version = Some(value);
                    } else if name == "encoding" {
                        return fatal("VersionInfoRequired", &[]);
                    } else {
                        return fatal("VersionInfoRequired", &[]);
                    }
                }
                1 => {
                    if name == "encoding" {
                        if !saw_space {
                            return fatal("SpaceRequiredBeforeEncodingInXMLDecl", &[]);
                        }
                        encoding = Some(value);
                        state = 2;
                    } else if name == "standalone" {
                        if !saw_space {
                            return fatal("SpaceRequiredBeforeStandalone", &[]);
                        }
                        state = 3;
                        if value != "yes" && value != "no" {
                            return fatal("SDDeclInvalid", &[value]);
                        }
                        standalone = Some(value);
                    } else {
                        return fatal("EncodingDeclRequired", &[]);
                    }
                }
                2 => {
                    if name == "standalone" {
                        if !saw_space {
                            return fatal("SpaceRequiredBeforeStandalone", &[]);
                        }
                        state = 3;
                        if value != "yes" && value != "no" {
                            return fatal("SDDeclInvalid", &[value]);
                        }
                        standalone = Some(value);
                    } else {
                        return fatal("EncodingDeclRequired", &[]);
                    }
                }
                _ => return fatal("NoMorePseudoAttributes", &[]),
            }
            saw_space = self.skip_decl_spaces()?;
        }
        if !data_found_for_target && version.is_none() {
            return fatal("VersionInfoRequired", &[]);
        }
        if !self.skip_char('?' as i32)? {
            return fatal("XMLDeclUnterminated", &[]);
        }
        if !self.skip_char('>' as i32)? {
            return fatal("XMLDeclUnterminated", &[]);
        }
        Ok((version, encoding, standalone))
    }

    fn scan_pseudo_attribute(&mut self) -> R<(String, String), H::Error> {
        let name = self.scan_name()?;
        let Some(name) = name else { return fatal("PseudoAttrNameExpected", &[]) };
        self.skip_decl_spaces()?;
        if !self.skip_char('=' as i32)? {
            return fatal("EqRequiredInXMLDecl", &[name]);
        }
        self.skip_decl_spaces()?;
        let quote = self.peek_char()?;
        if quote != '\'' as i32 && quote != '"' as i32 {
            return fatal("QuoteRequiredInXMLDecl", &[name]);
        }
        self.scan_char()?;
        let (mut value, mut c) = self.scan_literal(quote)?;
        if c != quote {
            let mut buf = Vec::new();
            loop {
                buf.extend_from_slice(&value);
                if c != -1 {
                    if c == '&' as i32 || c == '%' as i32 || c == '<' as i32 || c == ']' as i32 {
                        let ch = self.scan_char()?;
                        buf.push(ch as u16);
                    } else if xml_char::is_high_surrogate(c) {
                        self.scan_surrogates(&mut buf)?;
                    } else if xml_char::is_invalid(c) {
                        return fatal("InvalidCharInXMLDecl", &[format!("{c:x}")]);
                    }
                }
                let r = self.scan_literal(quote)?;
                value = r.0;
                c = r.1;
                if c == quote {
                    break;
                }
            }
            buf.extend_from_slice(&value);
            value = buf;
        }
        if !self.skip_char(quote)? {
            return fatal("CloseQuoteMissingInXMLDecl", &[name]);
        }
        Ok((name, to_string(&value)))
    }

    fn scan_pi(&mut self) -> R<(), H::Error> {
        let target = self.scan_ncname()?;
        let Some(target) = target else { return fatal("PITargetRequired", &[]) };
        self.scan_pi_data(&target)
    }

    fn scan_pi_data(&mut self, target: &str) -> R<(), H::Error> {
        let t: Vec<char> = target.chars().collect();
        if t.len() == 3 && t[0].to_lowercase().eq(['x']) && t[1].to_lowercase().eq(['m']) && t[2].to_lowercase().eq(['l']) {
            return fatal("ReservedPITarget", &[]);
        }
        let data;
        if !self.skip_spaces()? {
            if self.skip_string("?>")? {
                self.markup_depth -= 1;
                let loc = self.locator();
                let r = self.h.processing_instruction(target, "", loc);
                return self.handler(r);
            }
            if self.peek_char()? == ':' as i32 {
                self.scan_char()?;
                let mut b = format!("{target}:");
                if let Some(n) = self.scan_name()? {
                    b.push_str(&n);
                }
                return fatal("ColonNotLegalWithNS", &[b]);
            }
            return fatal("SpaceRequiredInPI", &[]);
        }
        let mut buf = Vec::new();
        while self.scan_data("?>", &mut buf)? {
            let c = self.peek_char()?;
            if c != -1 {
                if xml_char::is_high_surrogate(c) {
                    self.scan_surrogates(&mut buf)?;
                } else if xml_char::is_invalid(c) {
                    return fatal("InvalidCharInPI", &[format!("{c:x}")]);
                }
            }
        }
        data = to_string(&buf);
        self.markup_depth -= 1;
        let loc = self.locator();
        let r = self.h.processing_instruction(target, &data, loc);
        self.handler(r)
    }

    fn scan_comment(&mut self) -> R<(), H::Error> {
        let mut buf = Vec::new();
        while self.scan_data("--", &mut buf)? {
            let c = self.peek_char()?;
            if c != -1 {
                if xml_char::is_high_surrogate(c) {
                    self.scan_surrogates(&mut buf)?;
                } else if xml_char::is_invalid(c) {
                    return fatal("InvalidCharInComment", &[format!("{c:x}")]);
                }
            }
        }
        if !self.skip_char('>' as i32)? {
            return fatal("DashDashInComment", &[]);
        }
        self.markup_depth -= 1;
        let text = to_string(&buf);
        let loc = self.locator();
        let r = self.h.comment(&text, loc);
        self.handler(r)
    }

    fn scan_surrogates(&mut self, buf: &mut Vec<u16>) -> R<bool, H::Error> {
        let high = self.scan_char()?;
        let low = self.peek_char()?;
        if !xml_char::is_low_surrogate(low) {
            return fatal("InvalidCharInContent", &[format!("{high:x}")]);
        }
        self.scan_char()?;
        let c = xml_char::supplemental(high as u16, low as u16);
        if xml_char::is_invalid(c) {
            return fatal("InvalidCharInContent", &[format!("{c:x}")]);
        }
        buf.push(high as u16);
        buf.push(low as u16);
        Ok(true)
    }

    fn scan_char_reference_value(&mut self, buf: &mut Vec<u16>, buf2: Option<&mut Vec<u16>>) -> R<i32, H::Error> {
        let mut buf2 = buf2;
        let mut hex = false;
        let mut digits = String::new();
        let is_hex = |c: i32| (('0' as i32)..=('9' as i32)).contains(&c) || (('a' as i32)..=('f' as i32)).contains(&c) || (('A' as i32)..=('F' as i32)).contains(&c);
        let is_dec = |c: i32| (('0' as i32)..=('9' as i32)).contains(&c);
        if self.skip_char('x' as i32)? {
            if let Some(b) = buf2.as_mut() {
                b.push('x' as u16);
            }
            hex = true;
            let mut c = self.peek_char()?;
            if !is_hex(c) {
                return fatal("HexdigitRequiredInCharRef", &[]);
            }
            while is_hex(c) {
                if let Some(b) = buf2.as_mut() {
                    b.push(c as u16);
                }
                self.scan_char()?;
                digits.push(char::from_u32(c as u32).unwrap());
                c = self.peek_char()?;
            }
        } else {
            let mut c = self.peek_char()?;
            if !is_dec(c) {
                return fatal("DigitRequiredInCharRef", &[]);
            }
            while is_dec(c) {
                if let Some(b) = buf2.as_mut() {
                    b.push(c as u16);
                }
                self.scan_char()?;
                digits.push(char::from_u32(c as u32).unwrap());
                c = self.peek_char()?;
            }
        }
        if !self.skip_char(';' as i32)? {
            return fatal("SemicolonRequiredInCharRef", &[]);
        }
        if let Some(b) = buf2.as_mut() {
            b.push(';' as u16);
        }
        let value = i32::from_str_radix(&digits, if hex { 16 } else { 10 });
        let value = match value {
            Ok(v) if !xml_char::is_invalid(v) => v,
            _ => return fatal("InvalidCharRef", &[if hex { format!("x{digits}") } else { digits }]),
        };
        if !xml_char::is_supplemental(value) {
            buf.push(value as u16);
        } else {
            buf.push(xml_char::high_surrogate(value));
            buf.push(xml_char::low_surrogate(value));
        }
        Ok(value)
    }

    /// `scanAttributeValue`: the normalized value.
    fn scan_attribute_value(&mut self, atype: &str, ename: &str) -> R<String, H::Error> {
        let quote = self.peek_char()?;
        if quote != '\'' as i32 && quote != '"' as i32 {
            return fatal("OpenQuoteExpected", &[s(ename), s(atype)]);
        }
        self.scan_char()?;
        let (mut value, mut c) = self.scan_literal(quote)?;
        normalize_whitespace(&mut value);
        if c != quote {
            let mut buf: Vec<u16> = Vec::new();
            loop {
                buf.extend_from_slice(&value);
                if c == '&' as i32 {
                    self.skip_char('&' as i32)?;
                    if self.skip_char('#' as i32)? {
                        self.scan_char_reference_value(&mut buf, None)?;
                    } else {
                        let name = self.scan_name()?;
                        let Some(name) = name else { return fatal("NameRequiredInReference", &[]) };
                        if !self.skip_char(';' as i32)? {
                            return fatal("SemicolonRequiredInReference", &[name]);
                        }
                        match name.as_str() {
                            "amp" => buf.push('&' as u16),
                            "apos" => buf.push('\'' as u16),
                            "lt" => buf.push('<' as u16),
                            "gt" => buf.push('>' as u16),
                            "quot" => buf.push('"' as u16),
                            _ => return fatal("EntityNotDeclared", &[name]),
                        }
                    }
                } else if c == '<' as i32 {
                    return fatal("LessthanInAttValue", &[s(ename), s(atype)]);
                } else if c == '%' as i32 || c == ']' as i32 {
                    self.scan_char()?;
                    buf.push(c as u16);
                } else if c == '\n' as i32 || c == '\r' as i32 {
                    self.scan_char()?;
                    buf.push(' ' as u16);
                } else if c != -1 && xml_char::is_high_surrogate(c) {
                    let mut b3 = Vec::new();
                    if self.scan_surrogates(&mut b3)? {
                        buf.extend_from_slice(&b3);
                    }
                } else if c != -1 && xml_char::is_invalid(c) {
                    return fatal("InvalidCharInAttValue", &[s(ename), s(atype), format!("{c:x}")]);
                }
                let r = self.scan_literal(quote)?;
                value = r.0;
                c = r.1;
                normalize_whitespace(&mut value);
                if c == quote {
                    buf.extend_from_slice(&value);
                    value = buf;
                    break;
                }
            }
        }
        let cc = self.scan_char()?;
        if cc != quote {
            return fatal("CloseQuoteExpected", &[s(ename), s(atype)]);
        }
        Ok(to_string(&value))
    }

    // ---- XMLNSDocumentScannerImpl ----

    fn scan_start_element(&mut self) -> R<bool, H::Error> {
        let mut qn = QName::default();
        self.scan_qname(&mut qn)?;
        let rawname = qn.rawname.clone();
        self.ns.push_context();
        self.element_stack.push(qn.clone());
        self.current_element = qn.clone();
        let mut empty = false;
        self.attributes.clear();
        loop {
            let saw_space = self.skip_spaces()?;
            let c = self.peek_char()?;
            if c == '>' as i32 {
                self.scan_char()?;
                break;
            } else if c == '/' as i32 {
                self.scan_char()?;
                if !self.skip_char('>' as i32)? {
                    return fatal("ElementUnterminated", &[rawname]);
                }
                empty = true;
                break;
            } else if !xml_char::is_name_start(c) || !saw_space {
                return fatal("ElementUnterminated", &[rawname]);
            }
            self.scan_ns_attribute()?;
        }
        if qn.prefix.as_deref() == Some("xmlns") {
            return fatal("ElementXMLNSPrefix", &[qn.rawname.clone()]);
        }
        let prefix = qn.prefix.clone().unwrap_or_default();
        qn.uri = self.ns.get_uri(&prefix);
        if qn.prefix.is_none() && qn.uri.is_some() {
            qn.prefix = Some(String::new());
        }
        if qn.prefix.is_some() && qn.uri.is_none() {
            return fatal("ElementPrefixUnbound", &[qn.prefix.clone().unwrap(), qn.rawname.clone()]);
        }
        if let Some(top) = self.element_stack.last_mut() {
            top.uri = qn.uri.clone();
            top.prefix = qn.prefix.clone();
        }
        self.current_element = qn.clone();
        for i in 0..self.attributes.len() {
            let aprefix = self.attributes[i].name.prefix.clone().unwrap_or_default();
            let uri = self.ns.get_uri(&aprefix);
            if self.attributes[i].name.uri.is_some() && self.attributes[i].name.uri == uri {
                continue;
            }
            if !aprefix.is_empty() {
                if uri.is_none() {
                    return fatal("AttributePrefixUnbound", &[qn.rawname.clone(), self.attributes[i].name.rawname.clone(), aprefix]);
                }
                self.attributes[i].name.uri = uri;
            }
        }
        if self.attributes.len() > 1 {
            'dup: for i in 0..self.attributes.len() - 1 {
                for j in i + 1..self.attributes.len() {
                    let (a, b) = (&self.attributes[i].name, &self.attributes[j].name);
                    if a.localpart == b.localpart && a.uri == b.uri {
                        let name = b.clone();
                        match &name.uri {
                            Some(u) => return fatal("AttributeNSNotUnique", &[qn.rawname.clone(), name.localpart.clone(), u.clone()]),
                            None => return fatal("AttributeNotUnique", &[qn.rawname.clone(), name.rawname.clone()]),
                        }
                    }
                }
                if i > usize::MAX - 1 {
                    break 'dup;
                }
            }
        }
        if empty {
            self.markup_depth -= 1;
            if self.markup_depth < self.entity_stack[self.entity_depth - 1] {
                return fatal("ElementEntityMismatch", &[self.current_element.rawname.clone()]);
            }
            self.sax_start_element(&qn)?;
            self.sax_end_element(&qn)?;
            self.ns.pop_context();
            self.element_stack.pop();
        } else {
            self.sax_start_element(&qn)?;
        }
        Ok(empty)
    }

    fn scan_ns_attribute(&mut self) -> R<(), H::Error> {
        let mut aq = QName::default();
        self.scan_qname(&mut aq)?;
        self.skip_spaces()?;
        if !self.skip_char('=' as i32)? {
            return fatal("EqRequiredInAttribute", &[self.current_element.rawname.clone(), aq.rawname.clone()]);
        }
        self.skip_spaces()?;
        let index = self.attributes.len();
        self.attributes.push(Attr { name: aq.clone(), value: String::new() });
        let ename = self.current_element.rawname.clone();
        let value = self.scan_attribute_value(&aq.rawname, &ename)?;
        self.attributes[index].value = value.clone();
        let localpart = aq.localpart.clone();
        let prefix = aq.prefix.clone().unwrap_or_default();
        if prefix == "xmlns" || (prefix.is_empty() && localpart == "xmlns") {
            let uri = value;
            if prefix == "xmlns" && localpart == "xmlns" {
                return fatal("CantBindXMLNS", &[qname_string(&aq)]);
            }
            if uri == XMLNS_URI {
                return fatal("CantBindXMLNS", &[qname_string(&aq)]);
            }
            if localpart == "xml" {
                if uri != XML_URI {
                    return fatal("CantBindXML", &[qname_string(&aq)]);
                }
            } else if uri == XML_URI {
                return fatal("CantBindXML", &[qname_string(&aq)]);
            }
            let p = if localpart != "xmlns" { localpart.clone() } else { String::new() };
            if uri.is_empty() && localpart != "xmlns" {
                return fatal("EmptyPrefixedAttName", &[qname_string(&aq)]);
            }
            self.ns.declare_prefix(&p, if uri.is_empty() { None } else { Some(uri) });
            self.attributes[index].name.uri = self.ns.get_uri("xmlns");
        } else if aq.prefix.is_some() {
            self.attributes[index].name.uri = self.ns.get_uri(aq.prefix.as_deref().unwrap());
        }
        Ok(())
    }

    fn scan_end_element(&mut self) -> R<i32, H::Error> {
        let qn = self.element_stack.pop().unwrap_or_default();
        if !self.skip_string(&qn.rawname)? {
            return fatal("ETagRequired", &[qn.rawname.clone()]);
        }
        self.skip_spaces()?;
        if !self.skip_char('>' as i32)? {
            return fatal("ETagUnterminated", &[qn.rawname.clone()]);
        }
        self.markup_depth -= 2;
        if self.markup_depth < self.entity_stack[self.entity_depth - 1] {
            return fatal("ElementEntityMismatch", &[self.current_element.rawname.clone()]);
        }
        self.sax_end_element(&qn)?;
        self.ns.pop_context();
        Ok(self.markup_depth)
    }

    fn sax_start_element(&mut self, qn: &QName) -> R<(), H::Error> {
        let loc = self.locator();
        for (prefix, uri) in self.ns.declared() {
            let r = self.h.start_prefix_mapping(&prefix, uri.as_deref().unwrap_or(""), loc);
            self.handler(r)?;
        }
        let attrs: Vec<SaxAttr> = self
            .attributes
            .iter()
            .filter(|a| !(a.name.prefix.as_deref() == Some("xmlns") || a.name.rawname == "xmlns"))
            .map(|a| SaxAttr { uri: a.name.uri.clone().unwrap_or_default(), local: a.name.localpart.clone(), qname: a.name.rawname.clone(), value: a.value.clone() })
            .collect();
        let r = self.h.start_element(qn.uri.as_deref().unwrap_or(""), &qn.localpart, &qn.rawname, &attrs, loc);
        self.handler(r)
    }

    fn sax_end_element(&mut self, qn: &QName) -> R<(), H::Error> {
        let loc = self.locator();
        let r = self.h.end_element(qn.uri.as_deref().unwrap_or(""), &qn.localpart, &qn.rawname, loc);
        self.handler(r)?;
        for (prefix, _) in self.ns.declared() {
            let r = self.h.end_prefix_mapping(&prefix, loc);
            self.handler(r)?;
        }
        Ok(())
    }

    fn characters(&mut self, text: &[u16]) -> R<(), H::Error> {
        if text.is_empty() {
            return Ok(());
        }
        let text = to_string(text);
        let loc = self.locator();
        let r = self.h.characters(&text, loc);
        self.handler(r)
    }

    // ---- XMLDocumentFragmentScannerImpl ----

    fn scan_content_chars(&mut self) -> R<i32, H::Error> {
        let (content, mut c) = self.scan_content()?;
        let mut text = content.clone();
        if c == '\r' as i32 {
            self.scan_char()?;
            text.push(c as u16);
            c = -1;
        }
        self.characters(&text)?;
        if c == ']' as i32 && content.is_empty() {
            self.string_buffer.clear();
            let ch = self.scan_char()?;
            self.string_buffer.push(ch as u16);
            self.in_scan_content = true;
            if self.skip_char(']' as i32)? {
                self.string_buffer.push(']' as u16);
                while self.skip_char(']' as i32)? {
                    self.string_buffer.push(']' as u16);
                }
                if self.skip_char('>' as i32)? {
                    return fatal("CDEndInContent", &[]);
                }
            }
            let b = std::mem::take(&mut self.string_buffer);
            self.characters(&b)?;
            self.in_scan_content = false;
            c = -1;
        }
        Ok(c)
    }

    fn scan_cdata_section(&mut self) -> R<(), H::Error> {
        let loc = self.locator();
        let r = self.h.start_cdata(loc);
        self.handler(r)?;
        loop {
            let mut buf = Vec::new();
            if !self.scan_data("]]", &mut buf)? {
                self.characters(&buf)?;
                let mut brackets = 0;
                while self.skip_char(']' as i32)? {
                    brackets += 1;
                }
                if brackets > 0 {
                    if brackets > DEFAULT_BUFFER_SIZE {
                        let chunks = brackets / DEFAULT_BUFFER_SIZE;
                        let remainder = brackets % DEFAULT_BUFFER_SIZE;
                        let b = vec![']' as u16; DEFAULT_BUFFER_SIZE];
                        for _ in 0..chunks {
                            self.characters(&b)?;
                        }
                        if remainder != 0 {
                            self.characters(&b[..remainder])?;
                        }
                    } else {
                        self.characters(&vec![']' as u16; brackets])?;
                    }
                }
                if self.skip_char('>' as i32)? {
                    break;
                }
                self.characters(&[']' as u16, ']' as u16])?;
            } else {
                self.characters(&buf)?;
                let c = self.peek_char()?;
                if c != -1 && xml_char::is_invalid(c) {
                    if xml_char::is_high_surrogate(c) {
                        let mut b = Vec::new();
                        self.scan_surrogates(&mut b)?;
                        self.characters(&b)?;
                    } else {
                        return fatal("InvalidCharInCDSect", &[format!("{c:x}")]);
                    }
                }
            }
        }
        self.markup_depth -= 1;
        let loc = self.locator();
        let r = self.h.end_cdata(loc);
        self.handler(r)
    }

    fn scan_char_reference(&mut self) -> R<(), H::Error> {
        let mut buf = Vec::new();
        let ch = self.scan_char_reference_value(&mut buf, None)?;
        self.markup_depth -= 1;
        if ch != -1 {
            self.characters(&buf)?;
        }
        Ok(())
    }

    fn scan_entity_reference(&mut self) -> R<(), H::Error> {
        let name = self.scan_name()?;
        let Some(name) = name else { return fatal("NameRequiredInReference", &[]) };
        if !self.skip_char(';' as i32)? {
            return fatal("SemicolonRequiredInReference", &[name]);
        }
        self.markup_depth -= 1;
        let c = match name.as_str() {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => return fatal("EntityNotDeclared", &[name]),
        };
        let loc = self.locator();
        let r = self.h.start_entity(&name, loc);
        self.handler(r)?;
        self.characters(&[c as u16])?;
        let r = self.h.end_entity(&name, loc);
        self.handler(r)
    }

    fn dispatch(&mut self) -> R<bool, H::Error> {
        let r = match self.dispatcher {
            Dispatcher::XmlDecl => self.dispatch_xml_decl(),
            Dispatcher::Prolog => self.dispatch_prolog(),
            Dispatcher::Content => self.dispatch_content(),
            Dispatcher::TrailingMisc => self.dispatch_trailing_misc(),
        };
        match r {
            Err(Stop::Eof) => match self.dispatcher {
                Dispatcher::Content => fatal("PrematureEOF", &[]),
                Dispatcher::TrailingMisc => {
                    if self.markup_depth != 0 {
                        return fatal("PrematureEOF", &[]);
                    }
                    self.state = State::Terminated;
                    Ok(false)
                }
                _ => fatal("PrematureEOF", &[]),
            },
            r => r,
        }
    }

    fn dispatch_xml_decl(&mut self) -> R<bool, H::Error> {
        self.state = State::Prolog;
        self.dispatcher = Dispatcher::Prolog;
        if self.skip_string("<?xml")? {
            self.markup_depth += 1;
            if xml_char::is_name(self.peek_char()?) {
                let mut target = s("xml");
                while xml_char::is_ncname(self.peek_char()?) {
                    let c = self.scan_char()?;
                    target.push(char::from_u32(c as u32).unwrap_or('\u{FFFD}'));
                }
                self.scan_pi_data(&target)?;
            } else {
                let (_version, encoding, standalone) = self.scan_xml_decl_or_text_decl()?;
                self.markup_depth -= 1;
                self.standalone = standalone.as_deref() == Some("yes");
                if let Some(enc) = encoding {
                    self.set_encoding(&enc);
                }
            }
        }
        self.e().may_read_chunks = true;
        Ok(true)
    }

    fn dispatch_prolog(&mut self) -> R<bool, H::Error> {
        loop {
            match self.state {
                State::Prolog => {
                    self.skip_spaces()?;
                    if self.skip_char('<' as i32)? {
                        self.state = State::StartOfMarkup;
                    } else if self.skip_char('&' as i32)? {
                        self.state = State::Reference;
                    } else {
                        self.state = State::Content;
                    }
                }
                State::StartOfMarkup => {
                    self.markup_depth += 1;
                    if self.skip_char('!' as i32)? {
                        if self.skip_char('-' as i32)? {
                            if !self.skip_char('-' as i32)? {
                                return fatal("InvalidCommentStart", &[]);
                            }
                            self.state = State::Comment;
                        } else if self.skip_string("DOCTYPE")? {
                            self.state = State::Doctype;
                        } else {
                            return fatal("MarkupNotRecognizedInProlog", &[]);
                        }
                    } else if xml_char::is_name_start(self.peek_char()?) {
                        self.state = State::RootElement;
                        self.dispatcher = Dispatcher::Content;
                        return Ok(true);
                    } else if self.skip_char('?' as i32)? {
                        self.state = State::Pi;
                    } else {
                        return fatal("MarkupNotRecognizedInProlog", &[]);
                    }
                }
                State::Comment => {
                    self.scan_comment()?;
                    self.state = State::Prolog;
                }
                State::Pi => {
                    self.scan_pi()?;
                    self.state = State::Prolog;
                }
                State::Doctype => {
                    if self.seen_doctype_decl {
                        return fatal("AlreadySeenDoctype", &[]);
                    }
                    self.seen_doctype_decl = true;
                    self.scan_doctype_decl()?;
                    self.state = State::Prolog;
                }
                State::Content => return fatal("ContentIllegalInProlog", &[]),
                State::Reference => return fatal("ReferenceIllegalInProlog", &[]),
                _ => return Ok(true),
            }
        }
    }

    /// `scanDoctypeDecl` for a declaration without an internal subset; its external DTD is not read.
    fn scan_doctype_decl(&mut self) -> R<(), H::Error> {
        if !self.skip_spaces()? {
            return fatal("MSG_SPACE_REQUIRED_BEFORE_ROOT_ELEMENT_TYPE_IN_DOCTYPEDECL", &[]);
        }
        let name = self.scan_name()?;
        let Some(name) = name else { return fatal("MSG_ROOT_ELEMENT_TYPE_REQUIRED", &[]) };
        let mut external = false;
        if self.skip_spaces()? {
            for keyword in ["SYSTEM", "PUBLIC"] {
                if self.skip_string(keyword)? {
                    external = true;
                    let n = if keyword == "PUBLIC" { 2 } else { 1 };
                    for _ in 0..n {
                        self.skip_spaces()?;
                        let q = self.scan_char()?;
                        let mut b = Vec::new();
                        loop {
                            let c = self.scan_char()?;
                            if c == q {
                                break;
                            }
                            b.push(c as u16);
                        }
                    }
                    break;
                }
            }
            self.skip_spaces()?;
        }
        let loc = self.locator();
        let r = self.h.start_dtd(&name, loc);
        self.handler(r)?;
        if self.skip_char('[' as i32)? {
            return Err(Stop::Io(s("DTD internal subsets are not supported")));
        }
        self.skip_spaces()?;
        if !self.skip_char('>' as i32)? {
            return fatal("DoctypedeclUnterminated", &[name]);
        }
        self.markup_depth -= 1;
        if external {
            return Err(Stop::Io(s("java.io.FileNotFoundException: external DTDs are not read")));
        }
        let loc = self.locator();
        let r = self.h.end_dtd(loc);
        self.handler(r)
    }

    fn dispatch_content(&mut self) -> R<bool, H::Error> {
        loop {
            match self.state {
                State::Content => {
                    if self.skip_char('<' as i32)? {
                        self.state = State::StartOfMarkup;
                    } else if self.skip_char('&' as i32)? {
                        self.state = State::Reference;
                    } else {
                        loop {
                            let c = self.scan_content_chars()?;
                            if c == '<' as i32 {
                                self.scan_char()?;
                                self.state = State::StartOfMarkup;
                                break;
                            } else if c == '&' as i32 {
                                self.scan_char()?;
                                self.state = State::Reference;
                                break;
                            } else if c != -1 && xml_char::is_invalid(c) {
                                if xml_char::is_high_surrogate(c) {
                                    let mut b = Vec::new();
                                    if self.scan_surrogates(&mut b)? {
                                        self.characters(&b)?;
                                    }
                                } else {
                                    return fatal("InvalidCharInContent", &[format!("{c:x}")]);
                                }
                            }
                        }
                    }
                }
                State::StartOfMarkup => {
                    self.markup_depth += 1;
                    if self.skip_char('/' as i32)? {
                        if self.scan_end_element()? == 0 {
                            self.state = State::TrailingMisc;
                            self.dispatcher = Dispatcher::TrailingMisc;
                            return Ok(true);
                        }
                        self.state = State::Content;
                    } else if xml_char::is_name_start(self.peek_char()?) {
                        self.scan_start_element()?;
                        self.state = State::Content;
                    } else if self.skip_char('!' as i32)? {
                        if self.skip_char('-' as i32)? {
                            if !self.skip_char('-' as i32)? {
                                return fatal("InvalidCommentStart", &[]);
                            }
                            self.state = State::Comment;
                        } else if self.skip_string("[CDATA[")? {
                            self.state = State::Cdata;
                        } else if self.skip_string("DOCTYPE")? {
                            self.state = State::Doctype;
                        } else {
                            return fatal("MarkupNotRecognizedInContent", &[]);
                        }
                    } else if self.skip_char('?' as i32)? {
                        self.state = State::Pi;
                    } else {
                        return fatal("MarkupNotRecognizedInContent", &[]);
                    }
                }
                State::Comment => {
                    self.scan_comment()?;
                    self.state = State::Content;
                }
                State::Pi => {
                    self.scan_pi()?;
                    self.state = State::Content;
                }
                State::Cdata => {
                    self.scan_cdata_section()?;
                    self.state = State::Content;
                }
                State::Reference => {
                    self.markup_depth += 1;
                    self.state = State::Content;
                    if self.skip_char('#' as i32)? {
                        self.scan_char_reference()?;
                    } else {
                        self.scan_entity_reference()?;
                    }
                }
                State::RootElement => {
                    if self.scan_start_element()? {
                        self.state = State::TrailingMisc;
                        self.dispatcher = Dispatcher::TrailingMisc;
                        return Ok(true);
                    }
                    self.state = State::Content;
                }
                State::Doctype => return fatal("DoctypeIllegalInContent", &[]),
                _ => return Ok(true),
            }
        }
    }

    fn dispatch_trailing_misc(&mut self) -> R<bool, H::Error> {
        loop {
            match self.state {
                State::TrailingMisc => {
                    self.skip_spaces()?;
                    if self.skip_char('<' as i32)? {
                        self.state = State::StartOfMarkup;
                    } else {
                        self.state = State::Content;
                    }
                }
                State::StartOfMarkup => {
                    self.markup_depth += 1;
                    if self.skip_char('?' as i32)? {
                        self.state = State::Pi;
                    } else if self.skip_char('!' as i32)? {
                        self.state = State::Comment;
                    } else {
                        return fatal("MarkupNotRecognizedInMisc", &[]);
                    }
                }
                State::Pi => {
                    self.scan_pi()?;
                    self.state = State::TrailingMisc;
                }
                State::Comment => {
                    if !self.skip_string("--")? {
                        return fatal("InvalidCommentStart", &[]);
                    }
                    self.scan_comment()?;
                    self.state = State::TrailingMisc;
                }
                State::Content => {
                    let ch = self.peek_char()?;
                    if ch == -1 {
                        self.state = State::Terminated;
                        return Ok(false);
                    }
                    return fatal("ContentIllegalInTrailingMisc", &[]);
                }
                State::Reference => return fatal("ReferenceIllegalInTrailingMisc", &[]),
                _ => return Ok(false),
            }
        }
    }
}

fn normalize_whitespace(v: &mut [u16]) {
    for c in v.iter_mut() {
        if *c < 0x20 {
            *c = 0x20;
        }
    }
}

/// `XMLEntityManager.getEncodingName(b4, count)`: the auto-detected encoding and byte order.
fn get_encoding_name(b4: &[i32; 4]) -> (String, Option<bool>) {
    let (b0, b1, b2, b3) = (b4[0] & 0xFF, b4[1] & 0xFF, b4[2] & 0xFF, b4[3] & 0xFF);
    if b0 == 0xFE && b1 == 0xFF {
        return (s("UTF-16BE"), Some(true));
    }
    if b0 == 0xFF && b1 == 0xFE {
        return (s("UTF-16LE"), Some(false));
    }
    if b0 == 0xEF && b1 == 0xBB && b2 == 0xBF {
        return (s("UTF-8"), None);
    }
    if b0 == 0x00 && b1 == 0x00 && b2 == 0x00 && b3 == 0x3C {
        return (s("ISO-10646-UCS-4"), Some(true));
    }
    if b0 == 0x3C && b1 == 0x00 && b2 == 0x00 && b3 == 0x00 {
        return (s("ISO-10646-UCS-4"), Some(false));
    }
    if b0 == 0x00 && b1 == 0x3C && b2 == 0x00 && b3 == 0x3F {
        return (s("UTF-16BE"), Some(true));
    }
    if b0 == 0x3C && b1 == 0x00 && b2 == 0x3F && b3 == 0x00 {
        return (s("UTF-16LE"), Some(false));
    }
    if b0 == 0x4C && b1 == 0x6F && b2 == 0xA7 && b3 == 0x94 {
        return (s("CP037"), None);
    }
    (s("UTF-8"), None)
}

/// `XMLEntityManager.createReader(stream, encoding, isBigEndian)`.
fn create_reader(encoding: &str, big_endian: Option<bool>) -> Reader {
    let enc = encoding.to_ascii_uppercase();
    let _ = big_endian;
    match enc.as_str() {
        "UTF-8" | "UTF8" => Reader::utf8(),
        "ISO-8859-1" | "ISO8859_1" | "LATIN1" | "ISO-LATIN-1" | "L1" => Reader::latin1(),
        "US-ASCII" | "ASCII" => Reader::ascii(),
        "UTF-16BE" => Reader::java("UTF-16BE"),
        "UTF-16LE" => Reader::java("UTF-16LE"),
        "UTF-16" => Reader::java("UTF-16BE"),
        "WINDOWS-1252" | "CP1252" => Reader::java("CP1252"),
        _ => Reader::latin1(),
    }
}
