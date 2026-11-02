//! Port of `flex2.compiler.as3.managed.ManagedExtensionError`, which the MXML implementation pass adds.

use crate::asc::flex2::compiler::as3::extension::Extension;
use crate::asc::flex2::compiler::as3::syntax_tree_evaluator::localized2;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit::Level;

const MD_MANAGED: &str = "Managed";

pub struct ManagedExtensionError;

impl Extension for ManagedExtensionError {
    fn parse1(&mut self, unit: &UnitRef, _symbol_table: &mut SymbolTable) {
        let cx = unit.borrow().context.get_asc_context().expect("NullPointerException");
        let metadata = unit.borrow().metadata.clone();
        for meta_data_node in metadata {
            let (id, pos) = {
                let b = meta_data_node.borrow();
                (b.meta_data().expect("ClassCastException").get_id(), b.pos())
            };
            if id.as_deref() == Some(MD_MANAGED) {
                localized2(&cx, Level::Error, None, pos, "[Managed] may not be used on mxml components.".to_string());
            }
        }
    }
}
