//! Port of `macromedia/asc/semantics/CodeGenerator.java`: walks the analyzed tree and drives the emitter chain.

#![allow(non_upper_case_globals)]

use std::rc::Rc;

use crate::asc::emit::features::TARGET_AVM2;
use crate::asc::emit::runtime_constants::*;
use crate::asc::parser::evaluator::{evaluate, Evaluator};
use crate::asc::parser::node::*;
use crate::asc::parser::tokens::*;
use crate::asc::semantics::emitter::{Emitter, EmitterBase, EmitterRef};
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::q_name::QName;
use crate::asc::semantics::reference_value::ReferenceValue;
use crate::asc::semantics::slot::*;
use crate::asc::semantics::type_info::TypeInfo;
use crate::asc::semantics::type_value::TypeValue;
use crate::asc::semantics::value::Value;
use crate::asc::util::context::{Context, ContextRef};
use crate::asc::util::decimal128::Decimal128;
use crate::asc::util::errors::*;
use crate::asc::util::namespaces::Namespaces;
use crate::asc::util::number_constant::NumberConstant;

#[derive(Clone)]
struct StackFrame {
    catch_index: i32,
    first_inner_scope: i32,
    register_scope_index: i32,
    with_this: bool,
    activation_is_exposed: bool,
    max_temps: i32,
    max_params: i32,
    max_locals: i32,
    function_name: String,
    needs_arguments: i32,
}

impl Default for StackFrame {
    fn default() -> StackFrame {
        StackFrame {
            catch_index: 0,
            first_inner_scope: -1,
            register_scope_index: -1,
            with_this: false,
            activation_is_exposed: false,
            max_temps: 0,
            max_params: 0,
            max_locals: 0,
            function_name: String::new(),
            needs_arguments: 0,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct ExceptionState {
    finally_present: bool,
    any_finally_present: bool,
    inside_finally: bool,
    ignore_throws: bool,
}

pub struct CodeGenerator {
    base: EmitterBase,
    traverse_argslist_right_to_left: bool,
    traverse_binop_right_to_left: bool,
    c_call_sequence: bool,
    case_exprs: Vec<Vec<Option<NodeRef>>>,
    frame: Option<StackFrame>,
    frames: Vec<StackFrame>,
    exception_state: Option<ExceptionState>,
    current_class: Option<NodeRef>,
    used_namespaces_sets: Vec<Option<Namespaces>>,
    temp_cv_reg: i32,
    in_with: bool,
    in_anonymous_function: bool,
    is_ctor: bool,
    in_typeof: bool,
    pub emit_script_names: bool,
}

struct LazyTemp {
    index: i32,
}

impl LazyTemp {
    fn new() -> LazyTemp {
        LazyTemp { index: -1 }
    }

    fn get(&mut self, cg: &mut CodeGenerator) -> i32 {
        if self.index == -1 {
            self.index = cg.allocate_temp();
        }
        self.index
    }

    fn free(&mut self, cg: &mut CodeGenerator) {
        if self.index != -1 {
            cg.free_temp(self.index);
            self.index = -1;
        }
    }
}

fn nn(n: &Option<NodeRef>) -> &NodeRef {
    n.as_ref().expect("NullPointerException")
}

fn opt_obj_eq(a: &Option<ObjectValue>, b: &Option<ObjectValue>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.ptr_eq(b),
        (None, None) => true,
        _ => false,
    }
}

fn tv_is(a: &Option<TypeValue>, b: &TypeValue) -> bool {
    a.as_ref().map_or(false, |a| a.ptr_eq(b))
}

fn value_is_obj(v: &Option<Value>, o: &ObjectValue) -> bool {
    matches!(v, Some(Value::Object(x)) if x.ptr_eq(o))
}

fn builder_of(ob: &ObjectValue) -> Option<crate::asc::semantics::builder::Builder> {
    ob.builder()
}

fn depth(cx: &ContextRef) -> i32 {
    cx.borrow().get_scope_depth() as i32
}

fn scope_at(cx: &ContextRef, i: i32) -> ObjectValue {
    cx.borrow().scope_at(i)
}

fn scope_top(cx: &ContextRef) -> ObjectValue {
    cx.borrow().scope().expect("NullPointerException")
}

fn slot_of(r: &ReferenceValue, cx: &ContextRef, kind: i32) -> Option<Slot> {
    r.get_slot_kind(&cx.borrow(), kind)
}

fn type_of(r: &ReferenceValue, cx: &ContextRef) -> TypeInfo {
    r.get_type(&cx.borrow())
}

fn slot_type(s: &Slot) -> TypeInfo {
    s.get_type().expect("NullPointerException")
}

fn ti_name(t: &TypeInfo, cx: &ContextRef) -> QName {
    t.get_name_cx(&cx.borrow())
}

fn ident_name(n: &Option<NodeRef>) -> String {
    nn(n).borrow().identifier().expect("ClassCastException").name.clone()
}

/// `node.getIdentifier() instanceof QualifiedExpressionNode`: the qualifier `expr` and `nss`, or `None`.
fn qen_parts(ident: &NodeRef) -> Option<(Option<NodeRef>, Option<Namespaces>, bool)> {
    let b = ident.borrow();
    b.as_qualified_expression().map(|q| (q.expr.clone(), q.nss.clone(), q.qid.qualifier.is_some()))
}

fn items_of(n: &NodeRef) -> Vec<Option<NodeRef>> {
    let b = n.borrow();
    match &b.kind {
        Kind::ArgumentList(a) => a.items.clone(),
        Kind::List(l) | Kind::ListError(ListErrorNode { list: l, .. }) => l.items.clone(),
        Kind::StatementList(s) => s.items.clone(),
        Kind::AttributeList(a) => a.items.clone(),
        Kind::ParameterList(p) => p.items.clone(),
        _ => panic!("ClassCastException"),
    }
}

fn item_at(n: &NodeRef, i: usize) -> Option<Option<NodeRef>> {
    let b = n.borrow();
    let items = match &b.kind {
        Kind::ArgumentList(a) => &a.items,
        Kind::List(l) | Kind::ListError(ListErrorNode { list: l, .. }) => &l.items,
        Kind::StatementList(s) => &s.items,
        Kind::AttributeList(a) => &a.items,
        _ => panic!("ClassCastException"),
    };
    items.get(i).cloned()
}

fn list_size(n: &Option<NodeRef>) -> i32 {
    match n {
        Some(n) => items_of(n).len() as i32,
        None => 0,
    }
}

fn node_list(l: &Option<NodeList>) -> Vec<NodeRef> {
    l.as_ref().map(|l| l.borrow().clone()).unwrap_or_default()
}

fn java_last_index_of_slash(s: &str) -> i32 {
    let u: Vec<u16> = crate::asc::util::jstr::units(&s).collect();
    u.iter().rposition(|&c| c == '/' as u16).map_or(-1, |i| i as i32)
}

fn java_sub(s: &str, begin: i32, end: i32) -> String {
    let u: Vec<u16> = crate::asc::util::jstr::units(&s).collect();
    crate::asc::util::jstr::from_utf16(&u[begin as usize..end as usize])
}

fn java_len(s: &str) -> i32 {
    crate::asc::util::jstr::units(&s).count() as i32
}

impl CodeGenerator {
    pub fn new(emitter: EmitterRef) -> CodeGenerator {
        CodeGenerator {
            base: EmitterBase::new(Some(emitter)),
            traverse_argslist_right_to_left: false,
            traverse_binop_right_to_left: false,
            c_call_sequence: false,
            case_exprs: Vec::new(),
            frame: None,
            frames: Vec::new(),
            exception_state: None,
            current_class: None,
            used_namespaces_sets: Vec::new(),
            temp_cv_reg: -1,
            in_with: false,
            in_anonymous_function: false,
            is_ctor: false,
            in_typeof: false,
            emit_script_names: false,
        }
    }

    pub fn push_stack_frame(&mut self) {
        if let Some(f) = self.frame.take() {
            self.frames.push(f);
        }
        self.frame = Some(StackFrame::default());
    }

    fn pop_stack_frame(&mut self) {
        self.frame = self.frames.pop();
    }

    pub fn push_args_first(&mut self, b: bool) {
        self.c_call_sequence = b;
    }

    pub fn push_args_right_to_left(&mut self, b: bool) {
        self.traverse_argslist_right_to_left = b;
    }

    fn frame(&self) -> &StackFrame {
        self.frame.as_ref().expect("NullPointerException")
    }

    fn frame_mut(&mut self) -> &mut StackFrame {
        self.frame.as_mut().expect("NullPointerException")
    }

    fn uns(&self) -> Option<Namespaces> {
        self.used_namespaces_sets.last().cloned().flatten()
    }

    fn ev(&mut self, cx: &ContextRef, n: &Option<NodeRef>) -> Option<Value> {
        evaluate(nn(n), cx, self)
    }

    fn set_position_at(&mut self, cx: &ContextRef, pos: i32) {
        let input = cx.borrow().input.clone();
        if let Some(input) = input {
            let ln = input.borrow_mut().get_ln_num(pos);
            let col = input.borrow_mut().get_col_pos(pos);
            self.set_position(ln, col, pos);
        }
    }

    fn is_local_scope(&self, cx: &ContextRef, index: i32) -> bool {
        index >= 0
            && index >= self.frame().first_inner_scope
            && builder_of(&scope_at(cx, index)).map_or(false, |b| b.is_activation_builder())
    }

    /// `If(Context, Node)`.
    fn if_cx(&mut self, cx: &ContextRef, expr: &Option<NodeRef>) {
        let ben = Self::compare_of(expr);
        let mut kind = IF_false;
        if let Some((_, _, method_id)) = &ben {
            kind = match *method_id {
                BINARY_LessThanOp if HAS_IF_nlt => IF_nlt,
                BINARY_GreaterThanOp if HAS_IF_ngt => IF_ngt,
                BINARY_LessThanOrEqualOp if HAS_IF_nle => IF_nle,
                BINARY_GreaterThanOrEqualOp if HAS_IF_nge => IF_nge,
                BINARY_EqualsOp_II | BINARY_EqualsOp if HAS_IF_ne => IF_ne,
                BINARY_NotEqualsOp_II | BINARY_NotEqualsOp if HAS_IF_eq => IF_eq,
                BINARY_StrictEqualsOp_II | BINARY_StrictEqualsOp if HAS_IF_strictne => IF_strictne,
                BINARY_StrictNotEqualsOp_II | BINARY_StrictNotEqualsOp if HAS_IF_stricteq => IF_stricteq,
                BINARY_LessThanOp | BINARY_GreaterThanOp | BINARY_LessThanOrEqualOp | BINARY_GreaterThanOrEqualOp
                | BINARY_EqualsOp_II | BINARY_EqualsOp | BINARY_NotEqualsOp_II | BINARY_NotEqualsOp
                | BINARY_StrictEqualsOp_II | BINARY_StrictEqualsOp | BINARY_StrictNotEqualsOp_II
                | BINARY_StrictNotEqualsOp => kind,
                _ => IF_false,
            };
        }
        match kind {
            IF_lt | IF_ge | IF_le | IF_gt | IF_eq | IF_ne | IF_stricteq | IF_strictne | IF_nlt | IF_nge | IF_nle | IF_ngt => {
                let (lhs, rhs, _) = ben.unwrap();
                self.ev(cx, &lhs);
                self.ev(cx, &rhs);
                self.if_(kind);
            }
            _ => {
                self.ev(cx, expr);
                self.if_(kind);
            }
        }
    }

    /// The `BinaryExpressionNode` ending a condition list, as `(lhs, rhs, slot method id)`.
    fn compare_of(expr: &Option<NodeRef>) -> Option<(Option<NodeRef>, Option<NodeRef>, i32)> {
        let expr = expr.as_ref()?;
        let temp = {
            let b = expr.borrow();
            let ln = b.list()?;
            ln.items.last().cloned().flatten()
        };
        let temp = temp?;
        let ben = {
            let b = temp.borrow();
            match &b.kind {
                Kind::Coerce(cn) => cn.expr.clone().filter(|e| e.borrow().as_binary_expression().is_some()),
                Kind::BinaryExpression(_) => Some(temp.clone()),
                _ => None,
            }
        }?;
        let b = ben.borrow();
        let n = b.as_binary_expression().unwrap();
        let mid = n.slot.as_ref().expect("NullPointerException").get_method_id();
        Some((n.lhs.clone(), n.rhs.clone(), mid))
    }

    /// `LoopEnd(Context, Node)`.
    fn loop_end_cx(&mut self, cx: &ContextRef, expr: &Option<NodeRef>) {
        if expr.is_none() {
            self.push_boolean(true);
            self.loop_end(IF_true);
        } else {
            let mut kind = IF_true;
            let ben = Self::compare_of(expr);
            if let Some((_, _, method_id)) = &ben {
                kind = match *method_id {
                    BINARY_LessThanOp if HAS_IF_lt => IF_lt,
                    BINARY_GreaterThanOp if HAS_IF_gt => IF_gt,
                    BINARY_LessThanOrEqualOp if HAS_IF_le => IF_le,
                    BINARY_GreaterThanOrEqualOp if HAS_IF_ge => IF_ge,
                    BINARY_EqualsOp_II | BINARY_EqualsOp if HAS_IF_eq => IF_eq,
                    BINARY_NotEqualsOp_II | BINARY_NotEqualsOp if HAS_IF_ne => IF_ne,
                    BINARY_StrictEqualsOp_II | BINARY_StrictEqualsOp if HAS_IF_stricteq => IF_stricteq,
                    BINARY_StrictNotEqualsOp_II | BINARY_StrictNotEqualsOp if HAS_IF_strictne => IF_strictne,
                    BINARY_LessThanOp | BINARY_GreaterThanOp | BINARY_LessThanOrEqualOp | BINARY_GreaterThanOrEqualOp
                    | BINARY_EqualsOp_II | BINARY_EqualsOp | BINARY_NotEqualsOp_II | BINARY_NotEqualsOp
                    | BINARY_StrictEqualsOp_II | BINARY_StrictEqualsOp | BINARY_StrictNotEqualsOp_II
                    | BINARY_StrictNotEqualsOp => kind,
                    _ => IF_true,
                };
            }
            match kind {
                IF_lt | IF_ge | IF_le | IF_gt | IF_eq | IF_ne | IF_stricteq | IF_strictne | IF_nlt | IF_nge | IF_nle
                | IF_ngt => {
                    let (lhs, rhs, _) = ben.unwrap();
                    self.ev(cx, &lhs);
                    self.ev(cx, &rhs);
                    self.loop_end(kind);
                }
                _ => {
                    self.ev(cx, expr);
                    self.loop_end(IF_true);
                }
            }
        }
    }

    fn do_ctor_setup(&mut self, node: &NodeRef, cx: &ContextRef, needs_activation: bool) {
        self.is_ctor = true;
        let scope_depth = depth(cx);
        let iframe = scope_at(cx, scope_depth - 2);
        let ib = builder_of(&iframe).filter(|b| b.is_instance_builder());
        let es4_nullability = cx.borrow().statics.borrow().es4_nullability;

        if es4_nullability && needs_activation {
            let d = depth(cx);
            let f = self.frame_mut();
            f.activation_is_exposed = false;
            f.register_scope_index = d - 1;
        }

        {
            let cur = self.current_class.clone().expect("NullPointerException");
            let instanceinits = node_list(&cur.borrow().class_definition().expect("ClassCastException").instanceinits);
            for def in instanceinits {
                self.remove_constant_initializers(cx, &def);
                let is_def = def.borrow().is_definition();
                if es4_nullability && !is_def {
                    iframe.set_init_only(true);
                }
                evaluate(&def, cx, self);
                if es4_nullability && !is_def {
                    iframe.set_init_only(false);
                }
                self.clear_position_info();
            }
        }

        self.clear_position_info();

        let (sig_pos, inits) = {
            let b = node.borrow();
            let sig = nn(&b.as_function_common().unwrap().signature).clone();
            let s = sig.borrow();
            (s.pos(), s.as_function_signature().unwrap().inits.clone())
        };
        self.set_position_at(cx, sig_pos);

        if inits.is_some() {
            iframe.set_init_only(true);
            self.ev(cx, &inits);
            iframe.set_init_only(false);
        }

        if es4_nullability && needs_activation {
            let f = self.frame_mut();
            f.activation_is_exposed = true;
            f.register_scope_index = -1;
        }

        let ib = ib.expect("NullPointerException");
        let (has_base, calls_super) = {
            let d = ib.instance_data();
            (d.basebui.is_some(), d.calls_super_ctor)
        };
        if has_base && !calls_super {
            self.load_this();
            self.invoke_super(true, 0);
        }
    }

    fn get_program_name(&self, cx: &ContextRef) -> String {
        let input = cx.borrow().input.clone();
        match input {
            Some(input) if self.emit_script_names => {
                let origin = input.borrow().origin.clone().unwrap_or_default();
                std::path::Path::new(&origin).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()
            }
            _ => String::new(),
        }
    }

    fn push_static_scopes_helper(&mut self, cx: &ContextRef, cframe: Option<TypeValue>) -> i32 {
        let Some(cframe) = cframe else { return 0 };

        let mut count = 0;
        if let Some(b) = cframe.baseclass() {
            count = self.push_static_scopes_helper(cx, Some(b));
        }

        let namespaces = Namespaces::new();
        let qn = cframe.name();
        namespaces.push_back(qn.ns().expect("NullPointerException"));

        let name = qn.name().to_string();
        self.find_property_name_nss(&name, Some(&namespaces), true, true, false);
        self.get_property_name_nss(&name, Some(&namespaces), true, false, false);
        self.push_scope();

        count + 1
    }

    pub fn get_activation_object(&mut self, base_index: i32) {
        let f = self.frame().first_inner_scope;
        self.get_base_object(base_index - f);
    }

    pub fn get_register_offset(&self, cx: &ContextRef) -> i32 {
        let mut i = depth(cx);
        loop {
            i -= 1;
            if i < 0 {
                break;
            }
            let builder = builder_of(&scope_at(cx, i)).expect("NullPointerException");
            if builder.has_register_offset() {
                return builder.data().reg_offset;
            }
        }
        -1
    }

    pub fn store_default_value(&mut self, cx: &ContextRef, index: i32, s: &Slot, needs_activation: bool) {
        if needs_activation {
            let d = depth(cx);
            self.get_activation_object(d - 1);
        }
        let type_ = slot_type(s);
        let tv = type_.get_type_value();
        let (int_t, uint_t, bool_t, num_t, dbl_t, dec_t, no_t, void_t, es4_numerics) = {
            let c = cx.borrow();
            let es4 = c.statics.borrow().es4_numerics;
            (
                c.int_type(),
                c.uint_type(),
                c.boolean_type(),
                c.number_type(),
                c.double_type(),
                c.decimal_type(),
                c.no_type(),
                c.void_type(),
                es4,
            )
        };
        if tv_is(&tv, &int_t) {
            self.push_number(&NumberConstant::Int(0), TYPE_int);
        } else if tv_is(&tv, &uint_t) {
            self.push_number(&NumberConstant::Uint(0), TYPE_uint_external);
            let n = ti_name(&type_, cx);
            self.check_type(&n);
        } else if tv_is(&tv, &bool_t) {
            self.push_boolean(false);
        } else if tv_is(&tv, &num_t) {
            self.push_number(&NumberConstant::Double(f64::NAN), TYPE_double);
        } else if tv_is(&tv, &dbl_t) {
            self.push_number(&NumberConstant::Double(f64::NAN), TYPE_double);
        } else if es4_numerics && opt_tv_eq(&tv, &dec_t) {
            self.push_number(&NumberConstant::Decimal(Decimal128::nan()), TYPE_decimal);
        } else if tv_is(&tv, &no_t) {
            let obj_value = s.get_object_value();
            if cx.borrow().is_namespace(obj_value.as_ref()) {
                self.push_namespace(obj_value.as_ref().unwrap());
            } else {
                self.push_undefined();
                let n = ti_name(&type_, cx);
                self.check_type(&n);
            }
        } else if tv_is(&tv, &void_t) {
            self.push_undefined();
        } else {
            if type_.is_nullable() {
                self.push_null();
            } else {
                self.push_uninitialized();
            }
            let n = ti_name(&type_, cx);
            self.check_type(&n);
        }
        if needs_activation {
            self.store_var(index);
        } else {
            self.store_register(index, type_.get_type_id());
        }
    }

    pub fn is_class_initializer_reference(&self, cx: &ContextRef, value: Option<&Value>) -> bool {
        let tv = value.and_then(|v| v.as_type_value());
        if let Some(tv) = tv {
            if builder_of(&tv).map_or(false, |b| b.is_class_builder()) && !self.in_anonymous_function {
                let scope_depth = depth(cx);
                let mut i = scope_depth;
                loop {
                    i -= 1;
                    if i < 0 {
                        break;
                    }
                    let scope = scope_at(cx, i);
                    let sb = builder_of(&scope);
                    if sb.as_ref().map_or(false, |b| b.is_instance_builder()) {
                        return false;
                    }
                    if scope.ptr_eq(&tv) {
                        return true;
                    } else if sb.as_ref().map_or(false, |b| b.is_class_builder()) {
                        return false;
                    }
                }
            }
        }
        false
    }

    fn remove_constant_initializers(&mut self, cx: &ContextRef, def: &NodeRef) {
        let list = {
            let b = def.borrow();
            match b.as_expression_statement() {
                Some(es) => es.expr.clone(),
                None => return,
            }
        };
        let list = nn(&list).clone();
        if !list.borrow().is_list() {
            return;
        }
        let mut i = 0;
        loop {
            let men = {
                let b = list.borrow();
                let items = &b.list().unwrap().items;
                if i >= items.len() {
                    break;
                }
                items[i].clone()
            };
            let v = men.as_ref().and_then(|m| m.borrow().as_member_expression().map(|m| m.ref_.clone())).flatten();
            if let Some(v) = v {
                let s = v.get_slot(&cx.borrow());
                if let Some(s) = s {
                    if s.get_initializer_value().map_or(false, |iv| iv.has_value()) {
                        let empty = Context::get_node_factory(cx).borrow_mut().empty_statement();
                        list.borrow_mut().list_mut().unwrap().items[i] = Some(empty);
                    }
                }
            }
            i += 1;
        }
    }

    pub fn is_finally_present(&self) -> bool {
        self.exception_state.map_or(false, |e| e.finally_present)
    }

    pub fn is_any_finally_present(&self) -> bool {
        self.exception_state.map_or(false, |e| e.any_finally_present)
    }

    pub fn is_inside_finally(&self) -> bool {
        self.exception_state.map_or(false, |e| e.inside_finally)
    }

    fn eval_qualified_member(&mut self, cx: &ContextRef, ident: &NodeRef, is_super: bool, is_attribute: bool, op: MemberOp) {
        let (qexpr, nss, has_qualifier) = qen_parts(ident).unwrap();
        self.ev(cx, &qexpr);
        let uns = self.uns();
        match op {
            MemberOp::Get => {
                if nss.is_some() {
                    self.get_property(false, is_super, is_attribute, nss.as_ref());
                } else if has_qualifier {
                    self.to_string_();
                    self.get_property(true, is_super, is_attribute, uns.as_ref());
                } else {
                    self.get_property(false, is_super, is_attribute, uns.as_ref());
                }
            }
            MemberOp::Delete => {
                if nss.is_some() {
                    self.delete_property(false, is_super, is_attribute, nss.as_ref());
                } else if has_qualifier {
                    self.to_string_();
                    self.delete_property(true, is_super, is_attribute, uns.as_ref());
                } else {
                    self.delete_property(false, is_super, is_attribute, uns.as_ref());
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum MemberOp {
    Get,
    Delete,
}

fn opt_tv_eq(a: &Option<TypeValue>, b: &Option<TypeValue>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.ptr_eq(b),
        (None, None) => true,
        _ => false,
    }
}

impl Emitter for CodeGenerator {
    fn emitter_base(&self) -> &EmitterBase {
        &self.base
    }

    fn emitter_base_mut(&mut self) -> &mut EmitterBase {
        &mut self.base
    }
}

impl Evaluator for CodeGenerator {
    fn check_feature(&mut self, cx: &ContextRef, node: &NodeRef) -> bool {
        let has_input = cx.borrow().input.is_some();
        if has_input {
            let pos = node.borrow().pos();
            self.set_position_at(cx, pos);
        }
        true
    }

    fn evaluate_qualified_identifier(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let qualifier = node.borrow().as_qualified_identifier().unwrap().qualifier.clone();
        if qualifier.is_some() {
            self.ev(cx, &qualifier);
            let q = QName::new(Some(cx.borrow().public_namespace()), "Namespace");
            self.check_type(&q);
        }
        None
    }

    fn evaluate_qualified_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (qualifier, nss_null) = {
            let b = node.borrow();
            let q = b.as_qualified_expression().unwrap();
            (q.qid.qualifier.clone(), q.nss.is_none())
        };
        if qualifier.is_some() && nss_null {
            self.ev(cx, &qualifier);
            let q = QName::new(Some(cx.borrow().public_namespace()), "Namespace");
            self.check_type(&q);
        }
        None
    }

    fn evaluate_this_expression(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        if depth(cx) == 1 {
            self.get_global_scope();
        } else {
            self.load_this();
        }
        None
    }

    fn evaluate_literal_boolean(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (void_result, value) = {
            let b = node.borrow();
            let n = b.as_literal_boolean().unwrap();
            (n.void_result, n.value)
        };
        if !void_result {
            self.push_boolean(value);
        }
        None
    }

    fn evaluate_literal_null(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let void_result = node.borrow().as_literal_null().unwrap().void_result;
        if !void_result {
            self.push_null();
        }
        None
    }

    fn evaluate_literal_number(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (void_result, value, type_id) = {
            let b = node.borrow();
            let n = b.as_literal_number().unwrap();
            (n.void_result, n.numeric_value.clone(), n.type_.as_ref().map(|t| t.get_type_id()))
        };
        if !void_result {
            self.push_number(value.as_ref().expect("NullPointerException"), type_id.expect("NullPointerException"));
        }
        None
    }

    fn evaluate_literal_string(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (void_result, value) = {
            let b = node.borrow();
            let n = b.as_literal_string().unwrap();
            (n.void_result, n.value.clone())
        };
        if !void_result {
            self.push_string(&value);
        }
        None
    }

    fn evaluate_literal_reg_exp(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (void_result, value) = {
            let b = node.borrow();
            let n = b.as_literal_reg_exp().unwrap();
            (n.void_result, n.value.clone())
        };
        if !void_result {
            let namespaces = Namespaces::new();
            namespaces.add(cx.borrow().public_namespace());

            self.find_property_name_nss("RegExp", Some(&namespaces), true, true, false);
            self.get_property_name_nss("RegExp", Some(&namespaces), true, false, false);
            let end_index = java_last_index_of_slash(&value);
            let length = java_len(&value);
            if end_index > 0 && end_index < length - 1 {
                self.push_string(&java_sub(&value, 1, end_index));
                self.push_string(&java_sub(&value, end_index + 1, length));
                self.invoke_closure(true, 2);
            } else {
                self.push_string(&java_sub(&value, 1, length - 1));
                self.invoke_closure(true, 1);
            }
        }
        None
    }

    fn evaluate_literal_object(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (fieldlist, void_result) = {
            let b = node.borrow();
            let n = b.as_literal_object().unwrap();
            (n.fieldlist.clone(), n.void_result)
        };
        if fieldlist.is_some() {
            self.ev(cx, &fieldlist);
        }

        self.new_object(if fieldlist.is_some() { list_size(&fieldlist) } else { 0 });

        if void_result {
            self.pop();
        }
        None
    }

    fn evaluate_literal_field(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (ref_, name, value) = {
            let b = node.borrow();
            let n = b.as_literal_field().unwrap();
            (n.ref_.clone(), n.name.clone(), n.value.clone())
        };
        if let Some(r) = ref_ {
            self.push_string(&r.name());
        } else {
            self.ev(cx, &name);
            self.to_string_();
        }

        self.ev(cx, &value);
        None
    }

    fn evaluate_literal_array(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (elementlist, void_result) = {
            let b = node.borrow();
            let n = b.as_literal_array().unwrap();
            (n.elementlist.clone(), n.void_result)
        };
        let size = if elementlist.is_some() { list_size(&elementlist) } else { 0 };
        if self.c_call_sequence {
            if elementlist.is_some() {
                self.ev(cx, &elementlist);
            }
            self.push_null();
        } else if elementlist.is_some() {
            self.ev(cx, &elementlist);
        }
        self.new_array(size);

        if void_result {
            self.pop();
        }
        None
    }

    fn evaluate_literal_vector(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (elementlist, type_, void_result) = {
            let b = node.borrow();
            let n = b.as_literal_vector().unwrap();
            (n.elementlist.clone(), n.type_.clone(), n.void_result)
        };

        let vec_ns = Namespaces::of(cx.borrow().get_namespace("__AS3__.vec"));
        self.find_property_name_nss("Vector", Some(&vec_ns), true, false, false);
        self.ev(cx, &type_);

        let n_elements = if elementlist.is_none() { 0 } else { list_size(&elementlist) };

        let int_id = cx.borrow().int_type().get_type_id();
        self.push_number(&NumberConstant::Int(n_elements), int_id);
        self.invoke_closure(true, 1);

        if let Some(el) = &elementlist {
            let elements_nss = Namespaces::new();
            elements_nss.add(cx.borrow().public_namespace());

            let size = items_of(el).len();
            for i in 0..size {
                let item = item_at(el, i).expect("IndexOutOfBoundsException");
                self.dup();
                let int_id = cx.borrow().int_type().get_type_id();
                self.push_number(&NumberConstant::Int(i as i32), int_id);
                self.ev(cx, &item);
                self.set_property(false, false, false, Some(&elements_nss), false);
            }
        }

        if void_result {
            self.pop();
        }
        None
    }

    fn evaluate_empty_element(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        self.push_undefined();
        None
    }

    fn evaluate_member_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (ref_, base, selector) = {
            let b = node.borrow();
            let n = b.as_member_expression().unwrap();
            (n.ref_.clone(), n.base.clone(), nn(&n.selector).clone())
        };

        let base_index = ref_.as_ref().map_or(-1, |r| r.get_scope_index_kind(r.get_kind()));

        let sel_is_set = selector.borrow().is_set_expression();
        let sel_is_call_or_apply = {
            let s = selector.borrow();
            s.is_call_expression() || s.is_apply_type_expression()
        };
        let slot = ref_.as_ref().and_then(|r| slot_of(r, cx, if sel_is_set { SET_TOKEN } else { GET_TOKEN }));

        let scope_depth = depth(cx) - 1;

        if let Some(base) = &base {
            let old_in_typeof = self.in_typeof;
            self.in_typeof = false;

            evaluate(base, cx, self);
            let (is_super, is_this) = {
                let b = base.borrow();
                (b.as_super_expression().is_some(), b.as_this_expression().is_some())
            };
            {
                let mut s = selector.borrow_mut();
                let sel = s.selector_mut().expect("ClassCastException");
                sel.set_super(is_super);
                sel.set_this(is_this);
            }
            if let Some(r) = &ref_ {
                r.set_scope_index(-2);
            }
            self.in_typeof = old_in_typeof;
        } else if slot.is_some() && self.is_class_initializer_reference(cx, slot.as_ref().unwrap().get_value().as_ref()) {
            // $cinit referencing the class being initialized (bug #113887)
        } else if base_index == 0 && sel_is_call_or_apply {
            let (is_attribute, is_strict) = {
                let s = selector.borrow();
                let _is_qualified = s.is_qualified();
                (s.is_attribute_identifier(), !s.is_set_expression() && !s.is_delete_expression() && !self.in_typeof)
            };
            let r = ref_.as_ref().unwrap();
            let nss = r.get_immutable_namespaces();
            self.find_property_name_nss(&r.name(), Some(&nss), is_strict, r.is_qualified(), is_attribute);
        } else if base_index == 0 || self.is_local_scope(cx, base_index) {
            // global or local: no base object needed
        } else {
            let (is_strict, is_attribute, sel_ident, sel_expr, sel_is_qualified) = {
                let s = selector.borrow();
                let sel = s.selector().expect("ClassCastException");
                (
                    !s.is_set_expression() && !s.is_delete_expression() && !self.in_typeof,
                    s.is_attribute_identifier(),
                    sel.get_identifier(),
                    sel.expr.clone(),
                    s.is_qualified(),
                )
            };
            if let Some(r) = &ref_ {
                let use_this = cx.borrow().abc_version(TARGET_AVM2) && self.doing_class() && base_index == scope_depth - 1 && {
                    let sc = scope_at(cx, base_index);
                    builder_of(&sc).map_or(false, |b| b.is_instance_builder())
                        && slot.as_ref().map_or(false, |s| opt_obj_eq(&s.declared_by(), &Some(sc.clone())))
                };
                if use_this {
                    self.load_this();
                } else {
                    let nss = r.get_immutable_namespaces();
                    self.find_property_name_nss(&r.name(), Some(&nss), is_strict, r.is_qualified(), is_attribute);
                }
            } else if let Some(ident) = sel_ident {
                self.ev(cx, &sel_expr);

                if qen_parts(&ident).is_some() {
                    let (qexpr, nss, has_qualifier) = qen_parts(&ident).unwrap();
                    self.ev(cx, &qexpr);
                    if nss.is_some() {
                        self.find_property(false, is_attribute, false, nss.as_ref());
                    } else if has_qualifier {
                        self.to_string_();
                        self.find_property(true, is_attribute, true, None);
                    }
                } else {
                    let name = ident.borrow().identifier().unwrap().name.clone();
                    if sel_is_qualified {
                        self.find_property_name(&name, true, is_attribute);
                    } else {
                        let uns = self.uns();
                        self.find_property_name_nss(&name, uns.as_ref(), true, false, is_attribute);
                    }
                }
            }
        }

        evaluate(&selector, cx, self);
        None
    }

    fn evaluate_call_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (is_new, ref_, is_super, is_qualified, is_attribute, is_rvalue, expr, ident, args, void_result) = {
            let b = node.borrow();
            let n = b.as_call_expression().unwrap();
            (
                n.is_new,
                n.sel.ref_.clone(),
                n.sel.is_super(),
                b.is_qualified(),
                b.is_attribute_identifier(),
                n.sel.is_rvalue(),
                n.sel.expr.clone(),
                n.sel.get_identifier(),
                n.args.clone(),
                n.void_result,
            )
        };
        let _ = is_qualified;

        let slot_kind = if is_new { NEW_TOKEN } else { EMPTY_TOKEN };
        let disp_kind;
        let mut call_seq = CALL_Closure;
        let mut this_kind = THIS_undefined;

        let mut method_name = String::new();

        let mut base_index = -1;
        let scope_depth = depth(cx) - 1;

        let mut is_localref = false;

        match &ref_ {
            None => {
                if is_new {
                    call_seq = CALL_EnvThisArgs;
                    disp_kind = DISP_ConstructClosure;
                    this_kind = THIS_None;
                } else if is_rvalue {
                    call_seq = CALL_EnvThisArgs;
                    disp_kind = DISP_CallClosure;
                    this_kind = THIS_Global;
                } else {
                    call_seq = CALL_EnvThisArgs;
                    disp_kind = DISP_CallClosure;
                    this_kind = THIS_Base;
                }
            }
            Some(r) => {
                let slot = slot_of(r, cx, GET_TOKEN);
                let _expr_type = type_of(r, cx);
                base_index = r.get_scope_index_kind(GET_TOKEN);

                let basebui = if base_index >= 0 { builder_of(&scope_at(cx, base_index)) } else { None };

                is_localref = self.is_local_scope(cx, base_index);
                let is_outerfunc = base_index != 0
                    && base_index < self.frame().first_inner_scope
                    && basebui.as_ref().map_or(false, |b| b.is_activation_builder());
                let is_globalref = base_index == 0;
                let is_dotref = base_index == -2;
                let is_lexref = !(is_localref || is_dotref);
                let is_cinit_ref =
                    slot.is_some() && self.is_class_initializer_reference(cx, slot.as_ref().unwrap().get_value().as_ref());

                let mut dk = DISP_undefined;
                if is_cinit_ref {
                    self.load_this();

                    call_seq = CALL_Args;
                    dk = if is_new { DISP_ConstructClosure } else { DISP_CallClosure };
                    this_kind = if is_new { THIS_None } else { THIS_Scope };
                } else if is_localref {
                    call_seq = CALL_EnvThisArgs;
                    dk = DISP_CallClosure;
                    this_kind = THIS_Global;
                } else if is_globalref || is_dotref {
                    call_seq = CALL_Args;
                    dk = if is_new { DISP_ConstructProperty } else { DISP_CallProperty };
                    this_kind = if is_new { THIS_None } else { THIS_Base };
                } else if is_lexref {
                    if is_outerfunc {
                        call_seq = CALL_Args;
                        dk = if is_new { DISP_ConstructProperty } else { DISP_CallPropLex };
                        this_kind = if is_new { THIS_None } else { THIS_Base };
                    } else {
                        call_seq = CALL_Args;
                        dk = if is_new { DISP_ConstructProperty } else { DISP_CallProperty };
                        this_kind = if is_new { THIS_None } else { THIS_Base };
                    }
                } else {
                    cx.borrow().internal_error("missing call case");
                }

                let method_slot = slot_of(r, cx, slot_kind);
                if base_index > 0 {
                    if let Some(ms) = &method_slot {
                        if is_outerfunc && !self.in_with && ms.get_method_id() >= 0 {
                            call_seq = ms.get_call_sequence();
                            method_name = ms.get_method_name();
                            dk = DISP_CallFinal;
                            this_kind = THIS_Global;
                        }
                    }
                }

                this_kind = if is_new { THIS_None } else { this_kind };
                disp_kind = dk;
            }
        }

        let mut temp_this_reg = -1;
        let obj = scope_at(cx, scope_depth);
        let bui = builder_of(&obj).expect("NullPointerException");
        let reg_offset = self.get_register_offset(cx);
        let var_offset = bui.data().var_offset;

        match this_kind {
            THIS_Global | THIS_Scope | THIS_None => {}
            THIS_Base => {
                if (call_seq & PUSH_env) != 0 {
                    this_kind = THIS_Temp;
                    temp_this_reg = self.allocate_temp();
                    self.dup();
                    self.store_register(reg_offset + temp_this_reg, TYPE_none);
                }
            }
            _ => cx.borrow().internal_error("invalid this_kind"),
        }

        let size = list_size(&args);

        if (call_seq & PUSH_env) != 0 {
            match &ref_ {
                None => {
                    if is_rvalue {
                        self.ev(cx, &expr);
                    } else if let Some(ident) = &ident {
                        self.ev(cx, &expr);

                        if qen_parts(ident).is_some() {
                            self.eval_qualified_member(cx, ident, is_super, is_attribute, MemberOp::Get);
                        } else {
                            let name = ident.borrow().identifier().unwrap().name.clone();
                            self.get_property_name(&name, is_super, is_attribute);
                        }
                    } else {
                        self.ev(cx, &expr);
                        let uns = self.uns();
                        self.get_property(is_qualified, is_super, is_attribute, uns.as_ref());
                    }
                }
                Some(r) => {
                    if is_localref {
                        if self.frame().register_scope_index != base_index {
                            self.get_activation_object(base_index);
                            let vi = slot_of(r, cx, GET_TOKEN).expect("NullPointerException").get_var_index();
                            self.load_var(var_offset + vi);
                        } else {
                            let s = slot_of(r, cx, GET_TOKEN).expect("NullPointerException");
                            let vi = s.get_var_index();
                            let s2 = slot_of(r, cx, GET_TOKEN).expect("NullPointerException");
                            self.load_register(reg_offset + vi, slot_type(&s2).get_type_id());
                        }
                    } else {
                        let nss = r.get_immutable_namespaces();
                        self.get_property_name_nss(&r.name(), Some(&nss), r.is_qualified(), is_super, is_attribute);
                    }
                }
            }
        }

        {
            if (call_seq & PUSH_env) != 0 {
                self.get_scope_chain();
            }

            match this_kind {
                THIS_Global => self.get_global_scope(),
                THIS_Scope => self.get_base_object(base_index),
                THIS_Base => {}
                THIS_Temp => self.load_register(reg_offset + temp_this_reg, TYPE_none),
                THIS_None => {}
                _ => cx.borrow().internal_error("invalid this_kind"),
            }

            if (call_seq & PUSH_args) != 0 && args.is_some() {
                self.ev(cx, &args);
            }
        }

        match disp_kind {
            DISP_CallProperty | DISP_CallPropLex | DISP_ConstructProperty => {
                let r = ref_.as_ref().expect("NullPointerException");
                let nss = r.get_immutable_namespaces();
                let name = r.name();
                match disp_kind {
                    DISP_CallProperty => {
                        self.call_property(&name, Some(&nss), size, r.is_qualified(), is_super, is_attribute, false)
                    }
                    DISP_CallPropLex => {
                        self.call_property(&name, Some(&nss), size, r.is_qualified(), is_super, is_attribute, true)
                    }
                    _ => self.construct_property(&name, Some(&nss), size, r.is_qualified(), is_super, is_attribute),
                }
            }
            DISP_CallClosure | DISP_ConstructClosure => self.invoke_closure(is_new, size),
            DISP_CallFinal => {
                let emitter = cx.borrow().get_emitter().expect("NullPointerException");
                let method_info = emitter.borrow_mut().get_method_info(&method_name);
                self.invoke_method(false, method_info, size);
            }
            _ => cx.borrow().internal_error("invalid disp_kind"),
        }

        if void_result {
            self.pop();
        }

        if temp_this_reg != -1 {
            self.free_temp(temp_this_reg);
        }
        None
    }

    fn evaluate_invoke(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (ref_, args, index) = {
            let b = node.borrow();
            let n = b.as_invoke().unwrap();
            (n.sel.ref_.clone(), n.args.clone(), n.index)
        };
        if ref_.as_ref().map_or(true, |r| r.get_scope_index_kind(EMPTY_TOKEN) == 0) {
            self.get_global_scope();
        }

        let size = list_size(&args);
        if args.is_some() {
            self.ev(cx, &args);
        }

        let uns = self.uns();
        self.invoke_unary(index, size - 1, -1, uns.as_ref(), None);
        None
    }

    fn evaluate_set_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (is_qualified, is_attribute, is_super, mode, ref_, expr, is_this, args, void_result, ident, node_is_constinit, pos) = {
            let b = node.borrow();
            let n = b.as_set_expression().unwrap();
            (
                b.is_qualified(),
                b.is_attribute_identifier(),
                n.sel.is_super(),
                n.sel.get_mode(),
                n.sel.ref_.clone(),
                n.sel.expr.clone(),
                n.sel.is_this(),
                n.args.clone(),
                n.void_result,
                n.sel.get_identifier(),
                n.is_constinit,
                b.pos(),
            )
        };

        let mut temp_val_reg = 0;

        if mode == DOUBLEDOT_TOKEN {
            cx.borrow().internal_error("Descendant assigment not yet supported");
        } else if ref_.is_none() {
            let mut reg_offset = -1;
            self.ev(cx, &expr);

            let is_constinit = self.is_ctor && is_this;

            if ident.is_none() && (mode == EMPTY_TOKEN || mode == LEFTBRACKET_TOKEN) {
                self.ev(cx, &args);
                reg_offset = self.get_register_offset(cx);
                if !void_result {
                    temp_val_reg = self.allocate_temp();
                    self.dup();
                    let id = cx.borrow().no_type().get_type_id();
                    self.store_register(reg_offset + temp_val_reg, id);
                }
                let uns = self.uns();
                self.set_property(is_qualified, is_super, is_attribute, uns.as_ref(), is_constinit);
            } else if let Some(ident) = &ident {
                if let Some((qexpr, nss, has_qualifier)) = qen_parts(ident) {
                    self.ev(cx, &qexpr);
                    if has_qualifier {
                        self.to_string_();
                    }

                    self.ev(cx, &args);
                    reg_offset = self.get_register_offset(cx);
                    if !void_result {
                        temp_val_reg = self.allocate_temp();
                        self.dup();
                        let id = cx.borrow().no_type().get_type_id();
                        self.store_register(reg_offset + temp_val_reg, id);
                    }

                    let uns = self.uns();
                    if nss.is_some() {
                        self.set_property(false, is_super, is_attribute, nss.as_ref(), is_constinit);
                    } else if has_qualifier {
                        self.set_property(true, is_super, is_attribute, uns.as_ref(), is_constinit);
                    } else {
                        self.set_property(false, is_super, is_attribute, uns.as_ref(), is_constinit);
                    }
                } else {
                    self.ev(cx, &args);
                    reg_offset = self.get_register_offset(cx);
                    if !void_result {
                        temp_val_reg = self.allocate_temp();
                        self.dup();
                        let id = cx.borrow().no_type().get_type_id();
                        self.store_register(reg_offset + temp_val_reg, id);
                    }

                    let name = ident.borrow().identifier().unwrap().name.clone();
                    self.set_property_name(&name, is_super, is_attribute);
                }
            } else {
                cx.borrow().internal_error_at(pos, "internal error: lhs is not a reference");
            }

            if !void_result {
                let id = cx.borrow().no_type().get_type_id();
                self.load_register(reg_offset + temp_val_reg, id);
                self.free_temp(temp_val_reg);
            }
        } else {
            let r = ref_.as_ref().unwrap();
            let slot = slot_of(r, cx, SET_TOKEN);
            let expr_type = type_of(r, cx);
            let base_index = r.get_scope_index_kind(SET_TOKEN);
            let slot_index = r.get_slot_index(SET_TOKEN);
            let scope_depth = depth(cx) - 1;

            let reg_offset = self.get_register_offset(cx);
            let var_offset = builder_of(&scope_at(cx, scope_depth)).expect("NullPointerException").data().var_offset;

            let bui = if base_index > 0 { builder_of(&scope_at(cx, base_index)) } else { None };

            let is_constinit = if node_is_constinit {
                true
            } else if self.is_ctor && is_this {
                true
            } else {
                bui.as_ref().map_or(false, |b| b.is_instance_builder())
            };
            let is_const = slot.as_ref().map_or(false, |s| s.is_const());

            let is_localref = self.is_local_scope(cx, base_index);
            let is_globalref = base_index == 0;
            let is_dotref = base_index == -2;
            let is_unbound_lexref = base_index == -1;
            let is_unbound_dotref = is_dotref && slot_index < 0;
            let is_unbound_globalref = is_globalref && slot_index < 0;
            let is_unbound_ref = is_unbound_dotref || is_unbound_lexref || is_unbound_globalref;

            if is_unbound_ref {
                if is_globalref {
                    self.get_global_scope();
                }

                self.ev(cx, &args);

                if !void_result {
                    self.dup();
                    temp_val_reg = self.allocate_temp();
                    self.store_register(reg_offset + temp_val_reg, expr_type.get_type_id());
                }
                let nss = r.get_immutable_namespaces();
                self.set_property_name_nss(&r.name(), Some(&nss), r.is_qualified(), is_super, is_attribute, is_constinit);

                if !void_result {
                    self.load_register(reg_offset + temp_val_reg, expr_type.get_type_id());
                    self.free_temp(temp_val_reg);
                }
            } else if is_globalref {
                let slot = slot.as_ref().expect("NullPointerException");
                let mut var_index = slot.get_var_index();
                if !opt_obj_eq(&slot.declared_by(), &Some(scope_at(cx, 0))) {
                    var_index = -1;
                }

                if slot.get_method_id() >= 0 || var_index < 0 {
                    // Java's overload resolution picks FindProperty(name, namespaces, is_strict, is_qualified, is_attr)
                    let nss = r.get_immutable_namespaces();
                    self.find_property_name_nss(&r.name(), Some(&nss), r.is_qualified(), is_super, is_attribute);
                } else if is_const {
                    self.get_base_object(0);
                }

                self.ev(cx, &args);

                if !void_result {
                    self.dup();
                    temp_val_reg = self.allocate_temp();
                    self.store_register(reg_offset + temp_val_reg, expr_type.get_type_id());
                }

                let global = cx.borrow().global_scope();
                if slot.get_method_id() >= 0 && opt_obj_eq(&global, &slot.declared_by()) {
                    let mi = self.get_method_info(&slot.get_method_name());
                    self.invoke_method(false, mi, 1);

                    if void_result {
                        self.pop();
                    }
                } else if var_index >= 0 && !is_const {
                    self.store_global(var_index, expr_type.get_type_id());
                } else {
                    let nss = r.get_immutable_namespaces();
                    self.set_property_name_nss(&r.name(), Some(&nss), r.is_qualified(), is_super, is_attribute, is_constinit);
                }

                if !void_result {
                    self.load_register(reg_offset + temp_val_reg, expr_type.get_type_id());
                    self.free_temp(temp_val_reg);
                }
            } else if is_localref {
                let slot = slot.as_ref().expect("NullPointerException");
                if is_const && !is_constinit {
                    self.push_string(&format!("Illegal write to local const {}", r.name()));
                    self.throw();
                }

                if slot.get_method_id() >= 0 || self.frame().register_scope_index != base_index {
                    self.get_activation_object(base_index);
                }
                self.ev(cx, &args);
                if !void_result {
                    self.dup();
                    temp_val_reg = self.allocate_temp();
                    self.store_register(reg_offset + temp_val_reg, expr_type.get_type_id());
                }

                if slot.get_method_id() >= 0 {
                    self.invoke_method(true, slot.get_method_id(), 1);
                } else {
                    let n = ti_name(&slot_type(slot), cx);
                    self.check_type(&n);
                    if self.frame().register_scope_index != base_index {
                        self.store_var(var_offset + slot.get_var_index());
                    } else {
                        self.store_register_name(reg_offset + slot.get_var_index(), expr_type.get_type_id(), &r.name());
                    }
                }

                if !void_result {
                    self.load_register(reg_offset + temp_val_reg, expr_type.get_type_id());
                    self.free_temp(temp_val_reg);
                }
            } else {
                self.ev(cx, &args);
                if !void_result {
                    self.dup();
                    temp_val_reg = self.allocate_temp();
                    self.store_register(reg_offset + temp_val_reg, expr_type.get_type_id());
                }

                let nss = r.get_immutable_namespaces();
                self.set_property_name_nss(&r.name(), Some(&nss), r.is_qualified(), is_super, is_attribute, is_constinit);

                if !void_result {
                    self.load_register(reg_offset + temp_val_reg, expr_type.get_type_id());
                    self.free_temp(temp_val_reg);
                }
            }
        }
        None
    }

    fn evaluate_delete_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (is_qualified, is_attribute, is_super, mode, ref_, expr, ident, void_result, pos, has_base) = {
            let b = node.borrow();
            let n = b.as_delete_expression().unwrap();
            (
                b.is_qualified(),
                b.is_attribute_identifier(),
                n.sel.is_super(),
                n.sel.get_mode(),
                n.sel.ref_.clone(),
                n.sel.expr.clone(),
                n.sel.get_identifier(),
                n.void_result,
                b.pos(),
                n.sel.base.is_some(),
            )
        };

        if mode == DOUBLEDOT_TOKEN {
            if is_super {
                cx.borrow().error(pos, kError_CannotDeleteSuperDecendants);
            }

            if has_base {
                self.pop();
            }

            if !void_result {
                self.push_boolean(true);
            }
        } else if ref_.is_none() {
            if mode == LEFTBRACKET_TOKEN {
                self.ev(cx, &expr);
                let uns = self.uns();
                self.delete_property(is_qualified, is_super, is_attribute, uns.as_ref());
                if void_result {
                    self.pop();
                }
            } else if let Some(ident) = &ident {
                self.ev(cx, &expr);
                if qen_parts(ident).is_some() {
                    self.eval_qualified_member(cx, ident, is_super, is_attribute, MemberOp::Delete);
                } else {
                    let name = ident.borrow().identifier().unwrap().name.clone();
                    if !has_base {
                        self.find_property_name(&name, true, is_attribute);
                    }
                    self.delete_property_name(&name, is_super, is_attribute);
                }
                if void_result {
                    self.pop();
                }
            } else {
                if !has_base {
                    self.get_global_scope();
                }
                self.ev(cx, &expr);
                let uns = self.uns();
                self.delete_property(is_qualified, is_super, is_attribute, uns.as_ref());
                if void_result {
                    self.pop();
                }
            }
        } else {
            let r = ref_.as_ref().unwrap();
            let base_index = r.get_scope_index_kind(GET_TOKEN);

            if base_index >= self.frame().first_inner_scope {
                if base_index != 0 && !builder_of(&scope_at(cx, base_index)).map_or(false, |b| b.is_activation_builder()) {
                    self.pop();
                }

                if !void_result {
                    self.push_boolean(false);
                }
            } else {
                if base_index == 0 {
                    self.get_global_scope();
                }
                let nss = r.get_immutable_namespaces();
                self.delete_property_name_nss(&r.name(), Some(&nss), r.is_qualified(), is_super, is_attribute);
                if void_result {
                    self.pop();
                }
            }
        }
        None
    }

    fn evaluate_apply_type_expr(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (is_super, is_qualified, is_attribute, ref_, ident, expr, type_args) = {
            let b = node.borrow();
            let n = b.as_apply_type_expr().unwrap();
            (
                n.sel.is_super(),
                b.is_qualified(),
                b.is_attribute_identifier(),
                n.sel.ref_.clone(),
                n.sel.get_identifier(),
                n.sel.expr.clone(),
                n.type_args.clone(),
            )
        };

        if let Some(r) = &ref_ {
            let name = ident.as_ref().expect("NullPointerException").borrow().identifier().unwrap().name.clone();
            let nss = r.get_immutable_namespaces();
            self.get_property_name_nss(&name, Some(&nss), is_qualified, is_super, is_attribute);
        } else if let Some(ident) = &ident {
            self.ev(cx, &expr);

            if qen_parts(ident).is_some() {
                self.eval_qualified_member(cx, ident, is_super, is_attribute, MemberOp::Get);
            } else {
                let name = ident.borrow().identifier().unwrap().name.clone();
                self.get_property_name(&name, is_super, is_attribute);
            }
        } else {
            self.ev(cx, &expr);
        }
        if let Some(ta) = &type_args {
            let size = items_of(ta).len();
            for i in 0..size {
                let (item, val) = {
                    let b = ta.borrow();
                    let l = b.list().expect("ClassCastException");
                    (l.items[i].clone(), l.values.get(i).cloned().expect("IndexOutOfBoundsException"))
                };
                let star = val.and_then(|v| v.as_reference_value()).map_or(false, |r| r.name() == "*");
                if star {
                    self.push_null();
                } else {
                    self.ev(cx, &item);
                }
            }
        }

        let n = items_of(nn(&type_args)).len() as i32;
        self.apply_type(n);
        None
    }

    fn evaluate_get_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (is_qualified, is_attribute, is_super, mode, ref_, expr, ident, is_void_result, pos) = {
            let b = node.borrow();
            let n = b.as_get_expression().unwrap();
            (
                b.is_qualified(),
                b.is_attribute_identifier(),
                n.sel.is_super(),
                n.sel.get_mode(),
                n.sel.ref_.clone(),
                n.sel.expr.clone(),
                n.sel.get_identifier(),
                n.sel.is_void_result(),
                b.pos(),
            )
        };

        if mode == DOUBLEDOT_TOKEN {
            if let Some(r) = &ref_ {
                let nss = r.get_immutable_namespaces();
                self.get_descendants_name_nss(&r.name(), Some(&nss), r.is_qualified(), false, is_attribute);
            } else {
                self.ev(cx, &expr);

                let ident = ident.expect("NullPointerException");
                if let Some((qexpr, _nss, has_qualifier)) = qen_parts(&ident) {
                    self.ev(cx, &qexpr);
                    let uns = self.uns();
                    if has_qualifier {
                        self.to_string_();
                        self.get_descendants(true, is_attribute, uns.as_ref());
                    } else {
                        self.get_descendants(false, is_attribute, uns.as_ref());
                    }
                } else {
                    let name = ident.borrow().identifier().unwrap().name.clone();
                    self.get_descendants_name(&name, is_super, is_attribute);
                }
            }

            if is_void_result {
                self.pop();
            }
        } else if ref_.is_none() {
            if mode == LEFTBRACKET_TOKEN {
                self.ev(cx, &expr);
                let uns = self.uns();
                self.get_property(is_qualified, is_super, is_attribute, uns.as_ref());
            } else {
                self.ev(cx, &expr);

                let ident = ident.expect("NullPointerException");
                if qen_parts(&ident).is_some() {
                    self.eval_qualified_member(cx, &ident, is_super, is_attribute, MemberOp::Get);
                } else {
                    let name = ident.borrow().identifier().unwrap().name.clone();
                    self.get_property_name(&name, is_super, is_attribute);
                }
            }

            if is_void_result {
                self.pop();
            }
        } else {
            let r = ref_.as_ref().unwrap();
            let mut slot = slot_of(r, cx, GET_TOKEN);
            let expr_type = type_of(r, cx);
            let base_index = r.get_scope_index_kind(GET_TOKEN);
            let slot_index = r.get_slot_index(GET_TOKEN);
            let scope_depth = depth(cx) - 1;

            let _basebui = if base_index >= 0 { builder_of(&scope_at(cx, base_index)) } else { None };
            let is_localref = self.is_local_scope(cx, base_index);
            let is_globalref = base_index == 0;
            let is_dotref = base_index == -2;
            let is_unbound_lexref = base_index == -1;
            let is_unbound_dotref = is_dotref && slot_index < 0;
            let is_unbound_globalref = is_globalref && slot_index < 0;
            let is_unbound_ref = is_unbound_dotref || is_unbound_lexref || is_unbound_globalref;

            if is_unbound_ref {
                if slot_of(r, cx, SET_TOKEN).is_some() {
                    cx.borrow().error(pos, kError_PropertyIsWriteOnly);
                }

                if is_globalref {
                    self.get_global_scope();
                }

                let nss = r.get_immutable_namespaces();
                self.get_property_name_nss(&r.name(), Some(&nss), r.is_qualified(), is_super, is_attribute);
                if is_void_result {
                    self.pop();
                }
            } else if is_globalref {
                slot = slot_of(r, cx, GET_TOKEN);
                let slot = slot.as_ref().expect("NullPointerException");
                let global = cx.borrow().global_scope();
                if slot.get_method_id() >= 0 && opt_obj_eq(&global, &slot.declared_by()) {
                    self.get_global_scope();
                    let mi = self.get_method_info(&slot.get_method_name());
                    self.invoke_method(false, mi, 0);
                    if is_void_result {
                        self.pop();
                    }
                } else if value_is_obj(&slot.get_value(), &scope_at(cx, scope_depth)) {
                    self.load_this();
                } else if !is_void_result {
                    let mut var_index = slot.get_var_index();
                    if !opt_obj_eq(&slot.declared_by(), &Some(scope_at(cx, 0))) {
                        var_index = -1;
                    }
                    if var_index >= 0 {
                        self.load_global(var_index, expr_type.get_type_id());
                    } else {
                        let nss = r.get_immutable_namespaces();
                        self.find_property_name_nss(&r.name(), Some(&nss), true, r.is_qualified(), is_attribute);
                        self.get_property_name_nss(&r.name(), Some(&nss), r.is_qualified(), is_super, is_attribute);
                    }
                }
            } else if is_localref {
                let slot = slot.as_ref().expect("NullPointerException");
                let reg_offset = self.get_register_offset(cx);
                let var_offset = builder_of(&scope_at(cx, scope_depth)).expect("NullPointerException").data().var_offset;

                if slot.get_method_id() >= 0 {
                    self.get_activation_object(base_index);
                    let nss = r.get_immutable_namespaces();
                    self.get_property_name_nss(&r.name(), Some(&nss), r.is_qualified(), is_super, is_attribute);
                    if is_void_result {
                        self.pop();
                    }
                } else if !is_void_result {
                    if self.frame().register_scope_index != base_index {
                        self.get_activation_object(base_index);
                        self.load_var(var_offset + slot.get_var_index());
                    } else {
                        self.load_register(reg_offset + slot.get_var_index(), expr_type.get_type_id());
                    }
                }
            } else {
                let nss = r.get_immutable_namespaces();
                self.get_property_name_nss(&r.name(), Some(&nss), r.is_qualified(), is_super, is_attribute);
                if is_void_result {
                    self.pop();
                }
            }
        }
        None
    }

    fn evaluate_increment(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (is_qualified, is_attribute, is_super, ref_, expr, is_postfix, void_result, number_usage, nslot) = {
            let b = node.borrow();
            let n = b.as_increment().unwrap();
            (
                b.is_qualified(),
                b.is_attribute_identifier(),
                n.sel.is_super(),
                n.sel.ref_.clone(),
                n.sel.expr.clone(),
                n.is_postfix,
                n.void_result,
                n.number_usage.clone(),
                n.slot.clone(),
            )
        };
        let _ = is_qualified;
        let nslot = nslot.expect("NullPointerException");

        let mut temp_base_reg = LazyTemp::new();
        let mut temp_name_reg = LazyTemp::new();
        let mut temp_val_reg = LazyTemp::new();
        let mut reg = -1;

        let mut type_ = cx.borrow().no_type().get_default_type_info();

        let scope_depth = depth(cx) - 1;
        let reg_offset = self.get_register_offset(cx);
        let var_offset = builder_of(&scope_at(cx, scope_depth)).expect("NullPointerException").data().var_offset;

        match &ref_ {
            None => {
                self.dup();
                let t = temp_base_reg.get(self);
                self.store_register(reg_offset + t, TYPE_none);
                self.ev(cx, &expr);
                self.dup();
                let t = temp_name_reg.get(self);
                self.store_register(reg_offset + t, TYPE_none);

                let uns = self.uns();
                self.invoke_unary(UNARY_Get, 0, -1, uns.as_ref(), number_usage.as_ref());

                if is_postfix && !void_result {
                    self.to_double(TYPE_double);
                    self.dup();
                }
            }
            Some(r) => {
                let slot = r.get_slot(&cx.borrow());
                type_ = type_of(r, cx);

                let method_id = nslot.get_method_id();
                let already_int = method_id == UNARY_IncrementOp_I
                    || method_id == UNARY_IncrementLocalOp_I
                    || method_id == UNARY_DecrementOp_I
                    || method_id == UNARY_IncrementLocalOp_I;

                let base_index = r.get_scope_index_kind(GET_TOKEN);
                let slot_index = r.get_slot_index(GET_TOKEN);
                if slot_index < 0 || base_index < 0 {
                    if base_index == 0 {
                        self.get_global_scope();
                    }
                    self.dup();
                    let t = temp_base_reg.get(self);
                    self.store_register(reg_offset + t, TYPE_none);
                    let nss = r.get_immutable_namespaces();
                    self.get_property_name_nss(&r.name(), Some(&nss), r.is_qualified(), is_super, is_attribute);
                    if is_postfix && !void_result {
                        if !already_int {
                            self.to_double(TYPE_double);
                        }
                        self.dup();
                    }
                } else if base_index == 0 {
                    let vi = slot.as_ref().expect("NullPointerException").get_var_index();
                    self.load_global(vi, type_.get_type_id());
                    if is_postfix && !void_result {
                        if !already_int {
                            self.to_double(TYPE_double);
                        }
                        self.dup();
                    }
                } else if self.is_local_scope(cx, base_index) {
                    reg = slot_of(r, cx, GET_TOKEN).expect("NullPointerException").get_var_index();
                    if self.frame().register_scope_index != base_index {
                        self.get_activation_object(base_index);
                        let vi = slot_of(r, cx, GET_TOKEN).unwrap().get_var_index();
                        self.load_var(var_offset + vi);
                    } else {
                        let vi = slot_of(r, cx, GET_TOKEN).unwrap().get_var_index();
                        self.load_register(reg_offset + vi, type_.get_type_id());
                    }
                    if is_postfix && !void_result {
                        if !already_int {
                            self.to_double(TYPE_double);
                        }
                        self.dup();
                    }
                } else {
                    self.dup();
                    let t = temp_base_reg.get(self);
                    self.store_register(reg_offset + t, TYPE_none);
                    let nss = r.get_immutable_namespaces();
                    self.get_property_name_nss(&r.name(), Some(&nss), r.is_qualified(), is_super, is_attribute);
                    if is_postfix && !void_result {
                        if !already_int {
                            self.to_double(TYPE_double);
                        }
                        self.dup();
                    }
                }
            }
        }

        let uns = self.uns();
        self.invoke_unary(nslot.get_method_id(), 0, reg_offset + reg, uns.as_ref(), number_usage.as_ref());

        match &ref_ {
            None => {
                if !is_postfix && !void_result {
                    self.dup();
                }

                let t = temp_val_reg.get(self);
                self.store_register(reg_offset + t, type_.get_type_id());

                let t = temp_base_reg.get(self);
                self.load_register(reg_offset + t, TYPE_none);
                let t = temp_name_reg.get(self);
                self.load_register(reg_offset + t, TYPE_none);
                let t = temp_val_reg.get(self);
                self.load_register(reg_offset + t, TYPE_none);
                let uns = self.uns();
                self.invoke_unary(UNARY_Put, 1, -1, uns.as_ref(), number_usage.as_ref());
            }
            Some(r) => {
                let slot = r.get_slot(&cx.borrow());
                let base_index = r.get_scope_index_kind(SET_TOKEN);
                let slot_index = r.get_slot_index(SET_TOKEN);
                if slot_index < 0 || base_index < 0 {
                    if !is_postfix && !void_result {
                        self.dup();
                    }

                    let t = temp_val_reg.get(self);
                    self.store_register(reg_offset + t, type_.get_type_id());

                    let t = temp_base_reg.get(self);
                    self.load_register(reg_offset + t, TYPE_none);
                    let t = temp_val_reg.get(self);
                    self.load_register(reg_offset + t, TYPE_none);
                    let nss = r.get_immutable_namespaces();
                    self.set_property_name_nss(&r.name(), Some(&nss), r.is_qualified(), is_super, is_attribute, false);
                } else if base_index == 0 {
                    if !is_postfix && !void_result {
                        self.dup();
                    }

                    let t = temp_val_reg.get(self);
                    self.store_register(reg_offset + t, type_.get_type_id());

                    let vi = slot.as_ref().expect("NullPointerException").get_var_index();
                    self.pre_store_global(vi, type_.get_type_id());
                    let t = temp_val_reg.get(self);
                    self.load_register(reg_offset + t, type_.get_type_id());
                    self.store_global(vi, type_.get_type_id());
                } else if self.is_local_scope(cx, base_index) {
                    if !is_postfix && !void_result {
                        self.dup();
                    }
                    let n = ti_name(&slot_type(&slot_of(r, cx, GET_TOKEN).expect("NullPointerException")), cx);
                    self.check_type(&n);
                    if self.frame().register_scope_index != base_index {
                        self.get_activation_object(base_index);
                        self.swap();
                        let vi = slot_of(r, cx, GET_TOKEN).unwrap().get_var_index();
                        self.store_var(var_offset + vi);
                    } else {
                        let vi = slot_of(r, cx, GET_TOKEN).unwrap().get_var_index();
                        self.store_register(reg_offset + vi, type_.get_type_id());
                    }
                } else {
                    if !is_postfix && !void_result {
                        self.dup();
                    }

                    let t = temp_val_reg.get(self);
                    self.store_register(reg_offset + t, type_.get_type_id());
                    let t = temp_base_reg.get(self);
                    self.load_register(reg_offset + t, TYPE_none);
                    let t = temp_val_reg.get(self);
                    self.load_register(reg_offset + t, TYPE_none);
                    let nss = r.get_immutable_namespaces();
                    self.set_property_name_nss(&r.name(), Some(&nss), r.is_qualified(), is_super, is_attribute, false);
                }
            }
        }

        temp_val_reg.free(self);
        temp_base_reg.free(self);
        temp_name_reg.free(self);
        None
    }

    fn evaluate_unary_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (op, expr, void_result, slot, number_usage) = {
            let b = node.borrow();
            let n = b.as_unary_expression().unwrap();
            (n.op, n.expr.clone(), n.void_result, n.slot.clone(), n.number_usage.clone())
        };

        match op {
            VOID_TOKEN => {
                if !nn(&expr).borrow().is_literal_number() {
                    nn(&expr).borrow_mut().void_result();
                    self.ev(cx, &expr);
                }
                if !void_result {
                    self.push_undefined();
                }
            }
            _ => {
                let slot = slot.expect("NullPointerException");
                if (slot.get_call_sequence() & PUSH_this) != 0 {
                    self.get_global_scope();
                }
                if (slot.get_call_sequence() & PUSH_opd1) != 0 {
                    if op == TYPEOF_TOKEN {
                        let old_in_typeof = self.in_typeof;
                        self.in_typeof = true;
                        self.ev(cx, &expr);
                        self.in_typeof = old_in_typeof;
                    } else {
                        self.ev(cx, &expr);
                    }
                }
                let uns = self.uns();
                self.invoke_unary(slot.get_method_id(), 0, -1, uns.as_ref(), number_usage.as_ref());
                if void_result {
                    self.pop();
                }
            }
        }
        None
    }

    fn evaluate_binary_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (op, lhs, rhs, slot, lhstype, number_usage, void_result) = {
            let b = node.borrow();
            let n = b.as_binary_expression().unwrap();
            (n.op, n.lhs.clone(), n.rhs.clone(), n.slot.clone(), n.lhstype.clone(), n.number_usage.clone(), n.void_result)
        };

        if op == EMPTY_TOKEN {
            self.ev(cx, &lhs);
        } else {
            let method_id = slot.as_ref().expect("NullPointerException").get_method_id();
            match method_id {
                BINARY_LogicalAndOp_II | BINARY_LogicalAndOp_BB => {
                    self.ev(cx, &lhs);
                    self.dup();
                    self.if_(IF_false);
                    self.pop();
                    self.ev(cx, &rhs);
                    let ip = self.get_ip();
                    self.patch_if(ip);
                }
                BINARY_LogicalAndOp => {
                    self.ev(cx, &lhs);
                    self.dup();
                    self.to_boolean(lhstype.as_ref().map_or(TYPE_none, |t| t.get_type_id()));
                    self.to_native_bool();
                    self.if_(IF_false);
                    self.pop();
                    self.ev(cx, &rhs);
                    let ip = self.get_ip();
                    self.patch_if(ip);
                }
                BINARY_LogicalOrOp_II | BINARY_LogicalOrOp_BB => {
                    self.ev(cx, &lhs);
                    self.dup();
                    self.if_(IF_true);
                    self.pop();
                    self.ev(cx, &rhs);
                    let ip = self.get_ip();
                    self.patch_if(ip);
                }
                BINARY_LogicalOrOp => {
                    self.ev(cx, &lhs);
                    self.dup();
                    self.to_boolean(lhstype.as_ref().map_or(TYPE_none, |t| t.get_type_id()));
                    self.to_native_bool();
                    self.if_(IF_true);
                    self.pop();
                    self.ev(cx, &rhs);
                    let ip = self.get_ip();
                    self.patch_if(ip);
                }
                BINARY_IsOp | BINARY_AsOp => {
                    self.ev(cx, &lhs);
                }
                _ => {
                    if !self.traverse_binop_right_to_left {
                        self.ev(cx, &lhs);
                        self.ev(cx, &rhs);
                    } else {
                        self.ev(cx, &rhs);
                        self.ev(cx, &lhs);
                    }

                    self.invoke_binary(method_id, number_usage.as_ref());
                }
            }
        }

        if void_result {
            self.pop();
        }
        None
    }

    fn evaluate_conditional_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (condition, thenexpr, elseexpr) = {
            let b = node.borrow();
            let n = b.as_conditional_expression().unwrap();
            (n.condition.clone(), n.thenexpr.clone(), n.elseexpr.clone())
        };

        self.if_cx(cx, &condition);
        self.ev(cx, &thenexpr);
        if !nn(&thenexpr).borrow().in_terminal_block() {
            self.else_();
        }

        let ip = self.get_ip();
        self.patch_if(ip);
        self.ev(cx, &elseexpr);

        if !nn(&thenexpr).borrow().in_terminal_block() {
            let ip = self.get_ip();
            self.patch_else(ip);
        }
        None
    }

    fn evaluate_argument_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let size = items_of(node).len();
        if !self.traverse_argslist_right_to_left {
            for i in 0..size {
                let item = item_at(node, i).expect("IndexOutOfBoundsException");
                self.ev(cx, &item);
            }
        } else {
            for i in (0..size).rev() {
                let item = item_at(node, i).expect("IndexOutOfBoundsException");
                self.ev(cx, &item);
            }
        }
        None
    }

    fn evaluate_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let size = items_of(node).len();
        for i in 0..size {
            let item = item_at(node, i).expect("IndexOutOfBoundsException");
            self.ev(cx, &item);
        }
        None
    }

    fn evaluate_statement_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let temp_count = self.get_temp_count();

        let items = items_of(node);
        for item in items {
            if !self.doing_method() {
                let f = self.frame().clone();
                self.start_method(&f.function_name, f.max_params, f.max_locals, f.max_temps, f.activation_is_exposed, f.needs_arguments);
            }
            if let Some(item) = item {
                evaluate(&item, cx, self);
            }
        }

        let mut i = self.get_temp_count();
        while i > temp_count {
            self.free_temp(i - 2);
            i -= 1;
        }
        None
    }

    fn evaluate_empty_statement(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_expression_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, expected_type) = {
            let b = node.borrow();
            let n = b.as_expression_statement().unwrap();
            (n.expr.clone(), n.expected_type.clone())
        };

        if expr.is_some() {
            self.ev(cx, &expr);
            let void_type = cx.borrow().void_type();
            if !tv_is(&expected_type, &void_type) {
                if depth(cx) == 1 && self.temp_cv_reg != -1 {
                    let scope_depth = depth(cx) - 1;
                    let reg_offset = builder_of(&scope_at(cx, scope_depth)).expect("NullPointerException").data().reg_offset;
                    let no_type = cx.borrow().no_type();
                    self.check_type(&no_type.name());
                    self.store_register(reg_offset + self.temp_cv_reg, no_type.get_type_id());
                } else {
                    self.pop();
                }
            }
        }
        None
    }

    fn evaluate_labeled_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (statement, is_loop_label, loop_index) = {
            let b = node.borrow();
            let n = b.as_labeled_statement().unwrap();
            (n.statement.clone(), n.is_loop_label, n.loop_index)
        };

        if statement.is_some() {
            if !is_loop_label {
                self.label_statement_begin();
            }
            self.ev(cx, &statement);
            if !is_loop_label {
                self.label_statement_end(loop_index);
            }
        }
        None
    }

    fn evaluate_if_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (is_true, is_false, condition, thenactions, elseactions) = {
            let b = node.borrow();
            let n = b.as_if_statement().unwrap();
            (n.is_true, n.is_false, n.condition.clone(), n.thenactions.clone(), n.elseactions.clone())
        };

        if is_true {
            if thenactions.is_some() {
                self.ev(cx, &thenactions);
            }
        } else if is_false {
            if elseactions.is_some() {
                self.ev(cx, &elseactions);
            }
        } else {
            self.if_cx(cx, &condition);
            if thenactions.is_some() {
                self.ev(cx, &thenactions);
            }
            let then_terminal = || thenactions.as_ref().map_or(false, |t| t.borrow().in_terminal_block());
            if elseactions.is_some() {
                if thenactions.is_none() || !then_terminal() {
                    self.else_();
                }

                let ip = self.get_ip();
                self.patch_if(ip);
                self.ev(cx, &elseactions);

                if thenactions.is_none() || !then_terminal() {
                    let ip = self.get_ip();
                    self.patch_else(ip);
                }
            } else {
                let ip = self.get_ip();
                self.patch_if(ip);
            }
        }
        None
    }

    fn evaluate_switch_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (statements, expr, loop_index) = {
            let b = node.borrow();
            let n = b.as_switch_statement().unwrap();
            (n.statements.clone(), n.expr.clone(), n.loop_index)
        };

        let reg_offset = self.get_register_offset(cx);

        self.case_exprs.push(Vec::new());
        self.switch_begin();
        if statements.is_some() {
            self.ev(cx, &statements);
        }

        {
            let case_expr = self.case_exprs.last().unwrap();
            let size = case_expr.len();
            let mut i = 0;
            while i < size && case_expr[i].is_some() {
                i += 1;
            }
            if i == size {
                self.case_exprs.last_mut().unwrap().push(None);
                self.case_label(true);
            }
        }

        self.break_(loop_index);
        let ip = self.get_ip();
        self.patch_switch_begin(ip);
        self.ev(cx, &expr);

        {
            let temp_reg = self.allocate_temp();
            self.store_register(reg_offset + temp_reg, TYPE_none);
            let mut case_index: i32 = 0;
            let mut default_index = 0;
            let mut patch_else: Vec<bool> = Vec::new();

            if !self.case_exprs.last().map_or(true, |c| c.is_empty()) {
                let case_exprs_size = self.case_exprs.last().unwrap().len() as i32;
                case_index = 0;
                while case_index < case_exprs_size {
                    let expr = self.case_exprs.last().unwrap()[case_index as usize].clone();
                    if let Some(e) = &expr {
                        evaluate(e, cx, self);
                        self.load_register(reg_offset + temp_reg, TYPE_none);
                        self.if_(IF_strictne);
                    } else {
                        self.push_boolean(false);
                        self.if_(IF_false);
                        default_index = case_index;
                    }
                    self.push_case_index(case_index);
                    if expr.as_ref().map_or(true, |e| !e.borrow().in_terminal_block()) {
                        self.else_();
                        patch_else.push(true);
                    } else {
                        patch_else.push(false);
                    }

                    let ip = self.get_ip();
                    self.patch_if(ip);
                    case_index += 1;
                }
            }
            self.push_case_index(default_index);
            loop {
                let c = case_index;
                case_index -= 1;
                if c == 0 {
                    break;
                }
                if patch_else.last() == Some(&true) {
                    let ip = self.get_ip();
                    self.patch_else(ip);
                }
                patch_else.pop();
            }
            self.case_exprs.pop();
            self.free_temp(temp_reg);
        }
        self.switch_table();
        self.patch_break(loop_index);
        None
    }

    fn evaluate_case_label(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let label = node.borrow().as_case_label().unwrap().label.clone();
        let is_default = label.is_none();
        self.case_exprs.last_mut().expect("NullPointerException").push(label);
        self.case_label(is_default);
        None
    }

    fn evaluate_do_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (statements, expr, loop_index) = {
            let b = node.borrow();
            let n = b.as_do_statement().unwrap();
            (n.statements.clone(), n.expr.clone(), n.loop_index)
        };

        self.loop_begin();
        let ip = self.get_ip();
        self.patch_loop_begin(ip);
        if statements.is_some() {
            self.ev(cx, &statements);
        }
        self.patch_continue(loop_index);
        self.loop_end_cx(cx, &expr);
        self.patch_break(loop_index);
        None
    }

    fn evaluate_while_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (statement, expr, loop_index) = {
            let b = node.borrow();
            let n = b.as_while_statement().unwrap();
            (n.statement.clone(), n.expr.clone(), n.loop_index)
        };

        self.loop_begin();
        if statement.is_some() {
            self.ev(cx, &statement);
        }
        let ip = self.get_ip();
        self.patch_loop_begin(ip);
        self.patch_continue(loop_index);
        self.loop_end_cx(cx, &expr);
        self.patch_break(loop_index);
        None
    }

    fn evaluate_for_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (initialize, statement, increment, test, loop_index) = {
            let b = node.borrow();
            let n = b.as_for_statement().unwrap();
            (n.initialize.clone(), n.statement.clone(), n.increment.clone(), n.test.clone(), n.loop_index)
        };

        let temp_count = self.get_temp_count();

        if initialize.is_some() {
            self.ev(cx, &initialize);
        }
        self.loop_begin();
        if statement.is_some() {
            self.ev(cx, &statement);
        }
        self.patch_continue(loop_index);
        if increment.is_some() {
            self.ev(cx, &increment);
        }
        let ip = self.get_ip();
        self.patch_loop_begin(ip);
        self.loop_end_cx(cx, &test);
        self.patch_break(loop_index);

        let mut i = self.get_temp_count();
        while i > temp_count {
            self.free_temp(i - 2);
            i -= 1;
        }
        None
    }

    fn evaluate_with_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, statement, activation) = {
            let b = node.borrow();
            let n = b.as_with_statement().unwrap();
            (n.expr.clone(), n.statement.clone(), n.activation.clone())
        };

        if expr.is_some() {
            self.ev(cx, &expr);
        }

        let reg_offset = self.get_register_offset(cx);
        let with_object_reg = self.allocate_temp();

        self.dup();
        self.store_register(reg_offset + with_object_reg, TYPE_none);

        let activation = activation.expect("NullPointerException");
        let with_builder = builder_of(&activation).filter(|b| b.is_with_builder()).expect("ClassCastException");
        with_builder.data_mut().temp_reg = with_object_reg;

        cx.borrow().push_scope(activation.clone());

        self.push_with();

        if statement.is_some() {
            let saved_in_with = self.in_with;
            self.in_with = true;

            let save_with_depth = cx.borrow().statics.borrow().with_depth;
            let d = depth(cx) - 1;
            cx.borrow().statics.borrow_mut().with_depth = d;

            self.ev(cx, &statement);

            self.in_with = saved_in_with;

            cx.borrow().statics.borrow_mut().with_depth = save_with_depth;
        }

        self.pop_with();

        cx.borrow().pop_scope();

        self.free_temp(with_object_reg);
        None
    }

    fn evaluate_continue_statement(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let loop_index = node.borrow().as_continue_statement().unwrap().loop_index;
        self.continue_(loop_index);
        None
    }

    fn evaluate_break_statement(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let loop_index = node.borrow().as_break_statement().unwrap().loop_index;
        self.break_(loop_index);
        None
    }

    fn evaluate_return_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = node.borrow().as_return_statement().unwrap().expr.clone();

        if expr.is_some() {
            self.ev(cx, &expr);
        }

        if self.is_any_finally_present() {
            let top = builder_of(&scope_top(cx));
            if let Some(cb) = top.filter(|b| b.is_catch_builder()) {
                let temp_reg = cb.data().temp_reg;
                self.pop_scope();
                self.kill(temp_reg);
            }

            let mut reg_offset = -1;
            let mut temp_index_reg = -1;
            if expr.is_some() {
                reg_offset = self.get_register_offset(cx);
                temp_index_reg = self.allocate_temp();
                let n = cx.borrow().no_type().name();
                self.check_type(&n);
                self.store_register(reg_offset + temp_index_reg, TYPE_none);
            }
            self.call_finally(-1);

            if expr.is_some() {
                self.load_register(reg_offset + temp_index_reg, TYPE_none);
                self.free_temp(temp_index_reg);
            }
        }
        if expr.is_some() {
            self.return_(TYPE_none);
        } else {
            self.return_(TYPE_void);
        }
        None
    }

    fn evaluate_throw_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = node.borrow().as_throw_statement().unwrap().expr.clone();

        if expr.is_some() {
            self.ev(cx, &expr);
        }

        if self.is_finally_present() && !self.exception_state.unwrap().ignore_throws {
            let top = builder_of(&scope_top(cx));
            if let Some(cb) = top.filter(|b| b.is_catch_builder()) {
                let temp_reg = cb.data().temp_reg;
                self.pop_scope();
                self.kill(temp_reg);
            }

            let reg_offset = self.get_register_offset(cx);
            let temp_index_reg = self.allocate_temp();
            let n = cx.borrow().no_type().name();
            self.check_type(&n);
            self.store_register(reg_offset + temp_index_reg, TYPE_none);

            self.call_finally(1);

            self.load_register(reg_offset + temp_index_reg, TYPE_none);
            self.free_temp(temp_index_reg);
        }

        self.throw();
        None
    }

    fn evaluate_try_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (tryblock, catchlist, finallyblock) = {
            let b = node.borrow();
            let n = b.as_try_statement().unwrap();
            (n.tryblock.clone(), n.catchlist.clone(), n.finallyblock.clone())
        };

        let inside_finally = self.is_inside_finally();
        let save_exception_state = self.exception_state;
        self.exception_state = Some(ExceptionState::default());

        if let Some(s) = save_exception_state {
            self.exception_state.as_mut().unwrap().any_finally_present = s.any_finally_present;
        }

        if finallyblock.is_some() {
            self.exception_state.as_mut().unwrap().any_finally_present = true;
            self.try_(true);
        }

        self.try_(false);

        let reg_offset = self.get_register_offset(cx);
        let mut temp_index_reg = -2;

        self.exception_state.as_mut().unwrap().ignore_throws = true;
        if tryblock.is_some() {
            if inside_finally {
                temp_index_reg = self.allocate_temp();
                let n = cx.borrow().no_type().name();
                self.check_type(&n);
                self.store_register(reg_offset + temp_index_reg, TYPE_int);
            }

            self.ev(cx, &tryblock);
        }

        self.catch_clauses_begin();

        if catchlist.is_some() {
            self.ev(cx, &catchlist);
        }

        self.catch_clauses_end();

        self.exception_state.as_mut().unwrap().ignore_throws = false;

        self.finally_clause_begin();
        self.finally_clause_end();

        if let Some(fb) = &finallyblock {
            self.push_number(&NumberConstant::Int(-1), TYPE_int);

            self.catch_clauses_begin();

            self.exception_state.as_mut().unwrap().finally_present = true;
            let default_catch = fb.borrow().as_finally_clause().expect("ClassCastException").default_catch.clone();
            self.ev(cx, &default_catch);

            self.catch_clauses_end();

            self.finally_clause_begin();

            {
                let es = self.exception_state.as_mut().unwrap();
                es.finally_present = false;
                es.inside_finally = true;
            }
            evaluate(fb, cx, self);

            self.finally_clause_end();
        }

        if temp_index_reg > -2 {
            self.load_register(reg_offset + temp_index_reg, TYPE_int);
            let n = cx.borrow().int_type().name();
            self.check_type(&n);
            self.free_temp(temp_index_reg);
        }

        self.exception_state = save_exception_state;
        None
    }

    fn evaluate_catch_clause(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (activation, parameter, statements) = {
            let b = node.borrow();
            let n = b.as_catch_clause().unwrap();
            (n.activation.clone().expect("NullPointerException"), n.parameter.clone(), n.statements.clone())
        };

        let reg_offset = self.get_register_offset(cx);

        let bui = builder_of(&activation).expect("NullPointerException");
        let var_offset = bui.data().var_offset;

        let mut slot = None;
        let type_;
        let mut qname = None;

        cx.borrow().push_scope(activation.clone());

        let param_ref = parameter.as_ref().and_then(|p| p.borrow().parameter().map(|p| p.ref_.clone()));
        if let Some(pref) = param_ref {
            let pref = pref.expect("NullPointerException");
            let s = slot_of(&pref, cx, GET_TOKEN).expect("NullPointerException");
            type_ = slot_type(&s);
            slot = Some(s);

            qname = Some(QName::new(Some(cx.borrow().public_namespace()), &pref.name()));
        } else {
            type_ = cx.borrow().no_type().get_default_type_info();
        }

        let tv = type_.get_type_value();
        self.catch(tv.as_ref(), qname.as_ref());

        if self.frame().with_this {
            self.load_this();
            self.push_scope();
        }

        let first = self.frame().first_inner_scope;
        let n = depth(cx);
        let mut i = first;
        while i < n {
            let builder = builder_of(&scope_at(cx, i)).expect("NullPointerException");
            let temp_reg = builder.data().temp_reg;
            if temp_reg != -1 {
                self.load_register(temp_reg + reg_offset, TYPE_none);
                if builder.is_with_builder() {
                    self.push_with();
                } else {
                    self.push_scope();
                }
            }
            i += 1;
        }

        let catch_index = self.frame().catch_index;
        self.new_catch(catch_index);
        self.dup();

        let temp_activation_reg = self.allocate_temp();
        bui.data_mut().temp_reg = temp_activation_reg;
        self.store_register(reg_offset + temp_activation_reg, TYPE_none);

        if qname.is_some() {
            self.dup();
        }

        self.push_scope();

        if qname.is_some() {
            self.swap();
            self.store_var(slot.as_ref().unwrap().get_var_index() + var_offset);
        }

        self.frame_mut().catch_index += 1;

        if statements.is_some() {
            self.ev(cx, &statements);
        }

        if self.is_finally_present() {
            self.push_number(&NumberConstant::Int(-1), TYPE_int);
        }

        self.pop_scope();
        self.free_temp(temp_activation_reg);

        cx.borrow().pop_scope();
        None
    }

    fn evaluate_finally_clause(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let statements = node.borrow().as_finally_clause().unwrap().statements.clone();
        self.ev(cx, &statements);
        None
    }

    fn evaluate_variable_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (pkgdef, list) = {
            let b = node.borrow();
            let n = b.as_variable_definition().unwrap();
            (n.def.pkgdef.clone(), n.list.clone())
        };

        let pkg_used = pkgdef.as_ref().map(|p| p.borrow().as_package_definition().expect("ClassCastException").used_namespaces.clone());
        if pkg_used.is_some() && depth(cx) == 1 {
            self.used_namespaces_sets.push(pkg_used.clone());
        }

        for it in items_of(nn(&list)) {
            let binding = it.as_ref().and_then(|n| {
                let b = n.borrow();
                b.as_variable_binding().map(|v| (v.ref_.clone(), v.debug_name.clone(), b.pos()))
            });
            if let Some((ref_, debug_name, bpos)) = binding {
                let Some(r) = ref_ else { continue };
                let Some(slot) = r.get_slot(&cx.borrow()) else { continue };
                if slot.get_var_index() < 0 {
                    continue;
                }
                let on_activation_object = self.frame().activation_is_exposed;
                let expr_type = type_of(&r, cx);
                let base_index = r.get_scope_index_kind(GET_TOKEN);

                let basebui = if base_index >= 0 { builder_of(&scope_at(cx, base_index)) } else { None };
                let is_globalref = base_index == 0;

                let is_inherited_slot = !opt_obj_eq(&slot.declared_by(), &Some(scope_at(cx, base_index)));

                let is_instance_init = basebui.as_ref().map_or(false, |b| b.is_instance_builder())
                    && builder_of(&scope_top(cx)).map_or(false, |b| b.is_activation_builder());

                if !is_globalref && !is_inherited_slot && !on_activation_object && !is_instance_init && basebui.is_some() {
                    self.set_position_at(cx, bpos);
                    self.define_slot_variable(
                        cx,
                        &r.name(),
                        debug_name.as_deref().unwrap_or(""),
                        bpos,
                        Some(&expr_type),
                        slot.get_var_index(),
                    );
                }
            }
        }

        if pkg_used.is_some() && depth(cx) == 1 {
            self.used_namespaces_sets.pop();
        }
        None
    }

    fn evaluate_binary_function_definition(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_function_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (pkgdef, needs_init, init) = {
            let b = node.borrow();
            let n = b.as_function_definition().unwrap();
            (n.def.pkgdef.clone(), n.needs_init, n.init.clone())
        };

        let pkg_used = pkgdef.as_ref().map(|p| p.borrow().as_package_definition().expect("ClassCastException").used_namespaces.clone());
        if pkg_used.is_some() && depth(cx) == 1 {
            self.used_namespaces_sets.push(pkg_used.clone());
        }

        let builder = builder_of(&scope_top(cx));
        let class_method = builder.as_ref().map_or(false, |b| b.is_class_builder() || b.is_instance_builder());

        if !class_method && needs_init {
            self.ev(cx, &init);
            node.borrow_mut().as_function_definition_mut().unwrap().needs_init = false;
        }

        if pkg_used.is_some() && depth(cx) == 1 {
            self.used_namespaces_sets.pop();
        }
        None
    }

    fn evaluate_function_common(&mut self, _unused_cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let cx = node.borrow().as_function_common().unwrap().cx.clone().expect("NullPointerException");
        let cx = &cx;

        let origin = cx.borrow().get_qualified_error_origin();
        self.set_origin(&origin);

        let pos = node.borrow().pos();
        self.set_position_at(cx, pos);

        if self.doing_method() {
            let (anon_with_identifier, internal_name, is_void_result, identifier) = {
                let b = node.borrow();
                let n = b.as_function_common().unwrap();
                (n.is_named_inner_func(), n.internal_name.clone(), n.is_void_result(), n.identifier.clone())
            };
            if anon_with_identifier {
                self.new_object(0);
                self.push_with();
            }
            self.new_function_object(&internal_name);

            if anon_with_identifier {
                if !is_void_result {
                    self.dup();
                }
                let d = cx.borrow().get_scope_depth() as i32;
                let f = self.frame().first_inner_scope;
                self.get_base_object(d - f);
                self.swap();
                let nss = Namespaces::of(cx.borrow().public_namespace());
                self.set_property_name_nss(&ident_name(&identifier), Some(&nss), true, false, false, false);
                self.pop_scope();
            } else if is_void_result {
                self.pop();
            }

            return None;
        }

        let (with_depth, scope_chain, ref_, fun, fexprs, is_with_used, is_exceptions_used, used_namespaces) = {
            let b = node.borrow();
            let n = b.as_function_common().unwrap();
            (
                n.with_depth,
                n.scope_chain.clone(),
                n.ref_.clone().expect("NullPointerException"),
                n.fun.clone().expect("NullPointerException"),
                node_list(&n.fexprs),
                n.is_with_used(),
                n.is_exceptions_used(),
                n.used_namespaces.clone(),
            )
        };

        let saved_with_depth = cx.borrow().statics.borrow().with_depth;
        if with_depth != -1 {
            cx.borrow().statics.borrow_mut().with_depth = with_depth;
        }

        let saved_scopes = scope_chain.map(|sc| cx.borrow().swap_scope_chain(sc));

        let get_slot = slot_of(&ref_, cx, GET_TOKEN);
        let set_slot = slot_of(&ref_, cx, SET_TOKEN);
        let with_this =
            get_slot.as_ref().map_or(false, |s| s.is_method_slot()) || set_slot.as_ref().map_or(false, |s| s.is_method_slot());

        cx.borrow().push_scope(fun.activation().expect("NullPointerException"));

        let mut needs_activation = false;

        if !fexprs.is_empty() {
            needs_activation = true;
        }
        if is_with_used {
            needs_activation = true;
        }
        if is_exceptions_used {
            needs_activation = true;
        }

        self.used_namespaces_sets.push(used_namespaces);

        for def in &fexprs {
            evaluate(def, cx, self);
        }

        self.set_position_at(cx, pos);

        self.push_stack_frame();

        let (internal_name, signature, body, var_count, temp_count, needs_arguments, debug_name, default_dxns, is_fundef, is_native) = {
            let b = node.borrow();
            let n = b.as_function_common().unwrap();
            (
                n.internal_name.clone(),
                nn(&n.signature).clone(),
                n.body.clone(),
                n.var_count,
                n.temp_count,
                n.needs_arguments,
                n.debug_name.clone(),
                n.default_dxns.clone(),
                n.is_function_definition(),
                n.is_native(),
            )
        };
        let sig_size = signature.borrow().as_function_signature().expect("ClassCastException").size();

        {
            let d = depth(cx);
            let f = self.frame_mut();
            f.function_name = internal_name.clone();
            f.max_params = sig_size;
            f.max_locals = if body.is_some() { var_count } else { 0 };
            f.max_temps = if body.is_some() { temp_count } else { 0 };
            f.needs_arguments = needs_arguments;

            f.with_this = with_this;

            f.first_inner_scope = d - 1;
            if with_this {
                f.first_inner_scope -= 1;
            }

            f.activation_is_exposed = needs_activation;
            f.register_scope_index = if needs_activation { -1 } else { d - 1 };
        }

        {
            let f = self.frame().clone();
            self.start_method(&f.function_name, f.max_params, f.max_locals, 0, needs_activation, needs_arguments);
        }

        let es4_nullability = cx.borrow().statics.borrow().es4_nullability;
        let is_construct = ref_.name() == "$construct";
        if is_construct && es4_nullability {
            self.do_ctor_setup(node, cx, needs_activation);
        }
        if with_this {
            self.load_this();
            self.push_scope();
        }

        let mut activation = fun.activation();
        let act = activation.clone().expect("NullPointerException");
        let mut firstlocal = sig_size;
        if needs_arguments != 0 {
            firstlocal += 1;
        }
        let act_builder = builder_of(&act).expect("NullPointerException");
        let reg_offset = act_builder.data().reg_offset;
        let var_offset = act_builder.data().var_offset;

        if needs_activation {
            self.new_activation();
            let temp_activation_reg = self.allocate_temp();
            act_builder.data_mut().temp_reg = temp_activation_reg;
            self.dup();
            self.store_register(reg_offset + temp_activation_reg, TYPE_none);
            self.push_scope();

            let proto_type = ObjectValue::object_prototype().type_();
            self.define_slot_variable(cx, &internal_name, &debug_name, pos, proto_type.as_ref(), temp_activation_reg);
        }

        if let Some(slots) = act.slots() {
            let base_offset = if needs_activation { var_offset } else { reg_offset };

            for s in &slots {
                let index = base_offset + s.get_var_index();
                if s.needs_init() && s.get_var_index() >= firstlocal {
                    self.store_default_value(cx, index, s, needs_activation);
                }
            }
        }

        if needs_activation {
            let mut n = self.frame().max_params;
            if needs_arguments != 0 {
                n += 1;
            }
            for i in 0..n {
                let d = depth(cx);
                self.get_activation_object(d - 1);
                self.load_register(i + 1, TYPE_object);
                self.store_var(i);
            }
        }

        let (parms, sig_type, sig_pos) = {
            let s = signature.borrow();
            let sig = s.as_function_signature().unwrap();
            (sig.parameter.clone(), sig.type_.clone(), s.pos())
        };
        let mut arg_names: Option<Vec<String>> = None;
        if let Some(parms) = &parms {
            let items = items_of(parms);
            let mut names = vec![String::new(); items.len()];
            for (i, parm) in items.iter().enumerate() {
                let r = parm.as_ref().and_then(|p| p.borrow().parameter().map(|p| p.ref_.clone())).flatten();
                let Some(r) = r else { continue };
                let Some(slot) = r.get_slot(&cx.borrow()) else { continue };
                let expr_type = type_of(&r, cx);
                let epos = self.base.pos;
                self.define_slot_variable(cx, &r.name(), &r.name(), epos, Some(&expr_type), slot.get_var_index());
                names[i] = r.name();
            }
            arg_names = Some(names);
        }

        if is_construct && !es4_nullability {
            self.do_ctor_setup(node, cx, needs_activation);
        }

        if body.is_some() {
            if default_dxns.is_some() {
                self.ev(cx, &default_dxns);
            }
            let old_in_anonymous_function = self.in_anonymous_function;
            self.in_anonymous_function = !is_fundef;
            self.ev(cx, &body);
            self.in_anonymous_function = old_in_anonymous_function;
        }

        self.set_position_at(cx, sig_pos);

        if !needs_activation {
            activation = None;
        }

        let scope_depth = if activation.is_some() { depth(cx) } else { depth(cx) - 1 };
        let types = parms.as_ref().map(|p| p.borrow().as_parameter_list().expect("ClassCastException").types.clone());
        let is_interface = self.current_class.as_ref().map_or(false, |c| c.borrow().as_interface_definition().is_some());
        let function_name = self.frame().function_name.clone();
        let method_info = self.finish_method(
            cx,
            &function_name,
            sig_type.as_ref(),
            types.as_deref(),
            activation.as_ref(),
            needs_arguments,
            scope_depth,
            &debug_name,
            is_native,
            is_interface,
            arg_names.as_deref(),
        );
        fun.set_method_info(method_info);

        cx.borrow().pop_scope();
        self.is_ctor = false;
        fun.set_activation(None);
        fun.set_builder(None);

        self.pop_stack_frame();

        if let Some(saved) = saved_scopes {
            cx.borrow().swap_scope_chain(saved);
        }
        cx.borrow().statics.borrow_mut().with_depth = saved_with_depth;

        self.used_namespaces_sets.pop();
        None
    }

    fn evaluate_program(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (used_def_namespaces, fexprs, clsdefs, var_count, temp_count, statements, pkgdefs) = {
            let b = node.borrow();
            let n = b.as_program().unwrap();
            (
                n.used_def_namespaces.clone(),
                n.fexprs.clone(),
                node_list(&n.clsdefs),
                n.var_count,
                n.temp_count,
                n.statements.clone(),
                n.pkgdefs.clone(),
            )
        };

        self.used_namespaces_sets.push(Some(used_def_namespaces));

        let origin = cx.borrow().get_error_origin();
        self.set_origin(&origin);

        let name = self.get_program_name(cx);
        self.start_program(&name);

        let n = fexprs.as_ref().map_or(0, |f| f.borrow().len()) as i32;
        let mut i = n - 1;
        while i >= 0 {
            let fexpr = fexprs.as_ref().unwrap().borrow()[i as usize].clone();
            evaluate(&fexpr, cx, self);
            i -= 1;
        }

        for def in clsdefs {
            evaluate(&def, cx, self);
            let cframe = def.borrow().class_definition().expect("ClassCastException").cframe.clone();
            if let Some(cframe) = cframe {
                scope_top(cx).deferred_class_map_put(cframe, def.clone());
            }
        }

        self.push_stack_frame();

        {
            let d = depth(cx);
            let f = self.frame_mut();
            f.function_name = "$init".to_string();
            f.max_params = 0;
            f.max_locals = var_count;
            f.max_temps = temp_count;

            f.activation_is_exposed = false;
            f.with_this = true;
            f.first_inner_scope = d - 1;
            f.register_scope_index = -1;
        }

        {
            let f = self.frame().clone();
            self.start_method_counts(&f.function_name, f.max_params, f.max_locals);
        }
        self.clear_position_info();
        let (defines_cv, was_empty) = {
            let b = nn(&statements).borrow();
            let s = b.as_statement_list().expect("ClassCastException");
            (s.defines_cv(), s.was_empty)
        };
        if defines_cv && !was_empty {
            self.temp_cv_reg = self.allocate_temp();
        } else {
            self.temp_cv_reg = -1;
        }

        self.load_this();
        self.push_scope();

        let mut started_method = false;

        if statements.is_some() {
            for def in &pkgdefs {
                evaluate(def, cx, self);
            }

            self.ev(cx, &statements);
        } else {
            let f = self.frame().clone();
            self.start_method_counts(&f.function_name, f.max_params, f.max_locals);
            started_method = true;
        }

        let reg_offset = self.get_register_offset(cx);
        if self.temp_cv_reg != -1 {
            let id = cx.borrow().no_type().get_type_id();
            self.load_register(reg_offset + self.temp_cv_reg, id);
            self.return_(TYPE_none);
            self.free_temp(self.temp_cv_reg);
        } else {
            self.return_(TYPE_void);
        }

        if started_method {
            self.clear_position_info();
        }

        self.set_position(0, 0, 0);
        let d = depth(cx);
        let init_info = self.finish_method(cx, "$init", None, None, None, 0, d, "", false, false, None);

        let name = self.get_program_name(cx);
        self.finish_program(cx, &name, init_info);

        self.used_namespaces_sets.pop();

        self.pop_stack_frame();
        None
    }

    fn evaluate_package_definition(&mut self, _unused_cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_node(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_identifier(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_variable_binding(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_to_object(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = node.borrow().as_to_object().unwrap().expr.clone();
        self.ev(cx, &expr);
        self.to_object();
        None
    }

    fn evaluate_load_register(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (reg, void_result, type_id) = {
            let b = node.borrow();
            let n = b.as_load_register().unwrap();
            (n.reg.clone(), n.void_result, n.type_.as_ref().map(|t| t.get_type_id()))
        };
        self.ev(cx, &reg);
        let reg_offset = self.get_register_offset(cx);
        if !void_result {
            let index = nn(&reg).borrow().as_register().expect("ClassCastException").index;
            self.load_register(reg_offset + index, type_id.expect("NullPointerException"));
        }
        None
    }

    fn evaluate_store_register(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (reg, expr, void_result, type_id) = {
            let b = node.borrow();
            let n = b.as_store_register().unwrap();
            (n.reg.clone(), n.expr.clone(), n.void_result, n.type_.as_ref().map(|t| t.get_type_id()))
        };
        self.ev(cx, &reg);
        let reg_offset = self.get_register_offset(cx);
        self.ev(cx, &expr);
        if !void_result {
            self.dup();
        }
        let index = nn(&reg).borrow().as_register().expect("ClassCastException").index;
        self.store_register(reg_offset + index, type_id.expect("NullPointerException"));
        None
    }

    fn evaluate_register(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let index = node.borrow().as_register().unwrap().index;
        if index < 0 {
            let t = self.allocate_temp();
            node.borrow_mut().as_register_mut().unwrap().index = t;
        }
        None
    }

    fn evaluate_has_next(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (object_register, index_register) = {
            let b = node.borrow();
            let n = b.as_has_next().unwrap();
            (n.object_register.clone(), n.index_register.clone())
        };
        self.ev(cx, &object_register);
        self.ev(cx, &index_register);
        let reg_offset = self.get_register_offset(cx);
        let oi = nn(&object_register).borrow().as_register().expect("ClassCastException").index;
        let ii = nn(&index_register).borrow().as_register().expect("ClassCastException").index;
        self.has_next(reg_offset + oi, reg_offset + ii);
        None
    }

    fn evaluate_box(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, void_result, actual) = {
            let b = node.borrow();
            let n = b.as_box().unwrap();
            (n.expr.clone(), n.void_result, n.actual.clone())
        };
        self.ev(cx, &expr);

        if !void_result {
            let type_id = actual.as_ref().expect("NullPointerException").get_type_id();
            match type_id {
                TYPE_bool => self.to_boolean(type_id),
                TYPE_int => self.to_double(type_id),
                _ => {}
            }
        }
        None
    }

    fn evaluate_coerce(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, void_result, is_explicit, expected, actual) = {
            let b = node.borrow();
            let n = b.as_coerce().unwrap();
            (n.expr.clone(), n.void_result, n.is_explicit, n.expected.clone(), n.actual.clone())
        };
        self.ev(cx, &expr);

        if !void_result {
            let expected = expected.expect("NullPointerException");
            let actual_tv = actual.as_ref().map(|a| a.get_type_value());
            let actual_is = |t: &TypeValue| actual_tv.as_ref().map_or(false, |a| tv_is(a, t));
            if is_explicit {
                let type_id = expected.get_type_id();
                match type_id {
                    TYPE_uint => {
                        let uint_t = cx.borrow().uint_type();
                        if actual.is_none() || !actual_is(&uint_t) {
                            self.to_uint();
                        }
                    }
                    TYPE_int => {
                        let int_t = cx.borrow().int_type();
                        if actual.is_none() || !actual_is(&int_t) {
                            self.to_int();
                        }
                    }
                    TYPE_double | TYPE_number => {
                        let (dbl_t, num_t) = (cx.borrow().double_type(), cx.borrow().number_type());
                        if actual.is_none() || !(actual_is(&dbl_t) || actual_is(&num_t)) {
                            self.to_double(actual.as_ref().map_or(TYPE_none, |a| a.get_type_id()));
                        }
                    }
                    TYPE_decimal => {
                        let es4_numerics = cx.borrow().statics.borrow().es4_numerics;
                        let dec_t = cx.borrow().decimal_type();
                        let is_dec = actual_tv.as_ref().map_or(false, |a| opt_tv_eq(a, &dec_t));
                        if es4_numerics && (actual.is_none() || !is_dec) {
                            self.to_decimal(actual.as_ref().map_or(TYPE_none, |a| a.get_type_id()));
                        }
                    }
                    TYPE_boolean => {
                        let bool_t = cx.borrow().boolean_type();
                        if actual.is_none() || !actual_is(&bool_t) {
                            self.to_boolean(actual.as_ref().map_or(TYPE_none, |a| a.get_type_id()));
                        }
                    }
                    _ => {
                        let n = ti_name(&expected, cx);
                        self.check_type(&n);
                    }
                }
            } else {
                let n = ti_name(&expected, cx);
                self.check_type(&n);
            }
        }
        None
    }

    fn evaluate_class_definition(&mut self, _unused_cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (has_intrinsic, cx) = {
            let b = node.borrow();
            let n = b.class_definition().unwrap();
            let has_intrinsic =
                n.def.attrs.as_ref().map_or(false, |a| a.borrow().as_attribute_list().map_or(false, |a| a.has_intrinsic));
            (has_intrinsic, n.cx.clone())
        };
        if has_intrinsic {
            return None;
        }

        let cx = cx.expect("NullPointerException");
        let cx = &cx;

        let origin = cx.borrow().get_error_origin();
        self.set_origin(&origin);

        let cframe = node.borrow().class_definition().unwrap().cframe.clone().expect("NullPointerException");

        if self.doing_method() || self.doing_class() {
            let (needs_init, init, baseclass) = {
                let b = node.borrow();
                let n = b.class_definition().unwrap();
                (n.needs_init, n.init.clone(), n.baseclass.clone())
            };
            if needs_init {
                let base_class_node = cframe.baseclass().and_then(|bc| scope_top(cx).deferred_class_map_get(&bc));
                if let Some(bcn) = &base_class_node {
                    let (bneeds_init, bname) = {
                        let b = bcn.borrow();
                        let n = b.class_definition().expect("ClassCastException");
                        (n.needs_init, ident_name(&n.name))
                    };
                    if bneeds_init {
                        let name = ident_name(&node.borrow().class_definition().unwrap().name);
                        if name == "Object" && bname == "Class" {
                            // special case: not deferred
                        } else {
                            {
                                let mut b = bcn.borrow_mut();
                                let n = b.class_definition_mut().unwrap();
                                if n.deferred_subclasses.is_none() {
                                    n.deferred_subclasses = Some(new_node_list());
                                }
                            }
                            let bpos = nn(&baseclass).borrow().pos();
                            cx.borrow().error1(bpos, kError_ForwardReferenceToBaseClass, &bname);
                            let ds = bcn.borrow().class_definition().unwrap().deferred_subclasses.clone().unwrap();
                            ds.borrow_mut().push(node.clone());
                            return None;
                        }
                    }
                }

                node.borrow_mut().class_definition_mut().unwrap().needs_init = false;
                self.ev(cx, &init);

                let deferred = node.borrow().class_definition().unwrap().deferred_subclasses.clone();
                if let Some(ds) = deferred {
                    let list = ds.borrow().clone();
                    for n in list {
                        evaluate(&n, cx, self);
                    }
                    node.borrow_mut().class_definition_mut().unwrap().deferred_subclasses = None;
                }

                return None;
            }

            let mut pop_count = 0;

            if baseclass.is_some() {
                pop_count = self.push_static_scopes_helper(cx, cframe.baseclass());

                self.ev(cx, &baseclass);
            } else {
                self.push_null();
            }
            let classname = builder_of(&cframe).expect("NullPointerException").classname().expect("NullPointerException");
            self.new_class_object(&classname);

            for _ in 0..pop_count {
                self.pop_scope();
            }

            return None;
        }

        let (pkgdef, used_namespaces, clsdefs, ref_, temp_count, private_namespace, statements, staticfexprs, iframe, fexprs, attrs) = {
            let b = node.borrow();
            let n = b.class_definition().unwrap();
            (
                n.def.pkgdef.clone(),
                n.used_namespaces.clone(),
                node_list(&n.clsdefs),
                n.ref_.clone().expect("NullPointerException"),
                n.temp_count,
                n.private_namespace.clone(),
                n.statements.clone(),
                node_list(&n.staticfexprs),
                n.iframe.clone().expect("NullPointerException"),
                node_list(&n.fexprs),
                n.def.attrs.clone(),
            )
        };

        let pkg_used = pkgdef.as_ref().map(|p| p.borrow().as_package_definition().expect("ClassCastException").used_namespaces.clone());
        if pkg_used.is_some() && depth(cx) == 1 {
            self.used_namespaces_sets.push(pkg_used.clone());
        }

        self.used_namespaces_sets.push(Some(used_namespaces));

        cx.borrow().push_static_class_scopes(&cframe);

        for clsdef in clsdefs {
            evaluate(&clsdef, cx, self);
            let c = clsdef.borrow().class_definition().expect("ClassCastException").cframe.clone();
            if let Some(c) = c {
                scope_top(cx).deferred_class_map_put(c, clsdef.clone());
            }
        }

        self.current_class = Some(node.clone());

        self.start_class(&ref_.name());

        let class_builder = builder_of(&cframe).expect("NullPointerException");
        let classname = class_builder.classname().expect("NullPointerException");
        {
            self.push_stack_frame();

            {
                let d = depth(cx);
                let f = self.frame_mut();
                f.function_name = format!("{}$cinit", classname);
                f.max_params = 0;
                f.max_locals = 0;
                f.max_temps = temp_count;
                f.activation_is_exposed = false;
                f.with_this = true;
                f.first_inner_scope = d - 1;
                f.register_scope_index = -1;
            }

            {
                let f = self.frame().clone();
                self.start_method_counts(&f.function_name, f.max_params, f.max_locals);
            }

            self.load_this();
            self.push_scope();

            if !class_builder.is_class_builder() {
                cx.borrow().internal_error("internal error: invalid class builder");
            }

            self.new_namespace(private_namespace.as_ref().expect("NullPointerException"));

            if statements.is_some() {
                self.ev(cx, &statements);
            }

            self.clear_position_info();
            self.set_position(0, 0, 0);
            self.return_(TYPE_void);

            let function_name = self.frame().function_name.clone();
            let d = depth(cx);
            self.finish_method(cx, &function_name, None, None, None, 0, d, "", false, false, None);

            for expr in &staticfexprs {
                evaluate(expr, cx, self);
            }

            self.pop_stack_frame();
        }

        cx.borrow().push_scope(iframe);

        for f in &fexprs {
            evaluate(f, cx, self);
        }

        let (is_dynamic, is_final) = match &attrs {
            Some(a) => {
                let b = a.borrow();
                let al = b.as_attribute_list().expect("ClassCastException");
                (al.has_dynamic, al.has_final)
            }
            None => (false, false),
        };

        let mut basename = None;
        if let Some(bc) = cframe.baseclass() {
            basename = builder_of(&bc).expect("NullPointerException").classname();
            if basename.as_ref().map_or(false, |b| b.name() == "Class") {
                basename = None;
            }
        }
        self.finish_class(cx, &classname, basename.as_ref(), is_dynamic, is_final, false, cframe.is_nullable());
        cx.borrow().pop_scope();
        cx.borrow().pop_static_class_scopes(Some(&cframe));

        self.used_namespaces_sets.pop();

        if pkg_used.is_some() && depth(cx) == 1 {
            self.used_namespaces_sets.pop();
        }
        None
    }

    fn evaluate_interface_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (cframe, needs_init, init, ref_, temp_count, iframe, fexprs) = {
            let b = node.borrow();
            let n = b.class_definition().unwrap();
            (
                n.cframe.clone().expect("NullPointerException"),
                n.needs_init,
                n.init.clone(),
                n.ref_.clone(),
                n.temp_count,
                n.iframe.clone(),
                node_list(&n.fexprs),
            )
        };
        let classname = || builder_of(&cframe).expect("NullPointerException").classname().expect("NullPointerException");

        if self.doing_method() || self.doing_class() {
            if needs_init {
                node.borrow_mut().class_definition_mut().unwrap().needs_init = false;
                self.ev(cx, &init);
                return None;
            }

            self.push_null();
            self.new_class_object(&classname());
            return None;
        }
        let classname = classname();

        self.current_class = Some(node.clone());
        self.start_class(&ref_.expect("NullPointerException").name());

        cx.borrow().push_static_class_scopes(&cframe);

        self.push_stack_frame();

        {
            {
                let f = self.frame_mut();
                f.function_name = format!("{}$cinit", classname);
                f.max_params = 0;
                f.max_locals = 0;
                f.register_scope_index = -1;
                f.max_temps = temp_count;
                f.activation_is_exposed = false;
            }
            let f = self.frame().clone();
            self.start_method_counts(&f.function_name, f.max_params, f.max_locals);
            self.return_(TYPE_void);
            let types: Vec<TypeInfo> = Vec::new();
            let d = depth(cx);
            self.finish_method(cx, &f.function_name, None, Some(&types), None, 0, d, "", false, false, None);
        }

        cx.borrow().push_scope(iframe.expect("NullPointerException"));

        self.pop_stack_frame();

        for f in &fexprs {
            evaluate(f, cx, self);
        }

        self.finish_class(cx, &classname, None, false, false, true, cframe.is_nullable());
        cx.borrow().pop_scope();
        cx.borrow().pop_static_class_scopes(Some(&cframe));
        None
    }

    fn evaluate_class_name(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (pkgname, ident) = {
            let b = node.borrow();
            let n = b.as_class_name().unwrap();
            (n.pkgname.clone(), n.ident.clone())
        };
        if pkgname.is_some() {
            self.ev(cx, &pkgname);
        }
        if ident.is_some() {
            self.ev(cx, &ident);
        }
        None
    }

    fn evaluate_inheritance(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (baseclass, interfaces) = {
            let b = node.borrow();
            let n = b.as_inheritance().unwrap();
            (n.baseclass.clone(), n.interfaces.clone())
        };
        if baseclass.is_some() {
            self.ev(cx, &baseclass);
        }
        if interfaces.is_some() {
            self.ev(cx, &interfaces);
        }
        None
    }

    fn evaluate_attribute_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        for item in items_of(node) {
            self.ev(cx, &item);
        }
        None
    }

    fn evaluate_include_directive(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (in_this_include, node_cx, prev_cx) = {
            let n = node.borrow();
            let i = n.as_include_directive().unwrap();
            (i.in_this_include, i.cx.clone(), i.prev_cx.clone())
        };
        if !in_this_include {
            let statics = cx.borrow().statics.clone();
            let prev = Rc::new(std::cell::RefCell::new(Context::new(statics)));
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

        let origin = cx.borrow().get_qualified_error_origin();
        self.set_origin(&origin);
        None
    }

    fn evaluate_import_directive(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_super_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = node.borrow().as_super_expression().unwrap().expr.clone();
        if expr.is_some() {
            self.ev(cx, &expr);
        } else {
            self.load_this();
        }
        None
    }

    fn evaluate_super_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let scope_depth = depth(cx);
        let iframe = scope_at(cx, scope_depth - 2);
        let ib = builder_of(&iframe).filter(|b| b.is_instance_builder());
        let namespaces = Namespaces::new();
        namespaces.add(cx.borrow().public_namespace());

        let has_base = ib.expect("NullPointerException").instance_data().basebui.is_some();
        if has_base {
            self.load_this();
            let call = node.borrow().as_super_statement().unwrap().call.clone();
            let args = nn(&call).borrow().as_call_expression().expect("ClassCastException").args.clone();
            let size = list_size(&args);
            if size != 0 {
                self.ev(cx, &args);
            }
            self.invoke_super(true, size);
        }
        None
    }

    fn evaluate_namespace_definition(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_config_namespace_definition(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_use_directive(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_pragma(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let list = node.borrow().as_pragma().unwrap().list.clone();
        if list.is_some() {
            self.ev(cx, &list);
        }
        None
    }

    fn evaluate_use_precision(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_use_numeric(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_use_rounding(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_pragma_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let pos = node.borrow().pos();
        cx.borrow().internal_error_at(pos, "PragmaExpressionNode not yet implemented");
        None
    }

    fn evaluate_typed_identifier(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let pos = node.borrow().pos();
        cx.borrow().internal_error_at(pos, "TypedIdentifierNode not yet implemented");
        None
    }

    fn evaluate_untyped_variable_binding(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let pos = node.borrow().pos();
        cx.borrow().internal_error_at(pos, "UntypedVariableBindingNode not yet implemented");
        None
    }

    fn evaluate_literal_xml(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (void_result, is_xmllist, list) = {
            let b = node.borrow();
            let n = b.as_literal_xml().unwrap();
            (n.void_result, n.is_xmllist, n.list.clone())
        };

        if !void_result {
            let namespaces = Namespaces::new();
            namespaces.push_back(cx.borrow().public_namespace());

            let name = if is_xmllist { "XMLList" } else { "XML" };

            self.find_property_name_nss(name, Some(&namespaces), true, true, false);
            self.get_property_name_nss(name, Some(&namespaces), true, false, false);
            self.ev(cx, &list);
            self.invoke_closure(true, 1);
        }
        None
    }

    fn evaluate_package_name(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let pos = node.borrow().pos();
        cx.borrow().internal_error_at(pos, "PackageNameNode not yet implemented");
        None
    }

    fn evaluate_package_identifiers(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let pos = node.borrow().pos();
        cx.borrow().internal_error_at(pos, "PackageIdentifiersNode not yet implemented");
        None
    }

    fn evaluate_error(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let pos = node.borrow().pos();
        cx.borrow().internal_error_at(pos, "ErrorNode not yet implemented");
        None
    }

    fn evaluate_rest_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let pos = node.borrow().pos();
        cx.borrow().internal_error_at(pos, "RestExpressionNode not yet implemented");
        None
    }

    fn evaluate_rest_parameter(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_paren_list_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let pos = node.borrow().pos();
        cx.borrow().internal_error_at(pos, "ParenListExpressionNode not yet implemented");
        None
    }

    fn evaluate_paren_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let pos = node.borrow().pos();
        cx.borrow().internal_error_at(pos, "ParenExpressionNode not yet implemented");
        None
    }

    fn evaluate_meta_data(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_doc_comment(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_import(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_binary_program(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_binary_class_def(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_binary_interface_definition(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_default_xml_namespace(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (ref_, expr) = {
            let b = node.borrow();
            let n = b.as_default_xml_namespace().unwrap();
            (n.ref_.clone(), n.expr.clone())
        };
        match ref_ {
            None => {
                self.ev(cx, &expr);
                self.default_xml_namespace();
            }
            Some(r) => {
                let val = r.get_value(&cx.borrow());
                let obj = val.and_then(|v| v.as_object_value());
                match obj {
                    None => {
                        self.ev(cx, &expr);
                        self.default_xml_namespace();
                    }
                    Some(obj) => self.default_xml_namespace_name(&obj.name()),
                }
            }
        }
        None
    }

    fn evaluate_type_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = node.borrow().as_type_expression().unwrap().expr.clone();
        self.ev(cx, &expr)
    }
}
