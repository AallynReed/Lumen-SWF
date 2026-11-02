//! Port of `flex2.compiler.mxml.lang.NodeTypeResolver`.

use std::rc::Rc;

use super::standard_defs::{CLASS_XMLNODE, PROP_TYPE};
use super::value_node_handler::ValueNodeHandler;
use crate::asc::flex2::compiler::mxml::dom::node::{NodeRef, PrimitiveClass};
use crate::asc::flex2::compiler::mxml::reflect::{Assignable, TypeRef, TypeTable};
use crate::asc::flex2::compiler::mxml::rep::mxml_document::MxmlDocumentRef;
use crate::asc::flex2::compiler::util::name_formatter;

pub struct NodeTypeResolver {
    type_table: Rc<TypeTable>,
    type_: Option<TypeRef>,
}

impl NodeTypeResolver {
    pub fn new(type_table: Rc<TypeTable>) -> NodeTypeResolver {
        NodeTypeResolver { type_table, type_: None }
    }

    pub fn resolve_type(&mut self, node: &NodeRef, document: Option<&MxmlDocumentRef>) -> Option<TypeRef> {
        self.invoke(None, node, document);
        self.type_.clone()
    }

    #[allow(dead_code)]
    fn design_layer_node(&mut self, property: Option<&Assignable>, node: &NodeRef, document: Option<&MxmlDocumentRef>) {
        self.component_node(property, node, document);
    }
}

impl ValueNodeHandler for NodeTypeResolver {
    fn component_node(&mut self, _property: Option<&Assignable>, node: &NodeRef, document: Option<&MxmlDocumentRef>) {
        let (ns, local) = {
            let n = node.borrow();
            (n.get_namespace().to_string(), n.get_local_part().to_string())
        };
        if let Some(document) = document {
            if let Some(class_name) = document.get_local_class(&ns, &local) {
                if let Some(local_type) = self.type_table.get_type(&class_name) {
                    self.type_ = Some(local_type);
                    return;
                }
            }
        }
        self.type_ = self.type_table.get_type_ns(&ns, &local);
    }

    fn array_node(&mut self, _property: Option<&Assignable>, _node: &NodeRef) {
        self.type_ = Some(self.type_table.array_type());
    }

    fn primitive_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        let tt = &self.type_table;
        self.type_ = match node.borrow().primitive_class() {
            Some(PrimitiveClass::Boolean) => Some(tt.boolean_type()),
            Some(PrimitiveClass::Number) => Some(tt.number_type()),
            Some(PrimitiveClass::Int) => Some(tt.int_type()),
            Some(PrimitiveClass::UInt) => Some(tt.uint_type()),
            Some(PrimitiveClass::String) => Some(tt.string_type()),
            Some(PrimitiveClass::Class) => Some(tt.class_type()),
            Some(PrimitiveClass::Function) => Some(tt.function_type()),
            None => None,
        };
    }

    fn vector_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        let type_attribute_value = node.borrow().get_attribute(PROP_TYPE).and_then(|a| a.get_value().as_str().map(str::to_string)).expect("NullPointerException");
        if let Some(element_type) = self.type_table.get_type(&name_formatter::to_colon(&type_attribute_value)) {
            self.type_ = Some(self.type_table.get_vector_type(&element_type));
        }
    }

    fn xml_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        if node.borrow().is_e4x() {
            self.type_ = Some(self.type_table.xml_type());
        } else {
            self.type_ = self.type_table.get_type(CLASS_XMLNODE);
        }
    }

    fn xml_list_node(&mut self, _property: Option<&Assignable>, _node: &NodeRef) {
        self.type_ = Some(self.type_table.xml_list_type());
    }

    fn model_node(&mut self, _property: Option<&Assignable>, _node: &NodeRef) {
        self.type_ = Some(self.type_table.object_type());
    }

    fn inline_component_node(&mut self, _property: Option<&Assignable>, _node: &NodeRef) {
        self.type_ = self.type_table.get_type(&self.type_table.get_standard_defs().interface_ifactory);
    }

    fn reparent_node(&mut self, _property: Option<&Assignable>, _node: &NodeRef) {
        self.type_ = Some(self.type_table.object_type());
    }

    fn state_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        let (ns, local) = {
            let n = node.borrow();
            (n.get_namespace().to_string(), n.get_local_part().to_string())
        };
        self.type_ = self.type_table.get_type_ns(&ns, &local);
    }

    fn cdata_node(&mut self, _property: Option<&Assignable>, _node: &NodeRef) {
        self.type_ = Some(self.type_table.object_type());
    }

    fn unknown(&mut self, _property: Option<&Assignable>, _node: &NodeRef) {
        self.type_ = None;
    }
}
