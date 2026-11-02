//! Port of `flex2.compiler.mxml.builder.ArrayBuilder`.

use super::abstract_builder::{analyzer_basics, attr_str, err, AbstractBuilder, BindingHandlerKind, Builder, BuilderContext};
use super::component_builder::ComponentBuilder;
use super::inline_component_builder::InlineComponentBuilder;
use super::primitive_builder::PrimitiveBuilder;
use super::vector_builder::VectorBuilder;
use crate::asc::flex2::compiler::mxml::dom::analyzer::Analyzer;
use crate::asc::flex2::compiler::mxml::dom::node::{analyze, NodeRef};
use crate::asc::flex2::compiler::mxml::lang::standard_defs as sd;
use crate::asc::flex2::compiler::mxml::lang::text_parser;
use crate::asc::flex2::compiler::mxml::lang::value_node_handler::ValueNodeHandler;
use crate::asc::flex2::compiler::mxml::reflect::{Assignable, Stateful, TypeRef};
use crate::asc::flex2::compiler::mxml::rep::model::{Model, ModelRef, RValue};
use crate::asc::flex2::compiler::mxml::rep::mxml_document::MxmlDocumentRef;
use crate::asc::flex2::compiler::util::name_formatter;

pub struct ArrayBuilder {
    pub ab: AbstractBuilder,
    parent: Option<ModelRef>,
    assignable: Option<Assignable>,
    allow_binding: bool,
    pub array: Option<ModelRef>,
}

impl ArrayBuilder {
    /// `new ArrayBuilder(unit, typeTable, mxmlConfiguration, document)`.
    pub fn new_top(ctx: &BuilderContext) -> ArrayBuilder {
        ArrayBuilder::new(ctx, None, None, true)
    }

    /// `new ArrayBuilder(unit, typeTable, mxmlConfiguration, document, parent, assignable, allowBinding)`.
    pub fn new(ctx: &BuilderContext, parent: Option<ModelRef>, assignable: Option<Assignable>, allow_binding: bool) -> ArrayBuilder {
        ArrayBuilder { ab: AbstractBuilder::new(ctx), parent, assignable, allow_binding, array: None }
    }

    fn array(&self) -> ModelRef {
        self.array.clone().expect("array")
    }

    /// `createArrayModel(line)`.
    pub fn create_array_model(&mut self, line: i32) {
        let array = Model::new_array(&self.ab.document, self.parent.clone(), line, self.get_element_type());
        array.set_parent_index_state(self.get_name().as_deref(), self.get_state_name().as_deref());
        self.array = Some(array);
    }

    pub fn create_synthetic_array_model(&mut self, line: i32) {
        self.create_array_model(line);
    }

    fn ensure_id(&mut self, node: &NodeRef) {
        if let Some(id) = attr_str(self.get_language_attribute_value(node, sd::PROP_ID)) {
            self.array().set_id(Some(&id), false);
        }
    }

    /// `processChildren(nodes)`.
    pub fn process_children(&mut self, nodes: &[NodeRef]) {
        if let Some(cdata) = self.get_text_content(nodes, true) {
            let (image, in_cdata, line) = {
                let c = cdata.borrow();
                (c.image.clone(), c.in_cdata(), c.begin_line)
            };
            let ot = self.ab.type_table.object_type();
            self.process_text_initializer(&image, &ot, in_cdata, line);
        } else {
            let doc = self.ab.document.clone();
            let assignable = self.assignable.clone();
            for node in nodes {
                let mut h = ElementNodeHandler { b: self };
                h.invoke(assignable.as_ref(), node, Some(&doc));
            }
        }
    }

    fn get_name(&self) -> Option<String> {
        self.assignable.as_ref().and_then(|a| a.get_name())
    }

    fn get_state_name(&self) -> Option<String> {
        self.assignable.as_ref().and_then(|a| a.get_state_name())
    }

    fn get_element_type(&self) -> Option<TypeRef> {
        match &self.assignable {
            Some(a) => a.get_element_type(),
            None => Some(self.ab.type_table.object_type()),
        }
    }

    /// `processTextInitializer(text, arrayElementType, cdata, line)`.
    pub fn process_text_initializer(&mut self, text: &str, array_element_type: &TypeRef, cdata: bool, line: i32) {
        let flags = if cdata { text_parser::FLAG_IN_CDATA } else { 0 };
        let at = self.ab.type_table.array_type();
        let result = self.ab.text_parser.parse_value_elem(text, &at, Some(array_element_type), flags, line, &at.get_name());
        match result {
            Some(RValue::Binding(be)) => {
                if self.allow_binding {
                    match &self.parent {
                        Some(p) => be.set_destination(p),
                        None => be.set_destination(&self.array()),
                    }
                    be.set_destination_lvalue(self.get_name().as_deref());
                    be.set_destination_property(self.get_name().as_deref());
                } else {
                    self.ab.log_line(line, err("Data binding expression not allowed here.", &[]));
                }
            }
            Some(RValue::Model(result)) => self.array().set_entries(result.get_entries()),
            _ => {}
        }
    }
}

impl Builder for ArrayBuilder {
    fn ab(&mut self) -> &mut AbstractBuilder {
        &mut self.ab
    }

    fn abr(&self) -> &AbstractBuilder {
        &self.ab
    }
}

impl Analyzer for ArrayBuilder {
    analyzer_basics!();

    fn analyze_array_node(&mut self, node: &NodeRef) {
        self.create_array_model(node.borrow().begin_line);
        let array = self.array();
        self.process_state_attributes(node, &array);
        self.ensure_id(node);
        let children = node.borrow().get_children();
        self.process_children(&children);
        let top = self.parent.is_none();
        self.register_model(node, &array, top);
    }
}

/// `ArrayBuilder.ElementNodeHandler`.
struct ElementNodeHandler<'a> {
    b: &'a mut ArrayBuilder,
}

impl ElementNodeHandler<'_> {
    fn binding_handler(&self) -> BindingHandlerKind {
        BindingHandlerKind::ArrayElement(self.b.array())
    }

    fn component(&mut self, node: &NodeRef, binding: Option<BindingHandlerKind>) {
        let array = self.b.array();
        let mut builder = ComponentBuilder::new(&self.b.ab.ctx, Some(array.clone()), None, None, false, binding);
        analyze(node, &mut builder);
        let c = builder.component();
        c.set_parent_index_int(array.size());
        array.add_entry_model(&c);
    }

    fn primitive(&mut self, property: Option<&Assignable>, node: &NodeRef) {
        let array = self.b.array();
        let mut builder = PrimitiveBuilder::new(&self.b.ab.ctx, Some(array.clone()), false, property.cloned(), Some(self.binding_handler()));
        analyze(node, &mut builder);
        array.add_entry_model(&builder.value.clone().expect("value"));
    }

    fn not_supported(&self, node: &NodeRef) {
        let image = node.borrow().image.clone();
        self.b.ab.log_node(node, err("${image} not currently supported as Array element.", &[("image", &image)]));
    }
}

impl ValueNodeHandler for ElementNodeHandler<'_> {
    fn component_node(&mut self, _property: Option<&Assignable>, node: &NodeRef, _document: Option<&MxmlDocumentRef>) {
        let h = self.binding_handler();
        self.component(node, Some(h));
    }

    fn array_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        let array = self.b.array();
        let mut builder = ArrayBuilder::new(&self.b.ab.ctx, Some(array.clone()), None, self.b.allow_binding);
        analyze(node, &mut builder);
        let a = builder.array();
        a.set_parent_index_int(array.size());
        array.add_entry_model(&a);
    }

    fn vector_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        let array = self.b.array();
        let type_attribute_value = node.borrow().get_attribute_str(sd::PROP_TYPE).map(str::to_string).expect("NullPointerException");
        let element_type = self.b.ab.type_table.get_type(&name_formatter::to_colon(&type_attribute_value));
        let mut builder = VectorBuilder::new(&self.b.ab.ctx, Some(array.clone()), None, element_type, self.b.allow_binding);
        analyze(node, &mut builder);
        let v = builder.vector.clone().expect("vector");
        v.set_parent_index_int(array.size());
        array.add_entry_model(&v);
    }

    fn primitive_node(&mut self, property: Option<&Assignable>, node: &NodeRef) {
        self.primitive(property, node);
    }

    fn xml_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        self.not_supported(node);
    }

    fn xml_list_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        self.not_supported(node);
    }

    fn model_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        self.not_supported(node);
    }

    fn inline_component_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        let mut builder = InlineComponentBuilder::new(&self.b.ab.ctx, false);
        analyze(node, &mut builder);
        self.b.array().add_entry_model(&builder.get_rvalue().expect("rvalue"));
    }

    fn reparent_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        self.component(node, None);
    }

    fn cdata_node(&mut self, property: Option<&Assignable>, node: &NodeRef) {
        self.primitive(property, node);
    }

    fn state_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        self.component(node, None);
    }

    fn unknown(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        let image = node.borrow().image.clone();
        self.b.ab.log_node(node, err("${image} not allowed here.", &[("image", &image)]));
    }
}
