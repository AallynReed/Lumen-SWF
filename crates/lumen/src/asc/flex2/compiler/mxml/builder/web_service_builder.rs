//! Port of `flex2.compiler.mxml.builder.WebServiceBuilder`: the [`ComponentKind::WebService`] of a
//! [`ComponentBuilder`]; its `WebServiceChildNodeHandler` and `RequestChildNodeHandler` are the `in_operation` modes.

use super::abstract_builder::{Builder, BuilderContext};
use super::component_builder::{ComponentBuilder, ComponentChildNodeHandler, ComponentKind};
use super::service_request_builder::ServiceRequestBuilder;
use super::xml_builder::XmlBuilder;
use crate::asc::flex2::compiler::mxml::analyzer::web_service_analyzer::WebServiceAnalyzer;
use crate::asc::flex2::compiler::mxml::dom::node::{analyze, NodeRef};
use crate::asc::flex2::compiler::mxml::lang::child_node_handler::ChildNodeHandlerState;
use crate::asc::flex2::compiler::mxml::rep::model::{Model, ModelRef};
use crate::asc::flex2::compiler::toolkit;

const OPERATIONS: &str = "operations";
const WEB_SERVICE_OPERATION: &str = "WebServiceOperation";
const REQUEST: &str = "request";
const NAME: &str = "name";

#[derive(Default)]
pub struct WebServiceData {
    ops: Option<ModelRef>,
    /// Inside `addOperation`: the `RequestChildNodeHandler`, with its `requestCount`.
    in_operation: bool,
    request_count: i32,
}

fn data(b: &mut ComponentBuilder) -> &mut WebServiceData {
    match &mut b.kind {
        ComponentKind::WebService(d) => d,
        _ => unreachable!(),
    }
}

impl ComponentBuilder {
    /// `new WebServiceBuilder(unit, typeTable, mxmlConfiguration, document)`.
    pub fn new_web_service(ctx: &BuilderContext) -> ComponentBuilder {
        ComponentBuilder::with_kind(ctx, None, None, None, true, None, ComponentKind::WebService(WebServiceData::default()))
    }

    /// `WebServiceBuilder.analyze(WebServiceNode)`.
    pub(crate) fn web_service_analyze(&mut self, node: &NodeRef) {
        let mut a = WebServiceAnalyzer::new(self.ab.ctx.unit.clone(), self.ab.document.clone());
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
        let ops = Model::with_parent(&self.ab.document, self.ab.type_table.object_type(), Some(c.clone()), c.get_xml_line_number());
        ops.set_parent_index(OPERATIONS);
        c.set_property_model(OPERATIONS, &ops);
        data(self).ops = Some(ops);
        self.process_children(node, &t);
    }

    fn web_service_add_operation(&mut self, ops: &ModelRef, node: &NodeRef) {
        let ns = node.borrow().get_namespace().to_string();
        let t = self.ab.type_table.get_type_ns(&ns, WEB_SERVICE_OPERATION).expect("NullPointerException");
        let ws = self.component.clone();
        let is_fxg = self.cnh.is_fxg;
        let saved_cnh = std::mem::replace(&mut self.cnh, ChildNodeHandlerState::new(self.ab.type_table.clone(), is_fxg));
        let saved_nodes = std::mem::take(&mut self.default_property_nodes);
        let saved_mode = (data(self).in_operation, data(self).request_count);
        {
            let d = data(self);
            d.in_operation = true;
            d.request_count = 0;
        }
        let c = Model::with_parent(&self.ab.document, t.clone(), Some(ops.clone()), node.borrow().begin_line);
        self.component = Some(c.clone());
        let name = node.borrow().get_attribute_str(NAME).map(str::to_string).unwrap_or_else(|| "null".into());
        ops.set_property_model(&name, &c);
        c.set_parent_index(&name);
        self.process_attributes(node, &t);
        self.process_children(node, &t);
        self.cnh = saved_cnh;
        self.default_property_nodes = saved_nodes;
        let d = data(self);
        d.in_operation = saved_mode.0;
        d.request_count = saved_mode.1;
        self.component = ws;
    }

    fn web_service_add_request(&mut self, op: &ModelRef, node: &NodeRef) {
        let request = if node.borrow().is_xml_node() {
            let mut builder = XmlBuilder::new_with_parent(&self.ab.ctx, Some(op.clone()));
            analyze(node, &mut builder);
            builder.xml.clone().expect("xml")
        } else {
            let mut builder = ServiceRequestBuilder::new(&self.ab.ctx, REQUEST);
            analyze(node, &mut builder);
            builder.aog.graph()
        };
        op.set_property_model(REQUEST, &request);
        request.set_parent_index(REQUEST);
        request.set_parent(Some(op.clone()));
    }

    pub(crate) fn web_service_in_operation(&mut self) -> bool {
        data(self).in_operation
    }
}

impl ComponentChildNodeHandler<'_> {
    pub(crate) fn web_service_language_node(&mut self) {
        let child = self.b.cnh.child();
        let (in_operation, request_count) = {
            let d = data(self.b);
            (d.in_operation, d.request_count)
        };
        if !in_operation {
            if child.borrow().is_operation_node() {
                let ops = data(self.b).ops.clone().expect("ops");
                self.b.web_service_add_operation(&ops, &child);
            } else {
                self.component_language_node();
            }
        } else if child.borrow().is_request_node() && request_count == 0 {
            data(self.b).request_count = 1;
            let c = self.b.component();
            self.b.web_service_add_request(&c, &child);
        } else {
            self.component_language_node();
        }
    }

    /// `RequestChildNodeHandler.nestedDeclaration()`.
    pub(crate) fn web_service_nested_declaration(&mut self) {
        let child = self.b.cnh.child();
        if child.borrow().is_xml_node() && data(self.b).request_count == 0 {
            data(self.b).request_count = 1;
            let c = self.b.component();
            self.b.web_service_add_request(&c, &child);
        } else {
            self.component_language_node();
        }
    }
}
