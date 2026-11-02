//! Port of `flex2.compiler.as3.genext.GenerativeExtension`'s shared constant and its `parse2` step that attaches the
//! type analyzer's `ClassInfo` to each generative class info.

use indexmap::IndexMap;

use super::generative_class_info::GenerativeClassInfoRef;
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::parser::node::NodeRef;
use crate::asc::util::context::ContextRef;

pub const IEVENT_DISPATCHER: &str = "IEventDispatcher";

/// `node.evaluate(cx, typeAnalyzer)` and the `setClassInfo` loop of `parse2`.
pub fn analyze_classes(cx: &ContextRef, node: &NodeRef, symbol_table: &SymbolTable, class_map: &IndexMap<String, GenerativeClassInfoRef>) {
    let type_analyzer = symbol_table.get_type_analyzer();
    type_analyzer.borrow_mut().evaluate(cx, node, symbol_table);
    for (class_name, info) in class_map {
        let class_info = type_analyzer.borrow().get_class_info(class_name);
        info.borrow_mut().set_class_info(class_info);
    }
}
