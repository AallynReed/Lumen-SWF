//! Port of `flex2.compiler.mxml.lang.BindingHandler`.

use crate::asc::flex2::compiler::mxml::rep::binding_expression::BindingExpressionRef;
use crate::asc::flex2::compiler::mxml::rep::model::ModelRef;

pub trait BindingHandler {
    fn invoke(&mut self, binding_expression: &BindingExpressionRef, dest: &ModelRef) -> Option<BindingExpressionRef>;
}
