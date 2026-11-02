//! Port of `flex2.compiler.mxml.builder.VectorBuilder`.

use super::abstract_builder::{analyzer_basics, attr_str, err, AbstractBuilder, BindingHandlerKind, Builder, BuilderContext};
use super::array_builder::ArrayBuilder;
use super::component_builder::ComponentBuilder;
use super::inline_component_builder::InlineComponentBuilder;
use super::primitive_builder::PrimitiveBuilder;
use crate::asc::flex2::compiler::mxml::dom::analyzer::Analyzer;
use crate::asc::flex2::compiler::mxml::dom::node::{analyze, NodeRef};
use crate::asc::flex2::compiler::mxml::lang::standard_defs as sd;
use crate::asc::flex2::compiler::mxml::lang::text_parser;
use crate::asc::flex2::compiler::mxml::lang::value_node_handler::ValueNodeHandler;
use crate::asc::flex2::compiler::mxml::reflect::{Assignable, Stateful, TypeRef};
use crate::asc::flex2::compiler::mxml::rep::model::{Model, ModelRef, RValue};
use crate::asc::flex2::compiler::mxml::rep::mxml_document::MxmlDocumentRef;
use crate::asc::flex2::compiler::util::name_formatter;

const FLAGS: i32 = text_parser::FLAG_IGNORE_ARRAY_SYNTAX | text_parser::FLAG_IGNORE_AT_FUNCTION | text_parser::FLAG_IGNORE_AT_FUNCTION_ESCAPE;

pub struct VectorBuilder {
    pub ab: AbstractBuilder,
    element_type: Option<TypeRef>,
    parent: Option<ModelRef>,
    assignable_property: Option<Assignable>,
    allow_binding: bool,
    pub vector: Option<ModelRef>,
}

impl VectorBuilder {
    /// `new VectorBuilder(unit, typeTable, mxmlConfiguration, document, parent, assignable[, elementType], allowBinding)`.
    pub fn new(ctx: &BuilderContext, parent: Option<ModelRef>, assignable: Option<Assignable>, element_type: Option<TypeRef>, allow_binding: bool) -> VectorBuilder {
        VectorBuilder { ab: AbstractBuilder::new(ctx), element_type, parent, assignable_property: assignable, allow_binding, vector: None }
    }

    fn vector(&self) -> ModelRef {
        self.vector.clone().expect("vector")
    }

    /// `createVectorModel(line, fixed)`.
    pub fn create_vector_model(&mut self, line: i32, fixed: bool) {
        let et = self.get_element_type().expect("NullPointerException");
        let vector = Model::new_vector(&self.ab.document, self.parent.clone(), line, et, fixed);
        vector.set_parent_index_state(self.get_name().as_deref(), self.get_state_name().as_deref());
        self.vector = Some(vector);
    }

    pub fn create_synthetic_vector_model(&mut self, line: i32) {
        self.create_vector_model(line, false);
    }

    fn ensure_id(&mut self, node: &NodeRef) {
        if let Some(id) = attr_str(self.get_language_attribute_value(node, sd::PROP_ID)) {
            self.vector().set_id(Some(&id), false);
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
            let assignable = self.assignable_property.clone();
            for node in nodes {
                let mut h = ElementNodeHandler { b: self };
                h.invoke(assignable.as_ref(), node, Some(&doc));
            }
        }
    }

    fn get_name(&self) -> Option<String> {
        self.assignable_property.as_ref().and_then(|a| a.get_name())
    }

    fn get_state_name(&self) -> Option<String> {
        self.assignable_property.as_ref().and_then(|a| a.get_state_name())
    }

    fn get_element_type(&self) -> Option<TypeRef> {
        if self.element_type.is_some() {
            self.element_type.clone()
        } else {
            self.assignable_property.as_ref().and_then(|a| a.get_element_type())
        }
    }

    /// `processTextInitializer(text, vectorElementType, cdata, line)`.
    pub fn process_text_initializer(&mut self, text: &str, vector_element_type: &TypeRef, cdata: bool, line: i32) {
        let flags = if cdata { text_parser::FLAG_IN_CDATA } else { 0 };
        let vector_type = self.ab.type_table.get_vector_type(vector_element_type);
        let result = self.ab.text_parser.parse_value_elem(text, &vector_type, Some(vector_element_type), flags, line, "Vector");
        match result {
            Some(RValue::Binding(be)) => {
                if self.allow_binding {
                    match &self.parent {
                        Some(p) => be.set_destination(p),
                        None => be.set_destination(&self.vector()),
                    }
                    be.set_destination_lvalue(self.get_name().as_deref());
                    be.set_destination_property(self.get_name().as_deref());
                } else {
                    self.ab.log_line(line, err("Data binding expression not allowed here.", &[]));
                }
            }
            Some(RValue::Model(result)) => self.vector().set_entries(result.get_entries()),
            _ => {}
        }
    }
}

impl Builder for VectorBuilder {
    fn ab(&mut self) -> &mut AbstractBuilder {
        &mut self.ab
    }

    fn abr(&self) -> &AbstractBuilder {
        &self.ab
    }
}

impl Analyzer for VectorBuilder {
    analyzer_basics!();

    fn analyze_vector_node(&mut self, node: &NodeRef) {
        let mut fixed = false;
        let line = node.borrow().begin_line;
        if let Some(fixed_attribute) = attr_str(self.get_language_attribute_value(node, sd::PROP_FIXED)) {
            let bt = self.ab.type_table.boolean_type();
            match self.ab.text_parser.parse_value_simple(&fixed_attribute, &bt, FLAGS, line, sd::PROP_FIXED) {
                Some(RValue::Binding(_)) => self.ab.log_line(line, err("Data binding expression not allowed here.", &[])),
                Some(RValue::Boolean(b)) => fixed = b,
                _ => {}
            }
        }
        self.create_vector_model(line, fixed);
        let vector = self.vector();
        self.process_state_attributes(node, &vector);
        self.ensure_id(node);
        let children = node.borrow().get_children();
        self.process_children(&children);
        let top = self.parent.is_none();
        self.register_model(node, &vector, top);
    }
}

/// `VectorBuilder.ElementNodeHandler`.
struct ElementNodeHandler<'a> {
    b: &'a mut VectorBuilder,
}

impl ElementNodeHandler<'_> {
    fn binding_handler(&self) -> BindingHandlerKind {
        BindingHandlerKind::ArrayElement(self.b.vector())
    }

    fn wrong_element_type(&self, node: &NodeRef) {
        let et = self.b.get_element_type().map(|t| t.get_name()).unwrap_or_else(|| "null".into());
        self.b.ab.log_node(node, err("Elements of this Vector must be assignable to '${elementTypeName}'.", &[("elementTypeName", &et)]));
    }

    fn add_indexed(&mut self, node: &NodeRef, m: &ModelRef) {
        let vector = self.b.vector();
        if m.get_type().is_assignable_to(self.b.get_element_type().as_deref()) {
            m.set_parent_index_int(vector.size());
            vector.add_entry_model(m);
        } else {
            self.wrong_element_type(node);
        }
    }

    fn add_plain(&mut self, node: &NodeRef, m: &ModelRef) {
        if m.get_type().is_assignable_to(self.b.get_element_type().as_deref()) {
            self.b.vector().add_entry_model(m);
        } else {
            self.wrong_element_type(node);
        }
    }

    fn component(&mut self, node: &NodeRef, binding: Option<BindingHandlerKind>) {
        let mut builder = ComponentBuilder::new(&self.b.ab.ctx, Some(self.b.vector()), None, None, false, binding);
        analyze(node, &mut builder);
        let c = builder.component();
        self.add_indexed(node, &c);
    }

    fn primitive(&mut self, node: &NodeRef) {
        let mut builder = PrimitiveBuilder::new(&self.b.ab.ctx, Some(self.b.vector()), false, None, Some(self.binding_handler()));
        analyze(node, &mut builder);
        let v = builder.value.clone().expect("value");
        self.add_plain(node, &v);
    }

    fn not_supported(&self, node: &NodeRef) {
        self.b.ab.log_node(node, crate::asc::flex2::compiler::toolkit::CompilerMessage::error("flex2.compiler.mxml.builder.VectorBuilder$ElementNotSupported"));
    }
}

impl ValueNodeHandler for ElementNodeHandler<'_> {
    fn component_node(&mut self, _property: Option<&Assignable>, node: &NodeRef, _document: Option<&MxmlDocumentRef>) {
        let h = self.binding_handler();
        self.component(node, Some(h));
    }

    fn array_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        let mut builder = ArrayBuilder::new(&self.b.ab.ctx, Some(self.b.vector()), None, self.b.allow_binding);
        analyze(node, &mut builder);
        let a = builder.array.clone().expect("array");
        self.add_indexed(node, &a);
    }

    fn vector_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        let type_attribute_value = node.borrow().get_attribute_str(sd::PROP_TYPE).map(str::to_string).expect("NullPointerException");
        let element_type = self.b.ab.type_table.get_type(&name_formatter::to_colon(&type_attribute_value));
        let mut builder = VectorBuilder::new(&self.b.ab.ctx, Some(self.b.vector()), None, element_type, self.b.allow_binding);
        analyze(node, &mut builder);
        let v = builder.vector();
        self.add_indexed(node, &v);
    }

    fn primitive_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        self.primitive(node);
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
        let r = builder.get_rvalue().expect("rvalue");
        self.add_plain(node, &r);
    }

    fn reparent_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        self.component(node, None);
    }

    fn cdata_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        self.primitive(node);
    }

    fn state_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        self.component(node, None);
    }

    fn unknown(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        self.b.ab.log_node(node, crate::asc::flex2::compiler::toolkit::CompilerMessage::error("flex2.compiler.mxml.builder.VectorBuilder$UnknownNode"));
    }
}
