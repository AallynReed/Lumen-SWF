//! Port of `macromedia/asc/semantics/OrdinaryBuilder.java`: a builder whose `build` does nothing.

use crate::asc::semantics::builder::{Builder, BuilderKind};

pub fn new_ordinary_builder() -> Builder {
    Builder::from_kind(BuilderKind::Ordinary)
}
