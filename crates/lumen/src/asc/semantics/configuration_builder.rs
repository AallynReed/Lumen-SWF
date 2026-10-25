//! Port of `macromedia/asc/semantics/ConfigurationBuilder.java`, the builder of configuration (`CONFIG::`) objects.

use crate::asc::semantics::builder::{Builder, BuilderKind};

pub fn new_configuration_builder() -> Builder {
    Builder::from_kind(BuilderKind::Configuration)
}
