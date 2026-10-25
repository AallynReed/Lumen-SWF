//! Port of `macromedia/asc/embedding/avmplus/InstanceBuilder.java`: builds a class's instance traits.

use crate::asc::parser::tokens::*;
use crate::asc::semantics::builder::{Builder, BuilderKind};
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::q_name::QName;
use crate::asc::semantics::reference_value::ReferenceValue;
use crate::asc::semantics::slot::*;
use crate::asc::semantics::type_value::TypeValue;
use crate::asc::util::context::Context;
use crate::asc::util::namespaces::Namespaces;

pub struct InstanceBuilderData {
    pub has_ctor: bool,
    pub calls_super_ctor: bool,
    pub ctor_name: Option<String>,
    pub basebui: Option<Builder>,
    pub can_early_bind: bool,
    pub interface_refs: Vec<ReferenceValue>,
}

pub fn new_instance_builder(classname: QName) -> Builder {
    let b = Builder::from_kind(BuilderKind::Instance(InstanceBuilderData {
        has_ctor: false,
        calls_super_ctor: false,
        ctor_name: None,
        basebui: None,
        can_early_bind: true,
        interface_refs: Vec::new(),
    }));
    {
        let mut d = b.data_mut();
        d.local_method_count = 0;
        d.classname = Some(classname);
    }
    b
}

fn erase_ids(b: &Builder) -> bool {
    b.is_intrinsic() || b.instance_data().basebui.as_ref().map_or(false, |bb| bb.is_intrinsic())
}

pub(crate) fn build(b: &Builder, cx: &Context, ob: &ObjectValue) {
    b.bind(cx, ob);
    let mut d = b.data_mut();
    d.var_offset = 0;
    d.reg_offset = 1;
}

/// Allocates a method id, reusing the base class's id for an overridden name.
pub(crate) fn method(b: &Builder, cx: &Context, _ob: &ObjectValue, name: &str, namespaces: &Namespaces, is_intrinsic: bool) -> i32 {
    if b.is_intrinsic() || is_intrinsic {
        return -1;
    }
    let method_id = b.get_method_id(cx, name, namespaces);
    let mut d = b.data_mut();
    if method_id > d.method_count {
        d.method_count = method_id;
    }
    d.method_offset + method_id
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
    for n in namespaces.to_vec() {
        b.name(cx, GET_TOKEN, name, &n);
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
    for n in namespaces.to_vec() {
        b.name(cx, SET_TOKEN, name, &n);
    }
    slot_id
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn explicit_var(b: &Builder, cx: &Context, ob: &ObjectValue, name: &str, ns: &ObjectValue, type_: Option<&TypeValue>, expected_id: i32, method_id: i32, var_id: i32) -> i32 {
    let slot_id = ob.add_variable_slot(cx, type_, var_id);
    ob.get_slot(cx, slot_id).expect("NullPointerException").add_type(type_.expect("NullPointerException").get_default_type_info());
    b.check_slot_index(expected_id, slot_id);
    ob.define_name(cx, GET_TOKEN, name, ns, slot_id);
    ob.define_name(cx, SET_TOKEN, name, ns, slot_id);
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
    for n in namespaces.to_vec() {
        b.name(cx, VAR_TOKEN, name, &n);
    }
    slot_id
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn explicit_call_nss(b: &Builder, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, type_: Option<&TypeValue>, is_final: bool, is_override: bool, expected_id: i32, method_id: i32, var_id: i32) -> i32 {
    let slot_id = b.base_explicit_get_nss(cx, ob, name, namespaces, Some(&cx.function_type()), is_final, is_override, expected_id, method_id, var_id);
    ob.get_slot(cx, slot_id).expect("NullPointerException").set_getter(false);
    let implied_id = ob.add_slot_implicit(cx, slot_id, EMPTY_TOKEN, type_);
    let slot = ob.get_slot(cx, implied_id).expect("NullPointerException");
    slot.set_dispatch_kind(if is_final { DISPATCH_final } else { DISPATCH_virtual });
    slot.attrs(CALL_ThisMethod, method_id);
    slot.set_final(is_final);
    slot.set_override(is_override);
    slot.set_method_name(&format!("temp${}${}", b.classname().map_or("null".to_string(), |c| c.to_string()), name));
    slot.set_getter(false);
    if erase_ids(b) {
        slot.set_method_id(-1);
    }
    for n in namespaces.to_vec() {
        b.name(cx, EMPTY_TOKEN, name, &n);
    }
    slot_id
}

/// `ExplicitCall(cx, ob, name, ObjectValue ns, ...)`; unlike the namespaces form it leaves the getter flag alone.
#[allow(clippy::too_many_arguments)]
pub(crate) fn explicit_call(b: &Builder, cx: &Context, ob: &ObjectValue, name: &str, ns: &ObjectValue, type_: Option<&TypeValue>, is_final: bool, is_override: bool, expected_id: i32, method_id: i32, var_id: i32) -> i32 {
    let slot_id = b.explicit_get(cx, ob, name, ns, Some(&cx.function_type()), is_final, is_override, expected_id, method_id, var_id);
    ob.get_slot(cx, slot_id).expect("NullPointerException").set_getter(false);
    let implied_id = ob.add_slot_implicit(cx, slot_id, EMPTY_TOKEN, type_);
    let slot = ob.get_slot(cx, implied_id).expect("NullPointerException");
    slot.set_dispatch_kind(if is_final { DISPATCH_final } else { DISPATCH_virtual });
    slot.attrs(CALL_ThisMethod, method_id);
    slot.set_final(is_final);
    slot.set_override(is_override);
    slot.set_method_name(&format!("temp${}${}", b.classname().map_or("null".to_string(), |c| c.to_string()), name));
    if erase_ids(b) {
        slot.set_method_id(-1);
    }
    b.name(cx, EMPTY_TOKEN, name, ns);
    slot_id
}
