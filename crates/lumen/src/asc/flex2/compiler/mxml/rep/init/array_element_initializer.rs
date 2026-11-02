//! Port of `flex2.compiler.mxml.rep.init.ArrayElementInitializer`.

use std::rc::Rc;

use super::initializer::InitializerRef;
use super::value_initializer::{ValueInitializer, ValueInitializerKind};
use crate::asc::flex2::compiler::mxml::rep::deps::{StandardDefs, Type};
use crate::asc::flex2::compiler::mxml::rep::model::RValue;

impl ValueInitializer {
    /// `new ArrayElementInitializer(type, index, value, line, defs)`.
    pub fn new_array_element(type_: Type, index: usize, value: RValue, line: i32, defs: Rc<StandardDefs>) -> InitializerRef {
        ValueInitializer::create(value, line, defs, ValueInitializerKind::ArrayElement { type_, index })
    }
}
