//! Port of `flex2.compiler.mxml.builder.XMLListBuilder`.

use std::rc::Rc;

use super::abstract_builder::{analyzer_basics, attr_str, err, warn, AbstractBuilder, Builder, BuilderContext};
use super::prefix_mapping::{self, PrefixMapping};
use super::xml_builder::{assign_indices, create_expression, MIXED_CONTENT};
use crate::asc::flex2::compiler::mxml::attribute::AttrValue;
use crate::asc::flex2::compiler::mxml::dom::analyzer::Analyzer;
use crate::asc::flex2::compiler::mxml::dom::node::NodeRef;
use crate::asc::flex2::compiler::mxml::lang::standard_defs as sd;
use crate::asc::flex2::compiler::mxml::lang::text_parser::{self, TextParser};
use crate::asc::flex2::compiler::mxml::rep::binding_expression::BindingExpressionRef;
use crate::asc::flex2::compiler::mxml::rep::model::{Model, ModelRef, RValue};
use crate::asc::flex2::compiler::util::QName;
use crate::asc::util::jhash::JHashMap;

pub struct XmlListBuilder {
    pub ab: AbstractBuilder,
    parent: Option<ModelRef>,
    allow_top_level_binding: bool,
    pub xml_list: Option<ModelRef>,
}

impl XmlListBuilder {
    /// `new XMLListBuilder(unit, typeTable, mxmlConfiguration, document)`.
    pub fn new(ctx: &BuilderContext) -> XmlListBuilder {
        let mut b = XmlListBuilder::new_with_parent(ctx, None);
        b.allow_top_level_binding = true;
        b
    }

    /// `new XMLListBuilder(unit, typeTable, mxmlConfiguration, document, parent)`.
    pub fn new_with_parent(ctx: &BuilderContext, parent: Option<ModelRef>) -> XmlListBuilder {
        XmlListBuilder { ab: AbstractBuilder::new(ctx), parent, allow_top_level_binding: false, xml_list: None }
    }

    fn xml_list(&self) -> ModelRef {
        self.xml_list.clone().expect("xmlList")
    }

    fn process_node(&mut self, node: &NodeRef, w: &mut String, e4x_elements_by_local_name: &str, destination_property_stack: &mut Vec<String>, namespaces: &mut Vec<PrefixMapping>) {
        let attribute_bindings = self.process_binding_attributes(node);
        self.process_resource_attributes(node);
        let index = node.borrow().get_index();
        if let Some(attribute_bindings) = attribute_bindings {
            let destination_property = create_expression(destination_property_stack);
            for (attr_name, be) in attribute_bindings.iter() {
                let mut ns_uri = attr_name.get_namespace().to_string();
                let ns_id;
                if !ns_uri.is_empty() {
                    ns_id = prefix_mapping::get_namespace_id(&ns_uri, namespaces);
                } else {
                    let pm = namespaces.last().expect("EmptyStackException");
                    ns_uri = pm.get_uri().to_string();
                    ns_id = pm.get_ns();
                }
                let attr_expr = if ns_id > 0 { format!("{e4x_elements_by_local_name}.@ns{ns_id}::{}", attr_name.get_local_part()) } else { format!("{e4x_elements_by_local_name}.@{}", attr_name.get_local_part()) };
                be.set_destination_lvalue(Some(&attr_expr));
                be.set_destination_property(Some(&format!("{destination_property}[{index}]")));
                be.set_destination(&self.xml_list());
                be.set_destination_xml_attribute(true);
                be.set_destination_e4x(true);
                self.xml_list().set_has_bindings(true);
                prefix_mapping::push_namespaces(be, namespaces);
                if !ns_uri.is_empty() {
                    be.add_namespace(&ns_uri, ns_id);
                }
            }
        }
        node.borrow().to_start_element(w);
        let (count, first) = {
            let n = node.borrow();
            (n.get_child_count(), n.get_child_at(0))
        };
        if count == 1 && first.as_ref().is_some_and(|c| c.borrow().is_cdata_node()) {
            let cdata = first.unwrap();
            let (image, line) = {
                let c = cdata.borrow();
                (c.image.clone(), c.begin_line)
            };
            if !image.is_empty() {
                match self.ab.text_parser.parse_binding_expression_line(&image, line) {
                    Some(be) => {
                        let destination_property = create_expression(destination_property_stack);
                        be.set_destination_lvalue(Some(e4x_elements_by_local_name));
                        be.set_destination_property(Some(&format!("{destination_property}[{index}]")));
                        be.set_destination(&self.xml_list());
                        be.set_destination_xml_node(true);
                        be.set_destination_e4x(true);
                        self.xml_list().set_has_bindings(true);
                        prefix_mapping::push_namespaces(&be, namespaces);
                    }
                    None => {
                        let text = text_parser::cleanup_at_function_escapes(&text_parser::cleanup_binding_escapes(&image));
                        w.push_str(&text);
                    }
                }
            }
        } else {
            self.process_children(node, w, Some(e4x_elements_by_local_name), destination_property_stack, namespaces);
        }
        node.borrow().to_end_element(w);
    }

    fn process_children(&mut self, node: &NodeRef, w: &mut String, e4x_elements_by_local_name: Option<&str>, destination_property_stack: &mut Vec<String>, namespaces: &mut Vec<PrefixMapping>) {
        assign_indices(node);
        let children = node.borrow().get_children();
        for (i, child) in children.iter().enumerate() {
            if child.borrow().is_cdata_node() {
                let image = child.borrow().image.clone();
                if !java_trim(&image).is_empty() {
                    self.ab.log_node(child, warn(MIXED_CONTENT, &[("image", &image)]));
                } else {
                    w.push_str(&image);
                }
                continue;
            }
            prefix_mapping::push_node_namespace(child, namespaces);
            let (local, child_ns, child_index) = {
                let c = child.borrow();
                (c.get_local_part().to_string(), c.get_namespace().to_string(), c.get_index())
            };
            match e4x_elements_by_local_name {
                Some(base) => {
                    let mut dest_prop = local;
                    if !child_ns.is_empty() {
                        let pm = namespaces.last().unwrap();
                        dest_prop = format!("ns{}::{dest_prop}", pm.get_ns());
                    }
                    let e4xbuffer = format!("{base}.{dest_prop}[{child_index}]");
                    destination_property_stack.push(dest_prop);
                    self.process_node(child, w, &e4xbuffer, destination_property_stack, namespaces);
                    destination_property_stack.pop();
                }
                None => {
                    let e4xbuffer = format!("{}[{i}]", self.xml_list().get_id().unwrap_or_else(|| "null".into()));
                    destination_property_stack.push(local);
                    self.process_node(child, w, &e4xbuffer, destination_property_stack, namespaces);
                    destination_property_stack.pop();
                }
            }
            prefix_mapping::pop_node_namespace(namespaces);
        }
    }

    fn process_binding_attributes(&mut self, node: &NodeRef) -> Option<JHashMap<QName, BindingExpressionRef>> {
        let mut attribute_bindings: Option<JHashMap<QName, BindingExpressionRef>> = None;
        let line = node.borrow().begin_line;
        let names = node.borrow().get_attribute_names();
        for qname in names {
            let value = node.borrow().get_attribute_value_qname(&qname).and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
            if let Some(be) = self.ab.text_parser.parse_binding_expression_line(&value, line) {
                attribute_bindings.get_or_insert_with(JHashMap::new).insert(qname.clone(), be);
                node.borrow_mut().remove_attribute(&qname);
            }
        }
        attribute_bindings
    }

    fn process_resource_attributes(&mut self, node: &NodeRef) {
        let mut attribute_resources: JHashMap<QName, Rc<crate::asc::flex2::compiler::mxml::rep::at_resource::AtResource>> = JHashMap::new();
        let line = node.borrow().begin_line;
        let names = node.borrow().get_attribute_names();
        for qname in names {
            let text = node.borrow().get_attribute_value_qname(&qname).and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
            let at_function = text_parser::get_at_function_name(&text);
            if at_function.as_deref() == Some("Resource") {
                let st = self.ab.type_table.string_type();
                if let Some(RValue::AtResource(r)) = self.ab.text_parser.resource(&text, &st) {
                    attribute_resources.insert(qname.clone(), r);
                }
            } else if let Some(f) = at_function {
                self.ab.text_parser.desc = Some(f);
                self.ab.text_parser.line = line;
                self.ab.text_parser.error(text_parser::ERR_UNRECOGNIZED_AT_FUNCTION, "null", None, None);
            }
        }
        for (qname, at_resource) in attribute_resources.iter() {
            node.borrow_mut().add_attribute(qname.get_namespace(), qname.get_local_part(), AttrValue::AtResource(at_resource.clone()), line);
        }
    }
}

/// `String.trim()`.
fn java_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c <= ' ')
}

impl Builder for XmlListBuilder {
    fn ab(&mut self) -> &mut AbstractBuilder {
        &mut self.ab
    }

    fn abr(&self) -> &AbstractBuilder {
        &self.ab
    }
}

impl Analyzer for XmlListBuilder {
    analyzer_basics!();

    fn analyze_xml_list_node(&mut self, node: &NodeRef) {
        let id = attr_str(self.get_language_attribute_value(node, sd::PROP_ID));
        let line = node.borrow().begin_line;
        let xml_list = Model::new_xml_list(&self.ab.document, self.ab.type_table.xml_list_type(), self.parent.clone(), line);
        if let Some(id) = &id {
            xml_list.set_id(Some(id), false);
        }
        self.xml_list = Some(xml_list.clone());
        let mut writer = String::new();
        let (count, first) = {
            let n = node.borrow();
            (n.get_child_count(), n.get_child_at(0))
        };
        if count == 1 && first.as_ref().is_some_and(|c| c.borrow().is_cdata_node()) {
            let cdata = first.unwrap();
            let (image, cline) = {
                let c = cdata.borrow();
                (c.image.clone(), c.begin_line)
            };
            if !image.is_empty() {
                match self.ab.text_parser.parse_binding_expression_line(&image, cline) {
                    Some(be) => {
                        if self.allow_top_level_binding {
                            be.set_destination(&xml_list);
                            writer.push_str("null");
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
            self.process_children(node, &mut writer, None, &mut stack, &mut namespaces);
        }
        xml_list.set_literal_xml(Some(&writer));
    }
}
