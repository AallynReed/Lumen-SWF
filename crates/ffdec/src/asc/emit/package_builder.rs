//! Port of `macromedia/asc/embedding/avmplus/PackageBuilder.java`: builds package objects.

use crate::asc::emit::global_builder::new_global_builder;
use crate::asc::parser::tokens::*;
use crate::asc::semantics::builder::{Builder, BuilderKind};
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::q_name::QName;
use crate::asc::semantics::slot::*;
use crate::asc::semantics::type_value::TypeValue;
use crate::asc::util::context::Context;
use crate::asc::util::namespaces::Namespaces;

pub fn new_package_builder(name: Option<QName>) -> Builder {
    let b = Builder::from_kind(BuilderKind::Package);
    b.data_mut().classname = name;
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn explicit_call_nss(b: &Builder, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, type_: Option<&TypeValue>, is_final: bool, is_override: bool, expected_id: i32, method_id: i32, var_id: i32) -> i32 {
    let function_type = if cx.use_static_semantics() { Some(cx.function_type()) } else { type_.cloned() };
    let slot_id = b.base_explicit_get_nss(cx, ob, name, namespaces, function_type.as_ref(), true, false, expected_id, -1, var_id);
    ob.get_slot(cx, slot_id).expect("NullPointerException").set_getter(false);
    let implied_id = ob.add_slot_implicit(cx, slot_id, EMPTY_TOKEN, type_);
    let slot = ob.get_slot(cx, implied_id).expect("NullPointerException");
    slot.attrs(CALL_ThisMethod, method_id);
    slot.set_final(is_final);
    slot.set_override(is_override);
    slot.set_method_name(&format!("{}${}", b.classname().map_or("null".to_string(), |c| c.to_string()), name));
    slot.set_getter(false);
    if method_id >= 0 {
        for n in namespaces.to_vec() {
            b.name(cx, EMPTY_TOKEN, name, &n);
        }
    }
    slot_id
}
