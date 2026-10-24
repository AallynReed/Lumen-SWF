//! Port of `macromedia/abc/AbcParser.java`: imports an ABC file's definitions into the semantic model (builders, slots
//! and types) as a `BinaryProgramNode`, the way mxmlc loads `playerglobal.swc`.

use std::cell::{Ref, RefCell};
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;

use crate::asc::abc::abc_data::{AbcData, BinaryMN, Method, Trait};
use crate::asc::abc::bytecode_buffer::BytecodeBuffer;
use crate::asc::emit::class_builder::new_class_builder;
use crate::asc::emit::constants::*;
use crate::asc::emit::function_builder::new_function_builder;
use crate::asc::emit::global_builder::new_global_builder;
use crate::asc::emit::instance_builder::new_instance_builder;
use crate::asc::emit::runtime_constants::TYPE_object;
use crate::asc::parser::factory::NodeFactory;
use crate::asc::parser::meta_data_evaluator::{GO_TO_CTOR_DEFINITION_HELP, GO_TO_DEFINITION_HELP};
use crate::asc::parser::node::{new_node_list, NodeList, NodeRef};
use crate::asc::parser::tokens::*;
use crate::asc::semantics::builder::Builder;
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::parameterized_name::new_parameterized_name;
use crate::asc::semantics::q_name::QName;
use crate::asc::semantics::reference_value::ReferenceValue;
use crate::asc::semantics::slot::*;
use crate::asc::semantics::type_value::TypeValue;
use crate::asc::semantics::value::Value;
use crate::asc::util::context::{Context, ContextRef, NS_EXPLICIT, NS_INTERNAL, NS_PRIVATE, NS_PROTECTED, NS_PUBLIC, NS_STATIC_PROTECTED};
use crate::asc::util::decimal128::Decimal128;
use crate::asc::util::namespaces::Namespaces;
use crate::xfl::num::java_double;

thread_local! {
    /// `AbcParser.dummyFunc`: the one function object that stands for every imported method.
    static DUMMY_FUNC: RefCell<Option<ObjectValue>> = const { RefCell::new(None) };
}

struct DefAndSlot {
    def: Option<NodeRef>,
    slot: Option<Slot>,
}

pub struct AbcParser {
    ctx: ContextRef,
    abc_data: Rc<AbcData>,
    fun_names: HashMap<String, i32>,
    clsdefs_sets: Vec<NodeList>,
    region_name_stack: Vec<String>,
    /// `ce_unresolved_sets`: the program nodes whose `ce_unresolved` sets collect references to unresolved types.
    ce_unresolved_sets: Vec<NodeRef>,
}

fn rbm(n: &NodeRef) -> std::cell::RefMut<'_, crate::asc::parser::node::Node> {
    n.borrow_mut()
}

impl AbcParser {
    pub fn from_file(cx: &ContextRef, name: &str) -> std::io::Result<AbcParser> {
        let buf = BytecodeBuffer::from_file(name)?;
        let mut abc_data = AbcData::new("");
        abc_data.read_abc(&buf);
        Ok(Self::with_data(cx, abc_data))
    }

    pub fn new(cx: &ContextRef, bytes: Vec<u8>) -> AbcParser {
        let mut abc_data = AbcData::new("");
        abc_data.read_abc(&BytecodeBuffer::from_bytes(bytes));
        Self::with_data(cx, abc_data)
    }

    fn with_data(cx: &ContextRef, abc_data: AbcData) -> AbcParser {
        AbcParser {
            ctx: cx.clone(),
            abc_data: Rc::new(abc_data),
            fun_names: HashMap::new(),
            clsdefs_sets: Vec::new(),
            region_name_stack: Vec::new(),
            ce_unresolved_sets: Vec::new(),
        }
    }

    fn cx(&self) -> Ref<'_, Context> {
        self.ctx.borrow()
    }

    fn nf(&self) -> Rc<RefCell<NodeFactory>> {
        Context::get_node_factory(&self.ctx)
    }

    pub fn parse_abc(&mut self) -> Option<NodeRef> {
        self.parse_abc_with(false, false)
    }

    /// `collect_top_level` fills the program's `toplevel_definitions`; `build_ast_for_classes` builds full class
    /// bodies. mxmlc sets neither.
    pub fn parse_abc_with(&mut self, collect_top_level: bool, build_ast_for_classes: bool) -> Option<NodeRef> {
        let result = catch_unwind(AssertUnwindSafe(|| self.parse_abc_inner(collect_top_level, build_ast_for_classes)));
        match result {
            Ok(prog) => Some(prog),
            Err(_) => {
                // Pop whatever scopes the failed import pushed.
                let cx = self.cx();
                let global = cx.global_scope();
                let mut scope = cx.scope();
                while !match (&scope, &global) {
                    (Some(a), Some(b)) => a.ptr_eq(b),
                    (None, None) => true,
                    _ => false,
                } {
                    cx.pop_scope();
                    scope = cx.scope();
                }
                None
            }
        }
    }

    fn parse_abc_inner(&mut self, collect_top_level: bool, build_ast_for_classes: bool) -> NodeRef {
        // The program node holds the builders FlowAnalyzer would otherwise make, not a parse tree.
        let nf = self.nf();
        let empty = nf.borrow_mut().statement_list_concat(None, None);
        let program = nf.borrow_mut().binary_program(&self.ctx, Some(empty));
        let b = new_global_builder();
        b.set_is_in_package(true);
        let frame = {
            let cx = self.cx();
            let frame = ObjectValue::with_builder(&cx, b, Some(&cx.no_type()));
            cx.push_scope(frame.clone());
            frame
        };
        rbm(&program).program_mut().unwrap().frame = Some(frame);
        self.clsdefs_sets.push(new_node_list());
        self.ce_unresolved_sets.push(program.clone());
        self.region_name_stack.push(String::new());

        for i in 0..self.abc_data.get_script_info_size() {
            self.parse_script(i, &program, collect_top_level, build_ast_for_classes);
        }

        let clsdefs = self.clsdefs_sets.pop().unwrap();
        rbm(&program).program_mut().unwrap().clsdefs = Some(clsdefs);
        self.ce_unresolved_sets.pop();
        self.region_name_stack.pop();
        self.cx().pop_scope();

        // Top-level definitions are evaluated in the program's frame; class definitions stay in clsdefs, outside it.
        let list = nf.borrow_mut().statement_list(None, Some(program));
        let prog = nf.borrow_mut().program(&self.ctx, Some(list));
        prog
    }

    /// The `TypeValue` for a multiname, a placeholder if the type has not been seen yet.
    fn get_type_from_qname(&mut self, type_id: i32) -> TypeValue {
        if type_id == 0 {
            return self.cx().no_type();
        }
        let typename = self.get_full_name(&self.get_binary_mn_from_cpool(type_id));
        let cx = self.cx();
        if cx.is_builtin(&typename.to_string()) {
            return cx.builtin(&typename.to_string()).unwrap();
        }
        let t = TypeValue::get_type_value(&cx, &typename);
        if !t.resolved() {
            let r = ReferenceValue::new(&cx, None, typename.name(), typename.ns().as_ref());
            let last = self.ce_unresolved_sets.last().expect("NoSuchElementException");
            rbm(last).program_mut().unwrap().ce_unresolved.insert(r);
        }
        TypeValue::get_type_value(&cx, &typename)
    }

    #[allow(clippy::too_many_arguments)]
    fn slot_trait(&mut self, name: &str, ns: &ObjectValue, _slot_id: i32, type_id: i32, value_id: i32, value_kind: i32, is_const: bool, build_ast: bool) -> DefAndSlot {
        let obj = self.cx().scope().expect("NullPointerException");
        let nss = Namespaces::new();
        nss.push_back(ns.clone());
        let type_ = self.get_type_from_qname(type_id);
        let bui = obj.builder().expect("NullPointerException");
        let slot = {
            let cx = self.cx();
            let var_id = bui.variable(&cx, &obj);
            let slot_id = bui.explicit_var_nss(&cx, &obj, name, &nss, Some(&type_), -1, -1, var_id);
            obj.get_slot(&cx, slot_id).expect("NullPointerException")
        };
        slot.set_const(is_const);
        slot.set_imported(true);
        let mut ret = DefAndSlot { def: None, slot: Some(slot.clone()) };

        let mut id = None;
        let mut attr = None;
        if build_ast {
            id = Some(self.identifier_node(name, ns));
            attr = Some(self.attribute_list(false, false, false, Some(ns), &bui));
        }
        if value_kind == CONSTANT_Namespace {
            let ns_value = self.get_namespace(value_id);
            slot.set_object_value(Some(ns_value.clone()));
            slot.set_const(true);
            if build_ast {
                let nf = self.nf();
                let lit = nf.borrow_mut().literal_string(&ns_value.name());
                ret.def = Some(nf.borrow_mut().namespace_definition(attr, id, Some(lit)));
            }
            return ret;
        }
        if value_id != 0 {
            slot.set_object_value(self.get_init_value(value_id, value_kind));
        }
        if build_ast {
            let mut type_expr = None;
            if type_id != 0 {
                let t = self.get_binary_mn_from_cpool(type_id);
                type_expr = Some(self.member_expr_from_mn(&t));
            }
            let init = self.get_init_value_node(value_id, value_kind);
            let nf = self.nf();
            let ty = nf.borrow_mut().typed_identifier(id.clone(), type_expr.clone());
            let tok = if is_const { CONST_TOKEN } else { VAR_TOKEN };
            let bind = nf.borrow_mut().variable_binding(attr.clone(), tok, Some(ty), Some(init));
            {
                let id_ref = id.as_ref().unwrap().borrow().identifier().unwrap().ref_.clone();
                let mut b = rbm(&bind);
                let vb = b.as_variable_binding_mut().expect("ClassCastException");
                vb.ref_ = id_ref;
                if let Some(te) = &type_expr {
                    vb.typeref = te.borrow().as_member_expression().unwrap().ref_.clone();
                }
            }
            let list = nf.borrow_mut().list(None, Some(bind));
            ret.def = Some(nf.borrow_mut().variable_definition(attr, tok, Some(list)));
        }
        ret
    }

    fn member_expr_from_mn(&mut self, mn: &BinaryMN) -> NodeRef {
        let type_name = self.get_full_name(mn);
        let nf = self.nf();
        if type_name.is_parameterized_name() {
            let base_id_node = self.identifier_node(type_name.name(), &type_name.ns().expect("NullPointerException"));
            let p0 = self.get_binary_mn_from_cpool(mn.params[0]);
            let param_node = self.member_expr_from_mn(&p0);
            let list = nf.borrow_mut().list(None, Some(param_node.clone()));
            let apply = nf.borrow_mut().apply_type_expr(Some(base_id_node.clone()), Some(list), -1);
            let type_expr = nf.borrow_mut().member_expression(None, Some(apply));
            let base_ref = base_id_node.borrow().identifier().unwrap().ref_.clone();
            rbm(&type_expr).as_member_expression_mut().unwrap().ref_ = base_ref.clone();
            let param_ref = param_node.borrow().as_member_expression().unwrap().ref_.clone();
            base_ref.expect("NullPointerException").add_type_param(param_ref.expect("NullPointerException"));
            type_expr
        } else {
            let type_id_node = self.identifier_node(type_name.name(), &type_name.ns().expect("NullPointerException"));
            let get_node = nf.borrow_mut().get_expression(Some(type_id_node.clone()));
            let type_expr = nf.borrow_mut().member_expression(None, Some(get_node));
            let r = type_id_node.borrow().identifier().unwrap().ref_.clone();
            rbm(&type_expr).as_member_expression_mut().unwrap().ref_ = r;
            type_expr
        }
    }

    /// An identifier node with its reference value filled in.
    fn identifier_node(&self, simple_name: &str, ns: &ObjectValue) -> NodeRef {
        let id = self.nf().borrow_mut().identifier(simple_name);
        let name = id.borrow().identifier().unwrap().name.clone();
        let r = ReferenceValue::new(&self.cx(), None, &name, Some(ns));
        rbm(&id).identifier_mut().unwrap().ref_ = Some(r);
        id
    }

    #[allow(clippy::too_many_arguments)]
    fn method_trait(&mut self, method_name: &str, ns: &ObjectValue, _disp_id: i32, meth_info: i32, attrs: i32, kind: i32, build_ast: bool) -> DefAndSlot {
        let obj = self.cx().scope().expect("NullPointerException");
        let is_final = (attrs & TRAIT_FLAG_final) != 0;
        let is_override = (attrs & TRAIT_FLAG_override) != 0;
        let names = Namespaces::of(ns.clone());
        let bui = obj.builder().expect("NullPointerException");
        let method_slot = {
            let cx = self.cx();
            let method_id = bui.method(&cx, &obj, method_name, &names, false);
            let method_slot = match kind {
                TRAIT_Setter => bui.explicit_set_nss(&cx, &obj, method_name, &names, Some(&cx.no_type()), is_final, is_override, -1, method_id, -1),
                TRAIT_Getter => bui.explicit_get_nss(&cx, &obj, method_name, &names, Some(&cx.no_type()), is_final, is_override, -1, method_id, -1),
                _ => bui.explicit_call_nss(&cx, &obj, method_name, &names, Some(&cx.no_type()), is_final, is_override, -1, method_id, -1),
            };
            let func_obj = Self::get_function_object(&cx);
            let slot = obj.get_slot(&cx, method_slot).expect("NullPointerException");
            slot.set_value(Some(Value::Object(func_obj)));
            slot.set_imported(true);
            method_slot
        };

        // Getters and setters can share a name, so each method gets its own numbered internal name.
        let num = *self.fun_names.entry(method_name.to_string()).or_insert(0);
        let internal_name = format!("{}${}", method_name, num);
        self.fun_names.insert(method_name.to_string(), num + 1);

        let implied_slot = {
            let cx = self.cx();
            let slot_id = obj.get_implicit_index(&cx, method_slot, EMPTY_TOKEN);
            obj.get_slot(&cx, slot_id).expect("NullPointerException")
        };
        implied_slot.set_imported(true);
        implied_slot.set_method_name(&internal_name);
        let mut ret = DefAndSlot { def: None, slot: Some(implied_slot.clone()) };

        let function_kind = match kind {
            TRAIT_Getter => GET_TOKEN,
            TRAIT_Setter => SET_TOKEN,
            _ => EMPTY_TOKEN,
        };

        let data = self.abc_data.clone();
        let m_info: &Method = data.get_method(meth_info);
        let return_type_id = m_info.get_return_type();
        let param_type_ids = m_info.get_param_types().clone();
        let param_count = param_type_ids.len();
        let has_optional = m_info.get_has_optional();
        let optional_count = if has_optional { m_info.get_optional_param_types().len() } else { 0 };
        let needs_rest = m_info.get_needs_rest();
        let param_names = m_info.get_param_names();
        let mut optional_nodes = None;
        if has_optional && build_ast {
            optional_nodes = Some(self.parse_optional_params(meth_info));
        }

        let ret_type = self.get_type_from_qname(return_type_id);
        implied_slot.set_type(Some(ret_type.get_default_type_info()));

        let mut param_list: Option<NodeRef> = None;
        let mut param_types = Vec::with_capacity(param_count);
        let mut decl_styles: Vec<u8> = Vec::with_capacity(1);
        let mut cur_optional = 0;
        for (i, &ptid) in param_type_ids.iter().enumerate() {
            let mut param = None;
            if build_ast {
                let type_mn = if ptid != 0 { Some(self.get_binary_mn_from_cpool(ptid)) } else { None };
                let simple_param_name = match param_names.get(i) {
                    Some(n) => n.clone(),
                    None => format!("param{}", i + 1),
                };
                let p = self.parameter_node_mn(&simple_param_name, type_mn.as_deref());
                param_list = Some(self.nf().borrow_mut().parameter_list(param_list, Some(p.clone())));
                param = Some(p);
            }
            param_types.push(self.get_type_from_qname(ptid).get_default_type_info());
            if i >= param_count - optional_count {
                if build_ast {
                    let init = optional_nodes.as_ref().unwrap()[cur_optional].clone();
                    cur_optional += 1;
                    rbm(param.as_ref().unwrap()).parameter_mut().unwrap().init = Some(init);
                }
                decl_styles.push(PARAM_Optional as u8);
            } else {
                decl_styles.push(PARAM_Required as u8);
            }
        }

        if needs_rest {
            if build_ast {
                let array_name = self.cx().array_type().name();
                let param = self.parameter_node_qname("rest", Some(&array_name));
                let nf = self.nf();
                let rest_node = nf.borrow_mut().rest_parameter_pos(Some(param.clone()), -1).expect("NullPointerException");
                {
                    let (r, t) = {
                        let p = param.borrow();
                        let p = p.parameter().unwrap();
                        (p.ref_.clone(), p.typeref.clone())
                    };
                    let mut rn = rbm(&rest_node);
                    let rp = rn.parameter_mut().unwrap();
                    rp.ref_ = r;
                    rp.typeref = t;
                }
                param_list = Some(nf.borrow_mut().parameter_list(param_list, Some(rest_node)));
                // MovieClipMaker makes function nodes itself, so has_rest must not stay set for later functions.
                nf.borrow_mut().has_rest = false;
            }
            param_types.push(self.cx().array_type().get_default_type_info());
            decl_styles.push(PARAM_Rest as u8);
        }

        if !param_types.is_empty() {
            implied_slot.set_types(Some(Rc::new(RefCell::new(param_types))));
            implied_slot.set_decl_styles(Some(Rc::new(RefCell::new(decl_styles))));
        } else {
            param_types.push(self.cx().void_type().get_default_type_info());
            implied_slot.set_types(Some(Rc::new(RefCell::new(param_types))));
            implied_slot.add_decl_style(PARAM_Void);
        }

        if build_ast {
            let mut ret_type_node = None;
            if return_type_id != 0 {
                let ret_mn = self.get_binary_mn_from_cpool(return_type_id);
                ret_type_node = Some(self.member_expr_from_mn(&ret_mn));
            }
            let nf = self.nf();
            let fsn = nf.borrow_mut().function_signature(param_list, ret_type_node.clone());
            if let Some(rt) = &ret_type_node {
                let r = rt.borrow().as_member_expression().unwrap().ref_.clone();
                rbm(&fsn).as_function_signature_mut().unwrap().typeref = r;
            }
            let id = self.identifier_node(method_name, ns);
            let fcn = nf.borrow_mut().function_common(&self.ctx, Some(id.clone()), Some(fsn), None);
            {
                let id_ref = id.borrow().identifier().unwrap().ref_.clone();
                let mut f = rbm(&fcn);
                let fc = f.as_function_common_mut().unwrap();
                fc.kind = function_kind;
                fc.ref_ = id_ref;
            }
            let fn_ = nf.borrow_mut().function_name(function_kind, Some(id));
            let attr = self.attribute_list(is_final, is_override, false, Some(ns), &bui);
            let fdn = nf.borrow_mut().binary_function_definition(&self.ctx, Some(attr), Some(fn_), Some(fcn), -1);
            ret.def = Some(fdn);
        }
        ret
    }

    /// One function object stands for every imported method; it is only used as the slot's value.
    fn get_function_object(cx: &Context) -> ObjectValue {
        DUMMY_FUNC.with(|d| {
            let mut d = d.borrow_mut();
            if d.is_none() {
                *d = Some(ObjectValue::with_builder(cx, new_function_builder(), Some(&cx.function_type())));
            }
            d.clone().unwrap()
        })
    }

    fn get_init_value_node(&self, value_id: i32, kind: i32) -> NodeRef {
        let nf = self.nf();
        let mut nf = nf.borrow_mut();
        match kind {
            CONSTANT_Void => {
                let zero = nf.literal_number_pos("0", -1);
                nf.unary_expression_pos(VOID_TOKEN, Some(zero), -1)
            }
            CONSTANT_Integer | CONSTANT_UInteger | CONSTANT_Double => {
                let val = self.get_number_from_cpool(value_id, kind);
                nf.literal_number(&java_double(val))
            }
            CONSTANT_Decimal => {
                let mut sval = self.get_decimal_from_cpool(value_id).to_string();
                if self.cx().statics.borrow().es4_numerics {
                    sval.push('m');
                }
                nf.literal_number(&sval)
            }
            CONSTANT_True => nf.literal_boolean(true),
            CONSTANT_False => nf.literal_boolean(false),
            CONSTANT_Utf8 => nf.literal_string(&self.get_string_from_cpool(value_id)),
            CONSTANT_Null => nf.literal_null(),
            CONSTANT_Namespace => {
                drop(nf);
                let ns = self.get_namespace(value_id);
                self.nf().borrow_mut().literal_string(&ns.name())
            }
            _ => nf.literal_string(""),
        }
    }

    fn get_init_value(&self, value_id: i32, kind: i32) -> Option<ObjectValue> {
        let cx = self.cx();
        match kind {
            CONSTANT_Void => cx.void_type().prototype(),
            CONSTANT_Integer => {
                let v = self.get_number_from_cpool(value_id, kind);
                Some(ObjectValue::with_value(&java_double(v), Some(&cx.int_type())))
            }
            CONSTANT_UInteger => {
                let v = self.get_number_from_cpool(value_id, kind);
                Some(ObjectValue::with_value(&java_double(v), Some(&cx.uint_type())))
            }
            CONSTANT_Double => {
                let v = self.get_number_from_cpool(value_id, kind);
                Some(ObjectValue::with_value(&java_double(v), Some(&cx.double_type())))
            }
            CONSTANT_Decimal => {
                let mut sval = self.get_decimal_from_cpool(value_id).to_string();
                if cx.statics.borrow().es4_numerics {
                    sval.push('m');
                }
                Some(ObjectValue::with_value(&sval, cx.decimal_type().as_ref()))
            }
            CONSTANT_True => Some(cx.boolean_true()),
            CONSTANT_False => Some(cx.boolean_false()),
            CONSTANT_Utf8 => Some(ObjectValue::with_value(&self.get_string_from_cpool(value_id), Some(&cx.string_type()))),
            CONSTANT_Null => cx.null_type().prototype(),
            CONSTANT_Namespace => {
                drop(cx);
                Some(self.get_namespace(value_id))
            }
            _ => None,
        }
    }

    fn parse_optional_params(&self, meth_info: i32) -> Vec<NodeRef> {
        let m = self.abc_data.get_method(meth_info);
        let values = m.get_optional_param_types().clone();
        let kinds = m.get_optional_param_kinds().clone();
        values.iter().zip(kinds.iter()).map(|(&v, &k)| self.get_init_value_node(v, k)).collect()
    }

    fn parameter_node_qname(&self, simple_param_name: &str, type_name: Option<&QName>) -> NodeRef {
        let public = self.cx().public_namespace();
        let id = self.identifier_node(simple_param_name, &public);
        let mut ty = None;
        let mut param_type = None;
        if let Some(tn) = type_name {
            let t = self.identifier_node(tn.name(), &tn.ns().expect("NullPointerException"));
            let nf = self.nf();
            let get_node = nf.borrow_mut().get_expression(Some(t.clone()));
            param_type = Some(nf.borrow_mut().member_expression(None, Some(get_node)));
            ty = Some(t);
        }
        let param = self.nf().borrow_mut().parameter(VAR_TOKEN, Some(id.clone()), param_type);
        {
            let id_ref = id.borrow().identifier().unwrap().ref_.clone();
            let ty_ref = ty.as_ref().map(|t| t.borrow().identifier().unwrap().ref_.clone()).flatten();
            let mut p = rbm(&param);
            let pn = p.parameter_mut().unwrap();
            if ty.is_some() {
                pn.typeref = ty_ref;
            }
            pn.ref_ = id_ref;
        }
        param
    }

    fn parameter_node_mn(&mut self, simple_param_name: &str, type_mn: Option<&BinaryMN>) -> NodeRef {
        let public = self.cx().public_namespace();
        let id = self.identifier_node(simple_param_name, &public);
        let param_type = type_mn.map(|mn| self.member_expr_from_mn(mn));
        let param = self.nf().borrow_mut().parameter(VAR_TOKEN, Some(id.clone()), param_type.clone());
        {
            let id_ref = id.borrow().identifier().unwrap().ref_.clone();
            let pt_ref = param_type.as_ref().map(|pt| pt.borrow().as_member_expression().unwrap().ref_.clone()).flatten();
            let mut p = rbm(&param);
            let pn = p.parameter_mut().unwrap();
            if param_type.is_some() {
                pn.typeref = pt_ref;
            }
            pn.ref_ = id_ref;
        }
        param
    }

    fn attribute_list(&self, is_final: bool, is_override: bool, is_dynamic: bool, ns: Option<&ObjectValue>, builder: &Builder) -> NodeRef {
        let nf = self.nf();
        let lit = nf.borrow_mut().literal_string("");
        let attr = nf.borrow_mut().attribute_list(Some(lit), None);
        {
            let mut a = rbm(&attr);
            let al = a.as_attribute_list_mut().unwrap();
            al.has_final = is_final;
            al.has_override = is_override;
            al.has_dynamic = is_dynamic;
            // Statics are built by a ClassBuilder, instance members by an InstanceBuilder.
            if builder.is_class_builder() {
                al.has_static = true;
            }
            let ns = match ns {
                Some(n) => n,
                None => return attr.clone(),
            };
            let public = self.cx().public_namespace();
            if ns.ptr_eq(&public) || (ns.get_namespace_kind() == NS_PUBLIC && ns.is_package()) {
                // Only package-public namespaces carry the `public` specifier; user namespaces can be public too.
                al.has_public = true;
            } else if ns.is_internal() {
                al.has_internal = true;
            } else if ns.is_protected() {
                al.has_protected = true;
            } else if ns.is_private() {
                al.has_private = true;
            } else if builder.classname().map_or(false, |c| c.ns().map_or(false, |cns| ns.ptr_eq(&cns))) {
                al.has_public = true;
            } else {
                al.namespaces.add(ns.clone());
            }
        }
        attr
    }

    /// The qualified name of a multiname (`ns:name`), or the `ParameterizedName` of a type name.
    fn get_full_name(&self, mn: &BinaryMN) -> QName {
        if mn.kind == CONSTANT_TypeName {
            let base = self.get_binary_mn_from_cpool(mn.base_mn);
            let base_qn = self.get_full_name(&base);
            let params: Vec<QName> = mn.params.iter().map(|&p| self.get_full_name(&self.get_binary_mn_from_cpool(p))).collect();
            new_parameterized_name(params, base_qn.ns(), base_qn.name())
        } else {
            let full_name = self.get_string_from_cpool(mn.name_id);
            let ns = self.get_namespace(mn.ns_id);
            self.cx().compute_qualified_name("", &full_name, &ns, EMPTY_TOKEN)
        }
    }

    fn class_trait(&mut self, class_name: &str, ns: &ObjectValue, _slot_id: i32, class_id: i32, build_ast: bool) -> Option<DefAndSlot> {
        if class_id >= self.abc_data.get_class_info_size() || class_id < 0 {
            return None;
        }
        let current_scope = self.cx().scope().expect("NullPointerException");
        let mut ns = ns.clone();
        let mut region_name = self.region_name_stack.last().expect("NoSuchElementException").clone();
        if !region_name.is_empty() {
            region_name.push('/');
            ns = self.cx().get_namespace(&format!("{}{}", region_name, ns.name()));
        }
        let nf = self.nf();

        let iinfo_name_id = self.abc_data.get_instance_info(class_id).get_instance_name_id();
        let instance_mn = self.get_binary_mn_from_cpool(iinfo_name_id);
        let full_name = self.get_full_name(&instance_mn);
        let full_name_string = full_name.to_string();

        let super_id = self.abc_data.get_instance_info(class_id).get_super_id();
        let has_super = super_id != 0;
        let mut super_name: Option<QName> = None;
        let mut simple_super_name = String::new();
        let mut super_namespace: Option<ObjectValue> = None;
        if has_super {
            let super_mn = self.get_binary_mn_from_cpool(super_id);
            super_namespace = Some(self.get_namespace(super_mn.ns_id));
            super_name = Some(self.get_full_name(&super_mn));
            simple_super_name = self.get_string_from_cpool(super_mn.name_id);
        }

        let flags = self.abc_data.get_instance_info(class_id).get_flags();
        let interfaces = self.abc_data.get_instance_info(class_id).get_interfaces().clone();
        let mut interface_nodes: Option<NodeRef> = None;
        for &int_index in &interfaces {
            let int_mn = self.get_binary_mn_from_cpool(int_index);
            let simple_int_name = self.get_string_from_cpool(int_mn.name_id);
            let int_namespaces = if int_mn.ns_is_set {
                self.get_namespaces(int_mn.ns_id)
            } else {
                let n = Namespaces::with_capacity(1);
                n.add(self.get_namespace(int_mn.ns_id));
                n
            };
            let ident = nf.borrow_mut().identifier(&simple_int_name);
            let r = ReferenceValue::with_namespaces(&self.cx(), None, &simple_int_name, &int_namespaces);
            rbm(&ident).identifier_mut().unwrap().ref_ = Some(r.clone());
            let get_node = nf.borrow_mut().get_expression(Some(ident));
            let interface_node = nf.borrow_mut().member_expression(None, Some(get_node));
            rbm(&interface_node).as_member_expression_mut().unwrap().ref_ = Some(r.clone());
            let l = nf.borrow_mut().list(interface_nodes, Some(interface_node));
            rbm(&l).as_list_mut().unwrap().values.push(Value::Reference(r));
            interface_nodes = Some(l);
        }

        let is_final = (flags & CLASS_FLAG_final) != 0;
        let is_dynamic = (flags & CLASS_FLAG_sealed) == 0;
        let is_interface = (flags & CLASS_FLAG_interface) != 0;

        let id_node = nf.borrow_mut().identifier(class_name);
        {
            let name = id_node.borrow().identifier().unwrap().name.clone();
            let r = ReferenceValue::new(&self.cx(), None, &name, Some(&ns));
            rbm(&id_node).identifier_mut().unwrap().ref_ = Some(r);
        }
        let attr = self.attribute_list(is_final, false, is_dynamic, Some(&ns), &current_scope.builder().expect("NullPointerException"));
        let stmt_list = nf.borrow_mut().statement_list_concat(None, None);
        let cdn = if is_interface {
            nf.borrow_mut().binary_interface_definition(&self.ctx, Some(attr), Some(id_node.clone()), None, Some(stmt_list))
        } else {
            nf.borrow_mut().binary_class_definition(&self.ctx, Some(attr), Some(id_node.clone()), None, Some(stmt_list))
        };
        {
            let cx = self.cx();
            let id_ref = id_node.borrow().identifier().unwrap().ref_.clone();
            let mut c = rbm(&cdn);
            let cd = c.class_definition_mut().unwrap();
            cd.ref_ = id_ref;
            cd.interfaces = interface_nodes;
            cd.public_namespace = Some(cx.public_namespace());
            cd.protected_namespace = Some(cx.get_namespace_kind(&full_name_string, NS_PROTECTED));
            cd.static_protected_namespace = Some(cx.get_namespace_kind(&full_name_string, NS_STATIC_PROTECTED));
            cd.private_namespace = Some(cx.get_namespace_kind(&full_name_string, NS_PRIVATE));
            cd.default_namespace = Some(cx.get_namespace_kind(&full_name.ns().expect("NullPointerException").name(), NS_INTERNAL));
        }

        let (cframe, iframe) = {
            let cx = self.cx();
            if cx.is_builtin(&full_name_string) {
                let cframe = cx.builtin(&full_name_string).unwrap();
                let iframe = cframe.prototype().expect("NullPointerException");
                (cframe, iframe)
            } else {
                let (pns, spns) = {
                    let c = cdn.borrow();
                    let cd = c.class_definition().unwrap();
                    (cd.protected_namespace.clone(), cd.static_protected_namespace.clone())
                };
                let cb = new_class_builder(full_name.clone(), pns, spns);
                let cframe = TypeValue::define_type_value(&cx, cb, &full_name, TYPE_object);
                let ib = new_instance_builder(full_name.clone());
                ib.instance_data_mut().can_early_bind = false;
                let iframe = ObjectValue::with_builder(&cx, ib, Some(&cframe));
                cframe.set_prototype(Some(iframe.clone()));
                let mut c = rbm(&cdn);
                let cd = c.class_definition_mut().unwrap();
                cd.debug_name = Some(full_name_string.clone());
                // So that the class and instance builders get cleaned up.
                cd.owns_cframe = true;
                (cframe, iframe)
            }
        };
        {
            let mut c = rbm(&cdn);
            let cd = c.class_definition_mut().unwrap();
            cd.cframe = Some(cframe.clone());
            cd.iframe = Some(iframe.clone());
        }

        let cframe_builder = cframe.builder().expect("NullPointerException");
        let iframe_builder = iframe.builder().expect("NullPointerException");
        if is_interface {
            cframe_builder.class_data_mut().is_interface = true;
        }
        iframe_builder.set_is_dynamic(is_dynamic);
        cframe_builder.set_is_dynamic(is_dynamic);
        iframe_builder.set_is_final(is_final);
        cframe_builder.set_is_final(is_final);

        self.clsdefs_sets.last().expect("NoSuchElementException").borrow_mut().push(cdn.clone());

        if has_super {
            let super_name = super_name.unwrap();
            let lit = nf.borrow_mut().literal_string_pos(&super_name.to_string(), -1);
            rbm(&cdn).class_definition_mut().unwrap().baseclass = Some(lit);
            let cx = self.cx();
            if cx.is_builtin(&super_name.to_string()) {
                // int and uint derive from Number, so the base class matters.
                let super_type = cx.builtin(&super_name.to_string()).unwrap();
                cframe.set_baseclass(Some(super_type));
                let r = ReferenceValue::new(&cx, None, &super_name.to_string(), Some(&cx.public_namespace()));
                rbm(&cdn).class_definition_mut().unwrap().baseref = Some(r);
            } else {
                let r = ReferenceValue::new(&cx, None, &simple_super_name, super_namespace.as_ref());
                rbm(&cdn).class_definition_mut().unwrap().baseref = Some(r.clone());
                r.get_slot(&cx);
                drop(cx);
                let bc = self.get_type_from_qname(super_id);
                cframe.set_baseclass(Some(bc));
            }
        } else {
            let object_type = self.cx().object_type();
            if !cframe.ptr_eq(&object_type) {
                let id = nf.borrow_mut().identifier("Object");
                let get = nf.borrow_mut().get_expression(Some(id));
                let m = nf.borrow_mut().member_expression(None, Some(get));
                rbm(&cdn).class_definition_mut().unwrap().baseclass = Some(m);
                cframe.set_baseclass(Some(object_type));
            } else {
                rbm(&cdn).class_definition_mut().unwrap().baseclass = None;
                cframe.set_baseclass(None);
            }
        }

        let cs_bui = current_scope.builder().expect("NullPointerException");
        let (slot, slot_id) = {
            let cx = self.cx();
            let var_id = cs_bui.variable(&cx, &current_scope);
            let slot_id = cs_bui.explicit_var(&cx, &current_scope, class_name, &ns, Some(&cx.type_type()), -1, -1, var_id);
            let slot = current_scope.get_slot(&cx, slot_id).expect("NullPointerException");
            slot.set_object_value(Some(cframe.as_object().clone()));
            slot.set_imported(true);
            // Class definitions are const.
            slot.set_const(true);
            cs_bui.implicit_call(&cx, &current_scope, slot_id, Some(&cframe), CALL_Method, -1, -1);
            cs_bui.implicit_construct(&cx, &current_scope, slot_id, Some(&cframe), CALL_Method, -1, -1);
            (slot, slot_id)
        };
        let mut ret = DefAndSlot { def: None, slot: Some(slot.clone()) };

        if is_interface {
            cframe_builder.class_data_mut().is_interface = true;
            slot.set_impl_node(Some(cdn.clone()));
        }

        self.clsdefs_sets.push(new_node_list());
        self.region_name_stack.push(full_name_string.clone());

        self.cx().push_static_class_scopes(&cframe);
        {
            self.cx().push_scope(iframe.clone());
            {
                let instance_stmts = nf.borrow_mut().statement_list_concat(None, None);
                let data = self.abc_data.clone();
                self.parse_traits(data.get_instance_info(class_id).get_i_traits(), &instance_stmts, build_ast, None, build_ast);
                let items: Vec<NodeRef> = instance_stmts.borrow().as_statement_list().unwrap().items.iter().flatten().cloned().collect();
                let instanceinits = new_node_list();
                instanceinits.borrow_mut().extend(items);
                rbm(&cdn).class_definition_mut().unwrap().instanceinits = Some(instanceinits.clone());
                // The constructor has no trait of its own.
                let public = self.cx().public_namespace();
                let init_index = self.abc_data.get_instance_info(class_id).get_init_index();
                let d = self.method_trait("$construct", &public, 0, init_index, 0, 0, build_ast);
                if build_ast {
                    if let Some(def) = d.def.clone() {
                        instanceinits.borrow_mut().push(def);
                    }
                }
                let cx = self.cx();
                let implied_idx = current_scope.get_implicit_index(&cx, slot_id, NEW_TOKEN);
                if let Some(class_slot) = current_scope.get_slot(&cx, implied_idx) {
                    let ds = d.slot.as_ref().expect("NullPointerException");
                    class_slot.set_types(ds.get_types());
                    class_slot.set_decl_styles(ds.get_decl_styles());
                }
            }
            self.cx().pop_scope();

            let data = self.abc_data.clone();
            let ctraits = data.get_class_info(class_id).get_c_traits();
            let statements = cdn.borrow().class_definition().unwrap().statements.clone().expect("NullPointerException");
            self.parse_traits(ctraits, &statements, build_ast, None, build_ast);
        }
        self.cx().pop_static_class_scopes(Some(&cframe));

        self.clsdefs_sets.pop();
        self.region_name_stack.pop();

        ret.def = Some(cdn);
        Some(ret)
    }

    /// Defines a trait list's names in the current scope; `prog` collects the top-level definition names.
    fn parse_traits(&mut self, traits: &[Trait], statements: &NodeRef, build_ast: bool, prog: Option<&NodeRef>, build_ast_for_classes: bool) {
        for t in traits {
            let mn = self.get_binary_mn_from_cpool(t.get_name_index());
            let name = self.get_string_from_cpool(mn.name_id);
            let ns = self.get_namespace_from_multiname(&mn);

            let push_stmt = |def: &Option<NodeRef>| {
                rbm(statements).as_statement_list_mut().expect("ClassCastException").items.push(def.clone());
            };
            let add_top_level = |ns: &ObjectValue| {
                if let Some(p) = prog {
                    rbm(p).as_binary_program_mut().unwrap().toplevel_definitions.push(QName::new(Some(ns.clone()), &name));
                }
            };

            let d: Option<DefAndSlot> = match t.get_tag() {
                TRAIT_Var | TRAIT_Const => {
                    let d = self.slot_trait(&name, &ns, t.get_slot_id(), t.get_type_name(), t.get_value_index(), t.get_value_kind(), t.is_const_trait(), build_ast);
                    if build_ast {
                        push_stmt(&d.def);
                    }
                    add_top_level(&ns);
                    Some(d)
                }
                TRAIT_Method | TRAIT_Getter | TRAIT_Setter => {
                    let d = self.method_trait(&name, &ns, t.get_disp_id(), t.get_method_id(), t.get_attrs(), t.get_tag(), build_ast);
                    if build_ast {
                        push_stmt(&d.def);
                    }
                    add_top_level(&ns);
                    Some(d)
                }
                TRAIT_Class => {
                    let d = self.class_trait(&name, &ns, t.get_slot_id(), t.get_class_id(), build_ast_for_classes);
                    add_top_level(&ns);
                    d
                }
                _ => None,
            };

            if t.has_metadata() {
                if let Some(d) = &d {
                    for &meta_index in t.get_metadata().into_iter().flatten() {
                        let meta_node = self.parse_metadata_info(meta_index);
                        if let Some(def) = &d.def {
                            rbm(&meta_node).as_meta_data_mut().unwrap().def = Some(def.clone());
                            rbm(def).definition_mut().expect("ClassCastException").add_meta_data_node(Some(meta_node.clone()));
                            if build_ast {
                                push_stmt(&Some(meta_node.clone()));
                            }
                        }
                        if let Some(slot) = &d.slot {
                            let md_id = meta_node.borrow().as_meta_data().unwrap().get_id();
                            // The go-to-definition hints are only for tools and cost a lot of memory.
                            if md_id.as_deref() != Some(GO_TO_CTOR_DEFINITION_HELP) && md_id.as_deref() != Some(GO_TO_DEFINITION_HELP) {
                                let md = meta_node.borrow().as_meta_data().unwrap().get_metadata();
                                slot.add_metadata(md);
                            }
                        }
                    }
                }
            }
        }
    }

    /// The namespace of a multiname; of a set, the shortest URI, which is the one without a version mark.
    fn get_namespace_from_multiname(&self, mn: &BinaryMN) -> ObjectValue {
        if mn.ns_is_set {
            let ns_set = self.get_namespaces(mn.ns_id);
            if ns_set.size() > 1 {
                let mut ns: Option<ObjectValue> = None;
                for t in ns_set.to_vec() {
                    if ns.as_ref().map_or(true, |n| t.name_ref().encode_utf16().count() < n.name_ref().encode_utf16().count()) {
                        ns = Some(t);
                    }
                }
                ns.unwrap()
            } else {
                ns_set.at(0)
            }
        } else {
            self.get_namespace(mn.ns_id)
        }
    }

    fn parse_script(&mut self, script_index: i32, program: &NodeRef, collect_top_level: bool, build_ast_for_classes: bool) {
        if script_index < 0 || script_index >= self.abc_data.get_script_info_size() {
            return;
        }
        let data = self.abc_data.clone();
        let traits = data.get_script_info(script_index).get_script_traits();
        let statements = program.borrow().program().unwrap().statements.clone().expect("NullPointerException");
        // Top-level definitions always get nodes.
        self.parse_traits(traits, &statements, true, if collect_top_level { Some(program) } else { None }, build_ast_for_classes);
    }

    fn parse_metadata_info(&self, index: i32) -> NodeRef {
        let meta_node = self.nf().borrow_mut().meta_data(None, -1);
        let md = self.abc_data.get_metadata(index, &meta_node);
        rbm(&meta_node).as_meta_data_mut().unwrap().set_metadata(md);
        meta_node
    }

    fn get_string_from_cpool(&self, id: i32) -> String {
        self.abc_data.get_string(id)
    }

    fn get_decimal_from_cpool(&self, id: i32) -> Decimal128 {
        self.abc_data.get_decimal(id)
    }

    fn get_number_from_cpool(&self, id: i32, kind: i32) -> f64 {
        match kind {
            CONSTANT_Integer => self.abc_data.get_int(id) as f64,
            CONSTANT_UInteger => (self.abc_data.get_uint(id) & 0xFFFFFFFF) as f64,
            CONSTANT_Double => self.abc_data.get_double(id),
            _ => 0.0,
        }
    }

    fn get_binary_mn_from_cpool(&self, index: i32) -> Rc<BinaryMN> {
        self.abc_data.get_name(index)
    }

    fn get_namespaces(&self, namespace_set_id: i32) -> Namespaces {
        let ns_ids = self.abc_data.get_namespace_set(namespace_set_id).get_namespace_ids().clone();
        let val = Namespaces::with_capacity(ns_ids.len());
        for id in ns_ids {
            val.add(self.get_namespace(id));
        }
        val
    }

    fn get_namespace(&self, namespace_id: i32) -> ObjectValue {
        let cx = self.cx();
        if namespace_id == 0 {
            return cx.any_namespace();
        }
        let n = self.abc_data.get_namespace(namespace_id);
        let kind = n.ns_kind as i32;
        let uri = n.get_name();
        match kind {
            CONSTANT_Namespace => cx.get_namespace(&uri),
            CONSTANT_PackageNamespace => {
                let ns = cx.get_namespace(&uri);
                ns.set_package(true);
                ns
            }
            CONSTANT_ProtectedNamespace => cx.get_namespace_kind(&uri, NS_PROTECTED),
            CONSTANT_PackageInternalNs => cx.get_namespace_kind(&uri, NS_INTERNAL),
            CONSTANT_PrivateNamespace => cx.get_namespace_kind(&uri, NS_PRIVATE),
            CONSTANT_ExplicitNamespace => cx.get_namespace_kind(&uri, NS_EXPLICIT),
            CONSTANT_StaticProtectedNs => cx.get_namespace_kind(&uri, NS_STATIC_PROTECTED),
            _ => panic!("IllegalStateException: Invalid kind:{}", kind),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::asc::emit::features::{DIALECT_AS3, TARGET_AVM2};
    use crate::asc::util::context::ContextStatics;

    const PLAYERGLOBAL: &str = r"C:\Program Files\Adobe\Adobe Animate 2024\Common\Configuration\ActionScript 3.0\FP11.2\playerglobal.swc";

    /// `ActionBlockEmitter.GetMethodId`: one global list of method names.
    struct GlobalMethodNames(Vec<String>, crate::asc::semantics::emitter::EmitterBase);

    impl crate::asc::semantics::emitter::Emitter for GlobalMethodNames {
        fn emitter_base(&self) -> &crate::asc::semantics::emitter::EmitterBase {
            &self.1
        }

        fn emitter_base_mut(&mut self) -> &mut crate::asc::semantics::emitter::EmitterBase {
            &mut self.1
        }

        fn get_method_id(&mut self, name: &str, _namespaces: &Namespaces) -> i32 {
            if let Some(i) = self.0.iter().position(|n| n == name) {
                return i as i32 + 1;
            }
            self.0.push(name.to_string());
            self.0.len() as i32
        }
    }

    /// A context set up as mxmlc sets one up for an AS3 compile in strict mode.
    pub(crate) fn mxmlc_context() -> ContextRef {
        ObjectValue::init();
        let statics = Rc::new(RefCell::new(ContextStatics::new()));
        {
            let mut s = statics.borrow_mut();
            s.use_static_semantics = true;
            s.dialect = DIALECT_AS3;
            s.set_abc_version(TARGET_AVM2);
        }
        let cx = Rc::new(RefCell::new(Context::new(statics)));
        cx.borrow().set_emitter(Some(Rc::new(RefCell::new(GlobalMethodNames(Vec::new(), Default::default())))));
        Context::get_node_factory(&cx);
        cx
    }

    pub(crate) fn playerglobal_abcs() -> Option<Vec<Vec<u8>>> {
        let data = std::fs::read(PLAYERGLOBAL).ok()?;
        let bundle = crate::bundle::open(std::path::Path::new(PLAYERGLOBAL), &data).ok()?;
        let mut out = Vec::new();
        for (_, swf) in &bundle.entries {
            let swf = crate::swf::Swf::parse(swf).ok()?;
            for tag in &swf.timeline.tags {
                if let Some((abc, _)) = crate::abc::tag_abc(tag) {
                    out.push(abc.to_vec());
                }
            }
        }
        Some(out)
    }

    fn type_info_name(t: Option<crate::asc::semantics::type_info::TypeInfo>) -> String {
        match t.and_then(|t| t.get_type_value()) {
            Some(tv) => tv.name_opt().map_or("?".to_string(), |n| n.to_string()),
            None => "null".to_string(),
        }
    }

    fn value_string(v: Option<Value>) -> String {
        match v {
            None => "null".to_string(),
            Some(Value::Object(o)) => match o.as_type_value() {
                Some(t) => format!("type:{}", t),
                None => format!("obj:{}", o.get_value()),
            },
            Some(_) => "other".to_string(),
        }
    }

    /// The names and slots of one object, in the format the Java driver prints.
    pub(crate) fn dump_object(cx: &Context, label: &str, ob: &ObjectValue, out: &mut String) {
        use std::fmt::Write;
        writeln!(out, " OBJ {} name={} builder={}", label, ob.name(), ob.builder().map_or("null", |b| b.kind_name())).unwrap();
        ob.with_names(|names| {
            if let Some(names) = names {
                let mut i = 0;
                while names.has_next(i) != -1 {
                    let ns = names.get_namespace(i as usize);
                    writeln!(
                        out,
                        "  NAME {} {} {}/{} t={} slot={}",
                        i,
                        names.get_name(i as usize),
                        ns.name(),
                        ns.get_namespace_kind(),
                        names.get_type(i as usize),
                        names.get_slot(i as usize)
                    )
                    .unwrap();
                    i += 1;
                }
            }
        });
        for s in ob.slots().unwrap_or_default() {
            let types = s.get_types().map_or("null".to_string(), |t| t.borrow().iter().map(|ti| type_info_name(Some(ti.clone()))).collect::<Vec<_>>().join(","));
            let decl = s.get_decl_styles().map_or("null".to_string(), |d| d.borrow().iter().map(|b| b.to_string()).collect::<Vec<_>>().join(","));
            writeln!(
                out,
                "  SLOT {} {} type={} var={} mid={} mname={} f={} o={} c={} imp={} get={} cs={} dk={} call={} new={} by={} val={} init={} types={} decl={}",
                s.id(),
                if s.is_variable_slot() { "V" } else { "M" },
                type_info_name(s.get_type()),
                s.get_var_index(),
                s.get_method_id(),
                s.get_method_name(),
                s.is_final() as i32,
                s.is_override() as i32,
                s.is_const() as i32,
                s.is_imported() as i32,
                s.is_getter() as i32,
                s.get_call_sequence(),
                s.get_dispatch_kind(),
                s.implies(cx, EMPTY_TOKEN),
                s.implies(cx, NEW_TOKEN),
                s.declared_by().map_or("null".to_string(), |d| d.name()),
                value_string(s.get_value()),
                value_string(s.data().kind_value()),
                types,
                decl
            )
            .unwrap();
        }
    }

    pub(crate) fn dump_type(cx: &Context, name: &str, out: &mut String) {
        use std::fmt::Write;
        let t = match cx.user_defined(name).or_else(|| cx.builtin(name)) {
            Some(t) => t,
            None => {
                writeln!(out, "TYPE {} missing", name).unwrap();
                return;
            }
        };
        writeln!(
            out,
            "TYPE {} resolved={} type_id={} base={} iface={} dyn={} final={}",
            t,
            t.resolved(),
            t.type_id(),
            t.baseclass().map_or("null".to_string(), |b| b.to_string()),
            t.is_interface(),
            t.builder().map_or(false, |b| b.is_dynamic()),
            t.builder().map_or(false, |b| b.is_final())
        )
        .unwrap();
        dump_object(cx, "class", t.as_object(), out);
        if let Some(p) = t.prototype() {
            dump_object(cx, "instance", &p, out);
        }
    }

    pub(crate) fn import_playerglobal() -> Option<ContextRef> {
        let abcs = playerglobal_abcs()?;
        if let Ok(dir) = std::env::var("ASC_ABC_DIR") {
            for (i, abc) in abcs.iter().enumerate() {
                std::fs::write(format!("{}/{:04}.abc", dir, i), abc).unwrap();
            }
        }
        let cx = mxmlc_context();
        for abc in abcs {
            let mut p = AbcParser::new(&cx, abc);
            assert!(p.parse_abc().is_some(), "AbcParser failed");
        }
        Some(cx)
    }

    #[test]
    fn global_object_has_operator_and_attribute_slots() {
        let cx = mxmlc_context();
        let cxb = cx.borrow();
        let global = ObjectValue::with_builder(&cxb, new_global_builder(), Some(&cxb.no_type()));
        let proto = global.proto_field().expect("global prototype");
        let ns = cxb.public_namespace();
        let s = proto.get(&cxb, "void", &ns).expect("void attribute");
        assert!(s.get_value().and_then(|v| v.as_type_value()).map_or(false, |t| t.ptr_eq(&cxb.void_type())));
        let slot = proto.get_slot(&cxb, crate::asc::emit::runtime_constants::SLOT_Global_TypeofOp).unwrap();
        assert_eq!(slot.get_method_id(), crate::asc::emit::runtime_constants::UNARY_TypeofOp);
        let idx = proto.get_overload_index(&cxb, crate::asc::emit::runtime_constants::SLOT_Global_TypeofOp, Some(&cxb.int_type()));
        assert_eq!(idx, crate::asc::emit::runtime_constants::SLOT_Global_TypeofOp_I);
        assert!(global.has_name(&cxb, GET_TOKEN, "_cv", &ObjectValue::internal_namespace()));
    }

    #[test]
    fn imports_playerglobal() {
        let cx = match import_playerglobal() {
            Some(cx) => cx,
            None => return,
        };
        let cxb = cx.borrow();
        let sprite = cxb.user_defined("flash.display:Sprite").expect("Sprite");
        assert!(sprite.resolved());
        assert_eq!(sprite.baseclass().unwrap().to_string(), "flash.display:DisplayObjectContainer");
        let iframe = sprite.prototype().unwrap();
        let public = cxb.public_namespace();
        assert!(iframe.has_name(&cxb, GET_TOKEN, "graphics", &public));
        let r = ReferenceValue::new(&cxb, Some(iframe.clone()), "startDrag", Some(&public));
        let slot = r.get_slot_kind(&cxb, EMPTY_TOKEN).expect("startDrag call slot");
        assert_eq!(slot.get_method_name(), "startDrag$0");
        assert_eq!(slot.get_types().unwrap().borrow().len(), 2);
        let mut out = String::new();
        for name in ["flash.display:Sprite", "flash.events:EventDispatcher", "Object", "int", "__AS3__.vec:Vector", "flash.display:IBitmapDrawable"] {
            dump_type(&cxb, name, &mut out);
        }
        if let Ok(path) = std::env::var("ASC_MODEL_DUMP") {
            std::fs::write(path, &out).unwrap();
        }
        if let Ok(path) = std::env::var("ASC_MODEL_DUMP_ALL") {
            use std::fmt::Write;
            let mut all = String::new();
            // Instantiating Vector.<int> on first lookup exercises TypeValue.resolve and inheritClassSlotsStatic.
            let pn = new_parameterized_name(vec![cxb.int_type().name()], Some(cxb.get_namespace("__AS3__.vec")), "Vector");
            let vi = TypeValue::get_type_value(&cxb, &pn);
            writeln!(all, "VECTOR hasName length={}", vi.has_name(&cxb, GET_TOKEN, "length", &public)).unwrap();
            let ls = vi.prototype().unwrap().get(&cxb, "length", &public);
            writeln!(all, "VECTOR length slot={} indexed={}", ls.map_or("null".to_string(), |s| s.id().to_string()), vi.indexed_type().map_or("null".to_string(), |t| t.to_string())).unwrap();
            dump_type(&cxb, &pn.to_string(), &mut all);
            let mut names: Vec<String> = cxb.statics.borrow().user_defined.keys().cloned().collect();
            names.sort_by(|a, b| crate::asc::util::jhash::java_compare(a, b));
            for n in &names {
                dump_type(&cxb, n, &mut all);
            }
            let gp = cxb.statics.borrow().global_prototype.clone().unwrap();
            dump_object(&cxb, "globalPrototype", &gp, &mut all);
            std::fs::write(path, &all).unwrap();
        }
    }
}
