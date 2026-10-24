//! Port of `macromedia/asc/embedding/avmplus/ClassBuilder.java`: builds a class object (its static members).

use crate::asc::parser::tokens::*;
use crate::asc::semantics::builder::{Builder, BuilderKind};
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::q_name::QName;
use crate::asc::semantics::slot::*;
use crate::asc::semantics::type_value::TypeValue;
use crate::asc::util::context::Context;
use crate::asc::util::namespaces::Namespaces;

pub struct ClassBuilderData {
    pub basebui: Option<Builder>,
    pub is_interface: bool,
    /// Alias of the namespace the class definition node created.
    pub protected_namespace: Option<ObjectValue>,
    pub static_protected_namespace: Option<ObjectValue>,
}

pub fn new_class_builder(classname: QName, protected_namespace: Option<ObjectValue>, static_protected_namespace: Option<ObjectValue>) -> Builder {
    let b = Builder::from_kind(BuilderKind::Class(ClassBuilderData {
        basebui: None,
        is_interface: false,
        protected_namespace,
        static_protected_namespace,
    }));
    b.data_mut().classname = Some(classname);
    b
}

fn erase_ids(b: &Builder) -> bool {
    b.is_intrinsic() || b.class_data().basebui.as_ref().map_or(false, |bb| bb.is_intrinsic())
}

pub(crate) fn build(b: &Builder, cx: &Context, ob: &ObjectValue) {
    b.bind(cx, ob);
    if b.classname().map_or("null".to_string(), |c| c.to_string()) == "Class" {
        return;
    }
    {
        let mut d = b.data_mut();
        d.var_offset = 0;
        d.reg_offset = 1;
    }
    let nss = Namespaces::with_capacity(1);
    nss.push_back(cx.public_namespace());
    // Class has two instance methods, get and set prototype, which use up the first two dispatch ids.
    let meth_id = b.method(cx, ob, "prototype$get", &nss, false);
    b.explicit_get_nss(cx, ob, "prototype", &nss, None, true, false, -1, meth_id, -1);
    b.method(cx, ob, "prototype$set", &nss, false);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn explicit_get_nss(b: &Builder, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, type_: Option<&TypeValue>, is_final: bool, is_override: bool, expected_id: i32, method_id: i32, _var_id: i32) -> i32 {
    let slot_id = ob.add_method_slot(cx, type_);
    b.check_slot_index(expected_id, slot_id);
    ob.define_names(cx, GET_TOKEN, name, namespaces, slot_id);
    let slot = ob.get_slot(cx, slot_id).expect("NullPointerException");
    slot.attrs(CALL_ThisMethod, method_id);
    slot.set_final(is_final);
    slot.set_override(is_override);
    if erase_ids(b) {
        slot.set_method_id(-1);
    }
    for it in namespaces.to_vec() {
        b.name(cx, GET_TOKEN, name, &it);
    }
    slot_id
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn explicit_set(b: &Builder, cx: &Context, ob: &ObjectValue, name: &str, ns: &ObjectValue, type_: Option<&TypeValue>, is_final: bool, is_override: bool, expected_id: i32, method_id: i32, _var_id: i32) -> i32 {
    let slot_id = ob.add_method_slot(cx, type_);
    b.check_slot_index(expected_id, slot_id);
    ob.define_name(cx, SET_TOKEN, name, ns, slot_id);
    let slot = ob.get_slot(cx, slot_id).expect("NullPointerException");
    slot.attrs(CALL_ThisMethod, method_id);
    slot.set_final(is_final);
    slot.set_override(is_override);
    if erase_ids(b) {
        slot.set_method_id(-1);
    }
    b.name(cx, SET_TOKEN, name, ns);
    slot_id
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn explicit_set_nss(b: &Builder, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, type_: Option<&TypeValue>, is_final: bool, is_override: bool, expected_id: i32, method_id: i32, _var_id: i32) -> i32 {
    let slot_id = ob.add_method_slot(cx, type_);
    b.check_slot_index(expected_id, slot_id);
    ob.define_names(cx, SET_TOKEN, name, namespaces, slot_id);
    let slot = ob.get_slot(cx, slot_id).expect("NullPointerException");
    slot.attrs(CALL_ThisMethod, method_id);
    slot.set_final(is_final);
    slot.set_override(is_override);
    if erase_ids(b) {
        slot.set_method_id(-1);
    }
    for it in namespaces.to_vec() {
        b.name(cx, SET_TOKEN, name, &it);
    }
    slot_id
}

/// Java's version defines only the getter name for a single-namespace var.
#[allow(clippy::too_many_arguments)]
pub(crate) fn explicit_var(b: &Builder, cx: &Context, ob: &ObjectValue, name: &str, ns: &ObjectValue, type_: Option<&TypeValue>, expected_id: i32, method_id: i32, var_id: i32) -> i32 {
    let slot_id = ob.add_variable_slot(cx, type_, var_id);
    ob.get_slot(cx, slot_id).expect("NullPointerException").add_type(type_.expect("NullPointerException").get_default_type_info());
    b.check_slot_index(expected_id, slot_id);
    ob.define_name(cx, GET_TOKEN, name, ns, slot_id);
    let slot = ob.get_slot(cx, slot_id).expect("NullPointerException");
    slot.attrs(CALL_ThisMethod, method_id);
    if erase_ids(b) {
        slot.set_var_index(-1);
    }
    b.name(cx, VAR_TOKEN, name, ns);
    slot_id
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn explicit_var_nss(b: &Builder, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, type_: Option<&TypeValue>, expected_id: i32, method_id: i32, var_id: i32) -> i32 {
    let slot_id = ob.add_variable_slot(cx, type_, var_id);
    ob.get_slot(cx, slot_id).expect("NullPointerException").add_type(type_.expect("NullPointerException").get_default_type_info());
    b.check_slot_index(expected_id, slot_id);
    ob.define_names(cx, GET_TOKEN, name, namespaces, slot_id);
    ob.define_names(cx, SET_TOKEN, name, namespaces, slot_id);
    let slot = ob.get_slot(cx, slot_id).expect("NullPointerException");
    slot.attrs(CALL_ThisMethod, method_id);
    if erase_ids(b) {
        slot.set_var_index(-1);
    }
    for it in namespaces.to_vec() {
        b.name(cx, VAR_TOKEN, name, &it);
    }
    slot_id
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn explicit_call_nss(b: &Builder, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, type_: Option<&TypeValue>, is_final: bool, is_override: bool, expected_id: i32, method_id: i32, var_id: i32) -> i32 {
    let slot_id = b.base_explicit_get_nss(cx, ob, name, namespaces, Some(&cx.function_type()), true, false, expected_id, method_id, var_id);
    ob.get_slot(cx, slot_id).expect("NullPointerException").set_getter(false);
    let implied_id = ob.add_slot_implicit(cx, slot_id, EMPTY_TOKEN, type_);
    let slot = ob.get_slot(cx, implied_id).expect("NullPointerException");
    slot.attrs(CALL_ThisMethod, method_id);
    slot.set_final(is_final);
    slot.set_override(is_override);
    slot.set_method_name(&format!("{}${}", b.classname().map_or("null".to_string(), |c| c.to_string()), name));
    slot.set_getter(false);
    if erase_ids(b) {
        slot.set_method_id(-1);
    }
    for it in namespaces.to_vec() {
        b.name(cx, EMPTY_TOKEN, name, &it);
    }
    slot_id
}
