//! Port of `macromedia/asc/embedding/avmplus/ActivationBuilder.java`: builds activation objects (locals and parameters).

use crate::asc::semantics::builder::{Builder, BuilderKind};
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::type_value::TypeValue;
use crate::asc::util::context::Context;
use crate::asc::util::namespaces::Namespaces;

pub fn new_activation_builder() -> Builder {
    Builder::from_kind(BuilderKind::Activation)
}

pub(crate) fn build(b: &Builder, cx: &Context, ob: &ObjectValue) {
    b.bind(cx, ob);
    let mut d = b.data_mut();
    d.var_offset = 0;
    d.reg_offset = 1;
}

/// A local function is a variable holding a closure.
#[allow(clippy::too_many_arguments)]
pub(crate) fn explicit_call_nss(b: &Builder, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, type_: Option<&TypeValue>, _is_final: bool, _is_override: bool, expected_id: i32, method_id: i32, var_id: i32) -> i32 {
    let mut var_id = var_id;
    if method_id >= 0 || var_id < 0 {
        var_id = b.variable(cx, ob);
    }
    let function_type = if cx.use_static_semantics() { Some(cx.function_type()) } else { type_.cloned() };
    b.base_explicit_var_nss(cx, ob, name, namespaces, function_type.as_ref(), expected_id, -1, var_id)
}
