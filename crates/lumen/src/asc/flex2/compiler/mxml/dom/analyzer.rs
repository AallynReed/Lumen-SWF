//! Port of `flex2.compiler.mxml.dom.Analyzer` merged with the defaults of `AnalyzerAdapter`, which every analyzer
//! extends: one `analyze_*` method per `analyze(XNode)` overload, defaulting to `traverse`.

use super::analyzer_adapter::AnalyzerAdapter;
use super::node::{analyze, NodeRef};
use crate::asc::flex2::compiler::mxml::attribute::{AttrValue, Attribute};
use crate::asc::flex2::compiler::toolkit::CompilerMessage;

pub trait Analyzer {
    fn adapter(&self) -> &AnalyzerAdapter;
    fn adapter_mut(&mut self) -> &mut AnalyzerAdapter;

    /// `getDocumentVersion()` (abstract in `AnalyzerAdapter`).
    fn get_document_version(&self) -> i32;
    /// `getLanguageNamespace()` (abstract in `AnalyzerAdapter`).
    fn get_language_namespace(&self) -> Option<String>;

    fn prepare(&mut self, node: &NodeRef) {
        self.adapter_mut().current_node = Some(node.clone());
    }

    fn analyze_cdata_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_style_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_script_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_meta_data_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_model_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_xml_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_xml_list_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_array_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_vector_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_binding_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_string_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_number_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_int_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_uint_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_boolean_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_class_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_function_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_web_service_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_http_service_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_remote_object_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_operation_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_request_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_method_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_arguments_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_inline_component_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_declarations_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_library_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_definition_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_reparent_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_state_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_private_node(&mut self, _node: &NodeRef) {}
    fn analyze_design_layer_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    /// `analyze(Node)`, which a `DocumentNode` also reaches.
    fn analyze_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }
    fn analyze_layered_node(&mut self, node: &NodeRef) {
        self.traverse(node)
    }

    fn traverse(&mut self, node: &NodeRef) {
        let count = node.borrow().get_child_count();
        for i in 0..count {
            let n = node.borrow().get_child_at(i).expect("NullPointerException");
            analyze(&n, self);
        }
    }

    /// `getTextContent(nodes, allowNonText)`.
    fn get_text_content(&mut self, nodes: &[NodeRef], allow_non_text: bool) -> Option<NodeRef> {
        let first = nodes.first()?;
        if first.borrow().is_cdata_node() {
            if nodes.len() == 1 {
                return Some(first.clone());
            } else if self.get_document_version() < 4 {
                let second = nodes[1].clone();
                self.adapter().log(&second, CompilerMessage::error("Mixed content is not allowed here."));
            }
        } else if !allow_non_text {
            let line = first.borrow().begin_line;
            self.adapter().log_line(line, CompilerMessage::error("Child elements are not allowed here."));
        }
        None
    }

    /// `getLanguageAttribute(node, name)`.
    fn get_language_attribute(&self, node: &NodeRef, name: &str) -> Option<Attribute> {
        let n = node.borrow();
        let mut attr = n.get_attribute_uri("", name).cloned();
        if attr.is_none() && self.get_document_version() >= 4 {
            attr = n.get_attribute_uri(&self.get_language_namespace().unwrap_or_default(), name).cloned();
        }
        attr
    }

    /// `getLanguageAttributeValue(node, name)`.
    fn get_language_attribute_value(&self, node: &NodeRef, name: &str) -> Option<AttrValue> {
        self.get_language_attribute(node, name).map(|a| a.get_value().clone())
    }
}
