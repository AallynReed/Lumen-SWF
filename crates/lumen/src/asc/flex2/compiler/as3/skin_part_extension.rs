//! Port of `flex2.compiler.as3.SkinPartExtension`.

use super::extension::Extension;
use super::skin_part_evaluator::SkinPartEvaluator;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::symbol_table::SymbolTable;

pub struct SkinPartExtension;

impl Extension for SkinPartExtension {
    fn parse2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let node = unit.borrow().get_syntax_tree().expect("NullPointerException");
        let cx = unit.borrow().context.get_asc_context().expect("NullPointerException");
        let mut sp_evaluator = SkinPartEvaluator::new(symbol_table);
        super::evaluator_adapter::evaluate(&mut sp_evaluator, &cx, &node);
    }
}
