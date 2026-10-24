//! Port of `macromedia/asc/parser/MetaDataEvaluator.java`.

use std::cell::RefCell;
use std::rc::Rc;

use super::evaluator::{evaluate, Evaluator};
use super::node::*;
use crate::asc::semantics::slot::Slot;
use crate::asc::semantics::value::{KeyValuePair, KeylessValue, Value};
use crate::asc::util::context::{Context, ContextRef};
use crate::asc::util::errors::*;

pub const GO_TO_CTOR_DEFINITION_HELP: &str = "__go_to_ctor_definition_help";
pub const GO_TO_DEFINITION_HELP: &str = "__go_to_definition_help";

#[derive(Default)]
pub struct MetaDataEvaluator {
    pub doing_class: bool,
    add_go_to_definition_help_position: bool,
    add_go_to_definition_help_file: bool,
    current: Option<NodeRef>,
    pub doccomments: Vec<NodeRef>,
}

fn keyless(v: Option<Value>) -> Option<Rc<KeylessValue>> {
    match v {
        None => None,
        Some(Value::KeylessValue(k)) => Some(k),
        Some(_) => panic!("ClassCastException"),
    }
}

impl MetaDataEvaluator {
    pub fn new() -> MetaDataEvaluator {
        MetaDataEvaluator::default()
    }

    pub fn new_debug(debug: bool) -> MetaDataEvaluator {
        Self::new_help(debug, debug)
    }

    pub fn new_help(add_go_to_definition_help_position: bool, add_go_to_definition_help_file: bool) -> MetaDataEvaluator {
        MetaDataEvaluator { add_go_to_definition_help_position, add_go_to_definition_help_file, ..Default::default() }
    }

    fn eval(&mut self, cx: &ContextRef, node: &Option<NodeRef>) -> Option<Value> {
        evaluate(node.as_ref().expect("NullPointerException"), cx, self)
    }

    fn eval_items(&mut self, cx: &ContextRef, items: &[Option<NodeRef>]) -> Option<Value> {
        let mut val = None;
        for n in items {
            if n.is_some() {
                val = self.eval(cx, n);
            }
        }
        val
    }
}

impl Evaluator for MetaDataEvaluator {
    fn evaluate_node(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        cx.borrow().internal_error("error: undefined meta data method");
        None
    }

    fn evaluate_identifier(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let name = node.borrow().identifier().unwrap().name.clone();
        Some(Value::KeylessValue(Rc::new(KeylessValue::new(&name))))
    }

    fn evaluate_meta_data(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.current = Some(node.clone());

        let (pos, data) = {
            let n = node.borrow();
            (n.pos(), n.meta_data().unwrap().data.clone())
        };
        let elementlist = data.as_ref().and_then(|d| d.borrow().as_literal_array().expect("ClassCastException").elementlist.clone());
        let items = elementlist.as_ref().map(|e| e.borrow().as_argument_list().expect("ClassCastException").items.clone());

        match items {
            None => {}
            Some(items) if items.len() > 1 => {
                cx.borrow().error(pos, kError_MetaDataAttributesHasMoreThanOneElement);
            }
            Some(items) if items[0].as_ref().map_or(false, |i| i.borrow().as_member_expression().is_some()) => {
                let selector = items[0].as_ref().unwrap().borrow().as_member_expression().unwrap().selector.clone();

                if let Some(selector) = &selector {
                    let call = selector.borrow().as_call_expression().map(|c| (c.sel.expr.clone(), c.args.clone()));
                    if let Some((expr, args)) = call {
                        if expr.is_some() {
                            let val = keyless(self.eval(cx, &expr)).expect("NullPointerException");
                            self.current.as_ref().unwrap().borrow_mut().meta_data_mut().unwrap().set_id(Some(val.obj.clone()));
                        }

                        if args.is_some() {
                            let args_items = args.as_ref().unwrap().borrow().as_argument_list().unwrap().items.clone();
                            let mut values = Vec::with_capacity(args_items.len());
                            for n in &args_items {
                                let value = self.eval(cx, n);
                                match value {
                                    None => {
                                        let p = n.as_ref().unwrap().borrow().pos();
                                        cx.borrow().error(p, kError_InvalidMetaData);
                                    }
                                    Some(value) => values.push(value),
                                }
                            }
                            node.borrow_mut().meta_data_mut().unwrap().set_values(Some(values));
                        }
                    }

                    let get_expr = selector.borrow().as_get_expression().map(|g| g.sel.expr.clone());
                    if let Some(expr) = get_expr {
                        if expr.is_some() {
                            let value = keyless(self.eval(cx, &expr)).expect("NullPointerException");
                            self.current.as_ref().unwrap().borrow_mut().meta_data_mut().unwrap().set_id(Some(value.obj.clone()));
                        }
                    }
                }
                node.borrow_mut().meta_data_mut().unwrap().data = None;
            }
            Some(items) => {
                if let Some(first) = &items[0] {
                    let p = first.borrow().pos();
                    cx.borrow().error(p, kError_InvalidMetaData);
                } else {
                    cx.borrow().error(pos, kError_InvalidMetaData);
                }
            }
        }

        None
    }

    fn evaluate_doc_comment(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.evaluate_meta_data(cx, node);

        let mut add_node = true;
        let size = self.doccomments.len();
        if size > 0 {
            let dcn = self.doccomments[size - 1].clone();
            let (same_def, is_default) = {
                let d = dcn.borrow();
                let n = node.borrow();
                let same = match (&d.meta_data().unwrap().def, &n.meta_data().unwrap().def) {
                    (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                    (None, None) => true,
                    _ => false,
                };
                (same, n.as_doc_comment().unwrap().is_default)
            };
            if same_def {
                let dcn_meta_id = {
                    let d = dcn.borrow();
                    d.as_doc_comment().unwrap().meta_data.clone().map(|m| m.borrow().meta_data().expect("ClassCastException").get_id())
                };
                let keeps_previous = match &dcn_meta_id {
                    None => false,
                    Some(id) => matches!(
                        id.as_deref(),
                        Some("Event" | "Style" | "Effect" | "SkinState" | "Alternative" | "DiscouragedForProfile")
                    ),
                };
                if is_default && !keeps_previous {
                    add_node = false;
                } else {
                    self.doccomments.remove(size - 1);
                }
            }
        }
        if add_node {
            self.doccomments.push(node.clone());
        }

        None
    }

    fn evaluate_member_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let mut val = None;
        let (base, selector) = {
            let n = node.borrow();
            let m = n.as_member_expression().unwrap();
            (m.base.clone(), m.selector.clone())
        };
        if base.is_some() {
            self.eval(cx, &base);
        }
        if selector.is_some() {
            val = self.eval(cx, &selector);
        }

        val
    }

    fn evaluate_get_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = node.borrow().as_get_expression().unwrap().sel.expr.clone();
        if expr.is_some() {
            return self.eval(cx, &expr);
        }
        None
    }

    fn evaluate_set_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let mut name = None;
        let mut value = None;

        let (expr, args) = {
            let n = node.borrow();
            let s = n.as_set_expression().unwrap();
            (s.sel.expr.clone(), s.args.clone())
        };
        if expr.is_some() {
            let val = keyless(self.eval(cx, &expr));
            name = val.map(|v| v.obj.clone()).or(name);
        }
        if args.is_some() {
            let val = keyless(self.eval(cx, &args));
            value = val.map(|v| v.obj.clone()).or(value);
        }

        if let (Some(name), Some(value)) = (name, value) {
            return Some(Value::KeyValuePair(Rc::new(KeyValuePair::new(&name, &value))));
        }

        None
    }

    fn evaluate_argument_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let items = node.borrow().as_argument_list().unwrap().items.clone();
        self.eval_items(cx, &items)
    }

    fn evaluate_literal_boolean(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let value = node.borrow().as_literal_boolean().unwrap().value;
        Some(Value::KeylessValue(Rc::new(KeylessValue::new(if value { "true" } else { "false" }))))
    }

    fn evaluate_literal_number(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let value = node.borrow().as_literal_number().unwrap().value.clone();
        Some(Value::KeylessValue(Rc::new(KeylessValue::new(&value))))
    }

    fn evaluate_literal_string(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let value = node.borrow().as_literal_string().unwrap().value.clone();
        Some(Value::KeylessValue(Rc::new(KeylessValue::new(&value))))
    }

    fn evaluate_literal_null(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        Some(Value::KeylessValue(Rc::new(KeylessValue::new("null"))))
    }

    fn evaluate_literal_reg_exp(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let value = node.borrow().as_literal_reg_exp().unwrap().value.clone();
        Some(Value::KeylessValue(Rc::new(KeylessValue::new(&value))))
    }

    fn evaluate_qualified_identifier(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let name = node.borrow().identifier().unwrap().name.clone();
        Some(Value::KeylessValue(Rc::new(KeylessValue::new(&name))))
    }

    fn evaluate_literal_xml(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let list = node.borrow().as_literal_xml().unwrap().list.clone();
        if list.is_some() {
            return self.eval(cx, &list);
        }
        None
    }

    fn evaluate_unary_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = node.borrow().as_unary_expression().unwrap().expr.clone();
        if expr.is_some() {
            self.eval(cx, &expr);
        }
        None
    }

    fn evaluate_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let items = node.borrow().list().unwrap().items.clone();
        self.eval_items(cx, &items)
    }

    fn evaluate_statement_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let items = node.borrow().as_statement_list().unwrap().items.clone();
        self.eval_items(cx, &items);
        None
    }

    fn evaluate_expression_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = node.borrow().as_expression_statement().unwrap().expr.clone();
        if expr.is_some() {
            self.eval(cx, &expr);
        }
        None
    }

    fn evaluate_typed_identifier(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (identifier, type_) = {
            let n = node.borrow();
            let t = n.as_typed_identifier().unwrap();
            (t.identifier.clone(), t.type_.clone())
        };
        if identifier.is_some() {
            self.eval(cx, &identifier);
        }
        if type_.is_some() {
            self.eval(cx, &type_);
        }
        None
    }

    fn evaluate_program(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (pkgdefs, statements) = {
            let n = node.borrow();
            let p = n.program().unwrap();
            (p.pkgdefs.clone(), p.statements.clone())
        };
        for n in &pkgdefs {
            evaluate(n, cx, self);
        }
        if statements.is_some() {
            self.eval(cx, &statements);
        }
        None
    }

    fn evaluate_type_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = node.borrow().as_type_expression().unwrap().expr.clone();
        self.eval(cx, &expr)
    }

    fn evaluate_include_directive(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (in_this_include, node_cx, prev_cx) = {
            let n = node.borrow();
            let i = n.as_include_directive().unwrap();
            (i.in_this_include, i.cx.clone(), i.prev_cx.clone())
        };
        if !in_this_include {
            let statics = cx.borrow().statics.clone();
            let prev = Rc::new(RefCell::new(Context::new(statics)));
            prev.borrow_mut().switch_to_context(&cx.borrow());
            let node_cx = node_cx.expect("NullPointerException");
            if !Rc::ptr_eq(&node_cx, cx) {
                cx.borrow_mut().switch_to_context(&node_cx.borrow());
            }
            let mut n = node.borrow_mut();
            let i = n.as_include_directive_mut().unwrap();
            i.in_this_include = true;
            i.prev_cx = Some(prev);
        } else {
            let prev = prev_cx.expect("NullPointerException");
            cx.borrow_mut().switch_to_context(&prev.borrow());
            let mut n = node.borrow_mut();
            let i = n.as_include_directive_mut().unwrap();
            i.in_this_include = false;
            i.prev_cx = None;
        }
        None
    }

    fn evaluate_variable_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (attrs, list, meta_data) = {
            let n = node.borrow();
            let v = n.as_variable_definition().unwrap();
            (v.def.attrs.clone(), v.list.clone(), v.def.meta_data.clone())
        };
        if attrs.is_some() {
            self.eval(cx, &attrs);
        }
        if list.is_some() {
            self.eval(cx, &list);
        }

        if self.add_go_to_definition_help_position {
            self.add_position_metadata(cx, node);
        }

        if meta_data.is_some() {
            let items = list.as_ref().expect("NullPointerException").borrow().list().expect("ClassCastException").items.clone();
            for it in &items {
                let binding_ref = it.as_ref().and_then(|b| b.borrow().as_variable_binding().map(|b| b.ref_.clone()));
                if let Some(r) = binding_ref {
                    let slot = r.and_then(|r| r.get_slot(&cx.borrow()));
                    if let Some(slot) = slot {
                        if slot.get_metadata().is_none() {
                            for meta_node in Self::meta_nodes(&meta_data) {
                                self.add_metadata_to_slot(cx, &slot, &meta_node);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn evaluate_variable_binding(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (variable, initializer) = {
            let n = node.borrow();
            let b = n.as_variable_binding().unwrap();
            (b.variable.clone(), b.initializer.clone())
        };
        if variable.is_some() {
            self.eval(cx, &variable);
        }
        if Self::in_nullable_instance_scope(cx) {
            cx.borrow().scope().expect("NullPointerException").set_init_only(true);
        }
        if initializer.is_some() {
            self.eval(cx, &initializer);
        }
        if Self::in_nullable_instance_scope(cx) {
            cx.borrow().scope().expect("NullPointerException").set_init_only(false);
        }
        None
    }

    fn evaluate_function_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (fexpr, meta_data) = {
            let n = node.borrow();
            let f = n.function_definition().unwrap();
            (f.fexpr.clone(), f.def.meta_data.clone())
        };
        let (kind, ref_) = {
            let f = fexpr.as_ref().expect("NullPointerException").borrow();
            let f = f.as_function_common().expect("ClassCastException");
            (f.kind, f.ref_.clone())
        };
        let func_slot = ref_.and_then(|r| r.get_slot_kind(&cx.borrow(), kind));

        if self.add_go_to_definition_help_position {
            self.add_position_metadata(cx, node);
        }

        if let Some(func_slot) = func_slot {
            if func_slot.get_metadata().is_none() && meta_data.is_some() {
                for meta_node in Self::meta_nodes(&meta_data) {
                    self.add_metadata_to_slot(cx, &func_slot, &meta_node);
                    let i = Self::is_version_metadata(cx, &meta_node);
                    if i > -1 {
                        node.borrow_mut().function_definition_mut().unwrap().version = i;
                    }
                }
            }
        }
        None
    }

    fn evaluate_interface_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.evaluate_class_definition(cx, node)
    }

    fn evaluate_class_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if self.doing_class {
            return None;
        }
        self.doing_class = true;

        let (cframe, statements, clsdefs, iframe, instanceinits) = {
            let n = node.borrow();
            let c = n.class_definition().unwrap();
            (c.cframe.clone(), c.statements.clone(), c.clsdefs.clone(), c.iframe.clone(), c.instanceinits.clone())
        };
        cx.borrow().push_static_class_scopes(cframe.as_ref().expect("NullPointerException"));
        if statements.is_some() {
            self.eval(cx, &statements);
        }

        self.doing_class = false;
        let clsdefs = clsdefs.expect("NullPointerException").borrow().clone();
        for it in &clsdefs {
            evaluate(it, cx, self);
        }

        let iframe = iframe.expect("NullPointerException");
        cx.borrow().push_scope(iframe.clone());
        let instanceinits = instanceinits.expect("NullPointerException").borrow().clone();
        for init in &instanceinits {
            let es4_nullability = cx.borrow().statics.borrow().es4_nullability;
            if es4_nullability && !init.borrow().is_definition() {
                iframe.set_init_only(true);
            }
            evaluate(init, cx, self);
            if self.add_go_to_definition_help_position && init.borrow().as_function_definition().is_some() {
                let is_construct = {
                    let i = init.borrow();
                    let fexpr = &i.as_function_definition().unwrap().fexpr;
                    let f = fexpr.as_ref().expect("NullPointerException").borrow();
                    let r = f.as_function_common().expect("ClassCastException").ref_.clone();
                    r.map_or(false, |r| r.name() == "$construct")
                };
                if is_construct {
                    self.add_ctor_position_metadata(cx, node, init);
                }
            }
            if es4_nullability && !init.borrow().is_definition() {
                iframe.set_init_only(false);
            }
        }
        cx.borrow().pop_scope();
        cx.borrow().pop_static_class_scopes(cframe.as_ref());

        let (ref_, meta_data) = {
            let n = node.borrow();
            let c = n.class_definition().unwrap();
            (c.ref_.clone(), c.def.meta_data.clone())
        };
        let class_slot = ref_.and_then(|r| r.get_slot(&cx.borrow()));
        if self.add_go_to_definition_help_position {
            self.add_position_metadata(cx, node);
        }
        if let Some(class_slot) = class_slot {
            if class_slot.get_metadata().is_none() && meta_data.is_some() {
                for meta_node in Self::meta_nodes(&meta_data) {
                    self.add_metadata_to_slot(cx, &class_slot, &meta_node);
                    let i = Self::is_version_metadata(cx, &meta_node);
                    if i > -1 {
                        node.borrow_mut().class_definition_mut().unwrap().version = i;
                    }
                }
            }
        }
        None
    }
}

impl MetaDataEvaluator {
    /// The `MetaDataNode` items of a definition's `metaData` statement list.
    fn meta_nodes(meta_data: &Option<NodeRef>) -> Vec<NodeRef> {
        let m = meta_data.as_ref().expect("NullPointerException").borrow();
        let items = &m.as_statement_list().expect("ClassCastException").items;
        items.iter().flatten().filter(|n| n.borrow().meta_data().is_some()).cloned().collect()
    }

    fn in_nullable_instance_scope(cx: &ContextRef) -> bool {
        let cx = cx.borrow();
        cx.statics.borrow().es4_nullability
            && cx.scope().expect("NullPointerException").builder().map_or(false, |b| b.is_instance_builder())
    }

    fn add_metadata_to_slot(&mut self, cx: &ContextRef, slot: &Slot, metadata: &NodeRef) {
        if metadata.borrow().as_doc_comment().is_none() {
            if metadata.borrow().meta_data().unwrap().get_metadata().is_none() {
                evaluate(metadata, cx, self);
            }
            let md = metadata.borrow().meta_data().unwrap().get_metadata();
            slot.add_metadata(md);
            let i = Self::is_version_metadata(cx, metadata);
            if i != -1 {
                slot.set_version(i as i8);
            }
        }
    }

    fn is_version_metadata(cx: &ContextRef, metadata: &NodeRef) -> i32 {
        if cx.borrow().check_version() {
            let (id, values) = {
                let m = metadata.borrow();
                let m = m.meta_data().unwrap();
                (m.get_id(), m.get_values())
            };
            if id.as_deref() == Some("Version") && values.as_ref().map_or(false, |v| v.len() == 1) {
                if let Value::KeylessValue(k) = &values.unwrap()[0] {
                    return k.obj.parse::<i32>().unwrap_or(-1);
                }
            }
        }
        -1
    }

    fn make_position_metadata(&mut self, cx: &ContextRef, def: &NodeRef, is_ctor: bool) -> NodeRef {
        let mn = Context::get_node_factory(cx).borrow_mut().meta_data(None, -1);
        let pos = def.borrow().pos().to_string();
        let values = if self.add_go_to_definition_help_file {
            let origin = cx.borrow().get_error_origin();
            vec![
                Value::KeyValuePair(Rc::new(KeyValuePair::new("file", &origin))),
                Value::KeyValuePair(Rc::new(KeyValuePair::new("pos", &pos))),
            ]
        } else {
            vec![Value::KeyValuePair(Rc::new(KeyValuePair::new("pos", &pos)))]
        };
        {
            let mut m = mn.borrow_mut();
            let m = m.meta_data_mut().unwrap();
            m.set_id(Some(if is_ctor { GO_TO_CTOR_DEFINITION_HELP } else { GO_TO_DEFINITION_HELP }.to_string()));
            m.set_values(Some(values));
        }
        mn
    }

    fn add_ctor_position_metadata(&mut self, cx: &ContextRef, cdn: &NodeRef, ctor: &NodeRef) {
        let mn = self.make_position_metadata(cx, ctor, true);
        cdn.borrow_mut().definition_mut().unwrap().add_meta_data_node(Some(mn));
    }

    fn add_position_metadata(&mut self, cx: &ContextRef, def: &NodeRef) {
        let mn = self.make_position_metadata(cx, def, false);
        def.borrow_mut().definition_mut().unwrap().add_meta_data_node(Some(mn));
    }
}
