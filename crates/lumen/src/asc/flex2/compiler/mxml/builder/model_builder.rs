//! Port of `flex2.compiler.mxml.builder.ModelBuilder`.

use super::abstract_builder::{attr_str, err, Builder, BuilderContext};
use super::anonymous_object_graph_builder::AnonymousObjectGraphBuilder;
use crate::asc::flex2::compiler::mxml::dom::analyzer::Analyzer;
use crate::asc::flex2::compiler::mxml::dom::analyzer_adapter::AnalyzerAdapter;
use crate::asc::flex2::compiler::mxml::dom::node::NodeRef;
use crate::asc::flex2::compiler::mxml::lang::standard_defs as sd;
use crate::asc::flex2::compiler::mxml::lang::text_parser::TextParser;
use crate::asc::flex2::compiler::mxml::rep::model::{Model, ModelRef};
use crate::asc::flex2::compiler::util::name_formatter;

pub struct ModelBuilder {
    pub aog: AnonymousObjectGraphBuilder,
    parent: Option<ModelRef>,
}

impl ModelBuilder {
    pub fn new(ctx: &BuilderContext, parent: Option<ModelRef>) -> ModelBuilder {
        let mut aog = AnonymousObjectGraphBuilder::new(ctx);
        aog.set_allow_two_way_bind(true);
        ModelBuilder { aog, parent }
    }
}

impl Analyzer for ModelBuilder {
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

    fn analyze_model_node(&mut self, node: &NodeRef) {
        let ab = &self.aog.ab;
        let class_object_proxy = name_formatter::to_dot(&ab.standard_defs.class_objectproxy);
        let line = node.borrow().begin_line;
        ab.document.add_import(&class_object_proxy, line);
        let binding_class = ab.type_table.get_type(&ab.standard_defs.class_objectproxy);
        if binding_class.is_none() {
            ab.log_node(node, err("The ${className} class could not be found.", &[("className", &class_object_proxy)]));
        }
        let graph = Model::new_anonymous_object_graph(&ab.document, binding_class.expect("NullPointerException"), line);
        self.aog.graph = Some(graph.clone());
        let top = self.parent.is_none();
        self.aog.register_model(node, &graph, top);
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
                match self.aog.ab.text_parser.parse_binding_expression_line(&image, cline) {
                    Some(be) => {
                        be.set_destination(&graph);
                        be.set_destination_object_proxy(true);
                    }
                    None => {
                        let id = attr_str(self.aog.get_language_attribute_value(node, sd::PROP_ID)).unwrap_or_else(|| "null".into());
                        self.aog.ab.log_node(&cdata, err("Model '${id}' contains only scalar content - this is currently unsupported.", &[("id", &id)]));
                    }
                }
            }
        } else if count == 1 {
            self.aog.process_children(&first.unwrap(), &graph);
        } else if count > 1 {
            self.aog.ab.log_node(node, err("Only one root tag is allowed.", &[]));
        }
    }
}
