//! Port of `macromedia/asc/embedding/avmplus/FunctionBuilder.java`: the builder of function objects.

use crate::asc::semantics::builder::{Builder, BuilderKind};

pub fn new_function_builder() -> Builder {
    Builder::from_kind(BuilderKind::Function)
}
