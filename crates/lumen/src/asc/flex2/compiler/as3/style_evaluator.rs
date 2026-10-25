//! Ports of `flex2.compiler.as3.StyleEvaluator` and `StyleExtension`: a unit's `[Style]` metadata into its `Styles`.

use super::evaluator_adapter::{self, Adapter};
use super::extension::Extension;
use super::syntax_tree_evaluator::localized2;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit::Level;
use crate::asc::flex2::compiler::util::name_formatter;
use crate::asc::parser::node::{Kind, NodeRef};
use crate::asc::util::context::ContextRef;

pub struct StyleEvaluator {
    unit: UnitRef,
}

impl StyleEvaluator {
    pub fn new(unit: UnitRef) -> StyleEvaluator {
        StyleEvaluator { unit }
    }

    fn meta_data(&mut self, cx: &ContextRef, node: &NodeRef) {
        let (id, is_class, pos) = {
            let b = node.borrow();
            let m = b.as_meta_data().unwrap();
            (m.get_id(), m.def.as_ref().is_some_and(|d| d.borrow().class_definition().is_some()), b.pos())
        };
        if id.as_deref() != Some("Style") {
            return;
        }
        if is_class {
            self.process_style(cx, node);
        } else {
            localized2(cx, Level::Error, None, pos, "[Style] must annotate a class.".to_string());
        }
    }

    fn process_style(&mut self, cx: &ContextRef, meta_data_node: &NodeRef) {
        let (md, pos) = {
            let b = meta_data_node.borrow();
            (b.as_meta_data().unwrap().get_metadata().unwrap_or_default(), b.pos())
        };
        let style_name = md.get_value_key("name");
        let type_name = md.get_value_key("type");
        if style_name.is_none() {
            localized2(cx, Level::Error, None, pos, "[Style] metadata has missing 'name'.".to_string());
        }
        if let Some(t) = type_name {
            self.unit.borrow_mut().expressions.insert(crate::asc::flex2::compiler::util::Name::M(name_formatter::to_multi_name(&t)));
        }
        self.register_style(cx, meta_data_node, style_name, md);
    }

    fn register_style(&mut self, cx: &ContextRef, meta_data_node: &NodeRef, name: Option<String>, md: crate::asc::semantics::meta_data::MetaData) {
        let source = self.unit.borrow().get_source();
        let name = name.unwrap_or_else(|| "null".to_string());
        let r = self.unit.borrow_mut().styles.add_style(&name, md, Some(&source));
        if let Err(e) = r {
            // `localizedWarning2` of a `CompilerError`: the handler logs it as the error it is, outside ASC's count.
            let pos = meta_data_node.borrow().pos().max(0);
            let (ln, col, source, origin) = {
                let c = cx.borrow();
                (c.get_input_line(pos), c.get_input_col(pos), c.get_error_line_text(pos), c.get_error_origin())
            };
            crate::asc::flex2::compiler::toolkit::log(crate::asc::flex2::compiler::toolkit::CompilerMessage::error(e.get_localized_message()), Some(&origin), ln, col, Some(&source));
        }
    }
}

impl Adapter for StyleEvaluator {
    fn visit(&mut self, cx: &ContextRef, node: &NodeRef) -> bool {
        if matches!(node.borrow().kind, Kind::MetaData(_)) {
            self.meta_data(cx, node);
            return true;
        }
        false
    }
}

/// `StyleExtension`: runs a `StyleEvaluator` in `parse1` when the unit has metadata and no styles yet.
#[derive(Default)]
pub struct StyleExtension;

/// `StyleExtension.parse1(unit, typeTable)`, which `AbcCompiler` runs too.
pub fn style_extension_parse1(unit: &UnitRef) {
    let run = {
        let u = unit.borrow();
        !u.metadata.is_empty() && u.styles.size() == 0
    };
    if run {
        let (node, cx) = {
            let u = unit.borrow();
            (u.get_syntax_tree().unwrap(), u.context.get_asc_context().unwrap())
        };
        let mut e = StyleEvaluator::new(unit.clone());
        evaluator_adapter::evaluate(&mut e, &cx, &node);
    }
}

impl Extension for StyleExtension {
    fn parse1(&mut self, unit: &UnitRef, _symbol_table: &mut SymbolTable) {
        style_extension_parse1(unit);
    }
}

/// `symbolTable.registerStyles(unit.styles)`, logging a `StyleConflictException` against the unit's source.
pub fn register_unit_styles(unit: &UnitRef, symbol_table: &mut SymbolTable) {
    let styles = unit.borrow().styles.clone();
    if let Err(e) = symbol_table.register_styles(&styles) {
        let name = unit.borrow().get_source().borrow().get_name_for_reporting().to_string();
        crate::asc::flex2::compiler::toolkit::log_path(Level::Error, &name, &e.get_localized_message(), -1);
    }
}
