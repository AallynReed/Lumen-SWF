//! Port of `flex2.compiler.mxml.analyzer.SyntaxAnalyzer`: checks language tags' attributes and pulls in `source="..."`
//! files at parse time. `FontManager` is not passed to `StyleSheet.parse`; external files without a BOM are read as
//! Java 17's default `file.encoding` (Cp1252).

use std::cell::RefCell;
use std::rc::Rc;

use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::mxml::dom::analyzer::Analyzer;
use crate::asc::flex2::compiler::mxml::lang::standard_defs::{GRAPHICS_DEFINITION_NAME, GRAPHICS_MASK, PROP_EXCLUDE_STATES, PROP_ID, PROP_INCLUDE_STATES};
use crate::asc::flex2::compiler::mxml::lang::text_parser::{analyze_scoped_name, is_binding_expression, is_scoped_name};
use crate::asc::flex2::compiler::mxml::mxml_configuration::MxmlConfiguration;
use crate::asc::flex2::compiler::mxml::dom::analyzer_adapter::AnalyzerAdapter;
use crate::asc::flex2::compiler::mxml::dom::mxml_scanner::{MxmlScanner, ScannerFailure};
use crate::asc::flex2::compiler::mxml::dom::node::{self, java_trim, Node, NodeRef};
use crate::asc::flex2::compiler::mxml::dom::syntax_tree_builder::SyntaxTreeBuilder;
use crate::asc::flex2::compiler::mxml::parser::{Parser, ParserError};
use crate::asc::flex2::compiler::source::VirtualFile;
use crate::asc::flex2::compiler::toolkit::CompilerMessage;
use crate::asc::flex2::compiler::util::QName;
use crate::asc::flex2::flash::css::style_sheet::StyleSheet;

pub struct SyntaxAnalyzer {
    adapter: AnalyzerAdapter,
    mxml_configuration: Rc<MxmlConfiguration>,
    /// The root node of the document for this compilation unit.
    document_node: Option<NodeRef>,
}

fn err(text: impl Into<String>) -> CompilerMessage {
    CompilerMessage::error(text)
}

fn warn(text: impl Into<String>) -> CompilerMessage {
    CompilerMessage::warning(text)
}

const BINDING_UNSUPPORTED: &str = "Data binding expressions not supported with attributes processed at compile time.";

impl SyntaxAnalyzer {
    pub fn new(unit: UnitRef, mxml_configuration: Rc<MxmlConfiguration>) -> SyntaxAnalyzer {
        SyntaxAnalyzer { adapter: AnalyzerAdapter::new(unit), mxml_configuration, document_node: None }
    }

    fn log(&self, node: &NodeRef, msg: CompilerMessage) {
        self.adapter.log(node, msg);
    }

    fn log_line(&self, node: &NodeRef, line: i32, msg: CompilerMessage) {
        self.adapter.log_node_line(node, line, msg);
    }

    fn attr(node: &NodeRef, name: &str) -> Option<String> {
        node.borrow().get_attribute_str(name).map(str::to_string)
    }

    fn line(node: &NodeRef, name: &str) -> i32 {
        node.borrow().get_line_number(name)
    }

    /// `unit.getSource().resolve(path)`: next to the source, at its path root for a leading slash, then the toolkit's.
    fn resolve(&self, path: &str) -> Option<VirtualFile> {
        let source = self.adapter.unit.borrow().get_source();
        match crate::asc::flex2::compiler::css::css_file::source_resolve(&source, path)? {
            crate::asc::flex2::compiler::css::css_file::CssFile::Local(name) => Some(VirtualFile::local(std::path::Path::new(&name))),
            _ => None,
        }
    }

    fn add_file_include(&self, f: &VirtualFile) {
        let source = self.adapter.unit.borrow().get_source();
        source.borrow_mut().add_file_include(f.get_name().to_string());
    }

    fn check_for_extra_attributes(&self, valid_attributes: &[(&str, &str)], node: &NodeRef) {
        let names = node.borrow().get_attribute_names();
        for qname in names {
            let namespace = qname.get_namespace().to_string();
            let local_part = qname.get_local_part().to_string();
            let is_scoped = is_scoped_name(&local_part);
            let mut q = qname.clone();
            if is_scoped {
                if let Some(stateful) = analyze_scoped_name(&local_part) {
                    q = QName::new(&namespace, &stateful[0]);
                }
            }
            let valid = valid_attributes.iter().any(|(ns, lp)| q.equals(ns, lp));
            let line = node.borrow().get_line_number_qname(&q);
            if !valid {
                if local_part == PROP_INCLUDE_STATES || local_part == PROP_EXCLUDE_STATES {
                    let name = node.borrow().get_local_part().to_string();
                    self.log_line(node, line, err(format!("The includeIn and excludeFrom attributes are not allowed on MXML language tag '{name}'.")));
                } else {
                    let tag = node.borrow().image.clone();
                    let unknown = err(format!("Unknown attribute: {q} on {tag}"));
                    if self.get_document_version() < 4 {
                        self.log_line(node, line, unknown);
                    } else {
                        let doc_ns = self.document_node.as_ref().and_then(|d| d.borrow().get_language_namespace().map(str::to_string));
                        if namespace.is_empty() || namespace == node.borrow().get_namespace() || Some(&namespace) == doc_ns.as_ref() {
                            self.log_line(node, line, unknown);
                        }
                    }
                }
            } else if is_scoped && !node.borrow().is_design_layer_node() {
                self.adapter.log_line(line, err(format!("Cannot specify state-specific value for MXML language attribute '{}'.", q.get_local_part())));
            }
        }
    }

    fn script(&self, node: &NodeRef) {
        let source = Self::attr(node, "source");
        let cdata = node.borrow().get_child_at(0);
        if source.is_some() && cdata.is_some() {
            self.log_line(node, Self::line(node, "source"), warn("Ignoring inline script because a source was specified."));
        }
        if let Some(source) = source {
            if is_binding_expression(&source) {
                self.log_line(node, Self::line(node, "source"), err(BINDING_UNSUPPORTED));
                return;
            }
            match self.resolve(&source) {
                None => self.log_line(node, Self::line(node, "source"), err(format!("Problem finding external file: {source}"))),
                Some(f) => {
                    self.add_file_include(&f);
                    if let Some(n) = self.parse_external_file(node, &f) {
                        node.borrow_mut().set_source_file(n);
                    }
                }
            }
        }
    }

    fn primitive(&self, node: &NodeRef) {
        let source = Self::attr(node, "source");
        let cdata = node.borrow().get_child_at(0);
        if source.is_some() && cdata.is_some() {
            self.log_line(node, Self::line(node, "source"), warn("Ignoring embedded string because a source file was defined."));
        }
        if let Some(source) = source {
            if is_binding_expression(&source) {
                self.log_line(node, Self::line(node, "source"), err(BINDING_UNSUPPORTED));
                return;
            }
            match self.resolve(&source) {
                None => self.log_line(node, Self::line(node, "source"), err(format!("Problem finding external file: {source}"))),
                Some(f) => {
                    self.add_file_include(&f);
                    if let Some(n) = self.parse_external_file(node, &f) {
                        node.borrow_mut().set_source_file(n);
                    }
                }
            }
        }
    }

    fn parse_external_xml(&self, node: &NodeRef, f: &VirtualFile) -> Option<NodeRef> {
        let name = f.get_name().to_string();
        let bytes = match f.to_byte_array() {
            Ok(b) => b,
            Err(e) => {
                self.log(node, err(format!("Problem parsing external XML: {name} - {e}")));
                return None;
            }
        };
        // `XMLParseProblem1.line` is shadowed by `CompilerMessage.line`, which `log(node, msg)` sets to the node's line.
        let scanner_problem = |se: crate::asc::flex2::compiler::mxml::dom::scanner_error::ScannerError| {
            let line = node.borrow().begin_line;
            self.log(node, err(format!("Problem parsing external XML: {name} - (line {line}) {}", se.get_reason())));
        };
        let mut s = match MxmlScanner::new(bytes, self.mxml_configuration.enable_runtime_design_layers()) {
            Ok(s) => s,
            Err(ScannerFailure::Scanner(se)) => {
                scanner_problem(se);
                return None;
            }
            Err(ScannerFailure::Runtime(_)) => return None,
        };
        let result = {
            let mut p = Parser::new(&mut s);
            p.set_visitor(Box::new(SyntaxTreeBuilder));
            p.parse_anonymous_object()
        };
        match result {
            Ok(t) => Some(t.as_node()),
            Err(ParserError::Scanner(se)) => {
                scanner_problem(se);
                None
            }
            Err(ParserError::Parse(ex)) => {
                self.log(node, err(format!("Problem parsing external XML: {name}")));
                self.adapter.log_error(node, ex.next.begin_line(), ex.get_message());
                None
            }
            Err(ParserError::Runtime(_)) => None,
        }
    }

    fn parse_external_file(&self, node: &NodeRef, f: &VirtualFile) -> Option<NodeRef> {
        let name = f.get_name().to_string();
        let bytes = match f.to_byte_array() {
            Ok(b) => b,
            Err(_) => {
                self.log(node, err(format!("File not found: {name}")));
                return None;
            }
        };
        let mut charset_name: Option<&str> = None;
        if name.to_lowercase().ends_with(".css") {
            match crate::asc::flex2::flash::css::style_parser::read_css_charset(&bytes) {
                Ok(c) => charset_name = Some(c),
                Err(c) => {
                    self.log(node, warn(format!("@charset rule contains an invalid or unsupported charset encoding, {c}. Problem found in stylesheet {name}")));
                    return None;
                }
            }
        }
        let (bom_charset, body) = consume_bom(&bytes);
        let charset = charset_name.unwrap_or(bom_charset);
        let text = decode(body, charset);
        let mut buffer = String::new();
        for line in read_lines(&text) {
            buffer.push_str(line);
            buffer.push_str(crate::asc::flex2::compiler::mxml::parser::LINE_SEPARATOR);
        }
        let mut cdata = Node::new_cdata();
        cdata.image = java_trim(&buffer);
        Some(cdata.into_ref())
    }

    fn parse_style(&self, node: &NodeRef, style_path: &str, cdata: &NodeRef) {
        let mut sheet = StyleSheet::new();
        sheet.check_deprecation(self.mxml_configuration.show_deprecation_warnings());
        sheet.parse_reader(style_path, cdata.borrow().image.encode_utf16().collect());
        if sheet.errors_exist() {
            self.log(node, err(format!("Error parsing stylesheet: {style_path}")));
        }
        node.borrow_mut().set_style_sheet(Some(Rc::new(RefCell::new(sheet))));
    }

    fn parse_style_inline(&self, node: &NodeRef, enclosing_document_path: &str, start_line: i32) {
        let cdata = node.borrow().get_child_at(0).expect("NullPointerException");
        let mut sheet = StyleSheet::new();
        sheet.check_deprecation(self.mxml_configuration.show_deprecation_warnings());
        sheet.parse_mxml(enclosing_document_path, start_line, cdata.borrow().image.encode_utf16().collect());
        if sheet.errors_exist() {
            self.log(node, err(format!("Error parsing stylesheet: {enclosing_document_path}")));
        }
        node.borrow_mut().set_style_sheet(Some(Rc::new(RefCell::new(sheet))));
    }
}

/// `FileUtils.consumeBOM(in, null, true)`: the charset and the bytes after the BOM.
fn consume_bom(bytes: &[u8]) -> (&'static str, &[u8]) {
    if bytes.len() >= 3 && bytes[..3] == [0xef, 0xbb, 0xbf] {
        ("UTF-8", &bytes[3..])
    } else if bytes.len() >= 2 && (bytes[..2] == [0xff, 0xfe] || bytes[..2] == [0xfe, 0xff]) {
        ("UTF-16", &bytes[2..])
    } else {
        ("Cp1252", bytes)
    }
}

fn decode(bytes: &[u8], charset: &str) -> String {
    match charset.to_ascii_uppercase().as_str() {
        "UTF-8" => String::from_utf8_lossy(bytes).into_owned(),
        "UTF-16" | "UTF-16BE" | "UTF-16LE" => {
            let le = charset.eq_ignore_ascii_case("UTF-16LE");
            let units: Vec<u16> = bytes.chunks(2).filter(|c| c.len() == 2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
            String::from_utf16_lossy(&units)
        }
        "ISO-8859-1" | "US-ASCII" => bytes.iter().map(|&b| b as char).collect(),
        _ => crate::asc::flex2::compiler::mxml::dom::xerces::reader::decode_cp1252(bytes),
    }
}

/// `BufferedReader.readLine` over the whole text.
fn read_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let b = text.as_bytes();
    let (mut start, mut i) = (0, 0);
    while i < b.len() {
        if b[i] == b'\n' || b[i] == b'\r' {
            lines.push(&text[start..i]);
            if b[i] == b'\r' && i + 1 < b.len() && b[i + 1] == b'\n' {
                i += 1;
            }
            start = i + 1;
        }
        i += 1;
    }
    if start < b.len() {
        lines.push(&text[start..]);
    }
    lines
}

impl Analyzer for SyntaxAnalyzer {
    fn adapter(&self) -> &AnalyzerAdapter {
        &self.adapter
    }

    fn adapter_mut(&mut self) -> &mut AnalyzerAdapter {
        &mut self.adapter
    }

    fn get_document_version(&self) -> i32 {
        self.document_node.as_ref().map_or(0, |d| d.borrow().get_version())
    }

    fn get_language_namespace(&self) -> Option<String> {
        self.document_node.as_ref().and_then(|d| d.borrow().get_language_namespace().map(str::to_string))
    }

    fn analyze_node(&mut self, node: &NodeRef) {
        if node.borrow().is_document_node() {
            self.document_node = Some(node.clone());
        }
        self.traverse(node);
    }

    fn analyze_layered_node(&mut self, node: &NodeRef) {
        self.analyze_node(node);
    }

    fn analyze_cdata_node(&mut self, _node: &NodeRef) {}

    fn analyze_state_node(&mut self, node: &NodeRef) {
        let name = Self::attr(node, "name");
        if self.get_document_version() >= 4 && name.as_deref().is_some_and(is_binding_expression) {
            self.log_line(node, Self::line(node, "name"), err(BINDING_UNSUPPORTED));
        }
        self.traverse(node);
    }

    fn analyze_style_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::STYLE_NODE_ATTRIBUTES, node);
        let source = Self::attr(node, "source");
        let mut cdata = node.borrow().get_child_at(0);
        if source.is_some() && cdata.is_some() {
            self.log_line(node, Self::line(node, "source"), warn("Ignoring embedded stylesheet because a source file was defined."));
        }
        if let Some(source) = source {
            if is_binding_expression(&source) {
                self.log_line(node, Self::line(node, "source"), err(BINDING_UNSUPPORTED));
                return;
            }
            let mut file = self.resolve(&source);
            if file.is_none() {
                for dir in &self.mxml_configuration.source_path {
                    if file.is_some() {
                        break;
                    }
                    let p = dir.join(&source);
                    if p.is_file() {
                        file = Some(VirtualFile::local(&p));
                    }
                }
            }
            match file {
                None => self.log_line(node, Self::line(node, "source"), err(format!("Problem finding external stylesheet: {source}"))),
                Some(file) => {
                    self.add_file_include(&file);
                    cdata = self.parse_external_file(node, &file);
                    if let Some(cdata) = cdata {
                        self.parse_style(node, file.get_name(), &cdata);
                    }
                }
            }
        } else if let Some(cdata) = cdata {
            let source = self.adapter.unit.borrow().get_source();
            let path = source.borrow().get_name().to_string();
            let line = cdata.borrow().begin_line;
            self.parse_style_inline(node, &path, line);
        }
    }

    fn analyze_script_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::SCRIPT_NODE_ATTRIBUTES, node);
        self.script(node);
    }

    fn analyze_meta_data_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::META_DATA_NODE_ATTRIBUTES, node);
    }

    fn analyze_model_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::MODEL_NODE_ATTRIBUTES, node);
        let source = Self::attr(node, "source");
        let count = node.borrow().get_child_count();
        if source.is_some() && count > 0 {
            self.log_line(node, Self::line(node, "source"), warn("Tag must be empty if source attribute is specified."));
        }
        if let Some(source) = source {
            if is_binding_expression(&source) {
                self.log_line(node, Self::line(node, "source"), err(BINDING_UNSUPPORTED));
                return;
            }
            match self.resolve(&source) {
                None => self.log_line(node, Self::line(node, "source"), err(format!("Problem finding external Model: {source}"))),
                Some(f) => {
                    self.add_file_include(&f);
                    if let Some(root) = self.parse_external_xml(node, &f) {
                        node.borrow_mut().set_source_file_nodes(vec![root]);
                    }
                }
            }
        }
    }

    fn analyze_xml_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::XML_NODE_ATTRIBUTES, node);
        let source = Self::attr(node, "source");
        let count = node.borrow().get_child_count();
        if source.is_some() && count > 0 {
            self.log_line(node, Self::line(node, "source"), warn("Ignoring inline XML because a source was specified."));
        }
        if let Some(source) = source {
            if is_binding_expression(&source) {
                self.log_line(node, Self::line(node, "source"), err(BINDING_UNSUPPORTED));
                return;
            }
            match self.resolve(&source) {
                None => self.log_line(node, Self::line(node, "source"), err(format!("Problem finding external XML: {source}"))),
                Some(f) => {
                    self.add_file_include(&f);
                    if let Some(root) = self.parse_external_xml(node, &f) {
                        node.borrow_mut().set_source_file_nodes(vec![root]);
                    }
                }
            }
        }
    }

    fn analyze_xml_list_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::XML_LIST_NODE_ATTRIBUTES, node);
    }

    fn analyze_array_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::ARRAY_NODE_ATTRIBUTES, node);
        self.traverse(node);
    }

    fn analyze_vector_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::VECTOR_NODE_ATTRIBUTES, node);
        self.traverse(node);
    }

    fn analyze_binding_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::BINDING_NODE_ATTRIBUTES, node);
        let source = Self::attr(node, "source");
        if source.as_deref().is_none_or(|s| java_trim(s).is_empty()) {
            self.log(node, err("Binding must have a valid source."));
        }
        let destination = Self::attr(node, "destination");
        if destination.as_deref().is_none_or(|s| java_trim(s).is_empty()) {
            self.log(node, err("Binding must have a valid destination."));
        }
        if source.is_some() && source == destination {
            self.log(node, err("Binding source and destination can not be the same."));
        }
        if Self::attr(node, "twoWay").as_deref().is_some_and(is_binding_expression) {
            self.log_line(node, Self::line(node, "twoWay"), err(BINDING_UNSUPPORTED));
        }
    }

    fn analyze_reparent_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::REPARENT_NODE_ATTRIBUTES, node);
        match Self::attr(node, "target") {
            Some(t) if !java_trim(&t).is_empty() => {
                if is_binding_expression(&t) {
                    self.log_line(node, Self::line(node, "target"), err(BINDING_UNSUPPORTED));
                }
            }
            _ => self.log(node, err("Reparent must have a target attribute defined.")),
        }
        let include_in = Self::attr(node, PROP_INCLUDE_STATES);
        let exclude_from = Self::attr(node, PROP_EXCLUDE_STATES);
        if include_in.as_deref().is_none_or(|s| java_trim(s).is_empty()) && exclude_from.as_deref().is_none_or(|s| java_trim(s).is_empty()) {
            self.log(node, err("Reparent must have either includeIn or excludeFrom defined."));
        }
    }

    fn analyze_library_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::LIBRARY_NODE_ATTRIBUTES, node);
        let doc = self.document_node.clone().expect("NullPointerException");
        let mut i = 0;
        while i < doc.borrow().get_child_count() {
            let next = doc.borrow().get_child_at(i).unwrap();
            i += 1;
            if next.borrow().get_local_part() == GRAPHICS_MASK {
                continue;
            }
            if !Rc::ptr_eq(&next, node) {
                let line = node.borrow().begin_line;
                self.log_line(node, line, err("Library tag must be the first child of a document's root tag."));
            }
            break;
        }
        self.traverse(node);
    }

    fn analyze_declarations_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::DECLARATIONS_NODE_ATTRIBUTES, node);
        self.traverse(node);
    }

    fn analyze_definition_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::DEFINITION_NODE_ATTRIBUTES, node);
        let definition_name = Self::attr(node, GRAPHICS_DEFINITION_NAME);
        if definition_name.as_deref().is_none_or(|s| java_trim(s).is_empty()) {
            self.log_line(node, Self::line(node, GRAPHICS_DEFINITION_NAME), err("A Definition must have a name attribute."));
        }
        if node.borrow().get_child_count() != 1 {
            let line = node.borrow().begin_line;
            self.log_line(node, line, err(format!("Definition '{}' must have exactly one child element.", definition_name.as_deref().unwrap_or("null"))));
        }
        self.traverse(node);
    }

    fn analyze_private_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::PRIVATE_NODE_ATTRIBUTES, node);
        let doc = self.document_node.clone().expect("NullPointerException");
        let mut i = doc.borrow().get_child_count() as i64 - 1;
        while i >= 0 {
            let last = doc.borrow().get_child_at(i as usize).unwrap();
            i -= 1;
            if last.borrow().get_local_part() == GRAPHICS_MASK {
                continue;
            }
            if !Rc::ptr_eq(&last, node) {
                let line = node.borrow().begin_line;
                self.log_line(node, line, err("Private tag must be the last child of a document's root tag."));
            }
            break;
        }
    }

    fn analyze_string_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::PRIMITIVE_NODE_ATTRIBUTES, node);
        self.primitive(node);
    }

    fn analyze_number_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::PRIMITIVE_NODE_ATTRIBUTES, node);
        self.primitive(node);
    }

    fn analyze_int_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::PRIMITIVE_NODE_ATTRIBUTES, node);
        self.primitive(node);
    }

    fn analyze_uint_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::PRIMITIVE_NODE_ATTRIBUTES, node);
        self.primitive(node);
    }

    fn analyze_boolean_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::PRIMITIVE_NODE_ATTRIBUTES, node);
        self.primitive(node);
    }

    fn analyze_request_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::REQUEST_NODE_ATTRIBUTES, node);
        self.traverse(node);
    }

    fn analyze_arguments_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::ARGUMENTS_NODE_ATTRIBUTES, node);
        self.traverse(node);
    }

    fn analyze_inline_component_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::INLINE_COMPONENT_NODE_ATTRIBUTES, node);
        if node.borrow().get_child_count() == 0 {
            self.log(node, err("Must define an inline component."));
        }
        self.traverse(node);
    }

    fn analyze_design_layer_node(&mut self, node: &NodeRef) {
        self.check_for_extra_attributes(node::DESIGN_LAYER_NODE_ATTRIBUTES, node);
        self.traverse(node);
    }

    /// Replaces each `DesignLayerNode` child with its children once analyzed.
    fn traverse(&mut self, node: &NodeRef) {
        let mut i = 0;
        while i < node.borrow().get_child_count() {
            let child = node.borrow().get_child_at(i).unwrap();
            node::analyze(&child, self);
            if child.borrow().is_design_layer_node() {
                let design_info_children = child.borrow().get_children();
                let n = design_info_children.len();
                node.borrow_mut().replace_node(i, design_info_children);
                i = (i as i64 + n as i64 - 1) as usize;
                if child.borrow().get_attribute_value(PROP_ID).is_some() && n == 0 {
                    if let Some(doc) = &self.document_node {
                        doc.borrow_mut().document_mut().layer_declaration_nodes.push(child.clone());
                    }
                }
            }
            i = i.wrapping_add(1);
        }
    }
}
