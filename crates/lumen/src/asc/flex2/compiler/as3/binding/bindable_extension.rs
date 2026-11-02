//! Port of `flex2.compiler.as3.binding.BindableExtension` (a `GenerativeExtension`): `[Bindable]` codegen.

use std::cell::RefCell;
use std::rc::Rc;

use super::bindable_first_pass_evaluator::BindableFirstPassEvaluator;
use super::bindable_second_pass_evaluator::BindableSecondPassEvaluator;
use crate::asc::flex2::compiler::as3::evaluator_adapter::evaluate as adapter_evaluate;
use crate::asc::flex2::compiler::as3::extension::Extension;
use crate::asc::flex2::compiler::as3::genext::generative_extension::{analyze_classes, IEVENT_DISPATCHER};
use crate::asc::flex2::compiler::as3::genext::generative_second_pass_evaluator::GenerativeSecondPass;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::mxml::lang::standard_defs::{self, StandardDefs};
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::util::{MultiName, Name};

pub const FIRST_PASS_EVALUATOR_KEY: &str = "BindableFirstPassEvaluator";
pub type FirstPassEvaluatorRef = Rc<RefCell<BindableFirstPassEvaluator>>;

pub struct BindableExtension {
    generated_output_directory: Option<String>,
    generate_abstract_syntax_tree: bool,
    process_comments: bool,
}

impl BindableExtension {
    pub fn new(generated_output_directory: Option<String>, generate_abstract_syntax_tree: bool, process_comments: bool) -> BindableExtension {
        BindableExtension { generated_output_directory, generate_abstract_syntax_tree, process_comments }
    }
}

impl Extension for BindableExtension {
    fn parse1(&mut self, unit: &UnitRef, _symbol_table: &mut SymbolTable) {
        let (node, cx, meta_data) = {
            let u = unit.borrow();
            (u.get_syntax_tree().unwrap(), u.context.get_asc_context().unwrap(), u.metadata.clone())
        };
        let first_pass: FirstPassEvaluatorRef = Rc::new(RefCell::new(BindableFirstPassEvaluator::new(StandardDefs::get_standard_defs("halo"), meta_data)));
        adapter_evaluate(&mut *first_pass.borrow_mut(), &cx, &node);
        if first_pass.borrow().make_second_pass() {
            unit.borrow_mut().inheritance.insert(Name::M(MultiName::new(standard_defs::PACKAGE_FLASH_EVENTS, IEVENT_DISPATCHER)));
        }
        unit.borrow_mut().context.set_attribute(FIRST_PASS_EVALUATOR_KEY, first_pass);
    }

    fn parse2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let first_pass = unit.borrow_mut().context.remove_attribute(FIRST_PASS_EVALUATOR_KEY).and_then(|a| a.downcast::<RefCell<BindableFirstPassEvaluator>>().ok());
        let Some(first_pass) = first_pass else { return };
        if !first_pass.borrow().make_second_pass() {
            return;
        }
        let (node, cx) = {
            let u = unit.borrow();
            (u.get_syntax_tree().unwrap(), u.context.get_asc_context().unwrap())
        };
        let class_map = first_pass.borrow().get_class_map().clone();
        analyze_classes(&cx, &node, symbol_table, &class_map);
        let base = GenerativeSecondPass { unit: unit.clone(), generated_output_directory: self.generated_output_directory.clone(), generate_abstract_syntax_tree: self.generate_abstract_syntax_tree, process_comments: self.process_comments };
        let mut second_pass = BindableSecondPassEvaluator::new(base, class_map, symbol_table.get_type_analyzer(), symbol_table);
        adapter_evaluate(&mut second_pass, &cx, &node);
    }
}
