//! Port of `flex2.compiler.mxml.builder.XMLBuilder`; [`Out`] is its `serializer`: a `StringWriter` for E4X or an
//! `XMLStringSerializer` for the legacy `XMLNode` format.

use super::abstract_builder::{analyzer_basics, attr_str, err, AbstractBuilder, Builder, BuilderContext};
use super::prefix_mapping::{self, PrefixMapping};
use super::xml_string_serializer::{Attr, XmlStringSerializer};
use crate::asc::flex2::compiler::mxml::dom::analyzer::Analyzer;
use crate::asc::flex2::compiler::mxml::dom::node::NodeRef;
use crate::asc::flex2::compiler::mxml::lang::standard_defs as sd;
use crate::asc::flex2::compiler::mxml::lang::text_parser::{self, TextParser};
use crate::asc::flex2::compiler::mxml::rep::binding_expression::BindingExpressionRef;
use crate::asc::flex2::compiler::mxml::rep::model::{Model, ModelRef};
use crate::asc::flex2::compiler::util::{name_formatter, QName};
use crate::asc::util::jhash::JHashMap;

/// The `serializer` argument.
pub enum Out<'a> {
    E4x(&'a mut String),
    Xml(XmlStringSerializer<'a>),
}

impl Out<'_> {
    fn raw(&mut self, text: &str) {
        match self {
            Out::E4x(w) => w.push_str(text),
            Out::Xml(x) => x.write_string(text),
        }
    }

    fn text(&mut self, text: &str) {
        self.raw(text)
    }

    fn start(&mut self, node: &NodeRef) {
        match self {
            Out::E4x(w) => node.borrow().to_start_element(w),
            Out::Xml(x) => {
                let n = node.borrow();
                let attrs: Vec<Attr> = n.get_attribute_names().iter().map(|q| Attr { uri: q.get_namespace().to_string(), local_name: q.get_local_part().to_string(), value: n.get_attribute_value_qname(q).map(|v| v.to_java_string()).unwrap_or_default() }).collect();
                x.start_element(n.get_namespace(), n.get_local_part(), n.get_prefix().as_deref(), &attrs);
            }
        }
    }

    fn end(&mut self, node: &NodeRef) {
        match self {
            Out::E4x(w) => node.borrow().to_end_element(w),
            Out::Xml(x) => x.end_element(),
        }
    }
}

pub struct XmlBuilder {
    pub ab: AbstractBuilder,
    parent: Option<ModelRef>,
    allow_top_level_binding: bool,
    allow_two_way_bind: bool,
    pub xml: Option<ModelRef>,
}

/// `XMLBuilder.MixedContent`.
pub const MIXED_CONTENT: &str = "Mixed content not allowed here.";

impl XmlBuilder {
    /// `new XMLBuilder(unit, typeTable, mxmlConfiguration, document)`.
    pub fn new(ctx: &BuilderContext) -> XmlBuilder {
        let mut b = XmlBuilder::new_with_parent(ctx, None);
        b.allow_top_level_binding = true;
        b.allow_two_way_bind = true;
        b
    }

    /// `new XMLBuilder(unit, typeTable, mxmlConfiguration, document, parent)`.
    pub fn new_with_parent(ctx: &BuilderContext, parent: Option<ModelRef>) -> XmlBuilder {
        XmlBuilder { ab: AbstractBuilder::new(ctx), parent, allow_top_level_binding: false, allow_two_way_bind: false, xml: None }
    }

    fn xml(&self) -> ModelRef {
        self.xml.clone().expect("xml")
    }

    fn process_node(&mut self, e4x: bool, node: &NodeRef, w: &mut Out, get_elements_by_local_name: Option<&str>, destination_property_stack: &mut Vec<String>, namespaces: &mut Vec<PrefixMapping>) {
        let attribute_bindings = self.process_attributes(node);
        let index = node.borrow().get_index();
        if let Some(attribute_bindings) = attribute_bindings {
            let destination_property = create_expression(destination_property_stack);
            for (attr_name, be) in attribute_bindings.iter() {
                let mut ns_uri = attr_name.get_namespace().to_string();
                let mut ns_id = 0;
                let attr_expr;
                let gebln = get_elements_by_local_name.unwrap_or("null");
                if e4x {
                    if !ns_uri.is_empty() {
                        ns_id = prefix_mapping::get_namespace_id(&ns_uri, namespaces);
                    } else {
                        let pm = namespaces.last().expect("EmptyStackException");
                        ns_uri = pm.get_uri().to_string();
                        ns_id = pm.get_ns();
                    }
                    attr_expr = if ns_id > 0 { format!("{gebln}.@ns{ns_id}::{}", attr_name.get_local_part()) } else { format!("{gebln}.@{}", attr_name.get_local_part()) };
                } else {
                    attr_expr = format!("{gebln}.attributes[\"{}\"]", attr_name.get_local_part());
                }
                be.set_destination_e4x(e4x);
                be.set_destination_xml_attribute(true);
                be.set_destination_lvalue(Some(&attr_expr));
                be.set_destination_property(Some(&format!("{destination_property}[{index}]")));
                be.set_destination(&self.xml());
                self.xml().set_has_bindings(true);
                if e4x {
                    prefix_mapping::push_namespaces(be, namespaces);
                    if !ns_uri.is_empty() {
                        be.add_namespace(&ns_uri, ns_id);
                    }
                }
            }
        }

        w.start(node);
        let (count, first) = {
            let n = node.borrow();
            (n.get_child_count(), n.get_child_at(0))
        };
        if count == 1 && first.as_ref().is_some_and(|c| c.borrow().is_cdata_node()) {
            let cdata = first.unwrap();
            let (image, in_cdata, line) = {
                let c = cdata.borrow();
                (c.image.clone(), c.in_cdata(), c.begin_line)
            };
            if !image.is_empty() {
                if in_cdata {
                    if e4x {
                        w.raw(&format!("<![CDATA[{image}]]>"));
                    } else {
                        w.text(&image);
                    }
                } else {
                    match self.ab.text_parser.parse_binding_expression_line(&image, line) {
                        Some(be) => {
                            if be.is_two_way_primary() && !self.allow_two_way_bind {
                                self.ab.log_node(&cdata, err("Inline two-way data binding expression not allowed here.", &[]));
                            } else {
                                let destination_property = create_expression(destination_property_stack);
                                be.set_destination_lvalue(get_elements_by_local_name);
                                be.set_destination_property(Some(&format!("{destination_property}[{index}]")));
                                be.set_destination(&self.xml());
                                be.set_destination_xml_node(true);
                                self.xml().set_has_bindings(true);
                                if e4x {
                                    be.set_destination_e4x(true);
                                    prefix_mapping::push_namespaces(&be, namespaces);
                                }
                            }
                        }
                        None if e4x => w.raw(&text_parser::replace_binding_escapes_for_e4x(&image)),
                        None => {
                            let text = text_parser::cleanup_at_function_escapes(&text_parser::cleanup_binding_escapes(&image));
                            w.text(&text);
                        }
                    }
                }
            }
        } else {
            self.process_children(e4x, node, w, get_elements_by_local_name, destination_property_stack, namespaces);
        }
        w.end(node);
    }

    fn process_children(&mut self, e4x: bool, node: &NodeRef, w: &mut Out, get_elements_by_local_name: Option<&str>, destination_property: &mut Vec<String>, namespaces: &mut Vec<PrefixMapping>) {
        assign_indices(node);
        let mut num_cdata = 0;
        let children = node.borrow().get_children();
        for (i, child) in children.iter().enumerate() {
            if child.borrow().is_cdata_node() {
                num_cdata += 1;
                let (image, in_cdata, line) = {
                    let c = child.borrow();
                    (c.image.clone(), c.in_cdata(), c.begin_line)
                };
                if image.is_empty() {
                    continue;
                }
                if in_cdata {
                    if e4x {
                        w.raw(&format!("<![CDATA[{image}]]>"));
                    } else {
                        w.text(&image);
                    }
                    continue;
                }
                match self.ab.text_parser.parse_binding_expression_line(&image, line) {
                    Some(be) => {
                        if be.is_two_way_primary() && !self.allow_two_way_bind {
                            self.ab.log_node(child, err("Inline two-way data binding expression not allowed here.", &[]));
                        } else {
                            let gebln = get_elements_by_local_name.unwrap_or("null");
                            let index = node.borrow().get_index();
                            be.set_destination_lvalue(Some(&format!("{gebln}.text()[{}]", num_cdata - 1)));
                            be.set_destination_property(Some(&format!("{}[{index}].text()[{i}]", java_stack_to_string(destination_property))));
                            be.set_destination(&self.xml());
                            be.set_destination_xml_node(true);
                            self.xml().set_has_bindings(true);
                            if e4x {
                                be.set_destination_e4x(true);
                                prefix_mapping::push_namespaces(&be, namespaces);
                            }
                            w.raw("<![CDATA[]]>");
                        }
                    }
                    None if e4x => w.raw(&text_parser::replace_binding_escapes_for_e4x(&image)),
                    None => {
                        let text = text_parser::cleanup_at_function_escapes(&text_parser::cleanup_binding_escapes(&image));
                        w.text(&text);
                    }
                }
            } else if e4x {
                prefix_mapping::push_node_namespace(child, namespaces);
                match get_elements_by_local_name {
                    Some(gebln) => {
                        let (mut dest_prop, child_ns, child_index) = {
                            let c = child.borrow();
                            (c.get_local_part().to_string(), c.get_namespace().to_string(), c.get_index())
                        };
                        if !child_ns.is_empty() {
                            let pm = namespaces.last().unwrap();
                            dest_prop = format!("ns{}::{dest_prop}", pm.get_ns());
                        }
                        let e4xbuffer = format!("{gebln}.{dest_prop}[{child_index}]");
                        destination_property.push(dest_prop);
                        self.process_node(e4x, child, w, Some(&e4xbuffer), destination_property, namespaces);
                        destination_property.pop();
                    }
                    None => {
                        let id = self.xml().get_id();
                        self.process_node(e4x, child, w, id.as_deref(), destination_property, namespaces);
                    }
                }
                prefix_mapping::pop_node_namespace(namespaces);
            } else {
                let class_namespace_util = name_formatter::to_dot(&self.ab.standard_defs.class_namespaceutil);
                self.ab.document.add_import(&class_namespace_util, node.borrow().begin_line);
                let (child_local, child_index) = {
                    let c = child.borrow();
                    (c.get_local_part().to_string(), c.get_index())
                };
                let base = get_elements_by_local_name.map(str::to_string).or_else(|| self.xml().get_id()).unwrap_or_else(|| "null".into());
                let buffer = format!("{class_namespace_util}.getElementsByLocalName({base}, \"{child_local}\")[{child_index}]");
                destination_property.push(child_local);
                let mut none = Vec::new();
                self.process_node(e4x, child, w, Some(&buffer), destination_property, &mut none);
                destination_property.pop();
            }
        }
    }

    /// `processAttributes(node)`: binding-valued attributes, removed from the node.
    fn process_attributes(&mut self, node: &NodeRef) -> Option<JHashMap<QName, BindingExpressionRef>> {
        let mut attribute_bindings: Option<JHashMap<QName, BindingExpressionRef>> = None;
        let line = node.borrow().begin_line;
        let names = node.borrow().get_attribute_names();
        for qname in names {
            let value = node.borrow().get_attribute_value_qname(&qname).and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
            if let Some(be) = self.ab.text_parser.parse_binding_expression_line(&value, line) {
                if be.is_two_way_primary() && !self.allow_two_way_bind {
                    self.ab.log_node(node, err("Inline two-way data binding expression not allowed here.", &[]));
                    continue;
                }
                attribute_bindings.get_or_insert_with(JHashMap::new).insert(qname.clone(), be);
                node.borrow_mut().remove_attribute(&qname);
            }
        }
        attribute_bindings
    }
}

/// `assignIndices(parent)`: each element child's index among same-named siblings.
pub(crate) fn assign_indices(parent: &NodeRef) {
    let mut counts: JHashMap<String, i32> = JHashMap::new();
    let children = parent.borrow().get_children();
    for child in children {
        if child.borrow().is_cdata_node() {
            continue;
        }
        let image = child.borrow().image.clone();
        let num = match counts.get(&image) {
            None => 0,
            Some(n) => n + 1,
        };
        counts.insert(image, num);
        child.borrow_mut().set_index(num);
    }
}

/// `createExpression(stack)`.
pub(crate) fn create_expression(stack: &[String]) -> String {
    stack.join(".")
}

/// `Stack.toString()`, which Java concatenates where it means `createExpression`.
fn java_stack_to_string(stack: &[String]) -> String {
    format!("[{}]", stack.join(", "))
}

impl Builder for XmlBuilder {
    fn ab(&mut self) -> &mut AbstractBuilder {
        &mut self.ab
    }

    fn abr(&self) -> &AbstractBuilder {
        &self.ab
    }
}

impl Analyzer for XmlBuilder {
    analyzer_basics!();

    fn analyze_xml_node(&mut self, node: &NodeRef) {
        let id = attr_str(self.get_language_attribute_value(node, sd::PROP_ID));
        let e4x = node.borrow().is_e4x();
        let t = self.ab.type_table.get_type(self.ab.standard_defs.get_xml_backing_class_name(e4x)).expect("NullPointerException");
        let line = node.borrow().begin_line;
        let xml = Model::new_xml(&self.ab.document, t, self.parent.clone(), e4x, line);
        if let Some(id) = &id {
            xml.set_id(Some(id), false);
        }
        self.xml = Some(xml.clone());
        let mut writer = String::new();
        let (count, first) = {
            let n = node.borrow();
            (n.get_child_count(), n.get_child_at(0))
        };
        if count > 1 {
            self.ab.log_node(node, err("Only one root tag is allowed.", &[]));
        } else if count == 0 {
            writer.push_str("null");
        } else if first.as_ref().is_some_and(|c| c.borrow().is_cdata_node()) {
            let cdata = first.unwrap();
            let (image, cline) = {
                let c = cdata.borrow();
                (c.image.clone(), c.begin_line)
            };
            if !image.is_empty() {
                match self.ab.text_parser.parse_binding_expression_line(&image, cline) {
                    Some(be) => {
                        if self.allow_top_level_binding {
                            if !be.is_two_way_primary() || self.allow_two_way_bind {
                                be.set_destination(&xml);
                                writer.push_str("null");
                            } else {
                                self.ab.log_node(&cdata, err("Inline two-way data binding expression not allowed here.", &[]));
                            }
                        } else {
                            self.ab.log_node(&cdata, err("Data binding expression not allowed here.", &[]));
                        }
                    }
                    None => self.ab.log_node(node, err("XML content is required.", &[])),
                }
            }
        } else {
            let mut stack = Vec::new();
            let mut namespaces = Vec::new();
            let mut out = if e4x { Out::E4x(&mut writer) } else { Out::Xml(XmlStringSerializer::new(&mut writer)) };
            self.process_children(e4x, node, &mut out, None, &mut stack, &mut namespaces);
        }
        xml.set_literal_xml(Some(&writer));
    }
}
