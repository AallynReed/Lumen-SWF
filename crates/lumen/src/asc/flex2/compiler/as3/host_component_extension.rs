//! Port of `flex2.compiler.as3.HostComponentExtension`: the `hostComponent` variable a `[HostComponent]` skin gets,
//! and the host's required skin parts and states checked against the skin. The doc-comment copy (asdoc only) is not ported.

use std::cell::RefCell;

use super::abstract_syntax_tree_util as ast;
use super::extension::Extension;
use super::reflect::as3_class::{AbcClass, PUBLIC_NAMESPACE};
use super::reflect::meta_data::MetaData;
use super::reflect::node_magic;
use super::syntax_tree_evaluator::localized2;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit::Level;
use crate::asc::flex2::compiler::util::{name_formatter, MultiName, Name};
use crate::asc::parser::node::NodeRef;
use crate::asc::util::context::{Context, ContextRef};

const SKINHOSTCOMPONENT: &str = "hostComponent";
const BINDABLE: &str = "Bindable";

pub struct HostComponentExtension {
    report_missing_required_skin_parts_as_warnings: bool,
}

fn input_origin(cx: &ContextRef) -> Option<String> {
    cx.borrow().input.as_ref().and_then(|i| i.borrow().origin.clone())
}

impl HostComponentExtension {
    pub fn new(report_missing_required_skin_parts_as_warnings: bool) -> HostComponentExtension {
        HostComponentExtension { report_missing_required_skin_parts_as_warnings }
    }

    fn generate_host_component_variable(&self, cx: &ContextRef, unit: &UnitRef, symbol_table: &SymbolTable) {
        let node = unit.borrow().host_component_meta_data.clone().unwrap();
        let (count, def, value0) = {
            let b = node.borrow();
            let m = b.meta_data().expect("ClassCastException");
            let count = m.count();
            (count, m.def.clone(), if count > 0 { m.get_value_index(0) } else { None })
        };
        if count != 1 {
            return;
        }
        let Some(def) = def.filter(|d| d.borrow().class_definition().is_some()) else { return };
        let value0 = value0.expect("NullPointerException");
        unit.borrow_mut().expressions.insert(Name::M(name_formatter::to_multi_name(&value0)));
        if class_declares_identifier(cx, &def, symbol_table, SKINHOSTCOMPONENT) {
            return;
        }
        let nf = Context::get_node_factory(cx);
        let (statement_list, type_expression) = {
            let mut node_factory = nf.borrow_mut();
            let binding_meta_data = ast::generate_meta_data(&mut node_factory, BINDABLE);
            binding_meta_data.borrow_mut().meta_data_mut().unwrap().set_id(Some(BINDABLE.to_string()));
            let statements = def.borrow().class_definition().unwrap().statements.clone();
            let statement_list = node_factory.statement_list(statements, Some(binding_meta_data));
            let type_expression = ast::generate_type_expression(&mut node_factory, &value0, true);
            (statement_list, type_expression)
        };
        let variable_definition = ast::generate_public_variable(cx, type_expression, SKINHOSTCOMPONENT);
        let statements = nf.borrow_mut().statement_list(Some(statement_list), Some(variable_definition.clone()));
        def.borrow_mut().class_definition_mut().unwrap().statements = Some(statements);
        let first_pass = unit.borrow().context.get_attribute_as::<RefCell<super::binding::bindable_first_pass_evaluator::BindableFirstPassEvaluator>>(super::binding::bindable_extension::FIRST_PASS_EVALUATOR_KEY);
        if let Some(first_pass) = first_pass {
            first_pass.borrow_mut().register_bindable_class_variable(cx, &def, &variable_definition);
        }
    }

    fn validate_required_skin_parts(&self, host_component_class: &AbcClass, skin_class: &AbcClass, cx: &ContextRef, position: i32, symbol_table: &SymbolTable) {
        for variable in host_component_class.get_var_iterator() {
            if let Some(list) = variable.get_meta_data("SkinPart") {
                self.validate_required_skin_parts_list(&list, &variable.get_qname().local_part, variable.get_type_name(), skin_class, symbol_table, cx, position);
            }
        }
        for getter in host_component_class.get_getter_iterator() {
            if let Some(list) = getter.get_meta_data("SkinPart") {
                self.validate_required_skin_parts_list(&list, &getter.get_qname().local_part, getter.get_return_type_name(), skin_class, symbol_table, cx, position);
            }
        }
        if let Some(super_type_name) = host_component_class.get_super_type_name() {
            if let Some(super_type) = symbol_table.get_class(Some(super_type_name)) {
                self.validate_required_skin_parts(&super_type, skin_class, cx, position, symbol_table);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn validate_required_skin_parts_list(&self, list: &[MetaData], host_skin_part_name: &str, host_skin_part_type_name: Option<String>, skin_class: &AbcClass, symbol_table: &SymbolTable, cx: &ContextRef, position: i32) {
        for skin_parts_meta_data in list {
            let skin_part_type_name = match skin_class.get_variable(&[PUBLIC_NAMESPACE], host_skin_part_name, true) {
                Some(v) => v.get_type_name(),
                None => skin_class.get_getter(&[PUBLIC_NAMESPACE], host_skin_part_name, true).and_then(|g| g.get_return_type_name()),
            };
            let required = skin_parts_meta_data.get_value_key("required");
            if required.as_deref() == Some("true") && skin_part_type_name.is_none() {
                let (level, text) = if self.report_missing_required_skin_parts_as_warnings {
                    (Level::Warning, format!("The required skin part '{host_skin_part_name}' is missing."))
                } else {
                    (Level::Error, format!("The required skin part '{host_skin_part_name}' is missing."))
                };
                localized2(cx, level, input_origin(cx), position, text);
            } else if let Some(t) = &skin_part_type_name {
                let host = host_skin_part_type_name.clone().expect("NullPointerException");
                let c = symbol_table.get_class(Some(&name_formatter::to_colon(t))).unwrap_or_else(|| {
                    panic!("Cannot invoke \"flex2.compiler.abc.AbcClass.isSubclassOf(String)\" because the return value of \"flex2.compiler.as3.reflect.TypeTable.getClass(String)\" is null")
                });
                if !c.is_subclass_of(&host) {
                    localized2(cx, Level::Error, input_origin(cx), position, format!("The skin part type '{t}' must be assignable to '{host}'."));
                }
            }
        }
    }

    fn validate_required_skin_parts_and_states(&self, cx: &ContextRef, unit: &UnitRef, symbol_table: &SymbolTable) {
        let meta_data = unit.borrow().host_component_meta_data.clone().unwrap();
        let (host_component_class_name, pos) = {
            let b = meta_data.borrow();
            (b.meta_data().unwrap().get_value_index(0).expect("NullPointerException"), b.pos())
        };
        match symbol_table.get_class(Some(&name_formatter::to_colon(&host_component_class_name))) {
            None => localized2(cx, Level::Error, input_origin(cx), pos, format!("[HostComponent] class '{host_component_class_name}' not found.")),
            Some(host_component_class) => {
                let owner = unit.borrow().host_component_owner_class.clone();
                if let Some(owner) = owner {
                    let skin_class = symbol_table.get_class(Some(&owner)).expect("NullPointerException");
                    self.validate_required_skin_parts(&host_component_class, &skin_class, cx, pos, symbol_table);
                    validate_required_skin_states(&host_component_class, &skin_class, cx, pos);
                }
            }
        }
    }
}

fn class_declares_identifier(cx: &ContextRef, class_def: &NodeRef, symbol_table: &SymbolTable, identifier: &str) -> bool {
    let class_name = node_magic::get_class_name(class_def);
    let type_analyzer = symbol_table.get_type_analyzer();
    type_analyzer.borrow_mut().evaluate_class_definition(cx, class_def, symbol_table);
    let class_info = type_analyzer.borrow().get_class_info(&class_name);
    match class_info {
        Some(c) => {
            let c = c.borrow();
            c.defines_variable(identifier) || c.defines_function_inherited(identifier, true) || c.defines_getter_inherited(identifier, true) || c.defines_setter_inherited(identifier, true)
        }
        None => false,
    }
}

fn validate_required_skin_states(host_component_class: &AbcClass, skin_class: &AbcClass, cx: &ContextRef, position: i32) {
    let skin_states = host_component_class.get_meta_data("SkinState", true);
    let states = skin_class.get_meta_data("States", true);
    for skin_states_meta_data in &skin_states {
        let skin_state_name = skin_states_meta_data.get_value_index(0).expect("NullPointerException");
        let is_found = states.iter().any(|s| (0..s.count()).any(|i| s.get_value_index(i).as_deref() == Some(skin_state_name.as_str())));
        if !is_found {
            localized2(cx, Level::Error, input_origin(cx), position, format!("The required skin state '{skin_state_name}' is missing."));
        }
    }
}

impl Extension for HostComponentExtension {
    fn parse1(&mut self, unit: &UnitRef, _symbol_table: &mut SymbolTable) {
        if unit.borrow().host_component_meta_data.is_some() {
            unit.borrow_mut().inheritance.insert(Name::M(MultiName::new("flash.events", "IEventDispatcher")));
        }
    }

    fn parse2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().host_component_meta_data.is_some() {
            let cx = unit.borrow().context.get_asc_context().expect("NullPointerException");
            self.generate_host_component_variable(&cx, unit, symbol_table);
        }
    }

    fn generate(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().host_component_meta_data.is_some() {
            let cx = unit.borrow().context.get_asc_context().expect("NullPointerException");
            self.validate_required_skin_parts_and_states(&cx, unit, symbol_table);
        }
    }
}
