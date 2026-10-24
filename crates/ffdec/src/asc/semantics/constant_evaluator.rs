//! Port of `macromedia/asc/semantics/ConstantEvaluator.java`: the type checking and constant folding pass after
//! FlowAnalyzer, which records the slots, types and folded values CodeGenerator reads and reports strict-mode errors.

#![allow(non_upper_case_globals)]

use std::cell::{Ref, RefCell};
use std::rc::Rc;

use crate::asc::emit::runtime_constants::*;
use crate::asc::parser::evaluator::*;
use crate::asc::parser::node::*;
use crate::asc::parser::tokens::*;
use crate::asc::semantics::emitter::{Emitter, EmitterBase};
use crate::asc::semantics::interface_walker::InterfaceWalker;
use crate::asc::semantics::object_value_walker::ObjectValueWalker;
use crate::asc::semantics::slot::{DeclStyles, TypeInfos, PARAM_Optional, PARAM_Required, PARAM_Rest, PARAM_Void};
use crate::asc::semantics::value::*;
use crate::asc::util::context::{Context, ContextRef};
use crate::asc::util::decimal128::Decimal128;
use crate::asc::util::decimal128_context::Decimal128Context;
use crate::asc::util::errors::*;
use crate::asc::util::names::Names;
use crate::asc::util::number_constant::{java_d2i, java_d2l};
use crate::asc::util::numeric_conversions::{to_int32, to_uint32};
use crate::xfl::num::java_double;

pub struct ConstantEvaluator {
    base: EmitterBase,
    rch_bits: Option<BitSet>,
    block: Option<BlockRef>,
    doing_method: bool,
    doing_class: bool,
    in_anonymous_function: bool,
    pub return_type: TypeInfo,
    pub in_with: bool,
    this_contexts: Vec<i32>,
    super_context: Vec<i32>,
    type_info_preprocessing_complete: bool,
    current_decimal_context: Option<Decimal128Context>,
}

impl Emitter for ConstantEvaluator {
    fn emitter_base(&self) -> &EmitterBase {
        &self.base
    }

    fn emitter_base_mut(&mut self) -> &mut EmitterBase {
        &mut self.base
    }
}

macro_rules! field {
    ($node:expr, $acc:ident, $($f:ident).+) => {
        $node.borrow().$acc().expect("ClassCastException").$($f).+.clone()
    };
}

macro_rules! set_field {
    ($node:expr, $acc:ident, $($f:ident).+, $v:expr) => {{
        let v = $v;
        $node.borrow_mut().$acc().expect("ClassCastException").$($f).+ = v;
    }};
}

fn rb(n: &Option<NodeRef>) -> Ref<'_, Node> {
    n.as_ref().expect("NullPointerException").borrow()
}

fn nn(n: &Option<NodeRef>) -> NodeRef {
    n.clone().expect("NullPointerException")
}

fn pos_of(n: &Option<NodeRef>) -> i32 {
    rb(n).pos()
}

fn kind_is(n: &Option<NodeRef>, f: fn(&Node) -> bool) -> bool {
    n.as_ref().map_or(false, |n| f(&n.borrow()))
}

/// Java `t == u` for a possibly null type value.
fn is(t: &Option<TypeValue>, u: &TypeValue) -> bool {
    t.as_ref().map_or(false, |t| t.ptr_eq(u))
}

fn ti_is(t: &TypeInfo, u: &TypeValue) -> bool {
    is(&t.get_type_value(), u)
}

fn opt_tv_eq(a: &Option<TypeValue>, b: &Option<TypeValue>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.ptr_eq(b),
        (None, None) => true,
        _ => false,
    }
}

fn opt_ti_eq(a: &Option<TypeInfo>, b: &Option<TypeInfo>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.ptr_eq(b),
        (None, None) => true,
        _ => false,
    }
}

fn obj(o: Option<ObjectValue>) -> Option<Value> {
    o.map(Value::Object)
}

fn proto(t: &TypeInfo) -> Option<Value> {
    obj(t.get_prototype())
}

fn tv_proto(t: &TypeValue) -> Option<Value> {
    obj(t.prototype())
}

fn no_proto(cx: &ContextRef) -> Option<Value> {
    tv_proto(&cx.borrow().no_type())
}

fn no_ti(cx: &ContextRef) -> TypeInfo {
    cx.borrow().no_type().get_default_type_info()
}

fn size_types(t: &Option<TypeInfos>) -> usize {
    t.as_ref().map_or(0, |t| t.borrow().len())
}

fn size_decls(d: &Option<DeclStyles>) -> usize {
    d.as_ref().map_or(0, |d| d.borrow().len())
}

fn first_type(t: &Option<TypeInfos>) -> Option<TypeInfo> {
    if size_types(t) != 0 {
        Some(t.as_ref().unwrap().borrow()[0].clone())
    } else {
        None
    }
}

fn type_value_of(cx: &Context, o: &ObjectValue) -> Option<TypeValue> {
    o.get_type(cx).expect("NullPointerException").get_type_value()
}

fn type_name_of(cx: &Context, o: &ObjectValue) -> String {
    o.get_type(cx).expect("NullPointerException").get_name_cx(cx).to_string()
}

fn is_global_builder(o: &ObjectValue) -> bool {
    o.builder().map_or(false, |b| b.is_global_builder())
}

fn is_instance_builder(o: &ObjectValue) -> bool {
    o.builder().map_or(false, |b| b.is_instance_builder())
}

fn is_class_builder(o: &ObjectValue) -> bool {
    o.builder().map_or(false, |b| b.is_class_builder())
}

/// `!base.isDynamic() || (base.getType(cx).getTypeValue() == cx.functionType() && !(base.builder instanceof GlobalBuilder))`:
/// only global Functions are dynamic; methods of a class are MethodClosures.
fn is_fixed_base(cx: &Context, base: &ObjectValue) -> bool {
    !base.is_dynamic() || (is(&type_value_of(cx, base), &cx.function_type()) && !is_global_builder(base))
}

fn report_undefined_property(cx: &Context, pos: i32, base: &ObjectValue, name: &str) {
    if base.has_name_unqualified(cx, name, GET_TOKEN) {
        cx.error2(pos, kError_InaccessiblePropertyReference, name, &type_name_of(cx, base));
    } else {
        cx.error2(pos, kError_UndefinedProperty, name, &type_name_of(cx, base));
    }
}

fn is_xml_base(cx: &Context, base: &Option<ObjectValue>) -> bool {
    match base {
        Some(base) => {
            let t = type_value_of(cx, base);
            is(&t, &cx.xml_type()) || is(&t, &cx.xml_list_type())
        }
        None => false,
    }
}

fn current_number_type(cx: &Context, nu: &Option<NumberUsage>) -> TypeValue {
    match nu.as_ref().map(|n| n.get_usage()) {
        Some(NumberUsage::use_int) => cx.int_type(),
        Some(NumberUsage::use_uint) => cx.uint_type(),
        Some(NumberUsage::use_decimal) => cx.decimal_type().expect("NullPointerException"),
        _ => cx.double_type(),
    }
}

/// `cx.coerce(expr, actual, expected, isExplicit, force)`.
fn coerce(cx: &ContextRef, expr: &Option<NodeRef>, actual: &mut Option<TypeInfo>, expected: Option<TypeInfo>, is_explicit: bool, force: bool) -> Option<NodeRef> {
    let e = nn(expr);
    Some(cx.borrow().coerce(e, actual, expected, is_explicit, force))
}

/// `cx.getEmitter().getValueOfNumberLiteral(...)`.
fn number_literal(cx: &ContextRef, s: &str, type_: &mut Option<TypeValue>, usage: Option<&NumberUsage>) -> Option<NumberConstant> {
    let emitter = cx.borrow().get_emitter().expect("NullPointerException");
    let r = emitter.borrow_mut().get_value_of_number_literal(s, type_, usage);
    r
}

fn items_of(n: &Option<NodeRef>) -> Vec<Option<NodeRef>> {
    let n = rb(n);
    if let Some(s) = n.as_statement_list() {
        s.items.clone()
    } else if let Some(l) = n.list() {
        l.items.clone()
    } else if let Some(a) = n.as_argument_list() {
        a.items.clone()
    } else {
        panic!("ClassCastException")
    }
}

fn node_list(l: &Option<NodeList>) -> Vec<NodeRef> {
    l.as_ref().expect("NullPointerException").borrow().clone()
}

fn version_of_def(def: &Option<NodeRef>) -> i32 {
    match def {
        Some(d) => d.borrow().function_definition().expect("ClassCastException").version,
        None => -1,
    }
}

fn typeref_type_info(typeref: &ReferenceValue, tv: &TypeValue) -> TypeInfo {
    if typeref.has_nullable_anno() {
        tv.get_type_info(typeref.is_nullable())
    } else {
        tv.get_default_type_info()
    }
}

fn compare_type_infos(t1: &Option<TypeInfo>, t2: &Option<TypeInfo>) -> bool {
    if opt_ti_eq(t1, t2) {
        return true;
    }
    if let (Some(t1), Some(t2)) = (t1, t2) {
        return t1.is_nullable() == t2.is_nullable() && opt_tv_eq(&t1.get_type_value(), &t2.get_type_value());
    }
    false
}

impl ConstantEvaluator {
    pub fn new(cx: &ContextRef) -> ConstantEvaluator {
        ConstantEvaluator {
            base: EmitterBase::new(None),
            rch_bits: None,
            block: None,
            doing_method: false,
            doing_class: false,
            in_anonymous_function: false,
            return_type: no_ti(cx),
            in_with: false,
            this_contexts: Vec::new(),
            super_context: Vec::new(),
            type_info_preprocessing_complete: false,
            current_decimal_context: None,
        }
    }

    fn eval(&mut self, cx: &ContextRef, n: &Option<NodeRef>) -> Option<Value> {
        evaluate(n.as_ref().expect("NullPointerException"), cx, self)
    }

    fn this_context_last(&self) -> i32 {
        self.this_contexts.last().copied().unwrap_or(0)
    }

    /// `PreprocessDefinitionTypeInfo(cx, ProgramNode)`: mxmlc runs it over every file before evaluating any of them.
    pub fn preprocess_definition_type_info_program(&mut self, cx: &ContextRef, pn: &NodeRef) {
        if !self.type_info_preprocessing_complete {
            let items = items_of(&pn.borrow().program().expect("ClassCastException").statements);
            self.preprocess_definition_type_info_list(cx, &items, false);
            cx.borrow().process_unresolved_namespaces();
        }
        self.type_info_preprocessing_complete = true;
    }

    pub fn preprocess_definition_type_info_binary_program(&mut self, cx: &ContextRef, node: &NodeRef) {
        for cls_def in node_list(&field!(node, program, clsdefs)) {
            self.preprocess_definition_type_info_class(cx, &cls_def);
        }
        let items = items_of(&field!(node, program, statements));
        self.preprocess_definition_type_info_list(cx, &items, false);
    }

    fn preprocess_definition_type_info_list(&mut self, cx: &ContextRef, def_list: &[Option<NodeRef>], static_context: bool) {
        for it in def_list {
            let mut it = it.clone();
            if let Some(stmt) = it.as_ref().and_then(|n| n.borrow().as_labeled_statement().map(|l| l.statement.clone())) {
                it = stmt;
            }
            let Some(it) = it else { continue };
            let (is_dn, attrs) = {
                let b = it.borrow();
                match b.definition() {
                    Some(d) => (true, d.attrs.clone()),
                    None => (false, None),
                }
            };
            let has_static = attrs.as_ref().map(|a| a.borrow().as_attribute_list().expect("ClassCastException").has_static);
            if is_dn && ((has_static.is_some() && has_static == Some(static_context)) || (!static_context && (has_static.is_none() || has_static == Some(false)))) {
                let (is_class, is_function, is_variable) = {
                    let b = it.borrow();
                    (b.class_definition().is_some(), b.function_definition().is_some(), b.as_variable_definition().is_some())
                };
                if is_class {
                    self.preprocess_definition_type_info_class(cx, &it);
                } else if is_function {
                    self.preprocess_definition_type_info_function_definition(cx, &it);
                } else if is_variable {
                    self.preprocess_definition_type_info_variable_definition(cx, &it);
                }
                continue;
            }
            let member = it.borrow().as_member_expression().map(|m| m.selector.clone());
            if let Some(selector) = member {
                let selector = nn(&selector);
                let set_args = selector.borrow().as_set_expression().map(|s| s.args.clone());
                let call_args = selector.borrow().as_call_expression().map(|c| c.args.clone());
                if let Some(args) = set_args {
                    let first_arg = args.as_ref().and_then(|a| a.borrow().as_argument_list().unwrap().items[0].clone());
                    if kind_is(&first_arg, |n| n.as_function_definition().is_some()) {
                        self.preprocess_definition_type_info_function_definition(cx, first_arg.as_ref().unwrap());
                    } else if kind_is(&first_arg, |n| n.as_function_common().is_some()) {
                        self.preprocess_definition_type_info_function_common(cx, first_arg.as_ref().unwrap());
                    }
                } else if let Some(args) = call_args {
                    if let Some(args) = args {
                        for arg in items_of(&Some(args)) {
                            if kind_is(&arg, |n| n.as_function_definition().is_some()) {
                                self.preprocess_definition_type_info_function_definition(cx, arg.as_ref().unwrap());
                            } else if kind_is(&arg, |n| n.as_function_common().is_some()) {
                                self.preprocess_definition_type_info_function_common(cx, arg.as_ref().unwrap());
                            }
                        }
                    }
                }
                continue;
            }
            let expr_stmt = it.borrow().as_expression_statement().map(|e| e.expr.clone());
            if let Some(expr) = expr_stmt {
                if kind_is(&expr, |n| n.list().is_some()) {
                    let items = items_of(&expr);
                    self.preprocess_definition_type_info_list(cx, &items, false);
                }
                continue;
            }
            if it.borrow().as_binary_program().is_some() {
                self.preprocess_definition_type_info_binary_program(cx, &it);
                continue;
            }
            if it.borrow().as_statement_list().is_some() {
                let items = items_of(&Some(it.clone()));
                self.preprocess_definition_type_info_list(cx, &items, static_context);
                continue;
            }
            if !static_context {
                // Blocks for for, with, try, catch and finally clauses sit in the host statement list and can hold
                // their own definitions.
                enum K {
                    If(Option<NodeRef>, Option<NodeRef>),
                    Stmt(Option<NodeRef>),
                    Catch(Option<ObjectValue>, Option<NodeRef>),
                    Finally(Option<NodeRef>),
                    Switch(Option<NodeRef>),
                    Try(Option<NodeRef>, Option<NodeRef>, Option<NodeRef>),
                    None,
                }
                let k = {
                    let b = it.borrow();
                    match &b.kind {
                        Kind::IfStatement(n) => K::If(n.thenactions.clone(), n.elseactions.clone()),
                        Kind::ForStatement(n) => K::Stmt(n.statement.clone()),
                        Kind::WhileStatement(n) => K::Stmt(n.statement.clone()),
                        Kind::DoStatement(n) => K::Stmt(n.statements.clone()),
                        Kind::CatchClause(n) => K::Catch(n.activation.clone(), n.statements.clone()),
                        Kind::FinallyClause(n) => K::Finally(n.statements.clone()),
                        Kind::SwitchStatement(n) => K::Switch(n.statements.clone()),
                        Kind::TryStatement(n) => K::Try(n.tryblock.clone(), n.catchlist.clone(), n.finallyblock.clone()),
                        _ => K::None,
                    }
                };
                let is_sl = |n: &Option<NodeRef>| kind_is(n, |n| n.as_statement_list().is_some());
                match k {
                    K::If(thenactions, elseactions) => {
                        if is_sl(&thenactions) {
                            self.preprocess_definition_type_info_list(cx, &items_of(&thenactions), false);
                        }
                        if is_sl(&elseactions) {
                            self.preprocess_definition_type_info_list(cx, &items_of(&elseactions), false);
                        }
                    }
                    K::Stmt(s) => {
                        if is_sl(&s) {
                            self.preprocess_definition_type_info_list(cx, &items_of(&s), false);
                        }
                    }
                    K::Catch(activation, statements) => {
                        if statements.is_some() {
                            cx.borrow().push_scope(activation.expect("NullPointerException"));
                            self.preprocess_definition_type_info_list(cx, &items_of(&statements), false);
                            cx.borrow().pop_scope();
                        }
                    }
                    K::Finally(statements) | K::Switch(statements) => {
                        if statements.is_some() {
                            self.preprocess_definition_type_info_list(cx, &items_of(&statements), false);
                        }
                    }
                    K::Try(tryblock, catchlist, finallyblock) => {
                        if tryblock.is_some() {
                            self.preprocess_definition_type_info_list(cx, &items_of(&tryblock), false);
                        }
                        if catchlist.is_some() {
                            self.preprocess_definition_type_info_list(cx, &items_of(&catchlist), false);
                        }
                        if let Some(f) = &finallyblock {
                            let statements = field!(f, as_finally_clause, statements);
                            if statements.is_some() {
                                self.preprocess_definition_type_info_list(cx, &items_of(&statements), false);
                            }
                        }
                    }
                    K::None => {}
                }
            }
        }
    }

    fn preprocess_definition_type_info_function_common(&mut self, cx: &ContextRef, fcn: &NodeRef) {
        let (fun, def, signature, body, ref_, kind) = {
            let b = fcn.borrow();
            let f = b.as_function_common().expect("ClassCastException");
            (f.fun.clone(), f.def.clone(), f.signature.clone(), f.body.clone(), f.ref_.clone(), f.kind)
        };
        // A function expression hoisted out of a block that conditional compilation removed is never initialized.
        let Some(fun) = fun else { return };

        cx.borrow().push_scope(fun.activation().expect("NullPointerException"));

        let version = version_of_def(&def);
        if def.is_some() && version > -1 {
            cx.borrow().push_version(version);
        }

        self.preprocess_definition_type_info_function_signature(cx, &nn(&signature));
        if body.is_some() {
            self.preprocess_definition_type_info_list(cx, &items_of(&body), false);
        }
        cx.borrow().pop_scope();
        let slot = ref_.expect("NullPointerException").get_slot_kind(&cx.borrow(), kind);
        if let Some(slot) = slot {
            let (sig_type, parameter) = {
                let b = rb(&signature);
                let s = b.as_function_signature().unwrap();
                (s.type_.clone(), s.parameter.clone())
            };
            slot.set_type(Some(sig_type.unwrap_or_else(|| no_ti(cx))));
            if let Some(parameter) = &parameter {
                let (types, decl_styles) = {
                    let b = parameter.borrow();
                    let p = b.as_parameter_list().expect("ClassCastException");
                    (p.types.clone(), p.decl_styles.clone())
                };
                slot.set_types(Some(Rc::new(RefCell::new(types))));
                slot.set_decl_styles(Some(Rc::new(RefCell::new(decl_styles))));
            } else {
                // Void marks a function that declares no parameters, for argument count checking.
                slot.add_type(cx.borrow().void_type().get_default_type_info());
                slot.add_decl_style(PARAM_Void);
            }
        }

        if def.is_some() && version > -1 {
            cx.borrow().pop_version();
        }
    }

    fn preprocess_definition_type_info_function_definition(&mut self, _unused_cx: &ContextRef, node: &NodeRef) {
        // AbcParser already set up everything for a BinaryFunctionDefinitionNode.
        if node.borrow().as_binary_function_definition().is_some() {
            return;
        }
        let (cx, fexpr) = {
            let b = node.borrow();
            let f = b.as_function_definition().unwrap();
            (f.cx.clone().expect("NullPointerException"), f.fexpr.clone())
        };
        let fcn = nn(&fexpr);
        if field!(fcn, as_function_common, ref_).is_some() {
            self.preprocess_definition_type_info_function_common(&cx, &fcn);
        }
    }

    fn preprocess_definition_type_info_class(&mut self, _unused_cx: &ContextRef, node: &NodeRef) {
        // AbcParser already set up everything for a BinaryClassDefNode.
        if node.borrow().binary_class_def().is_some() {
            return;
        }
        let (cx, cframe, statements, iframe, version, instanceinits, ref_) = {
            let b = node.borrow();
            let c = b.class_definition().expect("ClassCastException");
            (
                c.cx.clone().expect("NullPointerException"),
                c.cframe.clone(),
                c.statements.clone(),
                c.iframe.clone(),
                c.version,
                c.instanceinits.clone(),
                c.ref_.clone(),
            )
        };
        let cframe = cframe.expect("NullPointerException");
        cx.borrow().push_static_class_scopes(&cframe);
        self.preprocess_definition_type_info_list(&cx, &items_of(&statements), true);

        cx.borrow().push_scope(iframe.expect("NullPointerException"));

        if version > -1 {
            cx.borrow().push_version(version);
        }

        if let Some(inits) = &instanceinits {
            let items: Vec<Option<NodeRef>> = inits.borrow().iter().cloned().map(Some).collect();
            self.preprocess_definition_type_info_list(&cx, &items, false);
        }

        // The constructor's signature is recorded in the class slot, for type checking of constructor calls.
        let class_slot = ref_.as_ref().and_then(|r| r.get_slot_kind(&cx.borrow(), NEW_TOKEN));
        if let Some(class_slot) = &class_slot {
            for init in node_list(&instanceinits) {
                let fexpr = init.borrow().as_function_definition().map(|f| f.fexpr.clone());
                if let Some(fexpr) = fexpr {
                    let fexpr = nn(&fexpr);
                    let (fref, fkind) = {
                        let b = fexpr.borrow();
                        let f = b.as_function_common().unwrap();
                        (f.ref_.clone().expect("NullPointerException"), f.kind)
                    };
                    if fref.name() == "$construct" {
                        let ctor_slot = fref.get_slot_kind(&cx.borrow(), fkind);
                        if let Some(ctor_slot) = ctor_slot {
                            class_slot.set_types(ctor_slot.get_types());
                            class_slot.set_decl_styles(ctor_slot.get_decl_styles());
                        }
                    }
                }
            }
        }

        cx.borrow().pop_scope();
        cx.borrow().pop_static_class_scopes(Some(&cframe));

        if version > -1 {
            cx.borrow().pop_version();
        }
    }

    fn preprocess_definition_type_info_variable_definition(&mut self, cx: &ContextRef, node: &NodeRef) {
        let list = field!(node, as_variable_definition, list);
        for it in items_of(&list) {
            if kind_is(&it, |n| n.as_variable_binding().is_some()) {
                self.preprocess_definition_type_info_variable_binding(cx, it.as_ref().unwrap());
            }
        }
    }

    fn preprocess_definition_type_info_variable_binding(&mut self, cx: &ContextRef, node: &NodeRef) {
        let (typeref, ref_, initializer) = {
            let b = node.borrow();
            let v = b.as_variable_binding().unwrap();
            (v.typeref.clone(), v.ref_.clone(), v.initializer.clone())
        };
        if let Some(typeref) = &typeref {
            let typeslot = typeref.get_slot(&cx.borrow());
            let mut type_ = None;
            if let Some(tv) = typeslot.and_then(|s| s.get_value()).and_then(|v| v.as_type_value()) {
                type_ = Some(typeref_type_info(typeref, &tv));
            }
            if let Some(type_) = type_ {
                let slot = ref_.expect("NullPointerException").get_slot(&cx.borrow());
                if let Some(slot) = slot {
                    slot.set_type(Some(type_.clone()));
                    let types = slot.get_types().expect("NullPointerException");
                    let mut t = types.borrow_mut();
                    t.clear();
                    t.push(type_);
                }
            }
        }

        if kind_is(&initializer, |n| n.as_function_common().is_some()) {
            self.preprocess_definition_type_info_function_common(cx, initializer.as_ref().unwrap());
        }
    }

    fn preprocess_definition_type_info_function_signature(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let parameter = field!(node, as_function_signature, parameter);
        if let Some(p) = &parameter {
            self.preprocess_definition_type_info_parameter_list(cx, p);
        }
        self.signature_type(cx, node)
    }

    /// The common tail of `evaluate(FunctionSignatureNode)` and its `PreprocessDefinitionTypeInfo`.
    fn signature_type(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (typeref, void_anno, result_node) = {
            let b = node.borrow();
            let s = b.as_function_signature().unwrap();
            (s.typeref.clone(), s.void_anno, s.result.clone())
        };
        let mut result = no_proto(cx);
        if let Some(typeref) = &typeref {
            let typeslot = typeref.get_slot(&cx.borrow());
            if let Some(typeslot) = typeslot {
                let typeval = typeslot.get_value().and_then(|v| v.as_type_value());
                set_field!(node, as_function_signature_mut, type_, typeval.map(|tv| typeref_type_info(typeref, &tv)));
            }
            let t = field!(node, as_function_signature, type_);
            if let Some(t) = t {
                result = proto(&t);
            } else {
                cx.borrow().error1(pos_of(&result_node), kError_UnknownType, &typeref.get_diagnostic_type_name());
            }
        } else if void_anno {
            let void = cx.borrow().void_type();
            set_field!(node, as_function_signature_mut, type_, Some(void.get_default_type_info()));
            result = tv_proto(&void);
        } else {
            set_field!(node, as_function_signature_mut, type_, Some(no_ti(cx)));
        }
        result
    }

    fn check_default_value(&mut self, cx: &ContextRef, type_: &TypeInfo, from: &ObjectValue) -> Option<ObjectValue> {
        let from_id = || from.type_().expect("NullPointerException").get_type_id();
        match type_.get_type_id() {
            TYPE_object | TYPE_none => return Some(from.clone()),
            TYPE_array | TYPE_xml => {
                if let TYPE_void | TYPE_null = from_id() {
                    return Some(from.clone());
                }
            }
            TYPE_string => {
                if let TYPE_void | TYPE_null | TYPE_string = from_id() {
                    return Some(from.clone());
                }
            }
            TYPE_function => {
                if let TYPE_void | TYPE_null | TYPE_function = from_id() {
                    return Some(from.clone());
                }
            }
            TYPE_type => {
                if let TYPE_void | TYPE_null | TYPE_type = from_id() {
                    return Some(from.clone());
                }
            }
            TYPE_boolean => {
                if let TYPE_void | TYPE_boolean = from_id() {
                    return Some(from.clone());
                }
            }
            TYPE_double => {
                if let TYPE_void | TYPE_double | TYPE_int | TYPE_uint | TYPE_decimal = from_id() {
                    return Some(from.clone());
                }
            }
            TYPE_decimal => {
                if let TYPE_void | TYPE_double | TYPE_decimal | TYPE_int = from_id() {
                    return Some(from.clone());
                }
            }
            TYPE_uint => match from_id() {
                TYPE_void | TYPE_uint => return Some(from.clone()),
                TYPE_int | TYPE_decimal | TYPE_double => {
                    let nstr = from.to_string();
                    let mut ntype = None;
                    let dval = number_literal(cx, &nstr, &mut ntype, Some(&NumberUsage::new())).expect("NullPointerException").double_value();
                    let uval = to_uint32(dval);
                    return if dval == uval as f64 {
                        Some(from.clone())
                    } else {
                        Some(ObjectValue::with_value(&uval.to_string(), Some(&cx.borrow().uint_type())))
                    };
                }
                _ => {}
            },
            TYPE_int => match from_id() {
                TYPE_void | TYPE_int => return Some(from.clone()),
                TYPE_uint | TYPE_decimal | TYPE_double => {
                    let nstr = from.to_string();
                    let mut ntype = None;
                    let dval = number_literal(cx, &nstr, &mut ntype, Some(&NumberUsage::new())).expect("NullPointerException").double_value();
                    let ival = to_int32(dval);
                    return if dval == ival as f64 {
                        Some(from.clone())
                    } else {
                        Some(ObjectValue::with_value(&(ival as i64).to_string(), Some(&cx.borrow().int_type())))
                    };
                }
                _ => {}
            },
            _ => {}
        }
        None
    }

    fn preprocess_definition_type_info_rest_parameter(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let result = self.preprocess_definition_type_info_parameter(cx, node);
        let (ref_, typeref, pos) = {
            let b = node.borrow();
            let p = b.parameter().unwrap();
            (p.ref_.clone(), p.typeref.clone(), b.pos())
        };
        let s = ref_.expect("NullPointerException").get_slot(&cx.borrow()).expect("NullPointerException");
        // Only an explicit type annotation other than Array is an error.
        if typeref.is_some() && s.get_type().map_or(false, |t| !ti_is(&t, &cx.borrow().array_type())) {
            cx.borrow().error(pos, kError_InvalidRestDecl);
        }
        result
    }

    fn preprocess_definition_type_info_parameter(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let type_ = self.parameter_type(cx, node);
        proto(&type_)
    }

    /// The type annotation handling shared by `evaluate(ParameterNode)` and its `PreprocessDefinitionTypeInfo`.
    fn parameter_type(&mut self, cx: &ContextRef, node: &NodeRef) -> TypeInfo {
        let (ref_, typeref, type_node) = {
            let b = node.borrow();
            let p = b.parameter().unwrap();
            (p.ref_.clone(), p.typeref.clone(), p.type_.clone())
        };
        let slot = ref_.expect("NullPointerException").get_slot(&cx.borrow());
        if let Some(typeref) = &typeref {
            let mut type_ = None;
            let typeslot = typeref.get_slot(&cx.borrow());
            if let Some(typeslot) = typeslot {
                let typeval = typeslot.get_value().and_then(|v| v.as_type_value());
                type_ = typeval.map(|tv| typeref_type_info(typeref, &tv));
            }
            if let Some(type_) = type_ {
                slot.expect("NullPointerException").set_type(Some(type_.clone()));
                type_
            } else {
                cx.borrow().error1(pos_of(&type_node), kError_UnknownType, &typeref.get_diagnostic_type_name());
                let t = no_ti(cx);
                slot.expect("NullPointerException").set_type(Some(t.clone()));
                t
            }
        } else {
            no_ti(cx)
        }
    }

    fn preprocess_definition_type_info_parameter_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if field!(node, as_parameter_list, types).is_empty() {
            let mut last_decl_style = PARAM_Required;
            for item in items_of_params(node) {
                let item = nn(&item);
                let decl_style;
                if item.borrow().as_rest_parameter().is_none() {
                    let val = self.preprocess_definition_type_info_parameter(cx, &item);
                    let o = val.expect("NullPointerException").get_value(&cx.borrow()).and_then(|v| v.as_object_value());
                    let type_ = o.expect("NullPointerException").type_().unwrap_or_else(|| no_ti(cx));
                    set_types_push(node, type_);
                    decl_style = if item.borrow().parameter().unwrap().init.is_none() { PARAM_Required } else { PARAM_Optional };
                } else {
                    let val = self.preprocess_definition_type_info_rest_parameter(cx, &item);
                    let _o = val.expect("NullPointerException").get_value(&cx.borrow()).and_then(|v| v.as_object_value());
                    // Only used for argument coercion.
                    set_types_push(node, cx.borrow().array_type().get_default_type_info());
                    decl_style = PARAM_Rest;
                }

                node.borrow_mut().as_parameter_list_mut().unwrap().decl_styles.push(decl_style as u8);

                if decl_style == PARAM_Required && last_decl_style != decl_style {
                    cx.borrow().error(item.borrow().pos(), kError_BadRequiredParameter);
                }
                last_decl_style = decl_style;
            }
        }
        None
    }

    fn is_unary_numeric(op: i32) -> bool {
        op == PLUS_TOKEN || op == MINUS_TOKEN || op == BITWISENOT_TOKEN
    }

    pub fn compute_binary_expr(&mut self, cx: &ContextRef, op: i32, lv: &ObjectValue, rv: &ObjectValue, number_usage: Option<&NumberUsage>) -> Option<Value> {
        let mut val: Option<ObjectValue> = None;
        let (ltval, rtval) = {
            let c = cx.borrow();
            let lt = lv.get_type(&c).expect("NullPointerException");
            let rt = rv.get_type(&c).expect("NullPointerException");
            (lt.get_type_value(), rt.get_type_value())
        };

        match op {
            MINUS_TOKEN | MULT_TOKEN | DIV_TOKEN | PLUS_TOKEN | MODULUS_TOKEN => {
                let numeric = {
                    let c = cx.borrow();
                    ltval.as_ref().map_or(false, |t| t.is_numeric(&c)) && rtval.as_ref().expect("NullPointerException").is_numeric(&c)
                };
                if numeric {
                    let mut ltype = None;
                    let mut rtype = None;
                    let lval = number_literal(cx, &lv.get_value(), &mut ltype, number_usage);
                    let rval = number_literal(cx, &rv.get_value(), &mut rtype, number_usage);
                    let lval = || lval.as_ref().expect("NullPointerException");
                    let rval = || rval.as_ref().expect("NullPointerException");

                    let number_usage = number_usage.expect("NullPointerException");
                    let mut force_type = true;
                    let mut usage = number_usage.get_usage();
                    if usage == NumberUsage::use_Number {
                        // The operand types decide the result type.
                        force_type = false;
                        let c = cx.borrow();
                        let es4_numerics = c.statics.borrow().es4_numerics;
                        let decimal = c.decimal_type();
                        if es4_numerics && (opt_tv_eq(&ltype, &decimal) || opt_tv_eq(&rtype, &decimal)) {
                            usage = NumberUsage::use_decimal;
                        } else if is(&ltype, &c.double_type()) || is(&rtype, &c.double_type()) || is(&ltype, &c.number_type()) || is(&rtype, &c.number_type()) {
                            usage = NumberUsage::use_double;
                        } else if is(&ltype, &c.int_type()) || is(&rtype, &c.uint_type()) {
                            usage = if lval().int_value() >= 0 { NumberUsage::use_uint } else { NumberUsage::use_double };
                        } else if is(&ltype, &c.uint_type()) || is(&rtype, &c.int_type()) {
                            usage = if rval().int_value() >= 0 { NumberUsage::use_uint } else { NumberUsage::use_double };
                        } else {
                            usage = NumberUsage::use_int;
                        }

                        if !es4_numerics {
                            usage = NumberUsage::use_double;
                        }
                    }
                    match usage {
                        NumberUsage::use_decimal => {
                            let mut d = Decimal128::nan();
                            let ld = lval().decimal_value();
                            let rd = rval().decimal_value();
                            let ctx = self.current_decimal_context.as_mut().expect("NullPointerException");
                            ctx.set_precision(number_usage.get_precision()).unwrap_or_else(|e| panic!("IllegalArgumentException: {e}"));
                            ctx.set_rounding_mode(java_rounding_mode(number_usage.get_java_rounding_mode().expect("NullPointerException")));
                            match op {
                                MINUS_TOKEN => d = ld.subtract_ctx(&rd, ctx),
                                PLUS_TOKEN => d = ld.add_ctx(&rd, ctx),
                                MULT_TOKEN => d = ld.multiply_ctx(&rd, ctx),
                                DIV_TOKEN => d = ld.divide_ctx(&rd, ctx),
                                MODULUS_TOKEN => d = ld.remainder_ctx(&rd, ctx),
                                _ => {}
                            }
                            val = Some(ObjectValue::with_value(&format!("{}m", d), cx.borrow().decimal_type().as_ref()));
                        }
                        NumberUsage::use_double => {
                            let mut d = f64::NAN;
                            let ld = lval().double_value();
                            let rd = rval().double_value();
                            match op {
                                MINUS_TOKEN => d = ld - rd,
                                PLUS_TOKEN => d = ld + rd,
                                MULT_TOKEN => d = ld * rd,
                                DIV_TOKEN => d = ld / rd,
                                MODULUS_TOKEN => d = ld % rd,
                                _ => {}
                            }
                            val = Some(ObjectValue::with_value(&java_double(d), Some(&cx.borrow().double_type())));
                        }
                        NumberUsage::use_int => {
                            let mut i: i32 = 0;
                            let li = lval().int_value();
                            let ri = rval().int_value();
                            let mut d: f64 = 0.0;
                            let ld = lval().double_value();
                            let rd = rval().double_value();
                            match op {
                                MINUS_TOKEN => {
                                    i = li.wrapping_sub(ri);
                                    d = ld - rd;
                                }
                                PLUS_TOKEN => {
                                    i = li.wrapping_add(ri);
                                    d = ld + rd;
                                }
                                MULT_TOKEN => {
                                    i = li.wrapping_mul(ri);
                                    d = ld * rd;
                                }
                                DIV_TOKEN => {
                                    if ri == 0 {
                                        panic!("ArithmeticException: / by zero");
                                    }
                                    i = li.wrapping_div(ri);
                                    d = ld / rd;
                                }
                                MODULUS_TOKEN => {
                                    if ri == 0 {
                                        return Some(ObjectValue::with_value("NaN", Some(&cx.borrow().double_type())).into());
                                    }
                                    i = li.wrapping_rem(ri);
                                    d = ld % rd;
                                }
                                _ => {}
                            }
                            val = Some(if force_type || java_d2i(d) == i {
                                ObjectValue::with_value(&i.to_string(), Some(&cx.borrow().int_type()))
                            } else {
                                ObjectValue::with_value(&java_double(d), Some(&cx.borrow().double_type()))
                            });
                        }
                        NumberUsage::use_uint => {
                            let mut d: i64 = 0;
                            let ld = lval().uint_value();
                            let rd = rval().uint_value();
                            match op {
                                MINUS_TOKEN => d = ld.wrapping_sub(rd),
                                PLUS_TOKEN => d = ld.wrapping_add(rd),
                                MULT_TOKEN => d = ld.wrapping_mul(rd),
                                DIV_TOKEN => {
                                    if rd == 0 {
                                        let sval = if ld == 0 {
                                            "NaN"
                                        } else if ld < 0 {
                                            "-Infinity"
                                        } else {
                                            "Infinity"
                                        };
                                        return Some(ObjectValue::with_value(sval, Some(&cx.borrow().double_type())).into());
                                    }
                                    d = ld.wrapping_div(rd);
                                }
                                MODULUS_TOKEN => {
                                    if rd == 0 {
                                        return Some(ObjectValue::with_value("NaN", Some(&cx.borrow().double_type())).into());
                                    }
                                    d = ld.wrapping_rem(rd);
                                }
                                _ => {}
                            }
                            if force_type || (d >= 0 && d <= 0xFFFFFFFF) {
                                d &= 0xFFFFFFFF;
                                val = Some(ObjectValue::with_value(&d.to_string(), Some(&cx.borrow().uint_type())));
                            } else {
                                let dval = if op == MULT_TOKEN && (d > 0xFFFFFFFF || d < 0) {
                                    // The multiply may have overflowed; redo it in double.
                                    lval().double_value() * rval().double_value()
                                } else {
                                    d as f64
                                };
                                val = Some(ObjectValue::with_value(&java_double(dval), Some(&cx.borrow().double_type())));
                            }
                        }
                        _ => {}
                    }
                } else if op == PLUS_TOKEN {
                    val = cx.borrow().no_type().prototype();
                }
            }
            _ => {
                val = cx.borrow().no_type().prototype();
            }
        }

        obj(val)
    }

    pub fn match_signatures(&self, implementing_slot: &Slot, interface_slot: &Slot) -> bool {
        if !opt_ti_eq(&implementing_slot.get_type(), &interface_slot.get_type()) {
            return false;
        }
        let it = implementing_slot.get_types();
        let count = size_types(&it);
        let ft = interface_slot.get_types();
        if count != size_types(&ft) {
            return false;
        }
        for i in 0..count {
            if !it.as_ref().unwrap().borrow()[i].ptr_eq(&ft.as_ref().unwrap().borrow()[i]) {
                return false;
            }
            let a = implementing_slot.get_decl_styles().expect("NullPointerException").borrow()[i];
            let b = interface_slot.get_decl_styles().expect("NullPointerException").borrow()[i];
            if a != b {
                return false;
            }
        }
        true
    }

    fn evaluate_parameter_impl(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let type_ = self.parameter_type(cx, node);
        let (init, ref_) = {
            let b = node.borrow();
            let p = b.parameter().unwrap();
            (p.init.clone(), p.ref_.clone())
        };
        if init.is_some() {
            let mut v = self.eval(cx, &init);
            if let Some(Value::Reference(r)) = &v {
                v = r.get_value(&cx.borrow());
            }
            let ov = v.and_then(|v| v.as_object_value());
            match ov.filter(|o| o.has_value()) {
                Some(ov) => {
                    let checked = self.check_default_value(cx, &type_, &ov);
                    if checked.is_none() {
                        let c = cx.borrow();
                        c.error2(pos_of(&init), kError_IncompatibleDefaultValue, &ov.type_().expect("NullPointerException").get_name_cx(&c).to_string(), &type_.get_name_cx(&c).to_string());
                    }
                    let slot = ref_.expect("NullPointerException").get_slot(&cx.borrow());
                    slot.expect("NullPointerException").set_value(obj(checked));
                }
                None => cx.borrow().error(pos_of(&init), kError_NonConstantParamInitializer),
            }
        }
        proto(&type_)
    }

    fn evaluate_class_definition_impl(&mut self, _unused_cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (cx, cframe) = {
            let b = node.borrow();
            let c = b.class_definition().expect("ClassCastException");
            (c.cx.clone().expect("NullPointerException"), c.cframe.clone())
        };
        let cx = &cx;
        let cframe_value = || cframe.clone().map(Value::from);

        if self.doing_method {
            return cframe_value();
        }

        if field!(node, class_definition, needs_init) {
            set_field!(node, class_definition_mut, needs_init, false);
            self.doing_method = true;
            let init = field!(node, class_definition, init);
            self.eval(cx, &init);
            self.doing_method = false;
        }
        let mut class_slot = None;
        let (ref_, version, attrs, name, baseclass) = {
            let b = node.borrow();
            let c = b.class_definition().unwrap();
            (c.ref_.clone(), c.version, c.def.attrs.clone(), c.name.clone(), c.baseclass.clone())
        };
        if let Some(r) = &ref_ {
            class_slot = r.get_slot_kind(&cx.borrow(), NEW_TOKEN);
        }

        if version > -1 {
            cx.borrow().push_version(version);
        }

        if attrs.is_some() {
            self.eval(cx, &attrs);
        }
        if name.is_some() {
            self.eval(cx, &name);
        }
        if baseclass.is_some() {
            self.eval(cx, &baseclass);
        }

        if self.doing_class {
            return cframe_value();
        }

        let cframe_tv = cframe.clone().expect("NullPointerException");
        cx.borrow().push_static_class_scopes(&cframe_tv);
        self.this_contexts.push(error_this);

        self.doing_class = true;

        for staticfexpr in node_list(&field!(node, class_definition, staticfexprs)) {
            evaluate(&staticfexpr, cx, self);
        }

        {
            self.doing_class = false;
            for clsdef in node_list(&field!(node, class_definition, clsdefs)) {
                evaluate(&clsdef, cx, self);
            }
            self.doing_class = true;
        }

        let statements = field!(node, class_definition, statements);
        if statements.is_some() {
            self.eval(cx, &statements);
        }

        // The outer code block is done, so methods are compiled now rather than later.
        self.doing_method = false;

        let iframe = field!(node, class_definition, iframe).expect("NullPointerException");
        cx.borrow().push_scope(iframe.clone());

        self.this_contexts.pop();
        self.this_contexts.push(instance_this);

        let interfaces = field!(node, class_definition, interfaces);
        if interfaces.is_some() {
            self.eval(cx, &interfaces);
        }

        {
            for init in node_list(&field!(node, class_definition, instanceinits)) {
                self.doing_method = true;
                let is_fd = init.borrow().as_function_definition().is_some();
                if is_fd {
                    // Static functions and accessors are evaluated here rather than from a StatementListNode, which
                    // is what normally sets doing_method.
                    evaluate(&init, cx, self);

                    let fexpr = nn(&field!(init, as_function_definition, fexpr));
                    let (fref, fkind) = {
                        let b = fexpr.borrow();
                        let f = b.as_function_common().unwrap();
                        (f.ref_.clone().expect("NullPointerException"), f.kind)
                    };
                    if fref.name() == "$construct" {
                        // The class slot carries the constructor's signature for checking constructor calls.
                        let ctor_slot = fref.get_slot_kind(&cx.borrow(), fkind);
                        if let (Some(class_slot), Some(ctor_slot)) = (&class_slot, &ctor_slot) {
                            class_slot.set_types(ctor_slot.get_types());
                            class_slot.set_decl_styles(ctor_slot.get_decl_styles());
                        }
                    }
                } else {
                    let es4_nullability = cx.borrow().statics.borrow().es4_nullability;
                    let is_def = init.borrow().is_definition();
                    if es4_nullability && !is_def {
                        iframe.set_init_only(true);
                    }

                    evaluate(&init, cx, self);

                    if es4_nullability && !is_def {
                        iframe.set_init_only(false);
                    }
                }
            }
            self.doing_method = false;
        }

        for fexpr in node_list(&field!(node, class_definition, fexprs)) {
            evaluate(&fexpr, cx, self);
        }

        self.doing_class = false;

        self.check_interface_methods(cx, node);

        self.this_contexts.pop();

        cx.borrow().pop_scope();
        cx.borrow().pop_static_class_scopes(cframe.as_ref());

        if version > -1 {
            cx.borrow().pop_version();
        }

        set_field!(node, class_definition_mut, needs_init, true);

        cframe_value()
    }

    fn check_interface_methods(&mut self, cx: &ContextRef, class_node: &NodeRef) {
        let iframe = field!(class_node, class_definition, iframe).expect("NullPointerException");
        let mut interface_walker = InterfaceWalker::with_super_interfaces(&iframe, false);
        let is_interface = class_node.borrow().is_interface();

        while interface_walker.has_next() {
            let interface_iframe = interface_walker.next().unwrap();
            let interface_cframe = interface_iframe.type_().expect("NullPointerException").type_value();

            if is_interface {
                self.check_interface_interface_methods(cx, class_node, &interface_cframe.prototype().expect("NullPointerException"));
            } else {
                self.check_class_interface_methods(cx, class_node, &interface_cframe.prototype().expect("NullPointerException"));
            }
        }
    }

    fn check_class_interface_methods(&mut self, cx: &ContextRef, class_node: &NodeRef, interface_iframe: &ObjectValue) {
        let builder = interface_iframe.builder().expect("NullPointerException");
        if !builder.is_instance_builder() {
            panic!("ClassCastException");
        }
        let names: Option<Names> = builder.get_names();
        let (iframe, pos) = {
            let b = class_node.borrow();
            (b.class_definition().unwrap().iframe.clone().expect("NullPointerException"), b.pos())
        };

        if let Some(names) = &names {
            let c = cx.borrow();
            let mut i = 0;
            loop {
                i = names.has_next(i);
                if i == -1 {
                    break;
                }
                let iu = i as usize;
                let name = names.get_name(iu);
                let namespace = names.get_namespace(iu);

                if name == "$construct" {
                    i += 1;
                    continue;
                }

                let lookup_kind = if names.get_type(iu) == Names::SET_NAMES { SET_TOKEN } else { GET_TOKEN };

                let mut error_id = 0;

                let slot_id = interface_iframe.get_slot_index(&c, lookup_kind, &name, Some(&namespace));
                let implied_id = interface_iframe.get_implicit_index(&c, slot_id, EMPTY_TOKEN);
                let orig_getter = slot_id == implied_id;

                if !iframe.has_name(&c, lookup_kind, &name, &namespace) {
                    error_id = kError_UnknownInterfaceMethod;
                } else {
                    let orig = interface_iframe.get_slot(&c, implied_id);

                    let slot_id = iframe.get_slot_index(&c, lookup_kind, &name, Some(&namespace));
                    let implied_id = iframe.get_implicit_index(&c, slot_id, EMPTY_TOKEN);
                    let slot_getter = slot_id == implied_id;
                    let slot = iframe.get_slot(&c, implied_id);

                    if !self.match_signatures(slot.as_ref().expect("NullPointerException"), orig.as_ref().expect("NullPointerException")) || orig_getter != slot_getter {
                        error_id = kError_IncompatibleInterfaceMethod;
                    }
                }

                if error_id != 0 {
                    let namespace_string = if namespace.ptr_eq(&c.public_namespace()) { "public".to_string() } else { namespace.name() };
                    let mut prop_type = "";
                    if names.get_type(iu) == Names::SET_NAMES {
                        prop_type = "set ";
                    } else if orig_getter {
                        prop_type = "get ";
                    }
                    let class_name = iframe.type_().expect("NullPointerException").type_value().name().to_string();
                    c.error3(pos, error_id, &format!("{}{}", prop_type, name), &namespace_string, &class_name);
                }
                i += 1;
            }
        }
        for base in interface_iframe.base_objs().unwrap_or_default() {
            self.check_class_interface_methods(cx, class_node, &base);
        }
    }

    fn check_interface_interface_methods(&mut self, cx: &ContextRef, class_node: &NodeRef, interface_iframe: &ObjectValue) {
        let builder = interface_iframe.builder().expect("NullPointerException");
        if !builder.is_instance_builder() {
            panic!("ClassCastException");
        }
        let names: Option<Names> = builder.get_names();
        let (iframe, pos) = {
            let b = class_node.borrow();
            (b.class_definition().unwrap().iframe.clone().expect("NullPointerException"), b.pos())
        };

        if let Some(names) = &names {
            let c = cx.borrow();
            let mut i = 0;
            loop {
                i = names.has_next(i);
                if i == -1 {
                    break;
                }
                let iu = i as usize;
                let name = names.get_name(iu);
                let namespace = names.get_namespace(iu);

                if name == "$construct" {
                    i += 1;
                    continue;
                }

                let lookup_kind = if names.get_type(iu) == Names::SET_NAMES { SET_TOKEN } else { GET_TOKEN };

                if iframe.has_name(&c, lookup_kind, &name, &namespace) {
                    let mut prop_type = "";
                    if names.get_type(iu) == Names::SET_NAMES {
                        prop_type = "set ";
                    } else if names.get_type(iu) == Names::GET_NAMES {
                        prop_type = "get ";
                    }
                    let iname = interface_iframe.type_().expect("NullPointerException").type_value().name().to_string();
                    c.error2(pos, kError_ConflictingInheritedNameInInterface, &format!("{}{}", prop_type, name), &iname);
                }
                i += 1;
            }
        }
        for base in interface_iframe.base_objs().unwrap_or_default() {
            self.check_interface_interface_methods(cx, class_node, &base);
        }
    }

    /// Check for data loss converting the literals of a Vector initializer to int or uint.
    fn check_literal_conversion(&mut self, cx: &ContextRef, dest_type: &TypeValue, element_list: &NodeRef) {
        let (conversion_to_int, conversion_to_uint) = {
            let c = cx.borrow();
            (dest_type.ptr_eq(&c.int_type()), dest_type.ptr_eq(&c.uint_type()))
        };

        if !conversion_to_int && !conversion_to_uint {
            return;
        }

        for item in items_of(&Some(element_list.clone())) {
            let item = nn(&item);
            let b = item.borrow();
            if b.is_literal_number() {
                let literal = b.as_literal_number().unwrap();
                let nv = literal.numeric_value.as_ref().expect("NullPointerException");
                let d_value = nv.double_value();

                if conversion_to_int {
                    if d_value != nv.int_value() as f64 {
                        cx.borrow().error1(b.get_position(), kError_LossyConversion, &dest_type.get_printable_name());
                    }
                } else if conversion_to_uint {
                    // uintValue() cannot be used: the parser parsed the numeric constant wrong.
                    if d_value < 0.0 || d_value != java_d2l(d_value) as f64 {
                        cx.borrow().error1(b.get_position(), kError_LossyConversion, &dest_type.get_printable_name());
                    }
                }
            }
        }
    }
}

fn java_rounding_mode(m: crate::asc::util::number_usage::RoundingMode) -> crate::asc::util::big_decimal::RoundingMode {
    use crate::asc::util::big_decimal::RoundingMode as B;
    use crate::asc::util::number_usage::RoundingMode as N;
    match m {
        N::Up => B::Up,
        N::Down => B::Down,
        N::Ceiling => B::Ceiling,
        N::Floor => B::Floor,
        N::HalfUp => B::HalfUp,
        N::HalfDown => B::HalfDown,
        N::HalfEven => B::HalfEven,
        N::Unnecessary => B::Unnecessary,
    }
}

fn items_of_params(node: &NodeRef) -> Vec<Option<NodeRef>> {
    node.borrow().as_parameter_list().unwrap().items.clone()
}

fn set_types_push(node: &NodeRef, t: TypeInfo) {
    node.borrow_mut().as_parameter_list_mut().unwrap().types.push(t);
}

impl Evaluator for ConstantEvaluator {
    // Called before every node: at a block leader, the reaching definitions restart from the block's in bits.
    fn check_feature(&mut self, _cx: &ContextRef, node: &NodeRef) -> bool {
        let b = node.borrow();
        if let Some(block) = &b.block {
            let same = self.block.as_ref().map_or(false, |cur| Rc::ptr_eq(cur, block));
            if !same && b.as_empty_statement().is_none() {
                self.rch_bits = block.borrow().in_bits.clone();
                self.block = Some(block.clone());
            }
        }
        true
    }

    fn evaluate_node(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        no_proto(cx)
    }

    fn evaluate_identifier(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let r = node.borrow().identifier().expect("ClassCastException").ref_.clone();
        if let Some(r) = &r {
            r.calc_use_definitions(&cx.borrow(), self.rch_bits.clone());
        }
        r.map(Value::Reference)
    }

    fn evaluate_qualified_identifier(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let qualifier = field!(node, qualified_identifier, qualifier);
        if qualifier.is_some() {
            self.eval(cx, &qualifier);
        }
        None
    }

    fn evaluate_qualified_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (qualifier, expr) = {
            let b = node.borrow();
            let q = b.as_qualified_expression().unwrap();
            (q.qid.qualifier.clone(), q.expr.clone())
        };
        if qualifier.is_some() {
            self.eval(cx, &qualifier);
        }
        if expr.is_some() {
            self.eval(cx, &expr);
        }
        None
    }

    fn evaluate_apply_type_expr(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (r, expr) = {
            let b = node.borrow();
            let s = b.selector().unwrap();
            (s.ref_.clone(), s.expr.clone())
        };
        let kind = EMPTY_TOKEN;
        if let Some(r) = &r {
            let val = self.eval(cx, &expr);
            let c = cx.borrow();
            r.calc_use_definitions(&c, self.rch_bits.clone());

            let _slot = r.get_slot_kind(&c, kind);
            let _type = r.get_type_kind(&c, kind);

            if c.use_static_semantics() {
                if let Some(t) = val.and_then(|v| v.as_type_value()) {
                    if !t.ptr_eq(&c.no_type()) && !t.is_parameterized() {
                        c.internal_error("Illegal use of parameterized type");
                    }
                }
            }
        }
        r.map(Value::Reference)
    }

    fn evaluate_call_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (r, expr, args, is_new, pos) = {
            let b = node.borrow();
            let c = b.as_call_expression().unwrap();
            (c.sel.ref_.clone(), c.sel.expr.clone(), c.args.clone(), c.is_new, b.pos())
        };
        let mut slot: Option<Slot> = None;
        let mut type_ = no_ti(cx);
        let kind = if is_new { NEW_TOKEN } else { EMPTY_TOKEN };

        if let Some(r) = &r {
            let c = cx.borrow();
            r.calc_use_definitions(&c, self.rch_bits.clone());

            slot = r.get_slot_kind(&c, kind);
            type_ = r.get_type_kind(&c, kind);
            if c.use_static_semantics() && slot.is_none() {
                slot = r.get_slot_kind(&c, GET_TOKEN);
                // Function or Class valued properties and getters returning them can be called; a Class valued
                // var acts like an explicit cast.
                if let Some(s) = &slot {
                    let t = s.get_type().and_then(|t| t.get_type_value());
                    if !is(&t, &c.type_type()) && !is(&t, &c.function_type()) && !is(&t, &c.object_type()) && !is(&t, &c.no_type()) {
                        slot = None;
                        r.set_slot(None);
                    }
                }

                if slot.is_none() {
                    match r.get_base() {
                        None => {
                            if c.statics.borrow().with_depth == -1 {
                                c.error1(pos, kError_Strict_PlainUndefinedMethod, &r.name());
                            }
                        }
                        Some(base) => {
                            if is_fixed_base(&c, &base) {
                                let typed = base.is_type_value() || base.type_().map_or(false, |t| !is(&t.get_type_value(), &c.no_type()));
                                if typed {
                                    let class_name = if base.is_type_value() { "Class".to_string() } else { type_name_of(&c, &base) };
                                    if base.has_name_unqualified(&c, &r.name(), GET_TOKEN) {
                                        c.error2(pos, kError_InaccessibleMethodReference, &r.name(), &class_name);
                                    } else {
                                        c.error2(pos, kError_Strict_UndefinedMethod, &r.name(), &class_name);
                                    }
                                }
                            }
                        }
                    }
                }
                if slot.is_some() {
                    // Do not permanently bind to the get slot.
                    r.set_slot(None);
                    slot = None;
                }
            }
            if slot.is_none() && is_new {
                let call_slot = r.get_slot_kind(&c, EMPTY_TOKEN);
                let get_slot = r.get_slot_kind(&c, GET_TOKEN);
                if let (Some(_), Some(get_slot)) = (&call_slot, &get_slot) {
                    if get_slot.declared_by().map_or(false, |d| is_class_builder(&d) || is_instance_builder(&d)) {
                        c.error(pos, kError_MethodIsNotAConstructor);
                    }
                }
            }
            if is_new {
                if let Some(st) = slot.as_ref().and_then(|s| s.get_type()) {
                    let bui = st.get_builder();
                    if bui.map_or(false, |b| b.is_class_builder() && b.class_data().is_interface) {
                        c.error(pos, kError_CannotInstantiateInterface);
                    }
                }
            }
        } else {
            self.eval(cx, &expr);
        }

        let mut call_of_class = false;
        if !is_new {
            if let Some(r) = &r {
                let c = cx.borrow();
                let new_slot = r.get_slot_kind(&c, NEW_TOKEN);
                // Global functions have new slots too, but only classes give them a type other than *.
                if new_slot.and_then(|s| s.get_type()).map_or(false, |t| !is(&t.get_type_value(), &c.no_type())) {
                    call_of_class = true;
                }
            }
        }

        let use_static = cx.borrow().use_static_semantics();
        if call_of_class {
            // Calling a class closure as a function.
            if let Some(args) = &args {
                if use_static && args.borrow().as_argument_list().unwrap().expected_types.is_none() {
                    let t = no_ti(cx);
                    let mut b = args.borrow_mut();
                    let a = b.as_argument_list_mut().unwrap();
                    a.add_type(Some(t));
                    a.add_decl_style(PARAM_Required);
                }
                evaluate(args, cx, self);
            } else if use_static {
                // An explicit cast needs an argument, except for the ES3 legacy Date(), which returns a string.
                if r.as_ref().unwrap().name() != "Date" {
                    cx.borrow().error1(pos, kError_WrongNumberOfArguments, "1");
                }
            }
        } else if let Some(args) = &args {
            if let Some(s) = &slot {
                let mut b = args.borrow_mut();
                let a = b.as_argument_list_mut().unwrap();
                a.expected_types = s.get_types().map(|t| t.borrow().iter().cloned().map(Some).collect());
                a.decl_styles = s.get_decl_styles().map(|d| d.borrow().clone());
            }
            let no_decl_styles = args.borrow().as_argument_list().unwrap().decl_styles.is_none();
            // A class without a declared constructor takes no arguments.
            if use_static && is_new && r.as_ref().map_or(false, |r| r.get_slot_kind(&cx.borrow(), NEW_TOKEN).is_some()) && no_decl_styles {
                cx.borrow().error1(pos, kError_WrongNumberOfArguments, "0");
            }
            evaluate(args, cx, self);
        } else if let Some(s) = &slot {
            let ds = s.get_decl_styles();
            if size_decls(&ds) != 0 && ds.as_ref().unwrap().borrow()[0] as i32 == PARAM_Required && use_static {
                let ds = ds.unwrap().borrow().clone();
                let mut expected_num_args = ds.len();
                while expected_num_args > 0 {
                    if ds[expected_num_args - 1] as i32 == PARAM_Required {
                        break;
                    }
                    expected_num_args -= 1;
                }
                cx.borrow().error1(pos, kError_WrongNumberOfArguments, &expected_num_args.to_string());
            }
        }

        // A new expression is never null.
        if is_new {
            type_ = type_.type_value().get_type_info(false);
        }

        proto(&type_)
    }

    fn evaluate_invoke(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (name, args) = {
            let b = node.borrow();
            let i = b.as_invoke().unwrap();
            (i.name.clone(), i.args.clone())
        };
        let c = cx.borrow();
        let (index, arg_type, type_) = match name.as_str() {
            "[[HasMoreNames]]" => (Some(UNARY_HasMoreNames), Some(c.int_type()), c.int_type()),
            "[[NextValue]]" => (Some(UNARY_NextValue), Some(c.int_type()), c.no_type()),
            "[[NextName]]" => (Some(UNARY_NextName), Some(c.int_type()), c.string_type()),
            "[[ToXMLString]]" => (Some(UNARY_ToXMLString), Some(c.no_type()), c.string_type()),
            "[[ToXMLAttrString]]" => (Some(UNARY_ToXMLAttrString), Some(c.no_type()), c.string_type()),
            "[[CheckFilterOperand]]" => (Some(UNARY_CheckFilterOp), Some(c.no_type()), c.no_type()),
            _ => (None, None, c.no_type()),
        };
        drop(c);
        if let Some(index) = index {
            node.borrow_mut().as_invoke_mut().unwrap().index = index;
            nn(&args).borrow_mut().as_argument_list_mut().unwrap().add_type(arg_type.map(|t| t.get_default_type_info()));
        }

        if args.is_some() {
            self.eval(cx, &args);
        }

        tv_proto(&type_)
    }

    fn evaluate_delete_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (r, expr) = {
            let b = node.borrow();
            let s = b.selector().unwrap();
            (s.ref_.clone(), s.expr.clone())
        };
        if let Some(r) = &r {
            let c = cx.borrow();
            r.calc_use_definitions(&c, self.rch_bits.clone());
            if c.use_static_semantics() {
                let slot = r.get_slot_kind(&c, GET_TOKEN);
                if slot.is_none() {
                    if let Some(base) = r.get_base() {
                        if is_fixed_base(&c, &base) {
                            report_undefined_property(&c, pos_of(&expr), &base, &r.name());
                        }
                    }
                }

                let base = r.get_base();
                let is_dynamic = base.map_or(false, |b| b.is_dynamic());

                if slot.is_some() && !self.in_with && !is_dynamic {
                    c.error1(pos_of(&expr), kError_Strict_AttemptToDeleteFixedProperty, &r.name());
                }
            }
        } else {
            self.eval(cx, &expr);
        }

        let slot = {
            let c = cx.borrow();
            let global = c.builtin_scope().expect("NullPointerException");
            global.get_slot(&c, SLOT_Global_DeleteOp)
        };
        node.borrow_mut().as_delete_expression_mut().unwrap().slot = slot.clone();

        proto(&slot.expect("NullPointerException").get_type().expect("NullPointerException"))
    }

    fn evaluate_get_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (r, base, mode, expr, pos) = {
            let b = node.borrow();
            let s = b.selector().unwrap();
            (s.ref_.clone(), s.base.clone(), s.get_mode(), s.expr.clone(), b.pos())
        };
        let val;
        if let Some(r) = &r {
            let c = cx.borrow();
            r.calc_use_definitions(&c, self.rch_bits.clone());

            if r.used_before_initialized() {
                if let Some(s) = r.get_slot_kind(&c, GET_TOKEN) {
                    s.set_needs_init(true);
                }
            }

            val = Some(Value::Reference(r.clone()));
            r.get_type_kind(&c, GET_TOKEN);
            if c.use_static_semantics() {
                let slot = r.get_slot_kind(&c, GET_TOKEN);
                if slot.is_none() {
                    if let Some(base) = r.get_base() {
                        if is_fixed_base(&c, &base) {
                            report_undefined_property(&c, pos, &base, &r.name());
                        }
                    }
                }
            }
        } else if base.is_some() && mode == LEFTBRACKET_TOKEN {
            self.eval(cx, &expr);
            let mut v = no_proto(cx);
            if let Some(bt) = base.as_ref().unwrap().type_() {
                let tv = bt.type_value();
                v = match tv.indexed_type() {
                    Some(it) => tv_proto(&it),
                    None => no_proto(cx),
                };
            }
            val = v;
        } else {
            // Without a reference, node.expr is a general expression.
            self.eval(cx, &expr);
            val = no_proto(cx);
        }

        val
    }

    fn evaluate_set_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        // Without a type annotation, the expected type of a definition is the union of the types of its uses,
        // except for indexed puts, which are Object since not all assignments may have been seen.
        let (r, base, mode, expr, args, is_initializer, pos) = {
            let b = node.borrow();
            let s = b.as_set_expression().unwrap();
            (s.sel.ref_.clone(), s.sel.base.clone(), s.sel.get_mode(), s.sel.expr.clone(), s.args.clone(), s.is_initializer, b.pos())
        };
        let args_ref = nn(&args);
        let add_type = |t: Option<TypeInfo>| args_ref.borrow_mut().as_argument_list_mut().expect("ClassCastException").add_type(t);

        if base.is_none() && !rb(&expr).is_lvalue() {
            cx.borrow().error(pos, kError_AssignmentToNonRefVar);
        } else if let Some(r) = &r {
            let c = cx.borrow();
            if is_initializer {
                r.calc_use_definitions(&c, self.rch_bits.clone());
                if !r.used_before_initialized() {
                    // A slot set before its declaration is initialized at the top of the method, so that the types
                    // agree at branch targets.
                    if let Some(s) = r.get_slot_kind(&c, GET_TOKEN) {
                        s.set_needs_init(true);
                    }
                }
            }

            let slot = r.get_slot_kind(&c, SET_TOKEN);

            r.get_type_kind(&c, SET_TOKEN);

            if let Some(slot) = &slot {
                // A negative var index means a setter; for slots inherited from an abc import only the original
                // slot has an accurate one.
                let types = slot.get_types();
                if slot.get_var_index() < 0 && size_types(&types) == 1 {
                    add_type(first_type(&types));
                } else {
                    add_type(slot.get_type());
                }
            } else {
                add_type(Some(c.no_type().get_default_type_info()));
            }
        } else if base.is_some() && mode == LEFTBRACKET_TOKEN {
            self.eval(cx, &expr);
            let mut t = no_ti(cx);
            if let Some(bt) = base.as_ref().unwrap().type_() {
                let tv = bt.type_value();
                t = match tv.indexed_type() {
                    Some(it) => it.get_default_type_info(),
                    None => no_ti(cx),
                };
            }
            add_type(Some(t));
        } else {
            self.eval(cx, &expr);
            add_type(Some(no_ti(cx)));
        }

        let val = self.eval(cx, &args);
        let type_ = match &val {
            Some(v) => v.get_type(&cx.borrow()),
            None => Some(no_ti(cx)),
        };

        let use_static = cx.borrow().use_static_semantics();
        if use_static {
            if let Some(r) = &r {
                let c = cx.borrow();
                let slot = r.get_slot_kind(&c, SET_TOKEN);

                // The kill bits that reach this definition; there should be none.
                let rchkill_bits_count = BitSet::and_count(&self.rch_bits, &node.borrow().get_kill_bits());
                let scope_index = r.get_scope_index_kind(GET_TOKEN);
                let base_index = c.get_scopes().len() as i32 - 1;
                let first_arg = || args_ref.borrow().as_argument_list().unwrap().items[0].clone();
                let slot_type_is = |s: &Slot, t: &TypeValue| ti_is(&s.get_type().expect("NullPointerException"), t);

                if slot.as_ref().map_or(false, |s| s.is_const() && slot_type_is(s, &c.type_type())) {
                    let slot = slot.as_ref().unwrap();
                    // The synthetic assignment of a class definition is fine; authors cannot assign to a class.
                    if !kind_is(&first_arg(), |n| n.class_definition().is_some()) {
                        // Only a class slot has a value.
                        if slot.get_object_value().is_some() {
                            c.error1(pos, kError_AssignmentToDefinedClass, &r.name());
                        }
                    }
                } else if slot.as_ref().map_or(false, |s| s.is_const() && slot_type_is(s, &c.function_type())) {
                    let slot = slot.as_ref().unwrap();
                    let fa = first_arg();
                    let is_fdn_init = kind_is(&fa, |n| n.as_function_common().map_or(false, |f| f.def.is_some()));
                    if !is_fdn_init {
                        // XML and XMLList properties such as .name may not be functions at runtime.
                        let is_xml_prop = is_xml_base(&c, &r.get_base());
                        if !is_xml_prop && slot.get_object_value().is_some() {
                            c.error1(pos, kError_AssignmentToDefinedFunction, &r.name());
                        }
                    }
                } else if slot.as_ref().map_or(false, |s| {
                    s.is_const()
                        && (s.is_imported() || scope_index != base_index || val.as_ref().expect("NullPointerException").has_value() || rchkill_bits_count > 0)
                }) {
                    c.error(pos, kError_AssignmentToConstVar);
                } else if c.use_static_semantics() && slot.is_none() {
                    // A get without a set means the property is read only.
                    let slot = r.get_slot_kind(&c, GET_TOKEN);
                    if let Some(slot) = &slot {
                        // Only the slot of a named function has a value.
                        if slot.get_type().map_or(false, |t| ti_is(&t, &c.function_type())) && slot.get_object_value().is_some() {
                            let fa = nn(&first_arg());
                            let fab = fa.borrow();
                            let cn_not_fcn = fab.as_coerce().map_or(false, |cn| !kind_is(&cn.expr, |n| n.as_function_common().is_some()));
                            // Not the synthetic assignment of a function definition.
                            if !fab.is_synthetic() || cn_not_fcn {
                                let is_xml_prop = is_xml_base(&c, &r.get_base());
                                if !is_xml_prop {
                                    let p = if pos == 0 { pos_of(&expr) } else { pos };
                                    c.error1(p, kError_AssignmentToDefinedFunction, &r.name());
                                }
                            }
                        } else {
                            c.error(pos, kError_PropertyIsReadOnly);
                        }
                    } else if let Some(base) = r.get_base() {
                        if is_fixed_base(&c, &base) {
                            report_undefined_property(&c, pos_of(&expr), &base, &r.name());
                        }
                    }
                }
            }
        }

        let gen_bits = field!(node, as_set_expression, gen_bits);
        cx.borrow().set_def_type(gen_bits, type_.clone());

        let (kill, gen) = {
            let b = node.borrow();
            (b.get_kill_bits(), b.get_gen_bits())
        };
        self.rch_bits = BitSet::reset_set(self.rch_bits.take(), kill, gen);

        if let Some(r) = &r {
            r.calc_use_definitions(&cx.borrow(), self.rch_bits.clone());
        }

        set_field!(node, as_set_expression_mut, value_type, type_);

        val
    }

    fn evaluate_this_expression(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        // Instance methods and accessors have `this` second from the end of the scope chain, global code has the
        // global object, and functions get it from the caller. The flow analyzer caught the error cases.
        let c = cx.borrow();
        let scope_depth = c.get_scopes().len() as i32 - 1;
        let this_context = self.this_context_last();
        let this_value = if this_context == instance_this {
            c.scope_at(scope_depth - 1)
        } else if this_context == global_this {
            c.scope_at(0)
        } else {
            ObjectValue::object_prototype()
        };
        Some(Value::Object(this_value))
    }

    fn evaluate_literal_boolean(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let v = node.borrow().as_literal_boolean().unwrap().value;
        let c = cx.borrow();
        Some(Value::Object(if v { c.boolean_true() } else { c.boolean_false() }))
    }

    fn evaluate_literal_number(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        // getValueOfNumberLiteral decides the type: whether the value fits its int representation.
        let (value, number_usage) = {
            let b = node.borrow();
            let n = b.as_literal_number().unwrap();
            (n.value.clone(), n.number_usage.clone())
        };
        let mut type_ = None;
        let nv = number_literal(cx, &value, &mut type_, number_usage.as_ref());
        {
            let mut b = node.borrow_mut();
            let n = b.as_literal_number_mut().unwrap();
            n.numeric_value = nv;
            n.type_ = type_.clone();
        }
        let ret = ObjectValue::with_value(&value, type_.as_ref());
        ret.set_number_usage(number_usage);
        Some(Value::Object(ret))
    }

    fn evaluate_literal_string(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let value = field!(node, as_literal_string, value);
        // Literal strings are never null.
        let t = cx.borrow().string_type().get_type_info(false);
        Some(Value::Object(ObjectValue::with_value_type_info(&value, &t)))
    }

    fn evaluate_literal_null(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        tv_proto(&cx.borrow().null_type())
    }

    fn evaluate_literal_reg_exp(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        tv_proto(&cx.borrow().reg_exp_type())
    }

    fn evaluate_paren_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_paren_list_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_literal_object(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let fieldlist = field!(node, as_literal_object, fieldlist);
        if fieldlist.is_some() {
            self.eval(cx, &fieldlist);
        }
        tv_proto(&cx.borrow().object_type())
    }

    fn evaluate_literal_field(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        {
            let name = field!(node, as_literal_field, name);
            let val = self.eval(cx, &name);
            let mut type_ = val.and_then(|v| v.get_type(&cx.borrow()));
            // The real type is string or number.
            let n = coerce(cx, &name, &mut type_, Some(no_ti(cx)), false, false);
            set_field!(node, as_literal_field_mut, name, n);
        }

        {
            let value = field!(node, as_literal_field, value);
            let val = self.eval(cx, &value);
            let mut type_ = val.and_then(|v| v.get_type(&cx.borrow()));
            // Object fields get a standard ECMA type.
            let n = coerce(cx, &value, &mut type_, Some(no_ti(cx)), false, false);
            set_field!(node, as_literal_field_mut, value, n);
        }

        no_proto(cx)
    }

    fn evaluate_literal_array(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let elementlist = field!(node, as_literal_array, elementlist);
        if elementlist.is_some() {
            let v = self.eval(cx, &elementlist);
            set_field!(node, as_literal_array_mut, value, v);
        }
        tv_proto(&cx.borrow().array_type())
    }

    fn evaluate_literal_vector(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (type_node, elementlist) = {
            let b = node.borrow();
            let v = b.as_literal_vector().unwrap();
            (v.type_.clone(), v.elementlist.clone())
        };
        let initializer_typeref = self.eval(cx, &type_node).expect("NullPointerException");
        let initializer_typevalue = initializer_typeref.get_value(&cx.borrow());

        if cx.borrow().use_static_semantics() && initializer_typevalue.is_none() {
            match &initializer_typeref {
                Value::Reference(r) => cx.borrow().error1(pos_of(&type_node), kError_UnknownType, &r.get_diagnostic_type_name()),
                _ => cx.borrow().error(pos_of(&type_node), kError_UnknownType),
            }
        }

        if let Some(el) = &elementlist {
            if let Some(tv) = initializer_typevalue.as_ref().and_then(|v| v.as_type_value()) {
                let vector_element_type = tv.indexed_type().expect("NullPointerException").get_default_type_info();

                {
                    let mut b = el.borrow_mut();
                    let a = b.as_argument_list_mut().expect("ClassCastException");
                    let start = a.expected_types.as_ref().map_or(0, |t| t.len());
                    for _ in start..a.items.len() {
                        a.add_type(Some(vector_element_type.clone()));
                        a.add_decl_style(PARAM_Required);
                    }
                }

                if cx.borrow().use_static_semantics() {
                    self.check_literal_conversion(cx, &vector_element_type.type_value(), el);
                }
            }

            let v = evaluate(el, cx, self);
            set_field!(node, as_literal_vector_mut, value, v);
        }
        Some(initializer_typeref)
    }

    fn evaluate_member_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (base, selector, r) = {
            let b = node.borrow();
            let m = b.as_member_expression().unwrap();
            (m.base.clone(), m.selector.clone(), m.ref_.clone())
        };
        let selector = nn(&selector);

        if let Some(base_node) = &base {
            let base_val = evaluate(base_node, cx, self);
            // An untypeable base, such as a conditional, is coerced to Object: (a > b ? "1" : 22.22).toString()
            let type_: Option<TypeInfo>;
            if base_val.is_none() {
                let mut actual = None;
                let nb = coerce(cx, &base, &mut actual, Some(no_ti(cx)), false, true);
                node.borrow_mut().as_member_expression_mut().unwrap().base = nb;
                let t = no_ti(cx);
                if let Some(r) = &r {
                    r.set_base(t.get_prototype());
                }
                type_ = Some(t);
            } else {
                type_ = base_val.as_ref().unwrap().get_type(&cx.borrow());
            }

            if let Some(base_val) = &base_val {
                let type_ = type_.expect("NullPointerException");
                if let Some(r) = &r {
                    let c = cx.borrow();
                    if ti_is(&type_, &c.type_type()) {
                        let val = base_val.get_value(&c);
                        r.set_base(val.and_then(|v| v.as_object_value()));
                    } else {
                        r.set_base(type_.get_prototype());
                    }

                    // An unqualified reference on an interface typed base sees all the interface namespaces.
                    if type_.is_interface() && !r.is_qualified() {
                        let mut interface_walker = InterfaceWalker::new(type_.type_value().as_object());
                        let namespaces = Namespaces::new();
                        while interface_walker.has_next() {
                            let i = interface_walker.next().unwrap();
                            namespaces.add(i.type_().expect("NullPointerException").type_value().into_object());
                        }
                        let interned = c.statics.borrow_mut().intern_namespaces.intern(&namespaces);
                        r.set_immutable_namespaces(interned);
                    }
                } else if selector.borrow().selector().expect("ClassCastException").get_mode() == LEFTBRACKET_TOKEN {
                    selector.borrow_mut().selector_mut().unwrap().set_base(type_.get_prototype());
                }
            }
        } else if let Some(r) = &r {
            let (sel_expr, is_set, is_call, is_package) = {
                let b = selector.borrow();
                (b.selector().unwrap().expr.clone(), b.is_set_expression(), b.is_call_expression(), b.selector().unwrap().is_package)
            };
            let c = cx.borrow();
            if c.use_static_semantics() {
                let base = r.get_base();
                let mut is_dynamic = false;

                if let Some(base) = &base {
                    // Only global Functions are dynamic; methods of a class are MethodClosures.
                    is_dynamic = base.is_dynamic() && !(is(&type_value_of(&c, base), &c.function_type()) && is_global_builder(base));
                }

                let ref_type = if !is_set { GET_TOKEN } else { SET_TOKEN };
                let slot = r.get_slot_kind(&c, ref_type);
                // Function is declared dynamic in Function.as, but is not dynamic in strict mode.
                if slot.is_none() && !self.in_with && !is_dynamic {
                    let scopes = c.get_scopes();
                    let sel_qualified = kind_is(&sel_expr, |n| n.qualified_identifier().is_some());
                    let mut x = 0;
                    while x < scopes.len() {
                        let scope = &scopes[x];
                        // A qualified reference falls through to the normal unfound property error.
                        if is_instance_builder(scope) && !sel_qualified && scope.has_name_unqualified(&c, &r.name(), GET_TOKEN) {
                            let tn = scope.type_().expect("NullPointerException").get_name_cx(&c).to_string();
                            c.error2(pos_of(&sel_expr), kError_InaccessiblePropertyReference, &r.name(), &tn);
                            break;
                        }
                        x += 1;
                    }
                    // Bad function calls were reported by the CallExpressionNode.
                    if x == scopes.len() && !is_call {
                        let mut qualified_pkg_name = None;
                        if sel_qualified {
                            let namespace = r.get_immutable_namespaces().at(0);
                            if namespace.is_package() {
                                qualified_pkg_name = Some(namespace.name());
                            }
                        }
                        if is_set {
                            let slot = r.get_slot_kind(&c, GET_TOKEN);
                            let is_function = slot.as_ref().map_or(false, |s| {
                                s.get_type().map_or(false, |t| ti_is(&t, &c.function_type())) && s.get_object_value().is_some()
                            });
                            // Setting a function is reported by the SetExpressionNode.
                            if !is_function {
                                match &qualified_pkg_name {
                                    Some(q) => c.error2(pos_of(&sel_expr), kError_UnfoundPackageProperty, &r.name(), q),
                                    None => c.error1(pos_of(&sel_expr), kError_UnfoundProperty, &r.name()),
                                }
                            }
                        } else {
                            match &qualified_pkg_name {
                                Some(q) => c.error2(pos_of(&sel_expr), kError_UnfoundPackageProperty, &r.get_diagnostic_type_name(), q),
                                None => c.error1(pos_of(&sel_expr), kError_UnfoundProperty, &r.get_diagnostic_type_name()),
                            }
                        }
                    }
                }
            }
            if is_package && r.get_slot_index(GET_TOKEN) < 0 {
                c.error1(pos_of(&sel_expr), kError_IllegalPackageReference, &r.name());
            }
        }

        let val = evaluate(&selector, cx, self);

        // A property such as .name of an XML or XMLList base may resolve to a method slot but be a dynamic attribute
        // or child, so the result is * to avoid bogus strict coercion errors.
        let base = field!(node, as_member_expression, base);
        if let Some(m) = base.as_ref().and_then(|b| b.borrow().as_member_expression().map(|m| m.ref_.clone())) {
            let c = cx.borrow();
            let s = m.and_then(|r| r.get_slot_kind(&c, GET_TOKEN));
            let slot_type = s.and_then(|s| s.get_type()).and_then(|t| t.get_type_value());
            if is(&slot_type, &c.xml_type()) || is(&slot_type, &c.xml_list_type()) {
                return tv_proto(&c.no_type());
            }
        }

        val
    }

    fn evaluate_unary_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, op, number_usage) = {
            let b = node.borrow();
            let u = b.as_unary_expression().unwrap();
            (u.expr.clone(), u.op, u.number_usage.clone())
        };
        let mut slot_index = 0;
        let mut val = self.eval(cx, &expr);
        let mut type_ = val.as_ref().and_then(|v| v.get_type(&cx.borrow()));

        let current_number_type = Self::is_unary_numeric(op).then(|| current_number_type(&cx.borrow(), &number_usage));

        match op {
            VOID_TOKEN => slot_index = SLOT_Global_VoidOp,
            TYPEOF_TOKEN => slot_index = SLOT_Global_TypeofOp,
            PLUS_TOKEN | MINUS_TOKEN | BITWISENOT_TOKEN => {
                slot_index = match op {
                    PLUS_TOKEN => SLOT_Global_UnaryPlusOp,
                    MINUS_TOKEN => SLOT_Global_UnaryMinusOp,
                    _ => SLOT_Global_BitwiseNotOp,
                };
                coerce(cx, &expr, &mut type_, current_number_type.map(|t| t.get_default_type_info()), false, false);
            }
            NOT_TOKEN => {
                slot_index = SLOT_Global_LogicalNotOp;
                let b = cx.borrow().boolean_type().get_default_type_info();
                coerce(cx, &expr, &mut type_, Some(b), false, false);
            }
            _ => cx.borrow().internal_error("unrecognized unary operator"),
        }

        if op == VOID_TOKEN {
            val = tv_proto(&cx.borrow().void_type());
        } else {
            if let Some(v) = &val {
                // A further evaluated value has the same or a more specific type, unless it is undefined, in which
                // case the VM uses the type of the original value.
                let c = cx.borrow();
                if let Some(const_val) = v.get_value(&c) {
                    let const_val_type = const_val.get_type(&c).expect("NullPointerException");
                    if !const_val_type.type_value().equals(&c.void_type()) {
                        type_ = Some(const_val_type);
                    }
                }
            } else {
                type_ = None;
            }

            let slot = {
                let c = cx.borrow();
                let global = c.builtin_scope().expect("NullPointerException");
                // Overloading does not work correctly for uint.
                if let Some(t) = &type_ {
                    if !ti_is(t, &c.uint_type()) {
                        slot_index = global.get_overload_index(&c, slot_index, t.get_type_value().as_ref());
                    }
                }
                global.get_slot(&c, slot_index).expect("NullPointerException")
            };

            // typeof must give 'undefined' for an undefined variable, so its operand is not coerced to Object.
            if op != TYPEOF_TOKEN {
                let e = coerce(cx, &expr, &mut type_, first_type(&slot.get_types()), false, false);
                set_field!(node, as_unary_expression_mut, expr, e);
            }

            set_field!(node, as_unary_expression_mut, slot, Some(slot.clone()));

            val = proto(&slot.get_type().expect("NullPointerException"));
        }
        val
    }

    fn evaluate_increment(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, mode, r, op, number_usage, identifier, pos) = {
            let b = node.borrow();
            let n = b.as_increment().unwrap();
            (n.sel.expr.clone(), n.sel.get_mode(), n.sel.ref_.clone(), n.op, n.number_usage.clone(), n.sel.get_identifier(), b.pos())
        };
        let mut val = self.eval(cx, &expr);
        if !(mode == DOT_TOKEN || mode == EMPTY_TOKEN) {
            val = None;
        }

        let type_ = val.as_ref().and_then(|v| v.get_type(&cx.borrow()));
        let bad_operand = if op == PLUSPLUS_TOKEN { kError_InvalidIncrementOperand } else { kError_InvalidDecrementOperand };

        if let Some(r) = &r {
            let c = cx.borrow();
            r.calc_use_definitions(&c, self.rch_bits.clone());
            let slot = r.get_slot_kind(&c, GET_TOKEN);
            if c.use_static_semantics() && slot.is_none() {
                if let Some(base) = r.get_base() {
                    if is_fixed_base(&c, &base) {
                        report_undefined_property(&c, pos, &base, &r.name());
                    }
                }
            }

            // Vars do not get a set slot.
            if let Some(slot) = &slot {
                if !slot.is_variable_slot() && r.get_slot_kind(&c, SET_TOKEN).is_none() {
                    c.error(pos, bad_operand);
                }
            }
        } else {
            let has_nss = identifier.map_or(false, |i| i.borrow().as_qualified_expression().map_or(false, |q| q.nss.is_some()));
            if has_nss {
                cx.borrow().error(pos, bad_operand);
            }
        }

        let mut slot_index = 0;
        match op {
            PLUSPLUS_TOKEN => slot_index = SLOT_Global_IncrementOp,
            MINUSMINUS_TOKEN => slot_index = SLOT_Global_DecrementOp,
            _ => {}
        }

        {
            let c = cx.borrow();
            let global = c.builtin_scope().expect("NullPointerException");
            // Overloading does not work correctly for uint.
            if let Some(t) = &type_ {
                if !ti_is(t, &c.uint_type()) {
                    slot_index = global.get_overload_index(&c, slot_index, t.get_type_value().as_ref());
                }
            }
            let s = global.get_slot(&c, slot_index);
            node.borrow_mut().as_increment_mut().unwrap().slot = s;
        }

        // The static type from the reference value, or of the literal.
        let mut atype = {
            let c = cx.borrow();
            let s = val.as_ref().and_then(|v| v.as_reference_value()).and_then(|r| r.get_slot(&c));
            match s {
                Some(s) => s.get_type(),
                None => match &val {
                    Some(v) => v.get_type(&c),
                    None => Some(c.no_type().get_default_type_info()),
                },
            }
        };

        let cnt = current_number_type(&cx.borrow(), &number_usage);
        coerce(cx, &expr, &mut atype, Some(cnt.get_default_type_info()), false, false);

        type_.and_then(|t| proto(&t))
    }

    fn evaluate_binary_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (lhs, rhs, op, number_usage, pos) = {
            let b = node.borrow();
            let n = b.as_binary_expression().unwrap();
            (n.lhs.clone(), n.rhs.clone(), n.op, n.number_usage.clone(), b.pos())
        };
        let mut val = None;
        let mut lhsval = None;
        let mut rhsval = None;
        let mut result_type: Option<TypeInfo> = None;

        let mut lhstype: Option<TypeInfo> = None;
        let mut rhstype: Option<TypeInfo> = None;
        if lhs.is_some() {
            let v = self.eval(cx, &lhs);
            if let Some(v) = v {
                let c = cx.borrow();
                let const_val = v.get_value(&c);
                // The static type from the reference value, or of the literal.
                let s = v.as_reference_value().and_then(|r| r.get_slot(&c));
                lhstype = match s {
                    Some(s) => s.get_type(),
                    None => v.get_type(&c),
                };
                lhsval = const_val.filter(|cv| cv.has_value());
            }
        }
        if rhs.is_some() {
            let v = self.eval(cx, &rhs);
            if let Some(v) = v {
                let c = cx.borrow();
                let const_val = v.get_value(&c);
                let s = v.as_reference_value().and_then(|r| r.get_slot(&c));
                rhstype = match s {
                    Some(s) => s.get_type(),
                    None => v.get_type(&c),
                };

                if op == AS_TOKEN {
                    // The result of "as" has the type of its right operand.
                    let typeval = const_val.as_ref().and_then(|cv| cv.as_type_value()).unwrap_or_else(|| c.object_type());
                    result_type = Some(typeval.get_default_type_info());
                }
                rhsval = const_val.filter(|cv| cv.has_value());
            }
        }

        let mut slot_index = 0;

        // Null types mean no type checking or opcode overriding.
        if cx.borrow().use_static_semantics() && lhstype.is_some() && rhstype.is_some() {
            if let EQUALS_TOKEN
            | NOTEQUALS_TOKEN
            | LESSTHAN_TOKEN
            | GREATERTHAN_TOKEN
            | LESSTHANOREQUALS_TOKEN
            | GREATERTHANOREQUALS_TOKEN
            | STRICTNOTEQUALS_TOKEN
            | STRICTEQUALS_TOKEN = op
            {
                // Comparisons between incompatible types.
                let c = cx.borrow();
                let (l, r) = (lhstype.as_ref().unwrap(), rhstype.as_ref().unwrap());
                let (lt, rt) = (l.get_type_value(), r.get_type_value());
                let incompatible = || {
                    c.error2(pos, kError_IncompatableValueComparison, &l.get_name_cx(&c).to_string(), &r.get_name_cx(&c).to_string())
                };
                // Everything converts to Boolean implicitly, so `if (s == true)` is like `if (s)`.
                if is(&lt, &c.no_type())
                    || is(&rt, &c.no_type())
                    || is(&lt, &c.boolean_type())
                    || is(&rt, &c.boolean_type())
                    || l.is_interface()
                    || r.is_interface()
                    || l.includes(&c, r)
                    || r.includes(&c, l)
                {
                } else if is(&lt, &c.null_type()) || is(&rt, &c.null_type()) {
                    // null against a non-nullable type.
                    if is(&lt, &c.null_type()) && (r.type_value().is_numeric(&c) || is(&rt, &c.boolean_type())) {
                        incompatible();
                    } else if is(&rt, &c.null_type()) && (l.type_value().is_numeric(&c) || is(&lt, &c.boolean_type())) {
                        incompatible();
                    }
                } else if (is(&lt, &c.xml_type()) && is(&rt, &c.string_type())) || (is(&rt, &c.string_type()) && is(&lt, &c.xml_type())) {
                    // E4X compares XML with Strings directly: <a>test</a> == "test" is true.
                } else if !(l.type_value().is_numeric(&c) && r.type_value().is_numeric(&c)) {
                    // Unrelated types, unless both are numbers.
                    incompatible();
                }
            }
        }

        let lhstypeval = lhstype.as_ref().and_then(|t| t.get_type_value());
        let rhstypeval = rhstype.as_ref().and_then(|t| t.get_type_value());

        let current_number_type = || current_number_type(&cx.borrow(), &number_usage);
        let numeric_operands = |lhstype: &mut Option<TypeInfo>, rhstype: &mut Option<TypeInfo>| {
            let t = current_number_type().get_default_type_info();
            coerce(cx, &lhs, lhstype, Some(t.clone()), false, false);
            coerce(cx, &rhs, rhstype, Some(t), false, false);
        };
        let int_or_uint = |t: &Option<TypeValue>| {
            let c = cx.borrow();
            is(t, &c.int_type()) || is(t, &c.uint_type())
        };

        match op {
            MULT_TOKEN => {
                slot_index = SLOT_Global_MultiplyOp;
                if int_or_uint(&lhstypeval) && int_or_uint(&rhstypeval) {
                    result_type = Some(cx.borrow().int_type().get_default_type_info());
                }
                numeric_operands(&mut lhstype, &mut rhstype);
            }
            DIV_TOKEN => {
                slot_index = SLOT_Global_DivideOp;
                numeric_operands(&mut lhstype, &mut rhstype);
            }
            MODULUS_TOKEN => {
                slot_index = SLOT_Global_ModulusOp;
                numeric_operands(&mut lhstype, &mut rhstype);
            }
            PLUS_TOKEN => {
                slot_index = SLOT_Global_BinaryPlusOp;
                let (both_xml, both_numeric, any_string) = {
                    let c = cx.borrow();
                    let xml = |t: &Option<TypeValue>| is(t, &c.xml_list_type()) || is(t, &c.xml_type());
                    let numeric = |t: &Option<TypeValue>| t.as_ref().map_or(false, |t| t.is_numeric(&c));
                    (
                        xml(&lhstypeval) && xml(&rhstypeval),
                        numeric(&lhstypeval) && numeric(&rhstypeval),
                        is(&lhstypeval, &c.string_type()) || is(&rhstypeval, &c.string_type()),
                    )
                };
                if both_xml {
                    result_type = Some(cx.borrow().xml_list_type().get_default_type_info());
                } else if both_numeric {
                    result_type = Some(current_number_type().get_default_type_info());
                } else if any_string {
                    // Anything plus a string is a string.
                    result_type = Some(cx.borrow().string_type().get_default_type_info());
                } else {
                    // Could be a number, a string or an XMLList.
                    result_type = Some(no_ti(cx));
                }
            }
            MINUS_TOKEN => {
                slot_index = SLOT_Global_BinaryMinusOp;
                numeric_operands(&mut lhstype, &mut rhstype);
                if int_or_uint(&lhstypeval) && int_or_uint(&rhstypeval) {
                    result_type = Some(cx.borrow().int_type().get_default_type_info());
                }
            }
            LEFTSHIFT_TOKEN => {
                slot_index = SLOT_Global_LeftShiftOp;
                numeric_operands(&mut lhstype, &mut rhstype);
            }
            RIGHTSHIFT_TOKEN => {
                slot_index = SLOT_Global_RightShiftOp;
                numeric_operands(&mut lhstype, &mut rhstype);
            }
            UNSIGNEDRIGHTSHIFT_TOKEN => {
                slot_index = SLOT_Global_UnsignedRightShiftOp;
                numeric_operands(&mut lhstype, &mut rhstype);
            }
            LESSTHAN_TOKEN => slot_index = SLOT_Global_LessThanOp,
            GREATERTHAN_TOKEN => slot_index = SLOT_Global_GreaterThanOp,
            LESSTHANOREQUALS_TOKEN => slot_index = SLOT_Global_LessThanOrEqualOp,
            GREATERTHANOREQUALS_TOKEN => slot_index = SLOT_Global_GreaterThanOrEqualOp,
            INSTANCEOF_TOKEN => slot_index = SLOT_Global_InstanceofOp,
            IN_TOKEN => slot_index = SLOT_Global_InOp,
            IS_TOKEN | AS_TOKEN => {
                slot_index = if op == IS_TOKEN { SLOT_Global_IsLateOp } else { SLOT_Global_AsLateOp };
                let t = cx.borrow().type_type().get_default_type_info();
                coerce(cx, &rhs, &mut rhstype, Some(t), false, false);
            }
            EQUALS_TOKEN => slot_index = SLOT_Global_EqualsOp,
            NOTEQUALS_TOKEN => slot_index = SLOT_Global_NotEqualsOp,
            STRICTEQUALS_TOKEN => slot_index = SLOT_Global_StrictEqualsOp,
            STRICTNOTEQUALS_TOKEN => slot_index = SLOT_Global_StrictNotEqualsOp,
            BITWISEAND_TOKEN => {
                slot_index = SLOT_Global_BitwiseAndOp;
                numeric_operands(&mut lhstype, &mut rhstype);
            }
            BITWISEXOR_TOKEN => {
                slot_index = SLOT_Global_BitwiseXorOp;
                numeric_operands(&mut lhstype, &mut rhstype);
            }
            BITWISEOR_TOKEN => {
                slot_index = SLOT_Global_BitwiseOrOp;
                numeric_operands(&mut lhstype, &mut rhstype);
            }
            LOGICALAND_TOKEN | LOGICALOR_TOKEN => {
                slot_index = if op == LOGICALAND_TOKEN { SLOT_Global_LogicalAndOp } else { SLOT_Global_LogicalOrOp };
                if opt_ti_eq(&lhstype, &rhstype) {
                    result_type = lhstype.clone();
                }
            }
            // Already folded.
            EMPTY_TOKEN => {}
            _ => cx.borrow().internal_error("unrecognized binary operator"),
        }

        if op != EMPTY_TOKEN {
            let slot = {
                let c = cx.borrow();
                let global = c.builtin_scope().expect("NullPointerException");
                // Overloading does not work correctly for uint.
                if !is(&lhstypeval, &c.uint_type()) && !is(&rhstypeval, &c.uint_type()) {
                    slot_index = global.get_overload_index2(&c, slot_index, lhstypeval.as_ref(), rhstypeval.as_ref());
                }
                global.get_slot(&c, slot_index).expect("NullPointerException")
            };

            // Coerce the operands to the types the overloaded operator expects.
            let types = slot.get_types();
            let (e0, e1) = if size_types(&types) > 0 {
                let t = types.as_ref().unwrap().borrow();
                (Some(t[0].clone()), Some(t[1].clone()))
            } else {
                (None, None)
            };
            let new_lhs = coerce(cx, &lhs, &mut lhstype, e0, false, false);
            set_field!(node, as_binary_expression_mut, lhs, new_lhs);
            let new_rhs = coerce(cx, &rhs, &mut rhstype, e1, false, false);
            {
                let mut b = node.borrow_mut();
                let n = b.as_binary_expression_mut().unwrap();
                n.rhs = new_rhs;
                n.lhstype = lhstype.clone();
                n.rhstype = rhstype.clone();
                n.slot = Some(slot.clone());
            }

            if let (Some(Value::Object(l)), Some(Value::Object(r))) = (&lhsval, &rhsval) {
                val = self.compute_binary_expr(cx, op, l, r, number_usage.as_ref());
            } else if let Some(rt) = &result_type {
                val = proto(rt);
            } else {
                val = proto(&slot.get_type().expect("NullPointerException"));
            }
        }

        val
    }

    fn evaluate_conditional_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (condition, thenexpr, elseexpr) = {
            let b = node.borrow();
            let n = b.as_conditional_expression().unwrap();
            (n.condition.clone(), n.thenexpr.clone(), n.elseexpr.clone())
        };
        if condition.is_some() {
            let val = self.eval(cx, &condition);
            let mut type_ = match val {
                Some(v) => v.get_type(&cx.borrow()),
                None => Some(no_ti(cx)),
            };
            let b = cx.borrow().boolean_type().get_default_type_info();
            let n = coerce(cx, &condition, &mut type_, Some(b), true, false);
            set_field!(node, as_conditional_expression_mut, condition, n);
        }

        // The branches are coerced to the expected type before the join node, by Context.coerce.
        if thenexpr.is_some() {
            let v = self.eval(cx, &thenexpr);
            set_field!(node, as_conditional_expression_mut, thenvalue, v);
        }

        if elseexpr.is_some() {
            let v = self.eval(cx, &elseexpr);
            set_field!(node, as_conditional_expression_mut, elsevalue, v);
        }

        None
    }

    fn evaluate_argument_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let mut result = None;
        let (items_size, expected_types, decl_styles, pos) = {
            let b = node.borrow();
            let a = b.as_argument_list().unwrap();
            (a.items.len(), a.expected_types.clone(), a.decl_styles.clone(), b.pos())
        };
        let types_size = expected_types.as_ref().map_or(0, |t| t.len());
        let item_at = |i: usize| node.borrow().as_argument_list().unwrap().items[i].clone();
        let set_item = |i: usize, n: Option<NodeRef>| node.borrow_mut().as_argument_list_mut().unwrap().items[i] = n;

        if decl_styles.as_ref().map_or(0, |d| d.len()) != 0 {
            // Declared parameter types: match them against the arguments.
            let ds = decl_styles.unwrap();
            let mut param_count = 0usize;
            let mut val_index = 0usize;
            let mut type_index: i32 = 0;

            let et = expected_types.expect("NullPointerException");
            // Void marks a class method that declares no parameters.
            if et[0].as_ref().map_or(false, |t| !ti_is(t, &cx.borrow().void_type())) {
                while val_index < items_size && (type_index as usize) < types_size {
                    let item = item_at(val_index);
                    let type_ = et[type_index as usize].clone();

                    let val = self.eval(cx, &item);
                    let mut actual = None;
                    if let Some(v) = val {
                        actual = v.get_type(&cx.borrow());
                    }

                    if ds[param_count] as i32 != PARAM_Rest {
                        let n = coerce(cx, &item, &mut actual, type_.clone(), false, false);
                        set_item(val_index, n);
                        result = proto(type_.as_ref().expect("NullPointerException"));
                        param_count += 1;
                    } else {
                        let n = coerce(cx, &item, &mut actual, Some(no_ti(cx)), false, false);
                        set_item(val_index, n);
                        result = actual.as_ref().and_then(proto);
                        type_index -= 1;
                    }
                    val_index += 1;
                    type_index += 1;
                }
            }
            if cx.borrow().use_static_semantics() {
                let too_many_args = val_index < items_size;
                let too_few_args = (type_index as usize) < types_size && ds[param_count] as i32 == PARAM_Required;

                if too_many_args || too_few_args {
                    let mut expected_num_args = if ds[0] as i32 == PARAM_Void { 0 } else { types_size };
                    if too_few_args {
                        while expected_num_args > 0 {
                            if ds[expected_num_args - 1] as i32 == PARAM_Required {
                                break;
                            }
                            expected_num_args -= 1;
                        }
                    } else if expected_num_args == 1 && ds[0] as i32 == PARAM_Void {
                        expected_num_args = 0;
                    }

                    let code = if too_few_args { kError_WrongNumberOfArguments } else { kError_TooManyArguments };
                    cx.borrow().error1(pos, code, &expected_num_args.to_string());
                }
            }
            // With too few parameters the remaining arguments were not evaluated above.
            while val_index < items_size {
                let item = item_at(val_index);
                self.eval(cx, &item);
                val_index += 1;
            }
        } else {
            for i in 0..items_size {
                let item = item_at(i);

                let val = self.eval(cx, &item);
                let mut actual = val.and_then(|v| v.get_type(&cx.borrow()));

                let expected = if i >= types_size { None } else { expected_types.as_ref().unwrap()[i].clone() };

                let n = coerce(cx, &item, &mut actual, expected.clone(), false, false);
                set_item(i, n);
                result = match &expected {
                    Some(e) => proto(e),
                    None => actual.as_ref().and_then(proto),
                };
            }
        }

        result
    }

    fn evaluate_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let mut val = None;

        let items = items_of(&Some(node.clone()));
        let size = items.len();
        for (i, item) in items.iter().enumerate() {
            val = self.eval(cx, item);
            if i + 1 != size {
                item.as_ref().unwrap().borrow_mut().void_result();
            }
        }

        val
    }

    // Statements

    fn evaluate_statement_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let mut n = 0;
        while n < node.borrow().as_statement_list().unwrap().items.len() {
            let item = node.borrow().as_statement_list().unwrap().items[n].clone();
            if !self.doing_method {
                self.doing_method = true;
            }

            // Nodes from imported abc files have no blocks, though they are synthetic.
            if let Some(item) = item {
                let terminal = item.borrow().block.as_ref().map_or(false, |b| b.borrow().is_terminal);
                if terminal {
                    // Unreachable code is removed, except variable definitions: those in a switch are hoisted to the
                    // top of its statement list where no path enters them, but still type their slots and constants.
                    if item.borrow().as_variable_definition().is_none() {
                        let empty = Context::get_node_factory(cx).borrow_mut().empty_statement();
                        node.borrow_mut().as_statement_list_mut().unwrap().items[n] = Some(empty);
                    }
                } else {
                    evaluate(&item, cx, self);
                }
            }
            n += 1;
        }

        Some(Value::Object(ObjectValue::undefined_value()))
    }

    fn evaluate_empty_statement(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        no_proto(cx)
    }

    fn evaluate_expression_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, expected_type) = {
            let b = node.borrow();
            let e = b.as_expression_statement().unwrap();
            (e.expr.clone(), e.expected_type.clone())
        };
        let mut result = None;
        if expr.is_some() {
            result = self.eval(cx, &expr);
            let mut type_ = match &result {
                Some(r) => r.get_type(&cx.borrow()),
                None => Some(no_ti(cx)),
            };
            let n = coerce(cx, &expr, &mut type_, expected_type.map(|t| t.get_default_type_info()), false, false);
            set_field!(node, as_expression_statement_mut, expr, n);
        }

        // Expression statements define the completion value.
        let (kill, gen) = {
            let b = node.borrow();
            (b.get_kill_bits(), b.get_gen_bits())
        };
        self.rch_bits = BitSet::reset_set(self.rch_bits.take(), kill, gen);

        result
    }

    fn evaluate_labeled_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let statement = field!(node, as_labeled_statement, statement);
        if statement.is_some() {
            self.eval(cx, &statement);
        }
        no_proto(cx)
    }

    fn evaluate_if_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (is_true, is_false, condition, thenactions, elseactions) = {
            let b = node.borrow();
            let n = b.as_if_statement().unwrap();
            (n.is_true, n.is_false, n.condition.clone(), n.thenactions.clone(), n.elseactions.clone())
        };

        if is_true {
            if thenactions.is_some() {
                self.eval(cx, &thenactions);
            }
        } else if is_false {
            if elseactions.is_some() {
                self.eval(cx, &elseactions);
            }
        } else {
            if condition.is_some() {
                let val = self.eval(cx, &condition);
                let mut type_ = match val {
                    Some(v) => v.get_type(&cx.borrow()),
                    None => Some(no_ti(cx)),
                };
                let b = cx.borrow().boolean_type().get_default_type_info();
                let n = coerce(cx, &condition, &mut type_, Some(b), true, false);
                set_field!(node, as_if_statement_mut, condition, n);
            }
            if thenactions.is_some() {
                self.eval(cx, &thenactions);
            }
            if elseactions.is_some() {
                self.eval(cx, &elseactions);
            }
        }
        no_proto(cx)
    }

    fn evaluate_switch_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, statements) = {
            let b = node.borrow();
            let n = b.as_switch_statement().unwrap();
            (n.expr.clone(), n.statements.clone())
        };
        if expr.is_some() {
            let val = self.eval(cx, &expr);
            let mut type_ = match val {
                Some(v) => v.get_type(&cx.borrow()),
                None => Some(no_ti(cx)),
            };
            let n = coerce(cx, &expr, &mut type_, Some(no_ti(cx)), false, false);
            set_field!(node, as_switch_statement_mut, expr, n);
        }
        if statements.is_some() {
            self.eval(cx, &statements);
        }

        no_proto(cx)
    }

    fn evaluate_case_label(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let label = field!(node, as_case_label, label);
        if label.is_some() {
            self.eval(cx, &label);
        }
        no_proto(cx)
    }

    fn evaluate_do_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, statements) = {
            let b = node.borrow();
            let n = b.as_do_statement().unwrap();
            (n.expr.clone(), n.statements.clone())
        };
        if expr.is_some() {
            let val = self.eval(cx, &expr);
            let mut type_ = match val {
                Some(v) => v.get_type(&cx.borrow()),
                None => Some(no_ti(cx)),
            };
            let b = cx.borrow().boolean_type().get_default_type_info();
            let n = coerce(cx, &expr, &mut type_, Some(b), true, false);
            set_field!(node, as_do_statement_mut, expr, n);
        }
        if statements.is_some() {
            self.eval(cx, &statements);
        }
        no_proto(cx)
    }

    fn evaluate_while_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, statement) = {
            let b = node.borrow();
            let n = b.as_while_statement().unwrap();
            (n.expr.clone(), n.statement.clone())
        };
        if statement.is_some() {
            self.eval(cx, &statement);
        }

        if expr.is_some() {
            let val = self.eval(cx, &expr);
            let mut type_ = match val {
                Some(v) => v.get_type(&cx.borrow()),
                None => Some(no_ti(cx)),
            };
            let b = cx.borrow().boolean_type().get_default_type_info();
            let n = coerce(cx, &expr, &mut type_, Some(b), true, false);
            set_field!(node, as_while_statement_mut, expr, n);
        }
        no_proto(cx)
    }

    fn evaluate_for_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let initialize = field!(node, as_for_statement, initialize);
        if let Some(init) = &initialize {
            if init.borrow().is_definition() {
                let es = initializer_statement(init, cx);
                let expr = field!(es, as_expression_statement, expr);
                set_field!(node, as_for_statement_mut, initialize, expr);
            }
            let init = field!(node, as_for_statement, initialize);
            self.eval(cx, &init);
            nn(&init).borrow_mut().void_result();
        }
        let (statement, increment, test) = {
            let b = node.borrow();
            let n = b.as_for_statement().unwrap();
            (n.statement.clone(), n.increment.clone(), n.test.clone())
        };
        if statement.is_some() {
            self.eval(cx, &statement);
        }
        if let Some(inc) = &increment {
            evaluate(inc, cx, self);
            inc.borrow_mut().void_result();
        }
        if test.is_some() {
            let val = self.eval(cx, &test);
            let mut type_ = match val {
                Some(v) => v.get_type(&cx.borrow()),
                None => Some(no_ti(cx)),
            };
            let b = cx.borrow().boolean_type().get_default_type_info();
            let n = coerce(cx, &test, &mut type_, Some(b), true, false);
            set_field!(node, as_for_statement_mut, test, n);
        }
        no_proto(cx)
    }

    fn evaluate_with_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, statement, activation) = {
            let b = node.borrow();
            let n = b.as_with_statement().unwrap();
            (n.expr.clone(), n.statement.clone(), n.activation.clone())
        };
        if expr.is_some() {
            self.eval(cx, &expr);
        }

        if statement.is_some() {
            cx.borrow().push_scope(activation.expect("NullPointerException"));

            let saved_in_with = self.in_with;
            self.in_with = true;

            let save_with_depth = cx.borrow().statics.borrow().with_depth;
            let depth = cx.borrow().get_scopes().len() as i32 - 1;
            cx.borrow().statics.borrow_mut().with_depth = depth;

            self.eval(cx, &statement);

            self.in_with = saved_in_with;

            cx.borrow().statics.borrow_mut().with_depth = save_with_depth;

            cx.borrow().pop_scope();
        }

        no_proto(cx)
    }

    fn evaluate_continue_statement(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        no_proto(cx)
    }

    fn evaluate_break_statement(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        no_proto(cx)
    }

    fn evaluate_return_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = field!(node, as_return_statement, expr);
        let pos = node.borrow().pos();
        if expr.is_some() {
            let val = self.eval(cx, &expr);
            let mut type_ = val.and_then(|v| v.get_type(&cx.borrow()));
            let (void, no) = (cx.borrow().void_type(), cx.borrow().no_type());
            if ti_is(&self.return_type, &void) {
                if type_.as_ref().map_or(true, |t| !ti_is(t, &void)) {
                    cx.borrow().error(pos, kError_ReturnTypeMustBeUndefined);
                }
                // An expression of type void stays: it may have side effects.
                return tv_proto(&void);
            } else if type_.as_ref().map_or(false, |t| ti_is(t, &void))
                && !ti_is(&self.return_type, &no)
                && node.borrow().is_synthetic()
                && cx.borrow().use_static_semantics()
            {
                // A missing return, rather than the type mismatch the coercion would report.
                cx.borrow().error(pos, kError_MustReturnValue);
            } else {
                let n = coerce(cx, &expr, &mut type_, Some(self.return_type.clone()), false, false);
                set_field!(node, as_return_statement_mut, expr, n);
            }
        } else {
            let c = cx.borrow();
            if c.use_static_semantics() && !ti_is(&self.return_type, &c.void_type()) && !ti_is(&self.return_type, &c.no_type()) {
                c.error(pos, kError_MustReturnValue);
            }
        }
        proto(&self.return_type)
    }

    // Definitions

    fn evaluate_variable_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (ncx, list) = {
            let b = node.borrow();
            let v = b.as_variable_definition().unwrap();
            (v.cx.clone(), v.list.clone())
        };
        let cx = ncx.as_ref().unwrap_or(cx);
        self.eval(cx, &list)
    }

    fn evaluate_variable_binding(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (ref_, typeref, variable, initializer, kind) = {
            let b = node.borrow();
            let v = b.as_variable_binding().unwrap();
            (v.ref_.clone(), v.typeref.clone(), v.variable.clone(), v.initializer.clone(), v.kind)
        };
        let type_;
        let slot = ref_.expect("NullPointerException").get_slot(&cx.borrow());
        let type_pos = || pos_of(&rb(&variable).as_typed_identifier().expect("ClassCastException").type_);

        if let Some(typeref) = &typeref {
            let type_slot = typeref.get_slot(&cx.borrow());
            match type_slot.as_ref().and_then(|s| s.get_value()) {
                None => {
                    cx.borrow().error1(type_pos(), kError_UnknownType, &typeref.get_diagnostic_type_name());
                    let t = no_ti(cx);
                    slot.as_ref().expect("NullPointerException").set_type(Some(t.clone()));
                    type_ = t;
                }
                Some(v) => {
                    if let Some(tv) = v.as_type_value() {
                        type_ = typeref_type_info(typeref, &tv);
                    } else {
                        // The slot's value is not a type.
                        cx.borrow().error1(type_pos(), kError_UnknownType, &typeref.get_diagnostic_type_name());
                        let t = no_ti(cx);
                        slot.as_ref().expect("NullPointerException").set_type(Some(t.clone()));
                        type_ = t;
                    }
                }
            }
        } else {
            type_ = no_ti(cx);
        }

        // Only consts and class members.
        let applies = initializer.is_some()
            && slot.as_ref().map_or(false, |s| {
                kind == CONST_TOKEN || {
                    let db = s.declared_by().expect("NullPointerException");
                    is_instance_builder(&db) || is_class_builder(&db)
                }
            });
        if applies {
            let slot = slot.unwrap();
            let es4_nullability = cx.borrow().statics.borrow().es4_nullability;
            let scope_is_instance = || is_instance_builder(&cx.borrow().scope().expect("NullPointerException"));
            if es4_nullability && scope_is_instance() {
                // Instance variable initializers cannot access this.
                cx.borrow().scope().unwrap().set_init_only(true);
            }
            let val = self.eval(cx, &initializer);
            if es4_nullability && scope_is_instance() {
                cx.borrow().scope().unwrap().set_init_only(false);
            }
            let val = val.and_then(|v| v.get_value(&cx.borrow()));
            if let Some(ov) = val.and_then(|v| v.as_object_value()).filter(|o| o.has_value()) {
                let checked = self.check_default_value(cx, &type_, &ov);
                if checked.is_none() {
                    let c = cx.borrow();
                    c.error2(pos_of(&initializer), kError_IncompatibleDefaultValue, &ov.type_().expect("NullPointerException").get_name_cx(&c).to_string(), &type_.get_name_cx(&c).to_string());
                }
                slot.set_object_value(checked);
            }
        }

        proto(&type_)
    }

    fn evaluate_binary_function_definition(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        // AbcParser set up all the type info.
        None
    }

    fn evaluate_function_definition(&mut self, _unused_cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        // Errors are reported against the context this node was parsed in.
        let (cx, attrs, fexpr, version, ref_, name) = {
            let b = node.borrow();
            let f = b.as_function_definition().unwrap();
            (f.cx.clone().expect("NullPointerException"), f.def.attrs.clone(), f.fexpr.clone(), f.version, f.ref_.clone(), f.name.clone())
        };
        let cx = &cx;

        if attrs.is_some() && !self.doing_class {
            self.eval(cx, &attrs);
        }
        if let Some(fexpr) = &fexpr {
            if version > -1 {
                cx.borrow().push_version(version);
            }

            let r = ref_.expect("NullPointerException");
            let state = if r.name() == "$construct" { super_statement } else { super_error };
            self.super_context.push(state);
            evaluate(fexpr, cx, self);
            self.super_context.pop();

            // Check getter and setter type compatibility, without version checking since the accessors may come
            // from different versions.
            let old_check_ver = cx.borrow().check_version();
            cx.borrow().statics.borrow_mut().check_version = false;

            let name_kind = rb(&name).as_function_name().expect("ClassCastException").kind;
            let signature = nn(&field!(fexpr, as_function_common, signature));
            let (sig_result, sig_parameter, sig_void_anno, sig_pos) = {
                let b = signature.borrow();
                let s = b.as_function_signature().unwrap();
                (s.result.clone(), s.parameter.clone(), s.void_anno, b.pos())
            };
            if name_kind == GET_TOKEN || name_kind == SET_TOKEN {
                let c = cx.borrow();
                // Inherits type info lazily.
                r.get_type_kind(&c, GET_TOKEN);
                r.get_type_kind(&c, SET_TOKEN);
                let s1 = r.get_slot_kind(&c, GET_TOKEN);
                let s2 = r.get_slot_kind(&c, SET_TOKEN);
                if let (Some(s1), Some(s2)) = (&s1, &s2) {
                    let t1 = s1.get_type();
                    let t2 = first_type(&s2.get_types()).unwrap_or_else(|| c.no_type().get_default_type_info());
                    let t1v = t1.as_ref().expect("NullPointerException");
                    if !compare_type_infos(&t1, &Some(t2.clone())) && (!ti_is(t1v, &c.no_type()) && !ti_is(&t2, &c.no_type())) {
                        let pos = if name_kind == GET_TOKEN {
                            if sig_result.is_some() { pos_of(&sig_result) } else { sig_pos }
                        } else if let Some(p) = &sig_parameter {
                            let first = p.borrow().as_parameter_list().expect("ClassCastException").items[0].clone();
                            let t = rb(&first).parameter().expect("ClassCastException").type_.clone();
                            pos_of(&t)
                        } else {
                            sig_pos
                        };

                        r.get_slot_kind(&c, SET_TOKEN);
                        c.error(pos, kError_AccessorTypesMustMatch);
                    }
                }
            }
            if name_kind == SET_TOKEN {
                let c = cx.borrow();
                let s2 = r.get_slot_kind(&c, SET_TOKEN);
                if let Some(s2) = &s2 {
                    let rt2 = s2.get_type().expect("NullPointerException");
                    if sig_result.is_some() && !ti_is(&rt2, &c.void_type()) {
                        c.error(pos_of(&sig_result), kError_BadSetterReturnType);
                    }
                }
                let param_info = sig_parameter.as_ref().map(|p| {
                    let b = p.borrow();
                    let pl = b.as_parameter_list().expect("ClassCastException");
                    (pl.items.len(), b.pos(), pl.items.first().cloned().flatten())
                });
                match param_info {
                    Some((len, ppos, first)) if len == 1 => {
                        let has_init = rb(&first).parameter().expect("ClassCastException").init.is_some();
                        if has_init {
                            c.error(ppos, kError_SetterCannotHaveOptional);
                        }
                    }
                    Some((_, ppos, _)) => c.error(ppos, kError_SetterMustHaveOneParameter),
                    None => c.error(sig_pos, kError_SetterMustHaveOneParameter),
                }
            } else if name_kind == GET_TOKEN {
                let c = cx.borrow();
                if sig_void_anno {
                    c.error(sig_pos, kError_BadGetterReturnType);
                }
                if let Some(p) = &sig_parameter {
                    let (len, ppos) = {
                        let b = p.borrow();
                        (b.as_parameter_list().expect("ClassCastException").items.len(), b.pos())
                    };
                    if len != 0 {
                        c.error(ppos, kError_GetterCannotHaveParameters);
                    }
                }
            }

            cx.borrow().statics.borrow_mut().check_version = old_check_ver;

            if version > -1 {
                cx.borrow().pop_version();
            }
        }

        if field!(node, as_function_definition, needs_init) {
            let init = field!(node, as_function_definition, init);
            self.eval(cx, &init);
        }

        no_proto(cx)
    }

    fn evaluate_function_common(&mut self, _unused_cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (cx, ref_, kind, fun, def, signature, pos) = {
            let b = node.borrow();
            let f = b.as_function_common().unwrap();
            (f.cx.clone().expect("NullPointerException"), f.ref_.clone(), f.kind, f.fun.clone(), f.def.clone(), f.signature.clone(), b.pos())
        };
        let cx = &cx;
        let version = version_of_def(&def);
        let fun_value = || fun.clone().map(Value::Object);

        if self.doing_method {
            if let Some(r) = &ref_ {
                let slot = r.get_slot_kind(&cx.borrow(), kind);
                if let Some(slot) = &slot {
                    if let Some(overridden) = slot.get_overridden_slot() {
                        if !self.match_signatures(slot, &overridden) {
                            cx.borrow().error(pos, kError_IncompatibleOverride);
                        }
                    }
                }

                let fun = fun.clone().expect("NullPointerException");
                cx.borrow().push_scope(fun.activation().expect("NullPointerException"));

                if def.is_some() && version > -1 {
                    cx.borrow().push_version(version);
                }

                self.eval(cx, &signature);

                if def.is_some() && version > -1 {
                    cx.borrow().pop_version();
                }
                cx.borrow().pop_scope();
            }

            return fun_value();
        }

        let (with_depth, scope_chain, body, is_native, is_function_definition) = {
            let b = node.borrow();
            let f = b.as_function_common().unwrap();
            (f.with_depth, f.scope_chain.clone(), f.body.clone(), f.is_native(), f.is_function_definition())
        };

        let saved_with_depth = cx.borrow().statics.borrow().with_depth;
        if with_depth != -1 {
            cx.borrow().statics.borrow_mut().with_depth = with_depth;
        }

        let mut saved_scopes = None;
        if let Some(sc) = scope_chain {
            saved_scopes = Some(cx.borrow().swap_scope_chain(sc));
        }

        if def.is_some() && version > -1 {
            cx.borrow().push_version(version);
        }

        let fun_ov = fun.clone().expect("NullPointerException");
        cx.borrow().push_scope(fun_ov.activation().expect("NullPointerException"));

        let r = ref_.clone().expect("NullPointerException");
        let is_constructor = r.name() == "$construct";
        if is_constructor || is_native {
            self.return_type = cx.borrow().void_type().get_default_type_info();
        } else {
            let t = field!(signature.as_ref().expect("NullPointerException"), as_function_signature, type_);
            self.return_type = t.unwrap_or_else(|| no_ti(cx));
        }

        if is_constructor && cx.borrow().use_static_semantics() {
            // Without an explicit super() call, the base class needs a constructor that takes no arguments.
            let c = cx.borrow();
            let scope_depth = c.get_scope_depth() as i32;
            let iframe = c.scope_at(scope_depth - 2);
            let ib = iframe.builder().filter(|b| b.is_instance_builder());
            if let Some(ib) = ib {
                let (calls_super_ctor, basebui) = {
                    let d = ib.instance_data();
                    (d.calls_super_ctor, d.basebui.clone())
                };
                if !calls_super_ctor {
                    if let Some(basebui) = basebui {
                        let baseobj = basebui.object_value();

                        let r = ReferenceValue::new(&c, baseobj, "$construct", Some(&c.public_namespace()));
                        let slot = r.get_slot_kind(&c, EMPTY_TOKEN);
                        r.get_type_kind(&c, EMPTY_TOKEN);

                        if let Some(slot) = &slot {
                            let ds = slot.get_decl_styles();
                            if size_decls(&ds) != 0 && ds.unwrap().borrow()[0] as i32 == PARAM_Required {
                                // The base constructor has required parameters.
                                let classname = basebui.classname().expect("NullPointerException").to_string();
                                c.error1(pos, kError_NoDefaultBaseclassCtor, &classname);
                            }
                        }
                    }
                }
            }
        }
        let old_in_anonymous_function = self.in_anonymous_function;
        self.in_anonymous_function = !is_function_definition;
        if self.in_anonymous_function {
            self.this_contexts.push(global_this);
        }

        let inits = field!(signature.as_ref().unwrap(), as_function_signature, inits);
        if inits.is_some() {
            let iframe = {
                let c = cx.borrow();
                let scope_depth = c.get_scope_depth() as i32;
                c.scope_at(scope_depth - 2)
            };

            // Only set slots are visible to the constructor initializer list.
            iframe.set_init_only(true);

            self.eval(cx, &inits);

            iframe.set_init_only(false);
        }

        if body.is_some() {
            // Interface method bodies are not evaluated: their default "return void 0" would give bogus missing
            // return errors in strict mode. -1 is the activation, -2 can be the iframe and -3 the cframe.
            let scope_depth = cx.borrow().get_scopes().len() as i32 - 3;
            if scope_depth > 0 {
                let scope = cx.borrow().scope_at(scope_depth);
                let is_interface_method = scope.builder().map_or(false, |b| b.is_class_builder() && b.class_data().is_interface);
                if !is_interface_method {
                    self.eval(cx, &body);
                }
            } else {
                self.eval(cx, &body);
            }
        }
        if self.in_anonymous_function {
            self.this_contexts.pop();
        }
        self.in_anonymous_function = old_in_anonymous_function;

        self.doing_method = false;
        self.return_type = no_ti(cx);

        for fexpr in node_list(&field!(node, as_function_common, fexprs)) {
            evaluate(&fexpr, cx, self);
        }

        cx.borrow().pop_scope();
        if def.is_some() && version > -1 {
            cx.borrow().pop_version();
        }

        if let Some(saved) = saved_scopes {
            cx.borrow().swap_scope_chain(saved);
        }
        cx.borrow().statics.borrow_mut().with_depth = saved_with_depth;

        fun_value()
    }

    fn evaluate_function_name(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_function_signature(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let parameter = field!(node, as_function_signature, parameter);
        if parameter.is_some() {
            self.eval(cx, &parameter);
        }
        self.signature_type(cx, node)
    }

    fn evaluate_parameter(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.evaluate_parameter_impl(cx, node)
    }

    fn evaluate_parameter_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        for item in items_of_params(node) {
            let val = self.eval(cx, &item);
            let _obj = val.expect("NullPointerException").get_value(&cx.borrow()).and_then(|v| v.as_object_value());
        }
        None
    }

    fn evaluate_program(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.super_context.push(super_error);

        let imports = node.borrow().program().unwrap().imports.clone();
        if !imports.is_empty() {
            // The flow analyzer consolidated all the imports into the program of the first ImportNode.
            let program = field!(imports[0], as_import, program);
            self.eval(cx, &program);
        }

        // For constant folding.
        self.current_decimal_context = Some(Decimal128Context::new());

        // Set slot.type, types and decl_styles of every definition; Flex has already done it.
        if !self.type_info_preprocessing_complete {
            let (ncx, statements) = {
                let b = node.borrow();
                let p = b.program().unwrap();
                (p.cx.clone(), p.statements.clone())
            };
            ncx.expect("NullPointerException").borrow().process_unresolved_namespaces();
            self.preprocess_definition_type_info_list(cx, &items_of(&statements), false);
        }

        self.doing_method = false;
        self.doing_class = false;

        for n in node_list(&field!(node, program, fexprs)) {
            evaluate(&n, cx, self);
        }

        for n in node_list(&field!(node, program, clsdefs)) {
            evaluate(&n, cx, self);
        }

        self.this_contexts.push(global_this);

        self.start_program("");

        // Function expressions are compiled as though they came at the end of their block, with the variable that
        // refers to them initialized at its start.
        let statements = field!(node, program, statements);
        if statements.is_some() {
            self.eval(cx, &statements);
        }
        self.doing_method = false;

        self.finish_program(cx, "", 0);

        self.super_context.pop();
        self.this_contexts.pop();

        None
    }

    fn evaluate_package_definition(&mut self, _unused_cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if !field!(node, as_package_definition, in_this_pkg) {
            set_field!(node, as_package_definition_mut, in_this_pkg, true);
        }
        None
    }

    fn evaluate_error(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (code, arg, pos) = {
            let b = node.borrow();
            let e = b.as_error().unwrap();
            (e.error_code, e.error_arg.clone(), b.pos())
        };
        cx.borrow().error1(pos, code, &arg);
        no_proto(cx)
    }

    fn evaluate_to_object(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = field!(node, as_to_object, expr);
        let v = self.eval(cx, &expr).expect("NullPointerException");
        let mut actual = v.get_type(&cx.borrow());
        let n = coerce(cx, &expr, &mut actual, Some(no_ti(cx)), false, false);
        set_field!(node, as_to_object_mut, expr, n);
        proto(actual.as_ref().expect("NullPointerException"))
    }

    fn evaluate_load_register(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let t = field!(node, as_load_register, type_);
        tv_proto(t.as_ref().expect("NullPointerException"))
    }

    fn evaluate_store_register(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, t) = {
            let b = node.borrow();
            let s = b.as_store_register().unwrap();
            (s.expr.clone(), s.type_.clone())
        };
        let val = self.eval(cx, &expr);
        let mut type_ = val.and_then(|v| v.get_type(&cx.borrow()));
        let n = coerce(cx, &expr, &mut type_, t.as_ref().map(|t| t.get_default_type_info()), false, false);
        set_field!(node, as_store_register_mut, expr, n);
        tv_proto(t.as_ref().expect("NullPointerException"))
    }

    fn evaluate_register(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        tv_proto(&cx.borrow().void_type())
    }

    fn evaluate_has_next(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        tv_proto(&cx.borrow().boolean_type())
    }

    fn evaluate_throw_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = field!(node, as_throw_statement, expr);
        if expr.is_some() {
            self.eval(cx, &expr);
        }
        no_proto(cx)
    }

    fn evaluate_try_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (tryblock, catchlist, finallyblock) = {
            let b = node.borrow();
            let t = b.as_try_statement().unwrap();
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
        no_proto(cx)
    }

    fn evaluate_catch_clause(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (activation, parameter, statements) = {
            let b = node.borrow();
            let c = b.as_catch_clause().unwrap();
            (c.activation.clone(), c.parameter.clone(), c.statements.clone())
        };
        cx.borrow().push_scope(activation.expect("NullPointerException"));

        if parameter.is_some() {
            self.eval(cx, &parameter);
        }
        if statements.is_some() {
            self.eval(cx, &statements);
        }

        cx.borrow().pop_scope();

        no_proto(cx)
    }

    fn evaluate_finally_clause(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (default_catch, statements) = {
            let b = node.borrow();
            let f = b.as_finally_clause().unwrap();
            (f.default_catch.clone(), f.statements.clone())
        };
        self.eval(cx, &default_catch);
        if statements.is_some() {
            self.eval(cx, &statements);
        }
        no_proto(cx)
    }

    fn evaluate_box(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = field!(node, as_box, expr);
        self.eval(cx, &expr);
        no_proto(cx)
    }

    fn evaluate_coerce(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, expected) = {
            let b = node.borrow();
            let c = b.as_coerce().unwrap();
            (c.expr.clone(), c.expected.clone())
        };
        self.eval(cx, &expr);
        proto(expected.as_ref().expect("NullPointerException"))
    }

    fn evaluate_class_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.evaluate_class_definition_impl(cx, node)
    }

    fn evaluate_binary_interface_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.evaluate_binary_class_def(cx, node)
    }

    fn evaluate_interface_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.evaluate_class_definition_impl(cx, node)
    }

    fn evaluate_class_name(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (pkgname, ident) = {
            let b = node.borrow();
            let c = b.as_class_name().unwrap();
            (c.pkgname.clone(), c.ident.clone())
        };
        if pkgname.is_some() {
            self.eval(cx, &pkgname);
        }
        if ident.is_some() {
            self.eval(cx, &ident);
        }
        Some(Value::Object(ObjectValue::undefined_value()))
    }

    fn evaluate_inheritance(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (baseclass, interfaces) = {
            let b = node.borrow();
            let i = b.as_inheritance().unwrap();
            (i.baseclass.clone(), i.interfaces.clone())
        };
        if baseclass.is_some() {
            self.eval(cx, &baseclass);
        }
        if interfaces.is_some() {
            self.eval(cx, &interfaces);
        }
        Some(Value::Object(ObjectValue::undefined_value()))
    }

    fn evaluate_attribute_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let items = field!(node, as_attribute_list, items);
        for n in items.iter().flatten() {
            evaluate(n, cx, self);
        }
        Some(Value::Object(ObjectValue::undefined_value()))
    }

    fn evaluate_include_directive(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (in_this_include, ncx) = {
            let b = node.borrow();
            let i = b.as_include_directive().unwrap();
            (i.in_this_include, i.cx.clone())
        };
        if !in_this_include {
            let prev_cx = Rc::new(RefCell::new(Context::new(cx.borrow().statics.clone())));
            prev_cx.borrow_mut().switch_to_context(&cx.borrow());
            {
                let mut b = node.borrow_mut();
                let i = b.as_include_directive_mut().unwrap();
                i.in_this_include = true;
                i.prev_cx = Some(prev_cx);
            }

            // This switches the context of the enclosing statement list.
            let ncx = ncx.expect("NullPointerException");
            if !Rc::ptr_eq(cx, &ncx) {
                cx.borrow_mut().switch_to_context(&ncx.borrow());
            }
        } else {
            let prev_cx = {
                let mut b = node.borrow_mut();
                let i = b.as_include_directive_mut().unwrap();
                i.in_this_include = false;
                i.prev_cx.take().expect("NullPointerException")
            };
            // Restore the prevailing context.
            cx.borrow_mut().switch_to_context(&prev_cx.borrow());
        }

        None
    }

    fn evaluate_import_directive(&mut self, _unused_cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (cx, name) = {
            let b = node.borrow();
            let i = b.as_import_directive().unwrap();
            (i.cx.clone().expect("NullPointerException"), i.name.clone())
        };
        let c = cx.borrow();

        if c.use_static_semantics() {
            let (package_name, definition_name, name_pos) = {
                let n = rb(&name);
                let pn = n.as_package_name().expect("ClassCastException");
                let id = rb(&pn.id);
                let pi = id.as_package_identifiers().expect("ClassCastException");
                (pi.pkg_part.clone(), pi.def_part.clone(), n.pos())
            };

            if let Some(definition_name) = definition_name.filter(|d| !d.is_empty()) {
                let mut b = String::new();
                if let Some(p) = package_name.as_ref().filter(|p| !p.is_empty()) {
                    b.push_str(p);
                    b.push(':');
                }
                b.push_str(&definition_name);

                let import_name = b;
                if !c.is_valid_import(&import_name) {
                    let namespaces = Namespaces::new();
                    namespaces.add(c.get_namespace(package_name.as_deref().expect("NullPointerException")));
                    let r = ReferenceValue::with_namespaces(&c, None, &definition_name, &namespaces);
                    let slot = r.get_slot_kind(&c, GET_TOKEN);
                    if slot.is_none() {
                        c.error1(name_pos, kError_DefinitionNotFound, &import_name);
                    } else {
                        c.add_valid_import(&import_name);
                    }
                }
            } else {
                // Does the namespace exist?
                let package_name = package_name.expect("NullPointerException");
                let ns = c.get_namespace(&package_name);
                if !ns.is_package() {
                    c.error1(name_pos, kError_DefinitionNotFound, &package_name);
                }
            }
        }

        Some(Value::Object(ObjectValue::undefined_value()))
    }

    fn evaluate_super_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let mut super_value = Some(ObjectValue::object_prototype());
        let this_value;

        // The flow analyzer handled all the error cases.
        let expr = field!(node, as_super_expression, expr);
        if expr.is_some() {
            let val = self.eval(cx, &expr);
            this_value = val.and_then(|v| v.as_type_value());
        } else {
            let c = cx.borrow();
            let scope_depth = c.get_scopes().len() as i32 - 1;
            let val = c.scope_at(scope_depth - 2);
            this_value = val.as_type_value();
        }

        if let Some(b) = this_value.and_then(|t| t.baseclass()) {
            super_value = b.prototype();
        }

        obj(super_value)
    }

    fn evaluate_super_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let mut slot = None;
        let (baseobj, call, pos) = {
            let b = node.borrow();
            let s = b.as_super_statement().unwrap();
            (s.baseobj.clone(), s.call.clone(), b.pos())
        };

        if let Some(baseobj) = &baseobj {
            let c = cx.borrow();
            let r = ReferenceValue::new(&c, Some(baseobj.clone()), "$construct", Some(&c.public_namespace()));
            slot = r.get_slot_kind(&c, EMPTY_TOKEN);
            r.get_type_kind(&c, EMPTY_TOKEN);

            // Without an explicit base class constructor decl_styles was never set; set it now to catch a wrong
            // number of arguments.
            if let Some(s) = &slot {
                if size_types(&s.get_types()) == 0 && size_decls(&s.get_decl_styles()) == 0 {
                    s.add_decl_style(PARAM_Void);
                    s.add_type(c.void_type().get_default_type_info());
                }
            }
        }

        let args = field!(nn(&call), as_call_expression, args);
        if let Some(args) = &args {
            if let Some(s) = &slot {
                let mut b = args.borrow_mut();
                let a = b.as_argument_list_mut().expect("ClassCastException");
                a.expected_types = s.get_types().map(|t| t.borrow().iter().cloned().map(Some).collect());
                a.decl_styles = s.get_decl_styles().map(|d| d.borrow().clone());
            }

            evaluate(args, cx, self);
        } else if let Some(s) = &slot {
            let ds = s.get_decl_styles();
            if cx.borrow().use_static_semantics() && size_decls(&ds) != 0 && ds.as_ref().unwrap().borrow()[0] as i32 == PARAM_Required {
                let ds = ds.unwrap().borrow().clone();
                let mut expected_num_args = ds.len();
                while expected_num_args > 0 {
                    if ds[expected_num_args - 1] as i32 == PARAM_Required {
                        break;
                    }
                    expected_num_args -= 1;
                }
                cx.borrow().error1(pos, kError_WrongNumberOfArguments, &expected_num_args.to_string());
            }
        }
        Some(Value::Object(ObjectValue::undefined_value()))
    }

    fn evaluate_rest_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        cx.borrow().internal_error_at(node.borrow().pos(), "RestExpressionNode not yet implemented");
        None
    }

    fn evaluate_rest_parameter(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        // Errors were checked by PreprocessDefinitionTypeInfo(RestParameterNode).
        self.evaluate_parameter_impl(cx, node)
    }

    fn evaluate_config_namespace_definition(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_namespace_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (kill, gen, r) = {
            let b = node.borrow();
            (b.get_kill_bits(), b.get_gen_bits(), b.namespace_definition().unwrap().ref_.clone())
        };
        self.rch_bits = BitSet::reset_set(self.rch_bits.take(), kill, gen);

        if let Some(r) = &r {
            r.calc_use_definitions(&cx.borrow(), self.rch_bits.clone());
        }
        None
    }

    fn evaluate_pragma(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let list = field!(node, as_pragma, list);
        if list.is_some() {
            self.eval(cx, &list);
        }
        None
    }

    fn evaluate_use_precision(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        // FlowAnalyzer already set the parameters of the binary and unary expressions.
        None
    }

    fn evaluate_use_numeric(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        // FlowAnalyzer already set the parameters of the statement lists.
        None
    }

    fn evaluate_use_rounding(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_pragma_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        cx.borrow().internal_error_at(node.borrow().pos(), "PragmaExpressionNode not yet implemented");
        None
    }

    fn evaluate_typed_identifier(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        cx.borrow().internal_error_at(node.borrow().pos(), "TypedIdentifierNode not yet implemented");
        None
    }

    fn evaluate_untyped_variable_binding(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        cx.borrow().internal_error_at(node.borrow().pos(), "UntypedVariableBindingNode not yet implemented");
        None
    }

    fn evaluate_use_directive(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_literal_xml(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (list, is_xmllist) = {
            let b = node.borrow();
            let x = b.as_literal_xml().unwrap();
            (x.list.clone(), x.is_xmllist)
        };
        if list.is_some() {
            self.eval(cx, &list);
        }
        let c = cx.borrow();
        if is_xmllist {
            tv_proto(&c.xml_list_type())
        } else {
            tv_proto(&c.xml_type())
        }
    }

    fn evaluate_package_name(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        cx.borrow().internal_error_at(node.borrow().pos(), "PackageNameNode not yet implemented");
        None
    }

    fn evaluate_package_identifiers(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        cx.borrow().internal_error_at(node.borrow().pos(), "PackageIdentifiersNode not yet implemented");
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

    fn evaluate_binary_program(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (frame, statements) = {
            let b = node.borrow();
            let p = b.program().unwrap();
            (p.frame.clone(), p.statements.clone())
        };
        cx.borrow().push_scope(frame.expect("NullPointerException"));
        self.eval(cx, &statements);
        cx.borrow().pop_scope();

        None
    }

    fn evaluate_binary_class_def(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        // AbcParser set up all the type info.
        None
    }

    fn evaluate_empty_element(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_default_xml_namespace(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = field!(node, as_default_xml_namespace, expr);
        if expr.is_some() {
            let val = self.eval(cx, &expr);
            set_field!(node, as_default_xml_namespace_mut, ref_, val.and_then(|v| v.as_reference_value()));
        }
        None
    }

    fn evaluate_type_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = field!(node, as_type_expression, expr);
        self.eval(cx, &expr)
    }
}
