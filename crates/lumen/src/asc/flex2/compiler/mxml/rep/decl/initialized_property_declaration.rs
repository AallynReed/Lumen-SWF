//! Port of `flex2.compiler.mxml.rep.decl.InitializedPropertyDeclaration`, a `DynamicPropertyInitializer` kind.

use super::super::init::initializer::InitializerRef;
use super::super::init::value_initializer::{ValueInitializer, ValueInitializerKind};
use crate::asc::flex2::compiler::mxml::rep::deps::Type;
use crate::asc::flex2::compiler::mxml::rep::model::{ModelRef, RValue};
use crate::asc::flex2::compiler::util::name_formatter;

impl ValueInitializer {
    /// `new InitializedPropertyDeclaration(model, topLevel, line)`.
    pub fn new_initialized_property_declaration(model: &ModelRef, top_level: bool, line: i32) -> InitializerRef {
        let kind = ValueInitializerKind::InitializedPropertyDeclaration { type_: model.get_type().clone(), name: model.get_id().unwrap_or_default(), top_level };
        ValueInitializer::create(RValue::Model(model.clone()), line, model.get_standard_defs(), kind)
    }

    fn declared_model(&self) -> ModelRef {
        self.get_model().expect("ClassCastException")
    }

    pub(crate) fn initialized_property_declaration_lvalue_type(&self) -> Type {
        let ValueInitializerKind::InitializedPropertyDeclaration { type_, .. } = &self.kind else { unreachable!() };
        if self.declared_model().get_repeater_level() > 0 {
            type_.get_type_table().array_type()
        } else {
            type_.clone()
        }
    }

    pub fn get_type_expr(&self) -> String {
        name_formatter::to_dot(&self.get_lvalue_type().get_name().to_string())
    }

    pub fn get_top_level(&self) -> bool {
        matches!(self.kind, ValueInitializerKind::InitializedPropertyDeclaration { top_level: true, .. })
    }

    pub(crate) fn initialized_property_declaration_assign_expr(&self, lvalue_base: &str) -> String {
        if lvalue_base == "this" && self.has_definition() && self.declared_model().is_declared() {
            self.get_value_expr()
        } else {
            self.named_assign_expr(lvalue_base)
        }
    }
}
