//! Port of `macromedia/asc/semantics/Type.java`, the abstract interface for types.

use crate::asc::semantics::type_value::TypeValue;
use crate::asc::util::context::Context;

pub trait Type {
    fn includes(&self, cx: &Context, value: Option<&TypeValue>) -> bool;
}
