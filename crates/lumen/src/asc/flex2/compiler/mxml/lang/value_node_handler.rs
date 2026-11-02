//! Port of `flex2.compiler.mxml.lang.ValueNodeHandler`, dispatching on the node's exact Java class.

use crate::asc::flex2::compiler::mxml::dom::node::{NodeClass, NodeRef};
use crate::asc::flex2::compiler::mxml::reflect::Assignable;
use crate::asc::flex2::compiler::mxml::rep::mxml_document::MxmlDocumentRef;

/// `isValueNode(node)`.
pub fn is_value_node(node: &NodeRef) -> bool {
    matches!(
        node.borrow().class,
        NodeClass::Node
            | NodeClass::Layered { .. }
            | NodeClass::Document(_)
            | NodeClass::Array
            | NodeClass::Primitive { .. }
            | NodeClass::Xml { .. }
            | NodeClass::XmlList
            | NodeClass::Model { .. }
            | NodeClass::Reparent
            | NodeClass::InlineComponent { .. }
            | NodeClass::Vector
    )
}

pub trait ValueNodeHandler {
    fn component_node(&mut self, assignable: Option<&Assignable>, node: &NodeRef, document: Option<&MxmlDocumentRef>);
    fn array_node(&mut self, assignable: Option<&Assignable>, node: &NodeRef);
    fn primitive_node(&mut self, assignable: Option<&Assignable>, node: &NodeRef);
    fn xml_node(&mut self, assignable: Option<&Assignable>, node: &NodeRef);
    fn xml_list_node(&mut self, assignable: Option<&Assignable>, node: &NodeRef);
    fn model_node(&mut self, assignable: Option<&Assignable>, node: &NodeRef);
    fn reparent_node(&mut self, assignable: Option<&Assignable>, node: &NodeRef);
    fn state_node(&mut self, assignable: Option<&Assignable>, node: &NodeRef);
    fn inline_component_node(&mut self, assignable: Option<&Assignable>, node: &NodeRef);
    fn cdata_node(&mut self, assignable: Option<&Assignable>, node: &NodeRef);
    fn vector_node(&mut self, assignable: Option<&Assignable>, node: &NodeRef);
    fn unknown(&mut self, assignable: Option<&Assignable>, node: &NodeRef);

    /// `invoke(property, node, document)`.
    fn invoke(&mut self, property: Option<&Assignable>, node: &NodeRef, document: Option<&MxmlDocumentRef>) {
        enum K {
            Component,
            Array,
            Primitive,
            Xml,
            XmlList,
            Model,
            InlineComponent,
            Reparent,
            State,
            CData,
            Vector,
            Unknown,
        }
        let k = match node.borrow().class {
            NodeClass::Node | NodeClass::Layered { .. } | NodeClass::DesignLayer { .. } | NodeClass::Document(_) => K::Component,
            NodeClass::Array => K::Array,
            NodeClass::Primitive { .. } => K::Primitive,
            NodeClass::Xml { .. } => K::Xml,
            NodeClass::XmlList => K::XmlList,
            NodeClass::Model { .. } => K::Model,
            NodeClass::InlineComponent { .. } => K::InlineComponent,
            NodeClass::Reparent => K::Reparent,
            NodeClass::State => K::State,
            NodeClass::CData { .. } => K::CData,
            NodeClass::Vector => K::Vector,
            _ => K::Unknown,
        };
        match k {
            K::Component => self.component_node(property, node, document),
            K::Array => self.array_node(property, node),
            K::Primitive => self.primitive_node(property, node),
            K::Xml => self.xml_node(property, node),
            K::XmlList => self.xml_list_node(property, node),
            K::Model => self.model_node(property, node),
            K::InlineComponent => self.inline_component_node(property, node),
            K::Reparent => self.reparent_node(property, node),
            K::State => self.state_node(property, node),
            K::CData => self.cdata_node(property, node),
            K::Vector => self.vector_node(property, node),
            K::Unknown => self.unknown(property, node),
        }
    }
}
