//! Port of `macromedia/asc/embedding/avmplus/ImportBuilder.java`: builds the objects that imported definitions are
//! bound in; each slot's base node is the expression naming the import.

use crate::asc::emit::global_builder::new_global_builder;
use crate::asc::semantics::builder::{Builder, BuilderKind};
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::q_name::QName;
use crate::asc::semantics::slot::Slot;
use crate::asc::semantics::type_value::TypeValue;
use crate::asc::util::context::Context;
use crate::asc::util::namespaces::Namespaces;

pub fn new_import_builder(name: QName) -> Builder {
    let b = Builder::from_kind(BuilderKind::Import);
    b.data_mut().classname = Some(name);
    b
}

pub(crate) fn build(b: &Builder, cx: &Context, ob: &ObjectValue) {
    b.bind(cx, ob);
    {
        let mut d = b.data_mut();
        d.var_offset = 0;
        d.reg_offset = 1;
    }
    let globbui = new_global_builder();
    globbui.build(cx, ob);
}

/// `slot.setBaseNode(nf.memberExpression(null, nf.getExpression(nf.identifier(classname.toString(), pos))))`.
fn set_base_node(b: &Builder, cx: &Context, slot: &Slot, pos: i32) {
    let nf = cx.node_factory();
    let mut nf = nf.borrow_mut();
    let classname = b.classname().map_or("null".to_string(), |c| c.to_string());
    let id = nf.identifier_pos(&classname, pos);
    let get = nf.get_expression(Some(id));
    let member = nf.member_expression(None, Some(get));
    slot.set_base_node(Some(member));
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn explicit_var_nss(b: &Builder, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, type_: Option<&TypeValue>, _expected_id: i32, _method_id: i32, _var_id: i32) -> i32 {
    let slot_id = b.base_explicit_var_nss(cx, ob, name, namespaces, type_, -1, -1, -1);
    ob.get_slot(cx, slot_id).expect("NullPointerException").add_type(type_.expect("NullPointerException").get_default_type_info());
    let slot = ob.get_slot(cx, slot_id).expect("NullPointerException");
    set_base_node(b, cx, &slot, 0);
    slot_id
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn explicit_get_nss(b: &Builder, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, type_: Option<&TypeValue>, is_final: bool, is_override: bool, _expected_id: i32, _method_id: i32, _var_id: i32) -> i32 {
    let slot_id = b.base_explicit_get_nss(cx, ob, name, namespaces, type_, is_final, is_override, -1, -1, -1);
    let slot = ob.get_slot(cx, slot_id).expect("NullPointerException");
    set_base_node(b, cx, &slot, -1);
    slot_id
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn explicit_set_nss(b: &Builder, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, type_: Option<&TypeValue>, is_final: bool, is_override: bool, _expected_id: i32, _method_id: i32, _var_id: i32) -> i32 {
    let slot_id = b.base_explicit_set_nss(cx, ob, name, namespaces, type_, is_final, is_override, -1, -1, -1);
    let slot = ob.get_slot(cx, slot_id).expect("NullPointerException");
    set_base_node(b, cx, &slot, -1);
    slot_id
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn explicit_call_nss(b: &Builder, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, type_: Option<&TypeValue>, is_final: bool, is_override: bool, _expected_id: i32, _method_id: i32, _var_id: i32) -> i32 {
    let function_type = if cx.use_static_semantics() { Some(cx.function_type()) } else { type_.cloned() };
    let slot_id = b.base_explicit_call_nss(cx, ob, name, namespaces, function_type.as_ref(), is_final, is_override, -1, -1, -1);
    let slot = ob.get_slot(cx, slot_id).expect("NullPointerException");
    if cx.use_static_semantics() {
        slot.set_const(true);
    }
    set_base_node(b, cx, &slot, -1);
    slot_id
}
