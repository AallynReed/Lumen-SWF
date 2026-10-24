//! Port of `macromedia/asc/embedding/avmplus/WithBuilder.java`: the builder of `with` scopes, with no register offset.

use crate::asc::semantics::builder::{Builder, BuilderKind};

pub fn new_with_builder() -> Builder {
    Builder::from_kind(BuilderKind::With)
}
