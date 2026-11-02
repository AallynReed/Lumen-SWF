//! Port of `flex2.compiler.mxml.rep.init.StaticPropertyInitializer`.

use std::rc::Rc;

use super::initializer::InitializerRef;
use super::value_initializer::{ValueInitializer, ValueInitializerKind};
use crate::asc::flex2::compiler::mxml::rep::deps::{Property, StandardDefs};
use crate::asc::flex2::compiler::mxml::rep::model::RValue;

impl ValueInitializer {
    /// `new StaticPropertyInitializer(property, value, line, defs)`.
    pub fn new_static_property(property: Property, value: RValue, line: i32, defs: Rc<StandardDefs>) -> InitializerRef {
        ValueInitializer::create(value, line, defs, ValueInitializerKind::StaticProperty { property })
    }
}
