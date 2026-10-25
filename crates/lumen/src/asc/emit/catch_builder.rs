//! Port of `macromedia/asc/embedding/avmplus/CatchBuilder.java`: an `ActivationBuilder` for catch scopes, with no
//! register offset.

use crate::asc::semantics::builder::{Builder, BuilderKind};

pub fn new_catch_builder() -> Builder {
    Builder::from_kind(BuilderKind::Catch { has_parameter: false })
}
