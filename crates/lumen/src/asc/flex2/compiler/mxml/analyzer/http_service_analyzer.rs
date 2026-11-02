//! Port of `flex2.compiler.mxml.analyzer.HTTPServiceAnalyzer`.

use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::mxml::dom::analyzer::Analyzer;
use crate::asc::flex2::compiler::mxml::dom::analyzer_adapter::AnalyzerAdapter;
use crate::asc::flex2::compiler::mxml::dom::node::NodeRef;
use crate::asc::flex2::compiler::mxml::rep::mxml_document::MxmlDocumentRef;
use crate::asc::flex2::compiler::toolkit::CompilerMessage;

pub struct HttpServiceAnalyzer {
    adapter: AnalyzerAdapter,
    document: MxmlDocumentRef,
}

impl HttpServiceAnalyzer {
    pub fn new(unit: UnitRef, document: MxmlDocumentRef) -> HttpServiceAnalyzer {
        HttpServiceAnalyzer { adapter: AnalyzerAdapter::new(unit), document }
    }
}

impl Analyzer for HttpServiceAnalyzer {
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

    fn analyze_request_node(&mut self, node: &NodeRef) {
        if node.borrow().get_attribute_count() > 0 {
            self.adapter.log(node, CompilerMessage::error("Request may not have attributes."));
        }
        self.traverse(node);
    }
}
