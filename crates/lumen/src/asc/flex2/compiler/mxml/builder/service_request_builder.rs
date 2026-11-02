//! Port of `flex2.compiler.mxml.builder.ServiceRequestBuilder`.

use super::abstract_builder::{err, BuilderContext};
use super::anonymous_object_graph_builder::AnonymousObjectGraphBuilder;
use crate::asc::flex2::compiler::mxml::dom::analyzer::Analyzer;
use crate::asc::flex2::compiler::mxml::dom::analyzer_adapter::AnalyzerAdapter;
use crate::asc::flex2::compiler::mxml::dom::node::NodeRef;
use crate::asc::flex2::compiler::mxml::lang::text_parser::TextParser;
use crate::asc::flex2::compiler::mxml::rep::model::Model;

pub struct ServiceRequestBuilder {
    pub aog: AnonymousObjectGraphBuilder,
    request_name: String,
}

impl ServiceRequestBuilder {
    pub fn new(ctx: &BuilderContext, name: &str) -> ServiceRequestBuilder {
        let mut aog = AnonymousObjectGraphBuilder::new(ctx);
        aog.set_allow_two_way_bind(false);
        ServiceRequestBuilder { aog, request_name: name.to_string() }
    }

    /// `processRequest(node)`.
    pub fn process_request(&mut self, node: &NodeRef) {
        let graph = Model::new_anonymous_object_graph(&self.aog.ab.document, self.aog.ab.type_table.object_type(), node.borrow().begin_line);
        self.aog.graph = Some(graph.clone());
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
                match self.aog.ab.text_parser.parse_binding_expression_line(&image, line) {
                    Some(be) => {
                        if be.is_two_way_primary() {
                            self.aog.ab.log_node(&cdata, err("Inline two-way data binding expression not allowed here.", &[]));
                        } else {
                            be.set_destination(&graph);
                        }
                    }
                    None => self.aog.ab.log_node(&cdata, err("Model '${id}' contains only scalar content - this is currently unsupported.", &[("id", &self.request_name)])),
                }
            }
        } else {
            self.aog.process_children(node, &graph);
        }
    }
}

impl Analyzer for ServiceRequestBuilder {
    fn adapter(&self) -> &AnalyzerAdapter {
        &self.aog.ab.adapter
    }

    fn adapter_mut(&mut self) -> &mut AnalyzerAdapter {
        &mut self.aog.ab.adapter
    }

    fn get_document_version(&self) -> i32 {
        self.aog.ab.get_document_version()
    }

    fn get_language_namespace(&self) -> Option<String> {
        self.aog.ab.get_language_namespace()
    }

    fn analyze_arguments_node(&mut self, node: &NodeRef) {
        self.process_request(node);
    }

    fn analyze_request_node(&mut self, node: &NodeRef) {
        self.process_request(node);
    }
}
