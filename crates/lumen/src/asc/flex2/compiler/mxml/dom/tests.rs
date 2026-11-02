//! Dumps checked against the Java harnesses in the scratchpad (`SaxDump`, `DomDump`); the file lists come from the
//! environment, so these do nothing in a normal test run.

use super::xerces::{self, Locator, SaxAttr, SaxHandler};

pub(crate) fn esc(s: &str) -> String {
    let mut b = String::new();
    for c in s.encode_utf16() {
        match c {
            10 => b.push_str("\\n"),
            13 => b.push_str("\\r"),
            9 => b.push_str("\\t"),
            92 => b.push_str("\\\\"),
            c if !(0x20..=0x7e).contains(&c) => b.push_str(&format!("\\u{:04x}", c)),
            c => b.push(c as u8 as char),
        }
    }
    b
}

struct Dump(String);

fn at(l: Locator) -> String {
    if l.available { format!(" @{}:{}", l.line, l.column) } else { " @-".to_string() }
}

impl SaxHandler for Dump {
    type Error = ();
    fn start_document(&mut self, l: Locator) -> Result<(), ()> {
        self.0 += &format!("SD{}\n", at(l));
        Ok(())
    }
    fn end_document(&mut self, l: Locator) -> Result<(), ()> {
        self.0 += &format!("ED{}\n", at(l));
        Ok(())
    }
    fn start_prefix_mapping(&mut self, p: &str, u: &str, l: Locator) -> Result<(), ()> {
        self.0 += &format!("SP {p}|{u}{}\n", at(l));
        Ok(())
    }
    fn end_prefix_mapping(&mut self, p: &str, l: Locator) -> Result<(), ()> {
        self.0 += &format!("EP {p}{}\n", at(l));
        Ok(())
    }
    fn start_element(&mut self, u: &str, lp: &str, q: &str, attrs: &[SaxAttr], l: Locator) -> Result<(), ()> {
        self.0 += &format!("SE {u}|{lp}|{q}{}\n", at(l));
        for a in attrs {
            self.0 += &format!("  A {}|{}|{}|{}|?\n", a.uri, a.local, a.qname, esc(&a.value));
        }
        Ok(())
    }
    fn end_element(&mut self, u: &str, lp: &str, q: &str, l: Locator) -> Result<(), ()> {
        self.0 += &format!("EE {u}|{lp}|{q}{}\n", at(l));
        Ok(())
    }
    fn characters(&mut self, t: &str, l: Locator) -> Result<(), ()> {
        self.0 += &format!("CH \"{}\"{}\n", esc(t), at(l));
        Ok(())
    }
    fn processing_instruction(&mut self, t: &str, d: &str, l: Locator) -> Result<(), ()> {
        self.0 += &format!("PI {t}|{}{}\n", esc(d), at(l));
        Ok(())
    }
    fn start_cdata(&mut self, l: Locator) -> Result<(), ()> {
        self.0 += &format!("SC{}\n", at(l));
        Ok(())
    }
    fn end_cdata(&mut self, l: Locator) -> Result<(), ()> {
        self.0 += &format!("EC{}\n", at(l));
        Ok(())
    }
    fn comment(&mut self, t: &str, l: Locator) -> Result<(), ()> {
        self.0 += &format!("CM \"{}\"{}\n", esc(t), at(l));
        Ok(())
    }
    fn start_entity(&mut self, n: &str, l: Locator) -> Result<(), ()> {
        self.0 += &format!("SEN {n}{}\n", at(l));
        Ok(())
    }
    fn end_entity(&mut self, n: &str, l: Locator) -> Result<(), ()> {
        self.0 += &format!("EEN {n}{}\n", at(l));
        Ok(())
    }
    fn start_dtd(&mut self, n: &str, l: Locator) -> Result<(), ()> {
        self.0 += &format!("SDTD {n}{}\n", at(l));
        Ok(())
    }
    fn end_dtd(&mut self, l: Locator) -> Result<(), ()> {
        self.0 += &format!("EDTD{}\n", at(l));
        Ok(())
    }
    fn fatal_error(&mut self, m: &str, l: Locator) -> Result<(), ()> {
        self.0 += &format!("FE {} #{}:{}{}\n", esc(m), l.line, l.column, at(l));
        Ok(())
    }
}

/// `SAX_LIST=<file list> SAX_OUT=<dump>`: the same dump `SaxDump` writes.
#[test]
fn sax_dump() {
    let (Ok(list), Ok(out)) = (std::env::var("SAX_LIST"), std::env::var("SAX_OUT")) else { return };
    let mut all = String::from("# factory org.apache.xerces.jaxp.SAXParserFactoryImpl\n");
    for file in std::fs::read_to_string(list).unwrap().lines().filter(|l| !l.trim().is_empty()) {
        all += &format!("=== {file}\n");
        let mut d = Dump(String::new());
        let r = xerces::parse(std::fs::read(file).unwrap(), &mut d);
        all += &d.0;
        match r {
            Err(xerces::ParseFailure::Fatal(m)) => all += &format!("EXC {}\n", esc(&m)),
            Err(xerces::ParseFailure::Io(m)) => all += &format!("THROW java.io.IOException {}\n", esc(&m)),
            _ => {}
        }
    }
    std::fs::write(out, all).unwrap();
}
