//! SAC event dumps in the format of the Java `SacDump` harness, for comparison against Batik.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::conditions::Condition;
use super::css_lexical_unit::{Kind as LKind, LexicalUnitRef};
use super::css_sac_media_list::CSSSACMediaList;
use super::css_selector_list::CSSSelectorList;
use super::parser::Parser;
use super::selectors::{Kind, Selector};
use crate::asc::flex2::batik::css::sac::lexical_unit::*;
use crate::asc::flex2::batik::css::sac::{CSSParseException, CssError, CssResult, DocumentHandler, ErrorHandler};
use crate::asc::util::jstr;

fn s(v: Option<&str>) -> String {
    let Some(v) = v else { return "null".into() };
    let mut b = String::from("\"");
    for c in jstr::units(v) {
        if c == b'\\' as u16 {
            b.push_str("\\\\");
        } else if c == b'"' as u16 {
            b.push_str("\\\"");
        } else if !(0x20..0x7f).contains(&c) {
            b.push_str(&format!("\\u{c:04x}"));
        } else {
            b.push(c as u8 as char);
        }
    }
    b.push('"');
    b
}

fn lu(u: Option<&LexicalUnitRef>) -> String {
    let mut b = String::from("[");
    let mut x = u.cloned();
    let mut first = true;
    while let Some(cur) = x {
        if !first {
            b.push_str(", ");
        }
        first = false;
        let t = cur.get_lexical_unit_type();
        b.push_str(&format!("{t}@{}", cur.get_line_number()));
        match (&cur.kind, t) {
            (_, SAC_OPERATOR_COMMA | SAC_OPERATOR_SLASH | SAC_INHERIT) => {}
            (LKind::Integer(v), _) => b.push_str(&format!(" i{v}")),
            (LKind::Dimension(v, d), _) => b.push_str(&format!(" f{:x} {}", v.to_bits(), s(Some(d)))),
            (LKind::Function(n, p), _) => b.push_str(&format!(" {} {}", s(Some(n)), lu(p.as_ref()))),
            (LKind::PredefinedFunction(p), _) => b.push_str(&format!(" {}", lu(p.as_ref()))),
            (LKind::String(v), _) => b.push_str(&format!(" {}", s(Some(v)))),
            (LKind::Float(v), _) => b.push_str(&format!(" f{:x}", v.to_bits())),
            _ => b.push_str(" ?"),
        }
        x = cur.get_next_lexical_unit();
    }
    b.push(']');
    b
}

fn cond(c: &Condition) -> String {
    let base = format!("{} {}", c.get_condition_type(), s(Some(&c.to_string())));
    if c.is_combinator_condition() {
        return format!("({base} {} {})", cond(c.get_first_condition().unwrap()), cond(c.get_second_condition().unwrap()));
    }
    if c.is_attribute_condition() {
        return format!("({base} {} {})", s(c.get_local_name()), s(c.get_value()));
    }
    format!("({base})")
}

fn sel(x: &Selector) -> String {
    let base = format!("{}@{} {}", x.get_selector_type(), x.get_line_number(), s(Some(&x.to_string())));
    match &x.kind {
        Kind::Element { namespace_uri, local_name } | Kind::PseudoElement { namespace_uri, local_name } => format!("({base} {} {})", s(namespace_uri.as_deref()), s(local_name.as_deref())),
        Kind::Conditional { simple_selector, condition } => format!("({base} {} {})", sel(simple_selector), cond(condition)),
        Kind::Descendant { ancestor_selector, simple_selector } | Kind::Child { ancestor_selector, simple_selector } => format!("({base} {} {})", sel(ancestor_selector), sel(simple_selector)),
        Kind::DirectAdjacent { node_type, selector, simple_selector } => format!("({base} {node_type} {} {})", sel(selector), sel(simple_selector)),
    }
}

fn sl(l: &CSSSelectorList) -> String {
    (0..l.get_length()).map(|i| format!(" {}", sel(l.item(i).unwrap()))).collect()
}

fn ml(m: &CSSSACMediaList) -> String {
    (0..m.get_length()).map(|i| format!(" {}", s(m.item(i).as_deref()))).collect()
}

#[derive(Clone)]
struct Out {
    buf: Rc<RefCell<String>>,
    line: Rc<Cell<i32>>,
}

impl Out {
    fn ev(&self, e: String) {
        let l = self.line.get();
        self.buf.borrow_mut().push_str(&format!("{l}: {e}\n"));
    }
}

impl DocumentHandler for Out {
    fn start_document(&mut self) -> CssResult<()> {
        self.ev("startDocument".into());
        Ok(())
    }
    fn end_document(&mut self) -> CssResult<()> {
        self.ev("endDocument".into());
        Ok(())
    }
    fn comment(&mut self, text: &str) -> CssResult<()> {
        self.ev(format!("comment {}", s(Some(text))));
        Ok(())
    }
    fn ignorable_at_rule(&mut self, at_rule: &str) -> CssResult<()> {
        self.ev(format!("ignorableAtRule {}", s(Some(at_rule))));
        Ok(())
    }
    fn namespace_declaration(&mut self, prefix: Option<&str>, uri: Option<&str>) -> CssResult<()> {
        self.ev(format!("namespace {} {}", s(prefix), s(uri)));
        Ok(())
    }
    fn import_style(&mut self, uri: Option<&str>, media: &CSSSACMediaList, ns: Option<&str>) -> CssResult<()> {
        self.ev(format!("importStyle {}{} {}", s(uri), ml(media), s(ns)));
        Ok(())
    }
    fn start_media(&mut self, media: &CSSSACMediaList) -> CssResult<()> {
        self.ev(format!("startMedia{}", ml(media)));
        Ok(())
    }
    fn end_media(&mut self, media: &CSSSACMediaList) -> CssResult<()> {
        self.ev(format!("endMedia{}", ml(media)));
        Ok(())
    }
    fn start_page(&mut self, name: Option<&str>, pp: Option<&str>) -> CssResult<()> {
        self.ev(format!("startPage {} {}", s(name), s(pp)));
        Ok(())
    }
    fn end_page(&mut self, name: Option<&str>, pp: Option<&str>) -> CssResult<()> {
        self.ev(format!("endPage {} {}", s(name), s(pp)));
        Ok(())
    }
    fn start_font_face(&mut self) -> CssResult<()> {
        self.ev("startFontFace".into());
        Ok(())
    }
    fn end_font_face(&mut self) -> CssResult<()> {
        self.ev("endFontFace".into());
        Ok(())
    }
    fn start_selector(&mut self, l: &CSSSelectorList) -> CssResult<()> {
        self.ev(format!("startSelector{}", sl(l)));
        Ok(())
    }
    fn end_selector(&mut self, l: &CSSSelectorList) -> CssResult<()> {
        self.ev(format!("endSelector{}", sl(l)));
        Ok(())
    }
    fn property(&mut self, name: &str, value: &LexicalUnitRef, important: bool) -> CssResult<()> {
        self.ev(format!("property {} {important} {} {}", s(Some(name)), lu(Some(value)), s(Some(&value.to_string()))));
        Ok(())
    }
}

impl ErrorHandler for Out {
    fn warning(&mut self, e: CSSParseException) -> CssResult<()> {
        self.ev(format!("warning {} {} {}", s(Some(&e.message)), e.line_number, e.column_number));
        Ok(())
    }
    fn error(&mut self, e: CSSParseException) -> CssResult<()> {
        self.ev(format!("error {} {} {}", s(Some(&e.message)), e.line_number, e.column_number));
        Ok(())
    }
    fn fatal_error(&mut self, e: CSSParseException) -> CssResult<()> {
        self.ev(format!("fatal {} {} {}", s(Some(&e.message)), e.line_number, e.column_number));
        Ok(())
    }
}

pub fn dump(text: &[u8]) -> String {
    let mut parser = Parser::new();
    let out = Out { buf: Rc::new(RefCell::new(String::new())), line: parser.line_number_cell() };
    parser.set_document_handler(Box::new(out.clone()));
    parser.set_error_handler(Box::new(out.clone()));
    if let Err(e) = parser.parse_style_sheet(jstr::decode_java_utf8(text)) {
        let kind = match e {
            CssError::Parse(_) => "parse",
            CssError::Css(_) => "css",
            CssError::Runtime(_) => "runtime",
        };
        out.buf.borrow_mut().push_str(&format!("throw {kind} {}\n", s(e.get_localized_message().as_deref())));
    }
    let r = out.buf.borrow().clone();
    r
}

/// `LUMEN_SAC_CORPUS=<dir> LUMEN_SAC_OUT=<dir>` dumps every file of the corpus.
#[test]
fn sac_corpus_dump() {
    let (Ok(dir), Ok(out)) = (std::env::var("LUMEN_SAC_CORPUS"), std::env::var("LUMEN_SAC_OUT")) else { return };
    std::fs::create_dir_all(&out).unwrap();
    let mut names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    names.sort();
    for p in names {
        let text = std::fs::read(&p).unwrap();
        let d = std::panic::catch_unwind(|| dump(&text)).unwrap_or_else(|_| "PANIC\n".into());
        std::fs::write(std::path::Path::new(&out).join(format!("{}.txt", p.file_name().unwrap().to_string_lossy())), d).unwrap();
    }
}

#[test]
fn parses_a_namespaced_rule() {
    let d = dump(b"@namespace s \"lib://s\";\ns|Button.big { color: #f00; }\n");
    assert_eq!(d, "1: startDocument\n2: startSelector (0@0 \"Button.big\" (4@2 \"Button\" \"lib://s\" \"Button\") (9 \".big\" \"class\" \"big\"))\n2: property \"color\" false [27@2 [13@2 i255, 0@2, 13@2 i0, 0@2, 13@2 i0]] \"rgb(255 ,0 ,0)\"\n3: endSelector (0@0 \"Button.big\" (4@2 \"Button\" \"lib://s\" \"Button\") (9 \".big\" \"class\" \"big\"))\n3: endDocument\n");
}
