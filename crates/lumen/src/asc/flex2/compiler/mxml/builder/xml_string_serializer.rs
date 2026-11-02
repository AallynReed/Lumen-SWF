//! Port of `flex2.compiler.util.XMLStringSerializer` (with its `UTF8Encoder`, `NSStack` and `Mapping`), which
//! `XMLBuilder` writes the legacy `XMLNode` format with. `NSStack` is the `optimizePrefixes = false` form it uses.

use crate::asc::util::jhash::JHashMap;
use crate::asc::util::jstr;

const NS_URI_XML: &str = "http://www.w3.org/XML/1998/namespace";
const NS_URI_XMLNS: &str = "http://www.w3.org/2000/xmlns/";

#[derive(Clone)]
struct Mapping {
    namespace_uri: String,
    prefix: String,
}

struct NsStack {
    stack: Vec<Option<Mapping>>,
    top: usize,
    iterator: usize,
    current_default_ns: i32,
}

impl NsStack {
    fn new() -> NsStack {
        NsStack { stack: vec![None; 32], top: 0, iterator: 0, current_default_ns: -1 }
    }

    fn push(&mut self) {
        self.top += 1;
        if self.top >= self.stack.len() {
            let n = self.stack.len() * 2;
            self.stack.resize(n, None);
        }
        self.stack[self.top] = None;
    }

    fn pop(&mut self) {
        while self.stack[self.top].is_some() {
            self.top -= 1;
        }
        self.top -= 1;
        if (self.top as i32) < self.current_default_ns {
            self.current_default_ns = self.top as i32;
            while self.current_default_ns > 0 {
                if self.stack[self.current_default_ns as usize].as_ref().is_some_and(|m| m.prefix.is_empty()) {
                    break;
                }
                self.current_default_ns -= 1;
            }
        }
    }

    fn top_of_frame(&mut self) -> Option<Mapping> {
        self.iterator = self.top;
        while self.stack[self.iterator].is_some() {
            self.iterator -= 1;
        }
        self.iterator += 1;
        self.next()
    }

    fn next(&mut self) -> Option<Mapping> {
        if self.iterator > self.top {
            None
        } else {
            let m = self.stack[self.iterator].clone();
            self.iterator += 1;
            m
        }
    }

    fn add(&mut self, namespace_uri: &str, prefix: &str) {
        let mut idx = self.top;
        let mut cursor = self.top;
        let mut found = false;
        while let Some(m) = self.stack[cursor].as_mut() {
            if m.prefix == prefix {
                m.namespace_uri = namespace_uri.to_string();
                idx = cursor;
                found = true;
                break;
            }
            cursor -= 1;
        }
        if !found {
            self.push();
            self.stack[self.top] = Some(Mapping { namespace_uri: namespace_uri.to_string(), prefix: prefix.to_string() });
            idx = self.top;
        }
        if prefix.is_empty() {
            self.current_default_ns = idx as i32;
        }
    }

    fn get_prefix(&self, namespace_uri: &str, no_default: bool) -> Option<String> {
        if namespace_uri.is_empty() {
            return None;
        }
        let mut cursor = self.top;
        while cursor > 0 {
            if let Some(map) = &self.stack[cursor] {
                if map.namespace_uri == namespace_uri {
                    let possible_prefix = map.prefix.clone();
                    if !(no_default && possible_prefix.is_empty()) {
                        let mut cursor2 = self.top;
                        loop {
                            if cursor2 == cursor {
                                return Some(possible_prefix);
                            }
                            if let Some(m2) = &self.stack[cursor2] {
                                if m2.prefix == possible_prefix {
                                    break;
                                }
                            }
                            cursor2 -= 1;
                        }
                    }
                }
            }
            cursor -= 1;
        }
        None
    }

    fn get_namespace_uri(&self, prefix: &str) -> Option<String> {
        let mut cursor = self.top;
        while cursor > 0 {
            if let Some(map) = &self.stack[cursor] {
                if map.prefix == prefix {
                    return Some(map.namespace_uri.clone());
                }
            }
            cursor -= 1;
        }
        None
    }
}

/// One attribute as `XMLBuilder.AttributesHelper` reports it.
pub struct Attr {
    pub uri: String,
    pub local_name: String,
    pub value: String,
}

impl Attr {
    /// `AttributesHelper.getQName(index)`.
    fn get_qname(&self) -> String {
        if self.uri.is_empty() {
            self.local_name.clone()
        } else {
            format!("{}:{}", self.uri, self.local_name)
        }
    }
}

pub struct XmlStringSerializer<'a> {
    writer: &'a mut String,
    writing_start_tag: bool,
    no_namespace_mappings: bool,
    element_stack: Vec<String>,
    ns_stack: NsStack,
    last_prefix_index: i32,
    preferred_prefixes: JHashMap<String, String>,
}

impl<'a> XmlStringSerializer<'a> {
    pub fn new(writer: &'a mut String) -> XmlStringSerializer<'a> {
        let mut preferred_prefixes = JHashMap::new();
        for (uri, prefix) in [
            ("http://schemas.xmlsoap.org/soap/encoding/", "soapenc"),
            (NS_URI_XML, "xml"),
            ("http://www.w3.org/1999/XMLSchema", "xsd"),
            ("http://www.w3.org/1999/XMLSchema-instance", "xsi"),
            ("http://www.w3.org/2000/10/XMLSchema", "xsd"),
            ("http://www.w3.org/2000/10/XMLSchema-instance", "xsi"),
            ("http://www.w3.org/2001/XMLSchema", "xsd"),
            ("http://www.w3.org/2001/XMLSchema-instance", "xsi"),
            ("http://schemas.xmlsoap.org/soap/envelope/", "soapenv"),
        ] {
            preferred_prefixes.insert(uri.to_string(), prefix.to_string());
        }
        XmlStringSerializer { writer, writing_start_tag: false, no_namespace_mappings: true, element_stack: Vec::new(), ns_stack: NsStack::new(), last_prefix_index: 1, preferred_prefixes }
    }

    fn get_prefix_for_uri(&mut self, uri: &str, default_prefix: Option<&str>, attribute: bool) -> Option<String> {
        if uri.is_empty() {
            return None;
        }
        let mut prefix = self.ns_stack.get_prefix(uri, attribute);
        if prefix.is_none() {
            prefix = self.preferred_prefixes.get(uri).cloned();
            if prefix.is_none() {
                match default_prefix {
                    None => {
                        let mut p = format!("ns{}", self.last_prefix_index);
                        self.last_prefix_index += 1;
                        while self.ns_stack.get_namespace_uri(&p).is_some() {
                            p = format!("ns{}", self.last_prefix_index);
                            self.last_prefix_index += 1;
                        }
                        prefix = Some(p);
                    }
                    Some(d) => prefix = Some(d.to_string()),
                }
            }
            self.register_prefix_for_uri(prefix.as_deref(), Some(uri));
        }
        prefix
    }

    fn register_prefix_for_uri(&mut self, prefix: Option<&str>, uri: Option<&str>) {
        if let (Some(prefix), Some(uri)) = (prefix, uri) {
            if self.no_namespace_mappings {
                self.ns_stack.push();
                self.no_namespace_mappings = false;
            }
            let active_prefix = self.ns_stack.get_prefix(uri, true);
            if active_prefix.as_deref() != Some(prefix) {
                self.ns_stack.add(uri, prefix);
            }
        }
    }

    /// `startElement(qName, attributes)`; `preferred_prefix` is the QName's.
    pub fn start_element(&mut self, namespace: &str, local_part: &str, preferred_prefix: Option<&str>, attributes: &[Attr]) {
        let mut vec_qnames: Option<Vec<String>> = None;
        if self.writing_start_tag {
            self.writer.push('>');
        }
        let element_qname = self.qname_to_string(namespace, local_part, preferred_prefix, true);
        self.writer.push('<');
        self.writer.push_str(&element_qname);
        for a in attributes {
            let mut qname = a.get_qname();
            self.writer.push(' ');
            let mut prefix = String::new();
            if !a.uri.is_empty() {
                if qname.is_empty() {
                    prefix = self.get_prefix_for_uri(&a.uri, None, false).unwrap_or_default();
                } else if let Some(idx) = qname.find(':') {
                    let p = qname[..idx].to_string();
                    prefix = self.get_prefix_for_uri(&a.uri, Some(&p), true).unwrap_or_default();
                }
                qname = if !prefix.is_empty() { format!("{prefix}:{}", a.local_name) } else { a.local_name.clone() };
            } else if qname.is_empty() {
                qname = a.local_name.clone();
            }
            if qname.starts_with("xmlns") {
                vec_qnames.get_or_insert_with(Vec::new).push(qname.clone());
            }
            self.writer.push_str(&qname);
            self.writer.push_str("=\"");
            write_encoded(self.writer, &a.value);
            self.writer.push('"');
        }
        if self.no_namespace_mappings {
            self.ns_stack.push();
        } else {
            let mut map = self.ns_stack.top_of_frame();
            while let Some(m) = map {
                if !(m.namespace_uri == NS_URI_XMLNS && m.prefix == "xmlns") && !(m.namespace_uri == NS_URI_XML && m.prefix == "xml") {
                    let mut sb = String::from("xmlns");
                    if !m.prefix.is_empty() {
                        sb.push(':');
                        sb.push_str(&m.prefix);
                    }
                    if vec_qnames.as_ref().is_none_or(|v| !v.contains(&sb)) {
                        self.writer.push(' ');
                        sb.push_str("=\"");
                        sb.push_str(&m.namespace_uri);
                        sb.push('"');
                        self.writer.push_str(&sb);
                    }
                }
                map = self.ns_stack.next();
            }
            self.no_namespace_mappings = true;
        }
        self.writing_start_tag = true;
        self.element_stack.push(element_qname);
    }

    pub fn write_string(&mut self, string: &str) {
        if self.writing_start_tag {
            self.writer.push('>');
            self.writing_start_tag = false;
        }
        self.writer.push_str(string);
    }

    pub fn end_element(&mut self) {
        let element_qname = self.element_stack.pop().expect("EmptyStackException");
        self.ns_stack.pop();
        if self.writing_start_tag {
            self.writer.push_str("/>");
            self.writing_start_tag = false;
            return;
        }
        self.writer.push_str("</");
        self.writer.push_str(&element_qname);
        self.writer.push('>');
    }

    fn qname_to_string(&mut self, namespace_uri: &str, local_part: &str, preferred_prefix: Option<&str>, write_ns: bool) -> String {
        let mut prefix: Option<String> = None;
        let mut local_part = local_part.to_string();
        if !local_part.is_empty() {
            if let Some(index) = local_part.find(':') {
                let p = local_part[..index].to_string();
                if !p.is_empty() && p != "urn" {
                    self.register_prefix_for_uri(Some(&p), Some(namespace_uri));
                    local_part = local_part[index + 1..].to_string();
                    prefix = Some(p);
                }
            }
            local_part = get_last_local_part(&local_part);
        }
        if namespace_uri.is_empty() {
            if write_ns {
                if let Some(default_ns) = self.ns_stack.get_namespace_uri("") {
                    if !default_ns.is_empty() {
                        self.register_prefix_for_uri(Some(""), Some(""));
                    }
                }
            }
        } else {
            prefix = self.get_prefix_for_uri(namespace_uri, preferred_prefix, false);
        }
        match prefix {
            Some(p) if !p.is_empty() => format!("{p}:{local_part}"),
            _ => local_part,
        }
    }
}

fn get_last_local_part(local_part: &str) -> String {
    let i = jstr::last_index_of_unit(local_part, b'>' as u16);
    if i > -1 && i < jstr::utf16_len(local_part) - 1 {
        jstr::substring_from(local_part, i + 1)
    } else {
        local_part.to_string()
    }
}

/// `UTF8Encoder.writeEncoded(writer, xmlString)`; Java throws on control characters, which are kept here.
fn write_encoded(w: &mut String, s: &str) {
    for c in jstr::to_utf16(s) {
        match c {
            0x26 => w.push_str("&amp;"),
            0x22 => w.push_str("&quot;"),
            0x3c => w.push_str("&lt;"),
            0x3e => w.push_str("&gt;"),
            0x0a => w.push('\n'),
            0x0d => w.push('\r'),
            0x09 => w.push('\t'),
            c if c > 0x7f => w.push_str(&format!("&#x{:X};", c)),
            c => jstr::push_unit(w, c),
        }
    }
}
