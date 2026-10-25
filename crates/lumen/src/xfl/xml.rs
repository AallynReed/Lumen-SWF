//! XFL XML writer, a small DOM for the tween detectors, and the XML pretty printer.

use std::fmt::Write as _;

/// `XFLXmlWriter`: XML built as a string, so finished fragments can be spliced in raw.
#[derive(Default, Clone)]
pub struct XmlWriter {
    sb: String,
    new_line: bool,
    new_line_needed: bool,
    start_open: bool,
    stack: Vec<String>,
}

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            '\r' => out.push_str("&#xD;"),
            '\n' => out.push_str("&#xA;"),
            c if is_invalid_xml(c) => {}
            c => out.push(c),
        }
    }
    out
}

/// `Helper.containsInvalidXMLCharacter`: characters XML 1.0 can not carry.
fn is_invalid_xml(c: char) -> bool {
    let u = c as u32;
    !(u == 0x9 || u == 0xA || u == 0xD || (0x20..=0xD7FF).contains(&u) || (0xE000..=0xFFFD).contains(&u) || (0x10000..=0x10FFFF).contains(&u))
}

impl XmlWriter {
    pub fn new() -> XmlWriter {
        XmlWriter { new_line: true, start_open: false, ..Default::default() }
    }

    fn append(&mut self, s: &str) {
        self.sb.push_str(s);
        self.new_line = false;
    }

    fn make_new_line(&mut self) {
        if !self.new_line {
            self.sb.push('\n');
            self.new_line = true;
            for _ in 0..self.stack.len() {
                self.sb.push_str("  ");
            }
        }
    }

    fn close_start(&mut self) {
        if self.start_open {
            self.append(">");
            self.start_open = false;
        }
    }

    pub fn start(&mut self, name: &str) {
        self.close_start();
        self.make_new_line();
        self.append("<");
        self.append(name);
        self.stack.push(name.to_string());
        self.start_open = true;
        self.new_line_needed = false;
    }

    pub fn start_with(&mut self, name: &str, attrs: &[(&str, &str)]) {
        self.start(name);
        for (k, v) in attrs {
            self.attr(k, v);
        }
    }

    pub fn empty(&mut self, name: &str) {
        self.start(name);
        self.end();
    }

    pub fn empty_with(&mut self, name: &str, attrs: &[(&str, &str)]) {
        self.start_with(name, attrs);
        self.end();
    }

    pub fn end(&mut self) {
        let name = self.stack.pop().unwrap_or_default();
        if !self.start_open {
            if self.new_line_needed {
                self.make_new_line();
            }
            self.append("</");
            self.append(&name);
            self.append(">");
        } else {
            self.append(" />");
            self.start_open = false;
        }
        self.new_line_needed = true;
    }

    pub fn value(&mut self, name: &str, value: &str) {
        self.start(name);
        self.text(value);
        self.end();
    }

    pub fn value_raw(&mut self, name: &str, value: &str) {
        self.start(name);
        self.raw(value);
        self.end();
    }

    pub fn attr(&mut self, name: &str, value: &str) {
        let v = escape(value);
        self.append(" ");
        self.append(name);
        self.append("=\"");
        self.append(&v);
        self.append("\"");
    }

    pub fn attr_bool(&mut self, name: &str, v: bool) {
        self.attr(name, if v { "true" } else { "false" });
    }

    pub fn attr_i(&mut self, name: &str, v: i64) {
        self.attr(name, &v.to_string());
    }

    pub fn attr_f(&mut self, name: &str, v: f32) {
        self.attr(name, &super::num::java_float(v));
    }

    pub fn attr_d(&mut self, name: &str, v: f64) {
        self.attr(name, &super::num::java_double(v));
    }

    pub fn cdata(&mut self, data: &str) {
        self.close_start();
        self.append("<![CDATA[");
        self.append(data);
        self.append("]]>");
    }

    pub fn raw(&mut self, text: &str) {
        self.close_start();
        self.append(text);
    }

    pub fn text(&mut self, text: &str) {
        self.close_start();
        let t = escape(text);
        self.append(&t);
    }

    /// `writeCharacters("")`: closes the start tag.
    pub fn close(&mut self) {
        self.close_start();
    }

    pub fn len(&self) -> usize {
        self.sb.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sb.is_empty()
    }

    pub fn as_str(&self) -> &str {
        &self.sb
    }

    pub fn clear(&mut self) {
        self.sb.clear();
    }
}

impl std::fmt::Display for XmlWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.sb)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Element(Element),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|a| a.0 == name).map(|a| a.1.as_str())
    }

    pub fn has(&self, name: &str) -> bool {
        self.attr(name).is_some()
    }

    pub fn remove_attr(&mut self, name: &str) {
        self.attrs.retain(|a| a.0 != name);
    }

    pub fn set_attr(&mut self, name: &str, value: &str) {
        match self.attrs.iter_mut().find(|a| a.0 == name) {
            Some(a) => a.1 = value.to_string(),
            None => self.attrs.push((name.to_string(), value.to_string())),
        }
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find_map(|c| match c {
            Node::Element(e) if e.name == name => Some(e),
            _ => None,
        })
    }

    pub fn child_mut(&mut self, name: &str) -> Option<&mut Element> {
        self.children.iter_mut().find_map(|c| match c {
            Node::Element(e) if e.name == name => Some(e),
            _ => None,
        })
    }

    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|c| match c {
            Node::Element(e) => Some(e),
            _ => None,
        })
    }

    pub fn remove_child(&mut self, name: &str) {
        if let Some(i) = self.children.iter().position(|c| matches!(c, Node::Element(e) if e.name == name)) {
            self.children.remove(i);
        }
    }

    /// `Node.isEqualNode`: attributes in any order, children in order.
    pub fn equal_node(&self, o: &Element) -> bool {
        if self.name != o.name || self.attrs.len() != o.attrs.len() || self.children.len() != o.children.len() {
            return false;
        }
        if !self.attrs.iter().all(|(k, v)| o.attr(k) == Some(v.as_str())) {
            return false;
        }
        self.children.iter().zip(&o.children).all(|(a, b)| match (a, b) {
            (Node::Element(x), Node::Element(y)) => x.equal_node(y),
            (Node::Text(x), Node::Text(y)) => x == y,
            _ => false,
        })
    }

    pub fn remove_whitespace(&mut self) {
        self.children.retain(|c| !matches!(c, Node::Text(t) if t.trim().is_empty()));
        for c in &mut self.children {
            if let Node::Element(e) = c {
                e.remove_whitespace();
            }
        }
    }
}

struct Parser<'a> {
    s: &'a [u8],
    p: usize,
}

/// XML line-end normalization of literal text.
fn normalize_eol(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

fn decode_entities(s: &str, attribute: bool) -> String {
    let raw = normalize_eol(s);
    let raw = if attribute { raw.replace(['\n', '\t'], " ") } else { raw };
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw.as_str();
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';') else { break };
        let ent = &rest[1..end];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            _ if ent.starts_with("#x") || ent.starts_with("#X") => u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32),
            _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

impl Parser<'_> {
    fn starts(&self, t: &str) -> bool {
        self.s[self.p..].starts_with(t.as_bytes())
    }

    fn skip_ws(&mut self) {
        while self.p < self.s.len() && self.s[self.p].is_ascii_whitespace() {
            self.p += 1;
        }
    }

    fn name(&mut self) -> String {
        let st = self.p;
        while self.p < self.s.len() && !self.s[self.p].is_ascii_whitespace() && !matches!(self.s[self.p], b'>' | b'/' | b'=') {
            self.p += 1;
        }
        String::from_utf8_lossy(&self.s[st..self.p]).into_owned()
    }

    fn element(&mut self) -> Option<Element> {
        if !self.starts("<") {
            return None;
        }
        self.p += 1;
        let name = self.name();
        let mut e = Element { name, ..Default::default() };
        loop {
            self.skip_ws();
            if self.starts("/>") {
                self.p += 2;
                return Some(e);
            }
            if self.starts(">") {
                self.p += 1;
                break;
            }
            if self.p >= self.s.len() {
                return Some(e);
            }
            let k = self.name();
            self.skip_ws();
            if self.starts("=") {
                self.p += 1;
                self.skip_ws();
            }
            let q = *self.s.get(self.p)?;
            self.p += 1;
            let st = self.p;
            while self.p < self.s.len() && self.s[self.p] != q {
                self.p += 1;
            }
            let v = String::from_utf8_lossy(&self.s[st..self.p]).into_owned();
            self.p += 1;
            e.attrs.push((k, decode_entities(&v, true)));
        }
        self.content(&mut e);
        Some(e)
    }

    fn push_text(e: &mut Element, t: String) {
        if t.is_empty() {
            return;
        }
        if let Some(Node::Text(last)) = e.children.last_mut() {
            last.push_str(&t);
        } else {
            e.children.push(Node::Text(t));
        }
    }

    fn content(&mut self, e: &mut Element) {
        while self.p < self.s.len() {
            if self.starts("</") {
                while self.p < self.s.len() && self.s[self.p] != b'>' {
                    self.p += 1;
                }
                self.p += 1;
                return;
            }
            if self.starts("<![CDATA[") {
                self.p += 9;
                let st = self.p;
                while self.p < self.s.len() && !self.starts("]]>") {
                    self.p += 1;
                }
                let t = normalize_eol(&String::from_utf8_lossy(&self.s[st..self.p]));
                self.p += 3;
                Self::push_text(e, t);
                continue;
            }
            if self.starts("<!--") {
                while self.p < self.s.len() && !self.starts("-->") {
                    self.p += 1;
                }
                self.p += 3;
                continue;
            }
            if self.starts("<?") {
                while self.p < self.s.len() && !self.starts("?>") {
                    self.p += 1;
                }
                self.p += 2;
                continue;
            }
            if self.starts("<") {
                if let Some(c) = self.element() {
                    e.children.push(Node::Element(c));
                }
                continue;
            }
            let st = self.p;
            while self.p < self.s.len() && self.s[self.p] != b'<' {
                self.p += 1;
            }
            let t = decode_entities(&String::from_utf8_lossy(&self.s[st..self.p]), false);
            Self::push_text(e, t);
        }
    }
}

/// Parses the XML the writer produced (one root element).
pub fn parse(xml: &str) -> Option<Element> {
    let mut p = Parser { s: xml.as_bytes(), p: 0 };
    loop {
        p.skip_ws();
        if p.starts("<?") {
            while p.p < p.s.len() && !p.starts("?>") {
                p.p += 1;
            }
            p.p += 2;
            continue;
        }
        break;
    }
    p.element()
}

/// Parses a sequence of sibling elements, as `"<frames>" + xml + "</frames>"` does.
pub fn parse_fragment(xml: &str) -> Element {
    let wrapped = format!("<frames>{xml}</frames>");
    parse(&wrapped).unwrap_or_default()
}

fn esc_text(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
}

fn esc_attr(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            c => out.push(c),
        }
    }
}

fn strip_space(e: &mut Element) {
    e.children.retain(|c| !matches!(c, Node::Text(t) if t.chars().all(|c| matches!(c, ' ' | '\t' | '\n' | '\r'))));
    for c in &mut e.children {
        if let Node::Element(x) = c {
            strip_space(x);
        }
    }
}

fn serialize(e: &Element, depth: usize, indent: usize, out: &mut String) {
    out.push('<');
    out.push_str(&e.name);
    let ns = e.attrs.iter().filter(|a| a.0 == "xmlns").chain(e.attrs.iter().filter(|a| a.0.starts_with("xmlns:"))).chain(e.attrs.iter().filter(|a| a.0 != "xmlns" && !a.0.starts_with("xmlns:")));
    for (k, v) in ns {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        esc_attr(v, out);
        out.push('"');
    }
    if e.children.is_empty() {
        out.push_str("/>");
        return;
    }
    out.push('>');
    let mixed = e.children.iter().any(|c| matches!(c, Node::Text(_)));
    let mut prev_text = false;
    for c in &e.children {
        match c {
            Node::Text(t) => {
                esc_text(t, out);
                prev_text = true;
            }
            Node::Element(x) => {
                if !prev_text && !mixed {
                    out.push('\n');
                    out.push_str(&" ".repeat(indent * (depth + 1)));
                }
                serialize(x, depth + 1, indent, out);
                prev_text = false;
            }
        }
    }
    if !mixed {
        out.push('\n');
        out.push_str(&" ".repeat(indent * depth));
    }
    let _ = write!(out, "</{}>", e.name);
}

/// `XmlPrettyFormat.prettyFormat(input, 5, false)`, with the Windows line separator the JDK
/// serializer writes there.
pub fn pretty(xml: &str) -> String {
    let Some(mut root) = parse(xml) else { return xml.to_string() };
    strip_space(&mut root);
    let mut out = String::new();
    serialize(&root, 0, 5, &mut out);
    out.push('\n');
    out.replace('\n', "\r\n")
}
