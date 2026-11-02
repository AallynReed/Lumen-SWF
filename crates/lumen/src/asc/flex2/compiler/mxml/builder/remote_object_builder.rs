//! Port of `flex2.compiler.mxml.builder.RemoteObjectBuilder`: the [`ComponentKind::RemoteObject`] of a
//! [`ComponentBuilder`]; its `RemoteObjectChildNodeHandler` and `MethodChildNodeHandler` are the `in_method` modes.

use super::abstract_builder::{Builder, BuilderContext};
use super::component_builder::{ComponentBuilder, ComponentChildNodeHandler, ComponentKind};
use super::service_request_builder::ServiceRequestBuilder;
use crate::asc::flex2::compiler::mxml::analyzer::remote_object_analyzer::RemoteObjectAnalyzer;
use crate::asc::flex2::compiler::mxml::dom::node::{analyze, NodeRef};
use crate::asc::flex2::compiler::mxml::lang::child_node_handler::ChildNodeHandlerState;
use crate::asc::flex2::compiler::mxml::rep::model::{Model, ModelRef, RValue};
use crate::asc::flex2::compiler::toolkit;

const OPERATIONS: &str = "operations";
const NAME: &str = "name";
const ARGUMENTS: &str = "arguments";
const ARGUMENT_NAMES: &str = "argumentNames";

#[derive(Default)]
pub struct RemoteObjectData {
    ops: Option<ModelRef>,
    /// Inside `addOperation`: the `MethodChildNodeHandler`, with its `argumentsCount`.
    in_method: bool,
    arguments_count: i32,
}

fn data(b: &mut ComponentBuilder) -> &mut RemoteObjectData {
    match &mut b.kind {
        ComponentKind::RemoteObject(d) => d,
        _ => unreachable!(),
    }
}

impl ComponentBuilder {
    /// `new RemoteObjectBuilder(unit, typeTable, mxmlConfiguration, document)`.
    pub fn new_remote_object(ctx: &BuilderContext) -> ComponentBuilder {
        ComponentBuilder::with_kind(ctx, None, None, None, true, None, ComponentKind::RemoteObject(RemoteObjectData::default()))
    }

    /// `RemoteObjectBuilder.analyze(RemoteObjectNode)`.
    pub(crate) fn remote_object_analyze(&mut self, node: &NodeRef) {
        let mut a = RemoteObjectAnalyzer::new(self.ab.ctx.unit.clone(), self.ab.document.clone());
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

    /// `addOperation(ops, node)`.
    fn remote_object_add_operation(&mut self, ops: &ModelRef, node: &NodeRef) {
        let ns = node.borrow().get_namespace().to_string();
        let tag = self.ab.standard_defs.get_converted_tag_name(node);
        let t = self.ab.type_table.get_type_ns(&ns, &tag).expect("NullPointerException");
        let ro = self.component.clone();
        let is_fxg = self.cnh.is_fxg;
        let saved_cnh = std::mem::replace(&mut self.cnh, ChildNodeHandlerState::new(self.ab.type_table.clone(), is_fxg));
        let saved_nodes = std::mem::take(&mut self.default_property_nodes);
        let saved_mode = (data(self).in_method, data(self).arguments_count);
        *data(self) = RemoteObjectData { ops: data(self).ops.clone(), in_method: true, arguments_count: 0 };
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
        d.in_method = saved_mode.0;
        d.arguments_count = saved_mode.1;
        self.component = ro;
    }

    /// `addArguments(op, node)`.
    fn add_arguments(&mut self, op: &ModelRef, node: &NodeRef) {
        let mut builder = ServiceRequestBuilder::new(&self.ab.ctx, ARGUMENTS);
        analyze(node, &mut builder);
        let arguments = builder.aog.graph();
        op.set_property_model(ARGUMENTS, &arguments);
        arguments.set_parent_index(ARGUMENTS);
        arguments.set_parent(Some(op.clone()));
        let arg_names = Model::new_array(&self.ab.document, Some(op.clone()), node.borrow().begin_line, Some(self.ab.type_table.object_type()));
        op.set_property_model(ARGUMENT_NAMES, &arg_names);
        arg_names.set_parent_index(ARGUMENT_NAMES);
        for (s, _) in arguments.get_properties() {
            arg_names.add_entry(RValue::Str(s), op.get_xml_line_number());
        }
    }
}

impl ComponentChildNodeHandler<'_> {
    /// `RemoteObjectChildNodeHandler` / `MethodChildNodeHandler.languageNode()`, and `WebService`'s.
    pub(crate) fn service_language_node(&mut self) {
        if matches!(self.b.kind, ComponentKind::WebService(_)) {
            self.web_service_language_node();
            return;
        }
        let child = self.b.cnh.child();
        let (in_method, arguments_count) = {
            let d = data(self.b);
            (d.in_method, d.arguments_count)
        };
        if !in_method {
            if child.borrow().is_method_node() {
                let ops = data(self.b).ops.clone().expect("ops");
                self.b.remote_object_add_operation(&ops, &child);
            } else {
                self.component_language_node();
            }
        } else if child.borrow().is_arguments_node() && arguments_count == 0 {
            data(self.b).arguments_count = 1;
            let c = self.b.component();
            self.b.add_arguments(&c, &child);
        } else {
            self.component_language_node();
        }
    }
}
