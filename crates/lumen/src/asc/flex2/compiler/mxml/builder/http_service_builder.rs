//! Port of `flex2.compiler.mxml.builder.HTTPServiceBuilder`: the [`ComponentKind::HttpService`] of a
//! [`ComponentBuilder`], with its `HTTPServiceChildNodeHandler`.

use super::abstract_builder::{Builder, BuilderContext};
use super::component_builder::{ComponentBuilder, ComponentChildNodeHandler, ComponentKind};
use super::service_request_builder::ServiceRequestBuilder;
use crate::asc::flex2::compiler::mxml::analyzer::http_service_analyzer::HttpServiceAnalyzer;
use crate::asc::flex2::compiler::mxml::dom::node::{analyze, NodeRef};
use crate::asc::flex2::compiler::mxml::rep::model::Model;
use crate::asc::flex2::compiler::toolkit;

const REQUEST: &str = "request";

#[derive(Default)]
pub struct HttpServiceData {
    request_count: i32,
}

impl ComponentBuilder {
    /// `new HTTPServiceBuilder(unit, typeTable, mxmlConfiguration, document)`.
    pub fn new_http_service(ctx: &BuilderContext) -> ComponentBuilder {
        ComponentBuilder::with_kind(ctx, None, None, None, true, None, ComponentKind::HttpService(HttpServiceData::default()))
    }

    /// `HTTPServiceBuilder.analyze(HTTPServiceNode)`.
    pub(crate) fn http_service_analyze(&mut self, node: &NodeRef) {
        let mut a = HttpServiceAnalyzer::new(self.ab.ctx.unit.clone(), self.ab.document.clone());
        analyze(node, &mut a);
        if toolkit::error_count() > 0 {
            return;
        }
        let (ns, local, line) = {
            let n = node.borrow();
            (n.get_namespace().to_string(), n.get_local_part().to_string(), n.begin_line)
        };
        let t = self.ab.type_table.get_type_ns(&ns, &local).expect("NullPointerException");
        let c = Model::new(&self.ab.document, t.clone(), line);
        self.component = Some(c.clone());
        self.register_model(node, &c, true);
        self.process_attributes(node, &t);
        self.process_children(node, &t);
    }

    fn add_request(&mut self, child: &NodeRef) {
        let mut builder = ServiceRequestBuilder::new(&self.ab.ctx, REQUEST);
        analyze(child, &mut builder);
        let request = builder.aog.graph();
        let c = self.component();
        c.set_property_model(REQUEST, &request);
        request.set_parent_index(REQUEST);
        request.set_parent(Some(c));
    }
}

impl ComponentChildNodeHandler<'_> {
    /// `HTTPServiceChildNodeHandler.languageNode()`.
    pub(crate) fn http_service_language_node(&mut self) {
        let child = self.b.cnh.child();
        let first = matches!(&self.b.kind, ComponentKind::HttpService(d) if d.request_count == 0);
        if child.borrow().is_request_node() && first {
            if let ComponentKind::HttpService(d) = &mut self.b.kind {
                d.request_count = 1;
            }
            self.b.add_request(&child);
        } else {
            self.component_language_node();
        }
    }
}
