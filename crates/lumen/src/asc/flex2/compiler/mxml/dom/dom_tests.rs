//! The DOM dump checked against the scratchpad's `DomDump` harness, and `InterfaceCompiler.parseMXML`'s handling of
//! scanner and parser errors as the harness reproduces it.

use super::mxml_scanner::{self, MxmlScanner, ScannerFailure};
use super::node::{NodeClass, NodeRef};
use super::syntax_tree_builder::SyntaxTreeBuilder;
use super::tests::esc;
use crate::asc::flex2::compiler::mxml::mxml_namespaces::{MXML_1_NAMESPACE, MXML_2009_NAMESPACE, MXML_2_NAMESPACE};
use crate::asc::flex2::compiler::mxml::parser::{Parser, ParserError, LINE_SEPARATOR};
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage, Level};

/// `ThreadLocalToolkit.logError(path, line, col, msg)`.
fn log_error_col(path: &str, line: i32, col: i32, msg: &str) {
    toolkit::log_path_line_col(Level::Error, path, line, col, msg);
}

/// `InterfaceCompiler.parseMXML`, with extra lines for the harness.
pub(crate) fn parse_mxml(name: &str, bytes: Vec<u8>, layers: bool, out: &mut String) -> Option<NodeRef> {
    let mut s = match MxmlScanner::new(bytes, layers) {
        Ok(s) => s,
        Err(ScannerFailure::Scanner(err)) => return scanner_error(name, err, out),
        Err(ScannerFailure::Runtime(m)) => {
            toolkit::log_path(Level::Error, name, &m.unwrap_or_else(|| "null".into()), -1);
            return None;
        }
    };
    let result = {
        let mut p = Parser::new(&mut s);
        p.set_visitor(Box::new(SyntaxTreeBuilder));
        p.parse_application()
    };
    match result {
        Ok(t) => {
            let app = t.as_node();
            let ns = app.borrow().get_namespace().to_string();
            if ns == MXML_1_NAMESPACE || ns == MXML_2_NAMESPACE {
                let text = format!("unsupported MXML namespace found ('{ns}'). Please use the MXML namespace '{MXML_2009_NAMESPACE}' for MXML 2 source code.");
                toolkit::log(CompilerMessage::error(text), Some(name), -1, -1, None);
                return None;
            }
            Some(app)
        }
        Err(ParserError::Parse(ex)) => {
            let token = ex.next.clone();
            let mut msg = ex.get_message().to_string();
            *out += &format!("PARSEEXC \"{}\"\n", esc(&msg));
            if let Some(was) = msg.find(&format!("{LINE_SEPARATOR}Was expecting")).filter(|&w| w > 0) {
                msg.truncate(was);
                if regex::Regex::new("^Encountered \"(?s:.*)\" at line.*$").unwrap().is_match(&msg) {
                    msg = format!("Parse error: '{}' is not allowed to follow '{}'.", ex.next.image(), ex.current_token.image());
                }
            }
            log_error_col(name, token.begin_line(), token.begin_column(), &msg);
            None
        }
        Err(ParserError::Scanner(err)) => scanner_error(name, err, out),
        Err(ParserError::Runtime(m)) => {
            toolkit::log_path(Level::Error, name, &m, -1);
            None
        }
    }
}

fn scanner_error(name: &str, err: super::scanner_error::ScannerError, out: &mut String) -> Option<NodeRef> {
    let msg = err.get_reason().to_string();
    *out += &format!("SCANNERERR {}:{} \"{}\"\n", err.get_line_number(), err.get_column_number(), esc(&msg));
    let (line, col) = (err.get_line_number(), err.get_column_number());
    let logged = if msg == mxml_scanner::MARKUP_NOT_RECOGNIZED_IN_CONTENT {
        Some("Invalid character or markup found. Try surrounding your code with a CDATA block.")
    } else if msg == mxml_scanner::RESERVED_PI_TARGET {
        Some("Whitespace is not allowed before an XML Processing Instruction (<? ... ?>).")
    } else if msg == mxml_scanner::MARKUP_NOT_RECOGNIZED_IN_MISC {
        Some("The markup in the document following the root element must be well-formed.")
    } else {
        None
    };
    match logged {
        Some(text) => toolkit::log(CompilerMessage::error(text), Some(name), line, col, None),
        None => {
            let re = regex::Regex::new("^The element type \"(.*)\" must be terminated by the matching end-tag \"(.*)\".$").unwrap();
            let msg = match re.captures(&msg) {
                Some(c) => format!("The element type '{}' must be terminated by the matching end-tag '{}'.", &c[1], &c[2]),
                None => msg.clone(),
            };
            log_error_col(name, line, col, &msg);
        }
    }
    None
}

pub(crate) fn dump_node(n: &NodeRef, ind: &str, out: &mut String) {
    let b = n.borrow();
    let mut s = format!("{ind}{} k={} b={}:{} e={}:{} img=\"{}\"", b.class_name(), b.kind, b.begin_line, b.begin_column, b.end_line, b.end_column, esc(&b.image));
    if b.is_cdata_node() {
        s += &format!(" cdata={}", b.in_cdata());
        *out += &s;
        out.push('\n');
        return;
    }
    s += &format!(" ns={} lp={} pci={} pw={}", esc(b.get_namespace()), esc(b.get_local_part()), b.process_children_individually, b.preserve_whitespace);
    if let Some(c) = &b.comment {
        s += &format!(" comment=\"{}\"", esc(c));
    }
    if b.is_style_node() {
        s += &format!(" sheet={}", b.get_style_sheet().is_some());
    }
    if let NodeClass::Document(_) = &b.class {
        s += &format!(" lang={} ver={}", b.get_language_namespace().unwrap_or("null"), b.get_version());
    }
    if b.is_layered_node() {
        s += &match b.get_layer_parent() {
            None => " layer=null".to_string(),
            Some(p) => {
                let p = p.borrow();
                format!(" layer={}@{}:{}", p.image, p.begin_line, p.begin_column)
            }
        };
    }
    let pm = b.get_prefix_mappings();
    if !pm.is_empty() {
        s += " pm={";
        for (k, v) in pm {
            s += &format!("{k}={},", v.as_deref().unwrap_or("null"));
        }
        s += "}";
    }
    *out += &s;
    out.push('\n');
    for q in b.get_attribute_names() {
        let v = b.get_attribute_value_qname(&q).map(|v| v.to_java_string()).unwrap_or_else(|| "null".into());
        *out += &format!("{ind}  @{}|{}=\"{}\" {}\n", q.get_namespace(), q.get_local_part(), esc(&v), b.get_line_number_qname(&q));
    }
    let children = b.get_children();
    drop(b);
    for c in children {
        dump_node(&c, &format!("{ind}  "), out);
    }
}

/// `SyntaxAnalyzer` over a parsed document, as the harness runs it.
fn syntax(file: &str, app: &NodeRef, layers: bool, out: &mut String) {
    use crate::asc::flex2::compiler::mxml::mxml_configuration::MxmlConfiguration;
    use crate::asc::flex2::compiler::mxml::analyzer::syntax_analyzer::SyntaxAnalyzer;
    use crate::asc::flex2::compiler::source::{Owner, Source, VirtualFile};
    let path = std::path::Path::new(file);
    let short = path.file_stem().unwrap().to_string_lossy().into_owned();
    let src = Source::new(VirtualFile::local(path), None, "", &short, Owner::SourceList, false, true, true);
    let unit = Source::new_compilation_unit(&src, None);
    let mut cfg = MxmlConfiguration::new(&crate::asc::flex2::compiler::configuration::Configuration::default());
    cfg.enable_runtime_design_layers = layers;
    cfg.show_deprecation_warnings = true;
    cfg.source_path.clear();
    let mut a = SyntaxAnalyzer::new(unit, std::rc::Rc::new(cfg));
    super::node::analyze(app, &mut a);
    for d in app.borrow().layer_declaration_nodes() {
        let d = d.borrow();
        *out += &format!("LAYERDECL {}@{}
", d.image, d.begin_line);
    }
}

/// `DOM_LIST=<file list> DOM_OUT=<dump> [DOM_LAYERS=true]`: the same dump `DomDump` writes.
#[test]
fn dom_dump() {
    let (Ok(list), Ok(out)) = (std::env::var("DOM_LIST"), std::env::var("DOM_OUT")) else { return };
    let layers = std::env::var("DOM_LAYERS").is_ok_and(|v| v == "true");
    let mut all = String::new();
    for file in std::fs::read_to_string(list).unwrap().lines().filter(|l| !l.trim().is_empty()) {
        all += &format!("=== {file}\n");
        toolkit::reset(true);
        let mut lines = String::new();
        let app = parse_mxml(file, std::fs::read(file).unwrap(), layers, &mut lines);
        if let (Some(app), true) = (&app, std::env::var("DOM_SYNTAX").is_ok_and(|v| !v.is_empty())) {
            syntax(file, app, layers, &mut lines);
        }
        all += &lines;
        let d = toolkit::take_diagnostics();
        if !d.is_empty() {
            let text: String = d.iter().map(|d| format!("{}\n\n", d.text.replace("\r\n", "\n"))).collect();
            all += &format!("STDERR {}\n", esc(&text));
        }
        if let Some(app) = app {
            dump_node(&app, "", &mut all);
        }
    }
    std::fs::write(out, all).unwrap();
}
