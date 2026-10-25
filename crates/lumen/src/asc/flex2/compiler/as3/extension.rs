//! Port of `flex2.compiler.as3.Extension`, the hooks `As3Compiler` runs after each of its phases. `TypeTable` is
//! reached through the symbol table.

use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::symbol_table::SymbolTable;

pub trait Extension {
    fn parse1(&mut self, _unit: &UnitRef, _symbol_table: &mut SymbolTable) {}
    fn parse2(&mut self, _unit: &UnitRef, _symbol_table: &mut SymbolTable) {}
    fn analyze1(&mut self, _unit: &UnitRef, _symbol_table: &mut SymbolTable) {}
    fn analyze2(&mut self, _unit: &UnitRef, _symbol_table: &mut SymbolTable) {}
    fn analyze3(&mut self, _unit: &UnitRef, _symbol_table: &mut SymbolTable) {}
    fn analyze4(&mut self, _unit: &UnitRef, _symbol_table: &mut SymbolTable) {}
    fn generate(&mut self, _unit: &UnitRef, _symbol_table: &mut SymbolTable) {}
}
