//! Port of `macromedia/asc/semantics/ConfigurationEvaluator.java`: the first pass after parsing, which defines the
//! config namespaces and constants and folds `CONFIG::name` references and conditional definitions.

#![allow(non_upper_case_globals)]

use std::collections::HashSet;
use std::panic::{self, AssertUnwindSafe};
use std::rc::Rc;

use crate::asc::emit::runtime_constants::*;
use crate::asc::parser::evaluator::{evaluate, Evaluator};
use crate::asc::parser::node::*;
use crate::asc::parser::tokens::*;
use crate::asc::semantics::configuration_builder::new_configuration_builder;
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::reference_value::ReferenceValue;
use crate::asc::semantics::type_value::TypeValue;
use crate::asc::semantics::value::Value;
use crate::asc::util::context::{Context, ContextRef, NS_INTERNAL};
use crate::asc::util::errors::*;
use crate::asc::util::namespaces::Namespaces;
use crate::asc::util::number_constant::{java_d2i, java_d2l, NumberConstant};
use crate::asc::util::number_usage::NumberUsage;
use crate::xfl::num::java_double;

const UNDEFINED: &str = "undefined";
const TRUE: &str = "true";
const FALSE: &str = "false";

#[derive(Default)]
pub struct ConfigurationEvaluator {
    fold_expressions: bool,
    top_level: bool,
    config_namespaces: HashSet<String>,
}

type Field = fn(&mut Node) -> &mut Option<NodeRef>;

fn same_node(a: &Option<NodeRef>, b: &Option<NodeRef>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

fn same_type(a: &Option<TypeValue>, b: &TypeValue) -> bool {
    a.as_ref().map_or(false, |a| a.ptr_eq(b))
}

/// `typeval == cx.xType()` for an `ObjectValue`'s type value.
fn type_value_of(cx: &ContextRef, obj: &ObjectValue) -> Option<TypeValue> {
    obj.get_type(&cx.borrow()).and_then(|ti| ti.get_type_value())
}

/// `cx.getEmitter().getValueOfNumberLiteral(...)`.
fn number_literal(cx: &ContextRef, s: &str, type_: &mut Option<TypeValue>, usage: Option<&NumberUsage>) -> Option<NumberConstant> {
    let emitter = cx.borrow().get_emitter().expect("NullPointerException");
    let r = emitter.borrow_mut().get_value_of_number_literal(s, type_, usage);
    r
}

fn number_double(cx: &ContextRef, s: &str, usage: Option<&NumberUsage>) -> f64 {
    let mut type_ = None;
    number_literal(cx, s, &mut type_, usage).expect("NullPointerException").double_value()
}

fn kind_is(n: &Option<NodeRef>, f: fn(&Node) -> bool) -> bool {
    n.as_ref().map_or(false, |n| f(&n.borrow()))
}

impl ConfigurationEvaluator {
    pub fn new() -> ConfigurationEvaluator {
        ConfigurationEvaluator::default()
    }

    fn eval(&mut self, cx: &ContextRef, n: &Option<NodeRef>) -> Option<Value> {
        evaluate(n.as_ref().expect("NullPointerException"), cx, self)
    }

    fn fold_field(&mut self, cx: &ContextRef, node: &NodeRef, field: Field) {
        let n = field(&mut node.borrow_mut()).clone();
        let r = self.eval_and_fold(cx, n);
        *field(&mut node.borrow_mut()) = r;
    }

    fn get_boolean_object_value(&self, cx: &ContextRef, b: bool) -> ObjectValue {
        if b {
            cx.borrow().boolean_true()
        } else {
            cx.borrow().boolean_false()
        }
    }

    fn eval_array_or_object_arg_list(&mut self, cx: &ContextRef, list: &Option<NodeRef>) {
        let Some(list) = list else { return };
        let size = list.borrow().as_argument_list().expect("ClassCastException").items.len() as i32;
        let mut i = size - 1;
        while i > 0 {
            let n = list.borrow().as_argument_list().unwrap().items[i as usize].clone();
            let l = n.filter(|n| n.borrow().list().is_some());
            if let Some(l) = l {
                let (first, second, is_pair) = {
                    let b = l.borrow();
                    let items = &b.list().unwrap().items;
                    (items.first().cloned().flatten(), items.get(1).cloned().flatten(), items.len() == 2)
                };
                if is_pair && first.as_ref().expect("NullPointerException").borrow().is_configuration_name() {
                    let config_val = self.eval(cx, &first);

                    if let Some(Value::Reference(r)) = config_val.filter(|v| v.is_reference()) {
                        if r.is_config_ref() {
                            let v = r.get_value(&cx.borrow());
                            if v.is_none() {
                                cx.borrow().error1(r.get_position(), kError_UnfoundProperty, &r.name());
                            } else {
                                let ov = r.get_value(&cx.borrow()).unwrap().as_object_value().expect("ClassCastException");
                                let b = self.to_boolean(cx, &ov);
                                let mut lb = list.borrow_mut();
                                let items = &mut lb.as_argument_list_mut().unwrap().items;
                                if b == Some(true) {
                                    items[i as usize] = second;
                                } else {
                                    items.remove(i as usize);
                                }
                            }
                        }
                    }
                }
            }
            i -= 1;
        }
    }

    fn to_boolean(&mut self, cx: &ContextRef, obj: &ObjectValue) -> Option<bool> {
        let mut ret = None;
        let tv = type_value_of(cx, obj);
        if let Some(tv) = &tv {
            let (boolean_type, string_type) = (cx.borrow().boolean_type(), cx.borrow().string_type());
            if tv.ptr_eq(&boolean_type) {
                ret = Some(obj.boolean_value());
            } else if tv.ptr_eq(&string_type) {
                ret = Some(!obj.get_value().is_empty());
            } else if self.is_numeric_type(cx, Some(tv)) {
                let d = number_double(cx, &obj.get_value(), obj.get_number_usage().as_ref());
                ret = Some(!(d.is_nan() || d == 0.0));
            }
        }
        ret
    }

    fn to_string(&mut self, cx: &ContextRef, obj: &ObjectValue) -> Option<String> {
        let mut ret = None;
        let tv = type_value_of(cx, obj);
        if let Some(tv) = &tv {
            let (boolean_type, string_type) = (cx.borrow().boolean_type(), cx.borrow().string_type());
            if tv.ptr_eq(&boolean_type) {
                ret = Some(if obj.boolean_value() { "true" } else { "false" }.to_string());
            } else if tv.ptr_eq(&string_type) {
                ret = Some(obj.get_value());
            } else if self.is_numeric_type(cx, Some(tv)) {
                let mut type_ = None;
                let v = number_literal(cx, &obj.get_value(), &mut type_, obj.get_number_usage().as_ref());
                ret = Some(v.expect("NullPointerException").to_string());
            }
        }
        ret
    }

    fn to_number(&mut self, cx: &ContextRef, obj: &ObjectValue) -> Option<f64> {
        let mut ret = None;
        let tv = type_value_of(cx, obj);
        if let Some(tv) = &tv {
            let (boolean_type, string_type) = (cx.borrow().boolean_type(), cx.borrow().string_type());
            if tv.ptr_eq(&boolean_type) {
                ret = Some(if obj.boolean_value() { 1.0 } else { 0.0 });
            } else if tv.ptr_eq(&string_type) || self.is_numeric_type(cx, Some(tv)) {
                let (value, usage) = (obj.get_value(), obj.get_number_usage());
                match panic::catch_unwind(AssertUnwindSafe(|| number_double(cx, &value, usage.as_ref()))) {
                    Ok(d) => ret = Some(d),
                    Err(e) => {
                        let msg = e.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| e.downcast_ref::<String>().cloned());
                        if !msg.map_or(false, |m| m.starts_with("NumberFormatException")) {
                            panic::resume_unwind(e);
                        }
                    }
                }
            }
        }
        ret
    }

    fn to_int(&mut self, cx: &ContextRef, obj: &ObjectValue) -> Option<i32> {
        let mut i = None;
        if let Some(d) = self.to_number(cx, obj) {
            if d.is_infinite() || d.is_nan() || d == 0.0 {
                i = Some(0);
            } else {
                i = Some(java_d2i(d));
            }
        }
        i
    }

    fn to_uint(&mut self, cx: &ContextRef, obj: &ObjectValue) -> Option<i64> {
        let mut l = None;
        if let Some(d) = self.to_number(cx, obj) {
            if d.is_infinite() || d.is_nan() || d == 0.0 {
                l = Some(0);
            } else {
                l = Some(java_d2l(d));
            }
        }
        l
    }

    fn lessthan(&mut self, cx: &ContextRef, lhs: &ObjectValue, rhs: &ObjectValue) -> Option<&'static str> {
        let ld = self.to_number(cx, lhs);
        let rd = self.to_number(cx, rhs);

        if let (Some(ld), Some(rd)) = (ld, rd) {
            if ld.is_nan() || rd.is_nan() {
                return Some(UNDEFINED);
            }

            if ld == rd {
                return Some(FALSE);
            }

            if ld.is_infinite() {
                return Some(if ld > 0.0 { FALSE } else { TRUE });
            }

            if rd.is_infinite() {
                return Some(if rd < 0.0 { FALSE } else { TRUE });
            }

            return Some(if ld < rd { TRUE } else { FALSE });
        }
        None
    }

    fn compare(&mut self, cx: &ContextRef, lhs: &ObjectValue, rhs: &ObjectValue) -> bool {
        if lhs.has_value() && rhs.has_value() {
            if lhs.ptr_eq(rhs) {
                return true;
            }

            let rhs_type = rhs.get_type(&cx.borrow()).expect("NullPointerException").get_type_value();
            let lhs_type = lhs.get_type(&cx.borrow()).expect("NullPointerException").get_type_value();
            let (boolean_type, string_type) = (cx.borrow().boolean_type(), cx.borrow().string_type());

            let same = match (&lhs_type, &rhs_type) {
                (Some(a), Some(b)) => a.ptr_eq(b),
                (None, None) => true,
                _ => false,
            };
            if same {
                if same_type(&lhs_type, &string_type) {
                    return Some(lhs.get_value()) == self.to_string(cx, rhs);
                } else if same_type(&lhs_type, &boolean_type) {
                    let b1 = self.to_boolean(cx, lhs).expect("NullPointerException");
                    let b2 = self.to_boolean(cx, rhs).expect("NullPointerException");
                    return b1 == b2;
                } else if self.is_numeric_type(cx, lhs_type.as_ref()) {
                    let d1 = self.to_number(cx, lhs).expect("NullPointerException");
                    let d2 = self.to_number(cx, rhs).expect("NullPointerException");
                    return d1 == d2;
                }
            } else {
                let ln = self.is_numeric_type(cx, lhs_type.as_ref());
                let rn = self.is_numeric_type(cx, rhs_type.as_ref());
                if (ln && rn)
                    || (ln && same_type(&rhs_type, &string_type))
                    || (same_type(&lhs_type, &string_type) && rn)
                    || same_type(&lhs_type, &boolean_type)
                    || same_type(&rhs_type, &boolean_type)
                {
                    let d1 = self.to_number(cx, lhs);
                    let d2 = self.to_number(cx, rhs);
                    return matches!((d1, d2), (Some(d1), Some(d2)) if d1 == d2);
                }
            }
        }
        false
    }

    #[allow(dead_code)]
    fn number_obj_val(&self, cx: &ContextRef, d: f64) -> ObjectValue {
        ObjectValue::with_value(&java_double(d), Some(&cx.borrow().number_type()))
    }

    fn is_numeric_type(&self, cx: &ContextRef, typeval: Option<&TypeValue>) -> bool {
        let Some(t) = typeval else { return false };
        let c = cx.borrow();
        t.ptr_eq(&c.number_type()) || t.ptr_eq(&c.int_type()) || t.ptr_eq(&c.uint_type())
    }

    fn remove_meta_data(&mut self, cx: &ContextRef, def: &NodeRef, list: &NodeRef, def_index: usize) {
        if def.borrow().definition().unwrap().meta_data.is_some() {
            let nf = Context::get_node_factory(cx);
            let mut i = def_index as i32 - 1;
            while i >= 0 {
                let temp = list.borrow().as_statement_list().unwrap().items[i as usize].clone();
                let meta = temp.clone().filter(|t| t.borrow().meta_data().is_some());
                let is_meta_of_def = meta.as_ref().map_or(false, |m| same_node(&m.borrow().meta_data().unwrap().def, &Some(def.clone())));
                if is_meta_of_def {
                    let e = nf.borrow_mut().empty_statement();
                    list.borrow_mut().as_statement_list_mut().unwrap().items[i as usize] = Some(e);
                    meta.unwrap().borrow_mut().meta_data_mut().unwrap().def = None;
                } else if !kind_is(&temp, |t| t.as_include_directive().is_some()) {
                    break;
                }
                i -= 1;
            }
            def.borrow_mut().definition_mut().unwrap().meta_data = None;
        }
    }

    fn find_next_definition(&mut self, _cx: &ContextRef, list: &NodeRef, start: usize) -> Option<NodeRef> {
        let mut def = None;
        let items = list.borrow().as_statement_list().unwrap().items.clone();
        for n in items.iter().skip(start) {
            if kind_is(n, |t| t.meta_data().is_some() || t.as_include_directive().is_some()) {
                continue;
            }
            if kind_is(n, |t| t.definition().is_some()) {
                def = n.clone();
            }
            break;
        }
        def
    }

    fn starts_with_definition(&mut self, cx: &ContextRef, list: &NodeRef) -> Option<NodeRef> {
        self.find_next_definition(cx, list, 0)
    }

    fn eval_and_fold(&mut self, cx: &ContextRef, n: Option<NodeRef>) -> Option<NodeRef> {
        let mut ret = n.clone();
        if let Some(n) = &n {
            let val = evaluate(n, cx, self);
            ret = self.fold_ref_value(cx, val);
            if ret.is_none() {
                ret = Some(n.clone());
            }
        }
        ret
    }

    fn fold_ref_value(&mut self, cx: &ContextRef, val: Option<Value>) -> Option<NodeRef> {
        let mut ret = None;

        if let Some(Value::Reference(ref_val)) = val {
            if ref_val.is_config_ref() {
                let v = ref_val.get_value(&cx.borrow());
                match v {
                    None => cx.borrow().error1(ref_val.get_position(), kError_UnfoundProperty, &ref_val.name()),
                    Some(v) => ret = self.literal_from_value(cx, &v),
                }
                if let Some(r) = &ret {
                    evaluate(r, cx, self);
                }
            }
        }

        ret
    }

    fn literal_from_value(&mut self, cx: &ContextRef, val: &Value) -> Option<NodeRef> {
        let mut ret = None;

        if let Some(obj_val) = val.as_object_value() {
            let type_id = obj_val.get_type(&cx.borrow()).expect("NullPointerException").get_type_id();
            let nf = Context::get_node_factory(cx);
            let mut nf = nf.borrow_mut();
            let literal_node = match type_id {
                TYPE_string => Some(nf.literal_string(&obj_val.get_value())),
                TYPE_boolean => Some(nf.literal_boolean(obj_val.boolean_value())),
                TYPE_number | TYPE_int | TYPE_uint_external | TYPE_decimal | TYPE_double => {
                    Some(nf.literal_number(&obj_val.get_value()))
                }
                _ => None,
            };
            ret = literal_node;
        }
        ret
    }

    fn is_top_level(&self) -> bool {
        self.top_level
    }

    /// The `Builder.Variable`/`ExplicitVar` definition of `name` in the current config scope, or `None` when one
    /// already exists.
    fn define_config_slot(&self, cx: &ContextRef, name: &str, namespaces: &Namespaces) -> Option<crate::asc::semantics::slot::Slot> {
        let c = cx.borrow();
        let obj = c.scope().expect("NullPointerException");
        let bui = obj.builder().expect("NullPointerException");
        let has_namespaces = obj.has_names(&c, GET_TOKEN, name, namespaces);
        if has_namespaces.is_some() {
            return None;
        }
        let var_id = bui.variable(&c, &obj);
        let slot_id = bui.explicit_var_nss(&c, &obj, name, namespaces, Some(&c.no_type()), -1, -1, var_id);
        Some(obj.get_slot(&c, slot_id).expect("NullPointerException"))
    }

    fn push_config_scope(&self, cx: &ContextRef) {
        let c = cx.borrow();
        let scope = ObjectValue::with_builder(&c, new_configuration_builder(), None);
        c.push_scope(scope);
    }
}

impl Evaluator for ConfigurationEvaluator {
    fn evaluate_increment(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.selector_mut().unwrap().expr);
        None
    }

    fn evaluate_delete_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.selector_mut().unwrap().expr);
        None
    }

    fn evaluate_identifier(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let name = node.borrow().identifier().unwrap().name.clone();
        let c = cx.borrow();
        Some(Value::Reference(ReferenceValue::new(&c, None, &name, Some(&c.public_namespace()))))
    }

    fn evaluate_invoke(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let args = node.borrow().as_invoke().unwrap().args.clone();
        self.eval(cx, &args);
        self.fold_field(cx, node, |n| &mut n.selector_mut().unwrap().expr);
        None
    }

    fn evaluate_qualified_identifier(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let mut result = None;

        let (qualifier, name, node_pos) = {
            let n = node.borrow();
            let q = n.qualified_identifier().unwrap();
            (q.qualifier.clone(), q.id.name.clone(), n.get_position())
        };
        let mut val = if qualifier.is_some() { self.eval(cx, &qualifier) } else { None };
        let attr_items = qualifier.as_ref().and_then(|q| q.borrow().as_attribute_list().map(|a| a.items.clone()));
        if let Some(items) = attr_items.filter(|items| items.len() == 1) {
            val = self.eval(cx, &items[0]);
            if let Some(Value::Reference(r)) = &val {
                let s = r.get_slot(&cx.borrow());
                if let Some(ov) = s.and_then(|s| s.get_object_value()).filter(|o| o.is_config_ns()) {
                    let temp = ReferenceValue::new(&cx.borrow(), None, &name, Some(&ov));
                    temp.set_position(qualifier.as_ref().unwrap().borrow().get_position());
                    result = Some(Value::Reference(temp));
                }
            }
        } else if let Some(Value::Reference(r)) = &val {
            let s = r.get_slot(&cx.borrow());
            if let Some(ov) = s.and_then(|s| s.get_object_value()).filter(|o| o.is_config_ns()) {
                let temp = ReferenceValue::new(&cx.borrow(), None, &name, Some(&ov));
                temp.set_position(node_pos);
                result = Some(Value::Reference(temp));
            }
        }
        result
    }

    fn evaluate_literal_boolean(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let value = node.borrow().as_literal_boolean().unwrap().value;
        Some(Value::Object(self.get_boolean_object_value(cx, value)))
    }

    fn evaluate_literal_number(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let mut type_ = None;
        let (value, usage) = {
            let n = node.borrow();
            let l = n.as_literal_number().unwrap();
            (l.value.clone(), l.number_usage.clone())
        };
        let numeric_value = number_literal(cx, &value, &mut type_, usage.as_ref());
        let mut n = node.borrow_mut();
        let l = n.as_literal_number_mut().unwrap();
        l.numeric_value = numeric_value;
        l.type_ = type_.clone();
        Some(Value::Object(ObjectValue::with_value(&value, type_.as_ref())))
    }

    fn evaluate_literal_string(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let value = node.borrow().as_literal_string().unwrap().value.clone();
        Some(Value::Object(ObjectValue::with_value(&value, Some(&cx.borrow().string_type()))))
    }

    fn evaluate_function_common(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (signature, body) = {
            let n = node.borrow();
            let f = n.as_function_common().unwrap();
            (f.signature.clone(), f.body.clone())
        };
        self.eval(cx, &signature);

        self.push_config_scope(cx);

        self.eval(cx, &body);

        cx.borrow().pop_scope();

        None
    }

    fn evaluate_paren_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.as_paren_expression_mut().unwrap().expr);
        None
    }

    fn evaluate_paren_list_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.as_paren_list_expression_mut().unwrap().expr);
        None
    }

    fn evaluate_literal_object(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let fieldlist = node.borrow().as_literal_object().unwrap().fieldlist.clone();
        if fieldlist.is_some() {
            self.eval_array_or_object_arg_list(cx, &fieldlist);
            self.eval(cx, &fieldlist);
        }
        None
    }

    fn evaluate_literal_field(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.as_literal_field_mut().unwrap().name);
        self.fold_field(cx, node, |n| &mut n.as_literal_field_mut().unwrap().value);
        None
    }

    fn evaluate_literal_array(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let elementlist = node.borrow().as_literal_array().expect("ClassCastException").elementlist.clone();
        if elementlist.is_some() {
            self.eval_array_or_object_arg_list(cx, &elementlist);
            self.eval(cx, &elementlist);
        }
        None
    }

    fn evaluate_literal_vector(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (type_, elementlist) = {
            let n = node.borrow();
            let v = n.as_literal_vector().unwrap();
            (v.type_.clone(), v.elementlist.clone())
        };
        self.eval(cx, &type_);
        if elementlist.is_some() {
            self.eval_array_or_object_arg_list(cx, &elementlist);
            self.eval(cx, &elementlist);
        }
        None
    }

    fn evaluate_super_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.as_super_expression_mut().unwrap().expr);
        None
    }

    fn evaluate_super_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let call = node.borrow().as_super_statement().unwrap().call.clone();
        self.eval(cx, &call);
        None
    }

    fn evaluate_member_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if node.borrow().as_member_expression().unwrap().base.is_some() {
            self.fold_field(cx, node, |n| &mut n.as_member_expression_mut().unwrap().base);
        }
        let selector = node.borrow().as_member_expression().unwrap().selector.clone();
        self.eval(cx, &selector)
    }

    fn evaluate_call_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let args = node.borrow().as_call_expression().unwrap().args.clone();
        if args.is_some() {
            self.eval(cx, &args);
        }
        None
    }

    fn evaluate_get_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = node.borrow().selector().unwrap().expr.clone();
        self.eval(cx, &expr)
    }

    fn evaluate_set_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let args = node.borrow().as_set_expression().unwrap().args.clone();
        self.eval(cx, &args);
        None
    }

    fn evaluate_unary_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let mut val = None;
        if self.fold_expressions {
            let expr = node.borrow().as_unary_expression().unwrap().expr.clone();
            let mut expr_val = self.eval(cx, &expr);

            let new_expr = self.fold_ref_value(cx, expr_val.clone());
            if new_expr.is_some() {
                node.borrow_mut().as_unary_expression_mut().unwrap().expr = new_expr.clone();
                expr_val = self.eval(cx, &new_expr);
            }

            if let Some(expr_ov) = expr_val.filter(|v| v.has_value()).and_then(|v| v.as_object_value()) {
                let op = node.borrow().as_unary_expression().unwrap().op;
                if op == NOT_TOKEN {
                    if let Some(b) = self.to_boolean(cx, &expr_ov) {
                        val = Some(Value::Object(self.get_boolean_object_value(cx, !b)));
                    }
                }
            }
        } else {
            self.fold_field(cx, node, |n| &mut n.as_unary_expression_mut().unwrap().expr);
        }
        val
    }

    fn evaluate_binary_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let mut val = None;
        if self.fold_expressions {
            let (lhs, rhs) = {
                let n = node.borrow();
                let b = n.as_binary_expression().unwrap();
                (b.lhs.clone(), b.rhs.clone())
            };
            let mut lhs_val = self.eval(cx, &lhs);
            let mut rhs_val = self.eval(cx, &rhs);

            let new_lhs = self.fold_ref_value(cx, lhs_val.clone());
            let new_rhs = self.fold_ref_value(cx, rhs_val.clone());
            if new_lhs.is_some() {
                node.borrow_mut().as_binary_expression_mut().unwrap().lhs = new_lhs.clone();
                lhs_val = self.eval(cx, &new_lhs);
            }
            if new_rhs.is_some() {
                node.borrow_mut().as_binary_expression_mut().unwrap().rhs = new_rhs.clone();
                rhs_val = self.eval(cx, &new_rhs);
            }

            if let (Some(lhs_val), Some(rhs_val)) = (lhs_val, rhs_val) {
                if lhs_val.has_value() && rhs_val.has_value() {
                    let lhs_ov = lhs_val.as_object_value().expect("ClassCastException");
                    let rhs_ov = rhs_val.as_object_value().expect("ClassCastException");

                    let lt = lhs_ov.get_type(&cx.borrow()).expect("NullPointerException").get_type_value();
                    let rt = rhs_ov.get_type(&cx.borrow()).expect("NullPointerException").get_type_value();

                    let (op, number_usage) = {
                        let n = node.borrow();
                        let b = n.as_binary_expression().unwrap();
                        (b.op, b.number_usage.clone())
                    };
                    let (string_type, number_type, int_type) =
                        (cx.borrow().string_type(), cx.borrow().number_type(), cx.borrow().int_type());
                    let both_numeric = |s: &Self| s.is_numeric_type(cx, lt.as_ref()) && s.is_numeric_type(cx, rt.as_ref());

                    let mut fall_to_logicalor = false;
                    match op {
                        PLUS_TOKEN => {
                            if both_numeric(self) {
                                let ld = number_double(cx, &lhs_ov.get_value(), number_usage.as_ref());
                                let rd = number_double(cx, &rhs_ov.get_value(), number_usage.as_ref());
                                let d = ld + rd;
                                val = Some(Value::Object(ObjectValue::with_value(&java_double(d), Some(&number_type))));
                            } else if same_type(&lt, &string_type) || same_type(&rt, &string_type) {
                                let ls = self.to_string(cx, &lhs_ov).unwrap_or_else(|| "null".to_string());
                                let rs = self.to_string(cx, &rhs_ov).unwrap_or_else(|| "null".to_string());
                                val = Some(Value::Object(ObjectValue::with_value(&crate::asc::util::jstr::concat(&ls, &rs), Some(&string_type))));
                            }
                        }
                        MINUS_TOKEN | MULT_TOKEN | DIV_TOKEN | MODULUS_TOKEN => {
                            if both_numeric(self) {
                                let ld = number_double(cx, &lhs_ov.get_value(), number_usage.as_ref());
                                let rd = number_double(cx, &rhs_ov.get_value(), number_usage.as_ref());
                                let d = match op {
                                    MINUS_TOKEN => ld - rd,
                                    MULT_TOKEN => ld * rd,
                                    DIV_TOKEN => ld / rd,
                                    _ => ld % rd,
                                };
                                val = Some(Value::Object(ObjectValue::with_value(&java_double(d), Some(&number_type))));
                            } else {
                                fall_to_logicalor = true;
                            }
                        }
                        _ => {}
                    }
                    match op {
                        _ if fall_to_logicalor || op == LOGICALOR_TOKEN => {
                            let b = self.to_boolean(cx, &lhs_ov);
                            let b2 = self.to_boolean(cx, &rhs_ov);
                            if let (Some(b), Some(b2)) = (b, b2) {
                                val = Some(Value::Object(self.get_boolean_object_value(cx, b || b2)));
                            }
                        }
                        LOGICALAND_TOKEN => {
                            let b = self.to_boolean(cx, &lhs_ov);
                            let b2 = self.to_boolean(cx, &rhs_ov);
                            if let (Some(b), Some(b2)) = (b, b2) {
                                val = Some(Value::Object(self.get_boolean_object_value(cx, b && b2)));
                            }
                        }
                        EQUALS_TOKEN | NOTEQUALS_TOKEN => {
                            let b = self.compare(cx, &lhs_ov, &rhs_ov);
                            if op == NOTEQUALS_TOKEN {
                                val = Some(Value::Object(self.get_boolean_object_value(cx, !b)));
                            } else {
                                val = Some(Value::Object(self.get_boolean_object_value(cx, b)));
                            }
                        }
                        LESSTHAN_TOKEN | GREATERTHANOREQUALS_TOKEN => {
                            let less_result = self.lessthan(cx, &lhs_ov, &rhs_ov);
                            let b = if op == LESSTHAN_TOKEN {
                                !(less_result == Some(UNDEFINED) || less_result == Some(FALSE))
                            } else {
                                !(less_result == Some(UNDEFINED) || less_result == Some(TRUE))
                            };
                            val = Some(Value::Object(self.get_boolean_object_value(cx, b)));
                        }
                        LESSTHANOREQUALS_TOKEN | GREATERTHAN_TOKEN => {
                            let less_result = self.lessthan(cx, &rhs_ov, &lhs_ov);
                            let b = if op == LESSTHANOREQUALS_TOKEN {
                                !(less_result == Some(UNDEFINED) || less_result == Some(TRUE))
                            } else {
                                !(less_result == Some(UNDEFINED) || less_result == Some(FALSE))
                            };
                            val = Some(Value::Object(self.get_boolean_object_value(cx, b)));
                        }
                        LEFTSHIFT_TOKEN | RIGHTSHIFT_TOKEN => {
                            let li = self.to_int(cx, &lhs_ov);
                            let ri = self.to_int(cx, &rhs_ov);
                            if let (Some(li), Some(ri)) = (li, ri) {
                                let ri = (ri & 0x1F) as u32;
                                let r = if op == LEFTSHIFT_TOKEN { li.wrapping_shl(ri) } else { li >> ri };
                                val = Some(Value::Object(ObjectValue::with_value(&r.to_string(), Some(&int_type))));
                            }
                        }
                        UNSIGNEDRIGHTSHIFT_TOKEN => {
                            let ll = self.to_uint(cx, &lhs_ov);
                            let rl = self.to_uint(cx, &rhs_ov);
                            if let (Some(ll), Some(rl)) = (ll, rl) {
                                let rl = rl & 0x1F;
                                let r = ((ll as u64) >> rl) as i64;
                                val = Some(Value::Object(ObjectValue::with_value(&r.to_string(), Some(&int_type))));
                            }
                        }
                        BITWISEAND_TOKEN | BITWISEXOR_TOKEN | BITWISEOR_TOKEN => {
                            let li = self.to_int(cx, &lhs_ov);
                            let ri = self.to_int(cx, &rhs_ov);
                            if let (Some(li), Some(ri)) = (li, ri) {
                                let result = match op {
                                    BITWISEAND_TOKEN => li & ri,
                                    BITWISEOR_TOKEN => li | ri,
                                    _ => li ^ ri,
                                };
                                val = Some(Value::Object(ObjectValue::with_value(&result.to_string(), Some(&int_type))));
                            }
                        }
                        _ => {}
                    }
                }
            }
        } else {
            self.fold_field(cx, node, |n| &mut n.as_binary_expression_mut().unwrap().lhs);
            self.fold_field(cx, node, |n| &mut n.as_binary_expression_mut().unwrap().rhs);
        }
        val
    }

    fn evaluate_conditional_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.as_conditional_expression_mut().unwrap().condition);
        self.fold_field(cx, node, |n| &mut n.as_conditional_expression_mut().unwrap().thenexpr);
        self.fold_field(cx, node, |n| &mut n.as_conditional_expression_mut().unwrap().elseexpr);
        None
    }

    fn evaluate_argument_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let size = node.borrow().as_argument_list().unwrap().items.len();
        for i in 0..size {
            let n = node.borrow().as_argument_list().unwrap().items[i].clone();
            if n.is_some() {
                let temp = self.eval_and_fold(cx, n.clone());
                if !same_node(&temp, &n) {
                    node.borrow_mut().as_argument_list_mut().unwrap().items[i] = temp;
                }
            }
        }
        None
    }

    fn evaluate_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let mut val = None;
        let size = node.borrow().list().unwrap().items.len();
        for i in 0..size {
            let n = node.borrow().list().unwrap().items[i].clone();
            val = self.eval(cx, &n);
            let temp = self.fold_ref_value(cx, val.clone());
            if temp.is_some() {
                node.borrow_mut().list_mut().unwrap().items[i] = temp.clone();
                val = self.eval(cx, &temp);
            }
        }
        val
    }

    fn evaluate_statement_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let node_factory = Context::get_node_factory(cx);
        let empty_statement = || node_factory.borrow_mut().empty_statement();
        let item = |i: usize| node.borrow().as_statement_list().unwrap().items[i].clone();
        let set_item = |i: usize, v: Option<NodeRef>| node.borrow_mut().as_statement_list_mut().unwrap().items[i] = v;

        let config_attrs = node.borrow().as_statement_list().unwrap().config_attrs.clone();
        if config_attrs.is_some() {
            self.eval(cx, &config_attrs);
            if !config_attrs.as_ref().unwrap().borrow().as_attribute_list().expect("ClassCastException").compile_definition {
                node.borrow_mut().as_statement_list_mut().unwrap().items.clear();
            }
            node.borrow_mut().as_statement_list_mut().unwrap().config_attrs = None;
        }

        let size = node.borrow().as_statement_list().unwrap().items.len();
        for i in 0..size {
            let n = item(i);
            if kind_is(&n, Node::is_definition) {
                let def = n.clone().unwrap();
                let attrs = def.borrow().definition().expect("ClassCastException").attrs.clone();
                if attrs.is_some() {
                    self.eval(cx, &attrs);

                    if !attrs.as_ref().unwrap().borrow().as_attribute_list().expect("ClassCastException").compile_definition {
                        set_item(i, Some(empty_statement()));
                        self.remove_meta_data(cx, &def, node, i);
                        continue;
                    }
                }

                let old_toplevel = self.top_level;
                let keeps_top_level = {
                    let d = def.borrow();
                    d.as_config_namespace_definition().is_some() || d.as_variable_definition().is_some()
                };
                if !keeps_top_level {
                    self.top_level = false;
                }

                evaluate(&def, cx, self);

                self.top_level = old_toplevel;

                let empty_vardef = {
                    let d = def.borrow();
                    d.as_variable_definition()
                        .map(|v| v.list.as_ref().expect("NullPointerException").borrow().list().unwrap().items.is_empty())
                };
                if empty_vardef == Some(true) {
                    self.remove_meta_data(cx, &def, node, i);
                    set_item(i, Some(empty_statement()));
                }
            } else if kind_is(&n, |t| t.as_statement_list().is_some()) {
                let stmt = n.clone().unwrap();
                let stmt_config_attrs = stmt.borrow().as_statement_list().unwrap().config_attrs.clone();
                if stmt_config_attrs.is_some() {
                    self.eval(cx, &stmt_config_attrs);
                    let compile_definition =
                        stmt_config_attrs.as_ref().unwrap().borrow().as_attribute_list().expect("ClassCastException").compile_definition;
                    if compile_definition {
                        if let Some(def) = self.starts_with_definition(cx, &stmt) {
                            let mut m = i as i32 - 1;
                            while m >= 0 {
                                let temp = item(m as usize);
                                if kind_is(&temp, |t| t.meta_data().is_some()) {
                                    let metadata = temp.unwrap();
                                    metadata.borrow_mut().meta_data_mut().unwrap().def = Some(def.clone());
                                    def.borrow_mut().definition_mut().unwrap().add_meta_data_node(Some(metadata));
                                } else if !kind_is(&temp, |t| t.as_include_directive().is_some() || t.as_empty_statement().is_some()) {
                                    break;
                                }
                                m -= 1;
                            }
                        }
                        let ends_with_meta = kind_is(&last(&stmt), |t| t.meta_data().is_some());
                        if ends_with_meta {
                            if let Some(def) = self.find_next_definition(cx, node, i + 1) {
                                let stmt_size = stmt.borrow().as_statement_list().unwrap().items.len() as i32;
                                let mut m = stmt_size - 1;
                                while m >= 0 {
                                    let temp = stmt.borrow().as_statement_list().unwrap().items[m as usize].clone();
                                    if kind_is(&temp, |t| t.meta_data().is_some()) {
                                        let metadata = temp.unwrap();
                                        metadata.borrow_mut().meta_data_mut().unwrap().def = Some(def.clone());
                                        def.borrow_mut().definition_mut().unwrap().add_meta_data_node(Some(metadata));
                                    } else if !kind_is(&temp, |t| t.as_include_directive().is_some() || t.as_empty_statement().is_some()) {
                                        break;
                                    }
                                    m -= 1;
                                }
                            }
                        }
                    } else {
                        set_item(i, Some(empty_statement()));
                    }
                    stmt.borrow_mut().as_statement_list_mut().unwrap().config_attrs = None;
                }
                let n = item(i);
                let temp = self.eval_and_fold(cx, n.clone());
                if !same_node(&temp, &n) {
                    set_item(i, temp);
                }
            } else {
                let is_stmt_list = kind_is(&n, |t| t.as_statement_list().is_some());
                let old_top_level = self.top_level;
                if !is_stmt_list {
                    self.top_level = false;
                }

                let temp = self.eval_and_fold(cx, n.clone());
                if !same_node(&temp, &n) {
                    set_item(i, temp);
                }

                if !is_stmt_list {
                    self.top_level = old_top_level;
                }
            }
        }

        None
    }

    fn evaluate_expression_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = node.borrow().as_expression_statement().unwrap().expr.clone();
        let mut val = self.eval(cx, &expr);
        let temp = self.fold_ref_value(cx, val.clone());
        if temp.is_some() {
            node.borrow_mut().as_expression_statement_mut().unwrap().expr = temp.clone();
            val = self.eval(cx, &temp);
        }
        val
    }

    fn evaluate_labeled_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.as_labeled_statement_mut().unwrap().label);
        self.fold_field(cx, node, |n| &mut n.as_labeled_statement_mut().unwrap().statement);
        None
    }

    fn evaluate_if_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.as_if_statement_mut().unwrap().condition);
        self.fold_field(cx, node, |n| &mut n.as_if_statement_mut().unwrap().thenactions);
        self.fold_field(cx, node, |n| &mut n.as_if_statement_mut().unwrap().elseactions);
        None
    }

    fn evaluate_switch_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.as_switch_statement_mut().unwrap().expr);
        let statements = node.borrow().as_switch_statement().unwrap().statements.clone();
        if statements.is_some() {
            self.eval(cx, &statements);
        }
        None
    }

    fn evaluate_case_label(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.as_case_label_mut().unwrap().label);
        None
    }

    fn evaluate_do_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.as_do_statement_mut().unwrap().expr);
        self.fold_field(cx, node, |n| &mut n.as_do_statement_mut().unwrap().statements);
        None
    }

    fn evaluate_while_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.as_while_statement_mut().unwrap().expr);
        self.fold_field(cx, node, |n| &mut n.as_while_statement_mut().unwrap().statement);
        None
    }

    fn evaluate_for_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.as_for_statement_mut().unwrap().initialize);
        self.fold_field(cx, node, |n| &mut n.as_for_statement_mut().unwrap().test);
        self.fold_field(cx, node, |n| &mut n.as_for_statement_mut().unwrap().increment);
        self.fold_field(cx, node, |n| &mut n.as_for_statement_mut().unwrap().statement);
        None
    }

    fn evaluate_with_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.as_with_statement_mut().unwrap().expr);
        self.fold_field(cx, node, |n| &mut n.as_with_statement_mut().unwrap().statement);
        None
    }

    fn evaluate_return_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.as_return_statement_mut().unwrap().expr);
        None
    }

    fn evaluate_throw_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.as_throw_statement_mut().unwrap().expr);
        None
    }

    fn evaluate_try_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (tryblock, catchlist, finallyblock) = {
            let n = node.borrow();
            let t = n.as_try_statement().unwrap();
            (t.tryblock.clone(), t.catchlist.clone(), t.finallyblock.clone())
        };
        if tryblock.is_some() {
            self.eval(cx, &tryblock);
        }
        if catchlist.is_some() {
            self.eval(cx, &catchlist);
        }
        if finallyblock.is_some() {
            self.eval(cx, &finallyblock);
        }
        None
    }

    fn evaluate_catch_clause(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if node.borrow().as_catch_clause().unwrap().parameter.is_some() {
            self.fold_field(cx, node, |n| &mut n.as_catch_clause_mut().unwrap().parameter);
        }
        let statements = node.borrow().as_catch_clause().unwrap().statements.clone();
        if statements.is_some() {
            self.eval(cx, &statements);
        }
        None
    }

    fn evaluate_finally_clause(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (default_catch, statements) = {
            let n = node.borrow();
            let f = n.as_finally_clause().unwrap();
            (f.default_catch.clone(), f.statements.clone())
        };
        if default_catch.is_some() {
            self.eval(cx, &default_catch);
        }
        if statements.is_some() {
            self.eval(cx, &statements);
        }
        None
    }

    fn evaluate_meta_data(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let data = node.borrow().meta_data().unwrap().data.clone().expect("NullPointerException");
        self.evaluate_literal_array(cx, &data);
        None
    }

    fn evaluate_attribute_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let size = node.borrow().as_attribute_list().unwrap().items.len();
        for i in 0..size {
            let n = node.borrow().as_attribute_list().unwrap().items[i].clone();
            let val1 = self.eval(cx, &n);

            if let Some(obj) = val1.and_then(|v| v.as_object_value()) {
                if i == size - 1 {
                    let tv = obj.get_type(&cx.borrow()).expect("NullPointerException").get_type_value();
                    let boolean_type = cx.borrow().boolean_type();
                    if same_type(&tv, &boolean_type) {
                        let mut nb = node.borrow_mut();
                        let a = nb.as_attribute_list_mut().unwrap();
                        a.compile_definition = obj.boolean_value();
                        a.items.pop();
                    }
                }
            }
        }
        None
    }

    fn evaluate_variable_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let list = node.borrow().as_variable_definition().unwrap().list.clone();
        self.eval(cx, &list);
        let list = list.unwrap();
        let size = list.borrow().list().expect("ClassCastException").items.len() as i32;
        let mut i = size - 1;
        while i >= 0 {
            let n = list.borrow().list().unwrap().items[i as usize].clone();
            let is_config_binding = n.as_ref().map_or(false, |n| {
                n.borrow().as_variable_binding().map_or(false, |v| v.ref_.as_ref().map_or(false, |r| r.is_config_ref()))
            });
            if is_config_binding {
                list.borrow_mut().list_mut().unwrap().items.pop();
            }
            i -= 1;
        }
        None
    }

    fn evaluate_variable_binding(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let variable = node.borrow().as_variable_binding().unwrap().variable.clone();
        let identifier = variable.as_ref().expect("NullPointerException").borrow().as_typed_identifier().expect("ClassCastException").identifier.clone();
        let val = self.eval(cx, &identifier);
        let ref_ = val.and_then(|v| v.as_reference_value());

        if let Some(r) = ref_.filter(|r| r.is_config_ref()) {
            let node_pos = node.borrow().pos();
            if !self.is_top_level() {
                cx.borrow().error(node_pos, kError_InvalidConfigLocation);
                return None;
            }

            let (name, namespaces) = (r.name(), r.get_immutable_namespaces());
            match self.define_config_slot(cx, &name, &namespaces) {
                Some(slot) => {
                    let (kind, attrs, initializer) = {
                        let n = node.borrow();
                        let v = n.as_variable_binding().unwrap();
                        (v.kind, v.attrs.clone(), v.initializer.clone())
                    };
                    if kind != CONST_TOKEN {
                        let attrs_pos = attrs.as_ref().expect("NullPointerException").borrow().pos();
                        cx.borrow().error(attrs_pos, kError_NonConstConfigVar);
                    }
                    slot.set_const(true);
                    if initializer.is_none() {
                        cx.borrow().error(node_pos, kError_NonConstantConfigInit);
                    } else {
                        let old_fold = self.fold_expressions;
                        self.fold_expressions = true;
                        let init_val = self.eval(cx, &initializer);
                        self.fold_expressions = old_fold;
                        if init_val.as_ref().map_or(true, |v| !v.has_value()) {
                            let p = initializer.as_ref().unwrap().borrow().pos();
                            cx.borrow().error(p, kError_NonConstantConfigInit);
                        }
                        slot.set_value(init_val);
                    }
                    node.borrow_mut().as_variable_binding_mut().unwrap().ref_ = Some(r);
                }
                None => {
                    let p = identifier.as_ref().unwrap().borrow().pos();
                    let ns_name = namespaces.at(0).name();
                    cx.borrow().error2(p, kError_ConflictingNameInNamespace, &name, &ns_name);
                }
            }
        } else {
            self.fold_field(cx, node, |n| &mut n.as_variable_binding_mut().unwrap().initializer);
        }
        None
    }

    fn evaluate_typed_identifier(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let identifier = node.borrow().as_typed_identifier().unwrap().identifier.clone();
        self.eval(cx, &identifier)
    }

    fn evaluate_function_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let fexpr = node.borrow().function_definition().unwrap().fexpr.clone();
        self.eval(cx, &fexpr);
        None
    }

    fn evaluate_function_signature(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (parameter, result) = {
            let n = node.borrow();
            let s = n.as_function_signature().unwrap();
            (s.parameter.clone(), s.result.clone())
        };
        if parameter.is_some() {
            self.eval(cx, &parameter);
        }
        if result.is_some() {
            self.eval(cx, &result);
        }
        None
    }

    fn evaluate_parameter(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.parameter_mut().unwrap().init);
        None
    }

    fn evaluate_parameter_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let size = node.borrow().as_parameter_list().unwrap().items.len();
        for i in 0..size {
            let n = node.borrow().as_parameter_list().unwrap().items[i].clone();
            self.eval(cx, &n);
        }

        None
    }

    fn evaluate_interface_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.evaluate_class_definition(cx, node)
    }

    fn evaluate_class_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.push_config_scope(cx);

        let statements = node.borrow().class_definition().unwrap().statements.clone();
        if statements.is_some() {
            self.eval(cx, &statements);
        }

        cx.borrow().pop_scope();

        None
    }

    fn evaluate_config_namespace_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if !self.is_top_level() {
            let p = node.borrow().pos();
            cx.borrow().error(p, kError_InvalidConfigLocation);
            return None;
        }
        let namespaces = Namespaces::new();
        let namespace_ids: Vec<String> = Vec::new();
        namespaces.push_back(cx.borrow().public_namespace());

        let name = {
            let n = node.borrow();
            let nd = n.namespace_definition().unwrap();
            let id = nd.name.as_ref().expect("NullPointerException").borrow();
            id.identifier().expect("ClassCastException").name.clone()
        };

        if let Some(s) = self.define_config_slot(cx, &name, &namespaces) {
            let c = cx.borrow();
            let debug_name = c.debug_name("", &name, Some(&namespace_ids), EMPTY_TOKEN);
            let ns = c.get_namespace_kind(&debug_name, NS_INTERNAL);
            if ns.is_namespace_value() {
                ns.set_config_ns(true);
            }
            s.set_object_value(Some(ns));

            s.set_const(true);

            self.config_namespaces.insert(name);
        }

        None
    }

    fn evaluate_namespace_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (pos, name) = {
            let n = node.borrow();
            let nd = n.namespace_definition().unwrap();
            let id = nd.name.as_ref().expect("NullPointerException").borrow();
            (n.pos(), id.identifier().expect("ClassCastException").name.clone())
        };
        if self.config_namespaces.contains(&name) {
            cx.borrow().error1(pos, kError_ShadowedConfigNamespace, &name);
        }
        None
    }

    fn evaluate_program(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.push_config_scope(cx);

        self.top_level = true;

        let statements = node.borrow().program().unwrap().statements.clone();
        self.eval(cx, &statements);

        self.top_level = false;

        cx.borrow().pop_scope();
        None
    }

    fn evaluate_to_object(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.as_to_object_mut().unwrap().expr);
        None
    }

    fn evaluate_box(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.as_box_mut().unwrap().expr);
        None
    }

    fn evaluate_coerce(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.fold_field(cx, node, |n| &mut n.as_coerce_mut().unwrap().expr);
        None
    }

    fn evaluate_pragma(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let list = node.borrow().as_pragma().unwrap().list.clone();
        self.eval(cx, &list);
        None
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::asc::parser::parser::Parser;
    use crate::asc::parser::printer::NodePrinter;
    use crate::asc::semantics::emitter::{Emitter, EmitterBase};
    use crate::asc::util::context::{CompilerHandler, ContextStatics};

    struct Errors(Rc<RefCell<String>>);

    impl CompilerHandler for Errors {
        fn error(&mut self, _filename: &str, ln: i32, col: i32, _msg: &str, _source: &str, code: i32) {
            self.0.borrow_mut().push_str(&format!("{} {} {}\n", code, ln, col));
        }
    }

    /// Decimal literals as `ActionBlockEmitter.getDoubleValueOrInt` reads them (no hex, no es4 numerics).
    struct Literals(EmitterBase, ContextRef);

    impl Emitter for Literals {
        fn emitter_base(&self) -> &EmitterBase {
            &self.0
        }

        fn emitter_base_mut(&mut self) -> &mut EmitterBase {
            &mut self.0
        }

        fn get_value_of_number_literal(&mut self, s: &str, t: &mut Option<TypeValue>, _u: Option<&NumberUsage>) -> Option<NumberConstant> {
            let cx = self.1.borrow();
            if !(s.contains(['.', 'e', 'E']) || s.len() > 10) {
                if let Ok(i) = s.parse::<i32>() {
                    *t = Some(cx.int_type());
                    return Some(NumberConstant::Int(i));
                }
            }
            *t = Some(cx.double_type());
            Some(NumberConstant::Double(crate::abc::decomp::ecma::java_parse_double(s).expect("NumberFormatException")))
        }
    }

    /// The tree after `ConfigurationEvaluator` as `NodePrinter` prints it with positions, and the errors.
    fn run(src: &str) -> (String, String) {
        ObjectValue::init();
        let statics = Rc::new(RefCell::new(ContextStatics::new()));
        let cx = Rc::new(RefCell::new(Context::new(statics)));
        let errors = Rc::new(RefCell::new(String::new()));
        cx.borrow_mut().set_handler(Rc::new(RefCell::new(Errors(errors.clone()))));
        cx.borrow().set_emitter(Some(Rc::new(RefCell::new(Literals(EmitterBase::new(None), cx.clone())))));
        let program = Parser::new_stream(&cx, src.as_bytes(), "t.as").parse_program().unwrap();
        evaluate(&program, &cx, &mut ConfigurationEvaluator::new());
        let mut printer = NodePrinter::new_out(Vec::new());
        printer.show_positions = true;
        evaluate(&program, &cx, &mut printer);
        let tree = String::from_utf8(printer.into_inner()).unwrap();
        let errors = errors.borrow().clone();
        (tree, errors)
    }

    #[test]
    fn folds_config_constants_and_drops_false_definitions() {
        let (tree, errors) = run("config namespace MY;
MY const a = 1 + 2;
MY const on = MY::a > 2;
MY const off = !MY::on;
MY var bad = 1;
MY::on function f():void {}
MY::off function g():void {}
var v = [0, MY::on 1, MY::off 2, MY::a];
trace(MY::zzz);
");
        // Expected output of Java ASC.
        let expected = "@220
program @17
  statementlist @0
    config namespace  @0
      identifier CONFIG @0
    import  @0
      packagename @0
        packageidentifiers @0
          identifier __AS3__@0
          identifier vec@0
          identifier Vector@17
    config namespace  @17
      identifier MY @122
    function @110
      attributelist @122
      functionname @122
        qualifiedidentifier f 
          qualifier @110
            attributelist @122
      functioncommon @126
        functionsignature @0
        statementlist @132
          return @167
    var  @167
      list @167
        variablebinding @167
          typedidentifier @167
            qualifiedidentifier v@171
          literalarray @172
            argumentlist @172
              literalnumber:0@182
              literalnumber:1@0
              literalnumber:3.0@204
    expression @204
      list @204
        member @204
          call lexical @204
            identifier trace@214
            argumentlist @214
              member @214
                get lexical @214
                  qualifiedidentifier zzz 
                    qualifier @210
                      member @210
                        get lexical @210
                          identifier MY
";
        assert_eq!(tree, expected);
        assert_eq!(errors, "1209 5 1\n1120 9 11\n");
    }
}
