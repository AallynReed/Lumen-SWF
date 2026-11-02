//! Port of `flex2.compiler.mxml.analyzer.WebServiceAnalyzer`.

use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::mxml::dom::analyzer::Analyzer;
use crate::asc::flex2::compiler::mxml::dom::analyzer_adapter::AnalyzerAdapter;
use crate::asc::flex2::compiler::mxml::dom::node::NodeRef;
use crate::asc::flex2::compiler::mxml::rep::mxml_document::MxmlDocumentRef;
use crate::asc::flex2::compiler::toolkit::CompilerMessage;

pub struct WebServiceAnalyzer {
    adapter: AnalyzerAdapter,
    document: MxmlDocumentRef,
}

impl WebServiceAnalyzer {
    pub fn new(unit: UnitRef, document: MxmlDocumentRef) -> WebServiceAnalyzer {
        WebServiceAnalyzer { adapter: AnalyzerAdapter::new(unit), document }
    }
}

impl Analyzer for WebServiceAnalyzer {
    fn adapter(&self) -> &AnalyzerAdapter {
        &self.adapter
    }

    fn adapter_mut(&mut self) -> &mut AnalyzerAdapter {
        &mut self.adapter
    }

    fn get_document_version(&self) -> i32 {
        self.document.get_version()
    }

    fn get_language_namespace(&self) -> Option<String> {
        self.document.get_language_namespace()
    }

    fn analyze_operation_node(&mut self, node: &NodeRef) {
        if node.borrow().get_attribute_value("name").is_none() {
            self.adapter.log(node, CompilerMessage::error("Operation requires a name attribute."));
        }
        self.traverse(node);
    }

    fn analyze_request_node(&mut self, node: &NodeRef) {
        let (attr_count, first) = {
            let n = node.borrow();
            (n.get_attribute_count(), n.get_attribute_names().first().map(|q| q.get_local_part().to_string()))
        };
        if attr_count > 1 || (attr_count == 1 && first.as_deref() != Some("format")) {
            self.adapter.log(node, CompilerMessage::error("Request may only have 'format' attribute."));
        }
        self.traverse(node);
    }
}
