//! Port of `flex2.compiler.mxml.rep.init.DynamicPropertyInitializer`.

use std::rc::Rc;

use super::initializer::InitializerRef;
use super::value_initializer::{ValueInitializer, ValueInitializerKind};
use crate::asc::flex2::compiler::mxml::rep::deps::{StandardDefs, Type};
use crate::asc::flex2::compiler::mxml::rep::model::RValue;

impl ValueInitializer {
    /// `new DynamicPropertyInitializer(type, name, value, line, defs)`.
    pub fn new_dynamic_property(type_: Type, name: &str, value: RValue, line: i32, defs: Rc<StandardDefs>) -> InitializerRef {
        ValueInitializer::create(value, line, defs, ValueInitializerKind::DynamicProperty { type_, name: name.to_string() })
    }
}
