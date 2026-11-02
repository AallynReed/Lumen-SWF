//! Port of `flex2.compiler.mxml.rep.init.StyleInitializer`.

use std::rc::Rc;

use super::initializer::InitializerRef;
use super::value_initializer::{ValueInitializer, ValueInitializerKind};
use crate::asc::flex2::compiler::mxml::rep::deps::{StandardDefs, Style};
use crate::asc::flex2::compiler::mxml::rep::model::RValue;

impl ValueInitializer {
    /// `new StyleInitializer(style, value, line, standardDefs)`.
    pub fn new_style(style: Style, value: RValue, line: i32, defs: Rc<StandardDefs>) -> InitializerRef {
        ValueInitializer::create(value, line, defs, ValueInitializerKind::Style { style })
    }

    /// `StyleInitializer.getStyle()`.
    pub fn get_style(&self) -> Option<Style> {
        match &self.kind {
            ValueInitializerKind::Style { style } => Some(style.clone()),
            _ => None,
        }
    }
}
