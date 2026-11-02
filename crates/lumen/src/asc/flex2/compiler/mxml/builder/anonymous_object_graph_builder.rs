//! Port of `flex2.compiler.mxml.builder.AnonymousObjectGraphBuilder`, the base that `ModelBuilder` and
//! `ServiceRequestBuilder` hold.

use super::abstract_builder::{analyzer_basics, err, warn, AbstractBuilder, Builder, BuilderContext, FROM_ATTRIBUTE};
use crate::asc::flex2::compiler::mxml::dom::analyzer::Analyzer;
use crate::asc::flex2::compiler::mxml::dom::node::NodeRef;
use crate::asc::flex2::compiler::mxml::reflect::{TypeRef, TypeTable};
use crate::asc::flex2::compiler::mxml::rep::model::{Model, ModelRef, RValue};
use crate::asc::flex2::compiler::util::name_formatter;
use crate::asc::util::jhash::JHashMap;

pub struct AnonymousObjectGraphBuilder {
    pub ab: AbstractBuilder,
    pub graph: Option<ModelRef>,
    allow_two_way_bind: bool,
}

impl AnonymousObjectGraphBuilder {
    pub fn new(ctx: &BuilderContext) -> AnonymousObjectGraphBuilder {
        AnonymousObjectGraphBuilder { ab: AbstractBuilder::new(ctx), graph: None, allow_two_way_bind: true }
    }

    pub fn graph(&self) -> ModelRef {
        self.graph.clone().expect("graph")
    }

    pub fn set_allow_two_way_bind(&mut self, allow_two_way_bind: bool) {
        self.allow_two_way_bind = allow_two_way_bind;
    }

    fn process_node(&mut self, node: &NodeRef) -> Option<RValue> {
        let only_cdata = {
            let n = node.borrow();
            n.get_child_count() == 1 && n.get_child_at(0).is_some_and(|c| c.borrow().is_cdata_node())
        };
        if only_cdata {
            let cdata = node.borrow().get_child_at(0).unwrap();
            let (image, line) = {
                let c = cdata.borrow();
                (c.image.clone(), c.begin_line)
            };
            if image.is_empty() {
                return None;
            }
            let ot = self.ab.type_table.object_type();
            let desc = name_formatter::to_dot(&self.graph().get_type().get_name());
            let value = self.ab.text_parser.parse_value_simple(&image, &ot, 0, line, &desc);
            match value {
                Some(RValue::Binding(be)) => {
                    if !self.allow_two_way_bind && be.is_two_way_primary() {
                        self.ab.log_node(&cdata, err("Inline two-way data binding expression not allowed here.", &[]));
                        None
                    } else if self.has_attribute_initializers(node) {
                        self.ab.log_node(&cdata, err("Element may not contain both a binding expression and attribute initializers.", &[]));
                        None
                    } else {
                        Some(RValue::Binding(be))
                    }
                }
                Some(value) => {
                    let is_primitive = matches!(value, RValue::Str(_) | RValue::Integer(_) | RValue::Long(_) | RValue::Double(_) | RValue::Boolean(_));
                    if node.borrow().get_attribute_count() > 0 && is_primitive {
                        let p = Model::new_primitive_value(&self.ab.document, get_primitive_type(&self.ab.type_table, &value), &value, line);
                        let names = node.borrow().get_attribute_names();
                        for prop_name in names {
                            let (v, l) = {
                                let n = node.borrow();
                                (n.get_attribute_value_qname(&prop_name).map(|v| v.to_java_string()).unwrap_or_default(), n.get_line_number_qname(&prop_name))
                            };
                            p.set_property(prop_name.get_local_part(), RValue::Str(v), l);
                        }
                        return Some(RValue::Model(p));
                    }
                    Some(value)
                }
                None => None,
            }
        } else {
            let model = Model::new(&self.ab.document, self.graph().get_type().clone(), node.borrow().begin_line);
            model.set_id(Some(node.borrow().get_local_part()), false);
            model.set_is_anonymous(true);
            self.process_attributes(node, &model);
            self.process_children(node, &model);
            Some(RValue::Model(model))
        }
    }

    /// `processChildren(node, parent)`.
    pub fn process_children(&mut self, node: &NodeRef, parent: &ModelRef) {
        let counts = count_children(node);
        let arrays = self.create_arrays(parent, &counts);
        let children = node.borrow().get_children();
        for child in &children {
            if child.borrow().is_cdata_node() {
                let image = child.borrow().image.clone();
                self.ab.log_node(child, warn("Ignoring ${image} CDATA because other XML elements exist...", &[("image", &image)]));
                continue;
            }
            let (namespace_uri, local_part, line) = {
                let c = child.borrow();
                (c.get_namespace().to_string(), c.get_local_part().to_string(), c.begin_line)
            };
            if local_part == "Object" && namespace_uri.is_empty() {
                self.ab.log_node(child, err("Cannot have a tag named \"Object\" inside Models or services.", &[]));
                continue;
            }
            let Some(value) = self.process_node(child) else { continue };
            if let Some(array_val) = arrays.get(&local_part) {
                match &value {
                    RValue::Binding(bexpr) => {
                        bexpr.set_destination(array_val);
                        bexpr.set_destination_property_index(array_val.size() as i32);
                        bexpr.set_destination_lvalue(Some(&array_val.size().to_string()));
                    }
                    RValue::Model(value_model) => {
                        value_model.set_parent(Some(array_val.clone()));
                        value_model.set_parent_index_int(array_val.size());
                    }
                    _ => {}
                }
                array_val.add_entry(value, line);
            } else {
                match &value {
                    RValue::Binding(bexpr) => {
                        bexpr.set_destination(parent);
                        bexpr.set_destination_property(Some(&local_part));
                        bexpr.set_destination_lvalue(Some(&local_part));
                    }
                    RValue::Model(value_model) => value_model.set_parent(Some(parent.clone())),
                    _ => {}
                }
                parent.set_property(&local_part, value, line);
            }
        }
        for child in &children {
            if child.borrow().is_cdata_node() {
                continue;
            }
            let (namespace_uri, local_part) = {
                let c = child.borrow();
                (c.get_namespace().to_string(), c.get_local_part().to_string())
            };
            if local_part == "Object" && namespace_uri.is_empty() {
                continue;
            }
            if let Some(array_val) = arrays.get(&local_part) {
                parent.set_property_model(&local_part, array_val);
            }
        }
    }

    fn process_attributes(&mut self, node: &NodeRef, model: &ModelRef) {
        let names = node.borrow().get_attribute_names();
        for qname in names {
            let (text, line) = {
                let n = node.borrow();
                (n.get_attribute_value_qname(&qname).and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default(), n.get_line_number_qname(&qname))
            };
            self.process_dynamic_property_text(qname.get_local_part(), &text, FROM_ATTRIBUTE, line, model, None);
        }
    }

    fn create_arrays(&mut self, parent: &ModelRef, counts: &JHashMap<String, i32>) -> JHashMap<String, ModelRef> {
        let mut arrays = JHashMap::new();
        for (local_part, count) in counts.iter() {
            if *count > 1 {
                let a = Model::new_array(&self.ab.document, Some(parent.clone()), parent.get_xml_line_number(), Some(self.ab.type_table.object_type()));
                a.set_id(Some(local_part), false);
                a.set_is_anonymous(true);
                if parent.has_property(local_part) {
                    if let Some(v) = parent.get_property(local_part) {
                        a.add_entry(v, parent.get_xml_line_number());
                    }
                }
                arrays.insert(local_part.clone(), a);
            }
        }
        arrays
    }
}

fn count_children(node: &NodeRef) -> JHashMap<String, i32> {
    let mut counts: JHashMap<String, i32> = JHashMap::new();
    let n = node.borrow();
    let mut bump = |ns: &str, local: &str| {
        if local == "Object" && ns.is_empty() {
            return;
        }
        let c = counts.get(local).copied().unwrap_or(0);
        counts.insert(local.to_string(), c + 1);
    };
    for q in n.get_attribute_names() {
        bump(q.get_namespace(), q.get_local_part());
    }
    for child in n.get_children() {
        let c = child.borrow();
        if c.is_cdata_node() {
            continue;
        }
        bump(c.get_namespace(), c.get_local_part());
    }
    counts
}

fn get_primitive_type(type_table: &TypeTable, o: &RValue) -> TypeRef {
    match o {
        RValue::Boolean(_) => type_table.boolean_type(),
        RValue::Integer(_) | RValue::Long(_) | RValue::Double(_) => type_table.number_type(),
        _ => type_table.string_type(),
    }
}

impl Builder for AnonymousObjectGraphBuilder {
    fn ab(&mut self) -> &mut AbstractBuilder {
        &mut self.ab
    }

    fn abr(&self) -> &AbstractBuilder {
        &self.ab
    }
}

impl Analyzer for AnonymousObjectGraphBuilder {
    analyzer_basics!();
}
